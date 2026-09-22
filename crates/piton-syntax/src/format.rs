//! Canonical formatting.
//!
//! `piton format` normalizes structure, not prose. Indentation becomes four
//! spaces per level, comments get a space after `//`, declaration punctuation
//! gets its mandatory spacing, and import lists are sorted and wrapped. The
//! text of a string is left exactly as written, because in Piton that text is
//! the value.

use std::path::Path;

use piton_core::Diagnostic;

use crate::ast::{FromKind, Item};
use crate::parser;
use crate::prose;

/// Indent width. The language prefers four spaces and does not make it
/// configurable.
pub const INDENT: usize = 4;

/// Column at which an import declaration wraps onto separate lines.
pub const WRAP_COLUMN: usize = 80;

/// Formats a source file canonically, normalizing comments.
///
/// This is `piton format`'s behavior: it puts a space after every `//` and
/// reindents comment lines along with the code around them.
pub fn format(source: &str, path: &Path) -> String {
    format_impl(source, path, false)
}

/// Autoformats a source file the way an editor's format-on-save does.
///
/// Structure is normalized, but commented content is never rewritten, because
/// the specification says autoformat "should not format anything that is
/// commented." A whole-line comment is left byte-for-byte as written -- the
/// original indentation and the comment text both -- and a trailing comment
/// keeps its exact text, with no space inserted after `//`. The code on a line
/// that merely *carries* a comment is still formatted; only the commented part
/// is left alone.
pub fn autoformat(source: &str, path: &Path) -> String {
    format_impl(source, path, true)
}

fn format_impl(source: &str, path: &Path, preserve_comments: bool) -> String {
    let parse = parser::parse(source, path);
    let imports = import_replacements(&parse, source);

    let mut out = String::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut fence: Option<(usize, usize, usize)> = None; // (ticks, base indent, depth)
    let mut escape: Option<(usize, usize, usize)> = None; // (run, base indent, depth)
    let mut offset = 0usize;

    for raw in source.split_inclusive('\n') {
        let line_start = offset;
        offset += raw.len();
        let text = raw.trim_end_matches(['\n', '\r']);
        let indent = text.chars().take_while(|c| *c == ' ' || *c == '\t').count();
        let body = text[indent_bytes(text, indent)..].to_string();
        let trimmed = body.trim_end();

        // A replaced import declaration swallows its continuation lines.
        if let Some(replacement) = imports.get(&line_start) {
            out.push_str(&replacement.text);
            out.push('\n');
            continue;
        }
        if imports
            .values()
            .any(|r| line_start > r.start && line_start < r.end)
        {
            continue;
        }

        // A multi-line escape block is literal, so only its indentation moves.
        if let Some((run, base, depth)) = escape {
            let closing = trimmed.chars().count() == run && trimmed.chars().all(|c| c == '\\');
            let relative = indent.saturating_sub(base);
            push_line(&mut out, depth, relative, trimmed);
            if closing {
                escape = None;
            }
            continue;
        }

        if let Some((ticks, base, depth)) = fence {
            let closing = trimmed.chars().take_while(|c| *c == '`').count() >= ticks
                && trimmed.chars().all(|c| c == '`');
            let relative = indent.saturating_sub(base);
            push_line(&mut out, depth, relative, trimmed);
            if closing {
                fence = None;
            }
            continue;
        }

        if trimmed.is_empty() {
            out.push('\n');
            continue;
        }

        // Autoformat leaves a whole-line comment exactly as written: the
        // original indentation and the comment text both. Save-formatting must
        // not move or re-space someone's comments, so the raw line is emitted
        // untouched rather than reindented and normalized. The comment still
        // feeds the indentation stack above so the surrounding code lands where
        // `piton format` would put it; only the comment's own output differs.
        // This runs outside any fence or escape block, where a leading `//` is
        // literal text and is handled above.
        let depth = depth_for(&mut stack, indent);

        if preserve_comments && trimmed.starts_with("//") {
            out.push_str(text);
            out.push('\n');
            continue;
        }

        if !trimmed.is_empty() && trimmed.chars().all(|c| c == '\\') {
            escape = Some((trimmed.chars().count(), indent, depth));
            push_line(&mut out, depth, 0, trimmed);
            continue;
        }

        let opening = trimmed.chars().take_while(|c| *c == '`').count();
        if opening >= 3 {
            fence = Some((opening, indent, depth));
            push_line(&mut out, depth, 0, trimmed);
            continue;
        }

        push_line(&mut out, depth, 0, &normalize(trimmed, preserve_comments));
    }

    if !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    out
}

