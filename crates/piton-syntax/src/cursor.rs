//! The text that an author can write at a position in a file.
//!
//! An editor asks this question on each keystroke. Usually the text is not a
//! finished program. It can be a header without its colon, a key without its
//! value, or a brace that is still open. This module gives the answer. It uses
//! the line scanner and the header recognisers of the parser. Thus, completion
//! and the compiler always read a line in the same way.

use crate::ast::Sigil;
use crate::parser::{
    list_marker, merge_marker, parse_property_header, scan_lines, EscapeRole, Line,
};
use std::path::Path;

/// The item that the author writes at the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Site {
    /// No completion is possible. Examples are a comment, prose, a verbatim
    /// block and a quoted string. Other examples are a new name in a
    /// declaration, a new keyword, and an empty value.
    Inert,
    /// The module path after `use` or `from`.
    ModulePath,
    /// After `from <path>`: `import` or `export`.
    ImportDirection,
    /// A name in the list after `from <path> import`. It also has the names
    /// that the list contains at this time.
    ImportName { path: String, listed: Vec<String> },
    /// The first words of a top-level line.
    DeclarationHead { exported: bool, is_abstract: bool },
    /// A base, after `extends` or after a comma that follows a base.
    Base,
    /// A type, after `::`.
    Constraint,
    /// Inside braces. `member_of` is the dotted path before a `.` at the end.
    Expression { sigil: Sigil, member_of: Option<String> },
    /// The key at the start of a line in the body of an anchor. It also has the
    /// keys that the body has at this time, other than the key of this line. It also
    /// tells if the body has other content.
    Key { block_keys: Vec<String>, block_has_items: bool },
    /// A single word of value after `key: `, `- ` or `+ `. `key` is the
    /// anchor property, if there is one. Its constraints tell which values fit.
    Value { key: Option<String> },
}

/// The text that an author can write at `offset` in `source`.
pub fn site_at(source: &str, offset: usize) -> Site {
    let offset = offset.min(source.len());
    let lines = scan_lines(source, Path::new("")).lines;
    let Some(current) = lines
        .iter()
        .position(|line| line.start <= offset && offset <= line.end)
    else {
        return Site::Inert;
    };
    let line = &lines[current];

    // Piton cannot complete text in a verbatim block. Piton also cannot
    // complete a comment after the author writes its `//`.
    if line.escape != EscapeRole::None {
        return Site::Inert;
    }
    let content = &source[line.content_start.min(offset)..offset];
    if content.starts_with("//") {
        return Site::Inert;
    }

    // An open brace has priority over the text around it. Thus, `key: {x` is
    // an expression, not a value.
    if let Some((sigil, inside)) = open_sigil(content) {
        let inside = &content[inside..];
        if in_quotes(inside) {
            return Site::Inert;
        }
        return Site::Expression {
            sigil,
            member_of: member_path(inside).map(str::to_string),
        };
    }

    if line.indent == 0 && !source[line.start..offset].starts_with([' ', '\t']) {
        return top_level(content);
    }
    indented(&lines, current, content)
}

/// The rules for a line at the margin. These lines are imports, declarations,
/// and the variables that are keys at the top level.
fn top_level(content: &str) -> Site {
    if let Some(colon) = key_colon(content) {
        return match value_after_constraints(&content[colon..]) {
            Some(value) => single_word(value, None),
            None => Site::Constraint,
        };
    }
    let ends_in_space = content.ends_with(char::is_whitespace);
    let words: Vec<&str> = content.split_whitespace().collect();
    // The finished words. These are all the words, or all the words but the
    // word that the author still types.
    let finished = if ends_in_space {
        &words[..]
    } else {
        &words[..words.len().saturating_sub(1)]
    };

    match finished.first().copied() {
        Some("use") => return Site::ModulePath,
        Some("from") => {
            return match finished.len() {
                1 => Site::ModulePath,
                2 => Site::ImportDirection,
                _ if matches!(finished[2], "import" | "export") => {
                    let listed = content
                        .split_once(finished[2])
                        .map(|(_, list)| list)
                        .unwrap_or_default();
                    let mut names: Vec<String> = listed
                        .split(',')
                        .filter_map(|entry| entry.split_whitespace().next())
                        .map(str::to_string)
                        .collect();
                    if !ends_in_space && !listed.trim_end().ends_with(',') {
                        // The author still writes the last name.
                        names.pop();
                    }
                    Site::ImportName {
                        path: finished[1].to_string(),
                        listed: names,
                    }
                }
                _ => Site::Inert,
            };
        }
        _ => {}
    }

    // `anchor Name extends B as k:`. Bases come after `extends`. After `as`
    // comes a new keyword from the author. The cursor is after the word that
    // the author wrote last.
    let extends = finished.iter().rposition(|word| *word == "extends");
    let alias = finished.iter().rposition(|word| *word == "as");
    match (extends, alias) {
        (Some(extends), Some(alias)) if alias > extends => return Site::Inert,
        (Some(_), _) => return Site::Base,
        (None, Some(_)) => return Site::Inert,
        (None, None) => {}
    }

    let mut exported = false;
    let mut is_abstract = false;
    for word in finished {
        match *word {
            "export" => exported = true,
            "abstract" => is_abstract = true,
            // The keyword is complete. The author makes the name after it.
            _ => return Site::Inert,
        }
    }
    Site::DeclarationHead {
        exported,
        is_abstract,
    }
}

