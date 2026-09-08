//! Features that read the syntax tree directly: semantic tokens, folding,
//! document links, inlay hints, and document symbols.

use piton_core::hir::{Node, Property};
use piton_core::value::Value;
use piton_core::FileId;
use piton_syntax::kind::SyntaxKind::*;
use piton_syntax::{ast::AstNode, SyntaxToken};
use tower_lsp::lsp_types::{
    DocumentLink, DocumentSymbol, FoldingRange, FoldingRangeKind, InlayHint, InlayHintKind,
    InlayHintLabel, SemanticToken, SemanticTokenModifier, SemanticTokenType, SymbolKind, Url,
};

use crate::world::Snapshot;

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
pub fn semantic_tokens(snapshot: &Snapshot, file: FileId) -> Vec<SemanticToken> {
    let index = snapshot.line_index(file);
    let root = snapshot.compilation.analysis.db.file(file).parse.syntax();
    let mut out = Vec::new();
    let mut previous_line = 0u32;
    let mut previous_start = 0u32;

    for element in root.descendants_with_tokens() {
        let Some(token) = element.into_token() else { continue };
        let Some((token_type, modifiers)) = classify(&token) else { continue };
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
pub fn folding_ranges(snapshot: &Snapshot, file: FileId) -> Vec<FoldingRange> {
    let index = snapshot.line_index(file);
    let root = snapshot.compilation.analysis.db.file(file).parse.syntax();
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
pub fn document_links(snapshot: &Snapshot, file: FileId) -> Vec<DocumentLink> {
    let index = snapshot.line_index(file);
    let root = snapshot.compilation.analysis.db.file(file).parse.syntax();
    let mut out = Vec::new();
    for token in root.descendants_with_tokens().filter_map(|it| it.into_token()) {
        if token.kind() != PATH {
            continue;
        }
        let Some(target) = snapshot.compilation.analysis.module(file, token.text()) else {
            continue;
        };
        let Some(path) = snapshot.path_of(target) else { continue };
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

/// Show the inferred type wherever the author did not write a constraint.
pub fn inlay_hints(snapshot: &Snapshot, file: FileId) -> Vec<InlayHint> {
    let index = snapshot.line_index(file);
    let hir = &snapshot.compilation.analysis.db.file(file).hir;
    let mut out = Vec::new();

    for (position, variable) in hir.vars.iter().enumerate() {
        if !variable.constraints.is_empty() {
            continue;
        }
        if let Some(value) = snapshot.compilation.vars.get(&(file, position)) {
            out.push(type_hint(index, variable.name_range.end(), value));
        }
    }
    for (position, anchor) in hir.anchors.iter().enumerate() {
        let Some(id) = snapshot.compilation.analysis.anchor_id(file, position) else { continue };
        let Some(compiled) = snapshot.compilation.anchor(id) else { continue };
        let mut properties = Vec::new();
        collect(&anchor.body, &mut properties);
        for property in properties {
            if !property.constraints.is_empty() {
                continue;
            }
            if let Some(value) = compiled.props.get(&property.name) {
                out.push(type_hint(index, property.name_range.end(), value));
            }
        }
    }
    out
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
pub fn document_symbols(snapshot: &Snapshot, file: FileId) -> Vec<DocumentSymbol> {
    let index = snapshot.line_index(file);
    let hir = &snapshot.compilation.analysis.db.file(file).hir;
    let mut out = Vec::new();

    for anchor in &hir.anchors {
        let mut properties = Vec::new();
        collect(&anchor.body, &mut properties);
        out.push(symbol(
            anchor.name.clone(),
            if anchor.is_abstract { SymbolKind::INTERFACE } else { SymbolKind::CLASS },
            anchor.doc.clone(),
            index.range(anchor.range),
            index.range(anchor.name_range),
            properties.iter().map(|property| property_symbol(index, property)).collect(),
        ));
    }
    for variable in &hir.vars {
        out.push(symbol(
            variable.name.clone(),
            SymbolKind::CONSTANT,
            variable.doc.clone(),
            index.range(variable.range),
            index.range(variable.name_range),
            Vec::new(),
        ));
    }
    out
}

fn property_symbol(
    index: &crate::line_index::LineIndex,
    property: &Property,
) -> DocumentSymbol {
    let mut nested = Vec::new();
    collect(&property.node, &mut nested);
    symbol(
        property.name.clone(),
        SymbolKind::PROPERTY,
        property.doc.clone(),
        index.range(property.range),
        index.range(property.name_range),
        nested.iter().map(|child| property_symbol(index, child)).collect(),
    )
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

/// Properties written directly in a node, including inside a mixed block.
pub fn collect(node: &Node, out: &mut Vec<Property>) {
    match node {
        Node::Dict(properties) => out.extend(properties.iter().cloned()),
        Node::Mixed(nodes) => nodes.iter().for_each(|node| collect(node, out)),
        Node::List(elements) | Node::Merge(elements) => {
            elements.iter().for_each(|element| collect(&element.node, out))
        }
        _ => {}
    }
}
