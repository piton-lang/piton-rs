//! Editor feature implementations.
//!
//! These all work from the same two inputs: the resolved compilation and the
//! occurrence index. Answering from the resolved program is what lets an editor
//! show where an inherited value actually came from.

use std::collections::HashMap;
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
    let directory = from.parent()?;
    let context = piton_compile::module::ResolutionContext {
        from_directory: directory,
        source_root: &world.project.source_root,
    };
    piton_compile::module::resolve(&written.to_string_lossy(), &context).ok()
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

pub fn completion(world: &World, uri: &Url, position: Position) -> Option<CompletionResponse> {
    let cursor = cursor(world, uri, position)?;
    let compilation = cursor.compilation;
    let scope = compilation.resolution.scope(cursor.module);
    let line_start = cursor.text[..cursor.offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let prefix = &cursor.text[line_start..cursor.offset];
    let mut items = Vec::new();

    // Inside an interpolation, only values make sense.
    let in_expression = prefix.rfind(['{']).is_some_and(|open| {
        !prefix[open..].contains('}')
    });

    if in_expression {
        for (name, symbol) in &scope.declarations {
            items.push(value_completion(compilation, name, *symbol));
        }
        for name in compilation
            .resolution
            .scope(cursor.module)
            .keywords
            .keys()
        {
            let _ = name;
        }
        for keyword in ["this", "self", "super", "true", "false", "null"] {
            items.push(CompletionItem {
                label: keyword.into(),
                kind: Some(CompletionItemKind::KEYWORD),
                ..Default::default()
            });
        }
        return Some(CompletionResponse::Array(items));
    }

    let indented = prefix.starts_with(' ') || prefix.starts_with('\t');
    if !indented {
        // At the top level, declarations and imports are what belong.
        for keyword in ["anchor", "abstract anchor", "export", "use", "from"] {
            items.push(CompletionItem {
                label: keyword.into(),
                kind: Some(CompletionItemKind::KEYWORD),
                ..Default::default()
            });
        }
        for (keyword, anchor) in &scope.keywords {
            items.push(CompletionItem {
                label: keyword.clone(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some(format!(
                    "extends {}",
                    compilation.store().anchor(*anchor).name
                )),
                ..Default::default()
            });
        }
        return Some(CompletionResponse::Array(items));
    }

    // Inside an anchor body, suggest the properties that anchor can carry,
    // including the ones it inherits.
    if let Some(anchor) = enclosing_anchor(compilation, cursor.module, cursor.offset) {
        let def = compilation.store().anchor(anchor);
        for (name, slot) in &def.slots {
            let owner = compilation.store().anchor(slot.owner);
            let detail = if slot.owner == anchor {
                None
            } else {
                Some(format!("inherited from {}", owner.name))
            };
            items.push(CompletionItem {
                label: name.clone(),
                kind: Some(CompletionItemKind::PROPERTY),
                detail,
                insert_text: Some(format!("{name}: ")),
                documentation: (!slot.has_value).then(|| {
                    Documentation::String("required by an abstract declaration".into())
                }),
                ..Default::default()
            });
        }
    }
    Some(CompletionResponse::Array(items))
}

fn value_completion(compilation: &Compilation, name: &str, symbol: Symbol) -> CompletionItem {
    match symbol {
        Symbol::Anchor(anchor) => {
            let def = compilation.store().anchor(anchor);
            CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::CLASS),
                detail: Some(def.keyword.clone()),
                ..Default::default()
            }
        }
        Symbol::Variable(variable) => {
            let def = compilation.store().variable(variable);
            CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::VARIABLE),
                detail: def
                    .value
                    .as_ref()
                    .map(|value| value.kind().to_string()),
                ..Default::default()
            }
        }
    }
}

fn enclosing_anchor(
    compilation: &Compilation,
    module: piton_compile::ModuleId,
    offset: usize,
) -> Option<AnchorId> {
    let ast = compilation.graph().get(module).ast();
    for (index, item) in ast.items.iter().enumerate() {
        let Item::Anchor(decl) = item else { continue };
        if decl.span.contains(offset) || decl.span.end == offset {
            return compilation
                .store()
                .anchors
                .iter()
                .find(|def| def.module == module && def.item == index)
                .map(|def| def.id);
        }
    }
    None
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
    let anchor = enclosing_anchor(cursor.compilation, cursor.module, cursor.offset)?;
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
