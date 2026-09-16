//! A typed view over the untyped rowan tree.
//!
//! Each wrapper is a thin newtype around a [`SyntaxNode`]; accessors find
//! children by kind. Nothing here allocates or copies the tree.

use crate::kind::SyntaxKind::{self, *};
use crate::{SyntaxNode, SyntaxToken};

/// A node that can be recognised from an untyped syntax node.
pub trait AstNode: Sized {
    fn can_cast(kind: SyntaxKind) -> bool;
    fn cast(syntax: SyntaxNode) -> Option<Self>;
    fn syntax(&self) -> &SyntaxNode;

    /// The source range this node covers.
    fn range(&self) -> rowan::TextRange {
        self.syntax().text_range()
    }
}

macro_rules! ast_node {
    ($(#[$meta:meta])* $name:ident, $kind:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub struct $name {
            pub(crate) syntax: SyntaxNode,
        }

        impl AstNode for $name {
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == $kind
            }
            fn cast(syntax: SyntaxNode) -> Option<Self> {
                (syntax.kind() == $kind).then_some($name { syntax })
            }
            fn syntax(&self) -> &SyntaxNode {
                &self.syntax
            }
        }
    };
}

ast_node!(/// A whole `.pi` file.
    Root, ROOT);
ast_node!(/// `anchor Name extends A as kw:` and its body.
    AnchorDecl, ANCHOR_DECL);
ast_node!(/// A top-level `name: value` binding.
    VarDecl, VAR_DECL);
ast_node!(/// `from PATH import ...`.
    ImportDecl, IMPORT_DECL);
ast_node!(/// `from PATH export ...`.
    ReexportDecl, REEXPORT_DECL);
ast_node!(/// `export Name`.
    ExportDecl, EXPORT_DECL);
ast_node!(/// `use PATH`.
    UseDecl, USE_DECL);
ast_node!(/// `extends A, B`.
    ExtendsClause, EXTENDS_CLAUSE);
ast_node!(/// `as my-keyword`.
    AsClause, AS_CLAUSE);
ast_node!(/// The name list of an import or re-export.
    ImportList, IMPORT_LIST);
ast_node!(/// One `Name` or `Name Alias` entry.
    ImportItem, IMPORT_ITEM);
ast_node!(/// `:: Type`.
    TypeAnnotation, TYPE_ANNOTATION);
ast_node!(/// A named type.
    TypeRef, TYPE_REF);
ast_node!(/// `T[]`.
    TypeList, TYPE_LIST);
ast_node!(/// `extends T`.
    TypeExtends, TYPE_EXTENDS);
ast_node!(/// An indented body.
    Block, BLOCK);
ast_node!(/// `key: value` inside a block.
    Property, PROPERTY);
ast_node!(/// `- value`.
    ListItem, LIST_ITEM);
ast_node!(/// `+ value` or `++ value`.
    SpreadItem, SPREAD_ITEM);
ast_node!(/// A line of prose.
    TextLine, TEXT_LINE);
ast_node!(/// A fenced code block.
    CodeBlock, CODE_BLOCK);
ast_node!(/// An empty line.
    BlankLine, BLANK_LINE);
ast_node!(/// The value region of a line.
    Value, VALUE);
ast_node!(/// Prose, possibly with interpolations.
    TextValue, TEXT_VALUE);
ast_node!(/// `sigil{ expr }`.
    Interpolation, INTERPOLATION);
ast_node!(/// `{ expr }` used as an expression atom.
    BraceExpr, BRACE_EXPR);
ast_node!(/// A binary operation.
    BinExpr, BIN_EXPR);
ast_node!(/// A prefix operation.
    UnaryExpr, UNARY_EXPR);
ast_node!(/// `cond ? a : b`.
    TernaryExpr, TERNARY_EXPR);
ast_node!(/// `( expr )`.
    ParenExpr, PAREN_EXPR);
ast_node!(/// `[a, b]`.
    ArrayExpr, ARRAY_EXPR);
ast_node!(/// `a.b`.
    FieldExpr, FIELD_EXPR);
ast_node!(/// A reference to a name.
    NameRef, NAME_REF);
ast_node!(/// A literal atom.
    Literal, LITERAL);

