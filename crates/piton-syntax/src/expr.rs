//! Expression lexing and parsing.
//!
//! Expressions are the only context-free corner of Piton, so this is where
//! chumsky earns its keep. Everything outside `{...}` is layout-sensitive and is
//! handled by the line parser; everything inside is an ordinary precedence
//! grammar.

use chumsky::prelude::*;
use piton_core::{Diagnostic, Span};

use crate::ast::{BinaryOp, Expr, ExprKind, Sigil, Spanned, UnaryOp};

/// A token inside an expression.
///
/// The payload is kept as text rather than a parsed number so the token type can
/// derive `Hash` and `Eq`, which chumsky's error type requires.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprToken {
    pub kind: ExprTokenKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExprTokenKind {
    Ident,
    Number,
    Quoted,
    Dot,
    Comma,
    Plus,
    PlusPlus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,
    Question,
    Colon,
    EqEq,
    BangEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    AmpAmp,
    PipePipe,
    LParen,
    RParen,
    LBracket,
    RBracket,
    /// `${`, `#{`, or `@{` opening a nested conversion or reference.
    DollarBrace,
    HashBrace,
    AtBrace,
    RBrace,
    Unknown,
}

impl std::fmt::Display for ExprToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

/// Splits expression source into tokens.
///
/// `base` is the absolute offset of `source` within the file so spans point at
/// real positions.
pub fn lex(source: &str, base: usize) -> (Vec<(ExprToken, std::ops::Range<usize>)>, Vec<Diagnostic>) {
    let mut tokens = Vec::new();
    let diagnostics = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    // Byte offset of each char index, so spans stay byte-based.
    let mut offsets = Vec::with_capacity(chars.len() + 1);
    let mut offset = 0usize;
    for ch in &chars {
        offsets.push(offset);
        offset += ch.len_utf8();
    }
    offsets.push(offset);

    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        let (kind, len) = match ch {
            '0'..='9' => {
                let mut j = i;
                while j < chars.len()
                    && (chars[j].is_ascii_digit() || chars[j] == '_' || chars[j] == '.')
                {
                    // A dot only continues the number when a digit follows,
                    // otherwise it is property access on a numeric key.
                    if chars[j] == '.' && !chars.get(j + 1).is_some_and(|c| c.is_ascii_digit()) {
                        break;
                    }
                    j += 1;
                }
                (ExprTokenKind::Number, j - i)
            }
            '"' => {
                let mut j = i + 1;
                while j < chars.len() && chars[j] != '"' {
                    if chars[j] == '\\' {
                        j += 1;
                    }
                    j += 1;
                }
                let len = (j.min(chars.len()) + 1).min(chars.len()) - i;
                (ExprTokenKind::Quoted, len.max(1))
            }
            c if c.is_alphabetic() || c == '_' => {
                let mut j = i;
                loop {
                    while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                        j += 1;
                    }
                    // A hyphen in the middle of a name is part of the name, so
                    // `config.foo-bar` reads the key `foo-bar`. Subtraction
                    // needs spaces around it.
                    if j + 1 < chars.len()
                        && chars[j] == '-'
                        && (chars[j + 1].is_alphanumeric() || chars[j + 1] == '_')
                    {
                        j += 1;
                        continue;
                    }
                    break;
                }
                (ExprTokenKind::Ident, j - i)
            }
            '$' if chars.get(i + 1) == Some(&'{') => (ExprTokenKind::DollarBrace, 2),
            '#' if chars.get(i + 1) == Some(&'{') => (ExprTokenKind::HashBrace, 2),
            '@' if chars.get(i + 1) == Some(&'{') => (ExprTokenKind::AtBrace, 2),
            '}' => (ExprTokenKind::RBrace, 1),
            '.' => (ExprTokenKind::Dot, 1),
            ',' => (ExprTokenKind::Comma, 1),
            '+' if chars.get(i + 1) == Some(&'+') => (ExprTokenKind::PlusPlus, 2),
            '+' => (ExprTokenKind::Plus, 1),
            '-' => (ExprTokenKind::Minus, 1),
            '*' => (ExprTokenKind::Star, 1),
            '/' => (ExprTokenKind::Slash, 1),
            '%' => (ExprTokenKind::Percent, 1),
            '?' => (ExprTokenKind::Question, 1),
            ':' => (ExprTokenKind::Colon, 1),
            '(' => (ExprTokenKind::LParen, 1),
            ')' => (ExprTokenKind::RParen, 1),
            '[' => (ExprTokenKind::LBracket, 1),
            ']' => (ExprTokenKind::RBracket, 1),
            '=' if chars.get(i + 1) == Some(&'=') => (ExprTokenKind::EqEq, 2),
            '!' if chars.get(i + 1) == Some(&'=') => (ExprTokenKind::BangEq, 2),
            '!' => (ExprTokenKind::Bang, 1),
            '<' if chars.get(i + 1) == Some(&'=') => (ExprTokenKind::LtEq, 2),
            '<' => (ExprTokenKind::Lt, 1),
            '>' if chars.get(i + 1) == Some(&'=') => (ExprTokenKind::GtEq, 2),
            '>' => (ExprTokenKind::Gt, 1),
            '&' if chars.get(i + 1) == Some(&'&') => (ExprTokenKind::AmpAmp, 2),
            '|' if chars.get(i + 1) == Some(&'|') => (ExprTokenKind::PipePipe, 2),
            _ => (ExprTokenKind::Unknown, 1),
        };
        i = (start + len).min(chars.len()).max(start + 1);
        let text: String = chars[start..i.min(chars.len())].iter().collect();
        let range = (base + offsets[start])..(base + offsets[i.min(chars.len())]);
        tokens.push((ExprToken { kind, text }, range));
    }
    (tokens, diagnostics)
}

