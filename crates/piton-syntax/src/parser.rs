//! The layout parser.
//!
//! Piton is whitespace-structured, so the outer grammar is driven by lines and
//! indentation rather than by a token stream. This module turns source text into
//! both a typed [`SourceFile`] and a lossless rowan tree; expression interiors
//! are delegated to [`crate::expr`], and prose interiors to [`crate::prose`].

use std::path::{Path, PathBuf};

use piton_core::{Diagnostic, Span};
use rowan::{GreenNode, GreenNodeBuilder};

use crate::ast::*;
use crate::kind::{SyntaxKind, SyntaxNode};
use crate::prose;

/// The result of parsing one file.
pub struct Parse {
    pub green: GreenNode,
    pub file: SourceFile,
    pub diagnostics: Vec<Diagnostic>,
}

impl Parse {
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }
}

/// Parses a `.pi` source file.
pub fn parse(source: &str, path: &Path) -> Parse {
    let lines = scan_lines(source, path);
    let mut parser = Parser {
        source,
        file: path.to_path_buf(),
        lines: lines.lines,
        pos: 0,
        events: Vec::new(),
        diagnostics: lines.diagnostics,
    };
    let file = parser.parse_file();
    let events = std::mem::take(&mut parser.events);
    let mut diagnostics = std::mem::take(&mut parser.diagnostics);
    let green = build_green(source, &events, &mut diagnostics);
    Parse {
        green,
        file,
        diagnostics,
    }
}

// ---------------------------------------------------------------------------
// Line scanning
// ---------------------------------------------------------------------------

/// What a line is doing with respect to multi-line escape blocks.
///
/// A line whose entire content is a run of backslashes delimits a block whose
/// contents are literal. The delimiters are syntax and are consumed; a run
/// embedded in a line of text keeps the inline meaning instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EscapeRole {
    None,
    /// The opening line, carrying the length of the backslash run.
    Open(usize),
    /// Literal content.
    Body,
    /// The closing line.
    Close,
}

#[derive(Debug, Clone)]
struct Line {
    indent: usize,
    /// Offset of the first non-whitespace character.
    content_start: usize,
    /// Offset just past the last character, excluding the line terminator.
    end: usize,
    /// Offset of the start of the line, including indentation.
    start: usize,
    /// Source text with indentation and any trailing comment removed.
    code: String,
    /// Span of `code` within the file.
    code_span: Span,
    blank: bool,
    escape: EscapeRole,
}

impl Line {
    fn span(&self) -> Span {
        Span::new(self.start, self.end)
    }
}

struct ScannedLines {
    lines: Vec<Line>,
    diagnostics: Vec<Diagnostic>,
}

fn scan_lines(source: &str, path: &Path) -> ScannedLines {
    let mut lines = Vec::new();
    let mut diagnostics = Vec::new();

    let mut offset = 0usize;
    let mut raw: Vec<(usize, &str)> = Vec::new();
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        raw.push((offset, trimmed));
        offset += line.len();
    }
    if source.is_empty() {
        raw.push((0, ""));
    }

    // Pass one: find multi-line escape blocks. One opens and closes on a line
    // whose only content is a backslash run; its contents are literal, and the
    // delimiters are consumed rather than emitted.
    //
    // Code fences are not special. To Piton they're just text, so anything
    // inside them still gets parsed; an example puts an escape block inside
    // its fence to keep it literal.
    let mut escape_roles = vec![EscapeRole::None; raw.len()];
    let mut escape: Option<(usize, usize)> = None; // (line, run length)

    for (index, (_, text)) in raw.iter().enumerate() {
        let run = backslash_run(text.trim());
        if let Some((_, opener_run)) = escape {
            if run == Some(opener_run) {
                escape_roles[index] = EscapeRole::Close;
                escape = None;
            } else {
                escape_roles[index] = EscapeRole::Body;
            }
            continue;
        }
        if let Some(length) = run {
            escape_roles[index] = EscapeRole::Open(length);
            escape = Some((index, length));
        }
    }
    if let Some((start, length)) = escape {
        diagnostics.push(Diagnostic::warning(
            "unterminated-escape-block",
            format!(
                "multi-line escape block is never closed; it needs a line of {length} backslashes"
            ),
            path,
            Span::new(raw[start].0, raw[start].0 + raw[start].1.len()),
        ));
    }

    // Pass two: build line records, and check that indentation is consistent.
    let mut indent_char: Option<char> = None;
    let mut mixed_reported = false;
    for (index, (start, text)) in raw.iter().copied().enumerate() {
        let indent_text: String = text.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        let indent = indent_text.chars().count();
        let content_start = start + indent_text.len();
        let body = &text[indent_text.len()..];
        let escape = escape_roles[index];
        let verbatim = escape != EscapeRole::None;

        if !verbatim && !body.trim().is_empty() {
            if let Some(first) = indent_text.chars().next() {
                match indent_char {
                    None => indent_char = Some(first),
                    Some(expected) => {
                        if indent_text.chars().any(|c| c != expected) && !mixed_reported {
                            mixed_reported = true;
                            diagnostics.push(Diagnostic::error(
                                "inconsistent-indentation",
                                "indentation mixes tabs and spaces; a file must use one or the other",
                                path,
                                Span::new(start, content_start),
                            ));
                        }
                    }
                }
            }
        }

        // Comments are lexical, so they are removed before the line is
        // classified. The green tree recovers them from the gaps between
        // significant tokens, which is how the CST stays lossless. Verbatim
        // content keeps everything, including what looks like a comment.
        let code_text = if verbatim {
            body
        } else {
            prose::split_comment(body).0
        };

        let code_trimmed = code_text.trim_end();
        lines.push(Line {
            indent,
            content_start,
            start,
            end: start + text.len(),
            code: code_trimmed.to_string(),
            code_span: Span::new(content_start, content_start + code_trimmed.len()),
            blank: code_trimmed.trim().is_empty() && !verbatim,
            escape,
        });
    }

    ScannedLines { lines, diagnostics }
}

