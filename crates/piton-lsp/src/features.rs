//! Editor feature implementations.
//!
//! These all work from the same two inputs: the resolved compilation and the
//! occurrence index. Answering from the resolved program is what lets an editor
//! show where an inherited value actually came from.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use piton_compile::{Compilation, Project, Symbol};
use piton_core::{AnchorId, Diagnostic, Severity, Span};
use piton_emit::markdown;
use piton_syntax::ast::{self, Expr, ExprKind, Item};
use piton_syntax::{format, SyntaxKind};
use tower_lsp::lsp_types::*;

use crate::world::OutputSettings;
use piton_emit::Adapter;
use crate::convert::{
    offset_to_position, path_to_url, position_to_offset, span_to_range, url_to_path,
};
use crate::index::{Role, Target};
use crate::world::World;
use crate::{TOKEN_MODIFIERS, TOKEN_TYPES};

/// Converts a compiler diagnostic into the editor's form.
///
/// `text_of` supplies the text of other files, so a label pointing into one
/// (the first import, the base that constrained a property) lands on the right
/// line there rather than at the top of the file.
pub fn to_lsp_diagnostic(
    diagnostic: &Diagnostic,
    text: &str,
    text_of: &dyn Fn(&Path) -> Option<String>,
) -> tower_lsp::lsp_types::Diagnostic {
    let mut message = diagnostic.message.clone();
    if let Some(help) = &diagnostic.help {
        message.push_str("\n\nhelp: ");
        message.push_str(help);
    }
    if let Some(origin) = &diagnostic.origin {
        message.push_str("\n\norigin: ");
        message.push_str(origin);
    }
    tower_lsp::lsp_types::Diagnostic {
        range: span_to_range(text, diagnostic.span),
        severity: Some(match diagnostic.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
            Severity::Info => DiagnosticSeverity::INFORMATION,
        }),
        code: Some(NumberOrString::String(diagnostic.code.clone())),
        source: Some("piton".into()),
        message,
        tags: crate::analysis::is_unnecessary(&diagnostic.code)
            .then(|| vec![DiagnosticTag::UNNECESSARY]),
        related_information: (!diagnostic.labels.is_empty()).then(|| {
            diagnostic
                .labels
                .iter()
                .filter_map(|label| {
                    let range = if label.file == diagnostic.file {
                        span_to_range(text, label.span)
                    } else {
                        text_of(&label.file)
                            .map(|other| span_to_range(&other, label.span))
                            .unwrap_or_default()
                    };
                    Some(DiagnosticRelatedInformation {
                        location: Location {
                            uri: path_to_url(&label.file)?,
                            range,
                        },
                        message: label.message.clone(),
                    })
                })
                .collect()
        }),
        ..Default::default()
    }
}

/// Everything a feature needs about the file under the cursor.
struct Cursor<'a> {
    compilation: &'a Compilation,
    module: piton_compile::ModuleId,
    path: PathBuf,
    text: String,
    offset: usize,
    _marker: std::marker::PhantomData<&'a ()>,
}

fn cursor<'a>(world: &'a World, uri: &Url, position: Position) -> Option<Cursor<'a>> {
    let path = url_to_path(uri)?;
    let compilation = world.compilation.as_ref()?;
    let module = compilation.graph().id_for(&path)?;
    let text = world.text(&path)?;
    let offset = position_to_offset(&text, position);
    Some(Cursor {
        compilation,
        module,
        path,
        text,
        offset,
        _marker: std::marker::PhantomData,
    })
}

fn file_context<'a>(world: &'a World, uri: &Url) -> Option<Cursor<'a>> {
    cursor(world, uri, Position::default())
}

// ---------------------------------------------------------------------------
// Hover
// ---------------------------------------------------------------------------

pub fn hover(world: &World, uri: &Url, position: Position) -> Option<Hover> {
    let cursor = cursor(world, uri, position)?;
    let index = world.index.get(cursor.module)?;
    let occurrence = index.at(cursor.offset);
    let mut parts = Vec::new();
    if let Some(occurrence) = occurrence {
        if let Some(markdown) = describe(cursor.compilation, &world.output, &occurrence.target) {
            parts.push(markdown);
        }
    }
    // Composition and the compiled slice are answers about the position, not
    // about a named occurrence, so they are appended even when the cursor is
    // sitting on `+` rather than on a symbol.
    let compositions = world.analysis.compositions_at(cursor.module, cursor.offset);
    if !compositions.is_empty() {
        let mut seen = HashSet::new();
        for composition in &compositions {
            if seen.insert(composition.span) {
                parts.push(composition.summary.clone());
            }
        }
    }
    if let Some(note) = mapping_note(&world.analysis, &cursor.path, cursor.offset) {
        parts.push(note);
    }
    if parts.is_empty() {
        return None;
    }
    let range = occurrence
        .map(|item| item.span)
        .or_else(|| compositions.first().map(|composition| composition.span));
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: parts.join("\n\n"),
        }),
        range: range.map(|span| span_to_range(&cursor.text, span)),
    })
}

fn mapping_note(
    analysis: &crate::analysis::Analysis,
    file: &Path,
    offset: usize,
) -> Option<String> {
    let mappings = analysis.mappings_at(file, offset);
    if mappings.is_empty() {
        return None;
    }
    let mut lines = vec!["Compiled to:".to_string()];
    let mut seen = HashSet::new();
    for mapping in mappings {
        if seen.insert(mapping.label.clone()) {
            lines.push(format!("- {}", mapping.label));
        }
    }
    Some(lines.join("\n"))
}

/// A value as the project's configured renderer writes it, fenced for a hover.
fn rendered(compilation: &Compilation, output: &OutputSettings, value: &piton_core::Value) -> String {
    let (language, text) = match output.renderer {
        Adapter::Json => ("json", piton_emit::json::value(value, compilation)),
        Adapter::Yaml => ("yaml", piton_emit::yaml::value(value, compilation)),
        Adapter::Markdown => {
            let context = markdown::Context {
                anchors: compilation,
                links: &markdown::NoLinks,
            };
            ("markdown", markdown::body(value, 1, &context))
        }
    };
    format!(
        "Resolves to ({}):\n\n```{language}\n{}\n```",
        output.renderer,
        truncate(&text, 2000)
    )
}

/// The first sentence or line of an anchor's `description`, for places that
/// have room for a summary: symbol search, hierarchy items, outlines.
pub fn summary(compilation: &Compilation, anchor: AnchorId) -> Option<String> {
    let description = compilation
        .store()
        .anchor(anchor)
        .properties
        .get("description")?;
    let piton_core::Value::Str(text) = description else {
        return None;
    };
    let plain = text.render_plain(compilation);
    let first = plain.lines().find(|line| !line.trim().is_empty())?.trim();
    let sentence = match first.find(". ") {
        Some(index) => &first[..=index],
        None => first,
    };
    let clipped: String = sentence.chars().take(80).collect();
    Some(if sentence.chars().count() > 80 {
        format!("{clipped}\u{2026}")
    } else {
        clipped
    })
}

/// Builds the documentation shown for a target: what it is, where it came
/// from, its inheritance chain, and the value it resolves to.
fn describe(compilation: &Compilation, output: &OutputSettings, target: &Target) -> Option<String> {
    match target {
        Target::Anchor(anchor) => {
            let def = compilation.store().anchor(*anchor);
            let mut out = String::new();
            out.push_str("```piton\n");
            if def.exported {
                out.push_str("export ");
            }
            if def.is_abstract {
                out.push_str("abstract ");
            }
            out.push_str(&def.keyword);
            out.push(' ');
            out.push_str(&def.name);
            if let Some(alias) = &def.alias {
                out.push_str(&format!(" as {alias}"));
            }
            out.push_str("\n```\n\n");
            if let Some(summary) = summary(compilation, *anchor) {
                out.push_str(&format!("{summary}\n\n"));
            }

            let chain: Vec<String> = compilation
                .store()
                .base_chain(*anchor)
                .into_iter()
                .filter(|id| *id != *anchor)
                .map(|id| compilation.store().anchor(id).name.clone())
                .collect();
            if !chain.is_empty() {
                out.push_str(&format!("Inherits: {}\n\n", chain.join(" \u{2192} ")));
            }
            out.push_str(&format!(
                "Declared in `{}`\n\n",
                compilation.anchor_module_path(*anchor).display()
            ));
            if !def.exported {
                out.push_str("_Not exported; only this file can use it._\n\n");
            }

            if !def.properties.is_empty() && !def.is_abstract {
                // The compiled interpretation is often the real question.
                out.push_str(&rendered(
                    compilation,
                    output,
                    &piton_core::Value::Anchor(*anchor),
                ));
            }
            Some(out)
        }
        Target::Variable(variable) => {
            let def = compilation.store().variable(*variable);
            let mut out = format!(
                "```piton\n{}{}\n```\n\n",
                if def.exported { "export " } else { "" },
                def.name
            );
            if !def.constraints.is_empty() {
                let names: Vec<String> = def
                    .constraints
                    .iter()
                    .map(piton_compile::eval::constraint_label)
                    .collect();
                out.push_str(&format!("Constrained to: {}\n\n", names.join(" or ")));
            }
            if let Some(value) = &def.value {
                out.push_str(&rendered(compilation, output, value));
            }
            Some(out)
        }
        Target::Property(anchor, name) => {
            let def = compilation.store().anchor(*anchor);
            let slot = def.slots.get(name)?;
            let owner = compilation.store().anchor(slot.owner);
            let mut out = format!("`{}` on `{}`\n\n", name, def.name);
            if slot.owner != *anchor {
                // Knowing which base contributed a value is the whole point of
                // structural inheritance being visible.
                out.push_str(&format!("Inherited from `{}`\n\n", owner.name));
            } else if let Some(replaced) = overridden(compilation, *anchor, name) {
                out.push_str(&format!(
                    "Overrides `{}.{name}`\n\n",
                    compilation.store().anchor(replaced).name
                ));
            }
            if !slot.constraints.is_empty() {
                let names: Vec<String> = slot
                    .constraints
                    .iter()
                    .map(piton_compile::eval::constraint_label)
                    .collect();
                out.push_str(&format!("Constrained to: {}\n\n", names.join(" or ")));
            }
            if !slot.has_value {
                out.push_str("_Abstract: an implementing anchor must define it._\n\n");
            }
            if let Some(value) = def.properties.get(name) {
                out.push_str(&rendered(compilation, output, value));
            }
            Some(out)
        }
        Target::Key(anchor, path) => {
            let def = compilation.store().anchor(*anchor);
            let (last, parents) = path.split_last()?;
            let mut out = format!(
                "`{last}` in `{}.{}`\n\n_A key of that dictionary, not a property of `{}`._\n\n",
                def.name,
                parents.join("."),
                def.name
            );
            if let Some(value) = key_value(compilation, *anchor, path) {
                out.push_str(&rendered(compilation, output, value));
            }
            Some(out)
        }
        Target::Keyword(keyword, anchor) => {
            let mut out = format!("Keyword `{keyword}`\n\n");
            match anchor {
                Some(anchor) => {
                    let def = compilation.store().anchor(*anchor);
                    out.push_str(&format!(
                        "Shorthand for `extends {}`, declared in `{}`.\n\nA user keyword's anchor is always the last base, so it wins any collision with what is in `extends`.",
                        def.name,
                        compilation.anchor_module_path(*anchor).display()
                    ));
                }
                None => out.push_str(
                    "Not in scope. Add a `use` declaration for the module that exports it.",
                ),
            }
            Some(out)
        }
        Target::Module(path) => Some(format!("Module `{}`", path.display())),
        Target::Unresolved(name) => Some(format!("`{name}` is not in scope.")),
    }
}

/// The resolved value at a nested key.
fn key_value<'a>(
    compilation: &'a Compilation,
    anchor: AnchorId,
    path: &[String],
) -> Option<&'a piton_core::Value> {
    let (first, rest) = path.split_first()?;
    let mut value = compilation.store().anchor(anchor).properties.get(first)?;
    for segment in rest {
        value = value.property(segment, compilation)?;
    }
    Some(value)
}

/// The base whose declaration of `name` an anchor's own declaration replaces:
/// the right-most base that has it.
pub fn overridden(compilation: &Compilation, anchor: AnchorId, name: &str) -> Option<AnchorId> {
    let store = compilation.store();
    let def = store.anchor(anchor);
    if def.slots.get(name).is_none_or(|slot| slot.owner != anchor) {
        return None;
    }
    def.bases
        .iter()
        .rev()
        .find(|base| store.anchor(**base).slots.contains_key(name))
        .copied()
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.trim_end().to_string();
    }
    let truncated: String = text.chars().take(limit).collect();
    format!("{truncated}\n\u{2026}")
}

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------

/// The location a target is declared at.
fn definition_location(world: &World, target: &Target) -> Option<Location> {
    let compilation = world.compilation.as_ref()?;
    let (path, span) = match target {
        Target::Anchor(anchor) => {
            let def = compilation.store().anchor(*anchor);
            (compilation.anchor_module_path(*anchor), def.name_span)
        }
        Target::Variable(variable) => {
            let def = compilation.store().variable(*variable);
            (
                compilation.graph().get(def.module).path.clone(),
                def.name_span,
            )
        }
        Target::Property(anchor, name) => {
            // Land on the declaration that actually supplies the value.
            let slot = compilation.store().anchor(*anchor).slots.get(name)?;
            let owner = compilation.store().anchor(slot.owner);
            (
                compilation.graph().get(owner.module).path.clone(),
                slot.span,
            )
        }
        Target::Key(anchor, path) => key_declaration(compilation, *anchor, path)?,
        Target::Keyword(_, anchor) => {
            let anchor = (*anchor)?;
            let def = compilation.store().anchor(anchor);
            (compilation.anchor_module_path(anchor), def.name_span)
        }
        Target::Module(_) | Target::Unresolved(_) => return None,
    };
    let text = world.text(&path)?;
    Some(Location {
        uri: path_to_url(&path)?,
        range: span_to_range(&text, span),
    })
}

/// Where a nested key was written: in the anchor that supplies the property
/// it sits in, walking down the dictionaries by name.
fn key_declaration(
    compilation: &Compilation,
    anchor: AnchorId,
    path: &[String],
) -> Option<(PathBuf, Span)> {
    let (first, rest) = path.split_first()?;
    let slot = compilation.store().anchor(anchor).slots.get(first)?;
    let owner = compilation.store().anchor(slot.owner);
    let module = compilation.graph().get(owner.module);
    let Item::Anchor(decl) = &module.ast().items[owner.item] else {
        return None;
    };
    let mut property = decl.body.properties().find(|property| property.name == *first)?;
    for segment in rest {
        property = property
            .value
            .block
            .as_ref()?
            .properties()
            .find(|nested| nested.name == *segment)?;
    }
    Some((module.path.clone(), property.name_span))
}

/// The location of the declaration an anchor's own property replaces.
fn overridden_location(world: &World, anchor: AnchorId, name: &str) -> Option<Location> {
    let compilation = world.compilation.as_ref()?;
    let base = overridden(compilation, anchor, name)?;
    definition_location(world, &Target::Property(base, name.to_string()))
}

pub fn definition(world: &World, uri: &Url, position: Position) -> Option<GotoDefinitionResponse> {
    let cursor = cursor(world, uri, position)?;
    let index = world.index.get(cursor.module)?;
    let occurrence = index.at(cursor.offset)?;

    // On the declaration of a property that overrides an inherited one, the
    // interesting place to go is the declaration it replaces.
    if let (Target::Property(anchor, name), Role::Definition) = (&occurrence.target, occurrence.role)
    {
        if let Some(location) = overridden_location(world, *anchor, name) {
            return Some(GotoDefinitionResponse::Scalar(location));
        }
    }

    // A module path navigates to the module's own file.
    if let Target::Module(written) = &occurrence.target {
        let resolved = resolve_module_path(world, &cursor.path, written)?;
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri: path_to_url(&resolved)?,
            range: Range::default(),
        }));
    }

    definition_location(world, &occurrence.target).map(GotoDefinitionResponse::Scalar)
}

fn resolve_module_path(world: &World, from: &Path, written: &Path) -> Option<PathBuf> {
    module_path_from(&world.project, from, &written.to_string_lossy())
}

