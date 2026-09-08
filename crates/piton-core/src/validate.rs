//! Structural checks that do not need values.
//!
//! Anything that can be decided from the tree and the resolved scopes is
//! decided here, so that evaluation only has to report problems it uniquely
//! knows about.

use std::collections::{HashMap, HashSet};

use piton_syntax::kind::{RESERVED_KEYWORDS, SyntaxKind};
use piton_syntax::{SyntaxNode, TextRange};

use crate::diag::{Diagnostic, Diagnostics};
use crate::hir::{Node, Property, TypeExpr};
use crate::resolve::{Analysis, Symbol};
use crate::types;
use crate::value::AnchorId;
use crate::FileId;

/// Run every structural check over the workspace.
pub fn validate(analysis: &Analysis) -> Diagnostics {
    let mut diagnostics = Diagnostics::default();
    for file in analysis.db.files() {
        syntax_errors(file.id, &file.parse, &mut diagnostics);
        duplicate_declarations(analysis, file.id, &mut diagnostics);
        unresolved_names(analysis, file.id, &mut diagnostics);
    }
    for id in analysis.anchor_ids() {
        anchor_checks(analysis, id, &mut diagnostics);
    }
    diagnostics
}

fn syntax_errors(file: FileId, parse: &piton_syntax::Parse, diagnostics: &mut Diagnostics) {
    for error in &parse.errors {
        diagnostics.push(Diagnostic::error("syntax", file, error.range, error.message.clone()));
    }
    fn walk(node: &SyntaxNode, file: FileId, diagnostics: &mut Diagnostics) {
        if node.kind() == SyntaxKind::ERROR {
            let text = node.text().to_string();
            let snippet = text.trim();
            diagnostics.push(Diagnostic::error(
                "syntax",
                file,
                node.text_range(),
                if snippet.is_empty() {
                    "unexpected indentation".to_string()
                } else {
                    format!("cannot parse `{}`", first_line(snippet))
                },
            ));
            return;
        }
        for child in node.children() {
            walk(&child, file, diagnostics);
        }
    }
    walk(&parse.syntax(), file, diagnostics);
}

fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default();
    if line.chars().count() > 60 {
        format!("{}…", line.chars().take(60).collect::<String>())
    } else {
        line.to_string()
    }
}

fn duplicate_declarations(analysis: &Analysis, file: FileId, diagnostics: &mut Diagnostics) {
    let hir = &analysis.db.file(file).hir;
    let mut seen: HashMap<&str, TextRange> = HashMap::new();
    let declarations = hir
        .anchors
        .iter()
        .map(|it| (it.name.as_str(), it.name_range))
        .chain(hir.vars.iter().map(|it| (it.name.as_str(), it.name_range)));
    for (name, range) in declarations {
        if seen.insert(name, range).is_some() {
            diagnostics.push(Diagnostic::error(
                "duplicate",
                file,
                range,
                format!("`{name}` is already declared in this file"),
            ));
        }
    }
}

/// Every base and declaring keyword has to resolve to something.
fn unresolved_names(analysis: &Analysis, file: FileId, diagnostics: &mut Diagnostics) {
    let scope = analysis.scope(file);
    for anchor in &analysis.db.file(file).hir.anchors {
        if let Some(keyword) = &anchor.via_keyword {
            if !scope.keywords.contains_key(&keyword.value) {
                diagnostics.push(Diagnostic::error(
                    "unknown-keyword",
                    file,
                    keyword.range,
                    format!(
                        "`{}` is not a keyword here; declare it with `as` or bring it in with `use`",
                        keyword.value
                    ),
                ));
            }
        }
        if let Some(keyword) = &anchor.keyword {
            let text = &keyword.value;
            if RESERVED_KEYWORDS.contains(&text.as_str()) {
                diagnostics.push(Diagnostic::error(
                    "reserved-keyword",
                    file,
                    keyword.range,
                    format!("`{text}` is a reserved word and cannot be a user-defined keyword"),
                ));
            } else if text.chars().any(|c| c.is_uppercase()) {
                diagnostics.push(Diagnostic::error(
                    "keyword-case",
                    file,
                    keyword.range,
                    format!("user-defined keyword `{text}` must be lowercase, optionally kebab-case"),
                ));
            }
        }
        for base in &anchor.bases {
            if !matches!(scope.names.get(&base.value), Some(Symbol::Anchor(_))) {
                diagnostics.push(Diagnostic::error(
                    "unknown-base",
                    file,
                    base.range,
                    format!("cannot find anchor `{}`", base.value),
                ));
            }
        }
    }
    for var in &analysis.db.file(file).hir.vars {
        check_type_names(analysis, file, &var.constraints, diagnostics);
        check_node_types(analysis, file, &var.body, diagnostics);
    }
    for anchor in &analysis.db.file(file).hir.anchors {
        check_node_types(analysis, file, &anchor.body, diagnostics);
    }
}

fn check_node_types(
    analysis: &Analysis,
    file: FileId,
    node: &Node,
    diagnostics: &mut Diagnostics,
) {
    match node {
        Node::Dict(properties) => {
            for property in properties {
                check_type_names(analysis, file, &property.constraints, diagnostics);
                check_node_types(analysis, file, &property.node, diagnostics);
            }
        }
        Node::Mixed(nodes) => {
            nodes.iter().for_each(|node| check_node_types(analysis, file, node, diagnostics))
        }
        Node::List(elements) | Node::Merge(elements) => elements
            .iter()
            .for_each(|element| check_node_types(analysis, file, &element.node, diagnostics)),
        _ => {}
    }
}