/// True when the file is already canonically formatted.
pub fn is_formatted(source: &str, path: &Path) -> bool {
    format(source, path) == source
}

/// Reports the difference between a file and its formatted form as a
/// diagnostic, for `piton format --check`.
pub fn check(source: &str, path: &Path) -> Option<Diagnostic> {
    if is_formatted(source, path) {
        return None;
    }
    Some(Diagnostic::warning(
        "unformatted",
        "file is not canonically formatted",
        path,
        piton_core::Span::default(),
    ))
}

fn indent_bytes(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}

/// Maps a source indent column onto a nesting depth, so inconsistent widths
/// still produce consistent output.
fn depth_for(stack: &mut Vec<usize>, indent: usize) -> usize {
    while let Some(top) = stack.last() {
        if indent < *top {
            stack.pop();
        } else {
            break;
        }
    }
    match stack.last() {
        Some(top) if indent == *top => stack.len() - 1,
        _ => {
            stack.push(indent);
            stack.len() - 1
        }
    }
}

fn push_line(out: &mut String, depth: usize, extra: usize, text: &str) {
    if text.is_empty() {
        out.push('\n');
        return;
    }
    out.push_str(&" ".repeat(depth * INDENT + extra));
    out.push_str(text);
    out.push('\n');
}

/// Applies the small spacing rules: a space after `//`, and after `:` and `::`
/// where they carry structural meaning.
///
/// When `preserve_comments` is set (autoformat), the comment text is left
/// exactly as written -- no space is inserted after `//` -- because autoformat
/// must not format commented content. The code before the comment is still
/// normalized, so only a single space separates the two.
fn normalize(text: &str, preserve_comments: bool) -> String {
    let (code, comment) = prose::split_comment(text);
    let mut out = code.trim_end().to_string();
    if let Some(comment) = comment {
        if preserve_comments {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(comment);
            return out;
        }
        let body = comment.trim_start_matches('/').trim_start();
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str("// ");
        out.push_str(body);
        // A comment with no text keeps just the marker.
        if body.is_empty() {
            out.truncate(out.trim_end().len());
        }
    }
    out
}

struct Replacement {
    start: usize,
    end: usize,
    text: String,
}