fn token(kind: ExprTokenKind) -> impl Parser<ExprToken, ExprToken, Error = Simple<ExprToken>> + Clone
{
    filter(move |t: &ExprToken| t.kind == kind)
}

fn binary_of(kind: ExprTokenKind) -> BinaryOp {
    match kind {
        ExprTokenKind::Plus => BinaryOp::Add,
        ExprTokenKind::PlusPlus => BinaryOp::Concat,
        ExprTokenKind::Minus => BinaryOp::Subtract,
        ExprTokenKind::Star => BinaryOp::Multiply,
        ExprTokenKind::Slash => BinaryOp::Divide,
        ExprTokenKind::Percent => BinaryOp::Modulo,
        ExprTokenKind::EqEq => BinaryOp::Equal,
        ExprTokenKind::BangEq => BinaryOp::NotEqual,
        ExprTokenKind::Lt => BinaryOp::Less,
        ExprTokenKind::LtEq => BinaryOp::LessEqual,
        ExprTokenKind::Gt => BinaryOp::Greater,
        ExprTokenKind::GtEq => BinaryOp::GreaterEqual,
        ExprTokenKind::AmpAmp => BinaryOp::And,
        ExprTokenKind::PipePipe => BinaryOp::Or,
        other => unreachable!("{other:?} is not a binary operator"),
    }
}

/// Parses a number literal, honouring the underscore digit separators the
/// language allows for readability.
pub fn parse_number(text: &str) -> Option<f64> {
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();
    cleaned.parse::<f64>().ok()
}