// ---------------------------------------------------------------------------
// Events for the lossless tree
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Event {
    Enter(SyntaxKind, usize),
    Leave(usize),
    Token(SyntaxKind, Span),
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

struct Parser<'a> {
    source: &'a str,
    file: PathBuf,
    lines: Vec<Line>,
    pos: usize,
    events: Vec<Event>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    fn eof(&self) -> bool {
        self.pos >= self.lines.len()
    }

    fn peek(&self) -> Option<&Line> {
        self.lines.get(self.pos)
    }

    fn advance(&mut self) -> Option<Line> {
        let line = self.lines.get(self.pos).cloned();
        self.pos += 1;
        line
    }

    fn error(&mut self, code: &str, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(code, message, &self.file, span));
    }

    fn enter(&mut self, kind: SyntaxKind, at: usize) {
        self.events.push(Event::Enter(kind, at));
    }

    fn leave(&mut self, at: usize) {
        self.events.push(Event::Leave(at));
    }

    fn token(&mut self, kind: SyntaxKind, span: Span) {
        if !span.is_empty() {
            self.events.push(Event::Token(kind, span));
        }
    }

    /// True when a deeper non-blank line still belongs to the current block.
    fn more_below(&self, parent_indent: usize) -> bool {
        self.lines[self.pos..]
            .iter()
            .find(|line| !line.blank)
            .is_some_and(|line| line.indent > parent_indent)
    }

    // -- file -----------------------------------------------------------

    fn parse_file(&mut self) -> SourceFile {
        let end = self.source.len();
        self.enter(SyntaxKind::SOURCE_FILE, 0);
        let mut items = Vec::new();
        while !self.eof() {
            let line = self.peek().expect("not at end").clone();
            if line.blank {
                self.pos += 1;
                continue;
            }
            if line.indent > 0 {
                self.error(
                    "unexpected-indentation",
                    "declaration is indented but has no parent",
                    line.span(),
                );
                self.pos += 1;
                continue;
            }
            match self.parse_item() {
                Some(item) => items.push(item),
                None => {}
            }
        }
        self.leave(end);
        SourceFile {
            span: Span::new(0, end),
            items,
        }
    }

    fn parse_item(&mut self) -> Option<Item> {
        let line = self.peek()?.clone();
        let code = line.code.trim_start().to_string();

        if code.starts_with("use ") || code == "use" {
            return self.parse_use(&line);
        }
        if code.starts_with("from ") {
            return self.parse_from(&line);
        }

        let (exported, rest_offset) = if code == "export" || code.starts_with("export ") {
            (true, leading_len(&line.code) + "export".len())
        } else {
            (false, leading_len(&line.code))
        };
        let rest = line.code[rest_offset..].trim_start();
        let rest_start = line.content_start + rest_offset
            + (line.code[rest_offset..].len() - rest.len());

        // A property-shaped line at the top level is a variable declaration.
        if let Some(header) = parse_property_header(rest, rest_start) {
            return Some(Item::Variable(self.parse_variable(&line, exported, header)));
        }
        if let Some(header) = parse_declaration_header(rest, rest_start) {
            return Some(Item::Anchor(self.parse_anchor(&line, exported, header)));
        }
        if exported && is_identifier(rest) {
            self.pos += 1;
            self.enter(SyntaxKind::REEXPORT_DECL, line.start);
            self.token(
                SyntaxKind::KW_EXPORT,
                Span::new(line.content_start, line.content_start + "export".len()),
            );
            self.token(SyntaxKind::IDENT, Span::new(rest_start, rest_start + rest.len()));
            self.leave(line.end);
            return Some(Item::ReExport(ReExportDecl {
                span: line.span(),
                name: rest.to_string(),
                name_span: Span::new(rest_start, rest_start + rest.len()),
            }));
        }

        if misspaced_constraint(rest) {
            self.error(
                "constraint-spacing",
                "a type constraint needs a space after `::` and after `:`, like `name:: number: 42`",
                line.span(),
            );
        } else {
            self.error(
                "invalid-declaration",
                "expected a declaration, an import, or a variable",
                line.span(),
            );
        }
        self.pos += 1;
        // Consume any indented body so the error does not cascade.
        while self.more_below(line.indent) {
            self.pos += 1;
        }
        None
    }

    fn parse_use(&mut self, line: &Line) -> Option<Item> {
        self.pos += 1;
        let code = line.code.trim_start();
        let path_text = code["use".len()..].trim();
        let path_start = line.content_start
            + leading_len(&line.code)
            + "use".len()
            + (code["use".len()..].len() - code["use".len()..].trim_start().len());
        self.enter(SyntaxKind::USE_DECL, line.start);
        self.token(
            SyntaxKind::KW_USE,
            Span::new(line.content_start, line.content_start + "use".len()),
        );
        let path_span = Span::new(path_start, path_start + path_text.len());
        self.token(SyntaxKind::PATH, path_span);
        self.leave(line.end);

        if path_text.is_empty() {
            self.error("missing-module-path", "`use` needs a module path", line.span());
            return None;
        }
        Some(Item::Use(UseDecl {
            span: line.span(),
            path: ModulePath {
                span: path_span,
                text: path_text.to_string(),
            },
        }))
    }

    /// Parses `from <path> import ...` / `from <path> export ...`, joining any
    /// indented continuation lines.
    fn parse_from(&mut self, line: &Line) -> Option<Item> {
        let start_line = line.clone();
        self.pos += 1;
        let mut joined = start_line.code.trim_start().to_string();
        let mut segments = vec![(
            start_line.content_start + leading_len(&start_line.code),
            start_line.code.trim_start().to_string(),
        )];
        let mut end = start_line.end;
        while let Some(next) = self.peek() {
            if next.blank || next.indent == 0 {
                break;
            }
            let next = self.advance().expect("peeked");
            joined.push(' ');
            joined.push_str(next.code.trim());
            segments.push((next.code_span.start, next.code.trim().to_string()));
            end = next.end;
        }

        self.enter(SyntaxKind::FROM_DECL, start_line.start);
        self.token(
            SyntaxKind::KW_FROM,
            Span::new(start_line.content_start, start_line.content_start + "from".len()),
        );

        let span = Span::new(start_line.start, end);
        let rest = joined["from".len()..].trim_start();
        let Some((path_text, after_path)) = rest.split_once(char::is_whitespace) else {
            self.leave(end);
            self.error(
                "invalid-import",
                "expected `import` or `export` after the module path",
                span,
            );
            return None;
        };
        let path_start = locate(&segments, path_text).unwrap_or(start_line.content_start);
        let path_span = Span::new(path_start, path_start + path_text.len());
        self.token(SyntaxKind::PATH, path_span);

        let after_path = after_path.trim_start();
        let (kind, list_text) = if let Some(rest) = after_path.strip_prefix("import") {
            (FromKind::Import, rest)
        } else if let Some(rest) = after_path.strip_prefix("export") {
            (FromKind::Export, rest)
        } else {
            self.leave(end);
            self.error(
                "invalid-import",
                "expected `import` or `export` after the module path",
                span,
            );
            return None;
        };
        self.token(
            match kind {
                FromKind::Import => SyntaxKind::KW_IMPORT,
                FromKind::Export => SyntaxKind::KW_EXPORT,
            },
            Span::empty(path_span.end),
        );

        let list_text = list_text.trim();
        let mut star = false;
        let mut items = Vec::new();
        if list_text == "*" {
            star = true;
            if kind == FromKind::Import {
                self.error(
                    "invalid-import",
                    "`import *` is not supported; use `from ./module export *` in an index file",
                    span,
                );
            }
        } else if list_text.is_empty() {
            self.error("invalid-import", "import list is empty", span);
        } else {
            self.enter(SyntaxKind::IMPORT_LIST, path_span.end);
            for entry in list_text.split(',') {
                let entry = entry.trim();
                if entry.is_empty() {
                    continue;
                }
                let mut words = entry.split_whitespace();
                let name = words.next().unwrap_or_default().to_string();
                let alias = words.next().map(|s| s.to_string());
                if words.next().is_some() {
                    self.error(
                        "invalid-import",
                        format!("`{entry}` has more than a name and an alias"),
                        span,
                    );
                }
                let name_start = locate(&segments, &name).unwrap_or(path_span.end);
                let name_span = Span::new(name_start, name_start + name.len());
                self.enter(SyntaxKind::IMPORT_ITEM, name_span.start);
                self.token(SyntaxKind::IDENT, name_span);
                self.leave(name_span.end);
                let alias = alias.map(|alias| {
                    let at = locate(&segments, &alias).unwrap_or(name_span.end);
                    Spanned::new(Span::new(at, at + alias.len()), alias)
                });
                items.push(ImportItem {
                    span: name_span,
                    name,
                    name_span,
                    alias,
                });
            }
            self.leave(end);
        }
        self.leave(end);

        Some(Item::From(FromDecl {
            span,
            path: ModulePath {
                span: path_span,
                text: path_text.to_string(),
            },
            kind,
            star,
            items,
        }))
    }

    fn parse_variable(
        &mut self,
        line: &Line,
        exported: bool,
        header: PropertyHeader,
    ) -> VariableDecl {
        self.enter(SyntaxKind::VARIABLE_DECL, line.start);
        if exported {
            self.token(
                SyntaxKind::KW_EXPORT,
                Span::new(line.content_start, line.content_start + "export".len()),
            );
        }
        let decl = self.finish_property(line, header, SyntaxKind::VARIABLE_DECL);
        self.leave(decl.span.end);
        VariableDecl {
            span: Span::new(line.start, decl.span.end),
            exported,
            name: decl.name,
            name_span: decl.name_span,
            constraints: decl.constraints,
            value: decl.value,
        }
    }

    fn parse_anchor(
        &mut self,
        line: &Line,
        exported: bool,
        header: DeclarationHeader,
    ) -> AnchorDecl {
        self.pos += 1;
        self.enter(SyntaxKind::ANCHOR_DECL, line.start);
        if exported {
            self.token(
                SyntaxKind::KW_EXPORT,
                Span::new(line.content_start, line.content_start + "export".len()),
            );
        }
        self.enter(SyntaxKind::ANCHOR_HEADER, header.keyword_span.start);
        if let Some(span) = header.abstract_span {
            self.token(SyntaxKind::KW_ABSTRACT, span);
        }
        self.token(
            if header.keyword == "anchor" {
                SyntaxKind::KW_ANCHOR
            } else {
                SyntaxKind::IDENT
            },
            header.keyword_span,
        );
        self.token(SyntaxKind::IDENT, header.name_span);
        if let Some(alias) = &header.alias {
            self.enter(SyntaxKind::KEYWORD_ALIAS, alias.span.start);
            self.token(SyntaxKind::IDENT, alias.span);
            self.leave(alias.span.end);
        }
        if !header.extends.is_empty() {
            let start = header.extends[0].span.start;
            let end = header.extends.last().expect("non-empty").span.end;
            self.enter(SyntaxKind::EXTENDS_CLAUSE, start);
            for base in &header.extends {
                self.token(SyntaxKind::IDENT, base.span);
            }
            self.leave(end);
        }
        self.token(SyntaxKind::COLON, Span::empty(line.code_span.end));
        self.leave(line.end);

        let body = self.parse_block(line.indent);
        let end = body.span.end.max(line.end);
        self.leave(end);

        if body.items.is_empty() {
            self.diagnostics.push(
                Diagnostic::error(
                    "empty-anchor",
                    format!("`{}` has nothing indented under it", header.name),
                    &self.file,
                    header.name_span,
                )
                .with_help("an anchor always needs something indented under it; write `pass` if it has no properties of its own"),
            );
        }

        if let Some(alias) = &header.alias {
            if piton_core::names::is_reserved(&alias.value) {
                self.diagnostics.push(Diagnostic::error(
                    "reserved-keyword",
                    format!("`{}` is a reserved word, so it can't be used as a keyword", alias.value),
                    &self.file,
                    alias.span,
                ));
            } else if !piton_core::names::is_valid_keyword(&alias.value) {
                self.diagnostics.push(Diagnostic::error(
                    "invalid-keyword",
                    format!(
                        "`{}` is not a valid keyword; keywords are lowercase and may be kebab-case",
                        alias.value
                    ),
                    &self.file,
                    alias.span,
                ));
            }
        }

        AnchorDecl {
            span: Span::new(line.start, end),
            exported,
            is_abstract: header.abstract_span.is_some(),
            keyword: header.keyword,
            keyword_span: header.keyword_span,
            name: header.name,
            name_span: header.name_span,
            alias: header.alias,
            extends: header.extends,
            body,
        }
    }

    // -- blocks ---------------------------------------------------------

    /// Parses every line indented deeper than `parent_indent`.
    fn parse_block(&mut self, parent_indent: usize) -> Block {
        let start = self.peek().map(|l| l.start).unwrap_or(self.source.len());
        let mut items: Vec<BlockItem> = Vec::new();
        let mut paragraph: Vec<ProseLine> = Vec::new();
        let mut paragraph_span: Option<Span> = None;
        let mut end = start;
        let mut entered = false;

        macro_rules! flush {
            () => {
                if !paragraph.is_empty() {
                    let span = paragraph_span.take().unwrap_or_default();
                    items.push(BlockItem::Prose(Paragraph {
                        span,
                        lines: std::mem::take(&mut paragraph),
                    }));
                }
            };
        }

        loop {
            let Some(line) = self.peek().cloned() else { break };
            if line.blank {
                if !self.more_below(parent_indent) {
                    break;
                }
                // A blank line is how the language spells an explicit break.
                flush!();
                self.pos += 1;
                continue;
            }
            if line.indent <= parent_indent {
                break;
            }
            if !entered {
                entered = true;
                self.enter(SyntaxKind::BLOCK, line.start);
            }

            if let EscapeRole::Open(run) = line.escape {
                flush!();
                let block = self.parse_escape_block(&line, run);
                end = block.span.end;
                items.push(BlockItem::Escape(block));
                continue;
            }

            let code = line.code.trim_start();
            if code == "pass" {
                flush!();
                self.pos += 1;
                self.token(SyntaxKind::KW_PASS, line.code_span);
                items.push(BlockItem::Pass(line.span()));
                end = line.end;
                continue;
            }

            if let Some(rest_offset) = list_marker(code) {
                flush!();
                let item = self.parse_list_item(&line, rest_offset);
                end = item.span.end;
                items.push(BlockItem::ListItem(item));
                continue;
            }

            if let Some((op, rest_offset)) = merge_marker(code) {
                flush!();
                let item = self.parse_merge_item(&line, op, rest_offset);
                end = item.span.end;
                items.push(BlockItem::Merge(item));
                continue;
            }

            let code_start = line.content_start + leading_len(&line.code);
            if let Some(header) = parse_property_header(code, code_start) {
                flush!();
                if let Some(first) = items.iter().find_map(|item| match item {
                    BlockItem::Property(existing) if existing.name == header.name => Some(existing.name_span),
                    _ => None,
                }) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "duplicate-key",
                            format!("`{}` is already defined in this block", header.name),
                            &self.file,
                            header.name_span,
                        )
                        .with_label(piton_core::Label::new(
                            self.file.clone(),
                            first,
                            "first defined here".to_string(),
                        )),
                    );
                }
                self.enter(SyntaxKind::PROPERTY, line.start);
                let property = self.finish_property(&line, header, SyntaxKind::PROPERTY);
                self.leave(property.span.end);
                end = property.span.end;
                items.push(BlockItem::Property(property));
                continue;
            }

            if misspaced_constraint(code) {
                self.error(
                    "constraint-spacing",
                    "a type constraint needs a space after `::` and after `:`, like `name:: number: 42`",
                    line.code_span,
                );
            }

            // Anything else is prose.
            self.pos += 1;
            let (prose_line, mut diagnostics) =
                prose::scan_line(&line.code[leading_len(&line.code)..], code_start, &self.file);
            self.diagnostics.append(&mut diagnostics);
            self.enter(SyntaxKind::PROSE_LINE, line.code_span.start);
            self.emit_prose_tokens(&prose_line);
            self.leave(line.code_span.end);
            paragraph_span = Some(match paragraph_span {
                Some(existing) => existing.cover(prose_line.span),
                None => prose_line.span,
            });
            paragraph.push(prose_line);
            end = line.end;
        }
        flush!();
        if entered {
            self.leave(end);
        }

        Block {
            span: Span::new(start.min(end), end),
            items,
        }
    }

    /// Parses a multi-line escape block, consuming its delimiters.
    fn parse_escape_block(&mut self, open: &Line, run: usize) -> EscapeBlock {
        let start = open.start;
        let base_indent = open.indent;
        self.enter(SyntaxKind::ESCAPE_BLOCK, start);
        self.token(SyntaxKind::ESCAPE_MARK, open.code_span);
        self.pos += 1;

        let mut lines = Vec::new();
        let mut end = open.end;
        while let Some(line) = self.peek().cloned() {
            match line.escape {
                EscapeRole::Body => {
                    self.pos += 1;
                    let text = &self.source[line.start..line.end];
                    lines.push(strip_indent(text, base_indent));
                    self.token(SyntaxKind::ESCAPE_TEXT, Span::new(line.start, line.end));
                    end = line.end;
                }
                EscapeRole::Close => {
                    self.pos += 1;
                    self.token(SyntaxKind::ESCAPE_MARK, line.code_span);
                    end = line.end;
                    break;
                }
                _ => break,
            }
        }
        self.leave(end);
        EscapeBlock {
            span: Span::new(start, end),
            lines,
            run,
        }
    }

    fn parse_list_item(&mut self, line: &Line, rest_offset: usize) -> ListItem {
        self.pos += 1;
        let leading = leading_len(&line.code);
        let marker_start = line.content_start + leading;
        self.enter(SyntaxKind::LIST_ITEM, line.start);
        self.token(
            SyntaxKind::DASH,
            Span::new(marker_start, marker_start + 1),
        );
        let inline_text = &line.code[leading + rest_offset..];
        let inline_start = marker_start + rest_offset;
        let value = self.parse_value(line, inline_text, inline_start);
        let end = value.span.end.max(line.end);
        self.leave(end);
        ListItem {
            span: Span::new(line.start, end),
            value,
        }
    }

    fn parse_merge_item(&mut self, line: &Line, op: MergeOp, rest_offset: usize) -> MergeItem {
        self.pos += 1;
        let leading = leading_len(&line.code);
        let marker_start = line.content_start + leading;
        self.enter(SyntaxKind::MERGE_ITEM, line.start);
        self.token(
            match op {
                MergeOp::Merge => SyntaxKind::PLUS,
                MergeOp::Concat => SyntaxKind::PLUS_PLUS,
            },
            Span::new(marker_start, marker_start + rest_offset.min(2)),
        );
        let text = line.code[leading + rest_offset..].trim();
        let text_start = marker_start + rest_offset;
        let (prose_line, mut diagnostics) = prose::scan_line(text, text_start, &self.file);
        self.diagnostics.append(&mut diagnostics);
        self.emit_prose_tokens(&prose_line);
        self.leave(line.end);
        MergeItem {
            span: line.span(),
            op,
            value: prose_line,
        }
    }

    /// Completes a property or variable once its header has been recognized.
    fn finish_property(
        &mut self,
        line: &Line,
        header: PropertyHeader,
        _kind: SyntaxKind,
    ) -> Property {
        self.pos += 1;
        self.token(SyntaxKind::IDENT, header.name_span);
        for constraint in &header.constraints {
            self.enter(SyntaxKind::TYPE_CONSTRAINT, constraint.span.start);
            self.token(SyntaxKind::COLON_COLON, Span::empty(constraint.span.start));
            self.token(SyntaxKind::IDENT, constraint.span);
            self.leave(constraint.span.end);
        }

        let value = if header.has_value_colon {
            self.token(SyntaxKind::COLON, Span::empty(header.value_start));
            // `value_offset` is relative to the header text, which may start
            // partway into the line (after `export`), so index from the line's
            // own start instead.
            let offset = header.value_start - line.content_start;
            let inline = &line.code[offset..];
            self.parse_value(line, inline, header.value_start)
        } else {
            // A constraint-only declaration still takes an indented block when
            // one follows, which is how `supportedOperators:: Operator[]` gets
            // its list.
            let block = if self.more_below(line.indent) {
                Some(self.parse_block(line.indent))
            } else {
                None
            };
            let end = block.as_ref().map_or(line.end, |b| b.span.end);
            let declared = block.is_some();
            ValueNode {
                span: Span::new(line.code_span.end, end),
                inline: None,
                inline_list: None,
                block,
                declared,
            }
        };

        let end = value.span.end.max(line.end);
        Property {
            span: Span::new(line.start, end),
            name: header.name,
            name_span: header.name_span,
            constraints: header.constraints,
            value,
        }
    }

    /// Parses a value: whatever follows on the same line, plus any indented
    /// block beneath it.
    fn parse_value(&mut self, line: &Line, inline_text: &str, inline_start: usize) -> ValueNode {
        self.enter(SyntaxKind::VALUE, inline_start);
        let trimmed_start = inline_start + (inline_text.len() - inline_text.trim_start().len());
        let trimmed = inline_text.trim();

        let mut inline = None;
        let mut inline_list = None;

        if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() >= 2 {
            let items = self.parse_inline_list(trimmed, trimmed_start);
            inline_list = Some(items);
        } else if !trimmed.is_empty() {
            let (prose_line, mut diagnostics) =
                prose::scan_line(trimmed, trimmed_start, &self.file);
            self.diagnostics.append(&mut diagnostics);
            self.enter(SyntaxKind::PROSE_LINE, prose_line.span.start);
            self.emit_prose_tokens(&prose_line);
            self.leave(prose_line.span.end);
            inline = Some(prose_line);
        }

        let block = if self.more_below(line.indent) {
            Some(self.parse_block(line.indent))
        } else {
            None
        };

        let end = block
            .as_ref()
            .map_or(line.end, |b| b.span.end.max(line.end));
        self.leave(end);
        ValueNode {
            span: Span::new(inline_start, end),
            inline,
            inline_list,
            block,
            declared: true,
        }
    }

    /// Parses `[a, b, [c, d]]` written at a value position. Items are values,
    /// not expressions, so bare words stay prose.
    fn parse_inline_list(&mut self, text: &str, start: usize) -> Vec<ValueNode> {
        self.enter(SyntaxKind::INLINE_LIST, start);
        self.token(SyntaxKind::L_BRACKET, Span::new(start, start + 1));
        let inner = &text[1..text.len() - 1];
        let inner_start = start + 1;
        let mut items = Vec::new();
        for (piece, offset) in split_top_level(inner) {
            let piece_trimmed = piece.trim();
            if piece_trimmed.is_empty() {
                continue;
            }
            let at = inner_start
                + offset
                + (piece.len() - piece.trim_start().len());
            if piece_trimmed.starts_with('[') && piece_trimmed.ends_with(']') {
                let nested = self.parse_inline_list(piece_trimmed, at);
                items.push(ValueNode {
                    span: Span::new(at, at + piece_trimmed.len()),
                    inline: None,
                    inline_list: Some(nested),
                    block: None,
                    declared: true,
                });
            } else {
                let (prose_line, mut diagnostics) =
                    prose::scan_line(piece_trimmed, at, &self.file);
                self.diagnostics.append(&mut diagnostics);
                self.enter(SyntaxKind::PROSE_LINE, prose_line.span.start);
                self.emit_prose_tokens(&prose_line);
                self.leave(prose_line.span.end);
                items.push(ValueNode {
                    span: prose_line.span,
                    inline: Some(prose_line),
                    inline_list: None,
                    block: None,
                    declared: true,
                });
            }
        }
        let end = start + text.len();
        self.token(SyntaxKind::R_BRACKET, Span::new(end - 1, end));
        self.leave(end);
        items
    }

    fn emit_prose_tokens(&mut self, line: &ProseLine) {
        let mut cursor = line.span.start;
        for segment in &line.segments {
            match segment {
                ProseSegment::Text(_) | ProseSegment::Literal(_) => {}
                ProseSegment::Interpolation(interp) => {
                    if interp.span.start > cursor {
                        self.token(SyntaxKind::PROSE, Span::new(cursor, interp.span.start));
                    }
                    self.enter(SyntaxKind::INTERPOLATION, interp.span.start);
                    self.token(SyntaxKind::PROSE, interp.span);
                    self.leave(interp.span.end);
                    cursor = interp.span.end;
                }
            }
        }
        if cursor < line.span.end {
            self.token(SyntaxKind::PROSE, Span::new(cursor, line.span.end));
        }
    }
}

