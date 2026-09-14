//! `piton format`.
//!
//! The canonical style is four spaces per level and is not configurable. The
//! formatter walks the lossless tree and re-emits it, normalising the structure
//! and rewrapping prose paragraphs to fill the line. Lines inside a paragraph
//! join with a space when compiled, so where they break carries no meaning, and
//! nothing else about prose is touched.

use piton_syntax::ast::{self, AstNode};
use piton_syntax::kind::SyntaxKind::{self, *};
use piton_syntax::{NodeOrToken, SyntaxNode, SyntaxToken};

/// One indentation level.
pub const INDENT: &str = "    ";
/// Imports wrap, and prose fills, up to this column.
pub const MAX_WIDTH: usize = 80;
/// Imports wrap once they list more than this many names.
pub const MAX_INLINE_IMPORTS: usize = 2;

/// Format a Piton source file.
///
/// The file's own line endings are kept: rewriting every line of a CRLF file
/// would turn a formatting change into a whole-file diff.
pub fn format(source: &str) -> String {
    let parse = piton_syntax::parse(source);
    let mut out = Printer::default();
    out.container(parse.syntax(), 0);
    let formatted = out.finish();
    match source.contains("\r\n") {
        true => formatted.replace('\n', "\r\n"),
        false => formatted,
    }
}

#[derive(Default)]
struct Printer {
    buffer: String,
    /// Suppresses repeated and leading blank lines.
    blank_pending: bool,
    wrote_anything: bool,
}

impl Printer {
    fn finish(mut self) -> String {
        if !self.buffer.ends_with('\n') && self.wrote_anything {
            self.buffer.push('\n');
        }
        self.buffer
    }

    fn line(&mut self, depth: usize, text: &str) {
        if self.blank_pending && self.wrote_anything {
            self.buffer.push('\n');
        }
        self.blank_pending = false;
        if !text.is_empty() {
            self.buffer.push_str(&INDENT.repeat(depth));
            self.buffer.push_str(text);
        }
        self.buffer.push('\n');
        self.wrote_anything = true;
    }

    /// Emit a line exactly as it was written, indentation and all.
    fn verbatim_line(&mut self, text: &str) {
        if self.blank_pending && self.wrote_anything {
            self.buffer.push('\n');
        }
        self.blank_pending = false;
        self.buffer.push_str(text);
        self.buffer.push('\n');
        self.wrote_anything = true;
    }

    fn blank(&mut self) {
        self.blank_pending = true;
    }

    /// Print the entries of a root or a block, keeping stray comments in place.
    ///
    /// Consecutive lines of prose are gathered into a paragraph and rewrapped
    /// together; a comment or any other entry ends the paragraph.
    fn container(&mut self, node: SyntaxNode, depth: usize) {
        let mut paragraph: Vec<(SyntaxNode, Vec<String>)> = Vec::new();
        for element in node.children_with_tokens() {
            match element {
                NodeOrToken::Token(token) if token.kind() == COMMENT => {
                    self.paragraph(std::mem::take(&mut paragraph), depth);
                    self.line(depth, &comment(&token));
                }
                NodeOrToken::Token(_) => {}
                NodeOrToken::Node(child) => match prose_words(&child) {
                    Some(words) => paragraph.push((child, words)),
                    None => {
                        self.paragraph(std::mem::take(&mut paragraph), depth);
                        self.item(child, depth);
                    }
                },
            }
        }
        self.paragraph(paragraph, depth);
    }

    /// Print a paragraph of prose filled to the line, or as it was written when
    /// no rewrapping of it reads the same.
    fn paragraph(&mut self, lines: Vec<(SyntaxNode, Vec<String>)>, depth: usize) {
        if lines.is_empty() {
            return;
        }
        let width = MAX_WIDTH.saturating_sub(INDENT.len() * depth);
        let words = lines.iter().flat_map(|(_, words)| words.iter().cloned()).collect();
        match rewrap(words, width) {
            Some(filled) => {
                for line in filled {
                    self.line(depth, &line);
                }
            }
            // The lines keep their breaks, and their spacing is tidied when
            // that alone still reads the same.
            None => {
                let tidied: Vec<String> = lines.iter().map(|(_, words)| words.join(" ")).collect();
                if misread_line(&tidied).is_none() {
                    for line in tidied {
                        self.line(depth, &line);
                    }
                } else {
                    for (line, _) in lines {
                        self.item(line, depth);
                    }
                }
            }
        }
    }

