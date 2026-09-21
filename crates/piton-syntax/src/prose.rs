//! Prose scanning: escapes and interpolation.
//!
//! Prose is a first-class value in Piton, so a line of text is scanned rather
//! than tokenized. Two things interrupt ordinary text: backslash escapes and the
//! four interpolation sigils.
//!
//! # Escapes
//!
//! A maximal run of *n* backslashes emits *n - 1* literal backslashes. A run of
//! exactly one emits nothing and instead toggles literal mode, consuming one
//! space on the inner side of the delimiter. Inside literal mode, interpolation
//! sigils are ordinary characters. So `\ {1 + 2 + 3} \` renders as
//! `{1 + 2 + 3}`, and `\\ \ {1 + 2 + 3} \ \\` renders as `\ {1 + 2 + 3} \`.
//!
//! Literal mode does not survive a line break.

use std::path::Path;

use piton_core::{Diagnostic, Span};

use crate::ast::{Interpolation, ProseLine, ProseSegment, Sigil};
use crate::expr;

/// Scans one line of prose.
///
/// `base` is the byte offset of `text` within the file.
pub fn scan_line(text: &str, base: usize, file: &Path) -> (ProseLine, Vec<Diagnostic>) {
    let mut segments: Vec<ProseSegment> = Vec::new();
    let mut diagnostics = Vec::new();
    let mut buffer = String::new();
    let mut literal = false;

    let bytes: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0usize;

    let flush = |buffer: &mut String, segments: &mut Vec<ProseSegment>, literal: bool| {
        if !buffer.is_empty() {
            let taken = std::mem::take(buffer);
            segments.push(if literal {
                ProseSegment::Literal(taken)
            } else {
                ProseSegment::Text(taken)
            });
        }
    };

    while i < bytes.len() {
        let (_, ch) = bytes[i];

        if ch == '\\' {
            let mut run = 0usize;
            while i + run < bytes.len() && bytes[i + run].1 == '\\' {
                run += 1;
            }
            i += run;
            if run >= 2 {
                for _ in 0..run - 1 {
                    buffer.push('\\');
                }
            } else if literal {
                // Closing delimiter: drop one space written just inside it.
                if buffer.ends_with(' ') {
                    buffer.pop();
                }
                flush(&mut buffer, &mut segments, true);
                literal = false;
            } else {
                flush(&mut buffer, &mut segments, false);
                literal = true;
                // Opening delimiter: drop one space written just inside it.
                if i < bytes.len() && bytes[i].1 == ' ' {
                    i += 1;
                }
            }
            continue;
        }

        if !literal {
            if let Some((sigil, prefix_len)) = sigil_at(&bytes, i) {
                let open = i + prefix_len; // index of '{'
                match matching_brace(&bytes, open) {
                    Some(close) => {
                        let inner_start = bytes[open].0 + 1;
                        let inner_end = bytes[close].0;
                        let inner = &text[inner_start..inner_end];
                        let span = Span::new(base + bytes[i].0, base + bytes[close].0 + 1);
                        let (parsed, expr_diagnostics) =
                            expr::parse(inner, base + inner_start, file);

                        if expr_diagnostics.iter().any(Diagnostic::is_error) {
                            // Braces that do not contain an expression are
                            // ordinary text. This keeps prose able to talk about
                            // the syntax -- `{...}`, `${}` -- without escaping,
                            // and it does not reintroduce string fallback for
                            // unrecognized symbols: a bare name still parses as
                            // an expression and still fails during resolution.
                            diagnostics.push(Diagnostic::warning(
                                "braces-as-text",
                                format!(
                                    "`{}` does not contain a valid expression and is treated as text",
                                    &text[bytes[i].0..bytes[close].0 + 1]
                                ),
                                file,
                                span,
                            ));
                            buffer.push_str(&text[bytes[i].0..bytes[close].0 + 1]);
                            i = close + 1;
                            continue;
                        }

                        diagnostics.extend(expr_diagnostics);
                        flush(&mut buffer, &mut segments, false);
                        segments.push(ProseSegment::Interpolation(Interpolation {
                            span,
                            sigil,
                            expr: parsed,
                        }));
                        i = close + 1;
                        continue;
                    }
                    None => {
                        diagnostics.push(Diagnostic::error(
                            "unterminated-interpolation",
                            format!("`{}` is never closed", sigil.prefix()),
                            file,
                            Span::new(base + bytes[i].0, base + text.len()),
                        ));
                    }
                }
            }
        }

        buffer.push(ch);
        i += 1;
    }

    flush(&mut buffer, &mut segments, literal);

    (
        ProseLine {
            span: Span::new(base, base + text.len()),
            segments,
        },
        diagnostics,
    )
}

