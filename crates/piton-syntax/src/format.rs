//! The canonical formatter.
//!
//! `piton format` normalizes the structure. It does not change prose. The
//! indentation becomes four spaces for each level. Each comment gets a space
//! after `//`. Declaration punctuation gets its mandatory spaces. The formatter
//! sorts and wraps import lists. It does not change the text of a string,
//! because in Piton that text is the value.

use std::path::Path;

use piton_core::Diagnostic;

use crate::ast::{FromKind, Item};
use crate::parser;
use crate::prose;

/// The indent width. The language prefers four spaces. The user cannot
/// change this width.
pub const INDENT: usize = 4;

/// The column where an import declaration wraps to separate lines.
pub const WRAP_COLUMN: usize = 80;

/// Formats a source file in the canonical form.
///
/// This is the behavior of `piton format`. It puts a space after each `//`
/// that has no space. It reindents comment lines together with the code around
/// them. It does not change the other text of a comment. Thus commented-out
/// code keeps its own spaces.
pub fn format(source: &str, path: &Path) -> String {
    format_impl(source, path, false)
}

/// Autoformats a source file. An editor does the same with format-on-save.
///
/// This function normalizes the structure. It adds a space after `//`, as
/// `piton format` does. It does not change other commented text. A whole-line
/// comment keeps its original indentation. Each comment keeps its text after
/// the marker byte-for-byte. The function formats the code on a line that only
/// *carries* a comment.
pub fn autoformat(source: &str, path: &Path) -> String {
    format_impl(source, path, true)
}

/// Formats a source that came alone, as `piton format -` reads it from stdin.
/// It returns the formatted text, or the parse errors that stopped it.
///
/// If a file does not parse, a format of the file is only a guess. An editor
/// can send a buffer through the formatter. The editor then replaces the buffer
/// with the result. Thus a broken file gets no result. It does not get a rewrite.
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

/// Puts one space after the `//` of a comment when the next character is not
/// whitespace. It does not change the text after that.
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

        // A replaced import declaration also replaces its continuation lines.
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

        // A multi-line escape block is literal. Thus only its indentation moves.
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

        // Autoformat does not move a whole-line comment. The original
        // indentation stays. The only change is the space after `//`.
        // A format on save must not move the comments of a user. The comment
        // still goes into the indentation stack above. Thus the code around it
        // goes where `piton format` puts it. Only the output of the comment is
        // different. This code runs outside escape blocks. In an escape block,
        // a leading `//` is literal text, and the code above handles it.
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

/// Sorts each run of import lines. The `use` lines come first, then the `from`
/// lines. It sorts each group by path. A run is a group of top-level `use` and
/// `from` statements with no other lines between them. Thus a blank line
/// divides two groups. This function does not change a run with a comment.
/// A move of the lines would separate the comment from the lines it describes.
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
        // Collect the run. Each statement is its line and its wrapped lines.
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

/// Puts `use` before `from`, then sorts by path.
fn import_key(statement: &str) -> (u8, String) {
    let mut words = statement.split_whitespace();
    let group = u8::from(words.next() != Some("use"));
    (group, words.next().unwrap_or_default().to_string())
}

/// True when the file is already in the canonical format.
pub fn is_formatted(source: &str, path: &Path) -> bool {
    format(source, path) == source
}

/// Gives the difference between a file and its formatted form as a
/// diagnostic. `piton format --check` uses this function.
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

/// Maps a source indent column to a nesting depth. Thus different indent
/// widths give the same output.
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

/// Applies the small space rules. It puts a single space between code and a
/// trailing comment. It puts a space after `//` when the comment has no space.
/// It never changes the text of the comment.
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

/// A module path in canonical form. It does not have the optional `.pi` extension.
fn module_path(text: &str) -> &str {
    match text.strip_suffix(".pi") {
        // `./.pi` is a file with no name. Do not change unusual paths of this type.
        Some(stem) if !stem.is_empty() && !stem.ends_with('/') => stem,
        _ => text,
    }
}

/// The offset of the end of the line that contains `offset`.
fn line_end(source: &str, offset: usize) -> usize {
    let offset = offset.min(source.len());
    source[offset..]
        .find('\n')
        .map(|found| offset + found)
        .unwrap_or(source.len())
}

/// Rewrites each import declaration and each export declaration, sorted and
/// wrapped. It also rewrites each `use` with a path that has the `.pi` extension.
fn import_replacements(
    parse: &parser::Parse,
    source: &str,
) -> std::collections::BTreeMap<usize, Replacement> {
    let mut out = std::collections::BTreeMap::new();
    for item in &parse.file.items {
        // A `use` changes only when its path has the optional extension.
        if let Item::Use(decl) = item {
            let path = module_path(&decl.path.text);
            if path != decl.path.text {
                let end = line_end(source, decl.path.span.end);
                // The text after the path on its line stays. An example is a comment.
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
        // The sort is part of the canonical form. Thus two files that import
        // the same names look the same.
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

        // Extend the replaced region to the end of the last line of the declaration.
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
        // After code, `//` is text, not a comment. Thus the formatter does not change it.
        assert_eq!(fmt("value: 1 //tight\n"), "value: 1 //tight\n");
        assert_eq!(fmt("//\n"), "//\n");
    }

    #[test]
    fn the_rest_of_a_comment_is_never_rewritten() {
        // Commented-out code keeps its own spaces. The formatter adds only a
        // space that is not there, immediately after the marker.
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
        // The spec says: "Autoformat should add the space after //, but not touch
        // anything else that's commented." A whole-line comment keeps its
        // original indentation. This is also true for an incorrect indentation.
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
        // The formatter formats the code lines near a usual comment line.
        // The comment line does not move.
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
        // A blank line divides two groups.
        assert_eq!(
            fmt("from ./b import B\n\nfrom ./a import A\n"),
            "from ./b import B\n\nfrom ./a import A\n"
        );
        // Wrapped imports move together with their lines.
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
        // The formatter does not change paths that are already canonical.
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
        // To Piton, code blocks are only text. Thus their lines are structure,
        // as all other lines are. An escape block in the fence keeps them literal.
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
        // The comment space rule is for code. It is not for literal text.
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
