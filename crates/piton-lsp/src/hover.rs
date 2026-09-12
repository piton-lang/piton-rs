//! Hover text and inlay hints: what the compiler already knows about a name.

use piton_core::hir::TypeExpr;
use piton_core::resolve::Symbol;
use piton_core::serialize::to_json_string;
use piton_core::value::{AnchorId, Value};
use piton_core::FileId;

use crate::navigation::{property_declaration, visible_properties, Resolved, Target};
use crate::world::View;

/// The Markdown shown when hovering a resolved token.
pub fn hover(view: &View, resolved: &Resolved) -> Option<String> {
    let analysis = &view.compilation.analysis;
    match &resolved.target {
        Target::Symbol(Symbol::Anchor(id)) | Target::Keyword(id) => Some(anchor_hover(view, *id)),
        Target::Symbol(Symbol::Var { file, index }) => {
            let variable = &analysis.db.file(*file).hir.vars[*index];
            let mut out = format!("```piton\n{}", variable.name);
            for constraint in &variable.constraints {
                out.push_str(&format!(":: {}", constraint.render()));
            }
            out.push_str("\n```\n");
            if let Some(value) = view.compilation.vars.get(&(*file, *index)) {
                out.push_str(&value_preview(value));
            }
            if let Some(doc) = &variable.doc {
                out.push_str(&format!("\n{doc}\n"));
            }
            Some(out)
        }
        Target::Module(file) => Some(module_hover(view, *file)),
        Target::Builtin(name) => Some(builtin_hover(name)),
        Target::Property { owner, name } => Some(property_hover(view, *owner, name)),
    }
}

fn anchor_hover(view: &View, id: AnchorId) -> String {
    let analysis = &view.compilation.analysis;
    let definition = analysis.anchor_def(id);
    let mut signature = String::new();
    if definition.exported {
        signature.push_str("export ");
    }
    if definition.is_abstract {
        signature.push_str("abstract ");
    }
    signature.push_str(&format!("anchor {}", definition.name));
    let bases: Vec<String> =
        analysis.bases(id).iter().map(|base| analysis.anchor_def(*base).name.clone()).collect();
    if !bases.is_empty() {
        signature.push_str(&format!(" extends {}", bases.join(", ")));
    }
    if let Some(keyword) = &definition.keyword {
        signature.push_str(&format!(" as {}", keyword.value));
    }

    let mut out = format!("```piton\n{signature}:\n```\n");
    if let Some(doc) = &definition.doc {
        out.push_str(&format!("\n{doc}\n"));
    }
    let properties = visible_properties(view, id);
    if !properties.is_empty() {
        out.push_str("\n**Properties**\n\n");
        for property in properties.iter().take(24) {
            let constraint = render_constraints(&property.constraints);
            out.push_str(&format!("- `{}{constraint}`\n", property.name));
        }
    }
    if definition.is_abstract {
        let implementors = analysis.implementors(id);
        if !implementors.is_empty() {
            out.push_str(&format!("\nImplemented by {} anchor(s).\n", implementors.len()));
        }
    }
    out
}

fn property_hover(view: &View, owner: Option<AnchorId>, name: &str) -> String {
    let Some(owner) = owner else {
        return format!("```piton\n{name}\n```\n");
    };
    let properties = visible_properties(view, owner);
    let Some(property) = properties.iter().find(|property| property.name == name) else {
        return format!("```piton\n{name}\n```\n\nNo property named `{name}` on this anchor.");
    };
    let mut out =
        format!("```piton\n{}{}\n```\n", property.name, render_constraints(&property.constraints));
    if let Some(anchor) = view.compilation.anchor(owner) {
        if let Some(value) = anchor.props.get(name) {
            out.push_str(&value_preview(value));
        }
    }
    if let Some(doc) = &property.doc {
        out.push_str(&format!("\n{doc}\n"));
    }
    if property_declaration(view, owner, name).is_some() {
        let source = view.compilation.analysis.anchor_def(owner).name.clone();
        out.push_str(&format!("\nOn `{source}`.\n"));
    }
    out
}

fn module_hover(view: &View, file: FileId) -> String {
    let analysis = &view.compilation.analysis;
    let exports = &analysis.scope(file).exports;
    let source = &analysis.db.file(file).source;
    let name = match source.as_path() {
        Some(path) => view.display_path(path),
        None => source.display(),
    };
    let mut out = format!("```piton\n{name}\n```\n");
    if exports.is_empty() {
        out.push_str("\nThis module exports nothing.\n");
        return out;
    }
    out.push_str("\n**Exports**\n\n");
    for (name, symbol) in exports.iter().take(40) {
        let kind = match symbol {
            Symbol::Anchor(id) => {
                let definition = analysis.anchor_def(*id);
                match &definition.keyword {
                    Some(keyword) => format!("anchor, keyword `{}`", keyword.value),
                    None => "anchor".to_string(),
                }
            }
            Symbol::Var { .. } => "variable".to_string(),
        };
        out.push_str(&format!("- `{name}` — {kind}\n"));
    }
    out
}

fn builtin_hover(name: &str) -> String {
    let description = match name {
        "string" => "A Unicode string. Written unquoted; quote it to defeat coercion.",
        "number" => "A single numeric type; integers and floats are not distinguished.",
        "boolean" => "`true` or `false`.",
        "null" => "The absence of a value. `null == null`.",
        "list" => "An ordered collection. `+` deduplicates, `++` does not.",
        "dictionary" => "Ordered key/value pairs. `+` merges shallowly, `++` deeply.",
        "anchor" => "A named structural declaration.",
        "any" => "Accepts every type.",
        "simple" => "Accepts `string`, `number`, `boolean`, and `null`.",
        "complex" => "Accepts `list`, `dictionary`, and `anchor`.",
        _ => "A built-in Piton type.",
    };
    format!("```piton\n{name}\n```\n\n{description}\n")
}

fn render_constraints(constraints: &[TypeExpr]) -> String {
    constraints.iter().map(|it| format!(":: {}", it.render())).collect::<Vec<_>>().join("")
}

/// A short rendering of a compiled value, elided when it is large.
pub fn value_preview(value: &Value) -> String {
    let rendered = to_json_string(value, true);
    let text = if rendered.lines().count() > 20 {
        let head: Vec<&str> = rendered.lines().take(20).collect();
        format!("{}\n…", head.join("\n"))
    } else {
        rendered
    };
    format!("\n```json\n{text}\n```\n")
}
