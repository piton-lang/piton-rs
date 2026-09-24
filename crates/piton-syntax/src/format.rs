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

/// Formats a source file canonically.
///
/// This is `piton format`'s behavior: it puts a space after every `//` that
/// lacks one and reindents comment lines along with the code around them. The
/// rest of a comment is left exactly as written, so commented-out code keeps
/// its own spacing.
pub fn format(source: &str, path: &Path) -> String {
    format_impl(source, path, false)
}

/// Autoformats a source file the way an editor's format-on-save does.
///
/// Structure is normalized, and a space is added after `//` the same way
/// `piton format` adds one, but nothing else that is commented is touched: a
/// whole-line comment keeps its original indentation, and every comment keeps
/// its text byte-for-byte after the marker. The code on a line that merely
/// *carries* a comment is still formatted.
pub fn autoformat(source: &str, path: &Path) -> String {
    format_impl(source, path, true)
}

/// Formats a source that arrived on its own, as `piton format -` reads it from
/// stdin: the formatted text, or the parse errors that stopped it.
///
/// Formatting a file that does not parse would be formatting a guess, and an
/// editor that pipes a buffer through the formatter replaces the buffer with
/// whatever comes back -- so a broken file gets nothing back, not a rewrite.
pub fn format_checked(source: &str, path: &Path) -> Result<String, Vec<Diagnostic>> {
    let parse = parser::parse(source, path);
    let errors: Vec<Diagnostic> = parse
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.is_error())
        .cloned()
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(format(source, path))
}

/// Puts one space after a comment's `//` when the next character is not
/// already whitespace. Everything after that is left exactly as written.
fn space_comment(comment: &str) -> String {
    let Some(rest) = comment.strip_prefix("//") else {
        return comment.to_string();
    };
    match rest.chars().next() {
        Some(next) if !next.is_whitespace() => format!("// {rest}"),
        _ => comment.to_string(),
    }
}

fn format_impl(source: &str, path: &Path, preserve_comments: bool) -> String {
    let parse = parser::parse(source, path);
    let imports = import_replacements(&parse, source);

    let mut out = String::new();
    let mut stack: Vec<usize> = Vec::new();
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

        if trimmed.is_empty() {
            out.push('\n');
            continue;
        }

        // Autoformat leaves a whole-line comment where it was: the original
        // indentation stays, and the only change is the space after `//`.
        // Save-formatting must not move someone's comments. The comment still
        // feeds the indentation stack above so the surrounding code lands where
        // `piton format` would put it; only the comment's own output differs.
        // This runs outside any escape block, where a leading `//` is literal
        // text and is handled above.
        let depth = depth_for(&mut stack, indent);

        if preserve_comments && trimmed.starts_with("//") {
            out.push_str(&text[..indent_bytes(text, indent)]);
            out.push_str(&space_comment(trimmed));
            out.push('\n');
            continue;
        }

        if !trimmed.is_empty() && trimmed.chars().all(|c| c == '\\') {
            escape = Some((trimmed.chars().count(), indent, depth));
            push_line(&mut out, depth, 0, trimmed);
            continue;
        }

        push_line(&mut out, depth, 0, &normalize(trimmed));
    }

    if !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    sort_import_lines(&out)
}

/// Sorts each run of import lines: the `use` lines first, then the `from`
/// lines, each sorted by path. A run is a group of top-level `use` and `from`
/// statements with nothing between them, so a blank line keeps two groups
/// apart. A run with a comment in it is left alone, because moving the lines
/// would separate the comment from what it describes.
fn sort_import_lines(text: &str) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let is_import = |line: &str| line.starts_with("use ") || line.starts_with("from ");
    let is_continuation = |line: &str| {
        line.starts_with([' ', '\t']) && !line.trim().is_empty() && !line.trim_start().starts_with("//")
    };

    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < lines.len() {
        if !is_import(lines[i]) {
            out.push_str(lines[i]);
            i += 1;
            continue;
        }
        // Collect the run: each statement is its line plus any wrapped lines.
        let mut statements: Vec<String> = Vec::new();
        let mut commented = false;
        while i < lines.len() && is_import(lines[i]) {
            let mut statement = lines[i].to_string();
            i += 1;
            while i < lines.len() && is_continuation(lines[i]) {
                statement.push_str(lines[i]);
                i += 1;
            }
            statements.push(statement);
            if i < lines.len() && lines[i].trim_start().starts_with("//") && i + 1 < lines.len() && is_import(lines[i + 1]) {
                commented = true;
                statement = lines[i].to_string();
                i += 1;
                statements.push(statement);
            }
        }
        if !commented {
            statements.sort_by(|a, b| import_key(a).cmp(&import_key(b)));
        }
        for statement in statements {
            out.push_str(&statement);
        }
    }
    out
}

/// `use` before `from`, then by path.
fn import_key(statement: &str) -> (u8, String) {
    let mut words = statement.split_whitespace();
    let group = u8::from(words.next() != Some("use"));
    (group, words.next().unwrap_or_default().to_string())
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

/// Applies the small spacing rules: a single space between code and a trailing
/// comment, and a space after `//` when the comment has none. The comment's
/// own text is never rewritten.
fn normalize(text: &str) -> String {
    let (code, comment) = prose::split_comment(text);
    let mut out = code.trim_end().to_string();
    if let Some(comment) = comment {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&space_comment(comment.trim_end()));
    }
    out
}

struct Replacement {
    start: usize,
    end: usize,
    text: String,
}

