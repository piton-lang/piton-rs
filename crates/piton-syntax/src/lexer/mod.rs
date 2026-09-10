//! The Piton lexer.
//!
//! The lexer is line oriented because the language is. Each physical line is
//! classified once (blank, comment, declaration, block entry) and then lexed
//! with the appropriate token set. Indentation is turned into zero-width
//! [`SyntaxKind::INDENT`]/[`SyntaxKind::DEDENT`] markers so that the grammar
//! downstream is context free.
//!
//! The token stream is lossless: concatenating every token's text reproduces
//! the source exactly, except for the zero-width structural markers.

mod value;

use rowan::{TextRange, TextSize};

use crate::kind::{keyword_kind, SyntaxKind};

/// A lexed token: a kind and the source range it covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LexToken {
    pub kind: SyntaxKind,
    pub range: TextRange,
}

/// A problem found while lexing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexError {
    pub message: String,
    pub range: TextRange,
}

/// How a file indents. A file must pick one and stick with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndentStyle {
    /// The character used for indentation.
    pub tab: bool,
    /// How many of that character make one level.
    pub width: usize,
}

impl IndentStyle {
    /// The canonical style produced by `piton format`.
    pub const CANONICAL: IndentStyle = IndentStyle { tab: false, width: 4 };
}

/// The result of lexing one file.
#[derive(Clone, Debug)]
pub struct Lexed {
    pub tokens: Vec<LexToken>,
    pub errors: Vec<LexError>,
    /// The indentation style detected in the file, if it indents at all.
    pub style: Option<IndentStyle>,
}

/// Lex a whole Piton source file.
pub fn lex(src: &str) -> Lexed {
    let mut lexer = Lexer::new(src);
    lexer.run();
    Lexed { tokens: lexer.tokens, errors: lexer.errors, style: lexer.style }
}