// ---- helpers ------------------------------------------------------------

fn child<N: AstNode>(node: &SyntaxNode) -> Option<N> {
    node.children().find_map(N::cast)
}

fn children_of<N: AstNode>(node: &SyntaxNode) -> impl Iterator<Item = N> {
    node.children().filter_map(N::cast)
}

fn token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|it| it.into_token())
        .find(|it| it.kind() == kind)
}

fn tokens(node: &SyntaxNode, kind: SyntaxKind) -> impl Iterator<Item = SyntaxToken> + '_ {
    node.children_with_tokens().filter_map(|it| it.into_token()).filter(move |it| it.kind() == kind)
}

// ---- items ---------------------------------------------------------------

/// Any top-level declaration.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Item {
    Anchor(AnchorDecl),
    Var(VarDecl),
    Import(ImportDecl),
    Reexport(ReexportDecl),
    Export(ExportDecl),
    Use(UseDecl),
}

impl Item {
    fn cast(syntax: SyntaxNode) -> Option<Item> {
        Some(match syntax.kind() {
            ANCHOR_DECL => Item::Anchor(AnchorDecl { syntax }),
            VAR_DECL => Item::Var(VarDecl { syntax }),
            IMPORT_DECL => Item::Import(ImportDecl { syntax }),
            REEXPORT_DECL => Item::Reexport(ReexportDecl { syntax }),
            EXPORT_DECL => Item::Export(ExportDecl { syntax }),
            USE_DECL => Item::Use(UseDecl { syntax }),
            _ => return None,
        })
    }

    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Item::Anchor(it) => &it.syntax,
            Item::Var(it) => &it.syntax,
            Item::Import(it) => &it.syntax,
            Item::Reexport(it) => &it.syntax,
            Item::Export(it) => &it.syntax,
            Item::Use(it) => &it.syntax,
        }
    }
}

impl Root {
    pub fn items(&self) -> impl Iterator<Item = Item> {
        self.syntax.children().filter_map(Item::cast)
    }
}

impl AnchorDecl {
    pub fn export_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, EXPORT_KW)
    }
    pub fn abstract_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, ABSTRACT_KW)
    }
    /// The user-defined keyword that introduced this anchor, if any.
    pub fn keyword_token(&self) -> Option<SyntaxToken> {
        if token(&self.syntax, ANCHOR_KW).is_some() {
            return None;
        }
        tokens(&self.syntax, IDENT).next()
    }
    pub fn name_token(&self) -> Option<SyntaxToken> {
        let skip = usize::from(token(&self.syntax, ANCHOR_KW).is_none());
        tokens(&self.syntax, IDENT).nth(skip)
    }
    pub fn name(&self) -> Option<String> {
        self.name_token().map(|it| it.text().to_string())
    }
    pub fn extends_clause(&self) -> Option<ExtendsClause> {
        child(&self.syntax)
    }
    pub fn as_clause(&self) -> Option<AsClause> {
        child(&self.syntax)
    }
    pub fn value(&self) -> Option<Value> {
        child(&self.syntax)
    }
    pub fn block(&self) -> Option<Block> {
        child(&self.syntax)
    }
}

impl VarDecl {
    pub fn export_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, EXPORT_KW)
    }
    pub fn name_token(&self) -> Option<SyntaxToken> {
        tokens(&self.syntax, IDENT).next()
    }
    pub fn name(&self) -> Option<String> {
        self.name_token().map(|it| it.text().to_string())
    }
    pub fn type_annotations(&self) -> impl Iterator<Item = TypeAnnotation> {
        children_of(&self.syntax)
    }
    pub fn value(&self) -> Option<Value> {
        child(&self.syntax)
    }
    pub fn block(&self) -> Option<Block> {
        child(&self.syntax)
    }
}

impl ImportDecl {
    pub fn path_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, PATH)
    }
    pub fn path(&self) -> Option<String> {
        self.path_token().map(|it| it.text().to_string())
    }
    pub fn list(&self) -> Option<ImportList> {
        child(&self.syntax)
    }
}

