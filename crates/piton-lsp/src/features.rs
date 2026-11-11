//! Editor feature implementations.
//!
//! All of them use the same two inputs: the resolved compilation and the
//! occurrence index. Answers from the resolved program let an editor show the
//! true origin of an inherited value.

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
    offset_to_position, path_to_url, position_to_offset, span_to_range, url_to_path, Encoding,
    LineIndex,
};
use crate::index::{Role, Target};
use crate::world::World;
use crate::{TOKEN_MODIFIERS, TOKEN_TYPES};

/// Converts a compiler diagnostic into the form of the editor.
///
/// `text_of` supplies the text of other files. A label can point into
/// one, for example to the first import or to the base that constrained a
/// property. Thus the label goes to the correct line in that file. It does not
/// go to the top of the file.
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

/// All the data that a feature needs about the file below the cursor.
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
    let occurrence = index
        .at(cursor.offset)
        .filter(|occurrence| !matches!(occurrence.target, Target::Unresolved(_)));
    let mut parts = Vec::new();
    let mut range = None;
    if let Some(occurrence) = occurrence {
        if let Some(markdown) = describe(world, &cursor.path, &occurrence.target) {
            parts.push(markdown);
            range = Some(occurrence.span);
        }
    } else if let Some((span, markdown)) = describe_self_reference(&cursor) {
        parts.push(markdown);
        range = Some(span);
    }
    // The hover explains a composition on its operator, although `+` is not
    // a named occurrence. Plain text has no hover: a string is a string.
    let compositions = world.analysis.compositions_at(cursor.module, cursor.offset);
    let mut seen = HashSet::new();
    for composition in &compositions {
        if seen.insert(composition.span) {
            parts.push(composition.summary.clone());
        }
        if range.is_none() {
            range = composition
                .targets
                .iter()
                .find(|target| target.contains(cursor.offset))
                .copied();
        }
    }
    // The compiled output of a construct is a fact about its declaration.
    // Thus the hover shows it on the declared name, not on each word of its value.
    let declaring = occurrence.is_some_and(|item| item.role == Role::Definition);
    if declaring {
        if let Some(note) = mapping_note(&world.analysis, &cursor.path, cursor.offset) {
            parts.push(note);
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: parts.join("\n\n"),
        }),
        range: range.map(|span| span_to_range(&cursor.text, span)),
    })
}

/// `this`, `self` or `super` below the cursor, and the item that it identifies there.
fn describe_self_reference(cursor: &Cursor<'_>) -> Option<(Span, String)> {
    let parse = &cursor.compilation.graph().get(cursor.module).parse;
    let token = parse
        .syntax()
        .token_at_offset(rowan::TextSize::from(cursor.offset as u32))
        .find(|token| {
            matches!(
                token.kind(),
                SyntaxKind::KW_THIS | SyntaxKind::KW_SELF | SyntaxKind::KW_SUPER
            )
        })?;
    let range = token.text_range();
    let span = Span::new(usize::from(range.start()), usize::from(range.end()));
    let store = cursor.compilation.store();
    let anchor = store.anchor(enclosing_anchor(cursor)?);
    let markdown = match token.kind() {
        SyntaxKind::KW_THIS => format!(
            "`this` is `{}`, the anchor this expression is written in.",
            anchor.name
        ),
        SyntaxKind::KW_SELF => format!(
            "`self` is the anchor being compiled: `{}` or an anchor that extends it.",
            anchor.name
        ),
        _ => {
            let bases: Vec<String> = anchor
                .bases
                .iter()
                .map(|base| format!("`{}`", store.anchor(*base).name))
                .collect();
            match bases.as_slice() {
                [] => return None,
                [only] => format!("`super` is {only}."),
                _ => format!(
                    "`super` is the merge of {}; the right-most wins.",
                    bases.join(", ")
                ),
            }
        }
    };
    Some((span, markdown))
}

/// A path as the reader writes it. It is relative to the workspace,
/// with `/` between the parts. A bundled package keeps its `@` path.
fn shown_path(world: &World, path: &Path) -> String {
    match path.strip_prefix(&world.root) {
        Ok(relative) => slash_path(relative),
        Err(_) => path.to_string_lossy().replace('\\', "/"),
    }
}