/// Rewrites every import and export declaration, sorted and wrapped.
fn import_replacements(
    parse: &parser::Parse,
    source: &str,
) -> std::collections::BTreeMap<usize, Replacement> {
    let mut out = std::collections::BTreeMap::new();
    for item in &parse.file.items {
        let Item::From(decl) = item else { continue };
        let keyword = match decl.kind {
            FromKind::Import => "import",
            FromKind::Export => "export",
        };
        let mut entries: Vec<String> = decl
            .items
            .iter()
            .map(|entry| match &entry.alias {
                Some(alias) => format!("{} {}", entry.name, alias.value),
                None => entry.name.clone(),
            })
            .collect();
        // Sorting is part of canonical form, so two files that import the same
        // names look the same.
        entries.sort();

        let head = format!("from {} {keyword}", decl.path.text);
        let text = if decl.star {
            format!("{head} *")
        } else if entries.is_empty() {
            head
        } else {
            let single = format!("{head} {}", entries.join(", "));
            if entries.len() > 2 || single.chars().count() > WRAP_COLUMN {
                let mut wrapped = head;
                wrapped.push('\n');
                for (index, entry) in entries.iter().enumerate() {
                    wrapped.push_str(&" ".repeat(INDENT));
                    wrapped.push_str(entry);
                    if index + 1 < entries.len() {
                        wrapped.push(',');
                    }
                    wrapped.push('\n');
                }
                wrapped.pop();
                wrapped
            } else {
                single
            }
        };

        // Extend the replaced region to the end of the declaration's last line.
        let end = source[decl.span.end.min(source.len())..]
            .find('\n')
            .map(|offset| decl.span.end + offset)
            .unwrap_or(source.len());
        out.insert(
            decl.span.start,
            Replacement {
                start: decl.span.start,
                end,
                text,
            },
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(source: &str) -> String {
        format(source, Path::new("test.pi"))
    }

    #[test]
    fn indentation_becomes_four_spaces() {
        let source = "anchor A:\n  value: 1\n      nested: 2\n";
        assert_eq!(fmt(source), "anchor A:\n    value: 1\n        nested: 2\n");
    }

    #[test]
    fn tabs_become_spaces() {
        let source = "anchor A:\n\tvalue: 1\n";
        assert_eq!(fmt(source), "anchor A:\n    value: 1\n");
    }

    #[test]
    fn comments_gain_a_space() {
        assert_eq!(fmt("//no space\n"), "// no space\n");
        assert_eq!(fmt("value: 1 //tight\n"), "value: 1 // tight\n");
    }

    fn auto(source: &str) -> String {
        autoformat(source, Path::new("test.pi"))
    }

    #[test]
    fn autoformat_leaves_comment_lines_exactly_as_written() {
        // The spec: "Autoformat should not format anything that is commented."
        // A whole-line comment keeps its original indentation and its text is
        // never re-spaced -- the opposite of `piton format`, which normalizes
        // both. A misindented comment stays misindented rather than being moved.
        assert_eq!(auto("//no space\n"), "//no space\n");
        assert_eq!(
            auto("        // deeply indented\n"),
            "        // deeply indented\n"
        );
        assert_eq!(auto("  //odd indent\n"), "  //odd indent\n");
    }

    #[test]
    fn autoformat_still_formats_the_code_around_comments() {
        // Only the commented content is exempt. Code is formatted as usual, so
        // indentation is fixed and a trailing comment's code side is normalized
        // -- but the comment text itself keeps its exact bytes (no space after
        // `//`).
        assert_eq!(
            auto("anchor A:\n  value: 1 //tight\n"),
            "anchor A:\n    value: 1 //tight\n"
        );
        // An ordinary indented comment line still gets its code neighbours
        // formatted while it stays put.
        assert_eq!(
            auto("anchor A:\n  value: 1\n//note\n  other: 2\n"),
            "anchor A:\n    value: 1\n//note\n    other: 2\n"
        );
    }

    #[test]
    fn autoformat_never_invents_a_space_after_the_marker() {
        // The whole point: `//x` stays `//x` under autoformat but becomes
        // `// x` under the explicit command.
        assert_eq!(auto("value: 1 //x\n"), "value: 1 //x\n");
        assert_eq!(fmt("value: 1 //x\n"), "value: 1 // x\n");
    }

    #[test]
    fn short_import_lists_stay_on_one_line() {
        assert_eq!(
            fmt("from ./file import B, A\n"),
            "from ./file import A, B\n"
        );
    }

    #[test]
    fn more_than_two_items_wrap() {
        assert_eq!(
            fmt("from ./file import C, A, B\n"),
            "from ./file import\n    A,\n    B,\n    C\n"
        );
    }

    #[test]
    fn long_import_lines_wrap_even_with_two_items() {
        let source = "from ../operators/concatenation/ConcatenationOperator import AVeryLongSymbolName, AnotherVeryLongSymbolName\n";
        let formatted = fmt(source);
        assert!(
            formatted.contains("import\n    AVeryLongSymbolName,\n"),
            "{formatted}"
        );
    }

    #[test]
    fn already_wrapped_imports_are_rejoined_when_short() {
        assert_eq!(
            fmt("from ./file import\n    B,\n    A\n"),
            "from ./file import A, B\n"
        );
    }

    #[test]
    fn star_exports_are_preserved() {
        assert_eq!(
            fmt("from ./MyAnchor export *\n"),
            "from ./MyAnchor export *\n"
        );
    }

    #[test]
    fn prose_and_blank_lines_are_left_alone() {
        let source = "anchor A:\n    text:\n        one line\n\n        another after a break\n";
        assert_eq!(fmt(source), source);
    }

    #[test]
    fn fenced_content_keeps_its_relative_indentation() {
        let source = "anchor A:\n  body:\n    ```piton\n    key: value\n      nested: 2\n    ```\n";
        assert_eq!(
            fmt(source),
            "anchor A:\n    body:\n        ```piton\n        key: value\n          nested: 2\n        ```\n"
        );
    }

    #[test]
    fn escape_blocks_keep_their_relative_indentation() {
        let source =
            "anchor A:\n  body:\n    \\\\\\\n    key: value\n      nested: 2\n    \\\\\\\n";
        assert_eq!(
            fmt(source),
            "anchor A:\n    body:\n        \\\\\\\n        key: value\n          nested: 2\n        \\\\\\\n"
        );
    }

    #[test]
    fn escape_block_contents_are_never_rewritten() {
        // Comment spacing is a formatting rule for code, not for literal text.
        let source =
            "anchor A:\n    body:\n        \\\\\\\n        //no space added here\n        \\\\\\\n";
        assert_eq!(fmt(source), source);
    }

    #[test]
    fn formatting_is_idempotent_on_the_spec_style() {
        let source = "use ./lib/Type\n\nexport type Strings:\n    description:\n        Strings are not quoted.\n\n    supportedOperators:\n        - {ConcatenationOperator}\n";
        assert_eq!(fmt(source), source);
        assert_eq!(fmt(&fmt(source)), fmt(source));
    }
}
