//! Code actions: fixes for what the compiler reports, and tidying imports.
//!
//! Formatting is not among them. The editor has a formatting command already,
//! and an action offered on every line is a lightbulb nobody looks at.

use std::collections::HashSet;

use piton_core::diag::Diagnostic;
use piton_core::resolve::Symbol;
use piton_core::specifier::segments;
use piton_core::value::AnchorId;
use piton_core::FileId;
use piton_syntax::{TextRange, TextSize};
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, Range, TextEdit, Url, WorkspaceEdit,
};

use crate::imports::{self, ImportFix};
use crate::index::{Model, Sym};
use crate::world::View;

/// The kind of the action that removes every unused import, as TypeScript
/// names it, so an editor configured to run it on save finds it here too.
pub const REMOVE_UNUSED_IMPORTS: CodeActionKind = CodeActionKind::new("source.removeUnusedImports");

/// The actions for `range` in `file`, limited to the kinds in `only` when the
/// editor asks for some.
pub fn actions(
    view: &View,
    file: FileId,
    url: &Url,
    range: TextRange,
    only: Option<&[CodeActionKind]>,
) -> Vec<CodeActionOrCommand> {
    let wanted = |kind: &CodeActionKind| {
        only.map_or(true, |kinds| {
            kinds.iter().any(|it| {
                kind.as_str() == it.as_str() || kind.as_str().starts_with(&format!("{}.", it.as_str()))
            })
        })
    };
    let model = view.model();
    let unused = imports::unused(view, file);
    let mut out: Vec<CodeAction> = Vec::new();

    if wanted(&CodeActionKind::QUICKFIX) {
        for diagnostic in view.compilation.diagnostics.iter() {
            if diagnostic.file != file || !overlaps(diagnostic.range, range) {
                continue;
            }
            match diagnostic.code {
                "unimplemented" => out.extend(implement_missing(view, &model, file, url, diagnostic)),
                "eval" if diagnostic.message.starts_with("cannot find `") => {
                    out.extend(import_fixes(view, &model, file, url, diagnostic, false))
                }
                "unknown-base" | "unknown-type" => {
                    out.extend(import_fixes(view, &model, file, url, diagnostic, true))
                }
                "unknown-keyword" => out.extend(use_fixes(view, &model, file, url, diagnostic)),
                _ => {}
            }
        }
        for entry in unused.iter().filter(|it| overlaps(it.range, range)) {
            let edits = imports::removal(view, file, std::slice::from_ref(entry));
            out.push(action(entry.title.clone(), url, edits, CodeActionKind::QUICKFIX, false));
        }
    }

    if !unused.is_empty() && wanted(&REMOVE_UNUSED_IMPORTS) {
        let edits = imports::removal(view, file, &unused);
        out.push(action("Remove unused imports".to_string(), url, edits, REMOVE_UNUSED_IMPORTS, false));
    }
    if wanted(&CodeActionKind::SOURCE_ORGANIZE_IMPORTS) {
        let edits = imports::organize(view, file);
        if !edits.is_empty() {
            out.push(action(
                "Organize imports".to_string(),
                url,
                edits,
                CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
                false,
            ));
        }
    }

    // The same fix offered for several diagnostics is one action.
    let mut seen = HashSet::new();
    out.retain(|it| seen.insert(it.title.clone()));
    out.into_iter().map(CodeActionOrCommand::CodeAction).collect()
}

fn overlaps(a: TextRange, b: TextRange) -> bool {
    a.start() <= b.end() && b.start() <= a.end()
}

fn action(title: String, url: &Url, edits: Vec<TextEdit>, kind: CodeActionKind, preferred: bool) -> CodeAction {
    let changes = std::collections::HashMap::from([(url.clone(), edits)]);
    CodeAction {
        title,
        kind: Some(kind),
        edit: Some(WorkspaceEdit { changes: Some(changes), ..WorkspaceEdit::default() }),
        is_preferred: preferred.then_some(true),
        ..CodeAction::default()
    }
}

/// The first backtick-quoted name in a compiler message.
fn quoted(message: &str) -> Option<String> {
    let start = message.find('`')? + 1;
    let end = start + message[start..].find('`')?;
    Some(message[start..end].to_string())
}

/// Import the name a diagnostic says cannot be found, from each module that
/// exports something by that name, the preferred specifier first.
fn import_fixes(
    view: &View,
    model: &Model,
    file: FileId,
    url: &Url,
    diagnostic: &Diagnostic,
    anchors_only: bool,
) -> Vec<CodeAction> {
    let Some(name) = quoted(&diagnostic.message) else { return Vec::new() };
    let analysis = model.analysis();
    let mut symbols: Vec<Sym> = Vec::new();
    for module in analysis.db.files() {
        if module.id == file {
            continue;
        }
        let Some(sym) = model.origin(module.id, &name) else { continue };
        if anchors_only && model.anchor_of(&sym).is_none() {
            continue;
        }
        if !symbols.contains(&sym) {
            symbols.push(sym);
        }
    }
    let mut fixes: Vec<ImportFix> =
        symbols.iter().filter_map(|sym| imports::import_fix(view, model, file, &name, sym)).collect();
    fixes.sort_by(|a, b| {
        b.extends_existing
            .cmp(&a.extends_existing)
            .then(segments(&a.specifier).cmp(&segments(&b.specifier)))
            .then(a.specifier.cmp(&b.specifier))
    });
    fixes
        .into_iter()
        .enumerate()
        .map(|(position, fix)| {
            let title = match fix.extends_existing {
                true => format!("Add `{name}` to the import from `{}`", fix.specifier),
                false => format!("Import `{name}` from `{}`", fix.specifier),
            };
            action(title, url, vec![fix.edit], CodeActionKind::QUICKFIX, position == 0)
        })
        .collect()
}