/// `[`spec/Base.pi:2`](file:///.../spec/Base.pi#L2)`, a link to the line where
/// a span starts. A bundled package has no file to open. Thus it has a name
/// without a link.
fn source_link(world: &World, path: &Path, span: Span) -> String {
    let shown = shown_path(world, path);
    if path.to_string_lossy().starts_with('@') {
        return format!("`{shown}`");
    }
    let line = world
        .text(path)
        .map_or(1, |text| offset_to_position(&text, span.start).line + 1);
    match path_to_url(path) {
        Some(url) => format!("[`{shown}:{line}`]({url}#L{line})"),
        None => format!("`{shown}:{line}`"),
    }
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

/// A value as the configured renderer of the project writes it, fenced for a hover.
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

/// The first sentence or line of the `description` of an anchor. Locations
/// with space for a summary use it: symbol search, hierarchy items, outlines.
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

/// Builds the documentation for a target. It shows the kind of target, its
/// origin, its inheritance chain, and its resolved value. A blank line
/// separates the sections. An empty section does not show.
fn describe(world: &World, from: &Path, target: &Target) -> Option<String> {
    let compilation = world.compilation.as_ref()?;
    let output = &world.output;
    let store = compilation.store();
    let constrained = |constraints: &[ast::TypeConstraint]| -> Option<String> {
        (!constraints.is_empty()).then(|| {
            let names: Vec<String> = constraints
                .iter()
                .map(piton_compile::eval::constraint_label)
                .collect();
            format!("Constrained to: {}", names.join(" or "))
        })
    };
    let mut sections: Vec<String> = Vec::new();
    match target {
        Target::Anchor(anchor) => {
            let def = store.anchor(*anchor);
            let mut header = String::new();
            if def.exported {
                header.push_str("export ");
            }
            if def.is_abstract {
                header.push_str("abstract ");
            }
            header.push_str(&format!("{} {}", def.keyword, def.name));
            let module = compilation.graph().get(def.module);
            if let Item::Anchor(decl) = &module.ast().items[def.item] {
                if !decl.extends.is_empty() {
                    let bases: Vec<&str> = decl.extends.iter().map(|base| base.value.as_str()).collect();
                    header.push_str(&format!(" extends {}", bases.join(", ")));
                }
            }
            if let Some(alias) = &def.alias {
                header.push_str(&format!(" as {alias}"));
            }
            sections.push(format!("```piton\n{header}\n```"));
            sections.extend(summary(compilation, *anchor));

            let chain: Vec<String> = store
                .base_chain(*anchor)
                .into_iter()
                .filter(|id| *id != *anchor)
                .map(|id| store.anchor(id).name.clone())
                .collect();
            if !chain.is_empty() {
                sections.push(format!("Inherits: {}", chain.join(" \u{2192} ")));
            }
            let mut declared = format!(
                "Declared in {}",
                source_link(world, &compilation.anchor_module_path(*anchor), def.name_span)
            );
            if !def.exported {
                declared.push_str(" \u{b7} not exported");
            }
            sections.push(declared);

            if !def.properties.is_empty() && !def.is_abstract {
                // Often the compiled interpretation is the real question.
                sections.push(rendered(compilation, output, &piton_core::Value::Anchor(*anchor)));
            }
        }
        Target::Variable(variable) => {
            let def = store.variable(*variable);
            sections.push(format!(
                "```piton\n{}{}\n```",
                if def.exported { "export " } else { "" },
                def.name
            ));
            sections.extend(constrained(&def.constraints));
            sections.push(format!(
                "Declared in {}",
                source_link(world, &compilation.graph().get(def.module).path, def.name_span)
            ));
            if let Some(value) = &def.value {
                sections.push(rendered(compilation, output, value));
            }
        }
        Target::Property(anchor, name) => {
            let def = store.anchor(*anchor);
            let slot = def.slots.get(name)?;
            sections.push(format!("`{name}` on `{}`", def.name));
            if slot.owner != *anchor {
                // Structural inheritance is visible so that the user can see
                // which base gave a value.
                sections.push(format!("Inherited from `{}`", store.anchor(slot.owner).name));
            } else if let Some(replaced) = replaces_value(compilation, *anchor, name) {
                sections.push(format!("Overrides `{}.{name}`", store.anchor(replaced).name));
            } else if let Some(required) = overridden(compilation, *anchor, name) {
                sections.push(format!("Implements `{}.{name}`", store.anchor(required).name));
            }
            sections.extend(constrained(&slot.constraints));
            if !slot.has_value {
                sections.push("_Required: an implementing anchor must define it._".into());
            }
            if let Some(value) = def.properties.get(name) {
                sections.push(rendered(compilation, output, value));
            }
        }
        Target::Key(anchor, path) => {
            let def = store.anchor(*anchor);
            let (last, parents) = path.split_last()?;
            sections.push(format!("`{last}` in `{}.{}`", def.name, parents.join(".")));
            sections.push(format!(
                "_A key of that dictionary, not a property of `{}`._",
                def.name
            ));
            if let Some(value) = key_value(compilation, *anchor, path) {
                sections.push(rendered(compilation, output, value));
            }
        }
        Target::Keyword(keyword, anchor) => {
            let def = store.anchor((*anchor)?);
            sections.push(format!(
                "Keyword `{keyword}`: shorthand for `extends {}`, declared in {}.",
                def.name,
                source_link(world, &compilation.anchor_module_path(def.id), def.name_span)
            ));
        }
        Target::Module(written) => {
            let path = resolve_module_path(world, from, written)?;
            let shown = shown_path(world, &path);
            sections.push(match path_to_url(&path) {
                Some(url) if !shown.starts_with('@') => {
                    format!("Module `{}` \u{2192} [`{shown}`]({url})", written.display())
                }
                _ => format!("Module `{}` \u{2192} `{shown}`", written.display()),
            });
        }
        Target::Unresolved(_) => return None,
    }
    Some(sections.join("\n\n"))
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

/// The base with a value of `name` that the declaration of an anchor replaces.
/// This is the right-most base that has it, when that base gave it a value.
/// When an anchor writes a property that an abstract base only requires, it
/// implements the property. It does not override it.
pub fn replaces_value(compilation: &Compilation, anchor: AnchorId, name: &str) -> Option<AnchorId> {
    let base = overridden(compilation, anchor, name)?;
    compilation
        .store()
        .anchor(base)
        .slots
        .get(name)
        .is_some_and(|slot| slot.has_value)
        .then_some(base)
}

/// The base with a declaration of `name` that the declaration of an anchor
/// replaces. This is the right-most base that has it.
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

/// The location of the declaration of a target.
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
            // Go to the declaration that supplies the value.
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

/// The location of a nested key. It is in the anchor that supplies the
/// property that holds it. The search descends the dictionaries by name.
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

pub fn definition(world: &World, uri: &Url, position: Position) -> Option<GotoDefinitionResponse> {
    let cursor = cursor(world, uri, position)?;
    let index = world.index.get(cursor.module)?;
    let occurrence = index.at(cursor.offset)?;
    let origin = span_to_range(&cursor.text, occurrence.span);

    // On the declaration of a property that replaces an inherited property,
    // the useful target is the declaration that it replaces.
    let target = match (&occurrence.target, occurrence.role) {
        (Target::Property(anchor, name), Role::Definition) => {
            match overridden(cursor.compilation, *anchor, name) {
                Some(base) => Target::Property(base, name.clone()),
                None => occurrence.target.clone(),
            }
        }
        (target, _) => target.clone(),
    };

    // A module path goes to the file of the module.
    let (location, whole) = if let Target::Module(written) = &target {
        let resolved = resolve_module_path(world, &cursor.path, written)?;
        let location = Location {
            uri: path_to_url(&resolved)?,
            range: Range::default(),
        };
        (location.clone(), location.range)
    } else {
        let location = definition_location(world, &target)?;
        let whole = declaration_range(world, &target).unwrap_or(location.range);
        (location, whole)
    };

    Some(if world.caps.link_support {
        GotoDefinitionResponse::Link(vec![LocationLink {
            origin_selection_range: Some(origin),
            target_uri: location.uri,
            target_range: whole,
            target_selection_range: location.range,
        }])
    } else {
        GotoDefinitionResponse::Scalar(location)
    })
}

/// The full declaration that a target names, for an editor that shows it. For
/// an anchor or a variable, it goes from its first line to the end of its body.
fn declaration_range(world: &World, target: &Target) -> Option<Range> {
    let compilation = world.compilation.as_ref()?;
    let (path, span) = match target {
        Target::Anchor(anchor) => {
            let def = compilation.store().anchor(*anchor);
            let module = compilation.graph().get(def.module);
            let Item::Anchor(decl) = &module.ast().items[def.item] else {
                return None;
            };
            (module.path.clone(), trimmed(&module.source, decl.span))
        }
        Target::Variable(variable) => {
            let def = compilation.store().variable(*variable);
            let module = compilation.graph().get(def.module);
            let span = module.ast().items.iter().find_map(|item| match item {
                Item::Variable(decl) if decl.name_span == def.name_span => Some(decl.span),
                _ => None,
            })?;
            (module.path.clone(), trimmed(&module.source, span))
        }
        _ => return None,
    };
    let text = world.text(&path)?;
    Some(span_to_range(&text, span))
}

/// A span without the blank lines and whitespace at its ends.
fn trimmed(source: &str, span: Span) -> Span {
    let text = &source[span.start..span.end.min(source.len())];
    let start = span.start + (text.len() - text.trim_start().len());
    Span::new(start, (span.start + text.trim_end().len()).max(start))
}

/// The anchors that the type of a name identifies, for `textDocument/typeDefinition`.
///
/// On a property or a variable, these are the anchors that its constraints
/// name, in source order. On an anchor, it is the anchor itself. A built-in
/// type has no target.
pub fn type_definition(
    world: &World,
    uri: &Url,
    position: Position,
) -> Option<request::GotoTypeDefinitionResponse> {
    let cursor = cursor(world, uri, position)?;
    let index = world.index.get(cursor.module)?;
    let occurrence = index.at(cursor.offset)?;
    let compilation = cursor.compilation;
    let store = compilation.store();

    let mut anchors: Vec<AnchorId> = Vec::new();
    match &occurrence.target {
        Target::Anchor(anchor) => anchors.push(*anchor),
        Target::Property(anchor, name) => {
            let slot = store.anchor(*anchor).slots.get(name)?;
            for constraint in &slot.constraints {
                if let ast::TypeName::Named(written) = &constraint.name {
                    if let Some(found) = constraint_type(&cursor, *anchor, slot, written) {
                        anchors.push(found);
                    }
                }
            }
        }
        Target::Variable(variable) => {
            let def = store.variable(*variable);
            for constraint in &def.constraints {
                if let ast::TypeName::Named(written) = &constraint.name {
                    if let Some(Symbol::Anchor(found)) =
                        compilation.resolution.lookup(def.module, written)
                    {
                        anchors.push(found);
                    }
                }
            }
        }
        _ => return None,
    }
    let mut seen = HashSet::new();
    anchors.retain(|anchor| seen.insert(*anchor));
    let locations: Vec<Location> = anchors
        .into_iter()
        .filter_map(|anchor| definition_location(world, &Target::Anchor(anchor)))
        .collect();
    (!locations.is_empty()).then_some(request::GotoTypeDefinitionResponse::Array(locations))
}

/// All occurrences in this file of the item below the cursor. A declaration
/// is where the source writes it. All other occurrences are reads.
pub fn document_highlight(
    world: &World,
    uri: &Url,
    position: Position,
) -> Option<Vec<DocumentHighlight>> {
    let cursor = cursor(world, uri, position)?;
    let index = world.index.get(cursor.module)?;
    let target = index.at(cursor.offset)?.target.clone();
    if matches!(target, Target::Unresolved(_) | Target::Module(_)) {
        return None;
    }
    let related = related_to(cursor.compilation, &target);
    let lines = LineIndex::new(&cursor.text, Encoding::Utf16);
    let mut out: Vec<DocumentHighlight> = index
        .occurrences
        .iter()
        .filter(|occurrence| related(&occurrence.target))
        .map(|occurrence| DocumentHighlight {
            range: lines.range(occurrence.span),
            kind: Some(match occurrence.role {
                Role::Definition => DocumentHighlightKind::WRITE,
                Role::Reference => DocumentHighlightKind::READ,
            }),
        })
        .collect();
    out.sort_by_key(|highlight| (highlight.range.start.line, highlight.range.start.character));
    out.dedup_by_key(|highlight| highlight.range);
    Some(out)
}

fn resolve_module_path(world: &World, from: &Path, written: &Path) -> Option<PathBuf> {
    module_path_from(&world.project, from, &written.to_string_lossy())
}

/// Resolves the text of a module path relative to the file that contains it.
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
    out.dedup_by(|a, b| a.uri == b.uri && a.range == b.range);
    Some(out)
}

/// Builds a predicate for all occurrences that are references to `target`.
///
/// A user keyword is sugar for `extends`. Thus `type Strings:` references the
/// `Type` anchor in the same way as `extends Type`. A property that overrides
/// an inherited property is a reference to the same idea. Thus the predicate
/// also includes its family.
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
            // A nested key is only the same key at the same path. It is never
            // a property that has the same name.
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

/// Tells if a bundled package declares a target. The user cannot edit a bundled
/// package.
fn in_package(compilation: &Compilation, target: &Target) -> Option<String> {
    let module = match target {
        Target::Anchor(anchor) | Target::Keyword(_, Some(anchor)) | Target::Property(anchor, _) => {
            let store = compilation.store();
            let owner = match target {
                Target::Property(_, name) => store
                    .anchor(*anchor)
                    .slots
                    .get(name)
                    .map_or(*anchor, |slot| slot.owner),
                _ => *anchor,
            };
            store.anchor(owner).module
        }
        Target::Variable(variable) => compilation.store().variable(*variable).module,
        _ => return None,
    };
    let path = compilation.graph().get(module).path.to_string_lossy().to_string();
    path.starts_with('@').then(|| {
        // `@piton/belay/anchors/Skill.pi` is in the package `@piton/belay`.
        path.split('/').take(2).collect::<Vec<_>>().join("/")
    })
}

/// Checks that a rename of an item is possible. If not, it gives the reason.
fn renameable(compilation: &Compilation, occurrence: &crate::index::Occurrence) -> Result<(), String> {
    match &occurrence.target {
        Target::Module(_) => Err("A module path is renamed by moving its file.".into()),
        Target::Unresolved(_) => Err(format!("`{}` is not declared anywhere.", occurrence.text)),
        target => match in_package(compilation, target) {
            Some(package) => Err(format!(
                "`{}` is declared in `{package}` and cannot be renamed.",
                occurrence.text
            )),
            None => Ok(()),
        },
    }
}

pub fn prepare_rename(
    world: &World,
    uri: &Url,
    position: Position,
) -> Result<Option<PrepareRenameResponse>, String> {
    let Some(cursor) = cursor(world, uri, position) else {
        return Ok(None);
    };
    let Some(occurrence) = world
        .index
        .get(cursor.module)
        .and_then(|index| index.at(cursor.offset))
    else {
        return Ok(None);
    };
    renameable(cursor.compilation, occurrence)?;
    Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
        range: span_to_range(&cursor.text, occurrence.span),
        placeholder: occurrence.text.clone(),
    }))
}

/// Tells if `name` is valid at the declaration of `target`.
fn valid_new_name(target: &Target, name: &str) -> Result<(), String> {
    let key_characters = !name.is_empty()
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
        && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-');
    let ok = match target {
        Target::Anchor(_) => {
            name.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
                && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                && !piton_core::names::is_reserved(name)
        }
        Target::Variable(_) => key_characters && !piton_core::names::is_reserved(name),
        Target::Property(..) | Target::Key(..) => key_characters,
        Target::Keyword(..) => {
            piton_core::names::is_valid_keyword(name) && !piton_core::names::is_reserved(name)
        }
        Target::Module(_) | Target::Unresolved(_) => false,
    };
    if ok {
        return Ok(());
    }
    Err(match target {
        Target::Anchor(_) => format!(
            "`{name}` is not an anchor name: it starts with a letter or `_`, holds only letters, digits and `_`, and is not a reserved word."
        ),
        Target::Keyword(..) => format!(
            "`{name}` is not a keyword: keywords are lowercase, may be kebab-case, and are not reserved words."
        ),
        Target::Variable(_) => format!(
            "`{name}` is not a variable name: it holds only letters, digits, `_` and `-`, and is not a reserved word."
        ),
        _ => format!("`{name}` is not a key: keys hold only letters, digits, `_` and `-`."),
    })
}

/// The locations where `name` will collide with an item that exists. This is a
/// top-level name in a module that the rename changes. It is also a property
/// of an anchor in the family of the renamed property.
fn collision(
    compilation: &Compilation,
    target: &Target,
    modules: &[piton_compile::ModuleId],
    name: &str,
) -> Option<String> {
    let store = compilation.store();
    match target {
        Target::Anchor(_) | Target::Variable(_) => modules.iter().find_map(|module| {
            compilation.resolution.lookup(*module, name).map(|_| {
                format!(
                    "`{name}` is already defined in `{}`.",
                    compilation.graph().get(*module).path.display()
                )
            })
        }),
        Target::Property(anchor, _) => store
            .anchors
            .iter()
            .filter(|def| {
                store.inherits_from(def.id, *anchor) || store.inherits_from(*anchor, def.id)
            })
            .find(|def| def.slots.contains_key(name))
            .map(|def| format!("`{}` already has a property `{name}`.", def.name)),
        Target::Keyword(..) => store
            .anchors
            .iter()
            .find(|def| def.alias.as_deref() == Some(name))
            .map(|def| format!("`{name}` is already the keyword of `{}`.", def.name)),
        _ => None,
    }
}

