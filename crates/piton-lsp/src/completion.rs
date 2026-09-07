//! Completion.
//!
//! Piton has few places where a name can appear, so completion is driven by
//! what the line looks like to the left of the cursor rather than by a general
//! expression-context analysis.

use piton_core::resolve::Symbol;
use piton_core::FileId;
use piton_syntax::kind::{BUILTIN_TYPES, RESERVED_KEYWORDS, SELF_KEYWORDS};
use piton_syntax::TextSize;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent, MarkupKind,
};

use crate::navigation::{enclosing_anchor, token_at, visible_properties};
use crate::world::Snapshot;

/// Where in a line the cursor sits.
enum Context {
    /// Inside `{ ... }`, where bare words are references.
    Expression,
    /// Just after `::`, so a type is expected.
    Type,
    /// Just after `extends`.
    Base,
    /// At the start of a line inside an anchor body.
    Property,
    /// At the start of a top-level line.
    TopLevel,
}

/// Suggest completions at an offset.
pub fn complete(snapshot: &Snapshot, file: FileId, offset: TextSize) -> Vec<CompletionItem> {
    let text = snapshot.text(file);
    let start = usize::from(offset).min(text.len());
    let line_start = text[..start].rfind('\n').map_or(0, |it| it + 1);
    let prefix = &text[line_start..start];

    match context(prefix) {
        Context::Expression => expression_items(snapshot, file, offset),
        Context::Type => type_items(snapshot, file),
        Context::Base => anchor_items(snapshot, file),
        Context::Property => property_items(snapshot, file, offset),
        Context::TopLevel => top_level_items(snapshot, file),
    }
}

fn context(prefix: &str) -> Context {
    let open = prefix.matches('{').count();
    let close = prefix.matches('}').count();
    if open > close {
        return Context::Expression;
    }
    let trimmed = prefix.trim_end_matches(|c: char| c.is_alphanumeric() || c == '_' || c == '-');
    if trimmed.trim_end().ends_with("::") {
        return Context::Type;
    }
    if trimmed.trim_end().ends_with("extends") || trimmed.trim_end().ends_with(',') {
        return Context::Base;
    }
    if prefix.starts_with([' ', '\t']) {
        Context::Property
    } else {
        Context::TopLevel
    }
}

fn item(label: &str, kind: CompletionItemKind, detail: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(kind),
        detail: Some(detail.to_string()),
        ..CompletionItem::default()
    }
}

fn snippet(label: &str, insert: &str, detail: &str, docs: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::SNIPPET),
        detail: Some(detail.to_string()),
        insert_text: Some(insert.to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: docs.to_string(),
        })),
        ..CompletionItem::default()
    }
}

/// Names visible in an expression, plus the self-reference keywords.
fn expression_items(snapshot: &Snapshot, file: FileId, offset: TextSize) -> Vec<CompletionItem> {
    let analysis = &snapshot.compilation.analysis;
    let mut items: Vec<CompletionItem> = analysis
        .scope(file)
        .names
        .iter()
        .map(|(name, symbol)| match symbol {
            Symbol::Anchor(id) => item(
                name,
                CompletionItemKind::CLASS,
                if analysis.anchor_def(*id).is_abstract { "abstract anchor" } else { "anchor" },
            ),
            Symbol::Var { .. } => item(name, CompletionItemKind::VARIABLE, "variable"),
        })
        .collect();

    let root = analysis.db.file(file).parse.syntax();
    let inside_anchor = token_at(&root, offset)
        .and_then(|token| token.parent())
        .and_then(|node| enclosing_anchor(snapshot, file, &node));
    if let Some(anchor) = inside_anchor {
        for keyword in SELF_KEYWORDS {
            items.push(item(keyword, CompletionItemKind::KEYWORD, "self reference"));
        }
        for property in visible_properties(snapshot, anchor) {
            items.push(item(
                &format!("self.{}", property.name),
                CompletionItemKind::PROPERTY,
                "property of the most-derived anchor",
            ));
        }
    }
    items
}

fn type_items(snapshot: &Snapshot, file: FileId) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = BUILTIN_TYPES
        .iter()
        .map(|name| item(name, CompletionItemKind::KEYWORD, "built-in type"))
        .collect();
    items.push(item("extends", CompletionItemKind::KEYWORD, "any anchor whose chain includes"));
    items.extend(anchor_items(snapshot, file));
    items
}

