//! Lowering: concrete syntax tree to [`crate::hir`].
//!
//! This is where a block stops being "some indented lines" and becomes a list,
//! a dictionary, a string, or the implicit list that a mixed block produces.

use piton_syntax::ast::{self, AstNode};
use piton_syntax::kind::SyntaxKind::{self, *};
use piton_syntax::{SyntaxNode, SyntaxToken, TextRange};

use crate::hir::*;

/// Lower a parsed file.
pub fn lower(root: &ast::Root) -> Hir {
    let mut hir = Hir::default();
    for item in root.items() {
        match item {
            ast::Item::Anchor(decl) => {
                if let Some(anchor) = lower_anchor(&decl) {
                    hir.anchors.push(anchor);
                }
            }
            ast::Item::Var(decl) => {
                if let Some(var) = lower_var(&decl) {
                    hir.vars.push(var);
                }
            }
            ast::Item::Import(decl) => {
                if let Some(path) = spanned_token(decl.path_token()) {
                    hir.imports.push(ImportDef {
                        path,
                        items: import_items(decl.list()),
                        range: decl.range(),
                    });
                }
            }
            ast::Item::Reexport(decl) => {
                if let Some(path) = spanned_token(decl.path_token()) {
                    hir.reexports.push(ReexportDef {
                        path,
                        glob: decl.is_glob(),
                        items: import_items(decl.list()),
                        range: decl.range(),
                    });
                }
            }
            ast::Item::Export(decl) => hir.exports.extend(spanned_token(decl.name_token())),
            ast::Item::Use(decl) => hir.uses.extend(spanned_token(decl.path_token())),
        }
    }
    hir
}

fn lower_anchor(decl: &ast::AnchorDecl) -> Option<AnchorDef> {
    let name_token = decl.name_token()?;
    Some(AnchorDef {
        name: name_token.text().to_string(),
        name_range: name_token.text_range(),
        range: decl.range(),
        exported: decl.export_token().is_some(),
        is_abstract: decl.abstract_token().is_some(),
        keyword: decl.as_clause().and_then(|it| spanned_token(it.name_token())),
        via_keyword: spanned_token(decl.keyword_token()),
        bases: decl
            .extends_clause()
            .map(|clause| clause.name_tokens().filter_map(|t| spanned_token(Some(t))).collect())
            .unwrap_or_default(),
        body: lower_body(decl.value(), decl.block()),
        doc: doc_comment(decl.syntax()),
    })
}

fn lower_var(decl: &ast::VarDecl) -> Option<VarDef> {
    let name_token = decl.name_token()?;
    Some(VarDef {
        name: name_token.text().to_string(),
        name_range: name_token.text_range(),
        range: decl.range(),
        exported: decl.export_token().is_some(),
        constraints: decl.type_annotations().filter_map(|it| lower_type(&it)).collect(),
        body: lower_body(decl.value(), decl.block()),
        doc: doc_comment(decl.syntax()),
    })
}

fn import_items(list: Option<ast::ImportList>) -> Vec<ImportItem> {
    let Some(list) = list else { return Vec::new() };
    list.items()
        .filter_map(|item| {
            Some(ImportItem {
                name: spanned_token(item.name_token())?,
                alias: spanned_token(item.alias_token()),
            })
        })
        .collect()
}

fn lower_type(annotation: &ast::TypeAnnotation) -> Option<TypeExpr> {
    fn convert(expr: ast::TypeExpr) -> Option<TypeExpr> {
        let range = expr.syntax().text_range();
        Some(match expr {
            ast::TypeExpr::Ref(it) => TypeExpr::Named { name: it.name()?, range },
            ast::TypeExpr::List(it) => {
                TypeExpr::ListOf { element: Box::new(convert(it.element()?)?), range }
            }
            ast::TypeExpr::Extends(it) => {
                TypeExpr::Extends { base: Box::new(convert(it.base()?)?), range }
            }
        })
    }
    convert(annotation.type_expr()?)
}

// ---- bodies -----------------------------------------------------------------