impl ReexportDecl {
    pub fn path_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, PATH)
    }
    pub fn path(&self) -> Option<String> {
        self.path_token().map(|it| it.text().to_string())
    }
    pub fn is_glob(&self) -> bool {
        token(&self.syntax, STAR).is_some()
    }
    pub fn list(&self) -> Option<ImportList> {
        child(&self.syntax)
    }
}

impl ExportDecl {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, IDENT)
    }
    pub fn name(&self) -> Option<String> {
        self.name_token().map(|it| it.text().to_string())
    }
}

impl UseDecl {
    pub fn path_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, PATH)
    }
    pub fn path(&self) -> Option<String> {
        self.path_token().map(|it| it.text().to_string())
    }
}

impl ExtendsClause {
    pub fn name_tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        tokens(&self.syntax, IDENT)
    }
    pub fn names(&self) -> Vec<String> {
        self.name_tokens().map(|it| it.text().to_string()).collect()
    }
}

impl AsClause {
    /// The keyword being registered, which may be a reserved word that the
    /// validator will reject with a better message than the parser could.
    pub fn name_token(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|it| !it.kind().is_trivia())
            .nth(1)
    }
    pub fn name(&self) -> Option<String> {
        self.name_token().map(|it| it.text().to_string())
    }
}

impl ImportList {
    pub fn items(&self) -> impl Iterator<Item = ImportItem> {
        children_of(&self.syntax)
    }
}

impl ImportItem {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        tokens(&self.syntax, IDENT).next()
    }
    pub fn alias_token(&self) -> Option<SyntaxToken> {
        tokens(&self.syntax, IDENT).nth(1)
    }
    pub fn name(&self) -> Option<String> {
        self.name_token().map(|it| it.text().to_string())
    }
    pub fn alias(&self) -> Option<String> {
        self.alias_token().map(|it| it.text().to_string())
    }
    /// The name this item binds locally.
    pub fn local_name(&self) -> Option<String> {
        self.alias().or_else(|| self.name())
    }
}

// ---- types ----------------------------------------------------------------

/// A type expression in a `::` constraint.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TypeExpr {
    Ref(TypeRef),
    List(TypeList),
    Extends(TypeExtends),
}

impl TypeExpr {
    pub fn cast(syntax: SyntaxNode) -> Option<TypeExpr> {
        Some(match syntax.kind() {
            TYPE_REF => TypeExpr::Ref(TypeRef { syntax }),
            TYPE_LIST => TypeExpr::List(TypeList { syntax }),
            TYPE_EXTENDS => TypeExpr::Extends(TypeExtends { syntax }),
            _ => return None,
        })
    }
    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            TypeExpr::Ref(it) => &it.syntax,
            TypeExpr::List(it) => &it.syntax,
            TypeExpr::Extends(it) => &it.syntax,
        }
    }
}

impl TypeAnnotation {
    pub fn type_expr(&self) -> Option<TypeExpr> {
        self.syntax.children().find_map(TypeExpr::cast)
    }
}

impl TypeRef {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        self.syntax.children_with_tokens().filter_map(|it| it.into_token()).find(|it| {
            matches!(it.kind(), IDENT | ANCHOR_KW | NULL_KW)
        })
    }
    pub fn name(&self) -> Option<String> {
        self.name_token().map(|it| it.text().to_string())
    }
}

impl TypeList {
    pub fn element(&self) -> Option<TypeExpr> {
        self.syntax.children().find_map(TypeExpr::cast)
    }
}

impl TypeExtends {
    pub fn base(&self) -> Option<TypeExpr> {
        self.syntax.children().find_map(TypeExpr::cast)
    }
}

// ---- blocks ---------------------------------------------------------------

/// One line inside a block.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Entry {
    Property(Property),
    ListItem(ListItem),
    Spread(SpreadItem),
    Text(TextLine),
    Code(CodeBlock),
    Blank(BlankLine),
}