/// Resolves the text of a module path against the file that wrote it.
fn module_path_from(project: &Project, from: &Path, written: &str) -> Option<PathBuf> {
    let context = piton_compile::module::ResolutionContext::new(from.parent()?, project.roots());
    piton_compile::module::resolve(written, &context).ok()
}

pub fn references(
    world: &World,
    uri: &Url,
    position: Position,
    include_declaration: bool,
) -> Option<Vec<Location>> {
    let cursor = cursor(world, uri, position)?;
    let index = world.index.get(cursor.module)?;
    let target = index.at(cursor.offset)?.target.clone();
    let compilation = cursor.compilation;

    let related = related_to(compilation, &target);
    let mut out = Vec::new();
    let mut texts: HashMap<PathBuf, String> = HashMap::new();
    for (module, occurrence) in world.index.matching(|candidate| related(candidate)) {
        if occurrence.role == Role::Definition && !include_declaration {
            continue;
        }
        let path = compilation.graph().get(module).path.clone();
        if path.to_string_lossy().starts_with('@') {
            continue;
        }
        let text = texts
            .entry(path.clone())
            .or_insert_with(|| world.text(&path).unwrap_or_default());
        if let Some(uri) = path_to_url(&path) {
            out.push(Location {
                uri,
                range: span_to_range(text, occurrence.span),
            });
        }
    }
    out.sort_by(|a, b| {
        a.uri.as_str().cmp(b.uri.as_str()).then(
            (a.range.start.line, a.range.start.character)
                .cmp(&(b.range.start.line, b.range.start.character)),
        )
    });
    Some(out)
}

/// Builds a predicate for everything that counts as a reference to `target`.
///
/// A user keyword is sugar for `extends`, so writing `type Strings:` references
/// the `Type` anchor just as surely as `extends Type` would. A property that
/// overrides an inherited one is a reference to the same idea, so the family it
/// belongs to is included too.
fn related_to<'a>(
    compilation: &'a Compilation,
    target: &'a Target,
) -> impl Fn(&Target) -> bool + 'a {
    move |candidate: &Target| {
        if candidate == target {
            return true;
        }
        match (target, candidate) {
            (Target::Anchor(anchor), Target::Keyword(_, Some(aliased))) => aliased == anchor,
            (Target::Keyword(_, Some(aliased)), Target::Anchor(anchor)) => aliased == anchor,
            (Target::Keyword(keyword, _), Target::Keyword(other, _)) => keyword == other,
            (Target::Property(anchor, name), Target::Property(other_anchor, other_name)) => {
                name == other_name
                    && (compilation.store().inherits_from(*anchor, *other_anchor)
                        || compilation.store().inherits_from(*other_anchor, *anchor))
            }
            // A nested key is only ever the same key at the same path, never a
            // property that happens to share its name.
            (Target::Key(anchor, path), Target::Key(other_anchor, other_path)) => {
                path == other_path
                    && (compilation.store().inherits_from(*anchor, *other_anchor)
                        || compilation.store().inherits_from(*other_anchor, *anchor))
            }
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Rename
// ---------------------------------------------------------------------------

pub fn prepare_rename(
    world: &World,
    uri: &Url,
    position: Position,
) -> Option<PrepareRenameResponse> {
    let cursor = cursor(world, uri, position)?;
    let index = world.index.get(cursor.module)?;
    let occurrence = index.at(cursor.offset)?;
    if matches!(occurrence.target, Target::Module(_)) {
        return None;
    }
    Some(PrepareRenameResponse::RangeWithPlaceholder {
        range: span_to_range(&cursor.text, occurrence.span),
        placeholder: occurrence.text.clone(),
    })
}

pub fn rename(
    world: &World,
    uri: &Url,
    position: Position,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    let cursor = cursor(world, uri, position)?;
    let index = world.index.get(cursor.module)?;
    let target = index.at(cursor.offset)?.target.clone();
    if matches!(target, Target::Module(_)) {
        return None;
    }
    let compilation = cursor.compilation;

    let index = world.index.get(cursor.module)?;
    let written = index.at(cursor.offset)?.text.clone();
    let related = related_to(compilation, &target);

    let mut edits: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    let mut texts: HashMap<PathBuf, String> = HashMap::new();
    for (module, occurrence) in world.index.matching(|candidate| related(candidate)) {
        // Only rewrite occurrences spelled the same way. A keyword alias and an
        // aliased import bind other names for the same thing, and renaming one
        // must not silently rewrite the other.
        if occurrence.text != written {
            continue;
        }
        let path = compilation.graph().get(module).path.clone();
        if path.to_string_lossy().starts_with('@') {
            // A bundled package is not the user's to rename.
            return None;
        }
        let text = texts
            .entry(path.clone())
            .or_insert_with(|| world.text(&path).unwrap_or_default());
        let Some(url) = path_to_url(&path) else {
            continue;
        };
        // An aliased import binds a different local name; renaming the local
        // name must not rewrite the exported one.
        edits.entry(url).or_default().push(TextEdit {
            range: span_to_range(text, occurrence.span),
            new_text: new_name.to_string(),
        });
    }

    for list in edits.values_mut() {
        list.sort_by_key(|edit| (edit.range.start.line, edit.range.start.character));
        list.dedup_by_key(|edit| (edit.range.start.line, edit.range.start.character));
    }

    Some(WorkspaceEdit {
        changes: Some(edits),
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// Symbols
// ---------------------------------------------------------------------------

pub fn document_symbols(world: &World, uri: &Url) -> Option<DocumentSymbolResponse> {
    let cursor = file_context(world, uri)?;
    let module = cursor.compilation.graph().get(cursor.module);
    let text = &cursor.text;

    let mut symbols = Vec::new();
    for (item_index, item) in module.ast().items.iter().enumerate() {
        match item {
            Item::Anchor(decl) => {
                let described = cursor
                    .compilation
                    .store()
                    .anchors
                    .iter()
                    .find(|def| def.module == cursor.module && def.item == item_index)
                    .and_then(|def| summary(cursor.compilation, def.id));
                let children: Vec<DocumentSymbol> = decl
                    .body
                    .properties()
                    .map(|property| {
                        symbol(
                            &property.name,
                            SymbolKind::PROPERTY,
                            property.span,
                            property.name_span,
                            text,
                            None,
                            Vec::new(),
                        )
                    })
                    .collect();
                let mut detail = if decl.is_abstract {
                    format!("abstract {}", decl.keyword)
                } else {
                    decl.keyword.clone()
                };
                if let Some(described) = described {
                    detail.push_str(" \u{2014} ");
                    detail.push_str(&described);
                }
                let detail = Some(detail);
                symbols.push(symbol(
                    &decl.name,
                    if decl.is_abstract {
                        SymbolKind::INTERFACE
                    } else {
                        SymbolKind::CLASS
                    },
                    decl.span,
                    decl.name_span,
                    text,
                    detail,
                    children,
                ));
            }
            Item::Variable(decl) => symbols.push(symbol(
                &decl.name,
                SymbolKind::VARIABLE,
                decl.span,
                decl.name_span,
                text,
                None,
                Vec::new(),
            )),
            _ => {}
        }
    }
    Some(DocumentSymbolResponse::Nested(symbols))
}

#[allow(deprecated)]
fn symbol(
    name: &str,
    kind: SymbolKind,
    span: Span,
    selection: Span,
    text: &str,
    detail: Option<String>,
    children: Vec<DocumentSymbol>,
) -> DocumentSymbol {
    DocumentSymbol {
        name: name.to_string(),
        detail,
        kind,
        tags: None,
        deprecated: None,
        range: span_to_range(text, span),
        selection_range: span_to_range(text, selection),
        children: (!children.is_empty()).then_some(children),
    }
}

#[allow(deprecated)]
pub fn workspace_symbols(world: &World, query: &str) -> Option<Vec<SymbolInformation>> {
    let compilation = world.compilation.as_ref()?;
    let query = query.to_lowercase();
    let mut out = Vec::new();
    let mut texts: HashMap<PathBuf, String> = HashMap::new();

    for def in &compilation.store().anchors {
        if !query.is_empty() && !def.name.to_lowercase().contains(&query) {
            continue;
        }
        let path = compilation.anchor_module_path(def.id);
        if path.to_string_lossy().starts_with('@') {
            continue;
        }
        let text = texts
            .entry(path.clone())
            .or_insert_with(|| world.text(&path).unwrap_or_default());
        let Some(uri) = path_to_url(&path) else {
            continue;
        };
        out.push(SymbolInformation {
            name: def.name.clone(),
            kind: if def.is_abstract {
                SymbolKind::INTERFACE
            } else {
                SymbolKind::CLASS
            },
            tags: None,
            deprecated: None,
            location: Location {
                uri,
                range: span_to_range(text, def.name_span),
            },
            // The protocol has no detail field for a workspace symbol; the
            // container name is what editors show beside it, so the summary
            // of its description rides along there.
            container_name: Some(match summary(compilation, def.id) {
                Some(described) => format!("{} \u{2014} {described}", def.keyword),
                None => def.keyword.clone(),
            }),
        });
    }

    for def in &compilation.store().variables {
        if !query.is_empty() && !def.name.to_lowercase().contains(&query) {
            continue;
        }
        let path = compilation.graph().get(def.module).path.clone();
        if path.to_string_lossy().starts_with('@') {
            continue;
        }
        let text = texts
            .entry(path.clone())
            .or_insert_with(|| world.text(&path).unwrap_or_default());
        let Some(uri) = path_to_url(&path) else {
            continue;
        };
        out.push(SymbolInformation {
            name: def.name.clone(),
            kind: SymbolKind::VARIABLE,
            tags: None,
            deprecated: None,
            location: Location {
                uri,
                range: span_to_range(text, def.name_span),
            },
            container_name: None,
        });
    }

    out.sort_by(|a, b| a.name.cmp(&b.name));
    Some(out)
}

// ---------------------------------------------------------------------------
// Completion
// ---------------------------------------------------------------------------

/// What the cursor is in the middle of writing.
///
/// Everything Piton offers is contextual. The words that belong at the head of
/// an unindented line are not the ones that belong after `extends`, and neither
/// set belongs inside `@{...}`, where the only thing that can appear is
/// something a reference can resolve to. Classifying the line first and
/// building the list second keeps that from collapsing into one chain of
/// special cases, and keeps suggestions out of the places where the text is not
/// Piton at all.
#[derive(Debug, PartialEq, Eq)]
enum Context<'a> {
    /// Nothing sensible can be offered: a comment, a verbatim block, or the
    /// middle of a name the editor is in no position to invent.
    Nothing,
    /// After `use` or `from`, where a module path goes.
    ModulePath,
    /// After `from <path>`, where `import` or `export` goes.
    ImportDirection,
    /// After `from <path> import`, where the imported names go.
    ImportName { module: &'a str },
    /// The head of an unindented line, with the modifiers already written.
    Declaration { exported: bool, is_abstract: bool },
    /// After `extends` in a declaration head.
    Bases,
    /// Inside a `::` type constraint.
    TypeConstraint,
    /// Inside an interpolation, with the sigil that opened it and the path
    /// before a trailing `.`, if there is one.
    Expression {
        sigil: &'a str,
        member_of: Option<&'a str>,
    },
    /// Where a value goes. The key is the one whose constraints say what is
    /// valid there; a list item has none to name.
    Value { key: Option<&'a str> },
    /// The head of an indented line, where a key goes.
    Body,
}

pub fn completion(world: &World, uri: &Url, position: Position) -> Option<CompletionResponse> {
    let cursor = cursor(world, uri, position)?;
    let line_start = cursor.text[..cursor.offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let prefix = &cursor.text[line_start..cursor.offset];

    let context = if verbatim_at(&cursor.text, line_start) {
        Context::Nothing
    } else {
        context(prefix)
    };

    // Selecting a candidate takes the place of what the cursor is in the middle
    // of writing rather than being appended to it: `anc` becomes `anchor`, not
    // `ancanchor`. Every item the context offers replaces the same span, so it
    // is computed once here.
    let replace = Range {
        start: offset_to_position(
            &cursor.text,
            replace_start(&context, &cursor.text, cursor.offset),
        ),
        end: offset_to_position(&cursor.text, cursor.offset),
    };

    let items = match context {
        Context::Nothing => Vec::new(),
        Context::ModulePath => module_paths(&cursor),
        Context::ImportDirection => keywords(&["import", "export"]),
        Context::ImportName { module } => import_names(&cursor, module),
        Context::Declaration {
            exported,
            is_abstract,
        } => declaration_heads(&cursor, exported, is_abstract),
        Context::Bases => {
            let mut items = anchors_in_scope(&cursor, None, None);
            items.extend(importable(&cursor, Only::Anchors));
            items
        }
        Context::TypeConstraint => type_names(&cursor),
        Context::Expression { sigil, member_of } => expression(&cursor, sigil, member_of),
        // A list item holds the value of the key above it, so that key's
        // constraints are what say which values belong in it.
        Context::Value { key } => {
            // The key has to be one of the anchor's own properties for its
            // constraints to say anything; a key nested in a dictionary only
            // shares a name with them.
            let key = match key {
                Some(key) => (!in_nested_block(&cursor.text, cursor.offset)).then_some(key),
                None => enclosing_key(&cursor.text, line_start).and_then(|(key, at)| {
                    (!in_nested_block(&cursor.text, at)).then_some(key)
                }),
            };
            values(&cursor, key)
        }
        Context::Body => body(&cursor),
    };

    let items: Vec<CompletionItem> = items
        .into_iter()
        .map(|mut item| {
            // The edit is what says what gets written. `insert_text` says the
            // same thing where the two differ -- a body's key takes its colon,
            // a wrapped reference takes its braces -- and an edit is the only
            // form that also says what it stands in for.
            let written = item
                .insert_text
                .clone()
                .unwrap_or_else(|| item.label.clone());
            item.text_edit = Some(CompletionTextEdit::Edit(TextEdit {
                range: replace,
                new_text: written,
            }));
            item
        })
        .collect();

    Some(CompletionResponse::Array(items))
}

/// Where the text being completed begins.
///
/// The edit has to cover the word under the cursor -- that is what makes a
/// candidate replace it -- and what counts as that word depends on where the
/// cursor is. A module path is written whole (`@piton/belay`, `./lib/Type`),
/// a member is only ever the segment after the last dot, everything inside
/// `{...}` follows a brace and a sigil that are not part of the name, and a
/// `-` written against a list value is the item's marker rather than the
/// word's.
fn replace_start(context: &Context<'_>, text: &str, cursor: usize) -> usize {
    let in_word = |character: char| {
        let name = character.is_alphanumeric() || character == '_' || character == '-';
        match context {
            // The brace and sigil were already written, and a dot opens the
            // member that follows, so a name stops at both.
            Context::Expression { .. } => name,
            // A path carries characters no name would: `@`, `/`, and `.`.
            Context::ModulePath => !character.is_whitespace(),
            // `*` is a name of its own in an import list.
            Context::ImportName { .. } => name || character == '*',
            // `@` and `#` are half-written sigils: completing an anchor into a
            // reference value turns the `@` into `@{...}` rather than stacking
            // another one in front of it.
            _ => name || character == '@' || character == '#',
        }
    };

    let mut start = cursor;
    for (index, character) in text[..cursor].char_indices().rev() {
        if !in_word(character) {
            break;
        }
        start = index;
    }

    // `-` never begins a name, so a run that starts with one is the marker of
    // the list item it sits in front of (`- nu` stops at the space; `-nu` does
    // not). The marker is left in place either way -- the value goes after it.
    if text[start..cursor].starts_with('-') {
        start += '-'.len_utf8();
    }
    start
}

/// Fills in the documentation for the item the editor asked about.
///
/// Completion runs on every keystroke and a list can be hundreds of items long,
/// while the documentation for one anchor renders its whole compiled value.
/// Building that for every item every time would be the slowest thing the
/// server does, so each item carries only the identity of what it names, and
/// the description is built here, for the one item the cursor settled on.
pub fn resolve_completion(world: &World, mut item: CompletionItem) -> CompletionItem {
    let Some(compilation) = world.compilation.as_ref() else {
        return item;
    };
    let Some(target) = item.data.as_ref().and_then(target_from_data) else {
        return item;
    };
    if let Some(markdown) = describe(compilation, &world.output, &target) {
        item.documentation = Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: markdown,
        }));
    }
    item
}

/// The identity an item carries so `completionItem/resolve` can find it again.
fn data_for(target: &Target) -> Option<serde_json::Value> {
    match target {
        Target::Anchor(anchor) => Some(serde_json::json!({ "anchor": anchor.0 })),
        Target::Variable(variable) => Some(serde_json::json!({ "variable": variable.0 })),
        Target::Property(anchor, name) => {
            Some(serde_json::json!({ "anchor": anchor.0, "property": name }))
        }
        _ => None,
    }
}

fn target_from_data(data: &serde_json::Value) -> Option<Target> {
    let anchor = data.get("anchor").and_then(serde_json::Value::as_u64);
    if let Some(name) = data.get("property").and_then(serde_json::Value::as_str) {
        return Some(Target::Property(AnchorId(anchor? as u32), name.to_string()));
    }
    if let Some(anchor) = anchor {
        return Some(Target::Anchor(AnchorId(anchor as u32)));
    }
    let variable = data.get("variable").and_then(serde_json::Value::as_u64)?;
    Some(Target::Variable(piton_compile::VariableId(variable as u32)))
}

// --- Classifying the line ---------------------------------------------------

/// Whether the line beginning at `line_start` sits inside an escape block.
///
/// An escape block is verbatim: its contents are not Piton, so a `{` inside
/// one opens nothing and a `//` starts no comment. It is opened and closed by
/// a line of its own, so counting the delimiters above the cursor answers it.
/// A code fence is just text, so it isn't verbatim.
fn verbatim_at(text: &str, line_start: usize) -> bool {
    let mut escape: Option<usize> = None;
    for line in text[..line_start].lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && trimmed.chars().all(|c| c == '\\') {
            // An escape block closes on a run of the same length as the one
            // that opened it, which is what lets a block contain a shorter run.
            escape = match escape {
                Some(open) if trimmed.len() == open => None,
                other => other.or(Some(trimmed.len())),
            };
        }
    }
    escape.is_some()
}

/// Classifies the line up to the cursor.
fn context(prefix: &str) -> Context<'_> {
    if in_comment(prefix) {
        return Context::Nothing;
    }

    // An interpolation wins over whatever it is written inside: `key: {x` is an
    // expression, not a value.
    if let Some((sigil, open)) = open_interpolation(prefix) {
        let inside = &prefix[open..];
        // A string is text, and text has no members. `${"a sentence.` has
        // nothing to offer, and offering anything there would be inventing it.
        if in_string(inside) {
            return Context::Nothing;
        }
        return Context::Expression {
            sigil,
            member_of: member_path(inside),
        };
    }

    let indented = prefix.starts_with([' ', '\t']);
    let trimmed = prefix.trim_start();

    // Imports are only ever written unindented.
    if !indented {
        if word_after(trimmed, "use").is_some() {
            return Context::ModulePath;
        }
        if let Some(rest) = word_after(trimmed, "from") {
            let rest = rest.trim_start();
            let Some((module, after)) = rest.split_once(char::is_whitespace) else {
                // Still writing the path.
                return Context::ModulePath;
            };
            let after = after.trim_start();
            return match word_after(after, "import").or_else(|| word_after(after, "export")) {
                Some(_) => Context::ImportName { module },
                None => Context::ImportDirection,
            };
        }
    }

    // A list item and a merge item both hold a value, and neither names a key
    // whose constraints could narrow it.
    if let Some(written) = trimmed
        .strip_prefix("++")
        .or_else(|| trimmed.strip_prefix('+'))
        .or_else(|| trimmed.strip_prefix('-'))
    {
        return value_context(None, written);
    }

    // A key takes the first colon on the line, so what follows it is a chain of
    // `:: type :` constraints and then the value.
    if let Some((key, after)) = split_key(trimmed) {
        return match key_tail(after) {
            KeyTail::Constraint => Context::TypeConstraint,
            KeyTail::Value(written) => value_context(Some(key), written),
        };
    }

    if indented {
        // A key cannot contain a space, so a line that already has one is
        // prose, and prose is a value rather than something to complete.
        return if is_prose(trimmed) {
            Context::Nothing
        } else {
            Context::Body
        };
    }

    // `anchor Name extends B as k:` -- the bases come after `extends` and the
    // keyword the author is inventing comes after `as`, so whichever was
    // written last is the one the cursor is past.
    match (
        word_position(trimmed, "extends"),
        word_position(trimmed, "as"),
    ) {
        // A keyword the author is inventing is not the editor's to guess, and
        // neither is the name after the declaration keyword.
        (Some(extends), Some(alias)) if alias > extends => return Context::Nothing,
        (Some(_), _) => return Context::Bases,
        (None, Some(_)) => return Context::Nothing,
        (None, None) => {}
    }

    let mut written: Vec<&str> = trimmed.split_whitespace().collect();
    if !prefix.ends_with(char::is_whitespace) {
        // The last word is still being typed, so it is what to complete rather
        // than something already said.
        written.pop();
    }
    let mut exported = false;
    let mut is_abstract = false;
    for word in written {
        match word {
            "export" => exported = true,
            "abstract" => is_abstract = true,
            _ => return Context::Nothing,
        }
    }
    Context::Declaration {
        exported,
        is_abstract,
    }
}

/// Whether the cursor is past a `//` that begins a comment.
///
/// A comment only begins after whitespace, which is what keeps the `//` in a
/// URL written in prose from swallowing the rest of the line.
fn in_comment(prefix: &str) -> bool {
    let bytes = prefix.as_bytes();
    (0..bytes.len().saturating_sub(1)).any(|index| {
        bytes[index] == b'/'
            && bytes[index + 1] == b'/'
            && prefix[..index]
                .chars()
                .next_back()
                .is_none_or(char::is_whitespace)
    })
}

/// The innermost interpolation the cursor is inside: its sigil, and the offset
/// just past the opening brace.
fn open_interpolation(prefix: &str) -> Option<(&'static str, usize)> {
    let mut stack: Vec<(&'static str, usize)> = Vec::new();
    for (index, character) in prefix.char_indices() {
        match character {
            '}' => {
                stack.pop();
            }
            '{' => {
                let sigil = match prefix[..index].chars().next_back() {
                    Some('$') => "${",
                    Some('#') => "#{",
                    Some('@') => "@{",
                    _ => "{",
                };
                stack.push((sigil, index + 1));
            }
            _ => {}
        }
    }
    stack.pop()
}

/// The path written before a trailing `.`, if the cursor is completing a member.
///
/// `self.`, `Anchor.`, `Anchor.nested.par` -- everything up to the last dot,
/// with whatever has been typed after it left off.
fn member_path(inside: &str) -> Option<&str> {
    let start = inside
        // A hyphen inside a name is part of it: `config.foo-bar.` reads the
        // key `foo-bar`.
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.' || c == '-'))
        .map(|index| index + 1)
        .unwrap_or(0);
    let token = &inside[start..];
    let dot = token.rfind('.')?;
    Some(&token[..dot])
}

/// What follows `word`, if `text` begins with that whole word.
fn word_after<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(word)?;
    rest.chars()
        .next()
        .is_none_or(char::is_whitespace)
        .then_some(rest)
}

/// Where `word` last appears as a whole word.
fn word_position(text: &str, word: &str) -> Option<usize> {
    let mut search = text.len();
    while let Some(index) = text[..search].rfind(word) {
        let before = text[..index].chars().next_back();
        let after = text[index + word.len()..].chars().next();
        if before.is_none_or(char::is_whitespace) && after.is_none_or(char::is_whitespace) {
            return Some(index);
        }
        search = index;
    }
    None
}

/// Splits `name:` off the front of a line, returning the name and the rest.
///
/// The colon is the whole reason a line can be told apart from prose: `title: a
/// book` and `The title is long` begin the same way. A key cannot contain a
/// colon, so the first one always ends it.
fn split_key(trimmed: &str) -> Option<(&str, &str)> {
    let colon = trimmed.find(':')?;
    let name = &trimmed[..colon];
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    if name.starts_with(['{', '[', '#', '@', '$']) {
        return None;
    }
    Some((name, &trimmed[colon + 1..]))
}

#[derive(Debug, PartialEq, Eq)]
enum KeyTail<'a> {
    /// A `::` constraint is open and its type is being written.
    Constraint,
    /// Every constraint has closed, and this is the value written after them.
    Value(&'a str),
}

/// Where the text after a key leaves the cursor.
///
/// `name:: string: a description` -- the key took the first colon, so a
/// constraint opens on the next and closes on the one after it. Constraints
/// chain, so the state flips on every colon until the value begins.
fn key_tail(after_key: &str) -> KeyTail<'_> {
    if !after_key.starts_with(':') {
        return KeyTail::Value(after_key);
    }
    let mut open = false;
    for (index, character) in after_key.char_indices() {
        if character == ':' {
            open = !open;
        } else if !open && !character.is_whitespace() {
            // The value has started, and a colon inside a value is prose.
            return KeyTail::Value(&after_key[index..]);
        }
    }
    if open {
        KeyTail::Constraint
    } else {
        KeyTail::Value("")
    }
}

/// A value is either something to choose or something to write.
///
/// Piton values are prose, so most of what goes here is a sentence. Offering a
/// list of anchors part-way through one is offering things that have nothing to
/// do with what is being written, which is why a value only completes while it
/// is still short enough to be a name.
fn value_context<'a>(key: Option<&'a str>, written: &str) -> Context<'a> {
    // Nothing written yet is nothing to complete on: after `property: ` the
    // likely intent is to write prose, and a list popping up is in the way.
    if written.trim().is_empty() || is_prose(written) {
        Context::Nothing
    } else {
        Context::Value { key }
    }
}

