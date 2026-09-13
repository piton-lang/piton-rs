//! Hover: what the compiler knows about the name under the cursor.

use piton_core::hir::TypeExpr;
use piton_core::serialize::to_json_string;
use piton_core::value::{AnchorId, Value};
use piton_core::FileId;

use crate::index::{Access, AliasItem, Located, Model, SelfKind, SelfReference, Sym};
use crate::world::View;

/// The Markdown shown for whatever is under the cursor.
pub fn hover(view: &View, located: &Located) -> Option<String> {
    let model = view.model();
    match located {
        Located::SelfReference(reference) => Some(self_hover(&model, reference)),
        Located::Symbol(occurrence) => match (&occurrence.sym, occurrence.access) {
            (Sym::Property { name, .. }, Some(Access::Anchor(id))) => {
                property_hover(view, &model, id, name, false)
            }
            (Sym::Property { name, .. }, Some(Access::Super(id))) => {
                property_hover(view, &model, id, name, true)
            }
            (Sym::Key { .. }, _) => Some(key_hover(view, &model, &occurrence.sym)),
            (sym, _) => symbol_hover(view, &model, sym),
        },
    }
}

fn symbol_hover(view: &View, model: &Model, sym: &Sym) -> Option<String> {
    let analysis = model.analysis();
    match sym {
        Sym::Anchor(id) => Some(anchor_hover(model, *id)),
        Sym::Keyword(id) => {
            let definition = analysis.anchor_def(*id);
            let keyword = &definition.keyword.as_ref()?.value;
            Some(format!(
                "keyword `{keyword}`: a declaration written with it extends `{}`.\n\n{}",
                definition.name,
                anchor_hover(model, *id)
            ))
        }
        Sym::Var { file, index } => Some(var_hover(view, *file, *index)),
        Sym::Alias { file, item } => {
            let target = model.resolve_alias(sym.clone())?;
            let name = model.name_of(sym)?;
            let original = model.name_of(&target).unwrap_or_default();
            let from = alias_path(model, *file, *item)?;
            let rest = symbol_hover(view, model, &target).unwrap_or_default();
            Some(format!("`{name}` is an alias of `{original}` from `{from}`.\n\n{rest}"))
        }
        Sym::Module(file) => Some(module_hover(view, *file)),
        Sym::Builtin(name) => Some(builtin_hover(name)),
        Sym::Property { .. } | Sym::Key { .. } => None,
    }
}

fn alias_path(model: &Model, file: FileId, item: AliasItem) -> Option<String> {
    let hir = &model.analysis().db.file(file).hir;
    Some(match item {
        AliasItem::Import { decl, .. } => hir.imports.get(decl)?.path.value.clone(),
        AliasItem::Reexport { decl, .. } => hir.reexports.get(decl)?.path.value.clone(),
    })
}

fn anchor_hover(model: &Model, id: AnchorId) -> String {
    let analysis = model.analysis();
    let definition = analysis.anchor_def(id);
    let text = &analysis.db.file(model.file_of(id)).text;
    let start = usize::from(definition.name_range.start());
    let line_start = text[..start].rfind('\n').map_or(0, |at| at + 1);
    let line_end = text[start..].find('\n').map_or(text.len(), |at| start + at);
    let mut out = format!("```piton\n{}\n```\n", text[line_start..line_end].trim());
    if let Some(doc) = &definition.doc {
        out.push_str(&format!("\n{doc}\n"));
    }

    const SHOWN: usize = 40;
    let names = model.ordered_properties(id);
    if !names.is_empty() {
        out.push_str("\n**Properties**\n\n");
        for name in names.iter().take(SHOWN) {
            let Some(slot) = model.slot(id, name) else { continue };
            let constraint = render(slot.constraints.map(|(_, it)| it));
            let from = match slot.owner == id {
                true => String::new(),
                false => format!(" — from `{}`", analysis.anchor_def(slot.owner).name),
            };
            out.push_str(&format!("- `{name}{constraint}`{from}\n"));
        }
        if names.len() > SHOWN {
            out.push_str(&format!("- and {} more\n", names.len() - SHOWN));
        }
    }

    if definition.is_abstract {
        let implementors = analysis.implementors(id);
        if !implementors.is_empty() {
            let named: Vec<String> = implementors
                .iter()
                .take(10)
                .map(|it| format!("`{}`", analysis.anchor_def(*it).name))
                .collect();
            let more = match implementors.len() > 10 {
                true => format!(" and {} more", implementors.len() - 10),
                false => String::new(),
            };
            out.push_str(&format!("\nImplemented by {}{more}.\n", named.join(", ")));
        }
    }
    out
}