// ---------------------------------------------------------------------------
// Header recognition
// ---------------------------------------------------------------------------

struct PropertyHeader {
    name: String,
    name_span: Span,
    constraints: Vec<TypeConstraint>,
    has_value_colon: bool,
    /// Absolute offset of the value text.
    value_start: usize,
}

struct DeclarationHeader {
    abstract_span: Option<Span>,
    keyword: String,
    keyword_span: Span,
    name: String,
    name_span: Span,
    alias: Option<Spanned<String>>,
    extends: Vec<Spanned<String>>,
}

/// Recognizes `name`, `name:: type`, `name: value`, and combinations.
///
/// `text` must already have its indentation removed; `start` is its absolute
/// offset.
///
/// A key is anything that coerces to a string and contains no spaces, so
/// `thisIsAKey`, `123`, `foo-bar`, `false`, and `null` are all keys. A word that
/// is a keyword elsewhere is still a key here: what makes `anchor MyAnchor:` a
/// declaration and `anchor: a description` a property is the colon, not the
/// word. Only a leading `-` or `+` is excluded, because those open a list item
/// and a merge.
fn parse_property_header(text: &str, start: usize) -> Option<PropertyHeader> {
    let name_len = text
        .char_indices()
        .take_while(|(i, c)| {
            c.is_alphanumeric() || *c == '_' || (*c == '-' && *i > 0)
        })
        .map(|(i, c)| i + c.len_utf8())
        .last()
        .unwrap_or(0);
    if name_len == 0 {
        return None;
    }
    let name = &text[..name_len];

    let mut cursor = name_len;
    let mut constraints = Vec::new();
    loop {
        let rest = &text[cursor..];
        if let Some(after) = rest.strip_prefix("::") {
            if !after.starts_with(' ') && !after.is_empty() {
                // `::` must be followed by a space to be a constraint.
                return None;
            }
            let type_text_start = cursor + 2;
            let type_len = constraint_length(&text[type_text_start..]);
            let raw = text[type_text_start..type_text_start + type_len].trim();
            if raw.is_empty() {
                return None;
            }
            let offset = type_text_start
                + (text[type_text_start..type_text_start + type_len].len()
                    - text[type_text_start..type_text_start + type_len]
                        .trim_start()
                        .len());
            constraints.push(parse_constraint(raw, start + offset));
            cursor = type_text_start + type_len;
            continue;
        }
        break;
    }

    let rest = &text[cursor..];
    if let Some(after) = rest.strip_prefix(':') {
        if !after.is_empty() && !after.starts_with(' ') {
            // `key:value` is not a property; the space is part of the syntax.
            return None;
        }
        return Some(PropertyHeader {
            name: name.to_string(),
            name_span: Span::new(start, start + name_len),
            constraints,
            has_value_colon: true,
            value_start: start + cursor + 1,
        });
    }

    if constraints.is_empty() || !rest.trim().is_empty() {
        return None;
    }
    Some(PropertyHeader {
        name: name.to_string(),
        name_span: Span::new(start, start + name_len),
        constraints,
        has_value_colon: false,
        value_start: start + cursor,
    })
}

