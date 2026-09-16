//! The single source of truth for every token and node kind in Piton.
//!
//! Everything downstream — the parser, the formatter, the language server's
//! semantic tokens and the generated editor grammars — is derived from the
//! tables in this module so that they cannot drift apart.

/// Every terminal and non-terminal kind in a Piton syntax tree.
///
/// Values below [`SyntaxKind::__LAST_TOKEN`] are tokens; the rest are nodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
#[allow(non_camel_case_types)]
pub enum SyntaxKind {
    // ---- trivia -------------------------------------------------------
    /// Spaces, tabs, and newlines that carry no structure.
    WHITESPACE = 0,
    /// A `// ...` line comment.
    COMMENT,

    // ---- structural virtual tokens (zero width, never in the tree) ----
    /// Start of a deeper indentation block.
    INDENT,
    /// End of an indentation block.
    DEDENT,
    /// The newline terminating a content line.
    NEWLINE,
    /// The newline of an empty line; significant inside string blocks.
    BLANK,

    // ---- literals & names ---------------------------------------------
    /// An identifier in code position.
    IDENT,
    /// A numeric literal.
    NUMBER,
    /// A `"..."` string literal.
    QUOTED_STRING,
    /// A run of prose in text position.
    TEXT,
    /// A `\x` escape sequence.
    ESCAPE,
    /// An import path such as `./Foo`, `/a/b`, or `@piton/belay`.
    PATH,
    /// The `$`, `@`, or `name` preceding an interpolation's `{`.
    SIGIL,
    /// The opening or closing line of a fenced code block, such as ```` ```css ````.
    FENCE,
    /// One line inside a fenced code block, kept exactly as written.
    CODE,

    // ---- keywords ------------------------------------------------------
    ANCHOR_KW,
    ABSTRACT_KW,
    EXTENDS_KW,
    AS_KW,
    EXPORT_KW,
    FROM_KW,
    IMPORT_KW,
    USE_KW,
    THIS_KW,
    SELF_KW,
    SUPER_KW,
    TRUE_KW,
    FALSE_KW,
    NULL_KW,

    // ---- punctuation & operators ---------------------------------------
    COLON,
    COLON2,
    COMMA,
    DOT,
    STAR,
    L_BRACK,
    R_BRACK,
    L_BRACE,
    R_BRACE,
    L_PAREN,
    R_PAREN,
    DASH,
    PLUS,
    PLUS2,
    MINUS,
    SLASH,
    PERCENT,
    EQ2,
    BANG_EQ,
    GT,
    LT,
    GT_EQ,
    LT_EQ,
    AMP2,
    PIPE2,
    QUESTION,

    /// A character the lexer could not classify.
    ERROR_TOKEN,

    /// Marker: everything at or below this discriminant is a token.
    __LAST_TOKEN,

    // ---- nodes ----------------------------------------------------------
    /// The whole file.
    ROOT,
    /// `export`? `abstract`? `anchor Name extends A, B as kw:` + body.
    ANCHOR_DECL,
    /// `export`? `name:: T: value` at the top level.
    VAR_DECL,
    /// `from PATH import a, b Alias`.
    IMPORT_DECL,
    /// `from PATH export a, b` / `from PATH export *`.
    REEXPORT_DECL,
    /// `export Name` re-exporting an already bound name.
    EXPORT_DECL,
    /// `use PATH`.
    USE_DECL,
    /// `extends A, B`.
    EXTENDS_CLAUSE,
    /// `as my-keyword`.
    AS_CLAUSE,
    /// The bracketed or bare list of imported/exported names.
    IMPORT_LIST,
    /// One `Name` or `Name Alias` entry in an import list.
    IMPORT_ITEM,
    /// `:: T` attached to a key.
    TYPE_ANNOTATION,
    /// A named type such as `string` or `MyAnchor`.
    TYPE_REF,
    /// `T[]`.
    TYPE_LIST,
    /// `extends T`.
    TYPE_EXTENDS,
    /// An indented block of entries.
    BLOCK,
    /// `key:: T: value` inside a block.
    PROPERTY,
    /// `- value`.
    LIST_ITEM,
    /// `+ value` or `++ value`.
    SPREAD_ITEM,
    /// A line of prose inside a block.
    TEXT_LINE,
    /// A fenced code block inside a block, from its opening fence to its closing one.
    CODE_BLOCK,
    /// An empty line.
    BLANK_LINE,
    /// The value region following a `:`, `-`, or `+`.
    VALUE,
    /// Prose, possibly containing interpolations.
    TEXT_VALUE,
    /// `sigil{expr}` inside prose.
    INTERPOLATION,
    /// `{expr}` used as an expression atom.
    BRACE_EXPR,
    /// A binary operation.
    BIN_EXPR,
    /// A prefix operation.
    UNARY_EXPR,
    /// `cond ? a : b`.
    TERNARY_EXPR,
    /// `( expr )`.
    PAREN_EXPR,
    /// `[a, b, c]`.
    ARRAY_EXPR,
    /// `a.b.c`.
    FIELD_EXPR,
    /// A reference to a name inside an expression.
    NAME_REF,
    /// A literal atom.
    LITERAL,
    /// A region the parser could not understand.
    ERROR,
}


