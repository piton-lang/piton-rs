//! The Piton grammar, written with chumsky over the lexer's token stream.
//!
//! The grammar is context free because the lexer has already turned
//! indentation into `INDENT`/`DEDENT` markers and classified each line's head.
//! Every alternative consumes at least one token, and an explicit error line
//! catches anything unrecognised, so parsing always produces a complete tree.

pub mod expr;
pub mod tree;

use chumsky::input::ValueInput;
use chumsky::prelude::*;
use rowan::GreenNode;

use crate::kind::SyntaxKind::{self, *};
use crate::lexer::{lex, LexError, LexToken};

use tree::{build, children, Child, Tree};

/// A token as the grammar sees it: its kind plus its index in the full stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tok {
    pub kind: SyntaxKind,
    pub index: u32,
}

pub type Extra<'a> = extra::Default;

/// The input shape every grammar rule accepts.
pub trait TokenInput<'a>: ValueInput<'a, Token = Tok, Span = SimpleSpan> + 'a {}
impl<'a, T> TokenInput<'a> for T where T: ValueInput<'a, Token = Tok, Span = SimpleSpan> + 'a {}

/// Match exactly one token kind.
pub fn tok<'a, I: TokenInput<'a>>(kind: SyntaxKind) -> impl Parser<'a, I, Child, Extra<'a>> + Clone {
    any().filter(move |token: &Tok| token.kind == kind).map(|token: Tok| Child::Token(token.index))
}

/// Match any one of several token kinds.
pub fn kind_in<'a, I: TokenInput<'a>>(
    kinds: &'static [SyntaxKind],
) -> impl Parser<'a, I, Child, Extra<'a>> + Clone {
    any()
        .filter(move |token: &Tok| kinds.contains(&token.kind))
        .map(|token: Tok| Child::Token(token.index))
}

/// A separated list that keeps its separators in the tree.
///
/// `separated_by` drops separator tokens, which would leave commas to be
/// swept up as trivia by the tree builder, so the list is spelled out here.
pub fn separated<'a, I: TokenInput<'a>, P>(
    element: P,
    sep: SyntaxKind,
) -> impl Parser<'a, I, Vec<Child>, Extra<'a>> + Clone
where
    P: Parser<'a, I, Child, Extra<'a>> + Clone + 'a,
{
    let tail = group((tok(sep), element.clone()))
        .map(children)
        .repeated()
        .collect::<Vec<Vec<Child>>>();
    element.then(tail).then(tok(sep).or_not()).map(|((head, rest), trailing)| {
        let mut out = vec![head];
        out.extend(rest.into_iter().flatten());
        out.extend(trailing);
        out
    })
}

/// The parsed form of one file: a lossless tree plus everything that went wrong.
#[derive(Clone, Debug)]
pub struct Parse {
    pub green: GreenNode,
    pub errors: Vec<LexError>,
}

/// Lex and parse a Piton source file.
pub fn parse(src: &str) -> Parse {
    let lexed = lex(src);
    let stream: Vec<Tok> = lexed
        .tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| !token.kind.is_trivia())
        .map(|(index, token)| Tok { kind: token.kind, index: index as u32 })
        .collect();

    let tree = document()
        .parse(&stream[..])
        .into_output()
        .unwrap_or_else(|| every_token(&lexed.tokens));
    Parse { green: build(src, &lexed.tokens, &tree), errors: lexed.errors }
}

/// A last-resort tree used only if the grammar somehow fails outright.
fn every_token(tokens: &[LexToken]) -> Tree {
    Tree {
        kind: ROOT,
        children: (0..tokens.len() as u32).map(Child::Token).collect(),
    }
}