/// True for a line that starts like a typed property but gets the spacing
/// wrong, such as `myVariable::number:42`. The whitespace after both `::` and
/// `:` is part of the syntax.
fn misspaced_constraint(text: &str) -> bool {
    let name_len = text
        .char_indices()
        .take_while(|(i, c)| c.is_alphanumeric() || *c == '_' || (*c == '-' && *i > 0))
        .map(|(i, c)| i + c.len_utf8())
        .last()
        .unwrap_or(0);
    name_len > 0
        && text[name_len..].starts_with("::")
        && parse_property_header(text, 0).is_none()
}

/// Length of a type constraint starting at `text`, which ends at the next `::`
/// or at a `:` that is followed by a space or the end of the line.
fn constraint_length(text: &str) -> usize {
    let bytes: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < bytes.len() {
        let (offset, ch) = bytes[i];
        if ch == ':' {
            if bytes.get(i + 1).map(|(_, c)| *c) == Some(':') {
                return offset;
            }
            let next = bytes.get(i + 1).map(|(_, c)| *c);
            if next.is_none() || next == Some(' ') {
                return offset;
            }
        }
        i += 1;
    }
    text.len()
}

fn parse_constraint(raw: &str, start: usize) -> TypeConstraint {
    let span = Span::new(start, start + raw.len());
    let (extends, body) = match raw.strip_prefix("extends ") {
        Some(rest) => (true, rest.trim()),
        None => (false, raw),
    };
    let (list, body) = match body.strip_suffix("[]") {
        Some(rest) => (true, rest.trim()),
        None => (false, body),
    };
    TypeConstraint {
        span,
        extends,
        name: TypeName::from_ident(body),
        list,
    }
}

