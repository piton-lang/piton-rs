//! The typed syntax tree.
//!
//! The parser makes this tree together with a lossless rowan tree. The editor
//! walks the rowan tree. The compiler lowers this tree. The two trees are
//! separate. Thus each tree can have its best design. The CST keeps each byte.
//! The AST keeps the shape that the resolver needs.

use piton_core::Span;

/// A value together with its source range.
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
    /// `use ./path`. It brings user-defined keywords into scope.
    Use(UseDecl),
    /// `from ./path import A, B` or `from ./path export *`.
    From(FromDecl),
    /// An anchor declaration. This includes declarations with a user keyword.
    Anchor(AnchorDecl),
    /// A top-level variable. An export of the variable is optional.
    Variable(VariableDecl),
    /// `export Name`. It re-exports a symbol that already has a binding.
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

/// A module path as the source shows it. Examples are `./Foo`, `../bar`,
/// `/abs/path`, `@piton/belay`, and `.` for the module of the current directory.
#[derive(Debug, Clone, PartialEq)]
pub struct ModulePath {
    pub span: Span,
    pub text: String,
}

impl ModulePath {
    pub fn is_package(&self) -> bool {
        self.text.starts_with('@')
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

/// Tells if a `from` declaration binds names locally or re-exports them.
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

/// One entry in an import list or an export list. `alias` holds the optional
/// new name that follows the name, as in `from ./F import pi SliceOf`.
#[derive(Debug, Clone)]
pub struct ImportItem {
    pub span: Span,
    pub name: String,
    pub name_span: Span,
    pub alias: Option<Spanned<String>>,
}

impl ImportItem {
    /// The local name of the symbol.
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
    /// `anchor`, or the user-defined keyword that starts this declaration.
    pub keyword: String,
    pub keyword_span: Span,
    pub name: String,
    pub name_span: Span,
    /// `as my-keyword`. It declares a user keyword that is an alias of this anchor.
    pub alias: Option<Spanned<String>>,
    /// The bases in source order. The resolver adds the anchor of a user keyword
    /// after these bases. That anchor is the last base.
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

/// One `:: type` annotation. The compiler tries more than one constraint
/// from left to right.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeConstraint {
    pub span: Span,
    /// `extends A`. An anchor satisfies it when the inheritance chain of the
    /// anchor includes `A`. It has a meaning only in abstract declarations.
    pub extends: bool,
    pub name: TypeName,
    /// `T[]`. A list where each element satisfies `T`.
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
    /// A reference to a declared anchor. The anchor is the type.
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

/// A value. It has an optional same-line part, an optional inline list and an
/// optional indented block. A property can have a same-line fragment *and* an
/// indented continuation. Both parts go into the result.
#[derive(Debug, Clone, Default)]
pub struct ValueNode {
    pub span: Span,
    /// The text on the same line as the `:`.
    pub inline: Option<ProseLine>,
    /// A `[a, b, c]` on the same line as the `:`.
    pub inline_list: Option<Vec<ValueNode>>,
    /// The indented content below the declaration.
    pub block: Option<Block>,
    /// True when the source gives a value. The value can be empty.
    ///
    /// `description:` declares a property with a null value. `description::
    /// string` with no colon declares an abstract slot with no value.
    /// The two look almost the same, but they have different meanings.
    pub declared: bool,
}

impl ValueNode {
    pub fn is_empty(&self) -> bool {
        self.inline.is_none() && self.inline_list.is_none() && self.block.is_none()
    }
}

/// An indented run of lines that are part of one declaration.
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
    /// `key: value`. It can have type constraints.
    Property(Property),
    /// `- value`
    ListItem(ListItem),
    /// `+ {expr}` or `++ {expr}`
    Merge(MergeItem),
    /// A run of prose lines. A blank line or a structural line ends the run.
    Prose(Paragraph),
    /// A multi-line escape block. Its contents are literal. The parser removes
    /// its delimiters.
    Escape(EscapeBlock),
    /// `pass`. A body that is empty on purpose.
    Pass(Span),
}

impl BlockItem {
    pub fn span(&self) -> Span {
        match self {
            BlockItem::Property(property) => property.span,
            BlockItem::ListItem(item) => item.span,
            BlockItem::Merge(merge) => merge.span,
            BlockItem::Prose(paragraph) => paragraph.span,
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

/// `+` merges and removes duplicates. `++` concatenates and keeps duplicates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeOp {
    Merge,
    Concat,
}

#[derive(Debug, Clone)]
pub struct MergeItem {
    pub span: Span,
    pub op: MergeOp,
    /// The `+` or the `++` token.
    pub op_span: Span,
    pub value: ProseLine,
}

/// Prose lines with only line breaks between them. A blank line starts a new
/// paragraph. The language uses a blank line for an explicit line break.
#[derive(Debug, Clone, Default)]
pub struct Paragraph {
    pub span: Span,
    pub lines: Vec<ProseLine>,
}

#[derive(Debug, Clone, Default)]
pub struct ProseLine {
    pub span: Span,
    pub segments: Vec<ProseSegment>,
    /// The delimiter backslash of each inline escape `\ ... \`, in order.
    pub escapes: Vec<Span>,
}

impl ProseLine {
    /// The single interpolation of this line, if the line holds only that.
    ///
    /// A value that is exactly one `{...}` keeps its intrinsic type. All other
    /// values are strings.
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
    /// Usual text. The scanner has already applied the escapes.
    Text(String),
    /// Text from a backslash-delimited escape. No later step must interpret
    /// this text again.
    Literal(String),
    Interpolation(Interpolation),
}

/// The conversion that an interpolation requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sigil {
    /// `{...}`. The intrinsic value.
    Standard,
    /// `${...}`. It converts the value to a string.
    Stringify,
    /// `#{...}`. It converts the value to a number.
    Numeric,
    /// `@{...}`. A reference that keeps the identity of the anchor.
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

/// A run of lines between two lines that contain only backslashes.
///
/// The delimiters are syntax. The parser removes them. It keeps the text
/// between them exactly as the source shows it.
#[derive(Debug, Clone)]
pub struct EscapeBlock {
    pub span: Span,
    /// The content lines, without the base indentation of the block.
    pub lines: Vec<String>,
    /// The length of the backslash run that is the delimiter of the block.
    pub run: usize,
}

/// An expression in an interpolation or in an inline list.
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
    /// A quoted string. A quoted value is always a string. It never changes type.
    Quoted(String),
    /// A bare identifier. The resolver finds it in the scope that contains it.
    Name(String),
    /// `this`. The anchor that contains the expression.
    This,
    /// `self`. The most derived anchor in the inheritance chain.
    SelfRef,
    /// `super`. The right-most base of the anchor that contains the expression.
    Super,
    Field(Box<Expr>, Spanned<String>),
    Unary(UnaryOp, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    List(Vec<Expr>),
    Paren(Box<Expr>),
    /// `${...}`, `#{...}` or `@{...}` in an expression. Thus a conversion or a
    /// reference can be part of a larger expression, as in
    /// `{@{Button} == @{Card}}`.
    Nested(Sigil, Box<Expr>),
    /// Error recovery makes this node. Thus the later phases can continue.
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
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
    /// `++`. It concatenates and does not remove duplicates.
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