/// Whether what has been written is prose rather than a name being completed.
///
/// A name has no spaces in it, and it has no full stop: a `.` outside an
/// expression is punctuation, and there is nothing to complete on the end of a
/// sentence.
fn is_prose(written: &str) -> bool {
    let written = written.trim_start();
    written.contains(char::is_whitespace) || written.contains('.')
}

/// The key a more-indented line belongs to.
///
/// `frameworks:` followed by `- {BelayConfiguration}` -- the list item carries
/// the value of the key above it, and the key is where the constraints saying
/// what may go in it were written.
/// Returns the key and the offset of the line it is on.
fn enclosing_key(text: &str, line_start: usize) -> Option<(&str, usize)> {
    let indent = indent_width(&text[line_start..]);
    let mut end = line_start;
    while end > 0 {
        let start = text[..end - 1].rfind('\n').map(|index| index + 1).unwrap_or(0);
        let line = &text[start..end - 1];
        end = start;
        if line.trim().is_empty() || indent_width(line) >= indent {
            continue;
        }
        // The first less-indented line is the one this belongs to, whether or
        // not it turns out to be a key.
        return split_key(line.trim_start()).map(|(key, _)| (key, start + indent_width(line)));
    }
    None
}

fn indent_width(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

/// Whether the cursor sits inside a string literal in an expression.
fn in_string(inside: &str) -> bool {
    let mut open = false;
    let mut escaped = false;
    for character in inside.chars() {
        if escaped {
            escaped = false;
        } else if character == '\\' && open {
            escaped = true;
        } else if character == '"' {
            open = !open;
        }
    }
    open
}

// --- Building the list ------------------------------------------------------

fn keywords(words: &[&str]) -> Vec<CompletionItem> {
    words
        .iter()
        .map(|word| CompletionItem {
            label: (*word).to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            ..Default::default()
        })
        .collect()
}

/// Every module a `use` or `from` could name.
///
/// The compiled graph holds only what the project already reaches, and the
/// point of completing an import is to reach something new, so the source root
/// is read from disk as well. The bundled packages are not on disk at all --
/// they live inside the compiler -- so they are named outright.
fn module_paths(cursor: &Cursor<'_>) -> Vec<CompletionItem> {
    let compilation = cursor.compilation;
    let source_root = &compilation.project.source_root;

    let mut items = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut offer = |path: String, detail: Option<String>, items: &mut Vec<CompletionItem>| {
        if !seen.insert(path.clone()) {
            return;
        }
        items.push(CompletionItem {
            label: path,
            kind: Some(CompletionItemKind::MODULE),
            detail,
            ..Default::default()
        });
    };

    for package in piton_compile::prelude::PACKAGE_ROOTS {
        offer(
            package.to_string(),
            Some("bundled package".into()),
            &mut items,
        );
    }

    // An installed package is imported by its name, so that is what is offered
    // -- not the relative path to the directory it happens to sit in.
    let project_root = &compilation.project.root;
    let installed = piton_compile::packages::installed_names(project_root);
    for name in &installed {
        offer(name.clone(), Some("installed package".into()), &mut items);
    }

    let mut files = piton_compile::module::sources(source_root);
    // A module already in the graph may sit outside the source root -- an
    // installed package does -- and is worth offering even so.
    files.extend(
        compilation
            .graph()
            .iter()
            .map(|module| module.path.clone())
            .filter(|path| !path.to_string_lossy().starts_with('@')),
    );

    for file in files {
        if file == cursor.path {
            continue;
        }
        match package_import_path(project_root, &installed, &file) {
            Some((written, detail)) => offer(written, Some(detail), &mut items),
            None => {
                let written = import_path(&cursor.path, &file, source_root);
                let detail = file
                    .strip_prefix(source_root)
                    .unwrap_or(&file)
                    .to_string_lossy()
                    .to_string();
                offer(written, Some(detail), &mut items);
            }
        }
    }
    items
}

/// How a file inside an installed package is imported: through the package
/// name, not through a relative path that climbs out of the source root.
fn package_import_path(
    project_root: &Path,
    installed: &[String],
    file: &Path,
) -> Option<(String, String)> {
    for name in installed {
        let directory = piton_compile::packages::package_directory(project_root, name);
        let Ok(relative) = file.strip_prefix(&directory) else {
            continue;
        };
        let inside = relative
            .with_extension("")
            .to_string_lossy()
            .replace('\\', "/");
        let written = match inside.trim_end_matches("/index") {
            "" | "index" => name.clone(),
            rest => format!("{name}/{rest}"),
        };
        return Some((written, format!("in package {name}")));
    }
    None
}

/// The names a module exports, for `from <path> import ...`.
fn import_names(cursor: &Cursor<'_>, written: &str) -> Vec<CompletionItem> {
    let compilation = cursor.compilation;

    // A path that names no module exports nothing, and saying otherwise would
    // be offering names that do not exist.
    let Some(module) = module_path_from(&compilation.project, &cursor.path, written)
        .and_then(|path| compilation.graph().id_for(&path))
    else {
        return Vec::new();
    };

    let mut items = vec![CompletionItem {
        label: "*".into(),
        kind: Some(CompletionItemKind::KEYWORD),
        detail: Some("every exported name".into()),
        ..Default::default()
    }];

    for name in compilation.resolution.exported_names(module) {
        let symbol = compilation
            .resolution
            .lookup_export(module, &name, &mut Default::default());
        items.push(match symbol {
            Some(symbol) => symbol_completion(compilation, &name, symbol),
            None => CompletionItem {
                label: name,
                kind: Some(CompletionItemKind::VALUE),
                ..Default::default()
            },
        });
    }
    items
}

/// The words that can open an unindented line.
fn declaration_heads(
    cursor: &Cursor<'_>,
    exported: bool,
    is_abstract: bool,
) -> Vec<CompletionItem> {
    let compilation = cursor.compilation;
    let mut words: Vec<&str> = Vec::new();
    if !exported && !is_abstract {
        // `export` and `abstract` both come before the keyword, and an import
        // takes neither.
        words.extend(["export", "use", "from"]);
    }
    if !is_abstract {
        words.push("abstract");
    }
    words.push("anchor");
    let mut items = keywords(&words);

    // A user keyword is sugar for `extends`, so it opens a declaration exactly
    // where `anchor` does.
    for (keyword, anchor) in &compilation.resolution.scope(cursor.module).keywords {
        let def = compilation.store().anchor(*anchor);
        items.push(CompletionItem {
            label: keyword.clone(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some(format!("extends {}", def.name)),
            data: data_for(&Target::Anchor(*anchor)),
            ..Default::default()
        });
    }
    items
}

/// The anchors a module can name.
///
/// `inheriting` narrows the list to the anchors a constraint would accept, and
/// `wrap` writes the interpolation around the name when the place the cursor is
/// in needs one.
fn anchors_in_scope(
    cursor: &Cursor<'_>,
    inheriting: Option<AnchorId>,
    wrap: Option<&str>,
) -> Vec<CompletionItem> {
    let compilation = cursor.compilation;
    let mut items = Vec::new();
    for (name, symbol) in compilation.resolution.visible_names(cursor.module) {
        let Symbol::Anchor(anchor) = symbol else {
            continue;
        };
        if let Some(base) = inheriting {
            if !compilation.store().inherits_from(anchor, base) {
                continue;
            }
        }
        let mut item = symbol_completion(compilation, &name, symbol);
        if let Some(sigil) = wrap {
            // A value that has to be an anchor is written as a reference to
            // one, so completing it should write the braces too.
            item.insert_text = Some(format!("{sigil}{name}}}"));
        }
        items.push(item);
    }
    items
}

/// Which of the names a module exports are worth offering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Only {
    /// Anything exported.
    Anything,
    /// An anchor, because nothing else can go where the cursor is.
    Anchors,
    /// An anchor a constraint would accept.
    AnchorsInheriting(AnchorId),
}

impl Only {
    fn accepts(self, compilation: &Compilation, symbol: Symbol) -> bool {
        match (self, symbol) {
            (Only::Anything, _) => true,
            (Only::Anchors, Symbol::Anchor(_)) => true,
            (Only::AnchorsInheriting(base), Symbol::Anchor(anchor)) => {
                compilation.store().inherits_from(anchor, base)
            }
            _ => false,
        }
    }
}

/// Names other modules export that this one cannot yet see.
///
/// Completing one of these writes the import as well. Knowing which file a
/// symbol lives in is exactly what the editor is for, and it is the part a
/// person has to go and look up otherwise.
fn importable(cursor: &Cursor<'_>, only: Only) -> Vec<CompletionItem> {
    // Every name in the project would be a list nobody reads, and a name that
    // is already in scope is a name the editor has offered already.
    const LIMIT: usize = 400;

    let compilation = cursor.compilation;
    let visible: HashSet<String> = compilation
        .resolution
        .visible_names(cursor.module)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    let insert = insertion_point(compilation, cursor.module, &cursor.text);
    let at = offset_to_position(&cursor.text, insert);

    // A name re-exported by an index is exported from several modules, and
    // they all lead to the same symbol. It is offered once, imported from the
    // shortest path -- usually the index, which is what re-exporting it was
    // for -- rather than once per module that passes it along.
    let mut chosen: Vec<(String, Symbol, String)> = Vec::new();
    for module in compilation.graph().iter() {
        if module.id == cursor.module {
            continue;
        }
        let written = import_path(&cursor.path, &module.path, &compilation.project.source_root);
        for name in compilation.resolution.exported_names(module.id) {
            if visible.contains(&name) {
                continue;
            }
            let Some(symbol) =
                compilation
                    .resolution
                    .lookup_export(module.id, &name, &mut Default::default())
            else {
                continue;
            };
            if !only.accepts(compilation, symbol) {
                continue;
            }
            match chosen
                .iter_mut()
                .find(|(seen, bound, _)| *seen == name && *bound == symbol)
            {
                Some(entry) => {
                    let shorter = (written.len(), &written) < (entry.2.len(), &entry.2);
                    if shorter {
                        entry.2 = written.clone();
                    }
                }
                None => chosen.push((name, symbol, written.clone())),
            }
        }
    }

    chosen
        .into_iter()
        .take(LIMIT)
        .map(|(name, symbol, written)| {
            let mut item = symbol_completion(compilation, &name, symbol);
            item.label_details = Some(CompletionItemLabelDetails {
                description: Some(written.clone()),
                ..Default::default()
            });
            // Below everything already in scope: a name the file can already
            // see is almost always the one that was meant.
            item.sort_text = Some(format!("9{name}"));
            item.additional_text_edits = Some(vec![TextEdit {
                range: Range { start: at, end: at },
                new_text: format!("from {written} import {name}\n"),
            }]);
            item
        })
        .collect()
}

/// The types a `::` constraint accepts.
fn type_names(cursor: &Cursor<'_>) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = piton_syntax::language::TYPE_NAMES
        .iter()
        .map(|name| CompletionItem {
            label: (*name).to_string(),
            kind: Some(CompletionItemKind::STRUCT),
            detail: Some("built-in type".into()),
            ..Default::default()
        })
        .collect();
    // `p:: extends Operator:` accepts any anchor whose chain includes one.
    items.extend(keywords(&["extends"]));
    // An anchor's name is a type as well, and constraining to one is how a
    // property says which anchors it will take.
    items.extend(anchors_in_scope(cursor, None, None));
    items.extend(importable(cursor, Only::Anchors));
    items
}

/// What can be written inside an interpolation.
fn expression(cursor: &Cursor<'_>, sigil: &str, member_of: Option<&str>) -> Vec<CompletionItem> {
    if let Some(path) = member_of {
        return members(cursor, path);
    }

    let compilation = cursor.compilation;
    let mut items = Vec::new();

    // `@{...}` has to resolve to a referenceable anchor, so a variable or a
    // literal in there is an error waiting to be written.
    let references_only = sigil == "@{";
    for (name, symbol) in compilation.resolution.visible_names(cursor.module) {
        if references_only && !matches!(symbol, Symbol::Anchor(_)) {
            continue;
        }
        items.push(symbol_completion(compilation, &name, symbol));
    }

    for keyword in piton_syntax::language::EXPRESSION_KEYWORDS {
        items.push(CompletionItem {
            label: (*keyword).to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some(match *keyword {
                "self" => "the most derived anchor".into(),
                "this" => "the anchor that wrote this line".into(),
                _ => "the base this anchor extends".to_string(),
            }),
            ..Default::default()
        });
    }

    if !references_only {
        items.extend(literals());
    }
    items.extend(importable(
        cursor,
        if references_only {
            Only::Anchors
        } else {
            Only::Anything
        },
    ));
    items
}

/// The members of whatever `path` resolves to.
///
/// `self.` and `this.` reach the enclosing anchor, `super.` its first base, and
/// a name reaches the anchor it refers to. Past the first segment the walk
/// continues through the resolved value, so `Config.frameworks.` offers what is
/// actually in there.
fn members(cursor: &Cursor<'_>, path: &str) -> Vec<CompletionItem> {
    let compilation = cursor.compilation;
    let mut segments = path.split('.');
    let Some(head) = segments.next() else {
        return Vec::new();
    };

    if head == "super" {
        let Some(anchor) = enclosing_anchor(cursor) else {
            return Vec::new();
        };
        let rest: Vec<&str> = segments.filter(|segment| !segment.is_empty()).collect();
        return match rest.split_first() {
            // `super` is everything the anchor inherits, merged the way
            // inheritance merges it, so every base contributes.
            None => super_slots(compilation, anchor),
            Some((first, deeper)) => {
                let Some(base) = crate::index::super_provider(compilation, anchor, first) else {
                    return Vec::new();
                };
                let Some(value) = compilation.store().anchor(base).properties.get(*first) else {
                    return Vec::new();
                };
                nested_members(compilation, value, deeper)
            }
        };
    }

    let anchor = match head {
        "self" | "this" => enclosing_anchor(cursor),
        name => match compilation.resolution.lookup(cursor.module, name) {
            Some(Symbol::Anchor(anchor)) => Some(anchor),
            _ => None,
        },
    };
    let Some(anchor) = anchor else {
        return Vec::new();
    };

    let def = compilation.store().anchor(anchor);
    let rest: Vec<&str> = segments.filter(|segment| !segment.is_empty()).collect();
    if rest.is_empty() {
        // At the anchor itself, the slots are the answer: they carry what was
        // inherited as well as what was written here.
        return slots_of(compilation, anchor);
    }

    // Past the anchor, the resolved value is the only thing that knows the
    // shape, because a nested key may have come from a base or an expression.
    let Some(value) = def.properties.get(rest[0]) else {
        return Vec::new();
    };
    nested_members(compilation, value, &rest[1..])
}

/// The keys under `path` inside a resolved value.
fn nested_members<'a>(
    compilation: &'a Compilation,
    mut value: &'a piton_core::Value,
    path: &[&str],
) -> Vec<CompletionItem> {
    for segment in path {
        let Some(next) = value.property(segment, compilation) else {
            return Vec::new();
        };
        value = next;
    }
    let entries: Vec<(String, &piton_core::Value)> = match value {
        piton_core::Value::Anchor(anchor) => return slots_of(compilation, *anchor),
        piton_core::Value::Dict(map) => map.iter().map(|(k, v)| (k.clone(), v)).collect(),
        piton_core::Value::Mixed(mixed) => mixed
            .entries()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        _ => return Vec::new(),
    };
    entries
        .into_iter()
        .map(|(name, nested)| CompletionItem {
            label: name,
            kind: Some(CompletionItemKind::FIELD),
            detail: Some(nested.kind().to_string()),
            ..Default::default()
        })
        .collect()
}

