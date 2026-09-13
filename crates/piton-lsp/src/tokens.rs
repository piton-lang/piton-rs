//! Features that read the syntax tree directly: semantic tokens, folding,
//! document links, inlay hints, and document symbols.

use std::collections::HashMap;

use piton_core::hir::Property;
use piton_core::value::Value;
use piton_core::FileId;
use piton_syntax::kind::SyntaxKind::*;
use piton_syntax::{ast::AstNode, SyntaxNode, SyntaxToken, TextRange};
use tower_lsp::lsp_types::{
    DocumentLink, DocumentSymbol, FoldingRange, FoldingRangeKind, InlayHint, InlayHintKind,
    InlayHintLabel, SemanticToken, SemanticTokenModifier, SemanticTokenType, SymbolKind, Url,
};

use crate::index::{collect, Model, Occurrence, Role, Sym};
use crate::world::View;

/// The semantic token types this server produces, in legend order.
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,
    SemanticTokenType::TYPE,
    SemanticTokenType::CLASS,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::STRING,
    SemanticTokenType::NUMBER,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::COMMENT,
    SemanticTokenType::NAMESPACE,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::ENUM_MEMBER,
    SemanticTokenType::DECORATOR,
];

/// The semantic token modifiers this server produces, in legend order.
pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,
    SemanticTokenModifier::DEFINITION,
    SemanticTokenModifier::READONLY,
    SemanticTokenModifier::ABSTRACT,
];

const T_KEYWORD: u32 = 0;
const T_TYPE: u32 = 1;
const T_CLASS: u32 = 2;
const T_PROPERTY: u32 = 3;
const T_VARIABLE: u32 = 4;
const T_STRING: u32 = 5;
const T_NUMBER: u32 = 6;
const T_OPERATOR: u32 = 7;
const T_COMMENT: u32 = 8;
const T_NAMESPACE: u32 = 9;
const T_FUNCTION: u32 = 10;
const T_ENUM_MEMBER: u32 = 11;
const T_DECORATOR: u32 = 12;

const DECLARATION: u32 = 1 << 0;
const DEFINITION: u32 = 1 << 1;
const READONLY: u32 = 1 << 2;
const ABSTRACT: u32 = 1 << 3;

/// Classify every token in a file.
pub fn semantic_tokens(view: &View, file: FileId) -> Vec<SemanticToken> {
    let index = view.line_index(file);
    let root = view.compilation.analysis.db.file(file).parse.syntax();
    let model = view.model();
    let resolved: HashMap<TextRange, &Occurrence> =
        view.index().in_file(file).iter().map(|it| (it.range, it)).collect();
    let mut out = Vec::new();
    let mut previous_line = 0u32;
    let mut previous_start = 0u32;

    for element in root.descendants_with_tokens() {
        let Some(token) = element.into_token() else { continue };
        // A name is classified by what it resolves to; the tree only decides
        // for what no symbol covers.
        let classified = match token.kind() {
            IDENT => resolved.get(&token.text_range()).and_then(|it| by_symbol(&model, it)),
            _ => None,
        };
        let Some((token_type, modifiers)) = classified.or_else(|| classify(&token)) else { continue };
        let position = index.position(token.text_range().start());
        // Multi-line tokens cannot be encoded, and Piton has none that matter.
        if token.text().contains('\n') {
            continue;
        }
        let delta_line = position.line - previous_line;
        let delta_start =
            if delta_line == 0 { position.character - previous_start } else { position.character };
        out.push(SemanticToken {
            delta_line,
            delta_start,
            length: token.text().encode_utf16().count() as u32,
            token_type,
            token_modifiers_bitset: modifiers,
        });
        previous_line = position.line;
        previous_start = position.character;
    }
    out
}

/// The token type of a name that resolved to a symbol.
fn by_symbol(model: &Model, occurrence: &Occurrence) -> Option<(u32, u32)> {
    let declared = occurrence.role == Role::Declaration;
    let declaration = if declared { DECLARATION } else { 0 };
    Some(match model.resolve_alias(occurrence.sym.clone())? {
        Sym::Anchor(id) => {
            let abstract_flag = if model.analysis().anchor_def(id).is_abstract { ABSTRACT } else { 0 };
            let definition = if declared { DEFINITION } else { 0 };
            (T_CLASS, declaration | definition | abstract_flag)
        }
        Sym::Keyword(_) => (T_FUNCTION, declaration),
        Sym::Property { .. } | Sym::Key { .. } => (T_PROPERTY, declaration),
        Sym::Var { .. } => (T_VARIABLE, if declared { DECLARATION } else { READONLY }),
        Sym::Module(_) => (T_NAMESPACE, 0),
        Sym::Builtin(_) => (T_TYPE, 0),
        Sym::Alias { .. } => return None,
    })
}

