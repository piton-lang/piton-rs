//! Lexing of value regions.
//!
//! A value region is everything after a `:`, `-`, `+`, or `++` on a line, plus
//! whole lines of prose inside a block. It has two interleaved modes:
//!
//! * **Text mode** — prose. Words are [`SyntaxKind::TEXT`]; operators are only
//!   recognised when they stand alone between spaces, so `well-known` stays one
//!   word while `3.14 - 3.14` is arithmetic.
//! * **Expression mode** — everything between `{` and `}`. Bare words are
//!   identifiers and the usual operator set applies.

use crate::kind::{keyword_kind, SyntaxKind};

use super::Lexer;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Text,
    Expr,
}

/// Characters that always end a run of prose.
const TEXT_BREAKS: &[u8] = b" \t\r\n\"\\{}[](),";

impl<'a> Lexer<'a> {
    /// Lex from the cursor to the end of the line as a value.
    pub(crate) fn value_region(&mut self) {
        let region_start = self.cursor();
        let end = self.line_end();
        let mut modes = vec![Mode::Text];
        while self.cursor() < end {
            match modes.last().copied().unwrap_or(Mode::Text) {
                Mode::Text => self.text_token(region_start, end, &mut modes),
                Mode::Expr => self.expr_token(end, &mut modes),
            }
        }
        if modes.len() > 1 {
            self.error("unterminated `{` expression", region_start, end);
        }
    }

    fn text_token(&mut self, region_start: usize, end: usize, modes: &mut Vec<Mode>) {
        let src = self.source();
        let bytes = src.as_bytes();
        let at = self.cursor();
        let ch = bytes[at];

        if ch == b' ' || ch == b'\t' {
            let mut stop = at;
            while stop < end && matches!(bytes[stop], b' ' | b'\t') {
                stop += 1;
            }
            return self.emit_range_advance(SyntaxKind::WHITESPACE, at, stop);
        }
        // `//` only opens a comment at a word boundary, so URLs survive.
        if ch == b'/' && bytes.get(at + 1) == Some(&b'/') && self.at_word_boundary(region_start) {
            return self.emit_range_advance(SyntaxKind::COMMENT, at, end);
        }
        if ch == b'"' {
            return self.quoted_string(end);
        }
        if ch == b'\\' {
            let len = src[at + 1..end].chars().next().map_or(1, |c| 1 + c.len_utf8());
            return self.emit(SyntaxKind::ESCAPE, len);
        }
        match ch {
            b'{' => {
                modes.push(Mode::Expr);
                return self.emit(SyntaxKind::L_BRACE, 1);
            }
            b'}' => return self.emit(SyntaxKind::R_BRACE, 1),
            b'[' => return self.emit(SyntaxKind::L_BRACK, 1),
            b']' => return self.emit(SyntaxKind::R_BRACK, 1),
            b'(' => return self.emit(SyntaxKind::L_PAREN, 1),
            b')' => return self.emit(SyntaxKind::R_PAREN, 1),
            b',' => return self.emit(SyntaxKind::COMMA, 1),
            _ => {}
        }
        // A standalone operator, or a sign directly in front of a number.
        if let Some((kind, len)) = operator_at(src, at) {
            let after = at + len;
            let free_before = self.at_word_boundary(region_start);
            let free_after = after >= end || is_text_boundary(bytes[after]);
            if free_before && free_after {
                return self.emit(kind, len);
            }
            if free_before
                && kind == SyntaxKind::MINUS
                && bytes.get(after).is_some_and(|b| b.is_ascii_digit())
            {
                return self.emit(SyntaxKind::MINUS, 1);
            }
        }

        // Otherwise: a run of prose.
        let mut stop = at;
        while stop < end {
            let b = bytes[stop];
            if TEXT_BREAKS.contains(&b) {
                break;
            }
            if (b == b'$' || b == b'@') && bytes.get(stop + 1) == Some(&b'{') && stop > at {
                break;
            }
            stop += 1;
        }
        if stop == at {
            stop = at + src[at..].chars().next().map_or(1, char::len_utf8);
        }
        let text = &src[at..stop];
        let kind = if bytes.get(stop) == Some(&b'{') && is_sigil(text) {
            SyntaxKind::SIGIL
        } else if is_number_literal(text) {
            SyntaxKind::NUMBER
        } else {
            match keyword_kind(text) {
                Some(kw @ (SyntaxKind::TRUE_KW | SyntaxKind::FALSE_KW | SyntaxKind::NULL_KW)) => kw,
                _ => SyntaxKind::TEXT,
            }
        };
        self.emit_range_advance(kind, at, stop);
    }