fn check_type_names(
    analysis: &Analysis,
    file: FileId,
    constraints: &[TypeExpr],
    diagnostics: &mut Diagnostics,
) {
    for constraint in constraints {
        match constraint {
            TypeExpr::Named { name, range } => {
                if !types::is_builtin(name)
                    && !matches!(analysis.scope(file).names.get(name), Some(Symbol::Anchor(_)))
                {
                    diagnostics.push(Diagnostic::error(
                        "unknown-type",
                        file,
                        *range,
                        format!("unknown type `{name}`"),
                    ));
                }
            }
            TypeExpr::ListOf { element, .. } => {
                check_type_names(analysis, file, std::slice::from_ref(element), diagnostics)
            }
            TypeExpr::Extends { base, .. } => {
                check_type_names(analysis, file, std::slice::from_ref(base), diagnostics)
            }
        }
    }
}

/// Checks that need the inheritance chain.
fn anchor_checks(analysis: &Analysis, id: AnchorId, diagnostics: &mut Diagnostics) {
    let loc = analysis.anchor_loc(id);
    let def = analysis.anchor_def(id);
    let file = loc.file;

    if analysis.ancestors(id).contains(&id) {
        diagnostics.push(Diagnostic::error(
            "inheritance-cycle",
            file,
            def.name_range,
            format!("`{}` inherits from itself", def.name),
        ));
        return;
    }

    // `extends` as a type constraint only means something inside an abstract.
    if !def.is_abstract {
        let mut properties = Vec::new();
        collect(&def.body, &mut properties);
        for property in &properties {
            for constraint in &property.constraints {
                if let TypeExpr::Extends { range, .. } = constraint {
                    diagnostics.push(Diagnostic::error(
                        "extends-constraint",
                        file,
                        *range,
                        "`extends` constraints are only allowed inside abstract anchors",
                    ));
                }
            }
        }
    }

    let abstract_bases: Vec<AnchorId> = analysis
        .bases(id)
        .into_iter()
        .filter(|base| analysis.anchor_def(*base).is_abstract)
        .collect();
    if !def.is_abstract && abstract_bases.len() > 1 {
        let names: Vec<&str> =
            abstract_bases.iter().map(|base| analysis.anchor_def(*base).name.as_str()).collect();
        diagnostics.push(Diagnostic::error(
            "multiple-abstracts",
            file,
            def.name_range,
            format!(
                "`{}` implements more than one abstract anchor ({})",
                def.name,
                names.join(", ")
            ),
        ));
    }

    // Two abstract bases that constrain the same property differently leave no
    // way to decide which shape applies.
    if abstract_bases.len() > 1 {
        let mut seen: HashMap<String, (String, &str)> = HashMap::new();
        for base in &abstract_bases {
            let base_name = analysis.anchor_def(*base).name.as_str();
            for (name, constraint) in constraint_map(analysis, *base) {
                match seen.get(&name) {
                    Some((existing, from)) if *existing != constraint => {
                        diagnostics.push(Diagnostic::error(
                            "conflicting-constraints",
                            file,
                            def.name_range,
                            format!(
                                "`{name}` is constrained to `{existing}` by `{from}` and to \
                                 `{constraint}` by `{base_name}`"
                            ),
                        ));
                    }
                    Some(_) => {}
                    None => {
                        seen.insert(name, (constraint, base_name));
                    }
                }
            }
        }
    }

    if def.is_abstract {
        return;
    }
    // Every abstract property left without a value must be implemented.
    let mut defined: HashSet<String> = HashSet::new();
    let mut required: Vec<(String, AnchorId)> = Vec::new();
    let mut chain = analysis.ancestors(id);
    chain.reverse();
    chain.push(id);
    for link in chain {
        let mut properties = Vec::new();
        collect(&analysis.anchor_def(link).body, &mut properties);
        let is_abstract = analysis.anchor_def(link).is_abstract;
        for property in properties {
            if is_abstract && property.node.is_empty() {
                required.push((property.name.clone(), link));
            } else {
                defined.insert(property.name.clone());
            }
        }
    }
    for (name, source) in required {
        if !defined.contains(&name) {
            diagnostics.push(Diagnostic::error(
                "unimplemented",
                file,
                def.name_range,
                format!(
                    "`{}` must define `{name}`, required by abstract anchor `{}`",
                    def.name,
                    analysis.anchor_def(source).name
                ),
            ));
        }
    }
}

/// Every constraint an anchor's chain declares, rendered for comparison.
fn constraint_map(analysis: &Analysis, id: AnchorId) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut chain = analysis.ancestors(id);
    chain.reverse();
    chain.push(id);
    for link in chain {
        let mut properties = Vec::new();
        collect(&analysis.anchor_def(link).body, &mut properties);
        for property in properties {
            if property.constraints.is_empty() {
                continue;
            }
            let rendered =
                property.constraints.iter().map(TypeExpr::render).collect::<Vec<_>>().join(":: ");
            out.insert(property.name, rendered);
        }
    }
    out
}

fn collect(node: &Node, out: &mut Vec<Property>) {
    match node {
        Node::Dict(properties) => out.extend(properties.iter().cloned()),
        Node::Mixed(nodes) => nodes.iter().for_each(|node| collect(node, out)),
        Node::List(elements) | Node::Merge(elements) => {
            elements.iter().for_each(|element| collect(&element.node, out))
        }
        _ => {}
    }
}
