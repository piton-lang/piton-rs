//! Syntax kinds for the lossless concrete syntax tree.

/// Every token and node kind rowan can hold.
///
/// The discriminants are stable because rowan stores them as raw `u16`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
#[allow(non_camel_case_types)]
pub enum SyntaxKind {
    // ---- trivia -------------------------------------------------------
    WHITESPACE = 0,
    NEWLINE,
    COMMENT,

    // ---- tokens -------------------------------------------------------
    IDENT,
    NUMBER,
    QUOTED_STRING,
    /// A run of ordinary prose.
    PROSE,
    /// A backslash used as an escape delimiter.
    BACKSLASH,
    /// The literal backtick run that opens or closes a fenced block.
    FENCE_MARK,
    /// Verbatim content inside a fenced block.
    FENCE_TEXT,
    /// A backslash run that delimits a multi-line escape block.
    ESCAPE_MARK,
    /// Literal content inside a multi-line escape block.
    ESCAPE_TEXT,
    PATH,

    COLON,
    COLON_COLON,
    COMMA,
    DOT,
    DASH,
    PLUS,
    PLUS_PLUS,
    STAR,
    SLASH,
    PERCENT,
    BANG,
    QUESTION,
    EQ_EQ,
    BANG_EQ,
    LT,
    LT_EQ,
    GT,
    GT_EQ,
    AMP_AMP,
    PIPE_PIPE,
    L_PAREN,
    R_PAREN,
    L_BRACKET,
    R_BRACKET,
    L_CURLY,
    R_CURLY,
    DOLLAR_CURLY,
    HASH_CURLY,
    AT_CURLY,

    KW_USE,
    KW_FROM,
    KW_IMPORT,
    KW_EXPORT,
    KW_ANCHOR,
    KW_ABSTRACT,
    KW_AS,
    KW_EXTENDS,
    KW_PASS,
    KW_TRUE,
    KW_FALSE,
    KW_NULL,
    KW_THIS,
    KW_SELF,
    KW_SUPER,

    ERROR,

    // ---- nodes --------------------------------------------------------
    SOURCE_FILE,
    USE_DECL,
    FROM_DECL,
    IMPORT_LIST,
    IMPORT_ITEM,
    REEXPORT_DECL,
    ANCHOR_DECL,
    ANCHOR_HEADER,
    KEYWORD_ALIAS,
    EXTENDS_CLAUSE,
    VARIABLE_DECL,
    BLOCK,
    PROPERTY,
    LIST_ITEM,
    MERGE_ITEM,
    PROSE_LINE,
    FENCE,
    ESCAPE_BLOCK,
    TYPE_CONSTRAINT,
    VALUE,
    INLINE_LIST,
    INTERPOLATION,
    EXPR_BINARY,
    EXPR_UNARY,
    EXPR_TERNARY,
    EXPR_PAREN,
    EXPR_FIELD,
    EXPR_LIST,
    EXPR_LITERAL,
    EXPR_NAME,

    /// Must remain last; used to bound the conversion from `u16`.
    __LAST,
}

impl SyntaxKind {
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE | SyntaxKind::COMMENT
        )
    }

    pub fn is_keyword(self) -> bool {
        matches!(
            self,
            SyntaxKind::KW_USE
                | SyntaxKind::KW_FROM
                | SyntaxKind::KW_IMPORT
                | SyntaxKind::KW_EXPORT
                | SyntaxKind::KW_ANCHOR
                | SyntaxKind::KW_ABSTRACT
                | SyntaxKind::KW_AS
                | SyntaxKind::KW_EXTENDS
                | SyntaxKind::KW_PASS
                | SyntaxKind::KW_TRUE
                | SyntaxKind::KW_FALSE
                | SyntaxKind::KW_NULL
                | SyntaxKind::KW_THIS
                | SyntaxKind::KW_SELF
                | SyntaxKind::KW_SUPER
        )
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind as u16)
    }
}

/// Reserved words. These cannot be used as property names; a line such as
/// `null: The absence of a value` is prose, not a property, which is why such
/// lines do not appear as headings in compiled output.
pub const RESERVED_WORDS: &[&str] = &[
    "use", "from", "import", "export", "anchor", "abstract", "as", "extends", "pass", "null",
    "true", "false", "this", "self", "super",
];

/// True when `word` cannot be used as a property or variable name.
pub fn is_reserved(word: &str) -> bool {
    RESERVED_WORDS.contains(&word)
}

/// The rowan language marker for Piton.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Piton {}

impl rowan::Language for Piton {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        assert!(raw.0 < SyntaxKind::__LAST as u16);
        // SAFETY: `SyntaxKind` is `#[repr(u16)]` with contiguous discriminants
        // from 0, and the assertion above bounds the value.
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<Piton>;
pub type SyntaxToken = rowan::SyntaxToken<Piton>;
pub type SyntaxElement = rowan::SyntaxElement<Piton>;

#[cfg(test)]
mod tests {
    use super::*;
    use rowan::Language;

    #[test]
    fn kinds_round_trip() {
        for raw in 0..SyntaxKind::__LAST as u16 {
            let kind = Piton::kind_from_raw(rowan::SyntaxKind(raw));
            assert_eq!(Piton::kind_to_raw(kind).0, raw);
        }
    }

    #[test]
    fn reserved_words_block_property_names() {
        assert!(is_reserved("null"));
        assert!(is_reserved("anchor"));
        assert!(is_reserved("use"));
        assert!(!is_reserved("string"));
        assert!(!is_reserved("description"));
    }
}
