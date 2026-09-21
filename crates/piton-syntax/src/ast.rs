//! The typed syntax tree.
//!
//! The parser produces this alongside a lossless rowan tree. The rowan tree is
//! what the editor walks; this is what the compiler lowers. Keeping them
//! separate means neither has to compromise: the CST keeps every byte, and the
//! AST keeps the shape the resolver actually wants.

use piton_core::Span;

/// A value paired with its source range.
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    pub span: Span,
    pub value: T,
}

impl<T> Spanned<T> {
    pub fn new(span: Span, value: T) -> Self {
        Spanned { span, value }
    }
}

/// A parsed `.pi` file.
#[derive(Debug, Clone, Default)]
pub struct SourceFile {
    pub span: Span,
    pub items: Vec<Item>,
}

impl SourceFile {
    pub fn anchors(&self) -> impl Iterator<Item = &AnchorDecl> {
        self.items.iter().filter_map(|item| match item {
            Item::Anchor(anchor) => Some(anchor),
            _ => None,
        })
    }

    pub fn variables(&self) -> impl Iterator<Item = &VariableDecl> {
        self.items.iter().filter_map(|item| match item {
            Item::Variable(variable) => Some(variable),
            _ => None,
        })
    }
}

/// A top-level declaration.
#[derive(Debug, Clone)]
pub enum Item {
    /// `use ./path` — brings user-defined keywords into scope.
    Use(UseDecl),
    /// `from ./path import A, B` or `from ./path export *`.
    From(FromDecl),
    /// An anchor declaration, including ones written with a user keyword.
    Anchor(AnchorDecl),
    /// A top-level variable, optionally exported.
    Variable(VariableDecl),
    /// `export Name` — re-exports an already bound symbol.
    ReExport(ReExportDecl),
}

impl Item {
    pub fn span(&self) -> Span {
        match self {
            Item::Use(decl) => decl.span,
            Item::From(decl) => decl.span,
            Item::Anchor(decl) => decl.span,
            Item::Variable(decl) => decl.span,
            Item::ReExport(decl) => decl.span,
        }
    }
}

/// A module path as written in source: `./Foo`, `../bar`, `/abs/path`,
/// `@piton/belay`, or `.` for the current directory's module.
#[derive(Debug, Clone, PartialEq)]
pub struct ModulePath {
    pub span: Span,
    pub text: String,
}

impl ModulePath {
    pub fn is_package(&self) -> bool {
        self.text.starts_with('@')
    }

    pub fn is_relative(&self) -> bool {
        self.text.starts_with('.')
    }

    pub fn is_absolute(&self) -> bool {
        self.text.starts_with('/')
    }
}

#[derive(Debug, Clone)]
pub struct UseDecl {
    pub span: Span,
    pub path: ModulePath,
}

/// Whether a `from` declaration binds names locally or re-exports them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FromKind {
    Import,
    Export,
}

#[derive(Debug, Clone)]
pub struct FromDecl {
    pub span: Span,
    pub path: ModulePath,
    pub kind: FromKind,
    /// True for `from ./X export *`.
    pub star: bool,
    pub items: Vec<ImportItem>,
}

/// One entry in an import or export list. `alias` holds the optional rename
/// that follows the name: `from ./F import pi SliceOf`.
#[derive(Debug, Clone)]
pub struct ImportItem {
    pub span: Span,
    pub name: String,
    pub name_span: Span,
    pub alias: Option<Spanned<String>>,
}