fn document<'a, I: TokenInput<'a>>() -> impl Parser<'a, I, Tree, Extra<'a>> {
    let block = block_parser();
    let value = expr::value();

    let anchor_decl = group((
        tok(EXPORT_KW).or_not(),
        tok(ABSTRACT_KW).or_not(),
        kind_in(&[ANCHOR_KW, IDENT]),
        tok(IDENT),
        extends_clause().or_not(),
        as_clause().or_not(),
        tok(COLON),
        value.clone().or_not(),
        tok(NEWLINE).or_not(),
        block.clone().or_not(),
    ))
    .map(|parts| Tree::new(ANCHOR_DECL, children(parts)));

    let var_decl = choice((
        group((
            tok(EXPORT_KW).or_not(),
            tok(IDENT),
            type_annotation().repeated().collect::<Vec<Child>>(),
            tok(COLON),
            value.or_not(),
            tok(NEWLINE).or_not(),
            block.clone().or_not(),
        ))
        .map(children),
        // `name:: type` with no `:` declares a shape without a value.
        group((
            tok(EXPORT_KW).or_not(),
            tok(IDENT),
            type_annotation().repeated().at_least(1).collect::<Vec<Child>>(),
            tok(NEWLINE).or_not(),
            block.or_not(),
        ))
        .map(children),
    ))
    .map(|parts| Tree::new(VAR_DECL, parts));

    let import_decl = group((
        tok(FROM_KW),
        tok(PATH),
        tok(IMPORT_KW),
        import_list(),
        tok(NEWLINE).or_not(),
    ))
    .map(|parts| Tree::new(IMPORT_DECL, children(parts)));

    let reexport_decl = group((
        tok(FROM_KW),
        tok(PATH),
        tok(EXPORT_KW),
        choice((tok(STAR).map(|star| vec![star]), import_list().map(|list| vec![list]))),
        tok(NEWLINE).or_not(),
    ))
    .map(|parts| Tree::new(REEXPORT_DECL, children(parts)));

    let use_decl = group((tok(USE_KW), tok(PATH), tok(NEWLINE).or_not()))
        .map(|parts| Tree::new(USE_DECL, children(parts)));

    let export_decl = group((tok(EXPORT_KW), tok(IDENT), tok(NEWLINE).or_not()))
        .map(|parts| Tree::new(EXPORT_DECL, children(parts)));

    let blank = tok(BLANK).map(|token| Tree::new(BLANK_LINE, vec![token]));

    let item = choice((
        import_decl,
        reexport_decl,
        use_decl,
        anchor_decl,
        var_decl,
        export_decl,
        blank,
        error_line(),
        kind_in(&[INDENT, DEDENT, CONTINUE, NEWLINE, BLANK]).map(|token| Tree::new(ERROR, vec![token])),
    ));

    item.repeated()
        .collect::<Vec<Child>>()
        .map(|items| Tree { kind: ROOT, children: items })
}

/// An indented run of block entries.
fn block_parser<'a, I: TokenInput<'a>>() -> impl Parser<'a, I, Child, Extra<'a>> + Clone {
    recursive(|block| {
        let value = expr::value();

        let property = choice((
            group((
                tok(IDENT),
                type_annotation().repeated().collect::<Vec<Child>>(),
                tok(COLON),
                value.clone().or_not(),
                tok(NEWLINE).or_not(),
                block.clone().or_not(),
            ))
            .map(children),
            // `name:: type` with no `:` declares an abstract property.
            group((
                tok(IDENT),
                type_annotation().repeated().at_least(1).collect::<Vec<Child>>(),
                tok(NEWLINE).or_not(),
                block.clone().or_not(),
            ))
            .map(children),
        ))
        .map(|parts| Tree::new(PROPERTY, parts));

        // `- key: value` and `- key:` with a block are a dictionary written as
        // one list element, which is why the property alternative comes first.
        // A line the lexer found lined up under an item's text continues that
        // text, so it belongs to the item rather than to the block around it.
        let continuation = group((tok(CONTINUE), value.clone().or_not(), tok(NEWLINE).or_not()))
            .map(|parts| Tree::new(TEXT_LINE, children(parts)));

        let list_item = choice((
            group((tok(DASH), property.clone())).map(children),
            group((
                tok(DASH),
                value.clone().or_not(),
                tok(NEWLINE).or_not(),
                continuation.repeated().collect::<Vec<Child>>(),
                block.clone().or_not(),
            ))
            .map(children),
        ))
        .map(|parts| Tree::new(LIST_ITEM, parts));

        let spread_item = group((
            kind_in(&[PLUS, PLUS2]),
            value.clone().or_not(),
            tok(NEWLINE).or_not(),
            block.clone().or_not(),
        ))
        .map(|parts| Tree::new(SPREAD_ITEM, children(parts)));

        let text_line = group((value, tok(NEWLINE).or_not()))
            .map(|parts| Tree::new(TEXT_LINE, children(parts)));

        // The lexer has already found where the fence ends, so a code block
        // is its opening fence, its lines, and a closing fence if it has one.
        let code_block = group((
            tok(FENCE),
            tok(CODE).repeated().collect::<Vec<Child>>(),
            tok(FENCE).or_not(),
            tok(NEWLINE).or_not(),
        ))
        .map(|parts| Tree::new(CODE_BLOCK, children(parts)));

        let blank = tok(BLANK).map(|token| Tree::new(BLANK_LINE, vec![token]));

        let entry =
            choice((property, list_item, spread_item, code_block, text_line, blank, error_line()));

        group((
            tok(INDENT),
            entry.repeated().at_least(1).collect::<Vec<Child>>(),
            tok(DEDENT).or_not(),
        ))
        .map(|parts| Tree::new(BLOCK, children(parts)))
    })
}