/// Turn an optional inline value and an optional block into one node.
fn lower_body(inline: Option<ast::Value>, block: Option<ast::Block>) -> Node {
    match (inline, block) {
        (None, None) => Node::Empty,
        (Some(value), None) => match lower_value(&value) {
            Some(Piece::Expr(expr)) => Node::Value(expr),
            Some(Piece::Line(line)) => Node::Value(Expr::Text(Text {
                paragraphs: vec![vec![line]],
                range: value.range(),
            })),
            None => Node::Empty,
        },
        (inline, Some(block)) => lower_block(inline, &block),
    }
}

/// The two shapes a single value region can take.
enum Piece {
    Line(Line),
    Expr(Expr),
}

/// A run of same-shaped entries inside a block.
enum Group {
    Text(Vec<TextItem>),
    List(Vec<Element>),
    Dict(Vec<Property>),
}

enum TextItem {
    Line(Line),
    Expr(Expr),
    Break,
}

fn lower_block(inline: Option<ast::Value>, block: &ast::Block) -> Node {
    let mut groups: Vec<Group> = Vec::new();
    // A `+`/`++` line says the block builds a list, so prose beside it becomes
    // list content rather than turning the block into an implicit list.
    let entries: Vec<ast::Entry> = block.entries().collect();
    let has_spread = entries.iter().any(|it| matches!(it, ast::Entry::Spread(_)));
    let has_property = entries.iter().any(|it| matches!(it, ast::Entry::Property(_)));
    let has_item = entries.iter().any(|it| matches!(it, ast::Entry::ListItem(_)));
    let list_mode = has_spread && !has_property;
    // A spread beside `key: value` pairs merges dictionaries, exactly as `+`
    // and `++` do between two of them.
    let merge_mode = has_spread && has_property && !has_item;

    if let Some(value) = inline {
        push_text(&mut groups, lower_value(&value));
    }
    for entry in entries {
        match entry {
            ast::Entry::Property(property) => {
                let lowered = lower_property(&property);
                match groups.last_mut() {
                    Some(Group::Dict(items)) => items.push(lowered),
                    _ => groups.push(Group::Dict(vec![lowered])),
                }
            }
            ast::Entry::ListItem(item) => {
                // `- key: value` is one element that happens to be a
                // dictionary, not a run of `key: value` properties: the
                // dictionary belongs to the list, not to the enclosing block.
                let element = match item.property() {
                    Some(property) => vec![Element {
                        node: Node::Dict(vec![lower_property(&property)]),
                        spread: None,
                        range: item.range(),
                    }],
                    None => list_element(item.value(), item.block(), None, item.range()),
                };
                push_elements(&mut groups, element);
            }
            ast::Entry::Spread(item) => {
                let element = list_element(
                    item.value(),
                    item.block(),
                    Some(item.deduplicates()),
                    item.range(),
                );
                push_elements(&mut groups, element);
            }
            ast::Entry::Text(line) => {
                let piece = line.value().as_ref().and_then(lower_value);
                push_text(&mut groups, piece);
            }
            ast::Entry::Blank(_) => {
                if let Some(Group::Text(items)) = groups.last_mut() {
                    items.push(TextItem::Break);
                }
            }
        }
    }

    if merge_mode {
        let range = block.range();
        let mut parts = Vec::new();
        for group in groups {
            match group {
                Group::List(items) => parts.extend(items),
                other => {
                    if let Some(node) = finish_group(other) {
                        parts.push(Element { node, spread: None, range });
                    }
                }
            }
        }
        return Node::Merge(parts);
    }

    if list_mode {
        let range = block.range();
        let mut elements = Vec::new();
        for group in groups {
            match group {
                Group::List(items) => elements.extend(items),
                other => {
                    if let Some(node) = finish_group(other) {
                        elements.push(Element { node, spread: None, range });
                    }
                }
            }
        }
        return Node::List(elements);
    }

    let mut nodes: Vec<Node> = groups.into_iter().filter_map(finish_group).collect();
    match nodes.len() {
        0 => Node::Empty,
        1 => nodes.pop().unwrap(),
        _ => Node::Mixed(nodes),
    }
}