/// Recognizes `[abstract] <keyword> <Name> [as <kw>] [extends A, B]:`.
fn parse_declaration_header(text: &str, start: usize) -> Option<DeclarationHeader> {
    let body = text.strip_suffix(':')?;
    let mut cursor = 0usize;
    let mut abstract_span = None;

    let take_word = |cursor: &mut usize| -> Option<(String, Span)> {
        let rest = &body[*cursor..];
        let lead = rest.len() - rest.trim_start().len();
        let rest = rest.trim_start();
        if rest.is_empty() {
            return None;
        }
        let len = rest
            .char_indices()
            .take_while(|(_, c)| !c.is_whitespace() && *c != ',')
            .map(|(i, c)| i + c.len_utf8())
            .last()
            .unwrap_or(0);
        let at = *cursor + lead;
        *cursor = at + len;
        Some((rest[..len].to_string(), Span::new(start + at, start + at + len)))
    };

    let (first, first_span) = take_word(&mut cursor)?;
    let (keyword, keyword_span) = if first == "abstract" {
        abstract_span = Some(first_span);
        take_word(&mut cursor)?
    } else {
        (first, first_span)
    };
    if !piton_core::names::is_valid_keyword(&keyword) {
        return None;
    }
    let (name, name_span) = take_word(&mut cursor)?;
    if !name.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') {
        return None;
    }

    let mut alias = None;
    let mut extends = Vec::new();
    while let Some((word, span)) = take_word(&mut cursor) {
        match word.as_str() {
            "as" => {
                let (value, value_span) = take_word(&mut cursor)?;
                alias = Some(Spanned::new(value_span, value));
            }
            "extends" => loop {
                let Some((base, base_span)) = take_word(&mut cursor) else {
                    break;
                };
                let trimmed = base.trim_end_matches(',');
                extends.push(Spanned::new(
                    Span::new(base_span.start, base_span.start + trimmed.len()),
                    trimmed.to_string(),
                ));
                // Commas separate bases and may be attached to either side.
                let rest = body[cursor..].trim_start();
                if !rest.starts_with(',') {
                    if base.ends_with(',') {
                        continue;
                    }
                    break;
                }
                cursor = body[..cursor].len() + (body[cursor..].len() - rest.len()) + 1;
            },
            _ => {
                let _ = span;
                return None;
            }
        }
    }

    Some(DeclarationHeader {
        abstract_span,
        keyword,
        keyword_span,
        name,
        name_span,
        alias,
        extends,
    })
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// The length of a backslash run when `text` is nothing but backslashes.
fn backslash_run(text: &str) -> Option<usize> {
    if text.is_empty() || !text.chars().all(|c| c == '\\') {
        return None;
    }
    Some(text.chars().count())
}

