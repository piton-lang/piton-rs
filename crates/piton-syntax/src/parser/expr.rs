//! The value grammar: expressions, interpolations, and prose.
//!
//! A value position is parsed as an expression only when the *whole* region
//! matches the expression grammar over literal atoms. Anything else falls back
//! to prose, which is why `1 + 2` is `3` while `a + b` is the string `a + b`.

use chumsky::prelude::*;

use crate::kind::SyntaxKind::{self, *};

use super::tree::{children, Child, Tree};
use super::{kind_in, tok, Extra, TokenInput};

/// Kinds that end a value that occupies the rest of a line.
pub const LINE_STOPS: &[SyntaxKind] = &[NEWLINE, BLANK, INDENT, DEDENT];
/// Kinds that end a value used as an element of an inline list.
pub const ELEMENT_STOPS: &[SyntaxKind] = &[COMMA, R_BRACK, NEWLINE, BLANK, INDENT, DEDENT];

/// A value in text position: an expression if it parses as one, else prose.
pub fn value<'a, I: TokenInput<'a>>() -> impl Parser<'a, I, Child, Extra<'a>> + Clone {
    let expr = text_expr();
    let node = choice((
        expr.then_ignore(stop(LINE_STOPS)),
        text_run(LINE_STOPS),
    ));
    node.map(|inner| Tree::new(VALUE, vec![inner]))
}

/// The expression grammar used in text position.
///
/// Its atoms are literals, inline lists, parentheses, and `{ ... }` blocks;
/// bare words are deliberately not atoms.
fn text_expr<'a, I: TokenInput<'a>>() -> impl Parser<'a, I, Child, Extra<'a>> + Clone {
    let array = recursive(|array| {
        let leaf = choice((literal(), brace_expr(), array.clone()));
        let element = element_value(operators(leaf));
        inline_list(element)
    });
    operators(choice((literal(), brace_expr(), array)))
}

/// The expression grammar used inside `{ ... }`, where words are references.
pub fn ref_expr<'a, I: TokenInput<'a>>() -> impl Parser<'a, I, Child, Extra<'a>> + Clone {
    let array = recursive(|array| {
        let leaf = choice((literal(), name_ref(), array.clone()));
        inline_list(operators(leaf))
    });
    operators(choice((literal(), name_ref(), array)))
}

/// `sigil{ expr }` or a bare `{ expr }` embedded in prose.
fn interpolation<'a, I: TokenInput<'a>>() -> impl Parser<'a, I, Child, Extra<'a>> + Clone {
    group((tok(SIGIL).or_not(), tok(L_BRACE), ref_expr(), tok(R_BRACE)))
        .map(|parts| Tree::new(INTERPOLATION, children(parts)))
}

/// `{ expr }` used as an atom of a text-position expression.
fn brace_expr<'a, I: TokenInput<'a>>() -> impl Parser<'a, I, Child, Extra<'a>> + Clone {
    group((tok(L_BRACE), ref_expr(), tok(R_BRACE)))
        .map(|parts| Tree::new(BRACE_EXPR, children(parts)))
}

fn literal<'a, I: TokenInput<'a>>() -> impl Parser<'a, I, Child, Extra<'a>> + Clone {
    kind_in(&[NUMBER, QUOTED_STRING, TRUE_KW, FALSE_KW, NULL_KW])
        .map(|token| Tree::new(LITERAL, vec![token]))
}

fn name_ref<'a, I: TokenInput<'a>>() -> impl Parser<'a, I, Child, Extra<'a>> + Clone {
    kind_in(&[IDENT, THIS_KW, SELF_KW, SUPER_KW]).map(|token| Tree::new(NAME_REF, vec![token]))
}

/// `[a, b, c]` with a trailing comma allowed.
fn inline_list<'a, I: TokenInput<'a>, E>(element: E) -> impl Parser<'a, I, Child, Extra<'a>> + Clone
where
    E: Parser<'a, I, Child, Extra<'a>> + Clone + 'a,
{
    let items = super::separated(element, COMMA).or_not().map(Option::unwrap_or_default);
    group((tok(L_BRACK), items, tok(R_BRACK))).map(|parts| Tree::new(ARRAY_EXPR, children(parts)))
}