fn lower_property(property: &ast::Property) -> Property {
    Property {
        name: property.name().unwrap_or_default(),
        name_range: property
            .name_token()
            .map(|it| it.text_range())
            .unwrap_or_else(|| property.range()),
        range: property.range(),
        constraints: property.type_annotations().filter_map(|it| lower_type(&it)).collect(),
        node: lower_body(property.value(), property.block()),
        doc: doc_comment(property.syntax()),
    }
}

/// A list item contributes its own value and, separately, any nested block.
///
/// That is what makes `- Level 1` with a nested `- Level 2` come out as
/// `["Level 1", ["Level 2"]]`.
fn list_element(
    value: Option<ast::Value>,
    block: Option<ast::Block>,
    spread: Option<bool>,
    range: TextRange,
) -> Vec<Element> {
    let mut out = Vec::new();
    if let Some(value) = value.as_ref() {
        if let Some(piece) = lower_value(value) {
            out.push(Element { node: piece_node(piece, value.range()), spread, range });
        }
    }
    if let Some(block) = block {
        let nested = lower_block(None, &block);
        let spread = if out.is_empty() { spread } else { None };
        out.push(Element { node: nested, spread, range });
    }
    if out.is_empty() {
        out.push(Element { node: Node::Empty, spread, range });
    }
    out
}

fn piece_node(piece: Piece, range: TextRange) -> Node {
    match piece {
        Piece::Expr(expr) => Node::Value(expr),
        Piece::Line(line) => {
            Node::Value(Expr::Text(Text { paragraphs: vec![vec![line]], range }))
        }
    }
}

fn push_elements(groups: &mut Vec<Group>, elements: Vec<Element>) {
    match groups.last_mut() {
        Some(Group::List(items)) => items.extend(elements),
        _ => groups.push(Group::List(elements)),
    }
}

fn push_text(groups: &mut Vec<Group>, piece: Option<Piece>) {
    let Some(piece) = piece else { return };
    let item = match piece {
        Piece::Line(line) => TextItem::Line(line),
        Piece::Expr(expr) => TextItem::Expr(expr),
    };
    match groups.last_mut() {
        Some(Group::Text(items)) => items.push(item),
        _ => groups.push(Group::Text(vec![item])),
    }
}

fn finish_group(group: Group) -> Option<Node> {
    match group {
        Group::List(items) => Some(Node::List(items)),
        Group::Dict(items) => Some(Node::Dict(items)),
        Group::Text(items) => finish_text(items),
    }
}

/// Assemble prose. A single line that parsed as an expression stays an
/// expression; anything longer becomes text with the expressions interpolated.
fn finish_text(items: Vec<TextItem>) -> Option<Node> {
    let content: Vec<&TextItem> =
        items.iter().filter(|item| !matches!(item, TextItem::Break)).collect();
    if content.is_empty() {
        return None;
    }
    if let [TextItem::Expr(expr)] = content.as_slice() {
        return Some(Node::Value(expr.clone()));
    }

    let mut range: Option<TextRange> = None;
    let mut paragraphs: Vec<Vec<Line>> = vec![Vec::new()];
    for item in items {
        match item {
            TextItem::Break => {
                if !paragraphs.last().is_some_and(Vec::is_empty) {
                    paragraphs.push(Vec::new());
                }
            }
            TextItem::Line(line) => paragraphs.last_mut().unwrap().push(line),
            TextItem::Expr(expr) => {
                let expr_range = expr.range();
                range = Some(range.map_or(expr_range, |r: TextRange| r.cover(expr_range)));
                paragraphs.last_mut().unwrap().push(Line {
                    segments: vec![Segment::Interpolation(Interpolation {
                        sigil: "$".to_string(),
                        expr,
                        range: expr_range,
                    })],
                });
            }
        }
    }
    paragraphs.retain(|lines| !lines.is_empty());
    Some(Node::Value(Expr::Text(Text {
        paragraphs,
        range: range.unwrap_or_else(|| TextRange::empty(0.into())),
    })))
}

