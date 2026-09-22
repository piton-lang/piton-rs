//! Editor feature implementations.
//!
//! These all work from the same two inputs: the resolved compilation and the
//! occurrence index. Answering from the resolved program is what lets an editor
//! show where an inherited value actually came from.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use piton_compile::{reach, Compilation, Symbol};
use piton_core::{AnchorId, Diagnostic, Severity, Span};
use piton_emit::markdown;
use piton_syntax::ast::{self, Item};
use piton_syntax::{format, SyntaxKind};
use tower_lsp::lsp_types::*;

use crate::convert::{offset_to_position, path_to_url, position_to_offset, span_to_range, url_to_path};
use crate::index::{Role, Target};
use crate::world::World;
use crate::{TOKEN_MODIFIERS, TOKEN_TYPES};

/// Converts a compiler diagnostic into the editor's form.
pub fn to_lsp_diagnostic(diagnostic: &Diagnostic, text: &str) -> tower_lsp::lsp_types::Diagnostic {
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
        related_information: (!diagnostic.labels.is_empty()).then(|| {
            diagnostic
                .labels
                .iter()
                .filter_map(|label| {
                    Some(DiagnosticRelatedInformation {
                        location: Location {
                            uri: path_to_url(&label.file)?,
                            range: Range::default(),
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
    let occurrence = index.at(cursor.offset)?;
    let markdown = describe(cursor.compilation, &occurrence.target)?;
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: markdown,
        }),
        range: Some(span_to_range(&cursor.text, occurrence.span)),
    })
}

/// Builds the documentation shown for a target: what it is, where it came
/// from, its inheritance chain, and the value it resolves to.
fn describe(compilation: &Compilation, target: &Target) -> Option<String> {
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

            if !def.properties.is_empty() {
                // The compiled interpretation is often the real question.
                let context = markdown::Context {
                    anchors: compilation,
                    links: &markdown::NoLinks,
                };
                let rendered = markdown::render_map(&def.properties, 0, &context);
                out.push_str("Resolves to:\n\n```\n");
                out.push_str(&truncate(&rendered, 2000));
                out.push_str("\n```");
            }
            Some(out)
        }
        Target::Variable(variable) => {
            let def = compilation.store().variable(*variable);
            let mut out = format!("```piton\n{}{}\n```\n\n", if def.exported { "export " } else { "" }, def.name);
            if !def.constraints.is_empty() {
                let names: Vec<String> = def
                    .constraints
                    .iter()
                    .map(piton_compile::eval::constraint_label)
                    .collect();
                out.push_str(&format!("Constrained to: {}\n\n", names.join(" or ")));
            }
            if let Some(value) = &def.value {
                let context = markdown::Context {
                    anchors: compilation,
                    links: &markdown::NoLinks,
                };
                out.push_str("Resolves to:\n\n```\n");
                out.push_str(&truncate(&markdown::body(value, 1, &context), 2000));
                out.push_str("\n```");
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
                let context = markdown::Context {
                    anchors: compilation,
                    links: &markdown::NoLinks,
                };
                out.push_str("Resolves to:\n\n```\n");
                out.push_str(&truncate(&markdown::body(value, 1, &context), 2000));
                out.push_str("\n```");
            }
            Some(out)
        }
        Target::Keyword(keyword, anchor) => {
            let mut out = format!("Keyword `{keyword}`\n\n");
            match anchor {
                Some(anchor) => {
                    let def = compilation.store().anchor(*anchor);
                    out.push_str(&format!(
                        "Shorthand for `extends {}`, declared in `{}`.\n\nA user keyword contributes the left-most base, so anything further right in an `extends` list wins a collision.",
                        def.name,
                        compilation.anchor_module_path(*anchor).display()
                    ));
                }
                None => out.push_str("Not in scope. Add a `use` declaration for the module that exports it."),
            }
            Some(out)
        }
        Target::Module(path) => Some(format!("Module `{}`", path.display())),
        Target::Unresolved(name) => Some(format!("`{name}` is not in scope.")),
    }
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

pub fn definition(world: &World, uri: &Url, position: Position) -> Option<GotoDefinitionResponse> {
    let cursor = cursor(world, uri, position)?;
    let index = world.index.get(cursor.module)?;
    let occurrence = index.at(cursor.offset)?;

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
    module_path_from(&world.project.source_root, from, &written.to_string_lossy())
}

/// Resolves the text of a module path against the file that wrote it.
fn module_path_from(source_root: &Path, from: &Path, written: &str) -> Option<PathBuf> {
    let context = piton_compile::module::ResolutionContext {
        from_directory: from.parent()?,
        source_root,
    };
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
        a.uri
            .as_str()
            .cmp(b.uri.as_str())
            .then((a.range.start.line, a.range.start.character).cmp(&(
                b.range.start.line,
                b.range.start.character,
            )))
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
    for item in &module.ast().items {
        match item {
            Item::Anchor(decl) => {
                let children: Vec<DocumentSymbol> = decl
                    .body
                    .properties()
                    .map(|property| symbol(
                        &property.name,
                        SymbolKind::PROPERTY,
                        property.span,
                        property.name_span,
                        text,
                        None,
                        Vec::new(),
                    ))
                    .collect();
                let detail = if decl.is_abstract {
                    Some(format!("abstract {}", decl.keyword))
                } else {
                    Some(decl.keyword.clone())
                };
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
            container_name: Some(def.keyword.clone()),
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
            let key = key.or_else(|| enclosing_key(&cursor.text, line_start));
            values(&cursor, key)
        }
        Context::Body => body(&cursor),
    };

    Some(CompletionResponse::Array(items))
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
    if let Some(markdown) = describe(compilation, &target) {
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

/// Whether the line beginning at `line_start` sits inside a fenced block or an
/// escape block.
///
/// Both are verbatim: their contents are not Piton, so a `{` inside one opens
/// nothing and a `//` starts no comment. Both are opened and closed by a line
/// of their own, so counting the delimiters above the cursor answers it.
fn verbatim_at(text: &str, line_start: usize) -> bool {
    let mut fence: Option<usize> = None;
    let mut escape: Option<usize> = None;
    for line in text[..line_start].lines() {
        let trimmed = line.trim();
        if escape.is_none() {
            let backticks = trimmed.chars().take_while(|c| *c == '`').count();
            if backticks >= 3 {
                // Any run of three or more opens a fence; it closes on a run at
                // least as long.
                fence = match fence {
                    Some(open) if backticks >= open => None,
                    other => other.or(Some(backticks)),
                };
                continue;
            }
        }
        if fence.is_some() {
            continue;
        }
        if !trimmed.is_empty() && trimmed.chars().all(|c| c == '\\') {
            // An escape block closes on a run of the same length as the one
            // that opened it, which is what lets a block contain a shorter run.
            escape = match escape {
                Some(open) if trimmed.len() == open => None,
                other => other.or(Some(trimmed.len())),
            };
        }
    }
    fence.is_some() || escape.is_some()
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
    match (word_position(trimmed, "extends"), word_position(trimmed, "as")) {
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
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
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
    if is_prose(written) {
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
fn enclosing_key(text: &str, line_start: usize) -> Option<&str> {
    let indent = indent_width(&text[line_start..]);
    for line in text[..line_start].lines().rev() {
        if line.trim().is_empty() {
            continue;
        }
        if indent_width(line) >= indent {
            continue;
        }
        // The first less-indented line is the one this belongs to, whether or
        // not it turns out to be a key.
        return split_key(line.trim_start()).map(|(key, _)| key);
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

    for package in [
        piton_compile::prelude::CONFIG_PACKAGE,
        piton_compile::prelude::BELAY_PACKAGE,
    ] {
        offer(
            package.to_string(),
            Some("bundled package".into()),
            &mut items,
        );
    }

    let mut files = sources_under(source_root);
    // A module already in the graph may sit outside the source root -- a
    // tethered package does -- and is worth offering even so.
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
        let written = import_path(&cursor.path, &file, source_root);
        let detail = file
            .strip_prefix(source_root)
            .unwrap_or(&file)
            .to_string_lossy()
            .to_string();
        offer(written, Some(detail), &mut items);
    }
    items
}

/// Every `.pi` file under a directory.
///
/// Bounded, because this runs on a keystroke: a source root with more files
/// than anyone would scroll through is a root worth not walking to the end of.
fn sources_under(root: &Path) -> Vec<PathBuf> {
    const LIMIT: usize = 2000;
    let skip = ["target", ".git", "node_modules", ".piton"];

    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if found.len() >= LIMIT {
                return found;
            }
            let path = entry.path();
            let name = entry.file_name();
            if path.is_dir() {
                if !skip.contains(&name.to_string_lossy().as_ref()) {
                    stack.push(path);
                }
            } else if path
                .extension()
                .is_some_and(|extension| extension == piton_syntax::language::EXTENSION)
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// The names a module exports, for `from <path> import ...`.
fn import_names(cursor: &Cursor<'_>, written: &str) -> Vec<CompletionItem> {
    let compilation = cursor.compilation;

    // A path that names no module exports nothing, and saying otherwise would
    // be offering names that do not exist.
    let Some(module) =
        module_path_from(&compilation.project.source_root, &cursor.path, written)
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

    let mut items = Vec::new();
    for module in compilation.graph().iter() {
        if module.id == cursor.module {
            continue;
        }
        let written = import_path(
            &cursor.path,
            &module.path,
            &compilation.project.source_root,
        );
        for name in compilation.resolution.exported_names(module.id) {
            if items.len() >= LIMIT {
                return items;
            }
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
            items.push(item);
        }
    }
    items
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
fn expression(
    cursor: &Cursor<'_>,
    sigil: &str,
    member_of: Option<&str>,
) -> Vec<CompletionItem> {
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

    let anchor = match head {
        "self" | "this" => enclosing_anchor(cursor),
        "super" => enclosing_anchor(cursor)
            .and_then(|anchor| compilation.store().anchor(anchor).bases.first().copied()),
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
    let Some(mut value) = def.properties.get(rest[0]) else {
        return Vec::new();
    };
    for segment in &rest[1..] {
        let piton_core::Value::Dict(map) = value else {
            return Vec::new();
        };
        let Some(next) = map.get(*segment) else {
            return Vec::new();
        };
        value = next;
    }
    let piton_core::Value::Dict(map) = value else {
        return Vec::new();
    };
    map.iter()
        .map(|(name, nested)| CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some(nested.kind().to_string()),
            ..Default::default()
        })
        .collect()
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
                detail: (slot.owner != anchor)
                    .then(|| format!("inherited from {}", owner.name)),
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
        // Nothing says what belongs here, so offer what is always legal: a
        // literal, or a reference to something in scope.
        let mut items = literals();
        items.extend(anchors(cursor, None, "{"));
        return items;
    };

    let mut items = Vec::new();
    for constraint in &slot.constraints {
        match &constraint.name {
            ast::TypeName::Boolean => items.extend(constants(&["true", "false"])),
            ast::TypeName::Null => items.extend(constants(&["null"])),
            ast::TypeName::Anchor | ast::TypeName::Complex => {
                items.extend(anchors(cursor, None, "{"))
            }
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
            ast::TypeName::Any | ast::TypeName::Simple => {
                items.extend(literals());
                items.extend(anchors(cursor, None, "{"));
            }
            // A string, a number, a list or a dictionary is written rather
            // than chosen, and guessing at its contents would be noise.
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

/// The keys that belong in the anchor body the cursor is inside.
fn body(cursor: &Cursor<'_>) -> Vec<CompletionItem> {
    let compilation = cursor.compilation;
    let Some(anchor) = enclosing_anchor(cursor) else {
        return Vec::new();
    };
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
fn symbol_completion(
    compilation: &Compilation,
    name: &str,
    symbol: Symbol,
) -> CompletionItem {
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
    let formatted = format::format(&text, &path);
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
    let text = text.strip_suffix("/index").map(str::to_string).unwrap_or(text);
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
// Inlay hints and signature help
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
                if let Some(inherited) = def
                    .bases
                    .iter()
                    .rev()
                    .find(|base| compilation.store().anchor(**base).slots.contains_key(&property.name))
                {
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
    Some(out)
}

pub fn signature_help(world: &World, uri: &Url, position: Position) -> Option<SignatureHelp> {
    let cursor = cursor(world, uri, position)?;
    let anchor = enclosing_anchor(&cursor)?;
    let def = cursor.compilation.store().anchor(anchor);

    // Piton has no functions; the structured input a construct expects is its
    // set of properties, so that is what the editor shows.
    let parameters: Vec<ParameterInformation> = def
        .slots
        .iter()
        .map(|(name, slot)| {
            let constraints: Vec<String> = slot
                .constraints
                .iter()
                .map(piton_compile::eval::constraint_label)
                .collect();
            let label = if constraints.is_empty() {
                name.clone()
            } else {
                format!("{name}:: {}", constraints.join(":: "))
            };
            ParameterInformation {
                label: ParameterLabel::Simple(label),
                documentation: (!slot.has_value)
                    .then(|| Documentation::String("required".into())),
            }
        })
        .collect();

    Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label: format!("{} {}", def.keyword, def.name),
            documentation: Some(Documentation::String(format!(
                "{} {}",
                def.slots.len(),
                if def.slots.len() == 1 {
                    "property"
                } else {
                    "properties"
                }
            ))),
            parameters: Some(parameters),
            active_parameter: None,
        }],
        active_signature: Some(0),
        active_parameter: None,
    })
}

// ---------------------------------------------------------------------------
// Semantic tokens
// ---------------------------------------------------------------------------

pub fn semantic_tokens(world: &World, uri: &Url) -> Option<SemanticTokensResult> {
    let cursor = file_context(world, uri)?;
    let index = world.index.get(cursor.module)?;
    let compilation = cursor.compilation;

    let mut raw: Vec<(Span, u32, u32)> = Vec::new();

    // Syntax-level tokens the index does not carry.
    let syntax = compilation.graph().get(cursor.module).parse.syntax();
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
            4
        } else if kind.is_keyword() {
            3
        } else if kind == SyntaxKind::NUMBER {
            6
        } else if kind == SyntaxKind::FENCE_TEXT || kind == SyntaxKind::FENCE_MARK {
            5
        } else {
            continue;
        };
        raw.push((span, index_of, 0));
    }

    // Semantic tokens from the resolved program.
    for occurrence in &index.occurrences {
        let (kind, modifiers) = match &occurrence.target {
            Target::Anchor(anchor) => {
                let def = compilation.store().anchor(*anchor);
                let mut modifiers = 0u32;
                if occurrence.role == Role::Definition {
                    modifiers |= 1 << 1;
                }
                if def.is_abstract {
                    modifiers |= 1 << 2;
                }
                (0u32, modifiers)
            }
            Target::Property(..) => (
                1,
                if occurrence.role == Role::Definition {
                    1 << 0
                } else {
                    0
                },
            ),
            Target::Variable(_) => (2, 0),
            Target::Keyword(..) => (3, 0),
            Target::Module(_) => (8, 0),
            Target::Unresolved(_) => continue,
        };
        raw.push((occurrence.span, kind, modifiers));
    }

    raw.sort_by_key(|(span, _, _)| (span.start, span.len()));
    // Overlapping tokens confuse clients; keep the first at each start.
    let mut filtered: Vec<(Span, u32, u32)> = Vec::new();
    let mut cursor_offset = 0usize;
    for entry in raw {
        if entry.0.start < cursor_offset || entry.0.is_empty() {
            continue;
        }
        cursor_offset = entry.0.end;
        filtered.push(entry);
    }

    let mut data = Vec::new();
    let mut previous_line = 0u32;
    let mut previous_start = 0u32;
    for (span, kind, modifiers) in filtered {
        let start = offset_to_position(&cursor.text, span.start);
        let end = offset_to_position(&cursor.text, span.end);
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
            token_type: kind,
            token_modifiers_bitset: modifiers,
        });
        previous_line = start.line;
        previous_start = start.character;
    }

    debug_assert!(TOKEN_TYPES.len() >= 9 && !TOKEN_MODIFIERS.is_empty());
    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}

/// Exposed for tests: the anchors nothing reaches from the entry point.
pub fn unreachable_anchors(compilation: &Compilation) -> Vec<String> {
    reach::from_entry(compilation)
        .unreachable
        .into_iter()
        .map(|anchor| compilation.store().anchor(anchor).name.clone())
        .collect()
}

/// Exposed for tests: the AST item at an offset, for structural queries.
pub fn item_at(ast: &ast::SourceFile, offset: usize) -> Option<&Item> {
    ast.items.iter().find(|item| item.span().contains(offset))
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
        assert_eq!(context("    title:"), Context::Value { key: Some("title") });
        assert_eq!(context("    title: "), Context::Value { key: Some("title") });
        assert_eq!(context("    name:: "), Context::TypeConstraint);
        assert_eq!(context("    name:: str"), Context::TypeConstraint);
        assert_eq!(
            context("    name:: string: "),
            Context::Value { key: Some("name") }
        );
        // Constraints chain, and the state flips on every colon.
        assert_eq!(context("    p:: dictionary:: "), Context::TypeConstraint);
        assert_eq!(
            context("    p:: dictionary:: null: "),
            Context::Value { key: Some("p") }
        );
    }

    #[test]
    fn a_list_item_holds_a_value() {
        assert_eq!(context("    - "), Context::Value { key: None });
        assert_eq!(context("    ++ "), Context::Value { key: None });
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
        assert_eq!(
            context("    a: {x} and "),
            Context::Nothing
        );
    }

    #[test]
    fn a_string_has_nothing_to_complete() {
        // The specification is explicit about this one: a `.` typed in the
        // middle of a string has nothing to complete on, because a string has
        // no members.
        assert_eq!(context(r#"    a: ${"a sentence."#), Context::Nothing);
        assert_eq!(context(r#"    a: ${"in a string" + "#), Context::Expression {
            sigil: "${",
            member_of: None
        });
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
    fn verbatim_blocks_are_not_piton() {
        let fenced = "a:\n    ```json\n    { \"k\": 1 }\n";
        assert!(verbatim_at(fenced, fenced.len()));
        let closed = "a:\n    ```json\n    {}\n    ```\n";
        assert!(!verbatim_at(closed, closed.len()));

        let escaped = "a:\n\\\\\\\\\nliteral {text}\n";
        assert!(verbatim_at(escaped, escaped.len()));
        let unescaped = "a:\n\\\\\\\\\nliteral\n\\\\\\\\\n";
        assert!(!verbatim_at(unescaped, unescaped.len()));
    }
}