/// What `super.` reaches: the slots of every base, the right-most base
/// winning a name more than one of them has.
fn super_slots(compilation: &Compilation, anchor: AnchorId) -> Vec<CompletionItem> {
    let store = compilation.store();
    let mut seen: Vec<String> = Vec::new();
    let mut items = Vec::new();
    for base in store.anchor(anchor).bases.iter().rev() {
        for item in slots_of(compilation, *base) {
            if seen.contains(&item.label) {
                continue;
            }
            seen.push(item.label.clone());
            items.push(item);
        }
    }
    items
}

/// An anchor's properties, its inherited ones included.
fn slots_of(compilation: &Compilation, anchor: AnchorId) -> Vec<CompletionItem> {
    let def = compilation.store().anchor(anchor);
    def.slots
        .iter()
        .map(|(name, slot)| {
            let owner = compilation.store().anchor(slot.owner);
            CompletionItem {
                label: name.clone(),
                kind: Some(CompletionItemKind::PROPERTY),
                detail: (slot.owner != anchor).then(|| format!("inherited from {}", owner.name)),
                data: data_for(&Target::Property(anchor, name.clone())),
                ..Default::default()
            }
        })
        .collect()
}

/// What a value may be, according to the constraints on the key above it.
fn values(cursor: &Cursor<'_>, key: Option<&str>) -> Vec<CompletionItem> {
    let compilation = cursor.compilation;
    let declared = key.and_then(|key| {
        let anchor = enclosing_anchor(cursor)?;
        let slot = compilation.store().anchor(anchor).slots.get(key)?.clone();
        Some((anchor, slot))
    });

    let Some((anchor, slot)) = declared.filter(|(_, slot)| !slot.constraints.is_empty()) else {
        // Nothing says what belongs here, so there is nothing to propose. A
        // value in Piton is prose by default, and `property: ` with a list of
        // every anchor in the project under it is the editor guessing at a
        // sentence it cannot possibly guess.
        return Vec::new();
    };

    let mut items = Vec::new();
    for constraint in &slot.constraints {
        match &constraint.name {
            ast::TypeName::Boolean => items.extend(constants(&["true", "false"])),
            ast::TypeName::Null => items.extend(constants(&["null"])),
            // `anchor` admits nothing but an anchor, so every anchor is a
            // candidate. `complex` is not the same thing: it admits a list or
            // a dictionary too, which are written rather than chosen.
            ast::TypeName::Anchor => items.extend(anchors(cursor, None, "{")),
            ast::TypeName::Reference => items.extend(anchors(cursor, None, "@{")),
            ast::TypeName::Named(name) => {
                match constraint_type(cursor, anchor, &slot, name) {
                    Some(base) => items.extend(anchors(cursor, Some(base), "{")),
                    // A constraint naming something that does not resolve
                    // accepts nothing, and offering everything instead would
                    // be inventing an answer.
                    None => {}
                }
            }
            // Everything else describes the shape of a value rather than
            // naming which values there are. `any`, `simple` and `complex` say
            // "a value"; a string, a number, a list or a dictionary is written
            // rather than chosen. A list of every anchor in the project under
            // any of them is the editor guessing at prose.
            _ => {}
        }
    }
    items
}

/// The anchor a constraint's type name refers to.
///
/// The name was written wherever the constraint was, which is not necessarily
/// here: a property carries its constraints down the inheritance chain, and the
/// names in them were resolved against the imports of the file that wrote them.
/// The slot records which anchor supplied the *value*, not which supplied the
/// constraint, so every file along the chain is tried.
fn constraint_type(
    cursor: &Cursor<'_>,
    anchor: AnchorId,
    slot: &piton_compile::store::Slot,
    name: &str,
) -> Option<AnchorId> {
    let compilation = cursor.compilation;
    let store = compilation.store();

    let mut modules = vec![cursor.module, store.anchor(slot.owner).module];
    modules.extend(
        store
            .base_chain(anchor)
            .into_iter()
            .map(|base| store.anchor(base).module),
    );

    for module in modules {
        if let Some(Symbol::Anchor(base)) = compilation.resolution.lookup(module, name) {
            return Some(base);
        }
    }
    None
}

/// Every anchor that fits, in scope or not, written as an interpolation.
///
/// A property constrained to an anchor is the case where the name being
/// offered is most likely to live in a file this one has never mentioned, so
/// the ones that need an import are here too, with the import attached.
fn anchors(cursor: &Cursor<'_>, inheriting: Option<AnchorId>, sigil: &str) -> Vec<CompletionItem> {
    let mut items = anchors_in_scope(cursor, inheriting, Some(sigil));
    items.extend(importable(
        cursor,
        match inheriting {
            Some(base) => Only::AnchorsInheriting(base),
            None => Only::Anchors,
        },
    ));
    for item in &mut items {
        if item.insert_text.is_none() {
            item.insert_text = Some(format!("{sigil}{}}}", item.label));
        }
    }
    items
}

fn literals() -> Vec<CompletionItem> {
    constants(piton_syntax::language::LITERALS)
}

fn constants(words: &[&str]) -> Vec<CompletionItem> {
    words
        .iter()
        .map(|word| CompletionItem {
            label: (*word).to_string(),
            kind: Some(CompletionItemKind::CONSTANT),
            ..Default::default()
        })
        .collect()
}

/// Whether the line the cursor is on sits inside a property's dictionary
/// rather than directly in an anchor body.
///
/// The first less-indented line above is what the line belongs to: the
/// declaration (at the margin) for a body line, a key for a nested one.
fn in_nested_block(text: &str, offset: usize) -> bool {
    let line_start = text[..offset].rfind('\n').map(|index| index + 1).unwrap_or(0);
    let indent = indent_width(&text[line_start..]);
    for line in text[..line_start].lines().rev() {
        if line.trim().is_empty() || indent_width(line) >= indent {
            continue;
        }
        return indent_width(line) > 0;
    }
    false
}

/// The keys that belong in the anchor body the cursor is inside.
fn body(cursor: &Cursor<'_>) -> Vec<CompletionItem> {
    let compilation = cursor.compilation;
    let Some(anchor) = enclosing_anchor(cursor) else {
        return Vec::new();
    };
    // Inside a nested dictionary the anchor's own properties are not what
    // goes there, and nothing says what does.
    if in_nested_block(&cursor.text, cursor.offset) {
        return Vec::new();
    }
    let def = compilation.store().anchor(anchor);

    let mut items = Vec::new();
    for (name, slot) in &def.slots {
        let owner = compilation.store().anchor(slot.owner);
        let required = !slot.has_value;
        items.push(CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: (slot.owner != anchor).then(|| format!("inherited from {}", owner.name)),
            insert_text: Some(format!("{name}: ")),
            // A property an abstract base left without a value has to be
            // written, so it belongs at the top of the list.
            sort_text: Some(format!("{}{name}", if required { '0' } else { '1' })),
            label_details: required.then(|| CompletionItemLabelDetails {
                description: Some("required".into()),
                ..Default::default()
            }),
            data: data_for(&Target::Property(anchor, name.clone())),
            ..Default::default()
        });
    }

    // An anchor with nothing to say still has to say so.
    items.push(CompletionItem {
        label: "pass".into(),
        kind: Some(CompletionItemKind::KEYWORD),
        detail: Some("an intentionally empty body".into()),
        sort_text: Some("2pass".into()),
        ..Default::default()
    });
    items
}