// ---- values ------------------------------------------------------------------

fn lower_value(value: &ast::Value) -> Option<Piece> {
    match value.kind()? {
        ast::ValueKind::Expr(expr) => Some(Piece::Expr(lower_expr(&expr))),
        ast::ValueKind::Text(text) => Some(Piece::Line(lower_text_line(&text))),
    }
}

/// Turn a value region into an expression, wrapping prose as a one-line text.
fn lower_value_expr(value: &ast::Value) -> Expr {
    match lower_value(value) {
        Some(Piece::Expr(expr)) => expr,
        Some(Piece::Line(line)) => {
            Expr::Text(Text { paragraphs: vec![vec![line]], range: value.range() })
        }
        None => Expr::Text(Text { paragraphs: Vec::new(), range: value.range() }),
    }
}

fn lower_text_line(text: &ast::TextValue) -> Line {
    let mut segments: Vec<Segment> = Vec::new();
    let mut literal = String::new();

    let content: Vec<ast::TextPiece> = text.pieces().collect();
    // A value that is nothing but a quoted string drops its quotes.
    if let [ast::TextPiece::Token(token)] = trim_trivia(&content).as_slice() {
        if token.kind() == QUOTED_STRING {
            return Line { segments: vec![Segment::Literal(unquote(token.text()))] };
        }
    }

    for piece in content {
        match piece {
            ast::TextPiece::Token(token) => match token.kind() {
                COMMENT => {}
                ESCAPE => literal.push_str(token.text().trim_start_matches('\\')),
                _ => literal.push_str(token.text()),
            },
            ast::TextPiece::Interpolation(interpolation) => {
                flush(&mut literal, &mut segments);
                segments.push(Segment::Interpolation(Interpolation {
                    sigil: interpolation.sigil(),
                    expr: interpolation.expr().map(|it| lower_expr(&it)).unwrap_or(Expr::Error {
                        range: interpolation.range(),
                    }),
                    range: interpolation.range(),
                }));
            }
        }
    }
    flush(&mut literal, &mut segments);
    trim_segments(&mut segments);
    Line { segments }
}

fn flush(literal: &mut String, segments: &mut Vec<Segment>) {
    if !literal.is_empty() {
        segments.push(Segment::Literal(std::mem::take(literal)));
    }
}

/// Indentation and the space after `:` are syntax, not part of the string.
fn trim_segments(segments: &mut Vec<Segment>) {
    if let Some(Segment::Literal(first)) = segments.first_mut() {
        let trimmed = first.trim_start().to_string();
        *first = trimmed;
    }
    if let Some(Segment::Literal(last)) = segments.last_mut() {
        let trimmed = last.trim_end().to_string();
        *last = trimmed;
    }
    segments.retain(|segment| !matches!(segment, Segment::Literal(text) if text.is_empty()));
}

fn trim_trivia(pieces: &[ast::TextPiece]) -> Vec<ast::TextPiece> {
    pieces
        .iter()
        .filter(|piece| match piece {
            ast::TextPiece::Token(token) => !token.kind().is_trivia(),
            ast::TextPiece::Interpolation(_) => true,
        })
        .cloned()
        .collect()
}

