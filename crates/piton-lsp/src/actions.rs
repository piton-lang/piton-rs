//! Code actions: the fixes the compiler already knows how to describe.

use std::collections::HashMap;
use std::path::Path;

use piton_core::diag::Diagnostic;
use piton_core::resolve::Symbol;
use piton_core::FileId;
use piton_syntax::TextRange;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, TextEdit, Url, WorkspaceEdit,
};

use crate::tokens::collect;
use crate::world::View;

/// Offer actions for the diagnostics overlapping `range`.
pub fn actions(
    view: &View,
    file: FileId,
    url: &Url,
    range: TextRange,
) -> Vec<CodeActionOrCommand> {
    let mut out = Vec::new();
    for diagnostic in view.compilation.diagnostics.iter() {
        if diagnostic.file != file || !overlaps(diagnostic.range, range) {
            continue;
        }
        match diagnostic.code {
            "unimplemented" => out.extend(implement_missing(view, file, url, diagnostic)),
            "eval" => out.extend(add_import(view, file, url, diagnostic)),
            "unknown-keyword" => out.extend(add_use(view, file, url, diagnostic)),
            _ => {}
        }
    }
    out.push(format_action(view, file, url));
    out
}

fn overlaps(a: TextRange, b: TextRange) -> bool {
    a.start() <= b.end() && b.start() <= a.end()
}

fn action(title: String, url: &Url, edits: Vec<TextEdit>, kind: CodeActionKind) -> CodeActionOrCommand {
    let mut changes = HashMap::new();
    changes.insert(url.clone(), edits);
    CodeActionOrCommand::CodeAction(CodeAction {
        title,
        kind: Some(kind),
        edit: Some(WorkspaceEdit { changes: Some(changes), ..WorkspaceEdit::default() }),
        ..CodeAction::default()
    })
}