impl Entry {
    fn cast(syntax: SyntaxNode) -> Option<Entry> {
        Some(match syntax.kind() {
            PROPERTY => Entry::Property(Property { syntax }),
            LIST_ITEM => Entry::ListItem(ListItem { syntax }),
            SPREAD_ITEM => Entry::Spread(SpreadItem { syntax }),
            TEXT_LINE => Entry::Text(TextLine { syntax }),
            CODE_BLOCK => Entry::Code(CodeBlock { syntax }),
            BLANK_LINE => Entry::Blank(BlankLine { syntax }),
            _ => return None,
        })
    }
    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Entry::Property(it) => &it.syntax,
            Entry::ListItem(it) => &it.syntax,
            Entry::Spread(it) => &it.syntax,
            Entry::Text(it) => &it.syntax,
            Entry::Code(it) => &it.syntax,
            Entry::Blank(it) => &it.syntax,
        }
    }
}

impl Block {
    pub fn entries(&self) -> impl Iterator<Item = Entry> {
        self.syntax.children().filter_map(Entry::cast)
    }
}

impl Property {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, IDENT)
    }
    pub fn name(&self) -> Option<String> {
        self.name_token().map(|it| it.text().to_string())
    }
    pub fn type_annotations(&self) -> impl Iterator<Item = TypeAnnotation> {
        children_of(&self.syntax)
    }
    pub fn value(&self) -> Option<Value> {
        child(&self.syntax)
    }
    pub fn block(&self) -> Option<Block> {
        child(&self.syntax)
    }
}

impl ListItem {
    pub fn value(&self) -> Option<Value> {
        child(&self.syntax)
    }
    pub fn block(&self) -> Option<Block> {
        child(&self.syntax)
    }
    /// `- key: value` carries a property instead of a value: the element is a
    /// dictionary rather than the text `key: value`.
    pub fn property(&self) -> Option<Property> {
        child(&self.syntax)
    }
}

impl SpreadItem {
    /// `+` deduplicates, `++` does not.
    pub fn deduplicates(&self) -> bool {
        token(&self.syntax, PLUS).is_some()
    }
    pub fn value(&self) -> Option<Value> {
        child(&self.syntax)
    }
    pub fn block(&self) -> Option<Block> {
        child(&self.syntax)
    }
}

impl TextLine {
    pub fn value(&self) -> Option<Value> {
        child(&self.syntax)
    }
}

impl CodeBlock {
    /// The block as the string it compiles to: its fences and every line
    /// between them exactly as written, less the indentation the opening fence
    /// sits at, which is syntax. Blank lines inside the fence are kept.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for token in self.syntax.children_with_tokens().filter_map(|it| it.into_token()) {
            match token.kind() {
                FENCE => out.push_str(token.text().trim_end()),
                CODE => out.push_str(token.text()),
                // Only the line breaks: the indentation beside them is the
                // fence's, and the newline ending the block is a `NEWLINE`.
                WHITESPACE => out.extend(token.text().chars().filter(|&it| it == '\n')),
                _ => {}
            }
        }
        out
    }

    /// Whether the block has its closing fence.
    pub fn is_closed(&self) -> bool {
        self.syntax.children_with_tokens().filter(|it| it.kind() == FENCE).count() == 2
    }
}

// ---- values and expressions -------------------------------------------------

/// What a value region turned out to be.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ValueKind {
    Text(TextValue),
    Expr(Expr),
}

impl Value {
    pub fn kind(&self) -> Option<ValueKind> {
        let inner = self.syntax.first_child()?;
        if inner.kind() == TEXT_VALUE {
            Some(ValueKind::Text(TextValue { syntax: inner }))
        } else {
            Expr::cast(inner).map(ValueKind::Expr)
        }
    }
}

/// Any expression node.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Expr {
    Bin(BinExpr),
    Unary(UnaryExpr),
    Ternary(TernaryExpr),
    Paren(ParenExpr),
    Array(ArrayExpr),
    Field(FieldExpr),
    Name(NameRef),
    Literal(Literal),
    Brace(BraceExpr),
}