/// The rules for an indented line: keys, values and constraints.
fn indented(lines: &[Line], current: usize, content: &str) -> Site {
    let line = &lines[current];
    let parent = enclosing(lines, current, line.indent);
    // Only in the body of a declaration are the keys the properties of an
    // anchor. A key at a deeper indent names a dictionary entry.
    let in_body = parent.is_some_and(|parent| lines[parent].indent == 0);

    // A list item and a merge item hold a value. The key of their block tells
    // which values fit.
    let marker = list_marker(content).or_else(|| merge_marker(content).map(|(_, length)| length));
    if let Some(length) = marker {
        let written = content[length..].trim_start();
        let key = parent.and_then(|parent| {
            let grandparent = enclosing(lines, parent, lines[parent].indent)?;
            (lines[grandparent].indent == 0)
                .then(|| key_of(&lines[parent]))
                .flatten()
        });
        return single_word(written, key);
    }

    // A key ends at the first colon on the line. After it comes a chain of
    // `:: type` constraints, and then the value.
    if let Some(colon) = key_colon(content) {
        let key = &content[..colon];
        let after = &content[colon..];
        if let Some(value) = value_after_constraints(after) {
            return single_word(value, in_body.then(|| key.to_string()));
        }
        return Site::Constraint;
    }

    // The line has no colon at this time. Thus, the first word of the line is a key
    // that the author still writes. A key cannot contain a space. Thus, a line
    // with a space is prose.
    if content.contains(char::is_whitespace) || !in_body {
        return Site::Inert;
    }
    let Some(parent) = parent else {
        return Site::Inert;
    };
    let mut block_keys = Vec::new();
    let mut block_has_items = false;
    let body_indent = line.indent;
    for (index, other) in lines.iter().enumerate().skip(parent + 1) {
        if other.blank || other.escape == EscapeRole::Body || other.escape == EscapeRole::Close {
            continue;
        }
        if other.indent <= lines[parent].indent {
            break;
        }
        if index == current {
            continue;
        }
        block_has_items = true;
        if other.indent == body_indent {
            if let Some(key) = key_of(other) {
                block_keys.push(key);
            }
        }
    }
    Site::Key {
        block_keys,
        block_has_items,
    }
}

/// Tells if a line ends with the start of a block for the lines below it. If
/// it does, Enter must indent.
///
/// A declaration or a key that ends with its colon opens a block. Examples are
/// `anchor A:`, `frameworks:` and `config:: dictionary:`. A colon in a value
/// does not open a block. In `prompt: Careful:`, the colon ends a sentence,
/// not a property. A list item is never a key, with or without a colon.
/// `- Settings:` is the string `Settings:`. A `+`/`++` line holds a value. A
/// comment is not structure, for all last characters. A key with
/// constraints and no value (`title:: string`) also opens a block. A block can
/// follow it.
pub fn opens_block(line: &str) -> bool {
    let (code, _) = crate::prose::split_comment(line);
    let code = code.trim_end();
    let trimmed = code.trim_start();
    if trimmed.is_empty() || list_marker(trimmed).is_some() || merge_marker(trimmed).is_some() {
        return false;
    }
    if let Some(header) = parse_property_header(trimmed, 0) {
        if !header.has_value_colon {
            return true;
        }
        return trimmed[header.value_start..].trim().is_empty();
    }
    // A declaration starts at the margin and ends with its colon. Indented
    // text without a key is prose.
    !code.starts_with([' ', '\t']) && trimmed.ends_with(':')
}

/// A value of one word is a word to complete. An empty value, or a value with
/// a second word, is a sentence that the author still writes.
fn single_word(written: &str, key: Option<String>) -> Site {
    if written.is_empty() || written.contains(char::is_whitespace) {
        Site::Inert
    } else {
        Site::Value { key }
    }
}

/// The parent line of the block of a line. This is the nearest line above
/// that has less indentation and is not empty.
fn enclosing(lines: &[Line], from: usize, indent: usize) -> Option<usize> {
    (0..from).rev().find(|&index| {
        let line = &lines[index];
        !line.blank
            && line.escape != EscapeRole::Body
            && line.escape != EscapeRole::Close
            && line.indent < indent
    })
}