/// Recognizes an interpolation sigil at `index`, returning the sigil and the
/// number of characters before its opening brace.
fn sigil_at(bytes: &[(usize, char)], index: usize) -> Option<(Sigil, usize)> {
    let ch = bytes.get(index)?.1;
    let next = bytes.get(index + 1).map(|(_, c)| *c);
    match (ch, next) {
        ('$', Some('{')) => Some((Sigil::Stringify, 1)),
        ('#', Some('{')) => Some((Sigil::Numeric, 1)),
        ('@', Some('{')) => Some((Sigil::Reference, 1)),
        ('{', _) => Some((Sigil::Standard, 0)),
        _ => None,
    }
}

/// Finds the brace matching the one at `open`, ignoring braces inside quoted
/// strings.
fn matching_brace(bytes: &[(usize, char)], open: usize) -> Option<usize> {
    if bytes.get(open)?.1 != '{' {
        return None;
    }
    let mut depth = 0usize;
    let mut quoted = false;
    let mut i = open;
    while i < bytes.len() {
        let ch = bytes[i].1;
        if quoted {
            if ch == '\\' {
                i += 2;
                continue;
            }
            if ch == '"' {
                quoted = false;
            }
            i += 1;
            continue;
        }
        match ch {
            '"' => quoted = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Splits a source line into code and an optional trailing comment.
///
/// `//` opens a comment only when it follows whitespace or starts the line, is
/// not escaped by a backslash, and is not inside a double-quoted run. That last
/// rule is what keeps prose such as `Comments are created with "//".` intact.
pub fn split_comment(line: &str) -> (&str, Option<&str>) {
    let bytes: Vec<(usize, char)> = line.char_indices().collect();
    let mut quoted = false;
    let mut i = 0usize;
    while i < bytes.len() {
        let (offset, ch) = bytes[i];
        match ch {
            '"' => {
                quoted = !quoted;
                i += 1;
            }
            '\\' => {
                // Skip the whole run plus whatever it escapes.
                let mut run = 0;
                while i + run < bytes.len() && bytes[i + run].1 == '\\' {
                    run += 1;
                }
                i += run + usize::from(run % 2 == 1);
            }
            '/' if !quoted && bytes.get(i + 1).map(|(_, c)| *c) == Some('/') => {
                let preceded_ok = i == 0 || bytes[i - 1].1.is_whitespace();
                if preceded_ok {
                    return (&line[..offset], Some(&line[offset..]));
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    (line, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::ExprKind;

    fn segments(text: &str) -> Vec<ProseSegment> {
        let (line, diagnostics) = scan_line(text, 0, Path::new("t.pi"));
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        line.segments
    }

    fn rendered(text: &str) -> String {
        segments(text)
            .into_iter()
            .map(|segment| match segment {
                ProseSegment::Text(t) | ProseSegment::Literal(t) => t,
                ProseSegment::Interpolation(interp) => {
                    format!("<{:?}>", interp.sigil)
                }
            })
            .collect()
    }

    #[test]
    fn single_backslashes_delimit_a_literal_region() {
        assert_eq!(rendered(r"\ {1 + 2 + 3} \"), "{1 + 2 + 3}");
        assert_eq!(rendered(r"\ { 1 + 2 + 3 } \."), "{ 1 + 2 + 3 }.");
    }

    #[test]
    fn stacked_backslashes_unwrap_one_layer() {
        assert_eq!(
            rendered(r"So for example \\ \ {1 + 2 + 3} \ \\ would become \ { 1 + 2 + 3 } \."),
            r"So for example \ {1 + 2 + 3} \ would become { 1 + 2 + 3 }."
        );
        assert_eq!(
            rendered(r"\\\\ \\\ \\ \ {1 + 2 + 3} \ \\ \\\ \\\\"),
            r"\\\ \\ \ {1 + 2 + 3} \ \\ \\\"
        );
    }

    #[test]
    fn escaped_colon_survives() {
        assert_eq!(rendered(r"Similarly\:"), "Similarly:");
    }

    #[test]
    fn sigils_are_recognized() {
        let segs = segments("This is ${self.name}");
        assert_eq!(segs.len(), 2);
        match &segs[1] {
            ProseSegment::Interpolation(interp) => {
                assert_eq!(interp.sigil, Sigil::Stringify);
                assert!(matches!(interp.expr.kind, ExprKind::Field(..)));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn reference_sigil_parses() {
        let segs = segments("rules of @{TypeCoercion} and @{Inference}.");
        let sigils: Vec<_> = segs
            .iter()
            .filter_map(|s| match s {
                ProseSegment::Interpolation(i) => Some(i.sigil),
                _ => None,
            })
            .collect();
        assert_eq!(sigils, vec![Sigil::Reference, Sigil::Reference]);
    }

    #[test]
    fn comments_split_off_the_end() {
        assert_eq!(
            split_comment("myVariable: 42 // This is also a comment"),
            ("myVariable: 42 ", Some("// This is also a comment"))
        );
        assert_eq!(split_comment("// whole line").0, "");
    }

    #[test]
    fn quoted_and_escaped_slashes_are_not_comments() {
        assert_eq!(split_comment(r#"created with "//"."#).1, None);
        assert_eq!(split_comment(r"\// Just Text").1, None);
        assert_eq!(split_comment("https://example.com/a").1, None);
    }
}