    fn expr_token(&mut self, end: usize, modes: &mut Vec<Mode>) {
        let src = self.source();
        let bytes = src.as_bytes();
        let at = self.cursor();
        let ch = bytes[at];

        if ch == b' ' || ch == b'\t' {
            let mut stop = at;
            while stop < end && matches!(bytes[stop], b' ' | b'\t') {
                stop += 1;
            }
            return self.emit_range_advance(SyntaxKind::WHITESPACE, at, stop);
        }
        match ch {
            b'}' => {
                modes.pop();
                return self.emit(SyntaxKind::R_BRACE, 1);
            }
            b'{' => {
                modes.push(Mode::Expr);
                return self.emit(SyntaxKind::L_BRACE, 1);
            }
            b'"' => return self.quoted_string(end),
            b'[' => return self.emit(SyntaxKind::L_BRACK, 1),
            b']' => return self.emit(SyntaxKind::R_BRACK, 1),
            b'(' => return self.emit(SyntaxKind::L_PAREN, 1),
            b')' => return self.emit(SyntaxKind::R_PAREN, 1),
            b',' => return self.emit(SyntaxKind::COMMA, 1),
            _ => {}
        }
        if ch.is_ascii_digit() {
            let stop = number_end(src, at, end);
            return self.emit_range_advance(SyntaxKind::NUMBER, at, stop);
        }
        if let Some(len) = self.ident_len_at(at) {
            let text = &src[at..at + len];
            let kind = keyword_kind(text).unwrap_or(SyntaxKind::IDENT);
            return self.emit(kind, len);
        }
        if ch == b'.' {
            return self.emit(SyntaxKind::DOT, 1);
        }
        if let Some((kind, len)) = operator_at(src, at) {
            return self.emit(kind, len);
        }
        let len = src[at..].chars().next().map_or(1, char::len_utf8);
        self.error("unexpected character in expression", at, at + len);
        self.emit(SyntaxKind::ERROR_TOKEN, len);
    }

    fn quoted_string(&mut self, end: usize) {
        let src = self.source();
        let bytes = src.as_bytes();
        let start = self.cursor();
        let mut stop = start + 1;
        while stop < end {
            match bytes[stop] {
                b'\\' => stop += 2,
                b'"' => {
                    stop += 1;
                    return self.emit_range_advance(SyntaxKind::QUOTED_STRING, start, stop);
                }
                _ => stop += 1,
            }
        }
        self.error("unterminated quoted string", start, end);
        self.emit_range_advance(SyntaxKind::QUOTED_STRING, start, end);
    }

    /// True when the cursor sits at the start of the region or after a break.
    fn at_word_boundary(&self, region_start: usize) -> bool {
        let at = self.cursor();
        if at <= region_start {
            return true;
        }
        matches!(self.source().as_bytes()[at - 1], b' ' | b'\t' | b'(' | b'[' | b',')
    }

    fn emit_range_advance(&mut self, kind: SyntaxKind, start: usize, end: usize) {
        self.emit(kind, end - start);
    }
}

fn is_text_boundary(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | b')' | b']' | b',' | b'(' | b'[' | b'{' | b'"')
}

/// Recognise the longest operator spelling at `at`.
fn operator_at(src: &str, at: usize) -> Option<(SyntaxKind, usize)> {
    let rest = &src[at..];
    const TWO: &[(&str, SyntaxKind)] = &[
        ("++", SyntaxKind::PLUS2),
        ("::", SyntaxKind::COLON2),
        ("==", SyntaxKind::EQ2),
        ("!=", SyntaxKind::BANG_EQ),
        (">=", SyntaxKind::GT_EQ),
        ("<=", SyntaxKind::LT_EQ),
        ("&&", SyntaxKind::AMP2),
        ("||", SyntaxKind::PIPE2),
    ];
    for (text, kind) in TWO {
        if rest.starts_with(text) {
            return Some((*kind, 2));
        }
    }
    let kind = match rest.as_bytes().first()? {
        b'+' => SyntaxKind::PLUS,
        b'-' => SyntaxKind::MINUS,
        b'*' => SyntaxKind::STAR,
        b'/' => SyntaxKind::SLASH,
        b'%' => SyntaxKind::PERCENT,
        b'>' => SyntaxKind::GT,
        b'<' => SyntaxKind::LT,
        b'?' => SyntaxKind::QUESTION,
        b':' => SyntaxKind::COLON,
        _ => return None,
    };
    Some((kind, 1))
}

fn number_end(src: &str, at: usize, end: usize) -> usize {
    let bytes = src.as_bytes();
    let mut stop = at;
    while stop < end && (bytes[stop].is_ascii_digit() || bytes[stop] == b'_') {
        stop += 1;
    }
    if stop < end && bytes[stop] == b'.' && bytes.get(stop + 1).is_some_and(u8::is_ascii_digit) {
        stop += 1;
        while stop < end && (bytes[stop].is_ascii_digit() || bytes[stop] == b'_') {
            stop += 1;
        }
    }
    stop
}

/// `$`, `@`, or a lowercase kebab-case word may introduce an interpolation.
fn is_sigil(text: &str) -> bool {
    if text == "$" || text == "@" {
        return true;
    }
    let mut chars = text.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    text.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A whole run of prose that is exactly a numeric literal.
fn is_number_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_digit() {
        return false;
    }
    let mut seen_dot = false;
    let mut prev_digit = false;
    for (index, &byte) in bytes.iter().enumerate() {
        match byte {
            b'0'..=b'9' => prev_digit = true,
            b'_' if prev_digit => {}
            b'.' if !seen_dot && prev_digit && bytes.get(index + 1).is_some_and(u8::is_ascii_digit) => {
                seen_dot = true;
                prev_digit = false;
            }
            _ => return false,
        }
    }
    prev_digit
}
