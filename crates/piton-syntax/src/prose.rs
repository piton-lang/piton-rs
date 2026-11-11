//! The prose scanner. It finds escapes and interpolations.
//!
//! Prose is a first-class value in Piton. Thus the scanner scans a line of text.
//! It does not tokenize the line. Two items stop usual text: backslash escapes
//! and the four interpolation sigils.
//!
//! # Escapes
//!
//! A maximal run of *n* backslashes gives *n - 1* literal backslashes. A run of
//! exactly one backslash gives no text. It toggles literal mode. It also removes
//! one space on the inner side of the delimiter. In literal mode, interpolation
//! sigils are usual characters. Thus `\ {1 + 2 + 3} \` becomes
//! `{1 + 2 + 3}`, and `\\ \ {1 + 2 + 3} \ \\` becomes `\ {1 + 2 + 3} \`.
//!
//! Literal mode stops at a line break.

use std::path::Path;

use piton_core::{Diagnostic, Span};

use crate::ast::{Interpolation, ProseLine, ProseSegment, Sigil};
use crate::expr;

/// Scans one line of prose.
///
/// `base` is the byte offset of `text` in the file.
pub fn scan_line(text: &str, base: usize, file: &Path) -> (ProseLine, Vec<Diagnostic>) {
    let mut segments: Vec<ProseSegment> = Vec::new();
    let mut diagnostics = Vec::new();
    let mut buffer = String::new();
    let mut literal = false;
    let mut escapes = Vec::new();

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
            let delimiter = Span::new(base + bytes[i].0, base + bytes[i].0 + 1);
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
                // This is the end delimiter. The wrapper is a backslash with a
                // space on its inner side. The code removes the space with the wrapper.
                if buffer.ends_with(' ') {
                    buffer.pop();
                    flush(&mut buffer, &mut segments, true);
                    literal = false;
                    escapes.push(delimiter);
                } else {
                    buffer.push('\\');
                }
            } else if i < bytes.len() && bytes[i].1 == ' ' {
                // This is the start delimiter `\ `. The space is part of the wrapper.
                flush(&mut buffer, &mut segments, false);
                literal = true;
                escapes.push(delimiter);
                i += 1;
            } else {
                // A wrapper is the only escape. Thus a backslash that does not
                // start a wrapper is a usual character.
                buffer.push('\\');
            }
            continue;
        }

        if !literal {
            if let Some((sigil, prefix_len)) = sigil_at(&bytes, i) {
                let open = i + prefix_len; // The index of '{'.
                match matching_brace(&bytes, open) {
                    Some(close) => {
                        let inner_start = bytes[open].0 + 1;
                        let inner_end = bytes[close].0;
                        let inner = &text[inner_start..inner_end];
                        let span = Span::new(base + bytes[i].0, base + bytes[close].0 + 1);
                        let (parsed, expr_diagnostics) =
                            expr::parse(inner, base + inner_start, file);

                        if expr_diagnostics.iter().any(Diagnostic::is_error) {
                            // Braces without a valid expression are an error.
                            // Prose about the syntax escapes it with `\ ... \`.
                            // The code keeps the text. Thus the rest of the line
                            // stays readable. But the file will not compile.
                            diagnostics.extend(expr_diagnostics);
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
            escapes,
        },
        diagnostics,
    )
}

/// Finds an interpolation sigil at `index`. It returns the sigil and the
/// number of characters before the start brace of the sigil.
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

/// Finds the brace that matches the brace at `open`. It ignores braces in
/// quoted strings.
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

/// Splits a source line into code and a comment.
///
/// A comment must be on a line of its own. A line is a comment when its first
/// non-whitespace characters are `//`. In all other locations, `//` is usual
/// text. Thus `url: https://example.com // note` keeps all of the text as the value.
pub fn split_comment(line: &str) -> (&str, Option<&str>) {
    let offset = line.len() - line.trim_start().len();
    if line[offset..].starts_with("//") {
        (&line[..offset], Some(&line[offset..]))
    } else {
        (line, None)
    }
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
    fn a_colon_is_escaped_by_wrapping_it() {
        assert_eq!(rendered(r"Similarly\ : \"), "Similarly:");
        // A wrapper is the only escape. A bare backslash is a character.
        assert_eq!(rendered(r"Similarly\:"), r"Similarly\:");
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
    fn escape_delimiters_are_recorded() {
        let (line, _) = scan_line(r"a \ {x} \ b", 10, Path::new("t.pi"));
        assert_eq!(line.escapes, vec![Span::new(12, 13), Span::new(18, 19)]);
        // A run of two is an escaped backslash, not a delimiter.
        let (line, _) = scan_line(r"a \\ b", 0, Path::new("t.pi"));
        assert!(line.escapes.is_empty());
    }

    #[test]
    fn a_comment_is_a_whole_line() {
        assert_eq!(split_comment("// whole line"), ("", Some("// whole line")));
        assert_eq!(split_comment("    // indented"), ("    ", Some("// indented")));
    }

    #[test]
    fn slashes_after_code_are_text() {
        assert_eq!(split_comment("myVariable: 42 // not a comment").1, None);
        assert_eq!(split_comment("url: https://example.com // note").1, None);
        assert_eq!(split_comment(r#"created with "//"."#).1, None);
    }
}