/// A module path in canonical form: the optional `.pi` extension removed.
fn module_path(text: &str) -> &str {
    match text.strip_suffix(".pi") {
        // `./.pi` would be a file with no name; leave anything that odd alone.
        Some(stem) if !stem.is_empty() && !stem.ends_with('/') => stem,
        _ => text,
    }
}

/// The offset of the end of the line holding `offset`.
fn line_end(source: &str, offset: usize) -> usize {
    let offset = offset.min(source.len());
    source[offset..]
        .find('\n')
        .map(|found| offset + found)
        .unwrap_or(source.len())
}

/// Rewrites every import and export declaration, sorted and wrapped, and every
/// `use` whose path carries the `.pi` extension.
fn import_replacements(
    parse: &parser::Parse,
    source: &str,
) -> std::collections::BTreeMap<usize, Replacement> {
    let mut out = std::collections::BTreeMap::new();
    for item in &parse.file.items {
        // A `use` only changes when its path carries the optional extension.
        if let Item::Use(decl) = item {
            let path = module_path(&decl.path.text);
            if path != decl.path.text {
                let end = line_end(source, decl.path.span.end);
                // Whatever follows the path on its line, a comment say, stays.
                let rest = source
                    .get(decl.path.span.end.min(end)..end)
                    .unwrap_or_default()
                    .trim_end();
                let text = match prose::split_comment(rest) {
                    (_, Some(comment)) => format!("use {path} {}", space_comment(comment)),
                    _ => format!("use {path}{rest}"),
                };
                out.insert(
                    decl.span.start,
                    Replacement {
                        start: decl.span.start,
                        end,
                        text,
                    },
                );
            }
            continue;
        }
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

        let head = format!("from {} {keyword}", module_path(&decl.path.text));
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
        let end = line_end(source, decl.span.end);
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
        // After code, `//` is text rather than a comment, so it is left alone.
        assert_eq!(fmt("value: 1 //tight\n"), "value: 1 //tight\n");
        assert_eq!(fmt("//\n"), "//\n");
    }

    #[test]
    fn the_rest_of_a_comment_is_never_rewritten() {
        // Commented-out code keeps its own spacing, and only a missing space
        // right after the marker is added.
        assert_eq!(fmt("//     nested: 2\n"), "//     nested: 2\n");
        assert_eq!(fmt("///triple\n"), "// /triple\n");
        assert_eq!(fmt("// already spaced\n"), "// already spaced\n");
        assert_eq!(fmt("//  two spaces\n"), "//  two spaces\n");
    }

    fn auto(source: &str) -> String {
        autoformat(source, Path::new("test.pi"))
    }

    #[test]
    fn autoformat_adds_the_space_and_leaves_comment_lines_where_they_are() {
        // The spec: "Autoformat should add the space after //, but not touch
        // anything else that's commented." A whole-line comment keeps its
        // original indentation, even a misindented one.
        assert_eq!(auto("//no space\n"), "// no space\n");
        assert_eq!(
            auto("        // deeply indented\n"),
            "        // deeply indented\n"
        );
        assert_eq!(auto("  //odd indent\n"), "  // odd indent\n");
        assert_eq!(auto("//     nested: 2\n"), "//     nested: 2\n");
    }

    #[test]
    fn autoformat_still_formats_the_code_around_comments() {
        assert_eq!(
            auto("anchor A:\n  value: 1\n  //tight\n"),
            "anchor A:\n    value: 1\n  // tight\n"
        );
        // An ordinary comment line still gets its code neighbours formatted
        // while it stays put.
        assert_eq!(
            auto("anchor A:\n  value: 1\n//note\n  other: 2\n"),
            "anchor A:\n    value: 1\n// note\n    other: 2\n"
        );
    }

    #[test]
    fn import_lines_are_sorted_use_first_then_by_path() {
        assert_eq!(
            fmt("from ./b import Zed, Alpha\nuse ./kw\nfrom ./a import Beta\n"),
            "use ./kw\nfrom ./a import Beta\nfrom ./b import Alpha, Zed\n"
        );
        // A blank line keeps two groups apart.
        assert_eq!(
            fmt("from ./b import B\n\nfrom ./a import A\n"),
            "from ./b import B\n\nfrom ./a import A\n"
        );
        // Wrapped imports move with their lines.
        assert_eq!(
            fmt("from ./z import\n    A,\n    B,\n    C\nfrom ./a import D\n"),
            "from ./a import D\nfrom ./z import\n    A,\n    B,\n    C\n"
        );
    }

    #[test]
    fn a_pi_extension_is_removed_from_module_paths() {
        assert_eq!(
            fmt("from ./file.pi import B, A\n"),
            "from ./file import A, B\n"
        );
        assert_eq!(fmt("from ../x.pi export *\n"), "from ../x export *\n");
        assert_eq!(fmt("use ./Keywords.pi\n"), "use ./Keywords\n");
        // Already canonical paths are left as they are.
        assert_eq!(fmt("use ./Keywords\n"), "use ./Keywords\n");
        assert_eq!(fmt("use my-package\n"), "use my-package\n");
    }

    #[test]
    fn a_broken_source_is_not_formatted() {
        assert!(format_checked("anchor A:\n    value: 1\n", Path::new("t.pi")).is_ok());
        assert!(format_checked("from import\n", Path::new("t.pi")).is_err());
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
    fn a_code_fence_is_formatted_like_any_other_text() {
        // Code blocks are just text to Piton, so their lines are structure like
        // anything else. An escape block inside the fence keeps them literal.
        let source = "anchor A:\n  body:\n    ```piton\n    key: value\n      nested: 2\n    ```\n";
        assert_eq!(
            fmt(source),
            "anchor A:\n    body:\n        ```piton\n        key: value\n            nested: 2\n        ```\n"
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