/// The key that a line declares, as the parser reads it.
fn key_of(line: &Line) -> Option<String> {
    let code = line.code.trim_start();
    if list_marker(code).is_some() || merge_marker(code).is_some() {
        return None;
    }
    parse_property_header(code, 0).map(|header| header.name)
}

/// The position of the colon that ends a key. This is the first colon,
/// directly after a run of key characters.
fn key_colon(content: &str) -> Option<usize> {
    let name = content
        .char_indices()
        .take_while(|(index, c)| c.is_alphanumeric() || *c == '_' || (*c == '-' && *index > 0))
        .map(|(index, c)| index + c.len_utf8())
        .last()?;
    content[name..].starts_with(':').then_some(name)
}

/// The value after the colon and the constraints of a key. It is `None` while
/// the cursor is still in the constraints.
fn value_after_constraints(after: &str) -> Option<&str> {
    let mut rest = after;
    loop {
        if let Some(constraint) = rest.strip_prefix("::") {
            // A constraint continues to the next `:`. If there is no `:`
            // at this time, the cursor is in the constraint.
            let end = constraint.find(':')?;
            rest = &constraint[end..];
            continue;
        }
        let value = rest.strip_prefix(':')?;
        if !value.is_empty() && !value.starts_with(' ') {
            return None;
        }
        return Some(value.trim_start());
    }
}

/// The sigil of the innermost brace that is still open at the end of `text`.
/// It also gives the position where the text in that brace starts. This function obeys
/// inline escapes. Thus, a brace in `\ ... \` does not open an expression. The
/// prose scanner does the same.
pub fn open_sigil(text: &str) -> Option<(Sigil, usize)> {
    let mut open: Vec<(Sigil, usize)> = Vec::new();
    let mut literal = false;
    let mut quoted = false;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut index = 0;
    while index < chars.len() {
        let (at, ch) = chars[index];
        if quoted {
            match ch {
                '\\' => index += 1,
                '"' => quoted = false,
                _ => {}
            }
            index += 1;
            continue;
        }
        if ch == '\\' {
            let run = chars[index..].iter().take_while(|(_, c)| *c == '\\').count();
            if run == 1 && open.is_empty() {
                let next = chars.get(index + 1).map(|(_, c)| *c);
                let previous = index.checked_sub(1).map(|before| chars[before].1);
                if !literal && next == Some(' ') {
                    literal = true;
                } else if literal && previous == Some(' ') {
                    literal = false;
                }
            }
            index += run;
            continue;
        }
        if literal {
            index += 1;
            continue;
        }
        let next = chars.get(index + 1).map(|(_, c)| *c);
        match (ch, next) {
            ('$', Some('{')) | ('#', Some('{')) | ('@', Some('{')) => {
                let sigil = match ch {
                    '$' => Sigil::Stringify,
                    '#' => Sigil::Numeric,
                    _ => Sigil::Reference,
                };
                open.push((sigil, at + 2));
                index += 2;
                continue;
            }
            ('{', _) => open.push((Sigil::Standard, at + 1)),
            ('}', _) => {
                open.pop();
            }
            ('"', _) if !open.is_empty() => quoted = true,
            _ => {}
        }
        index += 1;
    }
    open.last().copied()
}

/// Tells if the end of an expression is in a quoted string.
fn in_quotes(inside: &str) -> bool {
    let mut quoted = false;
    let mut escaped = false;
    for ch in inside.chars() {
        match ch {
            '\\' if quoted && !escaped => escaped = true,
            '"' if !escaped => quoted = !quoted,
            _ => escaped = false,
        }
    }
    quoted
}