pub fn rename(
    world: &World,
    uri: &Url,
    position: Position,
    new_name: &str,
) -> Result<Option<WorkspaceEdit>, String> {
    let Some(cursor) = cursor(world, uri, position) else {
        return Ok(None);
    };
    let Some(occurrence) = world
        .index
        .get(cursor.module)
        .and_then(|index| index.at(cursor.offset))
    else {
        return Ok(None);
    };
    let compilation = cursor.compilation;
    renameable(compilation, occurrence)?;
    let target = occurrence.target.clone();
    let written = occurrence.text.clone();
    if new_name == written {
        return Ok(None);
    }
    valid_new_name(&target, new_name)?;

    let related = related_to(compilation, &target);
    let mut edits: HashMap<PathBuf, Vec<TextEdit>> = HashMap::new();
    let mut texts: HashMap<PathBuf, String> = HashMap::new();
    let mut touched = Vec::new();
    for (module, occurrence) in world.index.matching(|candidate| related(candidate)) {
        // Rewrite only occurrences with the same letters. A keyword alias and
        // an aliased import bind other names for the same item. A rename of
        // one must not also rewrite the other without a message.
        if occurrence.text != written {
            continue;
        }
        let path = compilation.graph().get(module).path.clone();
        if path.to_string_lossy().starts_with('@') {
            continue;
        }
        if !touched.contains(&module) {
            touched.push(module);
        }
        let text = texts
            .entry(path.clone())
            .or_insert_with(|| world.text(&path).unwrap_or_default());
        edits.entry(path).or_default().push(TextEdit {
            range: span_to_range(text, occurrence.span),
            new_text: new_name.to_string(),
        });
    }
    if let Some(problem) = collision(compilation, &target, &touched, new_name) {
        return Err(problem);
    }

    for list in edits.values_mut() {
        list.sort_by_key(|edit| (edit.range.start.line, edit.range.start.character));
        list.dedup_by_key(|edit| (edit.range.start.line, edit.range.start.character));
    }
    Ok(Some(workspace_edit(world, edits)))
}

/// A set of edits in the form that the client asked for. When the client
/// supports it, the edits have a version for each document. Thus the client
/// refuses an edit for an old buffer. It does not apply it at the wrong location.
pub fn workspace_edit(world: &World, edits: HashMap<PathBuf, Vec<TextEdit>>) -> WorkspaceEdit {
    let mut paths: Vec<PathBuf> = edits.keys().cloned().collect();
    paths.sort();
    if world.caps.document_changes {
        let changes = paths
            .into_iter()
            .filter_map(|path| {
                let uri = path_to_url(&path)?;
                let version = world.version(&path);
                let list = edits.get(&path)?.clone();
                Some(TextDocumentEdit {
                    text_document: OptionalVersionedTextDocumentIdentifier { uri, version },
                    edits: list.into_iter().map(OneOf::Left).collect(),
                })
            })
            .collect();
        WorkspaceEdit {
            document_changes: Some(DocumentChanges::Edits(changes)),
            ..Default::default()
        }
    } else {
        WorkspaceEdit {
            changes: Some(
                paths
                    .into_iter()
                    .filter_map(|path| Some((path_to_url(&path)?, edits.get(&path)?.clone())))
                    .collect(),
            ),
            ..Default::default()
        }
    }
}

/// The same edit, in the form that the client asked for (see [`workspace_edit`]).
fn versioned(world: &World, edit: WorkspaceEdit) -> WorkspaceEdit {
    let Some(changes) = edit.changes else {
        return edit;
    };
    let by_path = changes
        .into_iter()
        .filter_map(|(uri, edits)| Some((url_to_path(&uri)?, edits)))
        .collect();
    workspace_edit(world, by_path)
}

/// Rewrites the module paths that a file move breaks, before the move occurs.
///
/// Each `use` and `from` path that resolves into a moved file or folder changes
/// to its new location. The function also changes the relative paths in a
/// moved file. Thus they still point to the same items. A path keeps its style. A rooted path
/// stays rooted, and a relative path stays relative. `.pi` stays or stays
/// not there. A path to the index of a folder stays a path to the folder.
/// Package paths never change.
pub fn will_rename_files(world: &World, files: &[FileRename]) -> Option<WorkspaceEdit> {
    let compilation = world.compilation.as_ref()?;
    let moves: Vec<(PathBuf, PathBuf)> = files
        .iter()
        .filter_map(|file| {
            let old = url_to_path(&Url::parse(&file.old_uri).ok()?)?;
            let new = url_to_path(&Url::parse(&file.new_uri).ok()?)?;
            Some((old, new))
        })
        .collect();
    if moves.is_empty() {
        return None;
    }
    // The new location of a path, if a move changes it.
    let moved = |path: &Path| -> Option<PathBuf> {
        moves.iter().find_map(|(old, new)| {
            if path == old {
                Some(new.clone())
            } else {
                path.strip_prefix(old).ok().map(|rest| new.join(rest))
            }
        })
    };

    let mut edits: HashMap<PathBuf, Vec<TextEdit>> = HashMap::new();
    for module in compilation.graph().iter() {
        if module.is_package() {
            continue;
        }
        let file = module.path.clone();
        let file_after = moved(&file).unwrap_or_else(|| file.clone());
        let Some(text) = world.text(&file) else {
            continue;
        };
        for item in &module.ast().items {
            let path = match item {
                Item::Use(decl) => &decl.path,
                Item::From(decl) => &decl.path,
                _ => continue,
            };
            let written = path.text.as_str();
            if !(written.starts_with('.') || written.starts_with('/')) {
                continue;
            }
            let Some(target) = module_path_from(&world.project, &file, written) else {
                continue;
            };
            let target_after = moved(&target);
            if target_after.is_none() && file_after == file {
                continue;
            }
            let target_after = target_after.unwrap_or(target.clone());
            let Some(rewritten) =
                rewrite_module_path(&world.project, written, &target_after, &file_after)
            else {
                continue;
            };
            if rewritten != written {
                edits.entry(file.clone()).or_default().push(TextEdit {
                    range: span_to_range(&text, path.span),
                    new_text: rewritten,
                });
            }
        }
    }
    if edits.is_empty() {
        return None;
    }
    Some(workspace_edit(world, edits))
}

/// The text of a module path that names `target` from a file at `from`. It
/// uses the same style as `written`.
fn rewrite_module_path(project: &Project, written: &str, target: &Path, from: &Path) -> Option<String> {
    let index = format!("index.{}", piton_syntax::language::EXTENSION);
    let names_folder = target.file_name().is_some_and(|name| name == index.as_str())
        && !written.trim_end_matches('/').ends_with(&index)
        && !written.ends_with(&format!(".{}", piton_syntax::language::EXTENSION));
    let keeps_extension = written.ends_with(&format!(".{}", piton_syntax::language::EXTENSION));
    let mut target = target.to_path_buf();
    if names_folder {
        target.pop();
    } else if !keeps_extension {
        target.set_extension("");
    }

    if written.starts_with('/') {
        let relative = target.strip_prefix(&project.source_root).ok()?;
        return Some(format!("/{}", slash_path(relative)));
    }
    let relative = relative_path(from.parent()?, &target)?;
    let text = slash_path(&relative);
    Some(if text.is_empty() {
        ".".into()
    } else if text == ".." || text.starts_with("../") {
        text
    } else {
        format!("./{text}")
    })
}

fn slash_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// `to` as a path relative to the directory `from`.
fn relative_path(from: &Path, to: &Path) -> Option<PathBuf> {
    let from: Vec<_> = from.components().collect();
    let to_parts: Vec<_> = to.components().collect();
    let common = from
        .iter()
        .zip(&to_parts)
        .take_while(|(a, b)| a == b)
        .count();
    if common == 0 {
        return None;
    }
    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for part in &to_parts[common..] {
        out.push(part.as_os_str());
    }
    Some(out)
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
                let children = property_symbols(&decl.body, text, SymbolKind::PROPERTY);
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
                    trimmed(text, decl.span),
                    decl.name_span,
                    text,
                    detail,
                    children,
                ));
            }
            Item::Variable(decl) => symbols.push(symbol(
                &decl.name,
                SymbolKind::VARIABLE,
                trimmed(text, decl.span),
                decl.name_span,
                text,
                constraint_text(text, &decl.constraints),
                decl.value
                    .block
                    .as_ref()
                    .map(|block| property_symbols(block, text, SymbolKind::FIELD))
                    .unwrap_or_default(),
            )),
            _ => {}
        }
    }
    Some(DocumentSymbolResponse::Nested(symbols))
}

/// The properties of a block as symbols. Each has the keys of its dictionary
/// below it. List items are values, not names. Thus the function does not include them.
fn property_symbols(block: &ast::Block, text: &str, kind: SymbolKind) -> Vec<DocumentSymbol> {
    block
        .properties()
        .map(|property| {
            let children = property
                .value
                .block
                .as_ref()
                .map(|block| property_symbols(block, text, SymbolKind::FIELD))
                .unwrap_or_default();
            symbol(
                &property.name,
                kind,
                trimmed(text, property.span),
                property.name_span,
                text,
                constraint_text(text, &property.constraints),
                children,
            )
        })
        .collect()
}