/// One completion for a name that resolved to something.
fn symbol_completion(compilation: &Compilation, name: &str, symbol: Symbol) -> CompletionItem {
    match symbol {
        Symbol::Anchor(anchor) => {
            let def = compilation.store().anchor(anchor);
            CompletionItem {
                label: name.to_string(),
                kind: Some(if def.is_abstract {
                    CompletionItemKind::INTERFACE
                } else {
                    CompletionItemKind::CLASS
                }),
                detail: Some(def.keyword.clone()),
                data: data_for(&Target::Anchor(anchor)),
                ..Default::default()
            }
        }
        Symbol::Variable(variable) => {
            let def = compilation.store().variable(variable);
            CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::VARIABLE),
                detail: def.value.as_ref().map(|value| value.kind().to_string()),
                data: data_for(&Target::Variable(variable)),
                ..Default::default()
            }
        }
    }
}

/// The anchor whose body the cursor is in.
///
/// A declaration's span covers what was written, and what is being written is
/// past the end of it: a body with nothing in it yet ends where its header
/// does, which is exactly when the keys it can carry are most worth offering.
/// So the search falls back to the last declaration above the cursor and
/// confirms that nothing between the two has returned to column zero, which is
/// the only thing that ends an indented body.
fn enclosing_anchor(cursor: &Cursor<'_>) -> Option<AnchorId> {
    let compilation = cursor.compilation;
    let ast = compilation.graph().get(cursor.module).ast();

    let mut candidate = None;
    for (index, item) in ast.items.iter().enumerate() {
        let Item::Anchor(decl) = item else { continue };
        if decl.span.start > cursor.offset {
            break;
        }
        candidate = Some((index, decl));
    }
    let (index, decl) = candidate?;

    let inside = decl.span.contains(cursor.offset) || decl.span.end == cursor.offset;
    if !inside && returns_to_column_zero(&cursor.text, decl.span.end, cursor.offset) {
        return None;
    }

    compilation
        .store()
        .anchors
        .iter()
        .find(|def| def.module == cursor.module && def.item == index)
        .map(|def| def.id)
}

/// Whether any line between two offsets starts at column zero with something
/// other than whitespace.
fn returns_to_column_zero(text: &str, from: usize, to: usize) -> bool {
    let from = from.min(to);
    text[from..to]
        .split('\n')
        // `from` is the end of a line that has already been accounted for.
        .skip(1)
        .any(|line| !line.is_empty() && !line.starts_with([' ', '\t']))
}

// ---------------------------------------------------------------------------
// Formatting, folding, selection
// ---------------------------------------------------------------------------

pub fn formatting(world: &World, uri: &Url) -> Option<Vec<TextEdit>> {
    let path = url_to_path(uri)?;
    let text = world.text(&path)?;
    // The editor's formatting path is autoformat: it normalizes structure but
    // never rewrites commented content, because that is what a save-time format
    // must not do. The explicit `piton format` command is the one that
    // normalizes comments.
    let formatted = format::autoformat(&text, &path);
    if formatted == text {
        return Some(Vec::new());
    }
    Some(vec![TextEdit {
        range: Range {
            start: Position::default(),
            end: offset_to_position(&text, text.len()),
        },
        new_text: formatted,
    }])
}

/// The indentation a new line should carry, after the newline that opened it.
///
/// A declaration or a key that ends in a colon opens a block, and the block is
/// the indented lines beneath it -- so pressing enter there should land one
/// level in, not back at the margin. Every other line continues at the
/// indentation it was already at.
///
/// This is the language server's job rather than the editor's because the
/// grammar cannot say it. Indentation is deliberately outside the grammar, so
/// no node spans a body for a tree-sitter indent query to measure, and the
/// editors that have no tree-sitter at all have nothing else to go on.
pub fn on_type_formatting(
    world: &World,
    uri: &Url,
    position: Position,
    typed: &str,
) -> Option<Vec<TextEdit>> {
    // Only the newline is interesting. A client may send others if it asks for
    // them; nothing else changes the indentation of a line.
    if typed != "\n" && typed != "\r\n" {
        return None;
    }

    let path = url_to_path(uri)?;
    let text = world.text(&path)?;
    let offset = position_to_offset(&text, position);

    let line_start = text[..offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    if line_start == 0 {
        // Nothing above to take the indentation from.
        return None;
    }
    let previous = previous_line(&text, line_start);

    // Inside a fenced block or an escape block the text is verbatim, and
    // reindenting it would change what it says.
    if verbatim_at(&text, line_start) {
        return None;
    }

    let indent = if previous.is_empty() {
        indent_after_emptied_lines(&text, line_start)
    } else {
        wanted_indent(previous)
    };

    // The line may already carry indentation: some editors copy the one above
    // across, some compute their own, and the cursor may sit anywhere inside
    // it -- or still at the margin. Measure the whole leading run rather than
    // just the part before the cursor and replace all of it. Replacing only
    // what sits before the cursor would stack the two together, and the line
    // would end up twice as deep as it should be.
    let written = text[line_start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .count();
    let existing = &text[line_start..line_start + indent_bytes(&text[line_start..], written)];
    if existing == indent {
        return None;
    }

    Some(vec![TextEdit {
        range: Range {
            start: offset_to_position(&text, line_start),
            end: offset_to_position(&text, line_start + existing.len()),
        },
        new_text: indent,
    }])
}

/// The indentation after a line that is completely empty.
///
/// Some editors, Zed among them, strip the whitespace from a blank line as
/// Enter leaves it, so an indented blank line reaches the server as an empty
/// one and looks like it sits at the margin. So an empty line is measured from
/// the last line with content above it: the depth that line leads into, less
/// one level for each empty line since, which is how deep the blank lines
/// would have been had they kept their indentation.
fn indent_after_emptied_lines(text: &str, line_start: usize) -> String {
    let mut end = line_start - 1;
    let mut blank = 0usize;
    loop {
        let start = text[..end].rfind('\n').map(|index| index + 1).unwrap_or(0);
        let line = text[start..end].trim_end_matches('\r');
        if !line.trim().is_empty() {
            let base = wanted_indent(line).len();
            return " ".repeat(base.saturating_sub(blank * format::INDENT));
        }
        blank += 1;
        if start == 0 {
            return String::new();
        }
        end = start - 1;
    }
}

/// The line above the one starting at `line_start`, without its newline.
fn previous_line(text: &str, line_start: usize) -> &str {
    let above = &text[..line_start - 1];
    let start = above.rfind('\n').map(|index| index + 1).unwrap_or(0);
    above[start..].trim_end_matches('\r')
}

/// The indentation the line after `previous` should carry.
///
/// Two rules apply, and which one fires depends on the line being left behind.
/// A declaration or key that ends in a colon opens a block, so the line after
/// it lands one level in. A blank line is the opposite case: pressing enter on
/// a blank line inside an indented block backs the new line *out* one level, so
/// working down through blank lines walks you up and out of the nesting instead
/// of holding you at the depth you started at.
fn wanted_indent(previous: &str) -> String {
    let width = previous
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .count();

    // The line being left is blank and indented: dedent the new line by one
    // level. A blank line at the margin has nothing to dedent out of, so it
    // falls through and stays put.
    if previous.trim().is_empty() && width > 0 {
        return " ".repeat(width.saturating_sub(format::INDENT));
    }

    let width = if opens_a_block(previous) {
        width + format::INDENT
    } else {
        width
    };
    // Four spaces, and not configurable: `piton format` writes them whatever
    // the file already uses, so typing anything else only makes work.
    " ".repeat(width)
}

/// Whether the line ends by opening a block for the lines beneath it.
///
/// A declaration or a key that ends in its colon opens one: `anchor A:`,
/// `frameworks:`, `config:: dictionary:`. A colon inside a value opens
/// nothing -- `prompt: Careful: ` ends a sentence, not a property, and
/// indenting after it would push the next line a level deeper than it
/// belongs. And a comment is not structure whatever it happens to end with.
fn opens_a_block(line: &str) -> bool {
    let code = strip_comment(line).trim_end();
    if !code.ends_with(':') {
        return false;
    }

    // A list item is never a key, colon or not -- `- Settings:` is just the
    // string `Settings:` -- and a `+`/`++` line holds a value to combine, so
    // neither opens a block.
    let trimmed = code.trim_start();
    if trimmed == "-" || trimmed.starts_with("- ") || trimmed.starts_with('+') {
        return false;
    }

    match split_key(trimmed) {
        // The line ends where the value would begin: nothing has been
        // written after the colon that opens it.
        Some((_, after)) => {
            matches!(key_tail(after), KeyTail::Value(value) if value.trim().is_empty())
        }
        // No key on the line. A declaration sits at the margin and opens;
        // indented text without a key is prose, and prose opens nothing.
        None => !code.starts_with([' ', '\t']),
    }
}

/// The line with its comment removed. A comment is a whole line, so this is
/// either the line or nothing but its indentation.
fn strip_comment(line: &str) -> &str {
    // A comment has to be on its own line; after code, `//` is just text.
    piton_syntax::prose::split_comment(line).0
}

/// How many bytes `chars` characters of leading whitespace occupy.
fn indent_bytes(line: &str, chars: usize) -> usize {
    line.char_indices()
        .nth(chars)
        .map(|(index, _)| index)
        .unwrap_or(line.len())
}

pub fn folding(world: &World, uri: &Url) -> Option<Vec<FoldingRange>> {
    let cursor = file_context(world, uri)?;
    let index = world.index.get(cursor.module)?;
    let mut out = Vec::new();
    for (span, _) in &index.structures {
        let start = offset_to_position(&cursor.text, span.start);
        let end = offset_to_position(&cursor.text, span.end);
        if end.line <= start.line {
            continue;
        }
        out.push(FoldingRange {
            start_line: start.line,
            end_line: end.line,
            kind: Some(FoldingRangeKind::Region),
            ..Default::default()
        });
    }
    // Comment blocks fold too.
    let syntax = cursor.compilation.graph().get(cursor.module).parse.syntax();
    let mut comment_start: Option<u32> = None;
    let mut previous: Option<u32> = None;
    for token in syntax
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
    {
        if token.kind() != SyntaxKind::COMMENT {
            continue;
        }
        let line = offset_to_position(&cursor.text, usize::from(token.text_range().start())).line;
        match (comment_start, previous) {
            (Some(start), Some(last)) if line == last + 1 => {
                previous = Some(line);
                let _ = start;
            }
            _ => {
                if let (Some(start), Some(last)) = (comment_start, previous) {
                    if last > start {
                        out.push(FoldingRange {
                            start_line: start,
                            end_line: last,
                            kind: Some(FoldingRangeKind::Comment),
                            ..Default::default()
                        });
                    }
                }
                comment_start = Some(line);
                previous = Some(line);
            }
        }
    }
    if let (Some(start), Some(last)) = (comment_start, previous) {
        if last > start {
            out.push(FoldingRange {
                start_line: start,
                end_line: last,
                kind: Some(FoldingRangeKind::Comment),
                ..Default::default()
            });
        }
    }
    Some(out)
}

pub fn selection_ranges(
    world: &World,
    uri: &Url,
    positions: &[Position],
) -> Option<Vec<SelectionRange>> {
    let cursor = file_context(world, uri)?;
    let index = world.index.get(cursor.module)?;
    let mut out = Vec::new();

    for position in positions {
        let offset = position_to_offset(&cursor.text, *position);
        // Expanding selection should move from a value to its property, to the
        // declaration, to the file: widest last.
        let mut spans: Vec<Span> = index
            .structures
            .iter()
            .map(|(span, _)| *span)
            .filter(|span| span.contains(offset) || span.end == offset)
            .collect();
        if let Some(occurrence) = index.at(offset) {
            spans.push(occurrence.span);
        }
        spans.push(Span::new(0, cursor.text.len()));
        spans.sort_by_key(|span| span.len());
        spans.dedup();

        let mut range: Option<SelectionRange> = None;
        for span in spans.into_iter().rev() {
            range = Some(SelectionRange {
                range: span_to_range(&cursor.text, span),
                parent: range.map(Box::new),
            });
        }
        out.push(range.unwrap_or(SelectionRange {
            range: Range {
                start: *position,
                end: *position,
            },
            parent: None,
        }));
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Code actions
// ---------------------------------------------------------------------------

pub fn code_actions(world: &World, uri: &Url, range: Range) -> Option<CodeActionResponse> {
    let cursor = cursor(world, uri, range.start)?;
    let compilation = cursor.compilation;
    let index = world.index.get(cursor.module)?;
    let mut actions: Vec<CodeActionOrCommand> = Vec::new();

    let start = position_to_offset(&cursor.text, range.start);
    let end = position_to_offset(&cursor.text, range.end);

    for occurrence in &index.occurrences {
        if occurrence.span.end < start || occurrence.span.start > end {
            continue;
        }
        let Target::Unresolved(name) = &occurrence.target else {
            continue;
        };

        // Offer to import the symbol from wherever it is exported.
        for source in modules_exporting(compilation, name) {
            let source_path = compilation.graph().get(source).path.clone();
            let written = import_path(&cursor.path, &source_path, &world.project.source_root);
            let insert = insertion_point(compilation, cursor.module, &cursor.text);
            let edit = TextEdit {
                range: Range {
                    start: offset_to_position(&cursor.text, insert),
                    end: offset_to_position(&cursor.text, insert),
                },
                new_text: format!("from {written} import {name}\n"),
            };
            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: format!("Import `{name}` from `{written}`"),
                kind: Some(CodeActionKind::QUICKFIX),
                edit: Some(WorkspaceEdit {
                    changes: Some(HashMap::from([(uri.clone(), vec![edit])])),
                    ..Default::default()
                }),
                ..Default::default()
            }));
        }

        // Or to create it.
        let end_of_file = cursor.text.len();
        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: format!("Create anchor `{name}`"),
            kind: Some(CodeActionKind::QUICKFIX),
            edit: Some(WorkspaceEdit {
                changes: Some(HashMap::from([(
                    uri.clone(),
                    vec![TextEdit {
                        range: Range {
                            start: offset_to_position(&cursor.text, end_of_file),
                            end: offset_to_position(&cursor.text, end_of_file),
                        },
                        new_text: format!("\nexport anchor {name}:\n    description:\n"),
                    }],
                )])),
                ..Default::default()
            }),
            ..Default::default()
        }));
    }

    // Offer to export a declaration that nothing outside the file can see.
    for item in &compilation.graph().get(cursor.module).ast().items {
        let (name, span, exported) = match item {
            Item::Anchor(decl) => (&decl.name, decl.span, decl.exported),
            Item::Variable(decl) => (&decl.name, decl.span, decl.exported),
            _ => continue,
        };
        if exported || !(span.contains(start) || span.contains(end)) {
            continue;
        }
        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: format!("Export `{name}`"),
            kind: Some(CodeActionKind::REFACTOR),
            edit: Some(WorkspaceEdit {
                changes: Some(HashMap::from([(
                    uri.clone(),
                    vec![TextEdit {
                        range: Range {
                            start: offset_to_position(&cursor.text, span.start),
                            end: offset_to_position(&cursor.text, span.start),
                        },
                        new_text: "export ".into(),
                    }],
                )])),
                ..Default::default()
            }),
            ..Default::default()
        }));
    }

    actions.extend(qualify_actions(world, &cursor, uri, start, end));
    actions.extend(conflict_actions(&cursor, uri, start, end));

    if let Some(edits) = organize_import_edits(world, &cursor) {
        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: "Organize imports".into(),
            kind: Some(CodeActionKind::SOURCE_ORGANIZE_IMPORTS),
            edit: Some(WorkspaceEdit {
                changes: Some(HashMap::from([(uri.clone(), edits)])),
                ..Default::default()
            }),
            ..Default::default()
        }));
    }

    if formatting(world, uri).is_some_and(|edits| !edits.is_empty()) {
        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: "Format this file".into(),
            kind: Some(CodeActionKind::SOURCE),
            edit: Some(WorkspaceEdit {
                changes: Some(HashMap::from([(
                    uri.clone(),
                    formatting(world, uri).unwrap_or_default(),
                )])),
                ..Default::default()
            }),
            ..Default::default()
        }));
    }

    Some(actions)
}