fn unquote(text: &str) -> String {
    let inner = text
        .strip_prefix('"')
        .map(|rest| rest.strip_suffix('"').unwrap_or(rest))
        .unwrap_or(text);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn binary_level<P>(
    next: P,
    ops: &'static [ExprTokenKind],
) -> impl Parser<ExprToken, Expr, Error = Simple<ExprToken>> + Clone
where
    P: Parser<ExprToken, Expr, Error = Simple<ExprToken>> + Clone,
{
    let op = filter(move |t: &ExprToken| ops.contains(&t.kind));
    next.clone()
        .then(op.then(next).repeated())
        .foldl(|lhs, (op, rhs)| {
            let span = Span::new(lhs.span.start, rhs.span.end);
            Expr {
                span,
                kind: ExprKind::Binary(binary_of(op.kind), Box::new(lhs), Box::new(rhs)),
            }
        })
}

/// Builds the expression grammar.
pub fn parser() -> impl Parser<ExprToken, Expr, Error = Simple<ExprToken>> + Clone {
    recursive(|expr| {
        let ident = token(ExprTokenKind::Ident);

        let atom = choice((
            token(ExprTokenKind::Number).map_with_span(|t: ExprToken, span: std::ops::Range<usize>| Expr {
                span: span.into(),
                kind: parse_number(&t.text).map_or(ExprKind::Error, ExprKind::Number),
            }),
            token(ExprTokenKind::Quoted).map_with_span(|t: ExprToken, span: std::ops::Range<usize>| {
                Expr {
                    span: span.into(),
                    kind: ExprKind::Quoted(unquote(&t.text)),
                }
            }),
            ident.map_with_span(|t: ExprToken, span: std::ops::Range<usize>| {
                let kind = match t.text.as_str() {
                    "true" => ExprKind::Bool(true),
                    "false" => ExprKind::Bool(false),
                    "null" => ExprKind::Null,
                    "this" => ExprKind::This,
                    "self" => ExprKind::SelfRef,
                    "super" => ExprKind::Super,
                    _ => ExprKind::Name(t.text.clone()),
                };
                Expr {
                    span: span.into(),
                    kind,
                }
            }),
            expr.clone()
                .separated_by(token(ExprTokenKind::Comma))
                .allow_trailing()
                .delimited_by(
                    token(ExprTokenKind::LBracket),
                    token(ExprTokenKind::RBracket),
                )
                .map_with_span(|items, span: std::ops::Range<usize>| Expr {
                    span: span.into(),
                    kind: ExprKind::List(items),
                }),
            expr.clone()
                .delimited_by(token(ExprTokenKind::LParen), token(ExprTokenKind::RParen))
                .map_with_span(|inner: Expr, span: std::ops::Range<usize>| Expr {
                    span: span.into(),
                    kind: ExprKind::Paren(Box::new(inner)),
                }),
            filter(|t: &ExprToken| {
                matches!(
                    t.kind,
                    ExprTokenKind::DollarBrace | ExprTokenKind::HashBrace | ExprTokenKind::AtBrace
                )
            })
            .then(expr.clone())
            .then_ignore(token(ExprTokenKind::RBrace))
            .map_with_span(|(open, inner): (ExprToken, Expr), span: std::ops::Range<usize>| {
                let sigil = match open.kind {
                    ExprTokenKind::DollarBrace => Sigil::Stringify,
                    ExprTokenKind::HashBrace => Sigil::Numeric,
                    _ => Sigil::Reference,
                };
                Expr {
                    span: span.into(),
                    kind: ExprKind::Nested(sigil, Box::new(inner)),
                }
            }),
        ));

        // Property access binds tighter than every operator.
        let access = atom
            .then(
                token(ExprTokenKind::Dot)
                    .ignore_then(
                        filter(|t: &ExprToken| {
                            matches!(t.kind, ExprTokenKind::Ident | ExprTokenKind::Number)
                        })
                        .map_with_span(|t: ExprToken, span: std::ops::Range<usize>| {
                            Spanned::new(span.into(), t.text)
                        }),
                    )
                    .repeated(),
            )
            .foldl(|base, field| {
                let span = Span::new(base.span.start, field.span.end);
                Expr {
                    span,
                    kind: ExprKind::Field(Box::new(base), field),
                }
            });

        let unary = filter(|t: &ExprToken| {
            matches!(t.kind, ExprTokenKind::Bang | ExprTokenKind::Minus)
        })
        .map_with_span(|t: ExprToken, span: std::ops::Range<usize>| (t, Span::from(span)))
        .repeated()
        .then(access)
        .foldr(|(op, op_span), operand| {
            let span = Span::new(op_span.start, operand.span.end);
            let op = if op.kind == ExprTokenKind::Bang {
                UnaryOp::Not
            } else {
                UnaryOp::Negate
            };
            Expr {
                span,
                kind: ExprKind::Unary(op, Box::new(operand)),
            }
        });

        let product = binary_level(
            unary,
            &[
                ExprTokenKind::Star,
                ExprTokenKind::Slash,
                ExprTokenKind::Percent,
            ],
        );
        let sum = binary_level(
            product,
            &[
                ExprTokenKind::Plus,
                ExprTokenKind::PlusPlus,
                ExprTokenKind::Minus,
            ],
        );
        let comparison = binary_level(
            sum,
            &[
                ExprTokenKind::Lt,
                ExprTokenKind::LtEq,
                ExprTokenKind::Gt,
                ExprTokenKind::GtEq,
            ],
        );
        let equality = binary_level(
            comparison,
            &[ExprTokenKind::EqEq, ExprTokenKind::BangEq],
        );
        let and = binary_level(equality, &[ExprTokenKind::AmpAmp]);
        let or = binary_level(and, &[ExprTokenKind::PipePipe]);

        // The ternary is right-associative so it can be chained in either slot.
        or.clone()
            .then(
                token(ExprTokenKind::Question)
                    .ignore_then(expr.clone())
                    .then_ignore(token(ExprTokenKind::Colon))
                    .then(expr)
                    .or_not(),
            )
            .map(|(condition, branches)| match branches {
                None => condition,
                Some((consequent, alternative)) => {
                    let span = Span::new(condition.span.start, alternative.span.end);
                    Expr {
                        span,
                        kind: ExprKind::Ternary(
                            Box::new(condition),
                            Box::new(consequent),
                            Box::new(alternative),
                        ),
                    }
                }
            })
    })
    .then_ignore(end())
}

/// Parses one expression, returning a recovered [`ExprKind::Error`] node and a
/// diagnostic when the source does not parse.
pub fn parse(source: &str, base: usize, file: &std::path::Path) -> (Expr, Vec<Diagnostic>) {
    let span = Span::new(base, base + source.len());
    let (tokens, mut diagnostics) = lex(source, base);
    if tokens.is_empty() {
        diagnostics.push(Diagnostic::error(
            "empty-expression",
            "expression is empty",
            file,
            span,
        ));
        return (
            Expr {
                span,
                kind: ExprKind::Error,
            },
            diagnostics,
        );
    }

    let eoi = base + source.len();
    let stream = chumsky::Stream::from_iter(eoi..eoi + 1, tokens.into_iter());
    match parser().parse(stream) {
        Ok(expr) => (expr, diagnostics),
        Err(errors) => {
            for error in errors {
                let range = error.span();
                diagnostics.push(Diagnostic::error(
                    "invalid-expression",
                    describe(&error),
                    file,
                    Span::new(range.start, range.end.max(range.start + 1)),
                ));
            }
            (
                Expr {
                    span,
                    kind: ExprKind::Error,
                },
                diagnostics,
            )
        }
    }
}

fn describe(error: &Simple<ExprToken>) -> String {
    match error.found() {
        Some(token) => format!("unexpected `{}` in expression", token.text),
        None => "unexpected end of expression".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn parse_ok(source: &str) -> Expr {
        let (expr, diagnostics) = parse(source, 0, Path::new("test.pi"));
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        expr
    }

    #[test]
    fn arithmetic_precedence_follows_mathematics() {
        let expr = parse_ok("1 + 2 * 3");
        match expr.kind {
            ExprKind::Binary(BinaryOp::Add, _, rhs) => {
                assert!(matches!(rhs.kind, ExprKind::Binary(BinaryOp::Multiply, ..)));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parentheses_alter_order() {
        let expr = parse_ok("(1 + 2) * 3");
        assert!(matches!(
            expr.kind,
            ExprKind::Binary(BinaryOp::Multiply, ..)
        ));
    }

    #[test]
    fn property_access_binds_tightest() {
        let expr = parse_ok("myDictionary.list + [4, 5, 6]");
        match expr.kind {
            ExprKind::Binary(BinaryOp::Add, lhs, rhs) => {
                assert!(matches!(lhs.kind, ExprKind::Field(..)));
                assert!(matches!(rhs.kind, ExprKind::List(ref items) if items.len() == 3));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn keywords_become_their_own_nodes() {
        assert!(matches!(parse_ok("super.items").kind, ExprKind::Field(..)));
        assert!(matches!(parse_ok("self").kind, ExprKind::SelfRef));
        assert!(matches!(parse_ok("this").kind, ExprKind::This));
        assert!(matches!(parse_ok("null").kind, ExprKind::Null));
    }

    #[test]
    fn ternary_chains_on_the_right() {
        let expr = parse_ok("a ? b : c ? d : e");
        match expr.kind {
            ExprKind::Ternary(_, _, alternative) => {
                assert!(matches!(alternative.kind, ExprKind::Ternary(..)));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn numbers_allow_underscore_separators() {
        assert_eq!(parse_number("1_200_000.00"), Some(1_200_000.0));
        assert_eq!(parse_number("0.14"), Some(0.14));
    }

    #[test]
    fn quoted_strings_keep_their_text() {
        assert_eq!(
            parse_ok("\"false\"").kind,
            ExprKind::Quoted("false".to_string())
        );
    }

    #[test]
    fn hyphens_inside_names_are_part_of_the_name() {
        match parse_ok("config.foo-bar").kind {
            ExprKind::Field(_, field) => assert_eq!(field.value, "foo-bar"),
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(parse_ok("a - b").kind, ExprKind::Binary(BinaryOp::Subtract, ..)));
    }

    #[test]
    fn sigils_nest_inside_expressions() {
        match parse_ok("@{Button} == @{Card.color}").kind {
            ExprKind::Binary(BinaryOp::Equal, lhs, rhs) => {
                assert!(matches!(lhs.kind, ExprKind::Nested(Sigil::Reference, _)));
                assert!(matches!(rhs.kind, ExprKind::Nested(Sigil::Reference, _)));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(parse_ok("\"a\" + ${true}").kind, ExprKind::Binary(..)));
    }

    #[test]
    fn invalid_expressions_recover() {
        let (expr, diagnostics) = parse("1 +", 0, Path::new("test.pi"));
        assert!(matches!(expr.kind, ExprKind::Error));
        assert!(!diagnostics.is_empty());
    }
}