/// The constraints as the source writes them, `string` or `Foo[]`. The join
/// follows their chain.
fn constraint_text(text: &str, constraints: &[ast::TypeConstraint]) -> Option<String> {
    (!constraints.is_empty()).then(|| {
        constraints
            .iter()
            .map(|constraint| text[constraint.span.start..constraint.span.end].to_string())
            .collect::<Vec<_>>()
            .join(":: ")
    })
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
/// How well a name matches a query. 0 is for the same name and 1 is for a
/// prefix. 2 is for a part of the name. 3 is for the letters of the query in
/// order. Case has no effect.
pub fn match_tier(name: &str, query: &str) -> Option<u8> {
    let name = name.to_lowercase();
    let query = query.to_lowercase();
    if name == query {
        return Some(0);
    }
    if name.starts_with(&query) {
        return Some(1);
    }
    if name.contains(&query) {
        return Some(2);
    }
    let mut letters = name.chars();
    query
        .chars()
        .all(|wanted| letters.any(|letter| letter == wanted))
        .then_some(3)
}

/// The maximum number of symbols that one search returns.
const WORKSPACE_SYMBOL_LIMIT: usize = 256;

#[allow(deprecated)]
pub fn workspace_symbols(world: &World, query: &str) -> Option<Vec<SymbolInformation>> {
    let compilation = world.compilation.as_ref()?;
    let store = compilation.store();
    let mut candidates: Vec<(u8, String, PathBuf, Span, SymbolKind)> = Vec::new();
    let anchors = store.anchors.iter().map(|def| {
        (
            def.name.clone(),
            compilation.anchor_module_path(def.id),
            def.name_span,
            if def.is_abstract {
                SymbolKind::INTERFACE
            } else {
                SymbolKind::CLASS
            },
        )
    });
    let variables = store.variables.iter().map(|def| {
        (
            def.name.clone(),
            compilation.graph().get(def.module).path.clone(),
            def.name_span,
            SymbolKind::VARIABLE,
        )
    });
    for (name, path, span, kind) in anchors.chain(variables) {
        // A bundled package has no file that the editor can open.
        if path.to_string_lossy().starts_with('@') {
            continue;
        }
        let tier = if query.is_empty() {
            Some(0)
        } else {
            match_tier(&name, query)
        };
        if let Some(tier) = tier {
            candidates.push((tier, name, path, span, kind));
        }
    }
    candidates.sort_by(|a, b| {
        (a.0, a.1.len(), &a.1, &a.2).cmp(&(b.0, b.1.len(), &b.1, &b.2))
    });
    candidates.truncate(WORKSPACE_SYMBOL_LIMIT);

    let mut texts: HashMap<PathBuf, String> = HashMap::new();
    let out = candidates
        .into_iter()
        .filter_map(|(_, name, path, span, kind)| {
            let text = texts
                .entry(path.clone())
                .or_insert_with(|| world.text(&path).unwrap_or_default());
            let range = span_to_range(text, span);
            let mut container = shown_path(world, &path);
            if let Some(stripped) = container.strip_suffix(&format!(".{}", piton_syntax::language::EXTENSION)) {
                container = stripped.to_string();
            }
            Some(SymbolInformation {
                name,
                kind,
                tags: None,
                deprecated: None,
                location: Location {
                    uri: path_to_url(&path)?,
                    range,
                },
                container_name: Some(container),
            })
        })
        .collect();
    Some(out)
}

// ---------------------------------------------------------------------------
// Completion
// ---------------------------------------------------------------------------

/// The text that the cursor is in the middle of.
///
/// All that Piton offers depends on context. The words at the head of an
/// unindented line are not the words after `extends`. The two sets are not correct
/// inside `@{...}`. There, only a target of a reference can occur. Thus the
/// server first classifies the line, then makes the list. This prevents one
/// long chain of special cases. It also prevents suggestions in locations
/// where the text is not Piton.
#[derive(Debug, PartialEq, Eq)]
enum Context<'a> {
    /// No useful offer is possible. Examples are a comment, a verbatim block,
    /// or the middle of a name that the editor cannot make.
    Nothing,
    /// After `use` or `from`, at the location of a module path.
    ModulePath,
    /// After `from <path>`, at the location of `import` or `export`.
    ImportDirection,
    /// After `from <path> import`, at the location of the imported names.
    ImportName { module: &'a str },
    /// The head of an unindented line, after the modifiers that it has at this time.
    Declaration { exported: bool, is_abstract: bool },
    /// After `extends` in a declaration head.
    Bases,
    /// Inside a `::` type constraint.
    TypeConstraint,
    /// Inside an interpolation. It holds the sigil that opened it, and the path
    /// before a `.` at the end, if there is one.
    Expression {
        sigil: &'a str,
        member_of: Option<&'a str>,
    },
    /// The location of a value. The key is the key with the constraints that
    /// tell which values are valid. A list item has no key to name.
    Value { key: Option<&'a str> },
    /// The head of an indented line, at the location of a key.
    Body,
}

pub fn completion(world: &World, uri: &Url, position: Position) -> Option<CompletionResponse> {
    use piton_syntax::cursor::{site_at, Site};
    let cursor = cursor(world, uri, position)?;
    let site = site_at(&cursor.text, cursor.offset);
    let sigil_text = |sigil: ast::Sigil| match sigil {
        ast::Sigil::Standard => "{",
        ast::Sigil::Stringify => "${",
        ast::Sigil::Numeric => "#{",
        ast::Sigil::Reference => "@{",
    };
    let context = match &site {
        Site::Inert => Context::Nothing,
        Site::ModulePath => Context::ModulePath,
        Site::ImportDirection => Context::ImportDirection,
        Site::ImportName { path, .. } => Context::ImportName { module: path },
        Site::DeclarationHead {
            exported,
            is_abstract,
        } => Context::Declaration {
            exported: *exported,
            is_abstract: *is_abstract,
        },
        Site::Base => Context::Bases,
        Site::Constraint => Context::TypeConstraint,
        Site::Expression { sigil, member_of } => Context::Expression {
            sigil: sigil_text(*sigil),
            member_of: member_of.as_deref(),
        },
        Site::Key { .. } => Context::Body,
        Site::Value { key } => Context::Value { key: key.as_deref() },
    };

    // A selected candidate replaces the partial word at the cursor. It does
    // not append to it: `anc` becomes `anchor`, not `ancanchor`. All items of
    // the context replace the same span. Thus the server calculates the span
    // one time here.
    let lines = LineIndex::new(&cursor.text, Encoding::Utf16);
    let start = replace_start(&context, &cursor.text, cursor.offset);
    let word_end = cursor.offset
        + cursor.text[cursor.offset..]
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
            .unwrap_or(cursor.text.len() - cursor.offset);
    let insert = lines.range(Span::new(start, cursor.offset));
    let replace = lines.range(Span::new(start, word_end));
    // When the source has the colon of a completed key, the key keeps
    // that colon. The edit does not add a second colon.
    let colon_follows = cursor.text[word_end..].starts_with(':');

    let mut items = match context {
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
        // A list item holds the value of the key above it. Thus the constraints
        // of that key tell which values are valid in it.
        Context::Value { key } => values(&cursor, key),
        Context::Body => body(&cursor),
    };

    match &site {
        Site::Key {
            block_keys,
            block_has_items,
        } => {
            // The list does not offer a key that the body has. `pass`
            // has a sense only in a body that has no other items.
            items.retain(|item| {
                if item.label == "pass" {
                    return !block_has_items;
                }
                !block_keys.contains(&item.label)
            });
            if colon_follows {
                for item in &mut items {
                    item.insert_text = None;
                }
            }
        }
        Site::ImportName { listed, .. } => items.retain(|item| !listed.contains(&item.label)),
        _ => {}
    }

    let generation = world.generation;
    let items: Vec<CompletionItem> = items
        .into_iter()
        .map(|mut item| {
            // The edit tells what the editor writes. `insert_text` tells the
            // same text where the two are different. A key of a body has its
            // colon, and a wrapped reference has its braces. Only an edit also
            // tells which text it replaces.
            let written = item
                .insert_text
                .clone()
                .unwrap_or_else(|| item.label.clone());
            item.text_edit = Some(if world.caps.insert_replace {
                CompletionTextEdit::InsertAndReplace(InsertReplaceEdit {
                    new_text: written,
                    insert,
                    replace,
                })
            } else {
                CompletionTextEdit::Edit(TextEdit {
                    range: insert,
                    new_text: written,
                })
            });
            // The data records the compilation that made the item. Thus a resolve
            // after a different compile does not describe a different item.
            if let Some(serde_json::Value::Object(data)) = &mut item.data {
                data.insert("gen".into(), serde_json::json!(generation));
            }
            item
        })
        .collect();

    Some(CompletionResponse::Array(items))
}

/// The start of the text that the user completes.
///
/// The edit must cover the word below the cursor. Thus a candidate replaces
/// the word. The extent of that word depends on the cursor location. A module
/// path is one full word (`@piton/belay`, `./lib/Type`). A member is only the
/// segment after the last dot. Inside `{...}`, a name comes after a brace and
/// a sigil that are not part of the name. A `-` directly before a list value
/// is the marker of the item, not part of the word.
fn replace_start(context: &Context<'_>, text: &str, cursor: usize) -> usize {
    let in_word = |character: char| {
        let name = character.is_alphanumeric() || character == '_' || character == '-';
        match context {
            // The source has the brace and sigil. A dot opens the
            // member after it. Thus a name stops at both.
            Context::Expression { .. } => name,
            // A path contains characters that a name does not: `@`, `/`, and `.`.
            Context::ModulePath => !character.is_whitespace(),
            // `*` is a separate name in an import list.
            Context::ImportName { .. } => name || character == '*',
            // `@` and `#` are partial sigils. When the editor completes an
            // anchor into a reference value, the `@` becomes `@{...}`. The
            // editor does not add a second `@` in front of it.
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

    // A name never starts with `-`. Thus a run that starts with `-` is the
    // marker of the list item after it. `- nu` stops at the space, and `-nu`
    // does not. The marker stays in all cases. The value goes after it.
    if text[start..cursor].starts_with('-') {
        start += '-'.len_utf8();
    }
    start
}

/// Adds the documentation for the item that the editor asked about.
///
/// Completion runs on each keystroke, and a list can have hundreds of items.
/// The documentation for one anchor renders its full compiled value. To build
/// it for each item each time is the slowest operation of the server.
/// Thus each item holds only the identity of the item that it names. This
/// function builds the description only for the item that the cursor selects.
pub fn resolve_completion(world: &World, mut item: CompletionItem) -> CompletionItem {
    let Some(compilation) = world.compilation.as_ref() else {
        return item;
    };
    let current = item
        .data
        .as_ref()
        .and_then(|data| data.get("gen"))
        .and_then(serde_json::Value::as_u64)
        == Some(world.generation);
    if !current {
        // An earlier compilation made this item. Its ids can now name a
        // different item. No description is better than a wrong description.
        return item;
    }
    let Some(target) = item.data.as_ref().and_then(target_from_data) else {
        return item;
    };
    let _ = compilation;
    if let Some(markdown) = describe(world, &world.root, &target) {
        item.documentation = Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: markdown,
        }));
    }
    item
}

/// The identity that an item holds, so that `completionItem/resolve` can find it again.
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

// --- Classify the line ------------------------------------------------------

/// Tells if the line that starts at `line_start` is inside an escape block.
///
/// An escape block is verbatim. Its contents are not Piton. Thus a `{` inside
/// it opens nothing, and a `//` starts no comment. A separate line opens it and
/// a separate line closes it. Thus a count of the delimiters above the cursor
/// gives the answer. A code fence is only text. Thus it is not verbatim.
fn verbatim_at(text: &str, line_start: usize) -> bool {
    let mut escape: Option<usize> = None;
    for line in text[..line_start].lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && trimmed.chars().all(|c| c == '\\') {
            // A run with the same length as the first run closes an escape
            // block. Thus a block can contain a shorter run.
            escape = match escape {
                Some(open) if trimmed.len() == open => None,
                other => other.or(Some(trimmed.len())),
            };
        }
    }
    escape.is_some()
}

// --- Build the list ---------------------------------------------------------

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

/// All modules that a `use` or `from` can name.
///
/// The compiled graph holds only what the project reaches at this time. The
/// purpose of import completion is to reach a new item. Thus the server also
/// reads the source root from disk. The bundled packages are not on disk.
/// They are inside the compiler. Thus the server names them directly.
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

    // An import of an installed package uses its name. Thus the list offers the
    // name, not the relative path to the directory of the package.
    let project_root = &compilation.project.root;
    let installed = piton_compile::packages::installed_names(project_root);
    for name in &installed {
        offer(name.clone(), Some("installed package".into()), &mut items);
    }

    // Completion runs on a keystroke. Thus the walk has a limit.
    let mut files = piton_compile::module::sources_up_to(source_root, 5000);
    // A module in the graph can be outside the source root. An installed
    // package is an example. The list still offers it.
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

/// How to import a file inside an installed package. The import uses the name
/// of the package, not a relative path that leaves the source root.
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

/// The names that a module exports, for `from <path> import ...`.
fn import_names(cursor: &Cursor<'_>, written: &str) -> Vec<CompletionItem> {
    let compilation = cursor.compilation;

    // A path that names no module exports nothing. If the list gave names, it
    // will offer names that do not exist.
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
        // `export` and `abstract` both come before the keyword. An import
        // accepts none of the two.
        words.extend(["export", "use", "from"]);
    }
    if !is_abstract {
        words.push("abstract");
    }
    words.push("anchor");
    let mut items = keywords(&words);

    // A user keyword is sugar for `extends`. Thus it opens a declaration at
    // the same location as `anchor`.
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