/// `extends A, B`
fn extends_clause<'a, I: TokenInput<'a>>() -> impl Parser<'a, I, Child, Extra<'a>> + Clone {
    group((tok(EXTENDS_KW), separated(tok(IDENT), COMMA)))
        .map(|parts| Tree::new(EXTENDS_CLAUSE, children(parts)))
}

/// `as my-keyword`
///
/// A reserved word is accepted here so that it can be reported as a reserved
/// word rather than as an unparsable line.
fn as_clause<'a, I: TokenInput<'a>>() -> impl Parser<'a, I, Child, Extra<'a>> + Clone {
    group((tok(AS_KW), name_like())).map(|parts| Tree::new(AS_CLAUSE, children(parts)))
}

/// An identifier, or a keyword being used where a name belongs.
fn name_like<'a, I: TokenInput<'a>>() -> impl Parser<'a, I, Child, Extra<'a>> + Clone {
    kind_in(&[
        IDENT, ANCHOR_KW, ABSTRACT_KW, EXTENDS_KW, AS_KW, EXPORT_KW, FROM_KW, IMPORT_KW, USE_KW,
        THIS_KW, SELF_KW, SUPER_KW, TRUE_KW, FALSE_KW, NULL_KW,
    ])
}

/// `:: Type`
fn type_annotation<'a, I: TokenInput<'a>>() -> impl Parser<'a, I, Child, Extra<'a>> + Clone {
    group((tok(COLON2), type_ref())).map(|parts| Tree::new(TYPE_ANNOTATION, children(parts)))
}

/// `Name`, `Name[]`, `extends Name`, `extends Name[]`
fn type_ref<'a, I: TokenInput<'a>>() -> impl Parser<'a, I, Child, Extra<'a>> + Clone {
    let base = kind_in(&[IDENT, ANCHOR_KW, NULL_KW])
        .map(|token| Tree::new(TYPE_REF, vec![token]))
        .foldl(tok(L_BRACK).then(tok(R_BRACK)).repeated(), |inner, (open, close)| {
            Tree::new(TYPE_LIST, vec![inner, open, close])
        });
    let inheriting = group((tok(EXTENDS_KW), base.clone()))
        .map(|parts| Tree::new(TYPE_EXTENDS, children(parts)));
    choice((inheriting, base))
}

/// `a, b Alias, c`
fn import_list<'a, I: TokenInput<'a>>() -> impl Parser<'a, I, Child, Extra<'a>> + Clone {
    let item = group((tok(IDENT), tok(IDENT).or_not()))
        .map(|parts| Tree::new(IMPORT_ITEM, children(parts)));
    separated(item, COMMA).map(|items| Tree::new(IMPORT_LIST, items))
}

/// Anything the grammar could not classify, up to the end of the line.
fn error_line<'a, I: TokenInput<'a>>() -> impl Parser<'a, I, Child, Extra<'a>> + Clone {
    let junk = any()
        .filter(|token: &Tok| !matches!(token.kind, NEWLINE | BLANK | INDENT | DEDENT | CONTINUE))
        .map(|token: Tok| Child::Token(token.index));
    group((junk.repeated().at_least(1).collect::<Vec<Child>>(), tok(NEWLINE).or_not()))
        .map(|parts| Tree::new(ERROR, children(parts)))
}
