//! `piton format`.
//!
//! The canonical style is four spaces per level and is not configurable. The
//! formatter walks the lossless tree and re-emits it, so prose is copied
//! verbatim and only the structure around it is normalised.

use piton_syntax::ast::{self, AstNode};
use piton_syntax::kind::SyntaxKind::{self, *};
use piton_syntax::{NodeOrToken, SyntaxNode, SyntaxToken};

/// One indentation level.
pub const INDENT: &str = "    ";
/// Imports wrap once the line would pass this column.
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
    fn container(&mut self, node: SyntaxNode, depth: usize) {
        for element in node.children_with_tokens() {
            match element {
                NodeOrToken::Token(token) if token.kind() == COMMENT => {
                    self.line(depth, &comment(&token));
                }
                NodeOrToken::Token(_) => {}
                NodeOrToken::Node(child) => self.item(child, depth),
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
    fn keeps_prose_verbatim() {
        let source = "note:\n    This is  prose   with odd spacing.\n\n    And a second paragraph.\n";
        check(source, source);
    }

    #[test]
    fn collapses_repeated_blank_lines() {
        check("a: 1\n\n\n\nb: 2\n", "a: 1\n\nb: 2\n");
    }
}