impl Expr {
    pub fn cast(syntax: SyntaxNode) -> Option<Expr> {
        Some(match syntax.kind() {
            BIN_EXPR => Expr::Bin(BinExpr { syntax }),
            UNARY_EXPR => Expr::Unary(UnaryExpr { syntax }),
            TERNARY_EXPR => Expr::Ternary(TernaryExpr { syntax }),
            PAREN_EXPR => Expr::Paren(ParenExpr { syntax }),
            ARRAY_EXPR => Expr::Array(ArrayExpr { syntax }),
            FIELD_EXPR => Expr::Field(FieldExpr { syntax }),
            NAME_REF => Expr::Name(NameRef { syntax }),
            LITERAL => Expr::Literal(Literal { syntax }),
            BRACE_EXPR => Expr::Brace(BraceExpr { syntax }),
            _ => return None,
        })
    }
    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Expr::Bin(it) => &it.syntax,
            Expr::Unary(it) => &it.syntax,
            Expr::Ternary(it) => &it.syntax,
            Expr::Paren(it) => &it.syntax,
            Expr::Array(it) => &it.syntax,
            Expr::Field(it) => &it.syntax,
            Expr::Name(it) => &it.syntax,
            Expr::Literal(it) => &it.syntax,
            Expr::Brace(it) => &it.syntax,
        }
    }
}

impl BinExpr {
    pub fn lhs(&self) -> Option<Expr> {
        self.syntax.children().find_map(Expr::cast)
    }
    pub fn rhs(&self) -> Option<Expr> {
        self.syntax.children().filter_map(Expr::cast).nth(1)
    }
    pub fn op_token(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| !it.kind().is_trivia())
    }
}

impl UnaryExpr {
    pub fn operand(&self) -> Option<Expr> {
        self.syntax.children().find_map(Expr::cast)
    }
}

impl TernaryExpr {
    pub fn condition(&self) -> Option<Expr> {
        self.syntax.children().find_map(Expr::cast)
    }
    pub fn then_branch(&self) -> Option<Expr> {
        self.syntax.children().filter_map(Expr::cast).nth(1)
    }
    pub fn else_branch(&self) -> Option<Expr> {
        self.syntax.children().filter_map(Expr::cast).nth(2)
    }
}

impl ParenExpr {
    pub fn inner(&self) -> Option<Expr> {
        self.syntax.children().find_map(Expr::cast)
    }
}

impl ArrayExpr {
    pub fn elements(&self) -> impl Iterator<Item = Value> {
        children_of(&self.syntax)
    }
    /// Elements written directly as expressions, used inside `{ ... }`.
    pub fn exprs(&self) -> impl Iterator<Item = Expr> {
        self.syntax.children().filter_map(Expr::cast)
    }
}

impl FieldExpr {
    pub fn base(&self) -> Option<Expr> {
        self.syntax.children().find_map(Expr::cast)
    }
    pub fn field_token(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|it| matches!(it.kind(), IDENT | THIS_KW | SELF_KW | SUPER_KW))
            .last()
    }
    pub fn field(&self) -> Option<String> {
        self.field_token().map(|it| it.text().to_string())
    }
}

impl NameRef {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        first_meaningful(&self.syntax)
    }
    pub fn name(&self) -> Option<String> {
        self.name_token().map(|it| it.text().to_string())
    }
}

impl Literal {
    pub fn token(&self) -> Option<SyntaxToken> {
        first_meaningful(&self.syntax)
    }
}

/// The first token of a node that carries meaning rather than layout.
fn first_meaningful(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|it| it.into_token())
        .find(|it| !it.kind().is_trivia())
}

impl BraceExpr {
    pub fn expr(&self) -> Option<Expr> {
        self.syntax.children().find_map(Expr::cast)
    }
}

impl Interpolation {
    pub fn sigil_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, SIGIL)
    }
    /// The sigil name, defaulting to `$` when written as a bare `{ ... }`.
    pub fn sigil(&self) -> String {
        self.sigil_token().map(|it| it.text().to_string()).unwrap_or_else(|| "$".to_string())
    }
    pub fn expr(&self) -> Option<Expr> {
        self.syntax.children().find_map(Expr::cast)
    }
}

/// A piece of prose: either literal text or an interpolation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TextPiece {
    Token(SyntaxToken),
    Interpolation(Interpolation),
}

impl TextValue {
    pub fn pieces(&self) -> impl Iterator<Item = TextPiece> + '_ {
        self.syntax.children_with_tokens().filter_map(|element| match element {
            rowan::NodeOrToken::Token(t) => Some(TextPiece::Token(t)),
            rowan::NodeOrToken::Node(n) => Interpolation::cast(n).map(TextPiece::Interpolation),
        })
    }
}