/// An inline-list element: an expression when it fits, otherwise prose.
fn element_value<'a, I: TokenInput<'a>, P>(expr: P) -> impl Parser<'a, I, Child, Extra<'a>> + Clone
where
    P: Parser<'a, I, Child, Extra<'a>> + Clone + 'a,
{
    choice((expr.then_ignore(stop(ELEMENT_STOPS)), text_run(ELEMENT_STOPS)))
        .map(|inner| Tree::new(VALUE, vec![inner]))
}

/// A run of prose, ending before any of `stops`.
fn text_run<'a, I: TokenInput<'a>>(
    stops: &'static [SyntaxKind],
) -> impl Parser<'a, I, Child, Extra<'a>> + Clone {
    let plain = any()
        .filter(move |token: &super::Tok| {
            !stops.contains(&token.kind) && !LINE_STOPS.contains(&token.kind)
        })
        .map(|token: super::Tok| Child::Token(token.index));
    choice((interpolation(), plain))
        .repeated()
        .at_least(1)
        .collect::<Vec<Child>>()
        .map(|parts| Tree::new(TEXT_VALUE, parts))
}

/// Succeed without consuming when the next token ends the value.
fn stop<'a, I: TokenInput<'a>>(
    stops: &'static [SyntaxKind],
) -> impl Parser<'a, I, (), Extra<'a>> + Clone {
    choice((
        any().filter(move |token: &super::Tok| stops.contains(&token.kind)).ignored(),
        end(),
    ))
    .rewind()
}

/// Build the full precedence chain on top of an atom parser.
fn operators<'a, I: TokenInput<'a>, A>(leaf: A) -> impl Parser<'a, I, Child, Extra<'a>> + Clone
where
    A: Parser<'a, I, Child, Extra<'a>> + Clone + 'a,
{
    recursive(move |expr| {
        let paren = group((tok(L_PAREN), expr.clone(), tok(R_PAREN)))
            .map(|parts| Tree::new(PAREN_EXPR, children(parts)));
        let atom = choice((leaf.clone(), paren));

        // Property access binds tightest.
        let field = atom.foldl(
            tok(DOT).then(kind_in(&[IDENT, THIS_KW, SELF_KW, SUPER_KW])).repeated(),
            |base, (dot, name)| Tree::new(FIELD_EXPR, vec![base, dot, name]),
        );
        let unary = tok(MINUS)
            .repeated()
            .foldr(field, |op, operand| Tree::new(UNARY_EXPR, vec![op, operand]));

        let product = binary(unary, &[STAR, SLASH, PERCENT]);
        let sum = binary(product, &[PLUS, PLUS2, MINUS]);
        let comparison = binary(sum, &[EQ2, BANG_EQ, GT_EQ, LT_EQ, GT, LT]);
        let conjunction = binary(comparison, &[AMP2]);
        let disjunction = binary(conjunction, &[PIPE2]);

        // The ternary is the loosest operator and associates to the right.
        disjunction
            .then(group((tok(QUESTION), expr.clone(), tok(COLON), expr)).or_not())
            .map(|(head, tail)| match tail {
                None => head,
                Some(parts) => {
                    let mut kids = vec![head];
                    kids.extend(children(parts));
                    Tree::new(TERNARY_EXPR, kids)
                }
            })
    })
}

/// One left-associative binary precedence level.
fn binary<'a, I: TokenInput<'a>, P>(
    next: P,
    ops: &'static [SyntaxKind],
) -> impl Parser<'a, I, Child, Extra<'a>> + Clone
where
    P: Parser<'a, I, Child, Extra<'a>> + Clone + 'a,
{
    next.clone().foldl(kind_in(ops).then(next).repeated(), |lhs, (op, rhs)| {
        Tree::new(BIN_EXPR, vec![lhs, op, rhs])
    })
}