/// Write stubs for every abstract property a concrete anchor still owes.
fn implement_missing(
    view: &View,
    file: FileId,
    url: &Url,
    diagnostic: &Diagnostic,
) -> Vec<CodeActionOrCommand> {
    let analysis = &view.compilation.analysis;
    let hir = &analysis.db.file(file).hir;
    let Some(position) = hir.anchors.iter().position(|it| it.name_range == diagnostic.range) else {
        return Vec::new();
    };
    let anchor = &hir.anchors[position];
    let Some(id) = analysis.anchor_id(file, position) else { return Vec::new() };

    let mut defined = Vec::new();
    let mut required: Vec<(String, Option<String>)> = Vec::new();
    let mut chain = analysis.ancestors(id);
    chain.reverse();
    chain.push(id);
    for link in chain {
        let is_abstract = analysis.anchor_def(link).is_abstract;
        let mut properties = Vec::new();
        collect(&analysis.anchor_def(link).body, &mut properties);
        for property in properties {
            if is_abstract && property.node.is_empty() {
                let constraint = property.constraints.first().map(|it| it.render());
                required.push((property.name.clone(), constraint));
            } else {
                defined.push(property.name.clone());
            }
        }
    }
    required.retain(|(name, _)| !defined.contains(name));
    if required.is_empty() {
        return Vec::new();
    }

    let index = view.line_index(file);
    let text = view.text(file);
    let indent = " ".repeat(body_indent(text, anchor.range));
    let mut insertion = String::new();
    for (name, constraint) in &required {
        let placeholder = match constraint.as_deref() {
            Some("number") => "0",
            Some("boolean") => "false",
            Some("list") => "[]",
            _ => "TODO",
        };
        insertion.push_str(&format!("{indent}{name}: {placeholder}\n"));
    }
    let at = index.position(anchor.range.end());
    vec![action(
        format!(
            "Implement {} missing propert{}",
            required.len(),
            if required.len() == 1 { "y" } else { "ies" }
        ),
        url,
        vec![TextEdit { range: tower_lsp::lsp_types::Range { start: at, end: at }, new_text: insertion }],
        CodeActionKind::QUICKFIX,
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

/// Offer an import for an unresolved name that some module does export.
fn add_import(
    view: &View,
    file: FileId,
    url: &Url,
    diagnostic: &Diagnostic,
) -> Vec<CodeActionOrCommand> {
    let Some(name) = quoted_name(&diagnostic.message, "cannot find `") else { return Vec::new() };
    let analysis = &view.compilation.analysis;
    let mut out = Vec::new();
    for other in analysis.db.files() {
        if other.id == file || !analysis.scope(other.id).exports.contains_key(&name) {
            continue;
        }
        let Some(specifier) = specifier(view, file, other.id) else { continue };
        out.push(insert_line(
            view,
            file,
            url,
            format!("from {specifier} import {name}"),
            format!("Import `{name}` from `{specifier}`"),
        ));
    }
    out
}

/// Offer a `use` for a keyword some module exports.
fn add_use(
    view: &View,
    file: FileId,
    url: &Url,
    diagnostic: &Diagnostic,
) -> Vec<CodeActionOrCommand> {
    let Some(keyword) = quoted_name(&diagnostic.message, "`") else { return Vec::new() };
    let analysis = &view.compilation.analysis;
    let mut out = Vec::new();
    for other in analysis.db.files() {
        if other.id == file {
            continue;
        }
        let provides = analysis.scope(other.id).exports.values().any(|symbol| match symbol {
            Symbol::Anchor(id) => {
                analysis.anchor_def(*id).keyword.as_ref().is_some_and(|it| it.value == keyword)
            }
            Symbol::Var { .. } => false,
        });
        if !provides {
            continue;
        }
        let Some(specifier) = specifier(view, file, other.id) else { continue };
        out.push(insert_line(
            view,
            file,
            url,
            format!("use {specifier}"),
            format!("Add `use {specifier}` for the `{keyword}` keyword"),
        ));
    }
    out
}

/// Insert a statement above the first declaration in a file.
fn insert_line(
    view: &View,
    file: FileId,
    url: &Url,
    line: String,
    title: String,
) -> CodeActionOrCommand {
    let index = view.line_index(file);
    let hir = &view.compilation.analysis.db.file(file).hir;
    let after = hir
        .imports
        .iter()
        .map(|it| it.range.end())
        .chain(hir.reexports.iter().map(|it| it.range.end()))
        .max();
    let (at, text) = match after {
        Some(end) => (index.position(end), format!("{line}\n")),
        None => (
            tower_lsp::lsp_types::Position { line: 0, character: 0 },
            format!("{line}\n\n"),
        ),
    };
    action(
        title,
        url,
        vec![TextEdit { range: tower_lsp::lsp_types::Range { start: at, end: at }, new_text: text }],
        CodeActionKind::QUICKFIX,
    )
}

/// Rewrite the whole document with `piton format`.
fn format_action(view: &View, file: FileId, url: &Url) -> CodeActionOrCommand {
    let index = view.line_index(file);
    let formatted = piton_fmt::format(view.text(file));
    action(
        "Format with piton format".to_string(),
        url,
        vec![TextEdit { range: index.full_range(), new_text: formatted }],
        CodeActionKind::SOURCE,
    )
}

/// The import specifier that reaches `target` from `file`.
fn specifier(view: &View, file: FileId, target: FileId) -> Option<String> {
    let source = view.compilation.analysis.db.file(target).source.clone();
    match source {
        piton_core::db::Source::Virtual(name) => Some(name),
        piton_core::db::Source::Disk(path) => {
            let from = view.path_of(file)?.parent()?.to_path_buf();
            let stem = path.with_extension("");
            Some(format!("./{}", relative(&from, &stem)?))
        }
    }
}

/// A `/`-separated path from `from` to `to`, using `..` when it must.
fn relative(from: &Path, to: &Path) -> Option<String> {
    let mut base: Vec<_> = from.components().collect();
    let target: Vec<_> = to.components().collect();
    let shared = base.iter().zip(&target).take_while(|(a, b)| a == b).count();
    base.truncate(shared);
    let mut parts: Vec<String> =
        std::iter::repeat_n("..".to_string(), from.components().count() - shared).collect();
    parts.extend(target[shared..].iter().map(|it| it.as_os_str().to_string_lossy().to_string()));
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Pull the first backtick-quoted name out of a diagnostic message.
fn quoted_name(message: &str, prefix: &str) -> Option<String> {
    let start = message.find(prefix)? + prefix.len();
    let rest = &message[start..];
    let end = rest.find('`')?;
    Some(rest[..end].to_string())
}