fn edit_action(title: String, kind: CodeActionKind, uri: &Url, edits: Vec<TextEdit>) -> CodeActionOrCommand {
    CodeActionOrCommand::CodeAction(CodeAction {
        title,
        kind: Some(kind),
        edit: Some(WorkspaceEdit {
            changes: Some(HashMap::from([(uri.clone(), edits)])),
            ..Default::default()
        }),
        ..Default::default()
    })
}

/// A local name for `name` imported from `path` that says where it came
/// from: `./lib/Type` gives `LibType`, `./other` gives `OtherButton`.
fn qualified_alias(path: &str, name: &str) -> String {
    let trimmed = path.trim_end_matches('/').trim_end_matches(".pi");
    let mut segments: Vec<&str> = trimmed
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != "." && *segment != "..")
        .map(|segment| segment.trim_start_matches('@'))
        .collect();
    if segments.last() == Some(&"index") {
        segments.pop();
    }
    let mut stem = segments.pop().unwrap_or("Imported");
    if stem == name {
        stem = segments.pop().unwrap_or("Imported");
    }
    let mut words = String::new();
    for part in stem.split(['-', '_']) {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            words.extend(first.to_uppercase());
            words.push_str(chars.as_str());
        }
    }
    format!("{words}{name}")
}

/// "Qualify an ambiguous reference": give one of the imports behind an
/// ambiguous name an alias, and say at the reference which one is meant.
fn qualify_actions(
    world: &World,
    cursor: &Cursor<'_>,
    uri: &Url,
    start: usize,
    end: usize,
) -> Vec<CodeActionOrCommand> {
    let touches = |span: Span| span.start <= end && span.end >= start;
    let mut actions = Vec::new();
    for ambiguity in &world.analysis.ambiguities {
        if ambiguity.module != cursor.module {
            continue;
        }
        let Some(here) = ambiguity
            .references
            .iter()
            .copied()
            .find(|span| touches(*span))
        else {
            continue;
        };
        for (position, binding) in ambiguity.bindings.iter().enumerate() {
            let Some(from) = &binding.from else {
                continue;
            };
            let alias = qualified_alias(from, &ambiguity.name);
            let entry = &cursor.text[binding.entry_span.start..binding.entry_span.end];
            // `Name` becomes `Name Alias`; `Name Old` becomes `Name Alias`.
            let original = entry.split_whitespace().next().unwrap_or(&ambiguity.name);
            let mut edits = vec![TextEdit {
                range: span_to_range(&cursor.text, binding.entry_span),
                new_text: format!("{original} {alias}"),
            }];
            // Every reference meant the winning binding, so qualifying that
            // one rewrites them all; qualifying another only rewrites the
            // reference the action was asked from.
            let rewritten: Vec<Span> = if position == ambiguity.winner {
                ambiguity.references.clone()
            } else {
                vec![here]
            };
            for span in rewritten {
                edits.push(TextEdit {
                    range: span_to_range(&cursor.text, span),
                    new_text: alias.clone(),
                });
            }
            actions.push(edit_action(
                format!(
                    "Qualify `{}` as `{alias}` (the one from `{from}`)",
                    ambiguity.name
                ),
                CodeActionKind::QUICKFIX,
                uri,
                edits,
            ));
        }
    }
    actions
}

/// "Resolve a simple inheritance conflict": when two bases disagree about a
/// property, say which one should win -- by reordering `extends`, by writing
/// the property on the child, or, for abstracts whose constraints cannot
/// both hold, by dropping one of them.
fn conflict_actions(
    cursor: &Cursor<'_>,
    uri: &Url,
    start: usize,
    end: usize,
) -> Vec<CodeActionOrCommand> {
    let compilation = cursor.compilation;
    let store = compilation.store();
    let ast = compilation.graph().get(cursor.module).ast();
    let mut actions = Vec::new();

    for (item_index, item) in ast.items.iter().enumerate() {
        let Item::Anchor(decl) = item else { continue };
        // Asked from the declaration's header line.
        let header_end = cursor.text[decl.span.start..]
            .find('\n')
            .map(|offset| decl.span.start + offset)
            .unwrap_or(decl.span.end);
        if start > header_end || end < decl.span.start {
            continue;
        }
        let Some(def) = store
            .anchors
            .iter()
            .find(|def| def.module == cursor.module && def.item == item_index)
        else {
            continue;
        };
        if def.bases.len() < 2 {
            continue;
        }
        let explicit: Vec<(String, Span, Option<AnchorId>)> = decl
            .extends
            .iter()
            .map(|base| {
                let id = match compilation.resolution.lookup(cursor.module, &base.value) {
                    Some(Symbol::Anchor(anchor)) => Some(anchor),
                    _ => None,
                };
                (base.value.clone(), base.span, id)
            })
            .collect();
        let list_span = match (explicit.first(), explicit.last()) {
            (Some(first), Some(last)) => Some(Span::new(first.1.start, last.1.end)),
            _ => None,
        };
        let rewrite_extends = |order: &[&str]| -> Option<TextEdit> {
            let span = list_span?;
            if order.is_empty() {
                // Drop ` extends ...` altogether.
                let keyword = cursor.text[..span.start].rfind("extends")?;
                let from = cursor.text[..keyword].trim_end().len();
                return Some(TextEdit {
                    range: span_to_range(&cursor.text, Span::new(from, span.end)),
                    new_text: String::new(),
                });
            }
            Some(TextEdit {
                range: span_to_range(&cursor.text, span),
                new_text: order.join(", "),
            })
        };

        // Abstracts whose constraints cannot both hold: the compiler says
        // so, and the fix is to implement one of them, not both.
        let conflicting = compilation.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "conflicting-abstracts"
                && paths_equal(&diagnostic.file, &cursor.path)
                && diagnostic.span == def.name_span
        });
        if conflicting {
            for (name, _, id) in &explicit {
                if !id.is_some_and(|id| store.anchor(id).is_abstract) {
                    continue;
                }
                let remaining: Vec<&str> = explicit
                    .iter()
                    .map(|(other, _, _)| other.as_str())
                    .filter(|other| other != name)
                    .collect();
                if let Some(edit) = rewrite_extends(&remaining) {
                    actions.push(edit_action(
                        format!("Stop extending `{name}`"),
                        CodeActionKind::QUICKFIX,
                        uri,
                        vec![edit],
                    ));
                }
            }
        }

        // Properties more than one base supplies, with different values,
        // that the child does not settle itself.
        let own: Vec<&str> = decl.body.properties().map(|p| p.name.as_str()).collect();
        let indent = decl
            .body
            .items
            .first()
            .map(|item| {
                let line_start = cursor.text[..item.span().start]
                    .rfind('\n')
                    .map(|index| index + 1)
                    .unwrap_or(0);
                cursor.text[line_start..item.span().start].to_string()
            })
            .filter(|indent| indent.trim().is_empty() && !indent.is_empty())
            .unwrap_or_else(|| " ".repeat(format::INDENT));
        let insert_at = (header_end + 1).min(cursor.text.len());
        let mut names: Vec<&String> = def.slots.keys().collect();
        names.retain(|name| !own.contains(&name.as_str()));
        for name in names {
            let suppliers: Vec<AnchorId> = def
                .bases
                .iter()
                .copied()
                .filter(|base| store.anchor(*base).properties.contains_key(name))
                .collect();
            if suppliers.len() < 2 {
                continue;
            }
            let winner = *suppliers.last().expect("two suppliers");
            let winning = store.anchor(winner).properties.get(name);
            for loser in suppliers.iter().copied().filter(|base| *base != winner) {
                if store.anchor(loser).properties.get(name) == winning {
                    continue;
                }
                let loser_name = store.anchor(loser).name.clone();
                let winner_name = store.anchor(winner).name.clone();
                // Moving the base last in `extends` makes it win -- unless
                // the winner came from a keyword, which is always last.
                let movable = explicit.iter().any(|(_, _, id)| *id == Some(loser))
                    && explicit.iter().any(|(_, _, id)| *id == Some(winner));
                if movable {
                    let mut order: Vec<&str> = explicit
                        .iter()
                        .map(|(written, _, _)| written.as_str())
                        .filter(|written| *written != loser_name)
                        .collect();
                    order.push(loser_name.as_str());
                    if let Some(edit) = rewrite_extends(&order) {
                        actions.push(edit_action(
                            format!("Move `{loser_name}` last in `extends` so its `{name}` wins"),
                            CodeActionKind::QUICKFIX,
                            uri,
                            vec![edit],
                        ));
                    }
                }
                if compilation.resolution.lookup(cursor.module, &loser_name)
                    == Some(Symbol::Anchor(loser))
                {
                    actions.push(edit_action(
                        format!(
                            "Take `{name}` from `{loser_name}` (it currently comes from `{winner_name}`)"
                        ),
                        CodeActionKind::QUICKFIX,
                        uri,
                        vec![TextEdit {
                            range: span_to_range(&cursor.text, Span::empty(insert_at)),
                            new_text: format!("{indent}{name}: {{{loser_name}.{name}}}\n"),
                        }],
                    ));
                }
            }
        }
    }
    actions
}

fn organize_import_edits(world: &World, cursor: &Cursor<'_>) -> Option<Vec<TextEdit>> {
    let text = world.text(&cursor.path)?;
    let edits =
        crate::analysis::organize_imports(cursor.compilation, &world.index, cursor.module, &text)?;
    Some(
        edits
            .into_iter()
            .map(|(span, new_text)| TextEdit {
                range: span_to_range(&text, span),
                new_text,
            })
            .collect(),
    )
}

fn modules_exporting(compilation: &Compilation, name: &str) -> Vec<piton_compile::ModuleId> {
    compilation
        .graph()
        .iter()
        .filter(|module| {
            compilation
                .resolution
                .lookup_export(module.id, name, &mut Default::default())
                .is_some()
        })
        .map(|module| module.id)
        .collect()
}

/// The path text an import should use to reach `target` from `from`.
fn import_path(from: &Path, target: &Path, _source_root: &Path) -> String {
    if target.to_string_lossy().starts_with('@') {
        return target.to_string_lossy().to_string();
    }
    let directory = from.parent().unwrap_or(Path::new("."));
    let stripped = target.with_extension("");
    let relative = piton_compile::module::relative_path(directory, &stripped);
    let text = relative.to_string_lossy().replace('\\', "/");
    // `index` is written as the directory it lives in.
    let text = text
        .strip_suffix("/index")
        .map(str::to_string)
        .unwrap_or(text);
    if text.starts_with("..") || text.starts_with('.') {
        text
    } else {
        format!("./{text}")
    }
}

/// Where a new import declaration goes: after the last existing one.
fn insertion_point(
    compilation: &Compilation,
    module: piton_compile::ModuleId,
    text: &str,
) -> usize {
    let ast = compilation.graph().get(module).ast();
    let mut last = 0usize;
    for item in &ast.items {
        if matches!(item, Item::Use(_) | Item::From(_)) {
            last = item.span().end;
        }
    }
    if last == 0 {
        return 0;
    }
    text[last..]
        .find('\n')
        .map(|offset| last + offset + 1)
        .unwrap_or(text.len())
}

// ---------------------------------------------------------------------------
// Inlay hints
// ---------------------------------------------------------------------------

pub fn inlay_hints(world: &World, uri: &Url, range: Range) -> Option<Vec<InlayHint>> {
    let cursor = file_context(world, uri)?;
    let compilation = cursor.compilation;
    let start = position_to_offset(&cursor.text, range.start);
    let end = position_to_offset(&cursor.text, range.end);
    let mut out = Vec::new();

    let ast = compilation.graph().get(cursor.module).ast();
    for (item_index, item) in ast.items.iter().enumerate() {
        let Item::Anchor(decl) = item else { continue };
        let Some(anchor) = compilation
            .store()
            .anchors
            .iter()
            .find(|def| def.module == cursor.module && def.item == item_index)
            .map(|def| def.id)
        else {
            continue;
        };
        let def = compilation.store().anchor(anchor);

        for property in decl.body.properties() {
            if property.name_span.end < start || property.name_span.start > end {
                continue;
            }
            let Some(slot) = def.slots.get(&property.name) else {
                continue;
            };
            let mut labels = Vec::new();
            // The inferred type is the thing you cannot see from the source.
            if let Some(value) = def.properties.get(&property.name) {
                labels.push(value.kind().to_string());
            }
            if slot.owner != anchor {
                labels.push(format!(
                    "from {}",
                    compilation.store().anchor(slot.owner).name
                ));
            } else if property.constraints.is_empty() {
                if let Some(inherited) = def.bases.iter().rev().find(|base| {
                    compilation
                        .store()
                        .anchor(**base)
                        .slots
                        .contains_key(&property.name)
                }) {
                    labels.push(format!(
                        "overrides {}",
                        compilation.store().anchor(*inherited).name
                    ));
                }
            }
            if labels.is_empty() {
                continue;
            }
            out.push(InlayHint {
                position: offset_to_position(&cursor.text, property.name_span.end),
                label: InlayHintLabel::String(format!(": {}", labels.join(", "))),
                kind: Some(InlayHintKind::TYPE),
                text_edits: None,
                tooltip: None,
                padding_left: Some(false),
                padding_right: Some(false),
                data: None,
            });
        }
    }

    for composition in &world.analysis.compositions {
        if composition.module != cursor.module {
            continue;
        }
        if composition.span.end < start || composition.span.start > end {
            continue;
        }
        out.push(InlayHint {
            position: offset_to_position(&cursor.text, composition.span.end),
            label: InlayHintLabel::String(format!(" {}", composition.hint)),
            kind: Some(InlayHintKind::PARAMETER),
            text_edits: None,
            tooltip: Some(InlayHintTooltip::String(composition.summary.clone())),
            padding_left: Some(true),
            padding_right: Some(false),
            data: None,
        });
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Semantic tokens
// ---------------------------------------------------------------------------

/// Token type indexes into [`TOKEN_TYPES`].
mod token {
    pub const ANCHOR: u32 = 0;
    pub const PROPERTY: u32 = 1;
    pub const VARIABLE: u32 = 2;
    pub const KEYWORD: u32 = 3;
    pub const COMMENT: u32 = 4;
    pub const STRING: u32 = 5;
    pub const NUMBER: u32 = 6;
    pub const OPERATOR: u32 = 7;
    pub const NAMESPACE: u32 = 8;
    pub const TYPE: u32 = 9;
}

/// Token modifier bits, in the order of [`TOKEN_MODIFIERS`].
mod modifier {
    pub const DECLARATION: u32 = 1 << 0;
    pub const DEFINITION: u32 = 1 << 1;
    pub const ABSTRACT: u32 = 1 << 2;
    pub const DEFAULT_LIBRARY: u32 = 1 << 3;
    pub const INHERITED: u32 = 1 << 4;
    pub const EXPORTED: u32 = 1 << 5;
    pub const IMPORTED: u32 = 1 << 6;
}

/// One candidate token. Where two overlap at the same start, the lower
/// priority wins: a type constraint over the anchor it names, a resolved name
/// over the raw keyword token.
struct RawToken {
    span: Span,
    kind: u32,
    modifiers: u32,
    priority: u8,
}

/// Collects the tokens the syntax tree cannot give: type constraints, and
/// everything inside an interpolation, which the tree keeps as one run of text.
struct TokenWalker<'a> {
    source: &'a str,
    out: Vec<RawToken>,
}

impl TokenWalker<'_> {
    fn push(&mut self, span: Span, kind: u32, priority: u8) {
        if !span.is_empty() && span.end <= self.source.len() {
            self.out.push(RawToken {
                span,
                kind,
                modifiers: 0,
                priority,
            });
        }
    }

    fn constraints(&mut self, constraints: &[ast::TypeConstraint]) {
        for constraint in constraints {
            self.push(constraint.span, token::TYPE, 0);
        }
    }

    fn value(&mut self, value: &ast::ValueNode) {
        if let Some(line) = &value.inline {
            self.line(line);
        }
        for item in value.inline_list.iter().flatten() {
            self.value(item);
        }
        if let Some(block) = &value.block {
            self.block(block);
        }
    }

    fn block(&mut self, block: &ast::Block) {
        for item in &block.items {
            match item {
                ast::BlockItem::Property(property) => {
                    self.constraints(&property.constraints);
                    self.value(&property.value);
                }
                ast::BlockItem::ListItem(entry) => self.value(&entry.value),
                ast::BlockItem::Merge(merge) => {
                    let marker = match merge.op {
                        ast::MergeOp::Merge => 1,
                        ast::MergeOp::Concat => 2,
                    };
                    self.push(
                        Span::new(merge.span.start, merge.span.start + marker),
                        token::OPERATOR,
                        0,
                    );
                    self.line(&merge.value);
                }
                ast::BlockItem::Prose(paragraph) => {
                    for line in &paragraph.lines {
                        self.line(line);
                    }
                }
                _ => {}
            }
        }
    }

    fn line(&mut self, line: &ast::ProseLine) {
        for segment in &line.segments {
            if let ast::ProseSegment::Interpolation(interpolation) = segment {
                let open = interpolation.sigil.prefix().len();
                let span = interpolation.span;
                self.push(Span::new(span.start, span.start + open), token::OPERATOR, 0);
                self.push(Span::new(span.end.saturating_sub(1), span.end), token::OPERATOR, 0);
                self.expr(&interpolation.expr);
            }
        }
    }

    /// The operator written between two spans, found in the source.
    fn between(&mut self, from: usize, to: usize) {
        let Some(gap) = self.source.get(from..to) else {
            return;
        };
        let trimmed = gap.trim();
        if trimmed.is_empty() {
            return;
        }
        let offset = from + (gap.len() - gap.trim_start().len());
        self.push(Span::new(offset, offset + trimmed.len()), token::OPERATOR, 0);
    }

    fn expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Quoted(_) => self.push(expr.span, token::STRING, 0),
            ExprKind::Number(_) => self.push(expr.span, token::NUMBER, 0),
            ExprKind::Bool(_)
            | ExprKind::Null
            | ExprKind::This
            | ExprKind::SelfRef
            | ExprKind::Super => self.push(expr.span, token::KEYWORD, 2),
            ExprKind::Field(base, field) => {
                self.expr(base);
                self.between(base.span.end, field.span.start);
            }
            ExprKind::Unary(_, operand) => {
                self.between(expr.span.start, operand.span.start);
                self.expr(operand);
            }
            ExprKind::Binary(_, left, right) => {
                self.expr(left);
                self.between(left.span.end, right.span.start);
                self.expr(right);
            }
            ExprKind::Ternary(condition, consequent, alternative) => {
                self.expr(condition);
                self.between(condition.span.end, consequent.span.start);
                self.expr(consequent);
                self.between(consequent.span.end, alternative.span.start);
                self.expr(alternative);
            }
            ExprKind::List(items) => {
                for item in items {
                    self.expr(item);
                }
            }
            ExprKind::Paren(inner) => self.expr(inner),
            ExprKind::Nested(sigil, inner) => {
                let open = sigil.prefix().len();
                self.push(
                    Span::new(expr.span.start, expr.span.start + open),
                    token::OPERATOR,
                    0,
                );
                self.push(
                    Span::new(expr.span.end.saturating_sub(1), expr.span.end),
                    token::OPERATOR,
                    0,
                );
                self.expr(inner);
            }
            _ => {}
        }
    }
}

