//! What can be written at a position in a file.
//!
//! An editor asks this on every keystroke, about text that is usually not a
//! finished program: a header without its colon, a key without its value, a
//! brace still open. It is answered here, with the parser's own line scanner
//! and header recognisers, so that what completion believes a line is never
//! drifts from what the compiler will make of it.

use crate::ast::Sigil;
use crate::parser::{
    list_marker, merge_marker, parse_property_header, scan_lines, EscapeRole, Line,
};
use std::path::Path;

/// What the cursor is in the middle of writing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Site {
    /// Nothing can be offered: a comment, prose, a verbatim block, a quoted
    /// string, a name being declared, a keyword being invented, an empty
    /// value.
    Inert,
    /// The module path after `use` or `from`.
    ModulePath,
    /// After `from <path>`: `import` or `export`.
    ImportDirection,
    /// A name in the list after `from <path> import`, with the ones already
    /// listed.
    ImportName { path: String, listed: Vec<String> },
    /// The first words of a top-level line.
    DeclarationHead { exported: bool, is_abstract: bool },
    /// A base, after `extends` or a comma following one.
    Base,
    /// A type, after `::`.
    Constraint,
    /// Inside braces. `member_of` is the dotted path before a trailing `.`.
    Expression { sigil: Sigil, member_of: Option<String> },
    /// The key at the start of a line in an anchor's body, with the keys the
    /// body already has (other than this line's) and whether it has anything
    /// else in it.
    Key { block_keys: Vec<String>, block_has_items: bool },
    /// A single word of value after `key: `, `- ` or `+ `. `key` is the
    /// anchor property whose constraints say what fits, when there is one.
    Value { key: Option<String> },
}

/// What can be written at `offset` in `source`.
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

    // A verbatim block says nothing Piton can complete, and neither does a
    // comment once its `//` has been written.
    if line.escape != EscapeRole::None {
        return Site::Inert;
    }
    let content = &source[line.content_start.min(offset)..offset];
    if content.starts_with("//") {
        return Site::Inert;
    }

    // An open brace wins over whatever it is written inside: `key: {x` is an
    // expression, not a value.
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

/// The rules for a line at the margin: imports, declarations, and the
/// variables that are keys at the top level.
fn top_level(content: &str) -> Site {
    if let Some(colon) = key_colon(content) {
        return match value_after_constraints(&content[colon..]) {
            Some(value) => single_word(value, None),
            None => Site::Constraint,
        };
    }
    let ends_in_space = content.ends_with(char::is_whitespace);
    let words: Vec<&str> = content.split_whitespace().collect();
    // The words that are finished: everything, or everything but the one
    // still being typed.
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
                        // The last name is the one being written.
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

    // `anchor Name extends B as k:` -- after `extends` come bases, after `as`
    // a keyword the author is inventing; whichever was written last is the one
    // the cursor is past.
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
            // The keyword is written; the name after it is the author's own.
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
    // The body of a declaration is the only block whose keys are an anchor's
    // properties; a key further in names a dictionary entry.
    let in_body = parent.is_some_and(|parent| lines[parent].indent == 0);

    // A list item and a merge item hold a value, and the key whose block they
    // sit in says which values fit.
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

    // A key takes the first colon on the line; what follows it is a chain of
    // `:: type` constraints and then the value.
    if let Some(colon) = key_colon(content) {
        let key = &content[..colon];
        let after = &content[colon..];
        if let Some(value) = value_after_constraints(after) {
            return single_word(value, in_body.then(|| key.to_string()));
        }
        return Site::Constraint;
    }

    // No colon yet: the first word of the line is a key being written. A key
    // cannot hold a space, so once there is one the line is prose.
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

/// Whether a line ends by opening a block for the lines beneath it, which is
/// when Enter should indent.
///
/// A declaration or a key that ends in its colon opens one: `anchor A:`,
/// `frameworks:`, `config:: dictionary:`. A colon inside a value opens
/// nothing -- `prompt: Careful:` ends a sentence, not a property. A list item
/// is never a key, colon or not (`- Settings:` is the string `Settings:`), a
/// `+`/`++` line holds a value, and a comment is not structure whatever it
/// ends with. A key with constraints and no value (`title:: string`) opens a
/// block too, because one may follow.
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
    // A declaration sits at the margin and ends in its colon; indented text
    // without a key is prose.
    !code.starts_with([' ', '\t']) && trimmed.ends_with(':')
}

/// A value that is one word so far is something to complete; nothing at all,
/// or a second word, is a sentence being written.
fn single_word(written: &str, key: Option<String>) -> Site {
    if written.is_empty() || written.contains(char::is_whitespace) {
        Site::Inert
    } else {
        Site::Value { key }
    }
}

/// The line a line's block belongs to: the nearest line above with less
/// indentation and something on it.
fn enclosing(lines: &[Line], from: usize, indent: usize) -> Option<usize> {
    (0..from).rev().find(|&index| {
        let line = &lines[index];
        !line.blank
            && line.escape != EscapeRole::Body
            && line.escape != EscapeRole::Close
            && line.indent < indent
    })
}

/// The key a line declares, as the parser reads it.
fn key_of(line: &Line) -> Option<String> {
    let code = line.code.trim_start();
    if list_marker(code).is_some() || merge_marker(code).is_some() {
        return None;
    }
    parse_property_header(code, 0).map(|header| header.name)
}

/// Where the colon that ends a key is: the first colon, directly after a run
/// of key characters.
fn key_colon(content: &str) -> Option<usize> {
    let name = content
        .char_indices()
        .take_while(|(index, c)| c.is_alphanumeric() || *c == '_' || (*c == '-' && *index > 0))
        .map(|(index, c)| index + c.len_utf8())
        .last()?;
    content[name..].starts_with(':').then_some(name)
}

/// The value written after a key's colon and constraints, or `None` while the
/// cursor is still among the constraints.
fn value_after_constraints(after: &str) -> Option<&str> {
    let mut rest = after;
    loop {
        if let Some(constraint) = rest.strip_prefix("::") {
            // A constraint runs to the next `:`; until there is one, the
            // cursor is in it.
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

/// The sigil of the innermost brace still open at the end of `text`, and
/// where the text inside it starts. Inline escapes are honoured: a brace
/// inside `\ ... \` opens nothing, which is what the prose scanner does too.
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

/// Whether the end of an expression is inside a quoted string.
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

/// The dotted path before a trailing `.`: `this.config.` gives
/// `this.config`. Only the last operand counts: in `a + b.` it is `b`.
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
