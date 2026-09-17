//! Syntax for the Piton language: lexer, chumsky grammar, rowan CST, and a
//! typed AST layer.
//!
//! Everything above this crate — the evaluator, the formatter, the language
//! server, and the generated editor grammars — works from the tree and the
//! kind tables defined here, so there is exactly one description of Piton's
//! surface syntax in the project.

pub mod ast;
pub mod kind;
pub mod lexer;
pub mod loc;
pub mod parser;

pub use kind::SyntaxKind;
pub use lexer::{
    escape_group_text, identifier_len, in_prose_run, lex, IndentStyle, LexError, LexToken, Lexed,
};
pub use parser::{parse, Parse};
pub use rowan::{self, NodeOrToken, TextRange, TextSize};

/// Whether `text` is exactly one identifier, as the lexer reads one.
///
/// Anything that writes a name into a file, such as a rename, asks this rather
/// than keeping its own idea of what a name may contain.
pub fn is_identifier(text: &str) -> bool {
    identifier_len(text) == Some(text.len())
}

/// The rowan language marker for Piton.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PitonLanguage {}

impl rowan::Language for PitonLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        SyntaxKind::from_raw(raw.0)
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<PitonLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<PitonLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<PitonLanguage>;
pub type SyntaxNodeChildren = rowan::SyntaxNodeChildren<PitonLanguage>;

impl Parse {
    /// The root of the parsed file.
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    /// The typed root of the parsed file.
    pub fn root(&self) -> ast::Root {
        ast::Root { syntax: self.syntax() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(src: &str) {
        let parsed = parse(src);
        assert_eq!(parsed.syntax().text().to_string(), src, "lossless round trip");
    }

    #[test]
    fn round_trips_every_construct() {
        round_trip("myVariable: 42 // trailing\n");
        round_trip("anchor A:\n    name: Base\n\n    other:\n        - a\n        - b\n");
        round_trip("from ./x import A, B Alias\nuse ./kw\nexport pi: 3.14\n");
        round_trip("a:: string:: number: 42\n");
        round_trip("combined:\n    text\n\n    - one\n    - two\n\n    nested:\n        deep: v\n");
        round_trip("x: {a + b} && false\ny: Hello, ${name}!\n");
        round_trip("");
        round_trip("\n\n");
    }

    #[test]
    fn an_identifier_is_what_the_lexer_reads_as_one() {
        for name in ["a", "Tool", "_private", "kebab-case", "a--b", "héllo", "x1"] {
            assert!(is_identifier(name), "{name}");
        }
        for name in ["", "1x", "trailing-", "-leading", "two words", "a.b", "a:"] {
            assert!(!is_identifier(name), "{name}");
        }
    }

    #[test]
    fn number_and_prose_are_distinguished() {
        let parsed = parse("a: 42\nb: 42 things\nc: 3.14 - 3.14\nd: well-known thing\n");
        let text = format!("{:?}", parsed.syntax());
        assert!(text.contains("ROOT"));
    }
}
