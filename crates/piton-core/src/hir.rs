//! The lowered form of a Piton file.
//!
//! Lowering strips away the syntax that only exists to be readable — inline
//! versus block values, mixed blocks, indentation — and leaves a shape the
//! evaluator can walk without re-deciding what a line meant.

use piton_syntax::TextRange;

/// A value plus where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spanned<T> {
    pub value: T,
    pub range: TextRange,
}

impl<T> Spanned<T> {
    pub fn new(value: T, range: TextRange) -> Spanned<T> {
        Spanned { value, range }
    }
}

/// Everything one file declares.
#[derive(Clone, Debug, Default)]
pub struct Hir {
    pub anchors: Vec<AnchorDef>,
    pub vars: Vec<VarDef>,
    pub imports: Vec<ImportDef>,
    pub reexports: Vec<ReexportDef>,
    pub exports: Vec<Spanned<String>>,
    pub uses: Vec<Spanned<String>>,
}

/// `anchor Name extends A, B as keyword:` plus its body.
#[derive(Clone, Debug)]
pub struct AnchorDef {
    pub name: String,
    pub name_range: TextRange,
    pub range: TextRange,
    pub exported: bool,
    pub is_abstract: bool,
    /// The keyword this anchor registers via `as`.
    pub keyword: Option<Spanned<String>>,
    /// The user-defined keyword used to declare it, which becomes its first base.
    pub via_keyword: Option<Spanned<String>>,
    pub bases: Vec<Spanned<String>>,
    pub body: Node,
    pub doc: Option<String>,
}

impl AnchorDef {
    /// The names written as bases, for diagnostics. Resolution happens in
    /// `Analysis::bases`, because a declaring keyword lives in its own table.
    pub fn base_names(&self) -> Vec<Spanned<String>> {
        let mut names = Vec::new();
        names.extend(self.via_keyword.clone());
        names.extend(self.bases.iter().cloned());
        names
    }
}

/// A top-level `name: value` binding.
#[derive(Clone, Debug)]
pub struct VarDef {
    pub name: String,
    pub name_range: TextRange,
    pub range: TextRange,
    pub exported: bool,
    pub constraints: Vec<TypeExpr>,
    pub body: Node,
    pub doc: Option<String>,
}

/// `from PATH import a, b Alias`.
#[derive(Clone, Debug)]
pub struct ImportDef {
    pub path: Spanned<String>,
    pub items: Vec<ImportItem>,
    pub range: TextRange,
}

/// `from PATH export ...`.
#[derive(Clone, Debug)]
pub struct ReexportDef {
    pub path: Spanned<String>,
    pub glob: bool,
    pub items: Vec<ImportItem>,
    pub range: TextRange,
}

#[derive(Clone, Debug)]
pub struct ImportItem {
    pub name: Spanned<String>,
    pub alias: Option<Spanned<String>>,
}

impl ImportItem {
    /// The name bound in the importing file.
    pub fn local(&self) -> &str {
        self.alias.as_ref().map_or(self.name.value.as_str(), |it| it.value.as_str())
    }
}

/// A `::` type constraint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeExpr {
    /// `string`, `number`, `MyAnchor`, ...
    Named { name: String, range: TextRange },
    /// `T[]`
    ListOf { element: Box<TypeExpr>, range: TextRange },
    /// `extends T`
    Extends { base: Box<TypeExpr>, range: TextRange },
}

impl TypeExpr {
    pub fn range(&self) -> TextRange {
        match self {
            TypeExpr::Named { range, .. }
            | TypeExpr::ListOf { range, .. }
            | TypeExpr::Extends { range, .. } => *range,
        }
    }

    /// A human-readable rendering for hovers and diagnostics.
    pub fn render(&self) -> String {
        match self {
            TypeExpr::Named { name, .. } => name.clone(),
            TypeExpr::ListOf { element, .. } => format!("{}[]", element.render()),
            TypeExpr::Extends { base, .. } => format!("extends {}", base.render()),
        }
    }
}

/// The body of a declaration or a property, after block classification.
#[derive(Clone, Debug)]
pub enum Node {
    /// Nothing was written.
    Empty,
    /// A single expression or a run of prose.
    Value(Expr),
    /// A `- ` list, possibly with `+`/`++` spreads.
    List(Vec<Element>),
    /// A block of `key: value` pairs.
    Dict(Vec<Property>),
    /// A block that mixed shapes and therefore became an implicit list.
    Mixed(Vec<Node>),
}

impl Node {
    pub fn is_empty(&self) -> bool {
        matches!(self, Node::Empty)
    }
}

/// One entry of a list body.
#[derive(Clone, Debug)]
pub struct Element {
    pub node: Node,
    /// `Some(true)` for `+`, `Some(false)` for `++`, `None` for a plain item.
    pub spread: Option<bool>,
    pub range: TextRange,
}

/// One `key: value` pair.
#[derive(Clone, Debug)]
pub struct Property {
    pub name: String,
    pub name_range: TextRange,
    pub range: TextRange,
    pub constraints: Vec<TypeExpr>,
    pub node: Node,
    pub doc: Option<String>,
}

/// A lowered expression.
#[derive(Clone, Debug)]
pub enum Expr {
    Literal { value: Literal, range: TextRange },
    Text(Text),
    Array { elements: Vec<Expr>, range: TextRange },
    Binary { op: BinaryOp, lhs: Box<Expr>, rhs: Box<Expr>, range: TextRange },
    Unary { operand: Box<Expr>, range: TextRange },
    Ternary { condition: Box<Expr>, then: Box<Expr>, otherwise: Box<Expr>, range: TextRange },
    Name { name: String, range: TextRange },
    Field { base: Box<Expr>, name: String, name_range: TextRange, range: TextRange },
    Error { range: TextRange },
}

impl Expr {
    pub fn range(&self) -> TextRange {
        match self {
            Expr::Literal { range, .. }
            | Expr::Array { range, .. }
            | Expr::Binary { range, .. }
            | Expr::Unary { range, .. }
            | Expr::Ternary { range, .. }
            | Expr::Name { range, .. }
            | Expr::Field { range, .. }
            | Expr::Error { range } => *range,
            Expr::Text(text) => text.range,
        }
    }
}

/// A literal atom.
#[derive(Clone, Debug)]
pub enum Literal {
    Number(String),
    String(String),
    Bool(bool),
    Null,
}

/// Which binary operator, before value-level dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Concat,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

/// Prose. Lines inside a paragraph join with a space; paragraphs are separated
/// by the blank lines the author wrote.
#[derive(Clone, Debug)]
pub struct Text {
    pub paragraphs: Vec<Vec<Line>>,
    pub range: TextRange,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub segments: Vec<Segment>,
}

#[derive(Clone, Debug)]
pub enum Segment {
    Literal(String),
    Interpolation(Interpolation),
}

/// `sigil{ expr }` inside prose.
#[derive(Clone, Debug)]
pub struct Interpolation {
    pub sigil: String,
    pub expr: Expr,
    pub range: TextRange,
}

/// Convenience re-export so `db` does not have to name the lowering module.
pub use crate::lower::lower as lower_hir;