/// The anchors that a module can name.
///
/// `inheriting` limits the list to the anchors that a constraint accepts.
/// `wrap` writes the interpolation around the name when the cursor location
/// needs one.
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
            // A value that must be an anchor is a reference to an anchor. Thus
            // the completion also writes the braces.
            item.insert_text = Some(format!("{sigil}{name}}}"));
        }
        items.push(item);
    }
    items
}

/// The exported names of a module that the list offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Only {
    /// All exported items.
    Anything,
    /// An anchor, because no other item is valid at the cursor.
    Anchors,
    /// An anchor that a constraint accepts.
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

/// Names that other modules export and that this module cannot see at this time.
///
/// A completion of one of these also writes the import. The editor must know
/// the file of each symbol. Without the editor, a person must find that file
/// manually.
fn importable(cursor: &Cursor<'_>, only: Only) -> Vec<CompletionItem> {
    // All names in the project make a list that nobody reads. The editor
    // offered each name that is in scope before.
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

    // When an index re-exports a name, several modules export it. They all go
    // to the same symbol. The list offers it one time, with an import from the
    // shortest path. This is usually the index, which is the purpose of the
    // re-export. The list does not offer it one time for each module.
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
            // These go below all items in scope. A name that the file can
            // see is almost always the correct name.
            item.sort_text = Some(format!("9{name}"));
            item.additional_text_edits = Some(vec![TextEdit {
                range: Range { start: at, end: at },
                new_text: format!("from {written} import {name}\n"),
            }]);
            item
        })
        .collect()
}

/// The types that a `::` constraint accepts.
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
    // `p:: extends Operator:` accepts all anchors with a chain that includes one.
    items.extend(keywords(&["extends"]));
    // The name of an anchor is also a type. A constraint to an anchor tells
    // which anchors a property accepts.
    items.extend(anchors_in_scope(cursor, None, None));
    items.extend(importable(cursor, Only::Anchors));
    items
}

/// The items that are valid inside an interpolation.
fn expression(cursor: &Cursor<'_>, sigil: &str, member_of: Option<&str>) -> Vec<CompletionItem> {
    if let Some(path) = member_of {
        return members(cursor, path);
    }

    let compilation = cursor.compilation;
    let mut items = Vec::new();

    // `@{...}` must resolve to an anchor that a reference can target. Thus a
    // variable or a literal in it causes an error.
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

/// The members of the item that `path` resolves to.
///
/// `self.` and `this.` go to the anchor that contains them, and `super.` to its first
/// base. A name goes to the anchor that it refers to. After the first segment,
/// the walk continues through the resolved value. Thus `Config.frameworks.`
/// offers the real contents.
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
            // `super` is all that the anchor inherits. The merge is the same as
            // the merge of inheritance. Thus each base adds members.
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
        // At the anchor itself, the slots are the answer. They hold the
        // inherited items and the items that the anchor writes.
        return slots_of(compilation, anchor);
    }

    // After the anchor, only the resolved value knows the shape. A nested key
    // can come from a base or an expression.
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

/// The items that `super.` reaches: the slots of all bases. When more than one
/// base has a name, the right-most base wins.
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

/// The properties of an anchor, with the inherited properties.
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

/// The permitted values, from the constraints on the key above.
fn values(cursor: &Cursor<'_>, key: Option<&str>) -> Vec<CompletionItem> {
    let compilation = cursor.compilation;
    let declared = key.and_then(|key| {
        let anchor = enclosing_anchor(cursor)?;
        let slot = compilation.store().anchor(anchor).slots.get(key)?.clone();
        Some((anchor, slot))
    });

    let Some((anchor, slot)) = declared.filter(|(_, slot)| !slot.constraints.is_empty()) else {
        // No constraint tells what belongs here. Thus there is no proposal. A
        // value in Piton is prose by default. A list of all anchors in the
        // project below `property: ` is a guess at a sentence. The
        // editor cannot guess that sentence.
        return Vec::new();
    };

    let mut items = Vec::new();
    for constraint in &slot.constraints {
        match &constraint.name {
            ast::TypeName::Boolean => items.extend(constants(&["true", "false"])),
            ast::TypeName::Null => items.extend(constants(&["null"])),
            // `anchor` accepts only an anchor. Thus each anchor is a candidate.
            // `complex` is different. It also accepts a list or a dictionary.
            // The user writes those. The user does not select them.
            ast::TypeName::Anchor => items.extend(anchors(cursor, None, "{")),
            ast::TypeName::Reference => items.extend(anchors(cursor, None, "@{")),
            ast::TypeName::Named(name) => {
                match constraint_type(cursor, anchor, &slot, name) {
                    Some(base) => items.extend(anchors(cursor, Some(base), "{")),
                    // A constraint that names an unresolved item accepts
                    // nothing. An offer of all items makes an answer.
                    None => {}
                }
            }
            // All other constraints describe the shape of a value. They do not
            // name the permitted values. `any`, `simple` and `complex` mean
            // "a value". The user writes a string, a number, a list or a
            // dictionary. The user does not select it. A list of all anchors
            // in the project is a guess at prose.
            _ => {}
        }
    }
    items
}

/// The anchor that the type name of a constraint refers to.
///
/// The type name is at the location of the constraint. That is not always
/// here. A property carries its constraints down the inheritance chain. The
/// names in them resolved against the imports of the file that contains them.
/// The slot records the anchor that supplied the *value*. It does not record
/// the anchor that supplied the constraint. Thus the lookup tries each file
/// in the chain.
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

/// All anchors that fit, in scope or not, written as an interpolation.
///
/// A property can have a constraint to an anchor. Then the offered name is
/// probably in a file that this file never mentioned. Thus the list also
/// includes the anchors that need an import, with the import attached.
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

/// The keys that belong in the anchor body that contains the cursor.
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
            // The user must write a property that an abstract base left
            // without a value. Thus it goes to the top of the list.
            sort_text: Some(format!("{}{name}", if required { '0' } else { '1' })),
            label_details: required.then(|| CompletionItemLabelDetails {
                description: Some("required".into()),
                ..Default::default()
            }),
            data: data_for(&Target::Property(anchor, name.clone())),
            ..Default::default()
        });
    }

    // An anchor with no content must still state this.
    items.push(CompletionItem {
        label: "pass".into(),
        kind: Some(CompletionItemKind::KEYWORD),
        detail: Some("an intentionally empty body".into()),
        sort_text: Some("2pass".into()),
        ..Default::default()
    });
    items
}

/// One completion for a name that resolved to an item.
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

/// The anchor with the body that contains the cursor.
///
/// The span of a declaration covers the written text. The text that the user
/// writes now is after its end. A body with no content at this time ends where its
/// header ends. At that time, an offer of its keys is most useful. Thus the
/// search also tries the last declaration above the cursor. It then makes sure
/// that no line between the two starts at column zero. Only such a line ends
/// an indented body.
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

/// Tells if a line between two offsets has text that is not whitespace at
/// column zero.
fn returns_to_column_zero(text: &str, from: usize, to: usize) -> bool {
    let from = from.min(to);
    text[from..to]
        .split('\n')
        // `from` is the end of a line that the check examined before.
        .skip(1)
        .any(|line| !line.is_empty() && !line.starts_with([' ', '\t']))
}

// ---------------------------------------------------------------------------
// Format, fold, selection
// ---------------------------------------------------------------------------

pub fn formatting(text: &str, path: &Path) -> Vec<TextEdit> {
    // The format path of the editor is autoformat. It normalizes structure,
    // but it never rewrites commented content. A format on save must not do
    // that. Only the `piton format` command normalizes comments.
    let formatted = format::autoformat(text, path);
    line_edits(text, &formatted)
}

/// The edits that change `old` into `new`, one for each run of changed lines.
///
/// One edit that replaces the full document makes the same text. But an
/// editor that applies it loses the cursor, the selections and the marks in
/// all lines, changed or not. Not all clients calculate the smaller edit
/// themselves. Thus the server sends it.
fn line_edits(old: &str, new: &str) -> Vec<TextEdit> {
    if old == new {
        return Vec::new();
    }
    let lines = LineIndex::new(old, Encoding::Utf16);
    let diff = similar::TextDiff::from_lines(old, new);
    let old_lines: Vec<&str> = old.split_inclusive('\n').collect();
    let new_lines: Vec<&str> = new.split_inclusive('\n').collect();
    let line_offset = |line: usize| -> usize {
        old_lines[..line.min(old_lines.len())].iter().map(|line| line.len()).sum()
    };

    let mut edits = Vec::new();
    for op in diff.ops() {
        let (old_range, new_range) = match *op {
            similar::DiffOp::Equal { .. } => continue,
            similar::DiffOp::Delete { old_index, old_len, new_index } => {
                (old_index..old_index + old_len, new_index..new_index)
            }
            similar::DiffOp::Insert { old_index, new_index, new_len } => {
                (old_index..old_index, new_index..new_index + new_len)
            }
            similar::DiffOp::Replace { old_index, old_len, new_index, new_len } => {
                (old_index..old_index + old_len, new_index..new_index + new_len)
            }
        };
        let start = line_offset(old_range.start);
        let end = line_offset(old_range.end);
        edits.push(TextEdit {
            range: lines.range(Span::new(start, end)),
            new_text: new_lines[new_range].concat(),
        });
    }
    edits
}