fn leading_len(text: &str) -> usize {
    text.len() - text.trim_start().len()
}

fn is_identifier(text: &str) -> bool {
    !text.is_empty()
        && text.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// Returns the offset of the item text after a `- ` marker.
fn list_marker(code: &str) -> Option<usize> {
    if code == "-" {
        return Some(1);
    }
    if let Some(rest) = code.strip_prefix('-') {
        if rest.starts_with(' ') {
            return Some(1);
        }
    }
    None
}

/// Returns the merge operator and the offset of its operand.
fn merge_marker(code: &str) -> Option<(MergeOp, usize)> {
    if let Some(rest) = code.strip_prefix("++") {
        if rest.is_empty() || rest.starts_with(' ') {
            return Some((MergeOp::Concat, 2));
        }
        return None;
    }
    if let Some(rest) = code.strip_prefix('+') {
        if rest.is_empty() || rest.starts_with(' ') {
            return Some((MergeOp::Merge, 1));
        }
    }
    None
}

/// Splits on commas that are not inside brackets, braces, or quotes.
fn split_top_level(text: &str) -> Vec<(&str, usize)> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut quoted = false;
    let mut start = 0usize;
    for (offset, ch) in text.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            '[' | '{' | '(' if !quoted => depth += 1,
            ']' | '}' | ')' if !quoted => depth -= 1,
            ',' if !quoted && depth == 0 => {
                parts.push((&text[start..offset], start));
                start = offset + 1;
            }
            _ => {}
        }
    }
    parts.push((&text[start..], start));
    parts
}

/// Removes up to `indent` leading whitespace characters.
fn strip_indent(text: &str, indent: usize) -> String {
    let mut removed = 0;
    let mut chars = text.char_indices();
    let mut offset = 0;
    while removed < indent {
        match chars.next() {
            Some((i, c)) if c == ' ' || c == '\t' => {
                removed += 1;
                offset = i + c.len_utf8();
            }
            _ => break,
        }
    }
    text[offset..].trim_end_matches(['\n', '\r']).to_string()
}