/// Map one token to a semantic type, using its parent for identifiers.
fn classify(token: &SyntaxToken) -> Option<(u32, u32)> {
    let parent = token.parent();
    Some(match token.kind() {
        WHITESPACE | NEWLINE | BLANK => return None,
        COMMENT => (T_COMMENT, 0),
        ANCHOR_KW | ABSTRACT_KW | EXTENDS_KW | AS_KW | EXPORT_KW | FROM_KW | IMPORT_KW | USE_KW => {
            (T_KEYWORD, 0)
        }
        THIS_KW | SELF_KW | SUPER_KW => (T_KEYWORD, READONLY),
        TRUE_KW | FALSE_KW | NULL_KW => (T_ENUM_MEMBER, READONLY),
        NUMBER => (T_NUMBER, 0),
        QUOTED_STRING | TEXT | ESCAPE => (T_STRING, 0),
        PATH => (T_NAMESPACE, 0),
        SIGIL => (T_DECORATOR, 0),
        COLON | COLON2 | COMMA | DOT | DASH | STAR | PLUS | PLUS2 | MINUS | SLASH | PERCENT
        | EQ2 | BANG_EQ | GT | LT | GT_EQ | LT_EQ | AMP2 | PIPE2 | QUESTION => (T_OPERATOR, 0),
        L_BRACE | R_BRACE | L_BRACK | R_BRACK | L_PAREN | R_PAREN => (T_OPERATOR, 0),
        IDENT => {
            let parent = parent?;
            match parent.kind() {
                ANCHOR_DECL => {
                    let declaration = piton_syntax::ast::AnchorDecl::cast(parent.clone())?;
                    if declaration.keyword_token().map(|it| it.text_range())
                        == Some(token.text_range())
                    {
                        (T_FUNCTION, 0)
                    } else {
                        let abstract_flag =
                            if declaration.abstract_token().is_some() { ABSTRACT } else { 0 };
                        (T_CLASS, DECLARATION | DEFINITION | abstract_flag)
                    }
                }
                AS_CLAUSE => (T_FUNCTION, DECLARATION),
                EXTENDS_CLAUSE => (T_CLASS, 0),
                TYPE_REF => (T_TYPE, 0),
                PROPERTY | VAR_DECL => (T_PROPERTY, DECLARATION),
                IMPORT_ITEM | EXPORT_DECL => (T_VARIABLE, 0),
                NAME_REF | FIELD_EXPR => (T_VARIABLE, READONLY),
                _ => (T_VARIABLE, 0),
            }
        }
        _ => return None,
    })
}

/// Fold every indented block and every wrapped import list.
pub fn folding_ranges(view: &View, file: FileId) -> Vec<FoldingRange> {
    let index = view.line_index(file);
    let root = view.compilation.analysis.db.file(file).parse.syntax();
    let mut out = Vec::new();
    for node in root.descendants() {
        let kind = match node.kind() {
            BLOCK => FoldingRangeKind::Region,
            IMPORT_LIST => FoldingRangeKind::Imports,
            _ => continue,
        };
        // Fold from the header line so the collapsed view keeps its key.
        let header = node.parent().unwrap_or_else(|| node.clone());
        let start = index.position(header.text_range().start()).line;
        let end = index.position(node.text_range().end()).line;
        if end > start {
            out.push(FoldingRange {
                start_line: start,
                end_line: end.saturating_sub(u32::from(node.kind() == BLOCK)),
                kind: Some(kind),
                ..FoldingRange::default()
            });
        }
    }
    out
}

/// Turn every import path into a clickable link.
pub fn document_links(view: &View, file: FileId) -> Vec<DocumentLink> {
    let index = view.line_index(file);
    let root = view.compilation.analysis.db.file(file).parse.syntax();
    let mut out = Vec::new();
    for token in root.descendants_with_tokens().filter_map(|it| it.into_token()) {
        if token.kind() != PATH {
            continue;
        }
        let Some(target) = view.compilation.analysis.module(file, token.text()) else {
            continue;
        };
        let Some(path) = view.path_of(target) else { continue };
        let Ok(url) = Url::from_file_path(path) else { continue };
        out.push(DocumentLink {
            range: index.range(token.text_range()),
            target: Some(url),
            tooltip: Some(format!("Open {}", token.text())),
            data: None,
        });
    }
    out
}

/// The inferred type after a key whose value is a braced expression and that
/// states no constraint: the one place the text does not already say what the
/// value is. Prose, literals, lists, and dictionaries show their type in how
/// they are written.
pub fn inlay_hints(view: &View, file: FileId) -> Vec<InlayHint> {
    let index = view.line_index(file);
    let root = view.compilation.analysis.db.file(file).parse.syntax();
    let mut out = Vec::new();
    for node in root.descendants() {
        if !matches!(node.kind(), PROPERTY | VAR_DECL) {
            continue;
        }
        if node.children().any(|child| child.kind() == TYPE_ANNOTATION) {
            continue;
        }
        let Some(value) = node.children().find(|child| child.kind() == VALUE) else { continue };
        if !value.children().any(|child| child.kind() == BRACE_EXPR) {
            continue;
        }
        let Some(name) = node
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == IDENT)
        else {
            continue;
        };
        let Some(compiled) = compiled_value(view, file, &node) else { continue };
        out.push(type_hint(index, name.text_range().end(), &compiled));
    }
    out
}