pub(crate) struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    tokens: Vec<LexToken>,
    errors: Vec<LexError>,
    /// The stack of open indentation widths, always starting with 0.
    indents: Vec<usize>,
    style: Option<IndentStyle>,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Lexer { src, pos: 0, tokens: Vec::new(), errors: Vec::new(), indents: vec![0], style: None }
    }

    fn run(&mut self) {
        while self.pos < self.src.len() {
            self.line();
        }
        let end = TextSize::new(self.src.len() as u32);
        while self.indents.len() > 1 {
            self.indents.pop();
            self.push_virtual(SyntaxKind::DEDENT, end);
        }
    }

    /// Lex one physical line, including its terminating newline.
    fn line(&mut self) {
        let line_start = self.pos;
        let ws_end = self.scan_indent(line_start);
        let rest = &self.src[ws_end..];

        // A line that holds nothing but whitespace is a significant blank.
        if rest.is_empty() || rest.starts_with('\n') || rest.starts_with("\r\n") {
            self.emit_range(SyntaxKind::WHITESPACE, line_start, ws_end);
            self.pos = ws_end;
            self.eat_newline(SyntaxKind::BLANK);
            return;
        }

        // A line that holds nothing but a comment is entirely trivia so that it
        // never introduces a line break inside a string block.
        if rest.starts_with("//") {
            self.emit_range(SyntaxKind::WHITESPACE, line_start, ws_end);
            self.pos = ws_end;
            let comment_end = self.line_end();
            self.emit_range(SyntaxKind::COMMENT, self.pos, comment_end);
            self.pos = comment_end;
            self.eat_newline(SyntaxKind::WHITESPACE);
            return;
        }

        let width = ws_end - line_start;
        self.sync_indentation(width, ws_end);
        self.emit_range(SyntaxKind::WHITESPACE, line_start, ws_end);
        self.pos = ws_end;

        if self.indents.len() == 1 {
            self.declaration_line();
        } else {
            self.block_line();
        }

        // Anything the line lexers left behind (a stray `}`, say) is consumed
        // so that the token stream always covers the whole file.
        let end = self.line_end();
        if self.pos < end {
            self.emit_range(SyntaxKind::ERROR_TOKEN, self.pos, end);
            self.pos = end;
        }
        self.eat_newline(SyntaxKind::NEWLINE);
    }

    // ---- indentation ---------------------------------------------------

    /// Measure the leading whitespace of a line and record its style.
    fn scan_indent(&mut self, from: usize) -> usize {
        let bytes = self.src.as_bytes();
        let mut end = from;
        let mut tabs = 0usize;
        let mut spaces = 0usize;
        while end < bytes.len() && (bytes[end] == b' ' || bytes[end] == b'\t') {
            if bytes[end] == b'\t' {
                tabs += 1;
            } else {
                spaces += 1;
            }
            end += 1;
        }
        if tabs > 0 && spaces > 0 {
            self.error(
                "indentation mixes tabs and spaces; a file must use one or the other",
                from,
                end,
            );
        }
        end
    }

    /// Open or close blocks so that the indentation stack matches `width`.
    fn sync_indentation(&mut self, width: usize, at: usize) {
        let top = *self.indents.last().unwrap();
        if width > top {
            let step = width - top;
            match self.style {
                None => {
                    let tab = self.src.as_bytes().get(at.saturating_sub(1)) == Some(&b'\t');
                    self.style = Some(IndentStyle { tab, width: step });
                }
                // A step is any whole number of the file's indent unit, not
                // exactly one. A block opened under a `- ` list item sits past
                // the marker, so `  - key:` with its body at column 6 is two
                // units deep while still being one level deeper; so is a string
                // block the author chose to inset further. Both are consistent
                // with the file's unit, which is all the language promises.
                Some(style) if style.width == 0 || step % style.width != 0 => {
                    self.error(
                        &format!(
                            "inconsistent indentation: this file indents by {} but this line indents by {}",
                            style.width, step
                        ),
                        at - width,
                        at,
                    );
                }
                Some(_) => {}
            }
            self.indents.push(width);
            self.push_virtual(SyntaxKind::INDENT, TextSize::new(at as u32));
        } else if width < top {
            while *self.indents.last().unwrap() > width {
                self.indents.pop();
                self.push_virtual(SyntaxKind::DEDENT, TextSize::new(at as u32));
            }
            if *self.indents.last().unwrap() != width {
                self.error("dedent does not match any enclosing indentation level", at - width, at);
                self.indents.push(width);
                self.push_virtual(SyntaxKind::INDENT, TextSize::new(at as u32));
            }
        }
    }

    // ---- line shapes ----------------------------------------------------

    /// A line at the top level: a declaration, or a plain variable binding.
    fn declaration_line(&mut self) {
        loop {
            match self.peek_word() {
                Some("export") => self.eat_keyword("export"),
                Some("abstract") => self.eat_keyword("abstract"),
                _ => break,
            }
            self.eat_inline_space();
        }
        match self.peek_word() {
            Some("from") => return self.import_line(),
            Some("use") => return self.use_line(),
            Some("anchor") => {
                self.eat_keyword("anchor");
                self.eat_inline_space();
                return self.anchor_head();
            }
            _ => {}
        }
        // `my-keyword Name ...:` is a user-defined-keyword anchor declaration.
        if self.at_keyword_declaration() {
            self.eat_ident();
            self.eat_inline_space();
            return self.anchor_head();
        }
        if self.key_head().is_some() {
            self.eat_key_head();
            return self.value_region();
        }
        // A bare `export Name` re-export.
        if self.peek_ident_len().is_some() {
            self.eat_ident();
            self.eat_inline_space();
            return;
        }
        self.value_region();
    }

    /// A line inside an indented block: list item, spread, property, or prose.
    fn block_line(&mut self) {
        let bytes = self.src.as_bytes();
        let at_marker = |lexer: &Self, marker: &str| {
            lexer.src[lexer.pos..].starts_with(marker)
                && matches!(bytes.get(lexer.pos + marker.len()), None | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r'))
        };
        if at_marker(self, "-") {
            self.emit(SyntaxKind::DASH, 1);
            self.eat_inline_space();
            // `- key:` is a dictionary written as a list element, so the same
            // key head a bare line would get is recognised after the marker.
            if self.key_head().is_some() {
                self.eat_key_head();
            }
            return self.value_region();
        }
        if at_marker(self, "++") {
            self.emit(SyntaxKind::PLUS2, 2);
            self.eat_inline_space();
            return self.value_region();
        }
        if at_marker(self, "+") {
            self.emit(SyntaxKind::PLUS, 1);
            self.eat_inline_space();
            return self.value_region();
        }
        if self.key_head().is_some() {
            self.eat_key_head();
            return self.value_region();
        }
        self.value_region();
    }

    // ---- declaration fragments -------------------------------------------

    /// `Name extends A, B as keyword:`
    fn anchor_head(&mut self) {
        self.eat_ident();
        self.eat_inline_space();
        if self.peek_word() == Some("extends") {
            self.eat_keyword("extends");
            self.eat_inline_space();
            loop {
                self.eat_ident();
                self.eat_inline_space();
                if self.src.as_bytes().get(self.pos) == Some(&b',') {
                    self.emit(SyntaxKind::COMMA, 1);
                    self.eat_inline_space();
                    self.eat_line_continuation();
                } else {
                    break;
                }
            }
        }
        if self.peek_word() == Some("as") {
            self.eat_keyword("as");
            self.eat_inline_space();
            self.eat_ident();
            self.eat_inline_space();
        }
        if self.src.as_bytes().get(self.pos) == Some(&b':') {
            self.emit(SyntaxKind::COLON, 1);
            self.value_region();
        }
    }

    /// `from PATH import a, b Alias` or `from PATH export *`.
    fn import_line(&mut self) {
        self.eat_keyword("from");
        self.eat_inline_space();
        self.eat_path();
        self.eat_inline_space();
        match self.peek_word() {
            Some("import") => self.eat_keyword("import"),
            Some("export") => self.eat_keyword("export"),
            _ => return,
        }
        self.eat_inline_space();
        self.eat_line_continuation();
        loop {
            if self.src.as_bytes().get(self.pos) == Some(&b'*') {
                self.emit(SyntaxKind::STAR, 1);
            } else if self.peek_ident_len().is_some() {
                self.eat_ident();
                self.eat_inline_space();
                // An immediately following identifier is the alias.
                if self.peek_ident_len().is_some() && self.peek_word() != Some("from") {
                    self.eat_ident();
                }
            } else {
                break;
            }
            self.eat_inline_space();
            if self.src.as_bytes().get(self.pos) == Some(&b',') {
                self.emit(SyntaxKind::COMMA, 1);
                self.eat_inline_space();
                self.eat_line_continuation();
            } else {
                break;
            }
        }
        self.eat_inline_space();
    }

    /// `use PATH`
    fn use_line(&mut self) {
        self.eat_keyword("use");
        self.eat_inline_space();
        self.eat_path();
        self.eat_inline_space();
    }

    /// Consume a newline plus indentation in the middle of an import list.
    fn eat_line_continuation(&mut self) {
        let mut probe = self.pos;
        let bytes = self.src.as_bytes();
        loop {
            let start = probe;
            while probe < bytes.len() && (bytes[probe] == b' ' || bytes[probe] == b'\t') {
                probe += 1;
            }
            if probe < bytes.len() && bytes[probe] == b'\r' {
                probe += 1;
            }
            if probe < bytes.len() && bytes[probe] == b'\n' {
                probe += 1;
                continue;
            }
            probe = start;
            break;
        }
        if probe > self.pos {
            // Only continue when the next line is indented past the declaration.
            let mut after = probe;
            while after < bytes.len() && (bytes[after] == b' ' || bytes[after] == b'\t') {
                after += 1;
            }
            self.emit_range(SyntaxKind::WHITESPACE, self.pos, after);
            self.pos = after;
        }
    }

    // ---- key heads --------------------------------------------------------

    /// Test whether the line at the cursor opens with `name(:: type)*:`.
    ///
    /// Returns the byte offset just past the terminating `:` when it does.
    fn key_head(&self) -> Option<usize> {
        let mut at = self.pos + self.peek_ident_len()?;
        let bytes = self.src.as_bytes();
        loop {
            match bytes.get(at) {
                Some(b':') if bytes.get(at + 1) == Some(&b':') => {
                    // `:: type` — the space after `::` is mandatory.
                    if !matches!(bytes.get(at + 2), Some(b' ') | Some(b'\t')) {
                        return None;
                    }
                    at += 2;
                    at = self.skip_type_expr(at)?;
                    // `name:: type` with no value declares a property's shape.
                    if matches!(bytes.get(at), None | Some(b'\n') | Some(b'\r'))
                        || self.src[at..].starts_with("//")
                    {
                        return Some(at);
                    }
                }
                Some(b':') => {
                    return match bytes.get(at + 1) {
                        None | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') => Some(at + 1),
                        _ => None,
                    }
                }
                _ => return None,
            }
        }
    }

    /// Skip over one type expression, returning the offset after it.
    fn skip_type_expr(&self, mut at: usize) -> Option<usize> {
        let bytes = self.src.as_bytes();
        while matches!(bytes.get(at), Some(b' ') | Some(b'\t')) {
            at += 1;
        }
        let word = self.ident_len_at(at)?;
        at += word;
        if &self.src[at - word..at] == "extends" {
            while matches!(bytes.get(at), Some(b' ') | Some(b'\t')) {
                at += 1;
            }
            at += self.ident_len_at(at)?;
        }
        while bytes.get(at) == Some(&b'[') && bytes.get(at + 1) == Some(&b']') {
            at += 2;
        }
        while matches!(bytes.get(at), Some(b' ') | Some(b'\t')) {
            at += 1;
        }
        Some(at)
    }

    /// Emit the tokens of a key head that [`Self::key_head`] already accepted.
    fn eat_key_head(&mut self) {
        self.eat_ident();
        loop {
            let bytes = self.src.as_bytes();
            match bytes.get(self.pos) {
                Some(b':') if bytes.get(self.pos + 1) == Some(&b':') => {
                    self.emit(SyntaxKind::COLON2, 2);
                    self.eat_inline_space();
                    self.eat_type_expr();
                }
                Some(b':') => {
                    self.emit(SyntaxKind::COLON, 1);
                    return;
                }
                _ => return,
            }
        }
    }

    fn eat_type_expr(&mut self) {
        if self.peek_word() == Some("extends") {
            self.eat_keyword("extends");
            self.eat_inline_space();
        }
        self.eat_ident();
        while self.src.as_bytes().get(self.pos) == Some(&b'[')
            && self.src.as_bytes().get(self.pos + 1) == Some(&b']')
        {
            self.emit(SyntaxKind::L_BRACK, 1);
            self.emit(SyntaxKind::R_BRACK, 1);
        }
        self.eat_inline_space();
    }

    /// `ident ident` before any `:` marks a user-defined-keyword declaration.
    fn at_keyword_declaration(&self) -> bool {
        let Some(first) = self.peek_ident_len() else { return false };
        let mut at = self.pos + first;
        let bytes = self.src.as_bytes();
        if !matches!(bytes.get(at), Some(b' ') | Some(b'\t')) {
            return false;
        }
        while matches!(bytes.get(at), Some(b' ') | Some(b'\t')) {
            at += 1;
        }
        self.ident_len_at(at).is_some()
    }

    // ---- small token helpers ----------------------------------------------

    fn eat_keyword(&mut self, word: &str) {
        let kind = keyword_kind(word).unwrap_or(SyntaxKind::IDENT);
        self.emit(kind, word.len());
    }

    fn eat_ident(&mut self) {
        match self.peek_ident_len() {
            Some(len) => {
                let text = &self.src[self.pos..self.pos + len];
                let kind = match keyword_kind(text) {
                    Some(kw) => kw,
                    None => SyntaxKind::IDENT,
                };
                self.emit(kind, len);
            }
            None => {
                let end = self.line_end();
                if self.pos < end {
                    self.emit_range(SyntaxKind::ERROR_TOKEN, self.pos, end);
                    self.pos = end;
                }
            }
        }
    }

    fn eat_path(&mut self) {
        let bytes = self.src.as_bytes();
        let start = self.pos;
        let mut end = start;
        while end < bytes.len() && !matches!(bytes[end], b' ' | b'\t' | b'\n' | b'\r') {
            end += 1;
        }
        if end > start {
            self.emit_range(SyntaxKind::PATH, start, end);
            self.pos = end;
        }
    }

    fn eat_inline_space(&mut self) {
        let bytes = self.src.as_bytes();
        let start = self.pos;
        let mut end = start;
        while end < bytes.len() && matches!(bytes[end], b' ' | b'\t') {
            end += 1;
        }
        if end > start {
            self.emit_range(SyntaxKind::WHITESPACE, start, end);
            self.pos = end;
        }
    }

    fn eat_newline(&mut self, kind: SyntaxKind) {
        let bytes = self.src.as_bytes();
        let start = self.pos;
        let mut end = start;
        if bytes.get(end) == Some(&b'\r') {
            end += 1;
        }
        if bytes.get(end) == Some(&b'\n') {
            end += 1;
        }
        if end > start {
            self.emit_range(kind, start, end);
            self.pos = end;
        }
    }

    /// The offset of the end of the current line, not counting the newline.
    pub(crate) fn line_end(&self) -> usize {
        match self.src[self.pos..].find('\n') {
            Some(offset) => {
                let mut end = self.pos + offset;
                if end > self.pos && self.src.as_bytes()[end - 1] == b'\r' {
                    end -= 1;
                }
                end
            }
            None => self.src.len(),
        }
    }

    fn peek_word(&self) -> Option<&'a str> {
        let len = self.peek_ident_len()?;
        Some(&self.src[self.pos..self.pos + len])
    }

    fn peek_ident_len(&self) -> Option<usize> {
        self.ident_len_at(self.pos)
    }

    /// Length of the identifier starting at `at`, if there is one.
    ///
    /// Identifiers are Unicode letters and digits plus `_` and `-`. A `-` only
    /// continues an identifier when it sits directly between two identifier
    /// characters, so `a - b` still lexes as a subtraction.
    pub(crate) fn ident_len_at(&self, at: usize) -> Option<usize> {
        let rest = &self.src[at..];
        let mut chars = rest.char_indices();
        let (_, first) = chars.next()?;
        if !(first.is_alphabetic() || first == '_') {
            return None;
        }
        let mut end = first.len_utf8();
        let bytes = rest.as_bytes();
        while end < rest.len() {
            let ch = rest[end..].chars().next().unwrap();
            if ch.is_alphanumeric() || ch == '_' {
                end += ch.len_utf8();
            } else if ch == '-'
                && bytes.get(end + 1).is_some()
                && rest[end + 1..].chars().next().is_some_and(is_ident_continue)
            {
                end += 1;
            } else {
                break;
            }
        }
        Some(end)
    }

    // ---- emission ----------------------------------------------------------

    pub(crate) fn emit(&mut self, kind: SyntaxKind, len: usize) {
        let end = (self.pos + len).min(self.src.len());
        self.emit_range(kind, self.pos, end);
        self.pos = end;
    }

    pub(crate) fn emit_range(&mut self, kind: SyntaxKind, start: usize, end: usize) {
        if end > start {
            self.tokens.push(LexToken {
                kind,
                range: TextRange::new(TextSize::new(start as u32), TextSize::new(end as u32)),
            });
        }
    }

    fn push_virtual(&mut self, kind: SyntaxKind, at: TextSize) {
        self.tokens.push(LexToken { kind, range: TextRange::empty(at) });
    }

    pub(crate) fn error(&mut self, message: &str, start: usize, end: usize) {
        self.errors.push(LexError {
            message: message.to_string(),
            range: TextRange::new(TextSize::new(start as u32), TextSize::new(end.max(start) as u32)),
        });
    }

    pub(crate) fn source(&self) -> &'a str {
        self.src
    }

    pub(crate) fn cursor(&self) -> usize {
        self.pos
    }
}

pub(crate) fn is_ident_continue(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '-'
}