/// Bring in the keyword a diagnostic says the file cannot see.
fn use_fixes(view: &View, model: &Model, file: FileId, url: &Url, diagnostic: &Diagnostic) -> Vec<CodeAction> {
    let Some(keyword) = quoted(&diagnostic.message) else { return Vec::new() };
    let analysis = model.analysis();
    let mut providers: Vec<AnchorId> = Vec::new();
    for module in analysis.db.files() {
        if module.id == file {
            continue;
        }
        for symbol in analysis.scope(module.id).exports.values() {
            let Symbol::Anchor(id) = symbol else { continue };
            let declares = analysis.anchor_def(*id).keyword.as_ref().is_some_and(|it| it.value == keyword);
            if declares && !providers.contains(id) {
                providers.push(*id);
            }
        }
    }
    let mut specifiers: Vec<String> = providers
        .iter()
        .filter_map(|id| {
            imports::best_specifier(model, file, Some(model.file_of(*id)), |module| {
                analysis.scope(module).exports.values().any(|it| *it == Symbol::Anchor(*id))
            })
        })
        .collect();
    specifiers.sort_by(|a, b| segments(a).cmp(&segments(b)).then(a.cmp(b)));
    specifiers.dedup();
    specifiers
        .into_iter()
        .enumerate()
        .map(|(position, specifier)| {
            action(
                format!("Add `use {specifier}` for the `{keyword}` keyword"),
                url,
                vec![imports::use_edit(view, file, &specifier)],
                CodeActionKind::QUICKFIX,
                position == 0,
            )
        })
        .collect()
}

/// Write every abstract property a concrete anchor still owes.
fn implement_missing(
    view: &View,
    model: &Model,
    file: FileId,
    url: &Url,
    diagnostic: &Diagnostic,
) -> Vec<CodeAction> {
    let analysis = model.analysis();
    let hir = &analysis.db.file(file).hir;
    let Some(position) = hir.anchors.iter().position(|it| it.name_range == diagnostic.range) else {
        return Vec::new();
    };
    let anchor = &hir.anchors[position];
    let Some(id) = analysis.anchor_id(file, position) else { return Vec::new() };

    let mut defined: Vec<String> = Vec::new();
    let mut required: Vec<(String, Option<String>)> = Vec::new();
    let mut chain = analysis.ancestors(id);
    chain.reverse();
    chain.push(id);
    for link in chain {
        let is_abstract = analysis.anchor_def(link).is_abstract;
        for property in model.own(link).iter() {
            if is_abstract && property.node.is_empty() {
                if !required.iter().any(|(name, _)| *name == property.name) {
                    required.push((property.name.clone(), property.constraints.first().map(|it| it.render())));
                }
            } else {
                defined.push(property.name.clone());
            }
        }
    }
    required.retain(|(name, _)| !defined.contains(name));
    if required.is_empty() {
        return Vec::new();
    }

    let text = view.text(file);
    let indent = " ".repeat(body_indent(text, anchor.range));
    let at = end_of_content(text, anchor.range);
    let mut insertion = String::new();
    // An anchor on the last line of a file with no newline after it gets one,
    // so its new body starts on a line of its own.
    if !text[..at].ends_with('\n') {
        insertion.push('\n');
    }
    for (name, constraint) in &required {
        let placeholder = match constraint.as_deref() {
            Some("number") => "0",
            Some("boolean") => "false",
            Some(it) if it == "list" || it.ends_with("[]") => "[]",
            _ => "TODO",
        };
        insertion.push_str(&format!("{indent}{name}: {placeholder}\n"));
    }
    let at = view.line_index(file).position(TextSize::new(at as u32));
    vec![action(
        format!(
            "Implement {} missing propert{}",
            required.len(),
            if required.len() == 1 { "y" } else { "ies" }
        ),
        url,
        vec![TextEdit { range: Range { start: at, end: at }, new_text: insertion }],
        CodeActionKind::QUICKFIX,
        true,
    )]
}

/// The indentation an anchor's body uses, defaulting to the canonical four.
fn body_indent(text: &str, range: TextRange) -> usize {
    let body = &text[usize::from(range.start())..usize::from(range.end()).min(text.len())];
    body.lines()
        .skip(1)
        .find(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .filter(|width| *width > 0)
        .unwrap_or(4)
}

/// Just after the last line of a declaration that holds anything, so that what
/// is added lands before the blank lines a declaration can end with.
fn end_of_content(text: &str, range: TextRange) -> usize {
    let start = usize::from(range.start());
    let end = usize::from(range.end()).min(text.len());
    let content_end = start + text[start..end].trim_end().len();
    text[content_end..].find('\n').map_or(content_end, |at| content_end + at + 1)
}
