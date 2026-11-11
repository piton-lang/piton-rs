//! This module keeps all the lexical facts of the language in one location.
//!
//! The editor support is not in this crate. It includes a TextMate grammar, a
//! Vim syntax file, a tree-sitter grammar and more. Each of these must know the
//! same list of keywords. This module keeps that list. Tests compare the shipped
//! grammars with this module. Thus, when a person adds a keyword to the
//! compiler, the tests make sure that the grammars also get the keyword.

/// The file extension of Piton source files.
pub const EXTENSION: &str = "pi";

/// The project configuration filename.
pub const CONFIG_FILE: &str = "piton.config.pi";

/// Words that introduce or modify a declaration.
pub const DECLARATION_KEYWORDS: &[&str] = &[
    "use", "from", "import", "export", "anchor", "abstract", "as", "extends", "pass",
];

/// Literal values.
pub const LITERALS: &[&str] = &["true", "false", "null"];

/// Words that have a meaning only in an expression.
pub const EXPRESSION_KEYWORDS: &[&str] = &["this", "self", "super"];

/// Type names that a `::` constraint can use. They are contextual words.
/// Outside a constraint, they are usual words. Each of them can be a property key.
pub const TYPE_NAMES: &[&str] = &[
    "string",
    "number",
    "boolean",
    "null",
    "list",
    "dictionary",
    "anchor",
    "reference",
    "any",
    "simple",
    "complex",
];

/// The prefix of a comment. Piton has no block comments.
pub const COMMENT_PREFIX: &str = "//";

/// All the words that an editor must colour as keywords.
///
/// A syntax definition needs this list. It is the union of all the words that
/// have a meaning, in all contexts.
pub fn highlight_keywords() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = DECLARATION_KEYWORDS
        .iter()
        .chain(LITERALS)
        .chain(EXPRESSION_KEYWORDS)
        .copied()
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// All the words that have a meaning in some context. The list includes type names.
pub fn all_keywords() -> Vec<&'static str> {
    let mut out = highlight_keywords();
    out.extend(TYPE_NAMES);
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::RESERVED_WORDS;

    #[test]
    fn the_reserved_words_are_accounted_for() {
        // An editor must colour each word that the parser refuses as other
        // syntax. If not, the editor and the parser do not agree about the language.
        let highlighted = highlight_keywords();
        for word in RESERVED_WORDS {
            assert!(
                highlighted.contains(word),
                "`{word}` is reserved but is not in the highlight list"
            );
        }
    }

    #[test]
    fn keyword_lists_are_sorted_and_unique() {
        let all = all_keywords();
        let mut sorted = all.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(all, sorted);
    }
}