pub fn semantic_tokens(world: &World, uri: &Url) -> Option<SemanticTokensResult> {
    let cursor = file_context(world, uri)?;
    let index = world.index.get(cursor.module)?;
    let compilation = cursor.compilation;
    let module = compilation.graph().get(cursor.module);

    let mut raw: Vec<RawToken> = Vec::new();

    // Syntax-level tokens the index does not carry.
    let syntax = module.parse.syntax();
    for token in syntax
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
    {
        let span = Span::new(
            usize::from(token.text_range().start()),
            usize::from(token.text_range().end()),
        );
        let kind = token.kind();
        let index_of = if kind == SyntaxKind::COMMENT {
            token::COMMENT
        } else if kind.is_keyword() {
            token::KEYWORD
        } else if kind == SyntaxKind::NUMBER {
            token::NUMBER
        } else if kind == SyntaxKind::ESCAPE_TEXT {
            token::STRING
        } else {
            continue;
        };
        raw.push(RawToken {
            span,
            kind: index_of,
            modifiers: 0,
            priority: 5,
        });
    }

    // Constraints and the insides of interpolations.
    let mut walker = TokenWalker {
        source: &cursor.text,
        out: Vec::new(),
    };
    for item in &module.ast().items {
        match item {
            Item::Anchor(decl) => walker.block(&decl.body),
            Item::Variable(decl) => {
                walker.constraints(&decl.constraints);
                walker.value(&decl.value);
            }
            _ => {}
        }
    }
    raw.extend(walker.out);

    // Semantic tokens from the resolved program.
    let store = compilation.store();
    let is_package = |module: piton_compile::ModuleId| compilation.graph().get(module).is_package();
    for occurrence in &index.occurrences {
        let definition = occurrence.role == Role::Definition;
        let declared = if definition {
            modifier::DECLARATION | modifier::DEFINITION
        } else {
            0
        };
        let (kind, modifiers) = match &occurrence.target {
            Target::Anchor(anchor) => {
                let def = store.anchor(*anchor);
                let mut modifiers = declared;
                if def.is_abstract {
                    modifiers |= modifier::ABSTRACT;
                }
                if def.exported {
                    modifiers |= modifier::EXPORTED;
                }
                if def.module != cursor.module {
                    modifiers |= modifier::IMPORTED;
                }
                if is_package(def.module) {
                    modifiers |= modifier::DEFAULT_LIBRARY;
                }
                (token::ANCHOR, modifiers)
            }
            Target::Property(anchor, name) => {
                let mut modifiers = declared;
                // A property read through `self`, `super` or an anchor whose
                // value a base supplies is an inherited value.
                if !definition
                    && store
                        .anchor(*anchor)
                        .slots
                        .get(name)
                        .is_some_and(|slot| slot.owner != *anchor)
                {
                    modifiers |= modifier::INHERITED;
                }
                (token::PROPERTY, modifiers)
            }
            Target::Key(..) => (token::PROPERTY, declared),
            Target::Variable(variable) => {
                let def = store.variable(*variable);
                let mut modifiers = declared;
                if def.exported {
                    modifiers |= modifier::EXPORTED;
                }
                if def.module != cursor.module {
                    modifiers |= modifier::IMPORTED;
                }
                (token::VARIABLE, modifiers)
            }
            Target::Keyword(_, aliased) => {
                let mut modifiers = if definition { modifier::DECLARATION } else { 0 };
                if let Some(anchor) = aliased {
                    let def = store.anchor(*anchor);
                    if def.module != cursor.module {
                        modifiers |= modifier::IMPORTED;
                    }
                    if is_package(def.module) {
                        modifiers |= modifier::DEFAULT_LIBRARY;
                    }
                }
                (token::KEYWORD, modifiers)
            }
            Target::Module(path) => (
                token::NAMESPACE,
                if path.to_string_lossy().starts_with('@') {
                    modifier::DEFAULT_LIBRARY
                } else {
                    0
                },
            ),
            Target::Unresolved(_) => continue,
        };
        raw.push(RawToken {
            span: occurrence.span,
            kind,
            modifiers,
            priority: 1,
        });
    }

    raw.sort_by_key(|token| (token.span.start, token.priority, token.span.len()));
    // Overlapping tokens confuse clients; keep the first at each start.
    let mut filtered: Vec<RawToken> = Vec::new();
    let mut cursor_offset = 0usize;
    for entry in raw {
        if entry.span.start < cursor_offset || entry.span.is_empty() {
            continue;
        }
        cursor_offset = entry.span.end;
        filtered.push(entry);
    }

    let mut data = Vec::new();
    let mut previous_line = 0u32;
    let mut previous_start = 0u32;
    for entry in filtered {
        let start = offset_to_position(&cursor.text, entry.span.start);
        let end = offset_to_position(&cursor.text, entry.span.end);
        if end.line != start.line {
            continue;
        }
        let delta_line = start.line - previous_line;
        let delta_start = if delta_line == 0 {
            start.character.saturating_sub(previous_start)
        } else {
            start.character
        };
        data.push(SemanticToken {
            delta_line,
            delta_start,
            length: end.character - start.character,
            token_type: entry.kind,
            token_modifiers_bitset: entry.modifiers,
        });
        previous_line = start.line;
        previous_start = start.character;
    }

    debug_assert!(TOKEN_TYPES.len() > token::TYPE as usize && TOKEN_MODIFIERS.len() >= 7);
    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}

// ---------------------------------------------------------------------------
// Source to output
// ---------------------------------------------------------------------------

pub fn code_lenses(world: &World, uri: &Url) -> Option<Vec<CodeLens>> {
    let cursor = file_context(world, uri)?;
    let mut lenses = Vec::new();
    let mut seen = HashSet::new();
    for mapping in &world.analysis.mappings {
        if !paths_equal(&mapping.source_file, &cursor.path) {
            continue;
        }
        // One lens per construct, not one per adapter slice of the same span.
        if !seen.insert(mapping.source_span.start) {
            continue;
        }
        let labels: Vec<&str> = world
            .analysis
            .mappings
            .iter()
            .filter(|other| {
                paths_equal(&other.source_file, &cursor.path)
                    && other.source_span.start == mapping.source_span.start
            })
            .map(|other| other.label.as_str())
            .collect();
        let title = if labels.len() <= 2 {
            labels.join(" · ")
        } else {
            format!("{} · {} · +{}", labels[0], labels[1], labels.len() - 2)
        };
        lenses.push(CodeLens {
            range: span_to_range(
                &cursor.text,
                Span::new(mapping.source_span.start, mapping.source_span.start),
            ),
            command: Some(Command {
                title,
                command: SOURCE_TO_OUTPUT.into(),
                arguments: Some(vec![serde_json::json!({
                    "path": mapping.output_path,
                    "offset": mapping.output_start,
                })]),
            }),
            data: None,
        });
    }

    // An overriding property links to the declaration it replaces.
    if let Some(compilation) = world.compilation.as_ref() {
        for def in compilation
            .store()
            .anchors
            .iter()
            .filter(|def| def.module == cursor.module)
        {
            let Item::Anchor(decl) = &compilation.graph().get(def.module).ast().items[def.item]
            else {
                continue;
            };
            for property in decl.body.properties() {
                let Some(base) = overridden(compilation, def.id, &property.name) else {
                    continue;
                };
                let Some(location) = overridden_location(world, def.id, &property.name) else {
                    continue;
                };
                lenses.push(CodeLens {
                    range: span_to_range(&cursor.text, Span::empty(property.name_span.start)),
                    command: Some(Command {
                        title: format!(
                            "overrides {}.{}",
                            compilation.store().anchor(base).name,
                            property.name
                        ),
                        command: SHOW_LOCATION.into(),
                        arguments: Some(vec![serde_json::json!({
                            "uri": location.uri,
                            "range": location.range,
                        })]),
                    }),
                    data: None,
                });
            }
        }
    }
    Some(lenses)
}

/// The command a source-to-output code lens runs: open the compiled file at
/// the slice the construct produced.
pub const SOURCE_TO_OUTPUT: &str = "piton.sourceToOutput";
/// The command an "overrides" code lens runs: open a location.
pub const SHOW_LOCATION: &str = "piton.showLocation";

/// What running a command should do: show a document, or tell the user why
/// it cannot.
#[derive(Debug, PartialEq)]
pub enum CommandOutcome {
    Show(ShowDocumentParams),
    Message(String),
}

/// `workspace/executeCommand` for the commands the server's code lenses use.
pub fn execute_command(
    _world: &World,
    command: &str,
    arguments: &[serde_json::Value],
) -> Option<CommandOutcome> {
    let argument = arguments.first()?;
    match command {
        SOURCE_TO_OUTPUT => {
            let path = PathBuf::from(argument.get("path")?.as_str()?);
            let offset = argument.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let Ok(text) = std::fs::read_to_string(&path) else {
                return Some(CommandOutcome::Message(format!(
                    "`{}` has not been compiled yet; run `piton compile` to write it",
                    path.display()
                )));
            };
            let position = offset_to_position(&text, offset);
            Some(CommandOutcome::Show(ShowDocumentParams {
                uri: path_to_url(&path)?,
                external: Some(false),
                take_focus: Some(true),
                selection: Some(Range {
                    start: position,
                    end: position,
                }),
            }))
        }
        SHOW_LOCATION => {
            let uri = Url::parse(argument.get("uri")?.as_str()?).ok()?;
            let range: Range = serde_json::from_value(argument.get("range")?.clone()).ok()?;
            Some(CommandOutcome::Show(ShowDocumentParams {
                uri,
                external: Some(false),
                take_focus: Some(true),
                selection: Some(range),
            }))
        }
        _ => None,
    }
}

pub fn document_links(world: &World, uri: &Url) -> Option<Vec<DocumentLink>> {
    let cursor = file_context(world, uri)?;
    let mut links = Vec::new();
    let mut seen = HashSet::new();
    for mapping in &world.analysis.mappings {
        if !paths_equal(&mapping.source_file, &cursor.path) {
            continue;
        }
        if !seen.insert((mapping.source_span.start, mapping.output_path.clone())) {
            continue;
        }
        let Some(target) = path_to_url(&mapping.output_path) else {
            continue;
        };
        links.push(DocumentLink {
            range: span_to_range(&cursor.text, mapping.source_span),
            target: Some(target),
            tooltip: Some(mapping.label.clone()),
            data: None,
        });
    }
    Some(links)
}

/// `piton/sourceToOutput`: which compiled slices a source position produced.
pub fn source_to_output(world: &World, params: &serde_json::Value) -> serde_json::Value {
    let Some(uri) = params
        .get("uri")
        .and_then(|value| value.as_str())
        .and_then(|text| Url::parse(text).ok())
    else {
        return serde_json::json!([]);
    };
    let line = params
        .get("line")
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as u32;
    let character = params
        .get("character")
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as u32;
    let Some(path) = url_to_path(&uri) else {
        return serde_json::json!([]);
    };
    let Some(text) = world.text(&path) else {
        return serde_json::json!([]);
    };
    let offset = position_to_offset(&text, Position { line, character });
    let hits: Vec<serde_json::Value> = world
        .analysis
        .mappings_at(&path, offset)
        .into_iter()
        .map(|mapping| {
            serde_json::json!({
                "path": mapping.output_path,
                "start": mapping.output_start,
                "end": mapping.output_end,
                "adapter": mapping.adapter,
                "label": mapping.label,
            })
        })
        .collect();
    serde_json::Value::Array(hits)
}