/// Every kind, in discriminant order, so raw values can be recovered safely.
pub const ALL_KINDS: &[SyntaxKind] = &[
    SyntaxKind::WHITESPACE,
    SyntaxKind::COMMENT,
    SyntaxKind::INDENT,
    SyntaxKind::DEDENT,
    SyntaxKind::NEWLINE,
    SyntaxKind::BLANK,
    SyntaxKind::IDENT,
    SyntaxKind::NUMBER,
    SyntaxKind::QUOTED_STRING,
    SyntaxKind::TEXT,
    SyntaxKind::ESCAPE,
    SyntaxKind::PATH,
    SyntaxKind::SIGIL,
    SyntaxKind::FENCE,
    SyntaxKind::CODE,
    SyntaxKind::ANCHOR_KW,
    SyntaxKind::ABSTRACT_KW,
    SyntaxKind::EXTENDS_KW,
    SyntaxKind::AS_KW,
    SyntaxKind::EXPORT_KW,
    SyntaxKind::FROM_KW,
    SyntaxKind::IMPORT_KW,
    SyntaxKind::USE_KW,
    SyntaxKind::THIS_KW,
    SyntaxKind::SELF_KW,
    SyntaxKind::SUPER_KW,
    SyntaxKind::TRUE_KW,
    SyntaxKind::FALSE_KW,
    SyntaxKind::NULL_KW,
    SyntaxKind::COLON,
    SyntaxKind::COLON2,
    SyntaxKind::COMMA,
    SyntaxKind::DOT,
    SyntaxKind::STAR,
    SyntaxKind::L_BRACK,
    SyntaxKind::R_BRACK,
    SyntaxKind::L_BRACE,
    SyntaxKind::R_BRACE,
    SyntaxKind::L_PAREN,
    SyntaxKind::R_PAREN,
    SyntaxKind::DASH,
    SyntaxKind::PLUS,
    SyntaxKind::PLUS2,
    SyntaxKind::MINUS,
    SyntaxKind::SLASH,
    SyntaxKind::PERCENT,
    SyntaxKind::EQ2,
    SyntaxKind::BANG_EQ,
    SyntaxKind::GT,
    SyntaxKind::LT,
    SyntaxKind::GT_EQ,
    SyntaxKind::LT_EQ,
    SyntaxKind::AMP2,
    SyntaxKind::PIPE2,
    SyntaxKind::QUESTION,
    SyntaxKind::ERROR_TOKEN,
    SyntaxKind::__LAST_TOKEN,
    SyntaxKind::ROOT,
    SyntaxKind::ANCHOR_DECL,
    SyntaxKind::VAR_DECL,
    SyntaxKind::IMPORT_DECL,
    SyntaxKind::REEXPORT_DECL,
    SyntaxKind::EXPORT_DECL,
    SyntaxKind::USE_DECL,
    SyntaxKind::EXTENDS_CLAUSE,
    SyntaxKind::AS_CLAUSE,
    SyntaxKind::IMPORT_LIST,
    SyntaxKind::IMPORT_ITEM,
    SyntaxKind::TYPE_ANNOTATION,
    SyntaxKind::TYPE_REF,
    SyntaxKind::TYPE_LIST,
    SyntaxKind::TYPE_EXTENDS,
    SyntaxKind::BLOCK,
    SyntaxKind::PROPERTY,
    SyntaxKind::LIST_ITEM,
    SyntaxKind::SPREAD_ITEM,
    SyntaxKind::TEXT_LINE,
    SyntaxKind::CODE_BLOCK,
    SyntaxKind::BLANK_LINE,
    SyntaxKind::VALUE,
    SyntaxKind::TEXT_VALUE,
    SyntaxKind::INTERPOLATION,
    SyntaxKind::BRACE_EXPR,
    SyntaxKind::BIN_EXPR,
    SyntaxKind::UNARY_EXPR,
    SyntaxKind::TERNARY_EXPR,
    SyntaxKind::PAREN_EXPR,
    SyntaxKind::ARRAY_EXPR,
    SyntaxKind::FIELD_EXPR,
    SyntaxKind::NAME_REF,
    SyntaxKind::LITERAL,
    SyntaxKind::ERROR,
];