/// The compiled value of a key, found by walking from its anchor or variable
/// down through the keys that hold it.
fn compiled_value(view: &View, file: FileId, node: &SyntaxNode) -> Option<Value> {
    let analysis = &view.compilation.analysis;
    let hir = &analysis.db.file(file).hir;
    let key = |node: &SyntaxNode| {
        node.children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == IDENT)
            .map(|it| it.text().to_string())
    };
    let mut path: Vec<String> = Vec::new();
    let mut current = Some(node.clone());
    while let Some(at) = current {
        match at.kind() {
            PROPERTY => path.push(key(&at)?),
            // A key inside a list item is not reachable by name.
            LIST_ITEM | SPREAD_ITEM => return None,
            VAR_DECL => {
                let position = hir.vars.iter().position(|it| it.range == at.text_range())?;
                let value = view.compilation.vars.get(&(file, position))?.clone();
                return follow(value, path.iter().rev());
            }
            ANCHOR_DECL => {
                let position = hir.anchors.iter().position(|it| it.range == at.text_range())?;
                let anchor = view.compilation.anchor(analysis.anchor_id(file, position)?)?;
                let (first, rest) = path.split_last()?;
                let value = anchor.props.get(first)?.clone();
                return follow(value, rest.iter().rev());
            }
            _ => {}
        }
        current = at.parent();
    }
    None
}

fn follow<'p>(mut value: Value, path: impl Iterator<Item = &'p String>) -> Option<Value> {
    for segment in path {
        value = value.field(segment)?.clone();
    }
    Some(value)
}

fn type_hint(
    index: &crate::line_index::LineIndex,
    at: piton_syntax::TextSize,
    value: &Value,
) -> InlayHint {
    InlayHint {
        position: index.position(at),
        label: InlayHintLabel::String(format!(":: {}", value.type_name())),
        kind: Some(InlayHintKind::TYPE),
        text_edits: None,
        tooltip: None,
        padding_left: Some(false),
        padding_right: Some(false),
        data: None,
    }
}

/// The outline of a file: anchors, their properties, and top-level variables.
pub fn document_symbols(view: &View, file: FileId) -> Vec<DocumentSymbol> {
    let index = view.line_index(file);
    let hir = &view.compilation.analysis.db.file(file).hir;
    let mut out = Vec::new();

    for anchor in &hir.anchors {
        let mut properties = Vec::new();
        collect(&anchor.body, &mut properties);
        let mut detail: Vec<String> = Vec::new();
        if let Some(keyword) = &anchor.via_keyword {
            detail.push(keyword.value.clone());
        }
        if !anchor.bases.is_empty() {
            let bases: Vec<&str> = anchor.bases.iter().map(|it| it.value.as_str()).collect();
            detail.push(format!("extends {}", bases.join(", ")));
        }
        if let Some(keyword) = &anchor.keyword {
            detail.push(format!("as {}", keyword.value));
        }
        out.push(symbol(
            anchor.name.clone(),
            if anchor.is_abstract { SymbolKind::INTERFACE } else { SymbolKind::CLASS },
            (!detail.is_empty()).then(|| detail.join(" ")),
            index.range(anchor.range),
            index.range(anchor.name_range),
            properties.iter().map(|property| property_symbol(index, property)).collect(),
        ));
    }
    for variable in &hir.vars {
        let mut keys = Vec::new();
        collect(&variable.body, &mut keys);
        out.push(symbol(
            variable.name.clone(),
            SymbolKind::CONSTANT,
            constraints_detail(&variable.constraints),
            index.range(variable.range),
            index.range(variable.name_range),
            keys.iter().map(|key| property_symbol(index, key)).collect(),
        ));
    }
    out
}

fn property_symbol(index: &crate::line_index::LineIndex, property: &Property) -> DocumentSymbol {
    let mut nested = Vec::new();
    collect(&property.node, &mut nested);
    symbol(
        property.name.clone(),
        SymbolKind::PROPERTY,
        constraints_detail(&property.constraints),
        index.range(property.range),
        index.range(property.name_range),
        nested.iter().map(|child| property_symbol(index, child)).collect(),
    )
}

fn constraints_detail(constraints: &[piton_core::hir::TypeExpr]) -> Option<String> {
    (!constraints.is_empty())
        .then(|| constraints.iter().map(|it| format!(":: {}", it.render())).collect::<String>())
}

#[allow(deprecated)]
fn symbol(
    name: String,
    kind: SymbolKind,
    detail: Option<String>,
    range: tower_lsp::lsp_types::Range,
    selection_range: tower_lsp::lsp_types::Range,
    children: Vec<DocumentSymbol>,
) -> DocumentSymbol {
    DocumentSymbol {
        name,
        detail,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: if children.is_empty() { None } else { Some(children) },
    }
}