impl ImportItem {
    /// The name the symbol is bound under locally.
    pub fn local_name(&self) -> &str {
        match &self.alias {
            Some(alias) => &alias.value,
            None => &self.name,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReExportDecl {
    pub span: Span,
    pub name: String,
    pub name_span: Span,
}

#[derive(Debug, Clone)]
pub struct AnchorDecl {
    pub span: Span,
    pub exported: bool,
    pub is_abstract: bool,
    /// `anchor`, or the user-defined keyword that introduced this declaration.
    pub keyword: String,
    pub keyword_span: Span,
    pub name: String,
    pub name_span: Span,
    /// `as my-keyword` — declares a user keyword aliasing this anchor.
    pub alias: Option<Spanned<String>>,
    /// Bases in source order. A user keyword contributes the aliased anchor as
    /// the leftmost base.
    pub extends: Vec<Spanned<String>>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct VariableDecl {
    pub span: Span,
    pub exported: bool,
    pub name: String,
    pub name_span: Span,
    pub constraints: Vec<TypeConstraint>,
    pub value: ValueNode,
}

/// One `:: type` annotation. Multiple constraints are tried left to right.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeConstraint {
    pub span: Span,
    /// `extends A` — satisfied by any anchor whose inheritance chain includes
    /// `A`. Only meaningful inside abstract declarations.
    pub extends: bool,
    pub name: TypeName,
    /// `T[]` — a list whose elements each satisfy `T`.
    pub list: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeName {
    String,
    Number,
    Boolean,
    Null,
    List,
    Dictionary,
    Anchor,
    Reference,
    Any,
    Simple,
    Complex,
    /// A reference to a declared anchor used as a type.
    Named(String),
}

impl TypeName {
    pub fn from_ident(ident: &str) -> TypeName {
        match ident {
            "string" => TypeName::String,
            "number" => TypeName::Number,
            "boolean" => TypeName::Boolean,
            "null" => TypeName::Null,
            "list" => TypeName::List,
            "dictionary" => TypeName::Dictionary,
            "anchor" => TypeName::Anchor,
            "reference" => TypeName::Reference,
            "any" => TypeName::Any,
            "simple" => TypeName::Simple,
            "complex" => TypeName::Complex,
            other => TypeName::Named(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            TypeName::String => "string",
            TypeName::Number => "number",
            TypeName::Boolean => "boolean",
            TypeName::Null => "null",
            TypeName::List => "list",
            TypeName::Dictionary => "dictionary",
            TypeName::Anchor => "anchor",
            TypeName::Reference => "reference",
            TypeName::Any => "any",
            TypeName::Simple => "simple",
            TypeName::Complex => "complex",
            TypeName::Named(name) => name,
        }
    }
}

/// A value: an optional same-line part, an optional inline list, and an
/// optional indented block. A property can carry a same-line fragment *and* an
/// indented continuation, and both contribute to the result.
#[derive(Debug, Clone, Default)]
pub struct ValueNode {
    pub span: Span,
    /// Text written on the same line as the `:`.
    pub inline: Option<ProseLine>,
    /// `[a, b, c]` written on the same line as the `:`.
    pub inline_list: Option<Vec<ValueNode>>,
    /// Content indented beneath the declaration.
    pub block: Option<Block>,
    /// True when the source actually supplied a value, even an empty one.
    ///
    /// `description:` declares a property whose value is null; `description::
    /// string` with no colon declares an abstract slot with no value at all.
    /// The two look similar and mean different things.
    pub declared: bool,
}

impl ValueNode {
    pub fn is_empty(&self) -> bool {
        self.inline.is_none() && self.inline_list.is_none() && self.block.is_none()
    }
}

/// An indented run of lines belonging to one declaration.
#[derive(Debug, Clone, Default)]
pub struct Block {
    pub span: Span,
    pub items: Vec<BlockItem>,
}

impl Block {
    pub fn properties(&self) -> impl Iterator<Item = &Property> {
        self.items.iter().filter_map(|item| match item {
            BlockItem::Property(property) => Some(property),
            _ => None,
        })
    }
}

#[derive(Debug, Clone)]
pub enum BlockItem {
    /// `key: value`, possibly with type constraints.
    Property(Property),
    /// `- value`
    ListItem(ListItem),
    /// `+ {expr}` or `++ {expr}`
    Merge(MergeItem),
    /// A run of prose lines terminated by a blank line or a structural line.
    Prose(Paragraph),
    /// A fenced code block. Its contents are verbatim: no comments, no escapes,
    /// and no expression interpolation.
    Fence(Fence),
    /// A multi-line escape block. Its contents are literal and its delimiters
    /// are consumed.
    Escape(EscapeBlock),
    /// `pass` — an intentionally empty body.
    Pass(Span),
}

impl BlockItem {
    pub fn span(&self) -> Span {
        match self {
            BlockItem::Property(property) => property.span,
            BlockItem::ListItem(item) => item.span,
            BlockItem::Merge(merge) => merge.span,
            BlockItem::Prose(paragraph) => paragraph.span,
            BlockItem::Fence(fence) => fence.span,
            BlockItem::Escape(block) => block.span,
            BlockItem::Pass(span) => *span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Property {
    pub span: Span,
    pub name: String,
    pub name_span: Span,
    pub constraints: Vec<TypeConstraint>,
    pub value: ValueNode,
}

#[derive(Debug, Clone)]
pub struct ListItem {
    pub span: Span,
    pub value: ValueNode,
}

/// `+` merges with deduplication; `++` concatenates and keeps duplicates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeOp {
    Merge,
    Concat,
}

#[derive(Debug, Clone)]
pub struct MergeItem {
    pub span: Span,
    pub op: MergeOp,
    pub value: ProseLine,
}

/// Prose lines separated only by line breaks. A blank line starts a new
/// paragraph, which is how the language spells an explicit line break.
#[derive(Debug, Clone, Default)]
pub struct Paragraph {
    pub span: Span,
    pub lines: Vec<ProseLine>,
}

#[derive(Debug, Clone, Default)]
pub struct ProseLine {
    pub span: Span,
    pub segments: Vec<ProseSegment>,
}

impl ProseLine {
    /// The single interpolation this line consists of, if that is all it holds.
    ///
    /// A value written as exactly one `{...}` keeps its intrinsic type; anything
    /// else is a string.
    pub fn sole_interpolation(&self) -> Option<&Interpolation> {
        let mut found = None;
        for segment in &self.segments {
            match segment {
                ProseSegment::Text(text) if text.trim().is_empty() => {}
                ProseSegment::Interpolation(interp) if found.is_none() => found = Some(interp),
                _ => return None,
            }
        }
        found
    }

    pub fn is_blank(&self) -> bool {
        self.segments.iter().all(
            |segment| matches!(segment, ProseSegment::Text(text) if text.trim().is_empty()),
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProseSegment {
    /// Ordinary text. Escapes have already been applied.
    Text(String),
    /// Text that was written inside a backslash-delimited escape and must not be
    /// reinterpreted.
    Literal(String),
    Interpolation(Interpolation),
}

/// Which conversion an interpolation requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sigil {
    /// `{...}` — the intrinsic value.
    Standard,
    /// `${...}` — converted to a string.
    Stringify,
    /// `#{...}` — converted to a number.
    Numeric,
    /// `@{...}` — a reference that keeps the anchor's identity.
    Reference,
}

impl Sigil {
    pub fn prefix(self) -> &'static str {
        match self {
            Sigil::Standard => "{",
            Sigil::Stringify => "${",
            Sigil::Numeric => "#{",
            Sigil::Reference => "@{",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Interpolation {
    pub span: Span,
    pub sigil: Sigil,
    pub expr: Expr,
}

/// A run of lines delimited by backslash-only lines.
///
/// The delimiters are syntax: they are consumed, and what sits between them is
/// taken exactly as written.
#[derive(Debug, Clone)]
pub struct EscapeBlock {
    pub span: Span,
    /// Content lines with the block's base indentation removed.
    pub lines: Vec<String>,
    /// Length of the backslash run that delimits the block.
    pub run: usize,
}

#[derive(Debug, Clone)]
pub struct Fence {
    pub span: Span,
    /// The language tag written after the opening backticks.
    pub info: String,
    /// Content lines with the block's base indentation removed.
    pub lines: Vec<String>,
    /// The number of backticks used, so the closing fence can be reproduced.
    pub ticks: usize,
}

/// An expression inside an interpolation or an inline list.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub span: Span,
    pub kind: ExprKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Number(f64),
    Bool(bool),
    Null,
    /// A quoted string. Quoted values are explicitly strings and never coerce.
    Quoted(String),
    /// A bare identifier resolved in the enclosing scope.
    Name(String),
    /// `this` — the anchor the expression is written in.
    This,
    /// `self` — the most derived anchor in the inheritance chain.
    SelfRef,
    /// `super` — the right-most base of the anchor the expression is written in.
    Super,
    Field(Box<Expr>, Spanned<String>),
    Unary(UnaryOp, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    List(Vec<Expr>),
    Paren(Box<Expr>),
    /// Produced by error recovery so later phases can keep going.
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Negate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    /// `++` — concatenate without deduplication.
    Concat,
}

impl BinaryOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Subtract => "-",
            BinaryOp::Multiply => "*",
            BinaryOp::Divide => "/",
            BinaryOp::Modulo => "%",
            BinaryOp::Equal => "==",
            BinaryOp::NotEqual => "!=",
            BinaryOp::Less => "<",
            BinaryOp::LessEqual => "<=",
            BinaryOp::Greater => ">",
            BinaryOp::GreaterEqual => ">=",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
            BinaryOp::Concat => "++",
        }
    }
}