    fn item(&mut self, node: SyntaxNode, depth: usize) {
        match node.kind() {
            BLANK_LINE => self.blank(),
            ANCHOR_DECL => self.declaration(&node, depth, anchor_head(&node)),
            VAR_DECL => {
                let head = format!("{}{}", export_prefix(&node), key_head(&node));
                self.declaration(&node, depth, head)
            }
            PROPERTY => self.declaration(&node, depth, key_head(&node)),
            LIST_ITEM => match node.children().find(|it| it.kind() == PROPERTY) {
                // `- key: value` is a dictionary written as one element, so
                // the key head joins the marker and the value and the block
                // are the property's, not the item's.
                Some(property) => {
                    let head = format!("- {}", key_head(&property));
                    self.declaration(&property, depth, head)
                }
                None => self.declaration(&node, depth, "-".to_string()),
            },
            SPREAD_ITEM => {
                let marker = if token_of(&node, PLUS2).is_some() { "++" } else { "+" };
                self.declaration(&node, depth, marker.to_string())
            }
            TEXT_LINE => self.declaration(&node, depth, String::new()),
            IMPORT_DECL | REEXPORT_DECL => self.import(&node, depth),
            USE_DECL => {
                let path = token_of(&node, PATH).map(text_of).unwrap_or_default();
                self.line(depth, &format!("use {path}"));
            }
            EXPORT_DECL => {
                let name = token_of(&node, IDENT).map(text_of).unwrap_or_default();
                self.line(depth, &format!("export {name}"));
            }
            ERROR => {
                // Unparsable input is preserved exactly, indentation included,
                // so that formatting can never change what a broken file says.
                // Re-indenting it by the depth the formatter thinks it is at
                // moved an indented line to column zero, where it became a
                // different declaration.
                for raw in verbatim(&node).lines() {
                    self.verbatim_line(raw.trim_end());
                }
            }
            _ => {}
        }
    }

    /// Emit `head[ value][ // comment]`, then the nested block.
    fn declaration(&mut self, node: &SyntaxNode, depth: usize, head: String) {
        let mut line = head;
        if let Some(value) = node.children().find(|it| it.kind() == VALUE) {
            let text = value.text().to_string();
            let text = text.trim();
            if !text.is_empty() {
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(text);
            }
        }
        let (trailing, leading) = split_comments(node);
        for comment in &trailing {
            line.push_str("  ");
            line.push_str(comment);
        }
        self.line(depth, line.trim_end());
        for comment in leading {
            self.line(depth + 1, &comment);
        }
        if let Some(block) = node.children().find(|it| it.kind() == BLOCK) {
            self.container(block, depth + 1);
        }
    }