/// `piton/outputToSource`: which source construct produced a compiled offset.
pub fn output_to_source(world: &World, params: &serde_json::Value) -> serde_json::Value {
    let Some(path) = params.get("path").and_then(|value| value.as_str()) else {
        return serde_json::Value::Null;
    };
    let offset = params
        .get("offset")
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as usize;
    let path = PathBuf::from(path);
    let Some(mapping) = world.analysis.at_output(&path, offset) else {
        return serde_json::Value::Null;
    };
    let text = world.text(&mapping.source_file).unwrap_or_default();
    let range = span_to_range(&text, mapping.source_span);
    serde_json::json!({
        "uri": path_to_url(&mapping.source_file).map(|uri| uri.to_string()),
        "path": mapping.source_file,
        "start": mapping.source_span.start,
        "end": mapping.source_span.end,
        "line": range.start.line,
        "character": range.start.character,
        "adapter": mapping.adapter,
    })
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    left == right
        || left.to_string_lossy().replace('\\', "/") == right.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod completion_tests {
    use super::*;

    #[test]
    fn the_head_of_an_unindented_line_declares() {
        assert_eq!(
            context(""),
            Context::Declaration {
                exported: false,
                is_abstract: false
            }
        );
        assert_eq!(
            context("export "),
            Context::Declaration {
                exported: true,
                is_abstract: false
            }
        );
        assert_eq!(
            context("export abstract "),
            Context::Declaration {
                exported: true,
                is_abstract: true
            }
        );
        // The word being typed is what to complete, not something already said.
        assert_eq!(
            context("expo"),
            Context::Declaration {
                exported: false,
                is_abstract: false
            }
        );
        // Past the declaration keyword comes a name, which is the author's.
        assert_eq!(context("anchor "), Context::Nothing);
    }

    #[test]
    fn a_declaration_head_knows_its_clauses() {
        assert_eq!(context("anchor A extends "), Context::Bases);
        assert_eq!(context("anchor A extends Ba"), Context::Bases);
        // `as` names a keyword the author is inventing.
        assert_eq!(context("anchor A extends B as "), Context::Nothing);
        // `extends` inside a word is not the clause.
        assert_eq!(context("anchor Bases "), Context::Nothing);
    }

    #[test]
    fn imports_complete_by_part() {
        assert_eq!(context("use "), Context::ModulePath);
        assert_eq!(context("use ./oper"), Context::ModulePath);
        assert_eq!(context("from ./x"), Context::ModulePath);
        assert_eq!(context("from ./x "), Context::ImportDirection);
        assert_eq!(context("from ./x imp"), Context::ImportDirection);
        assert_eq!(
            context("from ./x import "),
            Context::ImportName { module: "./x" }
        );
        assert_eq!(
            context("from ./x export Na"),
            Context::ImportName { module: "./x" }
        );
    }

    #[test]
    fn a_key_takes_the_first_colon_and_the_constraints_take_the_rest() {
        assert_eq!(context("    ti"), Context::Body);
        // Nothing written after the colon is nothing to complete on.
        assert_eq!(context("    title:"), Context::Nothing);
        assert_eq!(context("    title: "), Context::Nothing);
        assert_eq!(
            context("    title: t"),
            Context::Value { key: Some("title") }
        );
        assert_eq!(context("    name:: "), Context::TypeConstraint);
        assert_eq!(context("    name:: str"), Context::TypeConstraint);
        assert_eq!(context("    name:: string: "), Context::Nothing);
        assert_eq!(
            context("    name:: string: n"),
            Context::Value { key: Some("name") }
        );
        // Constraints chain, and the state flips on every colon.
        assert_eq!(context("    p:: dictionary:: "), Context::TypeConstraint);
        assert_eq!(
            context("    p:: dictionary:: null: nu"),
            Context::Value { key: Some("p") }
        );
    }

    #[test]
    fn a_list_item_holds_a_value() {
        assert_eq!(context("    - t"), Context::Value { key: None });
        assert_eq!(context("    ++ t"), Context::Value { key: None });
        // An empty item proposes nothing, like an empty value.
        assert_eq!(context("    - "), Context::Nothing);
    }

    #[test]
    fn an_interpolation_knows_its_sigil() {
        assert_eq!(
            context("    a: {x"),
            Context::Expression {
                sigil: "{",
                member_of: None
            }
        );
        assert_eq!(
            context("    a: @{"),
            Context::Expression {
                sigil: "@{",
                member_of: None
            }
        );
        assert_eq!(
            context("    a: ${self.na"),
            Context::Expression {
                sigil: "${",
                member_of: Some("self")
            }
        );
        assert_eq!(
            context("    a: {A.b.c"),
            Context::Expression {
                sigil: "{",
                member_of: Some("A.b")
            }
        );
        // A closed interpolation is behind the cursor, not around it.
        assert_eq!(context("    a: {x} and "), Context::Nothing);
    }

    #[test]
    fn a_string_has_nothing_to_complete() {
        // The specification is explicit about this one: a `.` typed in the
        // middle of a string has nothing to complete on, because a string has
        // no members.
        assert_eq!(context(r#"    a: ${"a sentence."#), Context::Nothing);
        assert_eq!(
            context(r#"    a: ${"in a string" + "#),
            Context::Expression {
                sigil: "${",
                member_of: None
            }
        );
        assert!(in_string(r#""open"#));
        assert!(!in_string(r#""closed""#));
        assert!(in_string(r#""an escaped \" quote"#));
    }

    #[test]
    fn prose_is_written_rather_than_completed() {
        // A value that has become a sentence is not a name being completed.
        assert_eq!(context("    a: the quick brown"), Context::Nothing);
        assert_eq!(context("    a: The end."), Context::Nothing);
        assert_eq!(context("    a: tru"), Context::Value { key: Some("a") });
        // And neither is a line of prose inside a body.
        assert_eq!(context("    the quick brown"), Context::Nothing);
    }

    #[test]
    fn a_comment_has_nothing_to_complete() {
        assert_eq!(context("    // a note"), Context::Nothing);
        assert_eq!(context("a: see https://example.com/"), Context::Nothing);
        // A `//` that does not follow whitespace is not a comment, so the line
        // is still a value -- and a URL is prose, so there is nothing to offer.
        assert!(!in_comment("a: see https://example.com"));
    }

    #[test]
    fn escape_blocks_are_not_piton() {
        // A code fence is just text, so it isn't verbatim.
        let fenced = "a:\n    ```json\n    { \"k\": 1 }\n";
        assert!(!verbatim_at(fenced, fenced.len()));

        let escaped = "a:\n\\\\\\\\\nliteral {text}\n";
        assert!(verbatim_at(escaped, escaped.len()));
        let unescaped = "a:\n\\\\\\\\\nliteral\n\\\\\\\\\n";
        assert!(!verbatim_at(unescaped, unescaped.len()));
    }
}

// ---------------------------------------------------------------------------
// Hierarchy
//
// Inheritance is the structure of a specbase: an abstract anchor declares a
// shape, keywords are inheritance spelled as syntax, and what a concrete anchor
// resolves to is the whole chain above it flattened. Two navigations follow
// from that, and they answer opposite questions.
//
// `implementation` goes down one step: from a base to everything that extends
// it. Go-to-definition already goes up, and without this the relationship is
// only traversable in one direction -- which is the wrong one when the question
// is "who uses this shape?".
//
// The type hierarchy goes both ways and keeps going, so an editor can show the
// tree rather than one step of it.
// ---------------------------------------------------------------------------

/// The anchor a cursor is on, however it was named.
///
/// A keyword *is* the anchor it aliases, so landing on `skill` and landing on
/// `Skill` have to give the same answer.
fn anchor_at(world: &World, uri: &Url, position: Position) -> Option<AnchorId> {
    let cursor = cursor(world, uri, position)?;
    let index = world.index.get(cursor.module)?;
    match index.at(cursor.offset)?.target {
        Target::Anchor(anchor) => Some(anchor),
        Target::Keyword(_, anchor) => anchor,
        Target::Property(anchor, _) => Some(anchor),
        _ => None,
    }
}

/// Every anchor that names `base` among its bases, directly.
fn derived(compilation: &Compilation, base: AnchorId) -> Vec<AnchorId> {
    let mut found: Vec<AnchorId> = compilation
        .store()
        .anchors
        .iter()
        .filter(|def| def.bases.contains(&base))
        .map(|def| def.id)
        .collect();
    found.sort_by_key(|id| compilation.store().anchor(*id).name.clone());
    found
}

/// `textDocument/implementation`.
///
/// From a base anchor: what extends it, then what composes it with `{X}`.
/// From a property: the declarations in descendants that override it -- the
/// other direction of go-to-definition on an overriding property.
pub fn implementations(
    world: &World,
    uri: &Url,
    position: Position,
) -> Option<request::GotoImplementationResponse> {
    let compilation = world.compilation.as_ref()?;
    let cursor = cursor(world, uri, position)?;
    let index = world.index.get(cursor.module)?;
    let occurrence = index.at(cursor.offset)?;

    let locations: Vec<Location> = if let Target::Property(anchor, name) = &occurrence.target {
        let store = compilation.store();
        let owner = store
            .anchor(*anchor)
            .slots
            .get(name)
            .map(|slot| slot.owner)
            .unwrap_or(*anchor);
        let mut overriding: Vec<AnchorId> = store
            .anchors
            .iter()
            .filter(|def| {
                def.id != owner
                    && store.inherits_from(def.id, owner)
                    && def.slots.get(name).is_some_and(|slot| slot.owner == def.id)
            })
            .map(|def| def.id)
            .collect();
        overriding.sort_by_key(|id| store.anchor(*id).name.clone());
        overriding
            .into_iter()
            .filter_map(|id| definition_location(world, &Target::Property(id, name.clone())))
            .collect()
    } else {
        let anchor = anchor_at(world, uri, position)?;
        let mut related = derived(compilation, anchor);
        for composer in world.analysis.composers.get(&anchor).into_iter().flatten() {
            if !related.contains(composer) {
                related.push(*composer);
            }
        }
        related
            .into_iter()
            .filter_map(|id| definition_location(world, &Target::Anchor(id)))
            .collect()
    };
    (!locations.is_empty()).then_some(request::GotoImplementationResponse::Array(locations))
}

/// How a hierarchy item relates to the one it was asked from.
#[derive(Clone, Copy)]
enum Relation {
    Itself,
    Inheritance,
    /// It composes the other (`{Other}` in one of its values).
    Composes,
    /// The other composes it.
    ComposedInto,
}

/// Builds the hierarchy entry for one anchor.
fn hierarchy_item(world: &World, anchor: AnchorId, relation: Relation) -> Option<TypeHierarchyItem> {
    let compilation = world.compilation.as_ref()?;
    let def = compilation.store().anchor(anchor);
    let location = definition_location(world, &Target::Anchor(anchor))?;
    let mut detail = def.keyword.clone();
    if def.is_abstract {
        detail = format!("abstract {detail}");
    }
    if let Some(alias) = &def.alias {
        detail.push_str(&format!(" as {alias}"));
    }
    match relation {
        Relation::Composes => detail.push_str(" \u{00b7} composes it"),
        Relation::ComposedInto => detail.push_str(" \u{00b7} composed into it"),
        Relation::Itself | Relation::Inheritance => {}
    }
    if let Some(described) = summary(compilation, anchor) {
        detail.push_str(" \u{2014} ");
        detail.push_str(&described);
    }
    Some(TypeHierarchyItem {
        name: def.name.clone(),
        kind: if def.is_abstract {
            SymbolKind::INTERFACE
        } else {
            SymbolKind::STRUCT
        },
        tags: None,
        detail: Some(detail),
        uri: location.uri,
        range: location.range,
        selection_range: location.range,
        // The anchor's own identity, so a later supertypes/subtypes call does
        // not have to find it again by position in a file that may since have
        // been edited.
        data: Some(serde_json::json!({ "anchor": anchor.0 })),
    })
}

/// Reads the anchor identity back out of a hierarchy item.
fn hierarchy_anchor(world: &World, item: &TypeHierarchyItem) -> Option<AnchorId> {
    let compilation = world.compilation.as_ref()?;
    let id = item
        .data
        .as_ref()
        .and_then(|data| data.get("anchor"))
        .and_then(serde_json::Value::as_u64)
        .map(|id| AnchorId(id as u32))
        .filter(|id| (id.0 as usize) < compilation.store().anchors.len());
    // A stale id, from an item prepared before a recompilation, falls back to
    // the name -- which is what the editor is really showing.
    id.filter(|id| compilation.store().anchor(*id).name == item.name)
        .or_else(|| compilation.find_anchor(&item.name))
}

/// `textDocument/prepareTypeHierarchy`.
pub fn prepare_type_hierarchy(
    world: &World,
    uri: &Url,
    position: Position,
) -> Option<Vec<TypeHierarchyItem>> {
    let anchor = anchor_at(world, uri, position)?;
    hierarchy_item(world, anchor, Relation::Itself).map(|item| vec![item])
}

/// `typeHierarchy/supertypes`: the bases an anchor extends, in declaration
/// order (the order collisions resolve in), then the anchors it composes.
pub fn type_hierarchy_supertypes(
    world: &World,
    item: &TypeHierarchyItem,
) -> Option<Vec<TypeHierarchyItem>> {
    let compilation = world.compilation.as_ref()?;
    let anchor = hierarchy_anchor(world, item)?;
    let mut items: Vec<TypeHierarchyItem> = compilation
        .store()
        .anchor(anchor)
        .bases
        .iter()
        .filter_map(|base| hierarchy_item(world, *base, Relation::Inheritance))
        .collect();
    for component in world.analysis.components.get(&anchor).into_iter().flatten() {
        items.extend(hierarchy_item(world, *component, Relation::ComposedInto));
    }
    Some(items)
}

/// `typeHierarchy/subtypes`: everything that extends this anchor, then
/// everything that composes it.
pub fn type_hierarchy_subtypes(
    world: &World,
    item: &TypeHierarchyItem,
) -> Option<Vec<TypeHierarchyItem>> {
    let compilation = world.compilation.as_ref()?;
    let anchor = hierarchy_anchor(world, item)?;
    let mut items: Vec<TypeHierarchyItem> = derived(compilation, anchor)
        .into_iter()
        .filter_map(|id| hierarchy_item(world, id, Relation::Inheritance))
        .collect();
    for composer in world.analysis.composers.get(&anchor).into_iter().flatten() {
        items.extend(hierarchy_item(world, *composer, Relation::Composes));
    }
    Some(items)
}

// ---------------------------------------------------------------------------
// Compiled output preview
// ---------------------------------------------------------------------------

/// `piton/preview`: a file, or the anchor under a position, as it compiles.
///
/// Parameters: `uri`, an optional `position` (`{line, character}`) naming an
/// anchor to preview on its own, and an optional `renderer` -- `json`, `yaml`,
/// `markdown`, or a Belay target id such as `claude` -- defaulting to the
/// renderer the project is configured with. The answer carries the renderer
/// used, the file the output is written to, and the text.
pub fn preview(world: &World, params: &serde_json::Value) -> serde_json::Value {
    let failure = |message: &str| serde_json::json!({ "error": message });
    let Some(uri) = params
        .get("uri")
        .and_then(|value| value.as_str())
        .and_then(|text| Url::parse(text).ok())
    else {
        return failure("a `uri` is required");
    };
    let position = params
        .get("position")
        .and_then(|value| serde_json::from_value::<Position>(value.clone()).ok());
    let Some(cursor) = cursor(world, &uri, position.unwrap_or_default()) else {
        return failure("that file is not part of the compiled workspace");
    };
    let compilation = cursor.compilation;
    let anchor = position.and_then(|position| anchor_at(world, &uri, position));
    let requested = params.get("renderer").and_then(|value| value.as_str());

    let adapter = match requested {
        None => Some(world.output.renderer),
        Some(name) => name.parse::<Adapter>().ok(),
    };
    if let Some(adapter) = adapter {
        let declarations = match anchor {
            Some(anchor) => {
                let def = compilation.store().anchor(anchor);
                let mut one = piton_core::Properties::new();
                one.insert(def.name.clone(), piton_core::Value::Anchor(anchor));
                one
            }
            None => crate::analysis::file_declarations(compilation, cursor.module),
        };
        let output = world
            .output
            .output_with(&compilation.project, &cursor.path, adapter);
        let directory = output.parent().unwrap_or(Path::new("."));
        let context = piton_emit::MarkdownContext {
            from_directory: directory,
            source_root: &compilation.project.source_root,
        };
        let text = piton_emit::render(adapter, &declarations, compilation, context);
        return serde_json::json!({
            "renderer": adapter.as_str(),
            "path": output,
            "text": text,
        });
    }

    // Anything else names a Belay target.
    let target = requested.unwrap_or_default();
    let Some(config) = compilation.project.belay() else {
        return failure(&format!(
            "`{target}` is not a renderer (json, yaml, markdown), and the project has no Belay configuration"
        ));
    };
    let plan = piton_belay::plan(compilation, config);
    let files: Vec<&piton_belay::OutputFile> = plan
        .files
        .iter()
        .filter(|file| file.target == target)
        .filter(|file| match anchor {
            // `origin` names the anchor a file came from, or every anchor a
            // combined file carries, comma separated.
            Some(anchor) => {
                let name = &compilation.store().anchor(anchor).name;
                file.origin.split(',').any(|origin| origin.trim() == name)
                    && file.sources.iter().any(|source| paths_equal(source, &cursor.path))
            }
            None => file.sources.iter().any(|source| paths_equal(source, &cursor.path)),
        })
        .collect();
    if files.is_empty() {
        return failure(&format!(
            "the `{target}` target produces nothing from this construct"
        ));
    }
    let text = files
        .iter()
        .map(|file| format!("<!-- {} -->\n{}", file.path.display(), file.contents))
        .collect::<Vec<_>>()
        .join("\n\n");
    serde_json::json!({
        "renderer": target,
        "path": compilation.project.root.join(&files[0].path),
        "paths": files.iter().map(|file| compilation.project.root.join(&file.path)).collect::<Vec<_>>(),
        "text": text,
    })
}