/// A property on the anchor that holds it, or on its bases through `super`.
fn property_hover(view: &View, model: &Model, holder: AnchorId, name: &str, through_super: bool) -> Option<String> {
    let analysis = model.analysis();
    let slot = match through_super {
        true => model.base_slot(holder, name)?,
        false => model.slot(holder, name)?,
    };
    let mut out = format!("```piton\n{name}{}\n```\n", render(slot.constraints.map(|(_, it)| it)));
    // The compiler only compiles concrete anchors, and a value read through
    // `super` is not the one the anchor itself ends up with.
    if !through_super && !analysis.anchor_def(holder).is_abstract {
        if let Some(value) = view.compilation.anchor(holder).and_then(|anchor| anchor.props.get(name)) {
            out.push_str(&value_preview(value));
        }
    }
    if let Some(doc) = &slot.property.doc {
        out.push_str(&format!("\n{doc}\n"));
    }
    out.push_str(&format!("\nDeclared on `{}`.", analysis.anchor_def(slot.owner).name));
    if let Some(base) = model.base_slot(slot.owner, name) {
        out.push_str(&format!(" Overrides `{}`.", analysis.anchor_def(base.owner).name));
    }
    out.push('\n');
    Some(out)
}

fn key_hover(view: &View, model: &Model, sym: &Sym) -> String {
    let mut out = format!("```piton\n{}\n```\n", key_path(model, sym));
    if let Some(value) = key_value(view, sym) {
        out.push_str(&value_preview(&value));
    }
    out
}

fn key_path(model: &Model, sym: &Sym) -> String {
    match sym {
        Sym::Key { owner, name } => format!("{}.{name}", key_path(model, owner)),
        other => model.name_of(other).unwrap_or_default(),
    }
}

/// A key's compiled value, when its owner is a variable. A key under a
/// property has a value per anchor that reads it, so none is shown for one.
fn key_value(view: &View, sym: &Sym) -> Option<Value> {
    match sym {
        Sym::Key { owner, name } => key_value(view, owner)?.field(name).cloned(),
        Sym::Var { file, index } => view.compilation.vars.get(&(*file, *index)).cloned(),
        _ => None,
    }
}

fn self_hover(model: &Model, reference: &SelfReference) -> String {
    let analysis = model.analysis();
    let name = &analysis.anchor_def(reference.anchor).name;
    match reference.kind {
        SelfKind::SelfRef => format!(
            "```piton\nself\n```\n\nThe most-derived anchor being compiled. Written in `{name}`, it \
             is `{name}` or whichever anchor extends it.\n"
        ),
        SelfKind::This => format!(
            "```piton\nthis\n```\n\n`{name}`, exactly, even when read by an anchor that extends it.\n"
        ),
        SelfKind::Super => {
            let bases: Vec<String> = model
                .bases(reference.anchor)
                .iter()
                .map(|base| format!("`{}`", analysis.anchor_def(*base).name))
                .collect();
            match bases.is_empty() {
                true => format!("```piton\nsuper\n```\n\n`{name}` has no bases, so `super` has nothing to read.\n"),
                false => format!(
                    "```piton\nsuper\n```\n\nThe bases of `{name}`: {}, with the right-most one winning.\n",
                    bases.join(", ")
                ),
            }
        }
    }
}

fn var_hover(view: &View, file: FileId, index: usize) -> String {
    let variable = &view.compilation.analysis.db.file(file).hir.vars[index];
    let mut out = format!("```piton\n{}{}\n```\n", variable.name, render(Some(&variable.constraints)));
    if let Some(value) = view.compilation.vars.get(&(file, index)) {
        out.push_str(&value_preview(value));
    }
    if let Some(doc) = &variable.doc {
        out.push_str(&format!("\n{doc}\n"));
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
            piton_core::resolve::Symbol::Anchor(id) => match &analysis.anchor_def(*id).keyword {
                Some(keyword) => format!("anchor, keyword `{}`", keyword.value),
                None => "anchor".to_string(),
            },
            piton_core::resolve::Symbol::Var { .. } => "variable".to_string(),
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

fn render(constraints: Option<&[TypeExpr]>) -> String {
    constraints.unwrap_or_default().iter().map(|it| format!(":: {}", it.render())).collect()
}

/// A short rendering of a compiled value, cut off when it is large.
pub fn value_preview(value: &Value) -> String {
    let rendered = to_json_string(value, true);
    let text = match rendered.lines().count() > 20 {
        true => format!("{}\n…", rendered.lines().take(20).collect::<Vec<_>>().join("\n")),
        false => rendered,
    };
    format!("\n```json\n{text}\n```\n")
}