fn anchor_items(snapshot: &Snapshot, file: FileId) -> Vec<CompletionItem> {
    let analysis = &snapshot.compilation.analysis;
    analysis
        .scope(file)
        .names
        .iter()
        .filter_map(|(name, symbol)| match symbol {
            Symbol::Anchor(id) => Some(item(
                name,
                CompletionItemKind::CLASS,
                if analysis.anchor_def(*id).is_abstract { "abstract anchor" } else { "anchor" },
            )),
            Symbol::Var { .. } => None,
        })
        .collect()
}

/// Inside an anchor body: the properties it inherits but has not written yet.
fn property_items(snapshot: &Snapshot, file: FileId, offset: TextSize) -> Vec<CompletionItem> {
    let analysis = &snapshot.compilation.analysis;
    let root = analysis.db.file(file).parse.syntax();
    let anchor = token_at(&root, offset)
        .and_then(|token| token.parent())
        .and_then(|node| enclosing_anchor(snapshot, file, &node));
    let mut items = Vec::new();
    if let Some(anchor) = anchor {
        let own: Vec<String> = {
            let mut properties = Vec::new();
            collect_names(&analysis.anchor_def(anchor).body, &mut properties);
            properties
        };
        for property in visible_properties(snapshot, anchor) {
            if own.contains(&property.name) {
                continue;
            }
            let detail = property
                .constraints
                .first()
                .map(|constraint| constraint.render())
                .unwrap_or_else(|| "inherited property".to_string());
            let mut completion =
                item(&property.name, CompletionItemKind::PROPERTY, &detail);
            completion.insert_text = Some(format!("{}: ", property.name));
            items.push(completion);
        }
    }
    items.extend(value_keywords());
    items
}

fn top_level_items(snapshot: &Snapshot, file: FileId) -> Vec<CompletionItem> {
    let analysis = &snapshot.compilation.analysis;
    let mut items: Vec<CompletionItem> = RESERVED_KEYWORDS
        .iter()
        .filter(|word| !SELF_KEYWORDS.contains(word))
        .map(|word| item(word, CompletionItemKind::KEYWORD, "keyword"))
        .collect();

    for (keyword, id) in &analysis.scope(file).keywords {
        let definition = analysis.anchor_def(*id);
        let mut completion = item(
            keyword,
            CompletionItemKind::FUNCTION,
            &format!("shorthand for `extends {}`", definition.name),
        );
        completion.insert_text = Some(format!("{keyword} ${{1:Name}}:\n    $0"));
        completion.insert_text_format = Some(InsertTextFormat::SNIPPET);
        if let Some(doc) = &definition.doc {
            completion.documentation = Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: doc.clone(),
            }));
        }
        items.push(completion);
    }

    items.push(snippet(
        "anchor",
        "anchor ${1:Name}:\n    $0",
        "declare an anchor",
        "A named structural declaration.",
    ));
    items.push(snippet(
        "abstract anchor",
        "abstract anchor ${1:Name} as ${2:keyword}:\n    ${3:property}:: string\n",
        "declare an abstract anchor and its keyword",
        "Abstract anchors describe a shape; exporting one `as` a keyword is the \
         idiomatic way to make it implementable exactly once.",
    ));
    items.push(snippet(
        "from … import",
        "from ${1:./module} import ${2:Name}",
        "import names from a module",
        "Only exported names can be imported. `use` brings in keywords instead.",
    ));
    items.push(snippet("use", "use ${1:./module}", "bring in keywords", "`use` imports only user-defined keywords."));
    items
}

fn value_keywords() -> Vec<CompletionItem> {
    vec![
        item("true", CompletionItemKind::CONSTANT, "boolean"),
        item("false", CompletionItemKind::CONSTANT, "boolean"),
        item("null", CompletionItemKind::CONSTANT, "the absence of a value"),
    ]
}

fn collect_names(node: &piton_core::hir::Node, out: &mut Vec<String>) {
    use piton_core::hir::Node;
    match node {
        Node::Dict(properties) => out.extend(properties.iter().map(|it| it.name.clone())),
        Node::Mixed(nodes) => nodes.iter().for_each(|node| collect_names(node, out)),
        _ => {}
    }
}
