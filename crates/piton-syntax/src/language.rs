//! The lexical facts of the language, in one place.
//!
//! Editor support lives outside this crate — a TextMate grammar, a Vim syntax
//! file, a tree-sitter grammar, and more — and every one of them has to know the
//! same list of keywords. Listing them here and testing the shipped grammars
//! against this module is what stops a keyword being added to the compiler and
//! forgotten everywhere else.

/// The file extension Piton sources carry.
pub const EXTENSION: &str = "pi";

/// The filename that makes a directory a module.
pub const MODULE_FILE: &str = "index.pi";

/// The project configuration filename.
pub const CONFIG_FILE: &str = "piton.config.pi";

/// Words that introduce or modify a declaration.
pub const DECLARATION_KEYWORDS: &[&str] = &[
    "use", "from", "import", "export", "anchor", "abstract", "as", "extends", "pass",
];

/// Literal values.
pub const LITERALS: &[&str] = &["true", "false", "null"];

/// Words that only mean something inside an expression.
pub const EXPRESSION_KEYWORDS: &[&str] = &["this", "self", "super"];

/// Type names usable in a `::` constraint. These are contextual: outside a
/// constraint they are ordinary words, and any of them may be a property key.
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

/// The interpolation sigils, longest first so a matcher tries `${` before `{`.
pub const SIGILS: &[&str] = &["${", "#{", "@{", "{"];

/// Operators, longest first.
pub const OPERATORS: &[&str] = &[
    "++", "==", "!=", "<=", ">=", "&&", "||", "::", "+", "-", "*", "/", "%", "<", ">", "!", "?",
    ":", ".",
];

/// How a comment begins. There is no block form.
pub const COMMENT_PREFIX: &str = "//";

/// Every word an editor should colour as a keyword.
///
/// This is what a syntax definition needs: the union of the words that carry
/// meaning, whatever context they carry it in.
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

/// Every word that carries meaning anywhere, including type names.
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
        // Every word the parser refuses as something else has to be a word an
        // editor colours, or the two disagree about what the language is.
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

    #[test]
    fn sigils_are_ordered_longest_first() {
        // A matcher that tried `{` first would never see `${`.
        for pair in SIGILS.windows(2) {
            assert!(pair[0].len() >= pair[1].len(), "{pair:?}");
        }
        for pair in OPERATORS.windows(2) {
            assert!(pair[0].len() >= pair[1].len(), "{pair:?}");
        }
    }
}