/// Finds `needle` as a whole word in the joined declaration segments, returning
/// its absolute offset.
fn locate(segments: &[(usize, String)], needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    for (base, text) in segments {
        let mut from = 0;
        while let Some(found) = text[from..].find(needle) {
            let at = from + found;
            let before_ok = at == 0
                || !text[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_');
            let after = at + needle.len();
            let after_ok = after >= text.len()
                || !text[after..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_');
            if before_ok && after_ok {
                return Some(base + at);
            }
            from = at + needle.len();
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Green tree construction
// ---------------------------------------------------------------------------

/// Replays the parser's events into a rowan green tree, filling every gap with
/// trivia so the tree reproduces the source byte for byte.
fn build_green(source: &str, events: &[Event], diagnostics: &mut Vec<Diagnostic>) -> GreenNode {
    let mut builder = GreenNodeBuilder::new();
    let mut cursor = 0usize;
    let mut depth = 0usize;

    let emit_trivia = |builder: &mut GreenNodeBuilder<'static>, cursor: &mut usize, to: usize| {
        if to <= *cursor {
            return;
        }
        let text = &source[*cursor..to];
        for piece in split_trivia(text) {
            let kind = match piece.chars().next() {
                Some('\n') | Some('\r') => SyntaxKind::NEWLINE,
                Some('/') => SyntaxKind::COMMENT,
                _ if piece.trim().is_empty() => SyntaxKind::WHITESPACE,
                _ => SyntaxKind::PROSE,
            };
            builder.token(kind.into(), piece);
        }
        *cursor = to;
    };

    for event in events {
        match event {
            Event::Enter(kind, at) => {
                if depth > 0 {
                    emit_trivia(&mut builder, &mut cursor, (*at).min(source.len()));
                }
                builder.start_node((*kind).into());
                depth += 1;
            }
            Event::Leave(at) => {
                emit_trivia(&mut builder, &mut cursor, (*at).min(source.len()));
                builder.finish_node();
                depth = depth.saturating_sub(1);
            }
            Event::Token(kind, span) => {
                let start = span.start.min(source.len());
                let end = span.end.min(source.len());
                if start < cursor {
                    // Overlapping spans would corrupt the tree; skip and note it.
                    continue;
                }
                emit_trivia(&mut builder, &mut cursor, start);
                if end > start {
                    builder.token((*kind).into(), &source[start..end]);
                    cursor = end;
                }
            }
        }
    }

    if depth == 0 && cursor < source.len() {
        // The root node was closed with source still left over.
        diagnostics.push(Diagnostic::warning(
            "internal-tree",
            "syntax tree closed before the end of the file",
            Path::new(""),
            Span::new(cursor, source.len()),
        ));
    }
    builder.finish()
}

/// Splits trivia text into newline, whitespace, and comment runs.
fn split_trivia(text: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut start = 0usize;
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            if i > start {
                pieces.push(&text[start..i]);
            }
            pieces.push(&text[i..i + 1]);
            i += 1;
            start = i;
            continue;
        }
        i += 1;
    }
    if start < text.len() {
        pieces.push(&text[start..]);
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(source: &str) -> Parse {
        parse(source, Path::new("test.pi"))
    }

    fn assert_clean(parse: &Parse) {
        let errors: Vec<_> = parse.diagnostics.iter().filter(|d| d.is_error()).collect();
        assert!(errors.is_empty(), "{errors:#?}");
    }

    #[test]
    fn green_tree_reproduces_the_source() {
        let source = "// leading\nexport anchor Foo:\n    description: Hello\n\n    items:\n        - a\n        - b\n";
        let parse = parse_str(source);
        assert_clean(&parse);
        assert_eq!(parse.syntax().text().to_string(), source);
    }

    #[test]
    fn anchor_header_variants() {
        let parse = parse_str("export abstract anchor Type as type:\n    description:: string\n");
        assert_clean(&parse);
        let anchor = parse.file.anchors().next().expect("anchor");
        assert!(anchor.exported);
        assert!(anchor.is_abstract);
        assert_eq!(anchor.keyword, "anchor");
        assert_eq!(anchor.name, "Type");
        assert_eq!(anchor.alias.as_ref().unwrap().value, "type");
    }

    #[test]
    fn user_keyword_declarations_parse() {
        let parse = parse_str("export arithmetic-operator AdditionOperator:\n    symbol: +\n");
        assert_clean(&parse);
        let anchor = parse.file.anchors().next().expect("anchor");
        assert_eq!(anchor.keyword, "arithmetic-operator");
        assert_eq!(anchor.name, "AdditionOperator");
    }

    #[test]
    fn multiple_bases_parse_in_order() {
        let parse = parse_str("anchor Child extends First, Second:\n    a: 1\n");
        assert_clean(&parse);
        let anchor = parse.file.anchors().next().expect("anchor");
        let names: Vec<_> = anchor.extends.iter().map(|b| b.value.as_str()).collect();
        assert_eq!(names, vec!["First", "Second"]);
    }

    #[test]
    fn keyword_with_extends_combines_both() {
        let parse = parse_str("my-anchor ChildAnchor extends OtherBase:\n    a: 1\n");
        assert_clean(&parse);
        let anchor = parse.file.anchors().next().expect("anchor");
        assert_eq!(anchor.keyword, "my-anchor");
        assert_eq!(anchor.extends.len(), 1);
    }

    #[test]
    fn any_space_free_key_is_a_property() {
        // A key is anything that coerces to a string without spaces, keywords
        // and numbers included.
        let parse = parse_str(
            "anchor A:\n    null: The absence of a value\n    anchor: core\n    string: A unicode string\n    123: numeric\n    foo-bar: hyphenated\n    false: boolean-looking\n",
        );
        assert_clean(&parse);
        let anchor = parse.file.anchors().next().expect("anchor");
        let names: Vec<_> = anchor.body.properties().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["null", "anchor", "string", "123", "foo-bar", "false"]
        );
    }

    #[test]
    fn a_keyword_followed_by_a_space_is_still_a_declaration() {
        // The colon is what separates the two readings.
        let parse = parse_str("anchor MyAnchor:\n    v: 1\n");
        assert_clean(&parse);
        assert_eq!(parse.file.anchors().next().expect("anchor").name, "MyAnchor");

        let parse = parse_str("use ./lib/Type\n\nanchor A:\n    use: a property named use\n");
        assert_clean(&parse);
        assert!(matches!(parse.file.items[0], Item::Use(_)));
        let names: Vec<_> = parse
            .file
            .anchors()
            .next()
            .expect("anchor")
            .body
            .properties()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, vec!["use"]);
    }

    #[test]
    fn constraints_parse_with_and_without_values() {
        let parse = parse_str(
            "anchor A:\n    a:: string: 42\n    b:: extends Operator[]:: null\n    c:: dictionary:: null: null\n",
        );
        assert_clean(&parse);
        let anchor = parse.file.anchors().next().expect("anchor");
        let props: Vec<_> = anchor.body.properties().collect();
        assert_eq!(props[0].constraints.len(), 1);
        assert_eq!(props[0].constraints[0].name, TypeName::String);

        assert_eq!(props[1].constraints.len(), 2);
        assert!(props[1].constraints[0].extends);
        assert!(props[1].constraints[0].list);
        assert!(props[1].value.is_empty());

        assert_eq!(props[2].constraints.len(), 2);
        assert!(!props[2].value.is_empty());
    }

    #[test]
    fn imports_join_continuation_lines() {
        let parse = parse_str("from ./file import\n    FirstThing,\n    SecondThing,\n    ThirdThing\n");
        assert_clean(&parse);
        match &parse.file.items[0] {
            Item::From(decl) => {
                assert_eq!(decl.path.text, "./file");
                assert_eq!(decl.kind, FromKind::Import);
                let names: Vec<_> = decl.items.iter().map(|i| i.name.as_str()).collect();
                assert_eq!(names, vec!["FirstThing", "SecondThing", "ThirdThing"]);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn import_aliases_bind_the_alias() {
        let parse = parse_str("from ./FirstFile import pi SliceOf, MyAnchor MyAliasedAnchor\n");
        assert_clean(&parse);
        match &parse.file.items[0] {
            Item::From(decl) => {
                assert_eq!(decl.items[0].local_name(), "SliceOf");
                assert_eq!(decl.items[1].local_name(), "MyAliasedAnchor");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn star_reexport_parses() {
        let parse = parse_str("from ./MyAnchor export *\n");
        assert_clean(&parse);
        match &parse.file.items[0] {
            Item::From(decl) => {
                assert!(decl.star);
                assert_eq!(decl.kind, FromKind::Export);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn code_fences_are_ordinary_text() {
        // Code blocks are not escaped; what's inside is parsed like anything
        // else.
        let source = "anchor A:\n    body:\n        text\n\n        ```piton\n        key: value\n        - a list item\n        ```\n";
        let parse = parse_str(source);
        assert_clean(&parse);
        assert_eq!(parse.syntax().text().to_string(), source);
        let anchor = parse.file.anchors().next().expect("anchor");
        let body = &anchor.body.properties().next().expect("property").value;
        let block = body.block.as_ref().expect("block");
        assert!(block.items.iter().any(|item| matches!(item, BlockItem::Property(p) if p.name == "key")));
        assert!(block.items.iter().any(|item| matches!(item, BlockItem::ListItem(_))));
    }

    #[test]
    fn an_escape_block_consumes_its_delimiters() {
        let source = "anchor A:\n    body:\n        \\\\\\\n        key: not a property\n        // not a comment\n        {not an expression}\n        \\\\\\\n";
        let parse = parse_str(source);
        assert_clean(&parse);
        assert_eq!(parse.syntax().text().to_string(), source, "still lossless");

        let anchor = parse.file.anchors().next().expect("anchor");
        let block = anchor
            .body
            .properties()
            .next()
            .expect("property")
            .value
            .block
            .as_ref()
            .expect("block");
        let escape = block
            .items
            .iter()
            .find_map(|item| match item {
                BlockItem::Escape(escape) => Some(escape),
                _ => None,
            })
            .expect("escape block");
        assert_eq!(escape.run, 3);
        assert_eq!(
            escape.lines,
            vec!["key: not a property", "// not a comment", "{not an expression}"]
        );
        assert_eq!(block.items.len(), 1, "the delimiters produce nothing");
    }

    #[test]
    fn an_escape_block_inside_a_fence_keeps_only_its_contents() {
        let source = "anchor A:\n    body:\n        ```piton\n        \\\\\\\n        anchor B:\n            v: ${super.x}\n        \\\\\\\n        ```\n";
        let parse = parse_str(source);
        assert_clean(&parse);
        assert_eq!(parse.syntax().text().to_string(), source);

        let anchor = parse.file.anchors().next().expect("anchor");
        let block = anchor
            .body
            .properties()
            .next()
            .expect("property")
            .value
            .block
            .as_ref()
            .expect("block");
        let escape = block
            .items
            .iter()
            .find_map(|item| match item {
                BlockItem::Escape(escape) => Some(escape),
                _ => None,
            })
            .expect("escape block");
        assert_eq!(escape.lines, vec!["anchor B:", "    v: ${super.x}"]);
    }

    #[test]
    fn only_a_matching_run_closes_an_escape_block() {
        // A longer or shorter run is content, which is what lets a block quote
        // another block's delimiters.
        let source = "anchor A:\n    body:\n        \\\\\n        \\\\\\\n        inner\n        \\\\\n";
        let parse = parse_str(source);
        let anchor = parse.file.anchors().next().expect("anchor");
        let block = anchor
            .body
            .properties()
            .next()
            .expect("property")
            .value
            .block
            .as_ref()
            .expect("block");
        let escape = block
            .items
            .iter()
            .find_map(|item| match item {
                BlockItem::Escape(escape) => Some(escape),
                _ => None,
            })
            .expect("escape block");
        assert_eq!(escape.run, 2);
        assert_eq!(escape.lines, vec!["\\\\\\", "inner"]);
    }

    #[test]
    fn an_unterminated_escape_block_is_reported() {
        let parse = parse_str("anchor A:\n    body:\n        \\\\\\\n        never closed\n");
        assert!(parse
            .diagnostics
            .iter()
            .any(|d| d.code == "unterminated-escape-block"));
    }

    #[test]
    fn a_backslash_run_inside_a_line_keeps_its_inline_meaning() {
        // Only a line that is *nothing but* backslashes delimits a block.
        let parse = parse_str("anchor A:\n    body: so \\ x \\ ends here\n");
        assert_clean(&parse);
        let anchor = parse.file.anchors().next().expect("anchor");
        let property = anchor.body.properties().next().expect("property");
        assert!(property.value.inline.is_some(), "still an ordinary prose line");
    }

    #[test]
    fn nested_lists_and_inline_lists() {
        let parse = parse_str(
            "anchor A:\n    nested:\n        - Level 1\n            - Level 2\n    inline: [a, b, [c, d]]\n",
        );
        assert_clean(&parse);
        let anchor = parse.file.anchors().next().expect("anchor");
        let props: Vec<_> = anchor.body.properties().collect();
        let nested_block = props[0].value.block.as_ref().expect("block");
        assert_eq!(nested_block.items.len(), 1);
        let inline = props[1].value.inline_list.as_ref().expect("inline list");
        assert_eq!(inline.len(), 3);
        assert!(inline[2].inline_list.is_some());
    }

    #[test]
    fn pass_marks_an_empty_body() {
        let parse = parse_str("abstract anchor Command extends Construct as command:\n    pass\n");
        assert_clean(&parse);
        let anchor = parse.file.anchors().next().expect("anchor");
        assert!(matches!(anchor.body.items[0], BlockItem::Pass(_)));
    }

    #[test]
    fn merge_items_record_their_operator() {
        let parse = parse_str("anchor A:\n    items:\n        + {super.items}\n        ++ {other}\n");
        assert_clean(&parse);
        let anchor = parse.file.anchors().next().expect("anchor");
        let block = anchor
            .body
            .properties()
            .next()
            .expect("property")
            .value
            .block
            .as_ref()
            .expect("block");
        let ops: Vec<_> = block
            .items
            .iter()
            .filter_map(|item| match item {
                BlockItem::Merge(merge) => Some(merge.op),
                _ => None,
            })
            .collect();
        assert_eq!(ops, vec![MergeOp::Merge, MergeOp::Concat]);
    }

    #[test]
    fn blank_lines_split_paragraphs() {
        let parse = parse_str("anchor A:\n    text:\n        one\n        two\n\n        three\n");
        assert_clean(&parse);
        let anchor = parse.file.anchors().next().expect("anchor");
        let block = anchor
            .body
            .properties()
            .next()
            .expect("property")
            .value
            .block
            .as_ref()
            .expect("block");
        let paragraphs: Vec<_> = block
            .items
            .iter()
            .filter_map(|item| match item {
                BlockItem::Prose(p) => Some(p.lines.len()),
                _ => None,
            })
            .collect();
        assert_eq!(paragraphs, vec![2, 1]);
    }

    #[test]
    fn comments_do_not_reach_the_ast() {
        let parse = parse_str("anchor A:\n    // TODO: not content\n    text: value // not a comment\n");
        assert_clean(&parse);
        let anchor = parse.file.anchors().next().expect("anchor");
        let property = anchor.body.properties().next().expect("property");
        let inline = property.value.inline.as_ref().expect("inline");
        // A comment has to be on its own line; after code, `//` is text.
        assert_eq!(
            inline.segments,
            vec![ProseSegment::Text("value // not a comment".to_string())]
        );
        assert_eq!(anchor.body.items.len(), 1);
    }
}