/// The indentation of a new line, after the newline that opened it.
///
/// A declaration or a key that ends in a colon opens a block. The block is
/// the indented lines below it. Thus enter at that line must go one level in,
/// not back to the margin. All other lines keep their current indentation.
///
/// The language server does this, not the editor, because the grammar cannot
/// express it. Indentation is intentionally outside the grammar. Thus no node
/// spans a body, so a tree-sitter indent query has nothing to measure. Also,
/// editors without tree-sitter have no other data.
pub fn on_type_formatting(text: &str, position: Position, typed: &str) -> Option<Vec<TextEdit>> {
    // Only the newline is important. A client can send other characters if it
    // asks for them. No other character changes the indentation of a line.
    if typed != "\n" && typed != "\r\n" {
        return None;
    }
    let offset = position_to_offset(text, position);

    let line_start = text[..offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    if line_start == 0 {
        // There is no line above to supply the indentation.
        return None;
    }
    let previous = previous_line(text, line_start);

    // Inside a fenced block or an escape block, the text is verbatim. A change
    // to its indentation changes its sense.
    if verbatim_at(text, line_start) {
        return None;
    }

    let indent = if previous.is_empty() {
        indent_after_emptied_lines(text, line_start)
    } else {
        wanted_indent(previous)
    };

    // The line can have indentation at this time. Some editors copy the indentation
    // of the line above, and some calculate their own. The cursor can be at
    // any position in it, or still at the margin. Thus measure the full start
    // run, not only the part before the cursor, and replace all of it. If only
    // the part before the cursor changes, the two parts add together. Then the
    // line has two times the correct depth.
    let written = text[line_start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .count();
    let existing = &text[line_start..line_start + indent_bytes(&text[line_start..], written)];
    let lines = LineIndex::new(text, Encoding::Utf16);
    if existing == indent {
        return None;
    }

    Some(vec![TextEdit {
        range: lines.range(Span::new(line_start, line_start + existing.len())),
        new_text: indent,
    }])
}

/// The indentation after a line that is fully empty.
///
/// Some editors, for example Zed, remove the whitespace from a blank line when
/// Enter leaves it. Thus an indented blank line comes to the server as an empty
/// line. It looks like it is at the margin. Thus the server measures an
/// empty line from the last line above with content. It takes the depth that
/// that line opens, less one level for each empty line after it. That is the
/// depth of the blank lines if they had kept their indentation.
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

/// The line above the line that starts at `line_start`, without its newline.
fn previous_line(text: &str, line_start: usize) -> &str {
    let above = &text[..line_start - 1];
    let start = above.rfind('\n').map(|index| index + 1).unwrap_or(0);
    above[start..].trim_end_matches('\r')
}

/// The indentation of the line after `previous`.
///
/// Two rules apply. The line that the cursor leaves decides which rule fires.
/// A declaration or key that ends in a colon opens a block. Thus the line
/// after it goes one level in. A blank line is the opposite case. Enter on a
/// blank line inside an indented block moves the new line *out* one level.
/// Thus a walk down through blank lines leaves the nested levels. It
/// does not stay at the start depth.
fn wanted_indent(previous: &str) -> String {
    let width = previous
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .count();

    // The cursor leaves a blank, indented line. Thus the new line moves
    // one level to the left. A blank line at the margin has no level to leave. Thus
    // the next rule applies and the line does not move.
    if previous.trim().is_empty() && width > 0 {
        return " ".repeat(width.saturating_sub(format::INDENT));
    }

    let width = if opens_a_block(previous) {
        width + format::INDENT
    } else {
        width
    };
    // Four spaces. This is not configurable. `piton format` always writes four
    // spaces, also when a file uses tabs. Thus any other value only adds work.
    " ".repeat(width)
}

/// Tells if the line ends with the start of a block for the lines below it. The
/// parser decides; see [`piton_syntax::cursor::opens_block`].
fn opens_a_block(line: &str) -> bool {
    piton_syntax::cursor::opens_block(line)
}

/// The number of bytes in `chars` characters of whitespace at the start.
fn indent_bytes(line: &str, chars: usize) -> usize {
    line.char_indices()
        .nth(chars)
        .map(|(index, _)| index)
        .unwrap_or(line.len())
}

/// The line where a span ends. This does not include the blank lines that the
/// span contains after its last line of content.
fn last_content_line(lines: &LineIndex<'_>, span: Span) -> u32 {
    let text = &lines.text()[span.start..span.end.min(lines.text().len())];
    lines.position(span.start + text.trim_end().len()).line
}

fn fold(start: u32, end: u32, kind: Option<FoldingRangeKind>) -> FoldingRange {
    FoldingRange {
        start_line: start,
        end_line: end,
        kind,
        ..Default::default()
    }
}

/// `FoldingRange` values, from the syntax of the buffer.
///
/// These fold: declarations, properties with a block value, escape blocks, and
/// runs of comment or import lines. An escape block directly inside a code
/// fence folds with the fence, because the two read as one. Each range is full
/// lines. Thus a client that folds by line loses nothing.
pub fn folding(text: &str, path: &Path) -> Vec<FoldingRange> {
    let parse = piton_syntax::parser::parse(text, path);
    let lines = LineIndex::new(text, Encoding::Utf16);
    let mut out = Vec::new();

    for node in parse.syntax().descendants() {
        let range = node.text_range();
        let span = Span::new(usize::from(range.start()), usize::from(range.end()));
        let foldable = match node.kind() {
            SyntaxKind::ANCHOR_DECL | SyntaxKind::ERROR => true,
            SyntaxKind::PROPERTY | SyntaxKind::VARIABLE_DECL => node
                .descendants()
                .any(|child| child.kind() == SyntaxKind::BLOCK),
            _ => false,
        };
        if foldable {
            let start = lines.position(span.start).line;
            let end = last_content_line(&lines, span);
            if end > start {
                out.push(fold(start, end, None));
            }
        }
        if node.kind() == SyntaxKind::ESCAPE_BLOCK {
            let mut start = lines.position(span.start).line;
            let mut end = last_content_line(&lines, span);
            let is_fence = |line: u32| lines.line_text(line as usize).trim_start().starts_with("```");
            if start > 0 && is_fence(start - 1) {
                start -= 1;
                if is_fence(end + 1) {
                    end += 1;
                }
            }
            if end > start {
                out.push(fold(start, end, None));
            }
        }
    }

    // Runs of full-line comments, and runs of imports.
    let mut runs: Vec<(u32, u32, FoldingRangeKind)> = Vec::new();
    let extend = |line: u32, last: u32, kind: FoldingRangeKind, runs: &mut Vec<_>| {
        match runs.last_mut() {
            Some((_, end, previous)) if *previous == kind && *end + 1 >= line => *end = last,
            _ => runs.push((line, last, kind)),
        }
    };
    for element in parse.syntax().descendants_with_tokens() {
        match element.kind() {
            SyntaxKind::COMMENT => {
                let line = lines.position(usize::from(element.text_range().start())).line;
                extend(line, line, FoldingRangeKind::Comment, &mut runs);
            }
            SyntaxKind::USE_DECL | SyntaxKind::FROM_DECL => {
                let range = element.text_range();
                let span = Span::new(usize::from(range.start()), usize::from(range.end()));
                let line = lines.position(span.start).line;
                extend(line, last_content_line(&lines, span), FoldingRangeKind::Imports, &mut runs);
            }
            _ => {}
        }
    }
    for (start, end, kind) in runs {
        if end > start {
            out.push(fold(start, end, Some(kind)));
        }
    }

    out.sort_by_key(|range| (range.start_line, std::cmp::Reverse(range.end_line)));
    out.dedup_by_key(|range| (range.start_line, range.end_line));
    out
}

/// Spans that grow outward from `offset`. First come the word, each expression
/// around it, and the interpolation. Then come the value, the property, each
/// block around it, the declaration, and the file.
fn selection_spans(parse: &piton_syntax::parser::Parse, text: &str, offset: usize) -> Vec<Span> {
    let mut spans = Vec::new();

    // The word, or the operator, below the cursor.
    let token = parse
        .syntax()
        .token_at_offset(rowan::TextSize::from(offset as u32))
        .left_biased()
        .filter(|token| !matches!(token.kind(), SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE));
    let word_char = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
    let start = text[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, c)| word_char(*c))
        .last()
        .map_or(offset, |(at, _)| at);
    let end = offset
        + text[offset..]
            .find(|c: char| !word_char(c))
            .unwrap_or(text.len() - offset);
    if end > start {
        spans.push(Span::new(start, end));
    } else if let Some(token) = &token {
        let range = token.text_range();
        spans.push(Span::new(usize::from(range.start()), usize::from(range.end())));
    }

    // Expressions, the innermost first.
    let mut expressions = Vec::new();
    for item in &parse.file.items {
        let mut values: Vec<&ast::ValueNode> = Vec::new();
        match item {
            Item::Anchor(decl) => collect_values(&decl.body, &mut values),
            Item::Variable(decl) => values.push(&decl.value),
            _ => {}
        }
        for value in values {
            for line in value.inline.iter() {
                expressions_in_line(line, offset, &mut expressions);
            }
            if let Some(block) = &value.block {
                for entry in &block.items {
                    match entry {
                        ast::BlockItem::Merge(merge) => {
                            expressions_in_line(&merge.value, offset, &mut expressions)
                        }
                        ast::BlockItem::Prose(paragraph) => {
                            for line in &paragraph.lines {
                                expressions_in_line(line, offset, &mut expressions);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    expressions.sort_by_key(|span: &Span| span.len());
    spans.extend(expressions);

    // The syntax around the expression.
    if let Some(token) = token {
        for node in token.parent_ancestors() {
            let wanted = matches!(
                node.kind(),
                SyntaxKind::INTERPOLATION
                    | SyntaxKind::PROSE_LINE
                    | SyntaxKind::VALUE
                    | SyntaxKind::PROPERTY
                    | SyntaxKind::VARIABLE_DECL
                    | SyntaxKind::LIST_ITEM
                    | SyntaxKind::MERGE_ITEM
                    | SyntaxKind::BLOCK
                    | SyntaxKind::ANCHOR_DECL
                    | SyntaxKind::USE_DECL
                    | SyntaxKind::FROM_DECL
                    | SyntaxKind::ERROR
            );
            if wanted {
                let range = node.text_range();
                spans.push(trimmed(
                    text,
                    Span::new(usize::from(range.start()), usize::from(range.end())),
                ));
            }
        }
    }
    spans.push(trimmed(text, Span::new(0, text.len())));

    // Each step must be wider than the step before it.
    let mut out: Vec<Span> = Vec::new();
    for span in spans {
        let widens = out
            .last()
            .is_none_or(|last| span.start <= last.start && span.end >= last.end && span != *last);
        if widens {
            out.push(span);
        }
    }
    out
}

fn collect_values<'a>(block: &'a ast::Block, out: &mut Vec<&'a ast::ValueNode>) {
    for entry in &block.items {
        let value = match entry {
            ast::BlockItem::Property(property) => &property.value,
            ast::BlockItem::ListItem(item) => &item.value,
            _ => continue,
        };
        out.push(value);
        for nested in value.inline_list.iter().flatten() {
            out.push(nested);
        }
        if let Some(block) = &value.block {
            collect_values(block, out);
        }
    }
}

fn expressions_in_line(line: &ast::ProseLine, offset: usize, out: &mut Vec<Span>) {
    for segment in &line.segments {
        if let ast::ProseSegment::Interpolation(interpolation) = segment {
            if interpolation.span.contains(offset) {
                expressions_in(&interpolation.expr, offset, out);
            }
        }
    }
}

fn expressions_in(expr: &Expr, offset: usize, out: &mut Vec<Span>) {
    if !(expr.span.contains(offset) || expr.span.end == offset) {
        return;
    }
    out.push(expr.span);
    match &expr.kind {
        ExprKind::Field(base, _) => expressions_in(base, offset, out),
        ExprKind::Unary(_, operand) | ExprKind::Paren(operand) | ExprKind::Nested(_, operand) => {
            expressions_in(operand, offset, out)
        }
        ExprKind::Binary(_, left, right) => {
            expressions_in(left, offset, out);
            expressions_in(right, offset, out);
        }
        ExprKind::Ternary(condition, yes, no) => {
            for part in [condition, yes, no] {
                expressions_in(part, offset, out);
            }
        }
        ExprKind::List(items) => {
            for item in items {
                expressions_in(item, offset, out);
            }
        }
        _ => {}
    }
}

pub fn selection_ranges(text: &str, path: &Path, positions: &[Position]) -> Vec<SelectionRange> {
    let parse = piton_syntax::parser::parse(text, path);
    let lines = LineIndex::new(text, Encoding::Utf16);
    positions
        .iter()
        .map(|position| {
            let offset = lines.offset(*position);
            let mut range: Option<SelectionRange> = None;
            for span in selection_spans(&parse, text, offset).into_iter().rev() {
                range = Some(SelectionRange {
                    range: lines.range(span),
                    parent: range.map(Box::new),
                });
            }
            range.unwrap_or(SelectionRange {
                range: Range {
                    start: *position,
                    end: *position,
                },
                parent: None,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Code actions
// ---------------------------------------------------------------------------

pub fn code_actions(world: &World, uri: &Url, range: Range) -> Option<CodeActionResponse> {
    code_actions_for(world, uri, range, None)
}

/// The actions for a range. Each action belongs to an item in the range: an
/// unresolved name, the header line of a declaration, or an import. Source
/// actions for the full file come only when the client asks for that kind.
/// The organize-imports action also comes when the range is on an import.
/// Prose has no actions.
pub fn code_actions_for(
    world: &World,
    uri: &Url,
    range: Range,
    only: Option<&[CodeActionKind]>,
) -> Option<CodeActionResponse> {
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

        // Offer to import the symbol from each location that exports it.
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

        // If not, offer to create it.
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

    let requested = |kind: &CodeActionKind| {
        only.is_some_and(|kinds| {
            kinds
                .iter()
                .any(|wanted| kind.as_str().starts_with(wanted.as_str()))
        })
    };
    let touches = |span: Span| span.start <= end && span.end >= start;
    let header = |span: Span| {
        let header_end = cursor.text[span.start..]
            .find('\n')
            .map(|offset| span.start + offset)
            .unwrap_or(span.end);
        Span::new(span.start, header_end)
    };
    let items = &compilation.graph().get(cursor.module).ast().items;

    // Offer to export a declaration that no file outside this file can see.
    // The offer comes from its header line, not from any location in its value.
    for item in items {
        let (name, span, exported) = match item {
            Item::Anchor(decl) => (&decl.name, decl.span, decl.exported),
            Item::Variable(decl) => (&decl.name, decl.span, decl.exported),
            _ => continue,
        };
        if exported || !touches(header(span)) {
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

    let on_import = items
        .iter()
        .any(|item| matches!(item, Item::Use(_) | Item::From(_)) && touches(item.span()));
    let organize = on_import || requested(&CodeActionKind::SOURCE_ORGANIZE_IMPORTS);
    if let Some(edits) = organize.then(|| organize_import_edits(world, &cursor)).flatten() {
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

    let format_edits = formatting(&cursor.text, &cursor.path);
    if requested(&CodeActionKind::SOURCE) && !format_edits.is_empty() {
        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: "Format this file".into(),
            kind: Some(CodeActionKind::SOURCE),
            edit: Some(workspace_edit(
                world,
                HashMap::from([(cursor.path.clone(), format_edits)]),
            )),
            ..Default::default()
        }));
    }

    // When the client accepts versions, each edit has a version. Thus the
    // client refuses a fix for an older buffer. It does not apply it incorrectly.
    for action in &mut actions {
        if let CodeActionOrCommand::CodeAction(action) = action {
            if let Some(edit) = action.edit.take() {
                action.edit = Some(versioned(world, edit));
            }
        }
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

/// A local name for `name` from `path` that shows its origin. `./lib/Type`
/// gives `LibType`, and `./other` gives `OtherButton`.
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

/// "Qualify an ambiguous reference". This gives an alias to one of the imports
/// of an ambiguous name. It then names that import at the reference.
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
            // All references used the declaration that won. Thus a change to that
            // declaration rewrites all of them. A change to a different declaration
            // rewrites only the reference where the user asked for the action.
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

/// "Resolve a simple inheritance conflict". Two bases have different values for
/// a property. The action selects the base that wins. It can change the order
/// of `extends`, or write the property on the child. For abstracts with
/// constraints that cannot both be true, it removes one of the abstracts.
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
        // The user asked from the header line of the declaration.
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
                // Remove all of ` extends ...`.
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

        // Abstracts with constraints that cannot both be true. The compiler
        // reports this. The fix is to implement one of them, not both.
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

        // Properties that more than one base supplies, with different values.
        // The child does not set them itself.
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
                // The base wins when it moves to the last position in `extends`.
                // This is not true when the winner came from a keyword, which is
                // always last.
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

/// The path text that an import uses to reach `target` from `from`.
fn import_path(from: &Path, target: &Path, _source_root: &Path) -> String {
    if target.to_string_lossy().starts_with('@') {
        return target.to_string_lossy().to_string();
    }
    let directory = from.parent().unwrap_or(Path::new("."));
    let stripped = target.with_extension("");
    let relative = piton_compile::module::relative_path(directory, &stripped);
    let text = relative.to_string_lossy().replace('\\', "/");
    // An import of `index` uses the name of its directory.
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

/// The location of a new import declaration: after the last import.
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

/// Inlay hints. They show only data that the source does not show.
///
/// There are two kinds. The first kind goes after a name with no constraint type.
/// The value of that name is a single `{...}`. It shows the type of the result
/// in the form of a constraint (`:: number`). Thus when the user accepts the
/// hint, the type becomes visible in the source. The second kind goes after a member that the
/// source reads through `this`, `super` or an anchor. It shows `← Owner` when
/// the value comes from a different anchor than the named anchor.
pub fn inlay_hints(world: &World, uri: &Url, range: Range) -> Option<Vec<InlayHint>> {
    let cursor = file_context(world, uri)?;
    let compilation = cursor.compilation;
    let store = compilation.store();
    let lines = LineIndex::new(&cursor.text, Encoding::Utf16);
    let window = lines.span(range);
    let visible = |span: Span| span.end >= window.start && span.start <= window.end;
    let mut out = Vec::new();

    let ast = compilation.graph().get(cursor.module).ast();
    for (item_index, item) in ast.items.iter().enumerate() {
        match item {
            Item::Anchor(decl) => {
                let Some(anchor) = store
                    .anchors
                    .iter()
                    .find(|def| def.module == cursor.module && def.item == item_index)
                    .map(|def| def.id)
                else {
                    continue;
                };
                let def = store.anchor(anchor);
                for property in decl.body.properties() {
                    if !visible(property.name_span) {
                        continue;
                    }
                    let typed = def
                        .slots
                        .get(&property.name)
                        .is_none_or(|slot| !slot.constraints.is_empty());
                    if typed || !is_sole_expression(&property.value) {
                        continue;
                    }
                    if let Some(value) = def.properties.get(&property.name) {
                        out.extend(type_hint(world, &cursor, &lines, property.name_span, value));
                    }
                }
                let mut prose = Vec::new();
                prose_lines(&decl.body, &mut prose);
                for line in prose {
                    if visible(line.span) {
                        supplier_hints(world, &cursor, &lines, anchor, line, &mut out);
                    }
                }
            }
            Item::Variable(decl) => {
                if !visible(decl.name_span) || !is_sole_expression(&decl.value) {
                    continue;
                }
                let Some(variable) = store
                    .variables
                    .iter()
                    .find(|def| def.module == cursor.module && def.name_span == decl.name_span)
                else {
                    continue;
                };
                if !variable.constraints.is_empty() {
                    continue;
                }
                if let Some(value) = &variable.value {
                    out.extend(type_hint(world, &cursor, &lines, decl.name_span, value));
                }
            }
            _ => {}
        }
    }
    out.sort_by_key(|hint| (hint.position.line, hint.position.character));
    Some(out)
}

/// Tells if a value is only one `{...}`. The source does not write the type of
/// this form. `${...}`, `#{...}` and `@{...}` show their types.
fn is_sole_expression(value: &ast::ValueNode) -> bool {
    value.block.is_none()
        && value.inline_list.is_none()
        && value
            .inline
            .as_ref()
            .and_then(ast::ProseLine::sole_interpolation)
            .is_some_and(|interpolation| interpolation.sigil == ast::Sigil::Standard)
}

/// The type of a value, as a constraint names it, and the anchor when the
/// value is an anchor.
fn inferred_type(compilation: &Compilation, value: &piton_core::Value) -> (String, Option<AnchorId>) {
    use piton_core::Value;
    let simple = |value: &Value| -> Option<&'static str> {
        Some(match value {
            Value::Str(_) => "string",
            Value::Number(_) => "number",
            Value::Bool(_) => "boolean",
            _ => return None,
        })
    };
    match value {
        Value::Null => ("null".into(), None),
        Value::Dict(_) => ("dictionary".into(), None),
        Value::Reference(_) => ("reference".into(), None),
        Value::Anchor(anchor) => (compilation.store().anchor(*anchor).name.clone(), Some(*anchor)),
        Value::List(items) => {
            let kinds: HashSet<Option<&str>> = items.iter().map(simple).collect();
            match kinds.into_iter().collect::<Vec<_>>().as_slice() {
                [Some(kind)] if !items.is_empty() => (format!("{kind}[]"), None),
                _ => ("list".into(), None),
            }
        }
        Value::Mixed(_) => ("list".into(), None),
        other => (simple(other).unwrap_or("string").into(), None),
    }
}

fn type_hint(
    world: &World,
    cursor: &Cursor<'_>,
    lines: &LineIndex<'_>,
    name: Span,
    value: &piton_core::Value,
) -> Option<InlayHint> {
    let compilation = cursor.compilation;
    let (kind, anchor) = inferred_type(compilation, value);
    let text = format!(":: {kind}");
    let position = lines.position(name.end);
    // An inserted hint is correct only where its name identifies that anchor.
    let writable = match anchor {
        Some(anchor) => matches!(
            compilation.resolution.lookup(cursor.module, &kind),
            Some(Symbol::Anchor(found)) if found == anchor
        ),
        None => true,
    };
    let label = match anchor.and_then(|anchor| definition_location(world, &Target::Anchor(anchor))) {
        Some(location) => InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
            value: text.clone(),
            location: Some(location),
            ..Default::default()
        }]),
        None => InlayHintLabel::String(text.clone()),
    };
    Some(InlayHint {
        position,
        label,
        kind: Some(InlayHintKind::TYPE),
        text_edits: writable.then(|| {
            vec![TextEdit {
                range: Range {
                    start: position,
                    end: position,
                },
                new_text: text,
            }]
        }),
        tooltip: None,
        padding_left: Some(false),
        padding_right: Some(false),
        data: None,
    })
}

/// All lines of prose in a block, at all depths: values, list items,
/// composition lines, paragraphs.
fn prose_lines<'a>(block: &'a ast::Block, out: &mut Vec<&'a ast::ProseLine>) {
    fn value<'a>(node: &'a ast::ValueNode, out: &mut Vec<&'a ast::ProseLine>) {
        out.extend(node.inline.iter());
        for item in node.inline_list.iter().flatten() {
            value(item, out);
        }
        if let Some(block) = &node.block {
            prose_lines(block, out);
        }
    }
    for entry in &block.items {
        match entry {
            ast::BlockItem::Property(property) => value(&property.value, out),
            ast::BlockItem::ListItem(item) => value(&item.value, out),
            ast::BlockItem::Merge(merge) => out.push(&merge.value),
            ast::BlockItem::Prose(paragraph) => out.extend(paragraph.lines.iter()),
            _ => {}
        }
    }
}

/// `← Owner` after each member with a value from a different anchor. That
/// anchor is not the anchor that the source reads the member through.
fn supplier_hints(
    world: &World,
    cursor: &Cursor<'_>,
    lines: &LineIndex<'_>,
    anchor: AnchorId,
    line: &ast::ProseLine,
    out: &mut Vec<InlayHint>,
) {
    fn walk<'a>(expr: &'a Expr, out: &mut Vec<(&'a Expr, &'a piton_syntax::ast::Spanned<String>)>) {
        match &expr.kind {
            ExprKind::Field(base, field) => {
                out.push((base, field));
                walk(base, out);
            }
            ExprKind::Unary(_, inner) | ExprKind::Paren(inner) | ExprKind::Nested(_, inner) => {
                walk(inner, out)
            }
            ExprKind::Binary(_, left, right) => {
                walk(left, out);
                walk(right, out);
            }
            ExprKind::Ternary(a, b, c) => {
                walk(a, out);
                walk(b, out);
                walk(c, out);
            }
            ExprKind::List(items) => items.iter().for_each(|item| walk(item, out)),
            _ => {}
        }
    }
    let compilation = cursor.compilation;
    let store = compilation.store();
    for segment in &line.segments {
        let ast::ProseSegment::Interpolation(interpolation) = segment else {
            continue;
        };
        let mut fields = Vec::new();
        walk(&interpolation.expr, &mut fields);
        for (base, field) in fields {
            // The anchor that the source reads the member through, and the
            // anchor that supplies it. `self` is the anchor that the compiler
            // compiles. Thus it has no single answer.
            let (named, supplier) = match &base.kind {
                ExprKind::This => {
                    let Some(slot) = store.anchor(anchor).slots.get(&field.value) else {
                        continue;
                    };
                    (Some(anchor), slot.owner)
                }
                ExprKind::Super => {
                    let Some(base) = store
                        .anchor(anchor)
                        .bases
                        .iter()
                        .rev()
                        .find(|base| store.anchor(**base).slots.contains_key(&field.value))
                    else {
                        continue;
                    };
                    (None, store.anchor(*base).slots[&field.value].owner)
                }
                ExprKind::Name(name) => {
                    let Some(Symbol::Anchor(found)) = compilation.resolution.lookup(cursor.module, name)
                    else {
                        continue;
                    };
                    let Some(slot) = store.anchor(found).slots.get(&field.value) else {
                        continue;
                    };
                    (Some(found), slot.owner)
                }
                _ => continue,
            };
            if named == Some(supplier) {
                continue;
            }
            let owner = store.anchor(supplier);
            let location =
                definition_location(world, &Target::Property(supplier, field.value.clone()));
            out.push(InlayHint {
                position: lines.position(field.span.end),
                label: InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
                    value: format!("\u{2190} {}", owner.name),
                    location,
                    ..Default::default()
                }]),
                kind: None,
                text_edits: None,
                tooltip: None,
                padding_left: Some(true),
                padding_right: Some(false),
                data: None,
            });
        }
    }
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
    pub const NAMESPACE: u32 = 4;
}

/// Token modifier bits, in the order of [`TOKEN_MODIFIERS`].
mod modifier {
    pub const DECLARATION: u32 = 1 << 0;
    pub const ABSTRACT: u32 = 1 << 1;
    pub const DEFAULT_LIBRARY: u32 = 1 << 2;
    pub const INHERITED: u32 = 1 << 3;
    pub const EXPORTED: u32 = 1 << 4;
    pub const IMPORTED: u32 = 1 << 5;
}

/// The semantic tokens of a file: one for each resolved name and module path.
///
/// The syntax definition colours all other text: comments, keywords, literals,
/// operators, prose. The server gives no tokens for it. Thus the two layers
/// cannot disagree. A token can only add the result of resolution to a name.
/// It never changes the colour of text that the grammar coloured before.
pub fn semantic_tokens(world: &World, uri: &Url) -> Option<SemanticTokensResult> {
    let cursor = file_context(world, uri)?;
    let index = world.index.get(cursor.module)?;
    let compilation = cursor.compilation;
    let store = compilation.store();
    let is_package = |module: piton_compile::ModuleId| compilation.graph().get(module).is_package();

    let mut raw: Vec<(Span, u32, u32)> = Vec::new();
    for occurrence in &index.occurrences {
        let declared = if occurrence.role == Role::Definition {
            modifier::DECLARATION
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
                // The source reads a property through `self`, `super` or an
                // anchor. If its value comes from a base, it is an inherited value.
                if occurrence.role == Role::Reference
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
                let mut modifiers = declared;
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
        raw.push((occurrence.span, kind, modifiers));
    }

    // Where two occurrences overlap, the innermost occurrence names the text.
    raw.sort_by_key(|(span, _, _)| (span.start, span.len()));
    let mut kept: Vec<(Span, u32, u32)> = Vec::new();
    for entry in raw {
        if entry.0.is_empty() {
            continue;
        }
        match kept.last() {
            Some(last) if last.0.end > entry.0.start => {
                if entry.0.len() < last.0.len() {
                    kept.pop();
                    kept.push(entry);
                }
            }
            _ => kept.push(entry),
        }
    }

    let lines = LineIndex::new(&cursor.text, Encoding::Utf16);
    let mut data = Vec::new();
    let mut previous = Position::default();
    for (span, kind, modifiers) in kept {
        let start = lines.position(span.start);
        let end = lines.position(span.end);
        // A name cannot contain a line break. Thus a token on more than one line
        // is a bad span. It is better to discard it than to send bad data.
        if end.line != start.line {
            debug_assert!(false, "a semantic token spans lines: {span:?}");
            continue;
        }
        let delta_line = start.line - previous.line;
        data.push(SemanticToken {
            delta_line,
            delta_start: if delta_line == 0 {
                start.character - previous.character
            } else {
                start.character
            },
            length: end.character - start.character,
            token_type: kind,
            token_modifiers_bitset: modifiers,
        });
        previous = start;
    }

    debug_assert!(TOKEN_TYPES.len() > token::NAMESPACE as usize && TOKEN_MODIFIERS.len() == 6);
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
        // One lens for each construct, not one for each adapter slice of the same span.
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

    // A property that overrides links to the declaration that it replaces.
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
                // A property that an abstract base requires is an implementation.
                // Only a replaced value has its own line.
                let Some(base) = replaces_value(compilation, def.id, &property.name) else {
                    continue;
                };
                let Some(location) =
                    definition_location(world, &Target::Property(base, property.name.clone()))
                else {
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

/// The command of a source-to-output code lens. It opens the compiled file at
/// the slice that the construct made.
pub const SOURCE_TO_OUTPUT: &str = "piton.sourceToOutput";
/// The command of an "overrides" code lens. It opens a location.
pub const SHOW_LOCATION: &str = "piton.showLocation";

/// The result of a command. It shows a document, or it tells the user why it
/// cannot.
#[derive(Debug, PartialEq)]
pub enum CommandOutcome {
    Show(ShowDocumentParams),
    Message(String),
}

/// `workspace/executeCommand` for the commands of the code lenses of the server.
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

/// `piton/sourceToOutput`: the compiled slices that a source position made.
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

/// `piton/outputToSource`: the source construct that made a compiled offset.
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

// ---------------------------------------------------------------------------
// Hierarchy
//
// Inheritance is the structure of a specbase. An abstract anchor declares a
// shape. Keywords are inheritance in the form of syntax. A concrete anchor
// resolves to the flat merge of the full chain above it. Two navigations come
// from this, and they answer opposite questions.
//
// `implementation` descends one step: from a base to all anchors that extend
// it. Go-to-definition ascends. Without `implementation`, the user can
// move along the relation in only one direction. That is the wrong direction
// for the question "which anchors use this shape?".
//
// The type hierarchy goes in the two directions and continues. Thus an editor
// can show the tree, not only one step of it.
// ---------------------------------------------------------------------------

/// The anchor below the cursor, for all names that the source uses for it.
///
/// A keyword *is* the anchor that it aliases. Thus `skill` and `Skill` must
/// give the same answer.
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

/// All anchors that directly name `base` in their bases.
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
/// From a base anchor, it gives the anchors that extend it, then the anchors
/// that compose it with `{X}`. From a property, it gives the declarations in
/// descendants that override it. This is the opposite direction of
/// go-to-definition on a property that overrides.
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

/// The relation of a hierarchy item to the item that the request came from.
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
        // The item keeps the identity of the anchor. Thus a later supertypes or
        // subtypes call does not have to find it again by position. The file can have
        // changed after this call.
        data: Some(serde_json::json!({ "anchor": anchor.0 })),
    })
}

/// Reads the anchor identity from a hierarchy item.
fn hierarchy_anchor(world: &World, item: &TypeHierarchyItem) -> Option<AnchorId> {
    let compilation = world.compilation.as_ref()?;
    let id = item
        .data
        .as_ref()
        .and_then(|data| data.get("anchor"))
        .and_then(serde_json::Value::as_u64)
        .map(|id| AnchorId(id as u32))
        .filter(|id| (id.0 as usize) < compilation.store().anchors.len());
    // An old id, from an item made before a recompilation, uses the name
    // instead. The name is what the editor shows.
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

/// `typeHierarchy/supertypes`: the bases that an anchor extends, in declaration
/// order (the order that resolves collisions). Then the anchors that it composes.
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

/// `typeHierarchy/subtypes`: all anchors that extend this anchor. Then all
/// anchors that compose it.
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

/// `piton/preview`: a file, or the anchor at a position, in its compiled form.
///
/// Parameters: `uri`, an optional `position` (`{line, character}`) that names
/// an anchor to preview alone, and an optional `renderer`. The renderer is
/// `json`, `yaml`, `markdown`, or a Belay target id such as `claude`. The
/// default is the configured renderer of the project. The answer holds the
/// renderer, the output file, and the text.
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

    // All other values name a group of plugin output, for example a Belay
    // target id. This is the output that the plugin builds planned at the last save.
    let target = requested.unwrap_or_default();
    let files: Vec<&piton_plugin::output::OutputFile> = world
        .plugin_plans
        .iter()
        .flat_map(|plan| plan.files.iter())
        .filter(|file| file.group.as_deref() == Some(target))
        .filter(|file| {
            // `origin` names the anchor that a file came from. For a combined
            // file, it names all of its anchors, with commas between them.
            file.origin.split(',').map(str::trim).any(|origin| {
                compilation.store().anchors.iter().any(|def| {
                    def.name == origin
                        && paths_equal(&compilation.graph().get(def.module).path, &cursor.path)
                        && anchor.map_or(true, |anchor| anchor == def.id)
                })
            })
        })
        .collect();
    if files.is_empty() {
        let known = world.plugin_plans.iter().flat_map(|plan| plan.files.iter()).any(|file| file.group.as_deref() == Some(target));
        return failure(&if known {
            format!("the `{target}` target produces nothing from this construct")
        } else {
            format!("`{target}` is not a renderer (json, yaml, markdown), and no plugin builds a target of that name")
        });
    }
    let text = files
        .iter()
        .map(|file| format!("<!-- {} -->\n{}", file.path, String::from_utf8_lossy(&file.data)))
        .collect::<Vec<_>>()
        .join("\n\n");
    serde_json::json!({
        "renderer": target,
        "path": compilation.project.root.join(&files[0].path),
        "paths": files.iter().map(|file| compilation.project.root.join(&file.path)).collect::<Vec<_>>(),
        "text": text,
    })
}