impl SyntaxKind {
    /// Recover a kind from its raw rowan discriminant.
    pub fn from_raw(raw: u16) -> SyntaxKind {
        ALL_KINDS[raw as usize]
    }

    /// True when this kind labels a token rather than a node.
    pub fn is_token(self) -> bool {
        (self as u16) < SyntaxKind::__LAST_TOKEN as u16
    }

    /// True for tokens that carry no structure and are skipped by the parser.
    pub fn is_trivia(self) -> bool {
        matches!(self, SyntaxKind::WHITESPACE | SyntaxKind::COMMENT)
    }

    /// True for the zero-width markers that never reach the syntax tree.
    pub fn is_virtual(self) -> bool {
        matches!(self, SyntaxKind::INDENT | SyntaxKind::DEDENT)
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind as u16)
    }
}

/// Reserved words that may never be used as a user-defined keyword.
pub const RESERVED_KEYWORDS: &[&str] = &[
    "anchor", "abstract", "extends", "as", "export", "from", "import", "use", "this", "self",
    "super", "true", "false", "null",
];

/// The built-in type names usable in a `::` constraint.
pub const BUILTIN_TYPES: &[&str] = &[
    "string",
    "number",
    "boolean",
    "null",
    "list",
    "dictionary",
    "anchor",
    "any",
    "simple",
    "complex",
];

/// The literal constants of the language.
pub const LITERAL_KEYWORDS: &[&str] = &["true", "false", "null"];

/// The keywords that introduce or modify a declaration.
pub const DECLARATION_KEYWORDS: &[&str] =
    &["anchor", "abstract", "export", "from", "import", "use", "extends", "as"];

/// The self-reference keywords available inside anchors.
pub const SELF_KEYWORDS: &[&str] = &["this", "self", "super"];

/// Every operator spelling, longest first so that greedy matching works.
pub const OPERATORS: &[&str] = &[
    "++", "::", "==", "!=", ">=", "<=", "&&", "||", "+", "-", "*", "/", "%", ">", "<", "?", ":",
    ".", ",",
];

/// Maps a keyword spelling to its token kind.
pub fn keyword_kind(text: &str) -> Option<SyntaxKind> {
    Some(match text {
        "anchor" => SyntaxKind::ANCHOR_KW,
        "abstract" => SyntaxKind::ABSTRACT_KW,
        "extends" => SyntaxKind::EXTENDS_KW,
        "as" => SyntaxKind::AS_KW,
        "export" => SyntaxKind::EXPORT_KW,
        "from" => SyntaxKind::FROM_KW,
        "import" => SyntaxKind::IMPORT_KW,
        "use" => SyntaxKind::USE_KW,
        "this" => SyntaxKind::THIS_KW,
        "self" => SyntaxKind::SELF_KW,
        "super" => SyntaxKind::SUPER_KW,
        "true" => SyntaxKind::TRUE_KW,
        "false" => SyntaxKind::FALSE_KW,
        "null" => SyntaxKind::NULL_KW,
        _ => return None,
    })
}