/// The dotted path before a `.` at the end. For `this.config.`, it is
/// `this.config`. Only the last operand is important. In `a + b.`, it is `b`.
fn member_path(inside: &str) -> Option<&str> {
    let before = inside.strip_suffix('.').or_else(|| {
        let last = inside.rfind('.')?;
        let tail = &inside[last + 1..];
        tail.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            .then_some(&inside[..last])
    })?;
    let start = before
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-' || c == '.'))
        .map_or(0, |at| at + 1);
    let path = &before[start..];
    (!path.is_empty() && !path.starts_with('.')).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The site at `$0` in `marked`.
    fn site(marked: &str) -> Site {
        let offset = marked.find("$0").expect("a cursor marker");
        let source = marked.replacen("$0", "", 1);
        site_at(&source, offset)
    }

    const BODY: &str = "anchor A:\n    title: Hi\n";

    #[test]
    fn the_sites_of_the_design_table() {
        let expression = |member: Option<&str>| Site::Expression {
            sigil: Sigil::Standard,
            member_of: member.map(str::to_string),
        };
        let cases: Vec<(String, Site)> = vec![
            ("// note $0".into(), Site::Inert),
            (format!("{BODY}    // {{a$0"), Site::Inert),
            (format!("{BODY}    url: https://x.com // {{a$0"), expression(None)),
            (format!("{BODY}    v: \\ {{hel$0"), Site::Inert),
            (format!("{BODY}    v:\n        \\\\\\\n        {{x$0\n        \\\\\\\n"), Site::Inert),
            ("use ./li$0".into(), Site::ModulePath),
            ("from ./a $0".into(), Site::ImportDirection),
            (
                "from ./a import B, $0".into(),
                Site::ImportName {
                    path: "./a".into(),
                    listed: vec!["B".into()],
                },
            ),
            (
                "export abs$0".into(),
                Site::DeclarationHead {
                    exported: true,
                    is_abstract: false,
                },
            ),
            ("anchor A extends B, $0".into(), Site::Base),
            ("anchor A as k$0".into(), Site::Inert),
            ("anchor Na$0".into(), Site::Inert),
            (format!("{BODY}    t:: str$0"), Site::Constraint),
            (format!("{BODY}    t: {{this.$0"), expression(Some("this"))),
            (format!("{BODY}    t: see {{Fo$0}} x"), expression(None)),
            (
                format!("{BODY}    ti$0"),
                Site::Key {
                    block_keys: vec!["title".into()],
                    block_has_items: true,
                },
            ),
            (format!("{BODY}    t: Hello wor$0"), Site::Inert),
            (format!("{BODY}    t: $0"), Site::Inert),
            (format!("{BODY}    t: tr$0"), Site::Value { key: Some("t".into()) }),
            (format!("{BODY}    l:\n        - {{B$0"), expression(None)),
            (format!("{BODY}    l:\n        - B$0"), Site::Value { key: Some("l".into()) }),
            (format!("{BODY}    d:\n        k: v$0"), Site::Value { key: None }),
            (format!("{BODY}    d:\n        ke$0"), Site::Inert),
            (
                "anchor Foo extends\n    ti$0".into(),
                Site::Key {
                    block_keys: vec![],
                    block_has_items: false,
                },
            ),
            (
                format!("{BODY}    t: {{\"a.$0"),
                Site::Inert,
            ),
            (
                format!("{BODY}    t: ${{\"in a string\" + $0"),
                Site::Expression {
                    sigil: Sigil::Stringify,
                    member_of: None,
                },
            ),
            ("expo$0".into(), Site::DeclarationHead { exported: false, is_abstract: false }),
            ("from ./x imp$0".into(), Site::ImportDirection),
            (
                "from ./x export Na$0".into(),
                Site::ImportName {
                    path: "./x".into(),
                    listed: vec![],
                },
            ),
            (format!("{BODY}    title:$0"), Site::Inert),
            (format!("{BODY}    n:: string: $0"), Site::Inert),
            (format!("{BODY}    p:: dictionary:: null: nu$0"), Site::Value { key: Some("p".into()) }),
            (format!("{BODY}    l:\n        ++ t$0"), Site::Value { key: Some("l".into()) }),
            (format!("{BODY}    l:\n        - $0"), Site::Inert),
            (format!("{BODY}    a: {{x}} and $0"), Site::Inert),
            (format!("{BODY}    a: The end.$0"), Site::Inert),
            ("a: see https://example.com/$0".into(), Site::Inert),
            ("count:: num$0".into(), Site::Constraint),
            ("flag: tr$0".into(), Site::Value { key: None }),
        ];
        for (marked, expected) in cases {
            assert_eq!(site(&marked), expected, "{marked:?}");
        }
    }

    #[test]
    fn which_lines_open_a_block() {
        let cases = [
            ("anchor A:", true),
            ("export abstract anchor A extends B as k:", true),
            ("    config:: dictionary:", true),
            ("    config:", true),
            ("    title:: string", true),
            ("    title: Note:", false),
            ("    - Settings:", false),
            ("    + {x}:", false),
            ("    // Heading:", false),
            ("    The plan:", false),
            ("    url: https://x.com", false),
        ];
        for (line, opens) in cases {
            assert_eq!(opens_block(line), opens, "{line}");
        }
    }

    #[test]
    fn member_paths_take_the_last_operand() {
        assert_eq!(member_path("this.config."), Some("this.config"));
        assert_eq!(member_path("a + b."), Some("b"));
        assert_eq!(member_path("this.co"), Some("this"));
        assert_eq!(member_path("Foo"), None);
    }

    #[test]
    fn a_closed_brace_is_not_open() {
        assert_eq!(open_sigil("see {x} and"), None);
        assert_eq!(open_sigil("see ${x"), Some((Sigil::Stringify, 6)));
        assert_eq!(open_sigil(r"a \ {x"), None);
        assert_eq!(open_sigil(r"a \ {x} \ {y"), Some((Sigil::Standard, 11)));
    }
}