    /// `from PATH import a, b`, sorted and wrapped when it gets long.
    fn import(&mut self, node: &SyntaxNode, depth: usize) {
        let path = token_of(node, PATH).map(text_of).unwrap_or_default();
        let verb = if node.kind() == IMPORT_DECL { "import" } else { "export" };
        if token_of(node, STAR).is_some() {
            self.line(depth, &format!("from {path} export *"));
            return;
        }
        let mut names: Vec<String> = ast::ImportList::cast(
            node.children().find(|it| it.kind() == IMPORT_LIST).unwrap_or_else(|| node.clone()),
        )
        .map(|list| {
            list.items()
                .filter_map(|item| {
                    let name = item.name()?;
                    Some(match item.alias() {
                        Some(alias) => format!("{name} {alias}"),
                        None => name,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
        names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a.cmp(b)));

        let head = format!("from {path} {verb} ");
        let inline = format!("{head}{}", names.join(", "));
        if names.len() <= MAX_INLINE_IMPORTS
            && INDENT.repeat(depth).len() + inline.len() <= MAX_WIDTH
        {
            self.line(depth, inline.trim_end());
            return;
        }
        self.line(depth, head.trim_end());
        for (index, name) in names.iter().enumerate() {
            let comma = if index + 1 == names.len() { "" } else { "," };
            self.line(depth + 1, &format!("{name}{comma}"));
        }
    }
}

/// The words of a line of prose, or `None` for anything else.
///
/// A gap between words is pointless spacing and ends the word, except for two
/// spaces after a sentence, which stay inside it, and any spacing inside a
/// `code span`, which is content. An expression in braces and a quoted string
/// are always one word. A line holding a comment or text the parser could not
/// read is not prose to rewrap.
fn prose_words(node: &SyntaxNode) -> Option<Vec<String>> {
    if node.kind() != TEXT_LINE {
        return None;
    }
    if node.descendants_with_tokens().any(|it| matches!(it.kind(), COMMENT | ERROR | ERROR_TOKEN)) {
        return None;
    }
    // A line that is a single expression compiles to that value, not to text.
    let text = node.children().find(|it| it.kind() == VALUE)?.first_child()?;
    if text.kind() != TEXT_VALUE {
        return None;
    }
    // A line that begins like a key is prose only because a run of it came
    // first. It may well be a key written by mistake, so it keeps its place.
    if looks_like_key(text.text().to_string().trim_start()) {
        return None;
    }
    let mut words = Vec::new();
    let mut word = String::new();
    // Backticks seen so far on the line: an odd count means inside a code span.
    let mut ticks = 0;
    for element in text.children_with_tokens() {
        match element {
            NodeOrToken::Token(token) if token.kind() == WHITESPACE => {
                if ticks % 2 == 1 {
                    word.push_str(token.text());
                } else if token.text() != " " && ends_sentence(&word) {
                    word.push_str("  ");
                } else {
                    words.push(std::mem::take(&mut word));
                }
            }
            NodeOrToken::Token(token) => {
                ticks += token.text().matches('`').count();
                word.push_str(token.text());
            }
            NodeOrToken::Node(node) => {
                let text = node.text().to_string();
                ticks += text.matches('`').count();
                word.push_str(&text);
            }
        }
    }
    words.push(word);
    // Spacing at either end of a line is not part of what it says.
    if let Some(first) = words.first_mut() {
        *first = first.trim_start().to_string();
    }
    if let Some(last) = words.last_mut() {
        *last = last.trim_end().to_string();
    }
    words.retain(|it| !it.is_empty());
    Some(words)
}

/// Fill `words` into lines no wider than `width`, taking only breaks after
/// which every line still reads as prose.
///
/// A break that makes a line read as something else — a lone `-` starting it,
/// say — is not taken: the words either side of it are held together and the
/// paragraph is filled again. `None` when no filling reads the same.
fn rewrap(mut words: Vec<String>, width: usize) -> Option<Vec<String>> {
    loop {
        let lines = fill(&words, width);
        let texts: Vec<String> = lines.iter().map(|(_, text)| text.clone()).collect();
        match misread_line(&texts) {
            None => return Some(texts),
            Some(0) => return None,
            Some(index) => {
                let start = lines[index].0;
                let held = words.remove(start);
                words[start - 1].push(' ');
                words[start - 1].push_str(&held);
            }
        }
    }
}

/// Greedy filling: each line takes words until the next would pass `width`.
/// Each line is paired with the index of the word it starts with.
fn fill(words: &[String], width: usize) -> Vec<(usize, String)> {
    let mut lines: Vec<(usize, String)> = Vec::new();
    for (index, word) in words.iter().enumerate() {
        match lines.last_mut() {
            Some((_, line)) if line.chars().count() + 1 + word.chars().count() <= width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push((index, word.clone())),
        }
    }
    lines
}

/// The first of `lines` that does not parse back as the same line of prose.
///
/// The lines are read on their own, under a key. That is never more lenient
/// than where they came from: the first line starts a run of prose here, so a
/// `key:` it begins with reads as a key, which it may not have in its file.
fn misread_line(lines: &[String]) -> Option<usize> {
    let mut source = String::from("k:\n");
    let mut starts = Vec::new();
    for line in lines {
        starts.push(source.len() + INDENT.len());
        source.push_str(INDENT);
        source.push_str(line);
        source.push('\n');
    }
    let parse = piton_syntax::parse(&source);
    let entries: Vec<SyntaxNode> = parse
        .syntax()
        .descendants()
        .find(|it| it.kind() == BLOCK)
        .map(|block| block.children().collect())
        .unwrap_or_default();
    (0..lines.len()).find(|&index| {
        let start = starts[index];
        let end = start + lines[index].len();
        let entry = entries.iter().find(|it| usize::from(it.text_range().start()) == start);
        let same = entry.is_some_and(|entry| {
            usize::from(entry.text_range().end()) <= end + 1
                && prose_words(entry).map(|words| words.join(" ")) == Some(lines[index].clone())
        });
        !same
    })
}

/// Whether a word ends a sentence, closing brackets, quotes, and emphasis aside.
fn ends_sentence(word: &str) -> bool {
    word.trim_end_matches([')', ']', '"', '\'', '*', '_', '`']).ends_with(['.', '!', '?'])
}

/// Whether text opens with `name:` or `name::`, the way a key does.
///
/// A colon followed by anything else, as in `https://`, is not a key.
fn looks_like_key(text: &str) -> bool {
    let Some(colon) = text.find(':') else { return false };
    let after = text[colon + 1..].chars().next();
    colon > 0
        && piton_syntax::is_identifier(&text[..colon])
        && matches!(after, None | Some(':' | ' ' | '\t'))
}

/// `export abstract anchor Name extends A, B as kw:`
fn anchor_head(node: &SyntaxNode) -> String {
    let mut head = String::new();
    head.push_str(&export_prefix(node));
    if token_of(node, ABSTRACT_KW).is_some() {
        head.push_str("abstract ");
    }
    match token_of(node, ANCHOR_KW) {
        Some(_) => head.push_str("anchor "),
        None => {
            if let Some(keyword) = idents(node).next() {
                head.push_str(&format!("{keyword} "));
            }
        }
    }
    let skip = usize::from(token_of(node, ANCHOR_KW).is_none());
    if let Some(name) = idents(node).nth(skip) {
        head.push_str(&name);
    }
    if let Some(clause) = node.children().find_map(ast::ExtendsClause::cast) {
        head.push_str(&format!(" extends {}", clause.names().join(", ")));
    }
    if let Some(clause) = node.children().find_map(ast::AsClause::cast) {
        if let Some(name) = clause.name() {
            head.push_str(&format!(" as {name}"));
        }
    }
    head.push(':');
    head
}

/// `name:: T:: U:` or, for a shape-only declaration, `name:: T`.
fn key_head(node: &SyntaxNode) -> String {
    let mut head = idents(node).next().unwrap_or_default();
    for annotation in node.children().filter_map(ast::TypeAnnotation::cast) {
        if let Some(type_expr) = annotation.type_expr() {
            head.push_str(&format!(":: {}", type_expr.syntax().text().to_string().trim()));
        }
    }
    if token_of(node, COLON).is_some() {
        head.push(':');
    }
    head
}

fn export_prefix(node: &SyntaxNode) -> String {
    match token_of(node, EXPORT_KW) {
        Some(_) => "export ".to_string(),
        None => String::new(),
    }
}

/// Comments written on the declaration line, and those written after it.
fn split_comments(node: &SyntaxNode) -> (Vec<String>, Vec<String>) {
    let newline = node
        .children_with_tokens()
        .filter_map(|it| it.into_token())
        .find(|it| it.kind() == NEWLINE)
        .map(|it| it.text_range().start());
    let mut trailing = Vec::new();
    let mut leading = Vec::new();
    for token in node.children_with_tokens().filter_map(|it| it.into_token()) {
        if token.kind() != COMMENT {
            continue;
        }
        match newline {
            Some(at) if token.text_range().start() > at => leading.push(comment(&token)),
            _ => trailing.push(comment(&token)),
        }
    }
    (trailing, leading)
}

/// A node's text from the start of its first line.
///
/// The indentation in front of a node belongs to no token inside it, so the
/// node's own text starts after it. When only whitespace precedes the node on
/// its line, that whitespace is part of what was written and is kept.
fn verbatim(node: &SyntaxNode) -> String {
    let root = node.ancestors().last().unwrap_or_else(|| node.clone());
    let text = root.text().to_string();
    let start = usize::from(node.text_range().start());
    let end = usize::from(node.text_range().end());
    let line_start = text[..start].rfind('\n').map_or(0, |at| at + 1);
    let from = if text[line_start..start].trim().is_empty() { line_start } else { start };
    text[from..end].to_string()
}

fn comment(token: &SyntaxToken) -> String {
    let body = token.text().trim_start_matches('/').trim();
    if body.is_empty() {
        "//".to_string()
    } else {
        format!("// {body}")
    }
}

fn token_of(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    node.children_with_tokens().filter_map(|it| it.into_token()).find(|it| it.kind() == kind)
}

fn idents(node: &SyntaxNode) -> impl Iterator<Item = String> + '_ {
    node.children_with_tokens()
        .filter_map(|it| it.into_token())
        .filter(|it| it.kind() == IDENT)
        .map(|it| it.text().to_string())
}

fn text_of(token: SyntaxToken) -> String {
    token.text().to_string()
}

#[cfg(test)]
mod tests {
    use super::format;

    fn check(input: &str, expected: &str) {
        let output = format(input);
        assert_eq!(output, expected);
        assert_eq!(format(&output), output, "formatting must be idempotent");
    }

    #[test]
    fn reindents_to_four_spaces() {
        check(
            "anchor A:\n  name: Base\n  items:\n   - one\n   - two\n",
            "anchor A:\n    name: Base\n    items:\n        - one\n        - two\n",
        );
    }

    #[test]
    fn normalises_comments_and_annotations() {
        check("//comment\nx::  number:  42 //note\n", "// comment\nx:: number: 42  // note\n");
    }

    #[test]
    fn sorts_and_wraps_imports() {
        check("from ./x import B, A\n", "from ./x import A, B\n");
        check(
            "from ./x import Charlie, Alpha, Bravo\n",
            "from ./x import\n    Alpha,\n    Bravo,\n    Charlie\n",
        );
        check("from ./x export *\n", "from ./x export *\n");
    }

    #[test]
    fn preserves_unparsable_lines() {
        // `::` needs a space after it, so this line is an error and is kept.
        check("x::number: 42\n", "x::number: 42\n");
    }

    #[test]
    fn keeps_the_indentation_of_unparsable_lines() {
        // A comment inside an import list does not parse. The names after it
        // have to stay indented: at column zero they would be declarations.
        let source = "from ./y import\n    // inner\n    C,\n    D\n";
        check(source, source);
        let nested = "anchor A:\n    x::bad: 1\n    y: 2\n";
        let formatted = format(nested);
        assert!(formatted.contains("    x::bad: 1\n"), "{formatted}");
    }

    #[test]
    fn wraps_imports_that_run_past_eighty_columns() {
        check(
            "from ./a/very/long/module/path/that/goes/on/and/on import AlphaBravo, CharlieDelta\n",
            "from ./a/very/long/module/path/that/goes/on/and/on import\n    AlphaBravo,\n    CharlieDelta\n",
        );
    }

    #[test]
    fn collapses_pointless_spaces_between_words() {
        check(
            "note:\n    This is  prose   with odd spacing.\n\n    And a second paragraph.\n",
            "note:\n    This is prose with odd spacing.\n\n    And a second paragraph.\n",
        );
        check(
            "a:\n    b:\n        rectangleExample:\n            Rectangle is a primitive type of its own, not a Polygon made by a\n            rectangle verb.  Its corner and anchor definitions are parameter            modes on the verb that builds it.\n",
            "a:\n    b:\n        rectangleExample:\n            Rectangle is a primitive type of its own, not a Polygon made by a\n            rectangle verb.  Its corner and anchor definitions are parameter\n            modes on the verb that builds it.\n",
        );
    }

    #[test]
    fn keeps_two_spaces_after_a_sentence_and_no_more() {
        check("note:\n    One.     Two!   Three? Four.\n", "note:\n    One.  Two!  Three? Four.\n");
        check("note:\n    Emphasis.*   Then (aside.)   Done\n", "note:\n    Emphasis.*  Then (aside.)  Done\n");
        // A quoted string is one word, and its own spacing is its content.
        check("note:\n    He said \"stop.   now\"   and left\n", "note:\n    He said \"stop.   now\" and left\n");
    }

    #[test]
    fn keeps_the_spacing_inside_a_code_span() {
        check("note:\n    Run `a    b`   now\n", "note:\n    Run `a    b` now\n");
    }

    #[test]
    fn fills_a_paragraph_to_eighty_columns() {
        check(
            "note:\n    This line of prose runs on well past the eightieth column of the file, so it wraps.\n",
            "note:\n    This line of prose runs on well past the eightieth column of the file, so it\n    wraps.\n",
        );
        check("note:\n    Short\n    lines\n    join up.\n", "note:\n    Short lines join up.\n");
    }

    #[test]
    fn counts_indentation_against_the_width() {
        let words = "word ".repeat(20);
        let output = format(&format!("a:\n    b:\n        c:\n            {words}\n"));
        assert!(output.lines().all(|line| line.len() <= 80), "{output}");
        assert!(output.lines().nth(4).is_some(), "the paragraph wrapped: {output}");
    }

    #[test]
    fn keeps_paragraphs_and_what_ends_them_apart() {
        // A blank line is what lets `key: value` be a key; without it the line
        // would still be prose, and joining it would be correct.
        let source =
            "note:\n    First paragraph.\n\n    Second paragraph.\n    // an aside\n    Third.\n\n    key: value\n";
        check(source, source);
    }

    #[test]
    fn never_breaks_inside_a_wide_gap_an_expression_or_a_quote() {
        let source = "note:\n    Wrap here please.  Not inside {one + two} or \"a quoted phrase\" at all ever.\n";
        let output = format(source);
        assert!(output.contains("please.  Not"), "{output}");
        assert!(output.contains("{one + two}"), "{output}");
        assert!(output.contains("\"a quoted phrase\""), "{output}");
        assert_eq!(format(&output), output);
    }

    #[test]
    fn never_starts_a_line_with_a_list_marker() {
        // Breaking before the lone `-` would turn the rest into a list item.
        // Thirty-eight words fill the line, so the `-` would open the next one.
        let source = format!("note:\n    {}- beta\n", "x ".repeat(38));
        let output = format(&source);
        assert!(!output.lines().any(|line| line.trim_start().starts_with("- ")), "{output}");
        assert_eq!(format(&output), output);
    }

    #[test]
    fn keeps_a_line_that_begins_like_a_key_on_its_own() {
        let source = "note:\n    Some prose\n    and: more prose\n    that follows it\n";
        check(source, "note:\n    Some prose\n    and: more prose\n    that follows it\n");
    }

    #[test]
    fn never_starts_a_line_with_something_that_looks_like_a_key() {
        let source = format!("note:\n    {}note: beta\n", "x ".repeat(38));
        let output = format(&source);
        assert!(!output.lines().skip(1).any(|line| line.trim_start().starts_with("note:")), "{output}");
        assert_eq!(format(&output), output);
        // A URL's colon is not a key's.
        let url = format!("note:\n    {}https://example.com here\n", "x ".repeat(36));
        assert!(format(&url).contains("\n    https://example.com here\n"), "{}", format(&url));
    }

    #[test]
    fn leaves_a_line_that_is_an_expression_alone() {
        let source = "note:\n    Before\n    {1 + 2}\n    after\n";
        check(source, "note:\n    Before\n    {1 + 2}\n    after\n");
    }

    #[test]
    fn puts_an_overlong_word_on_a_line_of_its_own() {
        let url = format!("https://example.com/{}", "a".repeat(80));
        check(&format!("note:\n    see {url} now\n"), &format!("note:\n    see\n    {url}\n    now\n"));
    }

    #[test]
    fn collapses_repeated_blank_lines() {
        check("a: 1\n\n\n\nb: 2\n", "a: 1\n\nb: 2\n");
    }
}