fn lower_expr(expr: &ast::Expr) -> Expr {
    let range = expr.syntax().text_range();
    match expr {
        ast::Expr::Literal(literal) => {
            let Some(token) = literal.token() else { return Expr::Error { range } };
            let value = match token.kind() {
                NUMBER => Literal::Number(token.text().to_string()),
                QUOTED_STRING => Literal::String(unquote(token.text())),
                TRUE_KW => Literal::Bool(true),
                FALSE_KW => Literal::Bool(false),
                _ => Literal::Null,
            };
            Expr::Literal { value, range }
        }
        ast::Expr::Name(name) => match name.name() {
            Some(name) => Expr::Name { name, range },
            None => Expr::Error { range },
        },
        ast::Expr::Field(field) => {
            let base = field.base().map(|it| lower_expr(&it)).unwrap_or(Expr::Error { range });
            match (field.field(), field.field_token()) {
                (Some(name), Some(token)) => Expr::Field {
                    base: Box::new(base),
                    name,
                    name_range: token.text_range(),
                    range,
                },
                _ => Expr::Error { range },
            }
        }
        ast::Expr::Brace(brace) => {
            brace.expr().map(|it| lower_expr(&it)).unwrap_or(Expr::Error { range })
        }
        ast::Expr::Paren(paren) => {
            paren.inner().map(|it| lower_expr(&it)).unwrap_or(Expr::Error { range })
        }
        ast::Expr::Unary(unary) => Expr::Unary {
            operand: Box::new(
                unary.operand().map(|it| lower_expr(&it)).unwrap_or(Expr::Error { range }),
            ),
            range,
        },
        ast::Expr::Ternary(ternary) => Expr::Ternary {
            condition: Box::new(
                ternary.condition().map(|it| lower_expr(&it)).unwrap_or(Expr::Error { range }),
            ),
            then: Box::new(
                ternary.then_branch().map(|it| lower_expr(&it)).unwrap_or(Expr::Error { range }),
            ),
            otherwise: Box::new(
                ternary.else_branch().map(|it| lower_expr(&it)).unwrap_or(Expr::Error { range }),
            ),
            range,
        },
        ast::Expr::Array(array) => Expr::Array { elements: array_elements(array), range },
        ast::Expr::Bin(bin) => {
            let op = bin.op_token().and_then(|it| binary_op(it.kind()));
            match (op, bin.lhs(), bin.rhs()) {
                (Some(op), Some(lhs), Some(rhs)) => Expr::Binary {
                    op,
                    lhs: Box::new(lower_expr(&lhs)),
                    rhs: Box::new(lower_expr(&rhs)),
                    range,
                },
                _ => Expr::Error { range },
            }
        }
    }
}

/// Inline lists hold text-position values outside `{ }` and expressions inside.
fn array_elements(array: &ast::ArrayExpr) -> Vec<Expr> {
    array
        .syntax()
        .children()
        .filter_map(|child| match ast::Value::cast(child.clone()) {
            Some(value) => Some(lower_value_expr(&value)),
            None => ast::Expr::cast(child).map(|expr| lower_expr(&expr)),
        })
        .collect()
}

fn binary_op(kind: SyntaxKind) -> Option<BinaryOp> {
    Some(match kind {
        PLUS => BinaryOp::Add,
        PLUS2 => BinaryOp::Concat,
        MINUS => BinaryOp::Sub,
        STAR => BinaryOp::Mul,
        SLASH => BinaryOp::Div,
        PERCENT => BinaryOp::Rem,
        EQ2 => BinaryOp::Eq,
        BANG_EQ => BinaryOp::Ne,
        LT => BinaryOp::Lt,
        LT_EQ => BinaryOp::Le,
        GT => BinaryOp::Gt,
        GT_EQ => BinaryOp::Ge,
        AMP2 => BinaryOp::And,
        PIPE2 => BinaryOp::Or,
        _ => return None,
    })
}

// ---- odds and ends -------------------------------------------------------------

fn spanned_token(token: Option<SyntaxToken>) -> Option<Spanned<String>> {
    token.map(|it| Spanned::new(it.text().to_string(), it.text_range()))
}

/// Strip the wrapping quotes and resolve backslash escapes.
pub fn unquote(text: &str) -> String {
    let inner = text.strip_prefix('"').unwrap_or(text);
    let inner = inner.strip_suffix('"').unwrap_or(inner);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// The `//` comment lines written directly above a declaration.
fn doc_comment(node: &SyntaxNode) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    for element in node.children_with_tokens() {
        match element.into_token() {
            Some(token) if token.kind() == COMMENT => {
                lines.push(token.text().trim_start_matches('/').trim().to_string());
            }
            Some(token) if token.kind().is_trivia() => {}
            _ => break,
        }
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}
