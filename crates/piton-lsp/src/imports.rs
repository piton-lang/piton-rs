//! Imports written on the author's behalf, and imports nothing uses.
//!
//! A new import is added where `spec/scope/lsp/editing/ImportSpecifiers`
//! says, through a specifier `piton-core` writes. An unused one is found with
//! the same symbol model every other feature uses, so an import is only called
//! unused when nothing in the file resolves to what it binds.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use piton_core::db::Source;
use piton_core::resolve::Symbol;
use piton_core::specifier::best;
use piton_core::FileId;
use piton_syntax::ast::{self, AstNode};
use piton_syntax::kind::SyntaxKind::{self, COMMENT, FROM_KW, NEWLINE, USE_KW};
use piton_syntax::{SyntaxNode, TextRange, TextSize};
use tower_lsp::lsp_types::TextEdit;

use crate::index::{AliasItem, Model, Role, Sym};
use crate::world::View;

/// An import that would bring a name into a file, and the edit that adds it.
#[derive(Clone, Debug)]
pub struct ImportFix {
    /// The specifier the name comes from.
    pub specifier: String,
    /// True when the name joins an import the file already has.
    pub extends_existing: bool,
    pub edit: TextEdit,
}

/// The edit that brings `sym` into `file` under `name`.
///
/// `None` when no module the project can see exports it under that name.
pub fn import_fix(view: &View, model: &Model, file: FileId, name: &str, sym: &Sym) -> Option<ImportFix> {
    let analysis = model.analysis();
    let hir = &analysis.db.file(file).hir;

    // An import from a module that already exports the name takes it.
    for (decl, import) in hir.imports.iter().enumerate() {
        let Some(module) = analysis.module(file, &import.path.value) else { continue };
        if model.origin(module, name).as_ref() != Some(sym) {
            continue;
        }
        let node = declaration(view, file, SyntaxKind::IMPORT_DECL, decl)?;
        let mut names = item_texts(&node);
        names.push(name.to_string());
        let edit = rewrite_declaration(view, file, &node, &import.path.value, "import", &names)?;
        return Some(ImportFix { specifier: import.path.value.clone(), extends_existing: true, edit });
    }

    let specifier = specifier_for(model, file, name, sym)?;
    let edit = insertion(view, file, &format!("from {specifier} import {name}"), false);
    Some(ImportFix { specifier, extends_existing: false, edit })
}

/// The specifier a new import of `sym`, under `name`, comes from.
pub fn specifier_for(model: &Model, file: FileId, name: &str, sym: &Sym) -> Option<String> {
    best_specifier(model, file, model.declaring_file(sym), |module| {
        model.origin(module, name).as_ref() == Some(sym)
    })
}

/// The best specifier, written from `file`, for any module `exports` accepts.
///
/// `declaring` is the module that declares what is being imported, which wins
/// a tie over a module that only re-exports it.
pub fn best_specifier(
    model: &Model,
    file: FileId,
    declaring: Option<FileId>,
    exports: impl Fn(FileId) -> bool,
) -> Option<String> {
    let db = &model.analysis().db;
    let here = db.file(file).source.as_path()?.to_path_buf();
    let directory = here.parent()?;
    let candidates: Vec<(String, bool)> = db
        .files()
        .filter(|module| module.id != file && exports(module.id))
        .filter_map(|module| {
            let spec = match &module.source {
                Source::Virtual(name) => name.clone(),
                Source::Disk(path) => {
                    // A file is not imported through the index of its own package.
                    if contains_as_index(path, &here) {
                        return None;
                    }
                    db.preferred_specifier(directory, path, &|path| db.exists(path))?
                }
            };
            Some((spec, declaring == Some(module.id)))
        })
        .collect();
    best(candidates)
}

/// Whether `module` is the `index.pi` of a directory that holds `file`.
fn contains_as_index(module: &Path, file: &Path) -> bool {
    module.file_name().is_some_and(|name| name == "index.pi")
        && module.parent().is_some_and(|directory| file.starts_with(directory))
}

/// The edit that adds `use {specifier}` to a file.
pub fn use_edit(view: &View, file: FileId, specifier: &str) -> TextEdit {
    insertion(view, file, &format!("use {specifier}"), true)
}

/// The edit that inserts one import line where it belongs.
fn insertion(view: &View, file: FileId, line: &str, is_use: bool) -> TextEdit {
    let text = view.text(file);
    let root = view.compilation.analysis.db.file(file).parse.root();
    let mut imports: Vec<TextRange> = Vec::new();
    let mut uses: Vec<TextRange> = Vec::new();
    for item in root.items() {
        match item {
            ast::Item::Import(it) => imports.push(it.syntax().text_range()),
            ast::Item::Reexport(it) => imports.push(it.syntax().text_range()),
            ast::Item::Use(it) => uses.push(it.syntax().text_range()),
            _ => {}
        }
    }
    let after = |ranges: &[TextRange]| ranges.iter().map(|it| it.end()).max();
    let (at, inserted) = match is_use {
        false => match after(&imports).max(after(&uses)) {
            Some(end) => (end, after_line(text, end, line)),
            None => (TextSize::new(0), top_line(text, line)),
        },
        true => match (after(&uses), imports.iter().map(|it| it.start()).min()) {
            (Some(end), _) => (end, after_line(text, end, line)),
            (None, Some(start)) => (start, format!("{line}\n")),
            (None, None) => (TextSize::new(0), top_line(text, line)),
        },
    };
    let position = view.line_index(file).position(at);
    TextEdit { range: tower_lsp::lsp_types::Range { start: position, end: position }, new_text: inserted }
}

fn after_line(text: &str, end: TextSize, line: &str) -> String {
    match text[..usize::from(end)].ends_with('\n') {
        true => format!("{line}\n"),
        false => format!("\n{line}\n"),
    }
}

fn top_line(text: &str, line: &str) -> String {
    match text.is_empty() {
        true => format!("{line}\n"),
        false => format!("{line}\n\n"),
    }
}

// ---- unused imports --------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnusedKind {
    Item { decl: usize, item: usize },
    Use { index: usize },
}

/// An import that nothing in its file uses.
#[derive(Clone, Debug)]
pub struct Unused {
    /// What to fade: the item, or the whole `use` line.
    pub range: TextRange,
    pub kind: UnusedKind,
    /// How the diagnostic names it.
    pub label: String,
    /// The code action that removes it.
    pub title: String,
}

/// Every import item and `use` line in `file` that nothing in the file uses.
pub fn unused(view: &View, file: FileId) -> Vec<Unused> {
    if view.is_virtual(file)
        || view.compilation.diagnostics.iter().any(|it| it.file == file && it.code == "syntax")
    {
        return Vec::new();
    }
    let analysis = &view.compilation.analysis;
    let hir = &analysis.db.file(file).hir;
    let model = view.model();
    let index = view.index();
    let used: HashSet<&Sym> =
        index.in_file(file).iter().filter(|it| it.role == Role::Reference).map(|it| &it.sym).collect();
    let mut out = Vec::new();

    for (decl, import) in hir.imports.iter().enumerate() {
        let Some(module) = analysis.module(file, &import.path.value) else { continue };
        for (item, entry) in import.items.iter().enumerate() {
            let Some(origin) = model.origin(module, &entry.name.value) else { continue };
            let binding = match entry.alias {
                Some(_) => Sym::Alias { file, item: AliasItem::Import { decl, item } },
                None => origin,
            };
            if used.contains(&binding) {
                continue;
            }
            let end = entry.alias.as_ref().map_or(entry.name.range.end(), |it| it.range.end());
            out.push(Unused {
                range: TextRange::new(entry.name.range.start(), end),
                kind: UnusedKind::Item { decl, item },
                label: format!("`{}` is imported but never used", entry.local()),
                title: format!("Remove unused import `{}`", entry.local()),
            });
        }
    }

    // Which `use` line each keyword the file can see came from, in the order
    // the compiler builds the keywords: the file's own first, then each `use`,
    // a later one replacing an earlier one for the same keyword.
    let mut provider: HashMap<String, Option<usize>> = HashMap::new();
    for def in &hir.anchors {
        if let Some(keyword) = &def.keyword {
            provider.insert(keyword.value.clone(), None);
        }
    }
    for (position, spec) in hir.uses.iter().enumerate() {
        let Some(module) = analysis.module(file, &spec.value) else { continue };
        for symbol in analysis.scope(module).exports.values() {
            if let Symbol::Anchor(id) = symbol {
                if let Some(keyword) = &analysis.anchor_def(*id).keyword {
                    provider.insert(keyword.value.clone(), Some(position));
                }
            }
        }
    }
    let needed: HashSet<usize> = hir
        .anchors
        .iter()
        .filter_map(|def| def.via_keyword.as_ref())
        .filter_map(|keyword| provider.get(&keyword.value).copied().flatten())
        .collect();
    for (position, spec) in hir.uses.iter().enumerate() {
        if analysis.module(file, &spec.value).is_none() || needed.contains(&position) {
            continue;
        }
        let Some(node) = declaration(view, file, SyntaxKind::USE_DECL, position) else { continue };
        out.push(Unused {
            range: without_newline(&node),
            kind: UnusedKind::Use { index: position },
            label: format!("`use {}` brings in no keyword this file uses", spec.value),
            title: format!("Remove unused `use {}`", spec.value),
        });
    }
    out
}

/// The edits that remove `unused` from a file, keeping everything else on the
/// lines they share exactly as it was written.
pub fn removal(view: &View, file: FileId, unused: &[Unused]) -> Vec<TextEdit> {
    let mut by_decl: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut edits = Vec::new();
    for entry in unused {
        match entry.kind {
            UnusedKind::Item { decl, item } => by_decl.entry(decl).or_default().push(item),
            UnusedKind::Use { index } => {
                if let Some(node) = declaration(view, file, SyntaxKind::USE_DECL, index) {
                    edits.push(delete(view, file, from_keyword(&node, USE_KW), node.text_range().end()));
                }
            }
        }
    }
    for (decl, mut removed) in by_decl {
        let Some(node) = declaration(view, file, SyntaxKind::IMPORT_DECL, decl) else { continue };
        removed.sort_unstable();
        removed.dedup();
        let items = item_ranges(&node);
        if removed.len() >= items.len() {
            edits.push(delete(view, file, from_keyword(&node, FROM_KW), node.text_range().end()));
            continue;
        }
        // Each run of removed items takes the separator after it, or, when the
        // run reaches the end of the list, the separator before it, so that
        // what is left reads exactly as the author wrote it.
        let mut start = 0;
        while start < removed.len() {
            let mut end = start;
            while end + 1 < removed.len() && removed[end + 1] == removed[end] + 1 {
                end += 1;
            }
            let (first, last) = (removed[start], removed[end]);
            let range = match items.get(last + 1) {
                Some(next) => TextRange::new(items[first].start(), next.start()),
                None => TextRange::new(items[first - 1].end(), items[last].end()),
            };
            edits.push(delete(view, file, range.start(), range.end()));
            start = end + 1;
        }
    }
    edits.sort_by_key(|edit| (edit.range.start.line, edit.range.start.character));
    edits
}

/// Remove the unused imports and write every remaining import the way
/// `piton format` does, without moving any line.
pub fn organize(view: &View, file: FileId) -> Vec<TextEdit> {
    let hir = &view.compilation.analysis.db.file(file).hir;
    let unused = unused(view, file);
    let removed: HashSet<UnusedKind> = unused.iter().map(|it| it.kind).collect();
    let mut edits: Vec<TextEdit> = unused
        .iter()
        .filter(|it| matches!(it.kind, UnusedKind::Use { .. }))
        .flat_map(|it| removal(view, file, std::slice::from_ref(it)))
        .collect();

    for (decl, import) in hir.imports.iter().enumerate() {
        let Some(node) = declaration(view, file, SyntaxKind::IMPORT_DECL, decl) else { continue };
        let names: Vec<String> = item_texts(&node)
            .into_iter()
            .enumerate()
            .filter(|(item, _)| !removed.contains(&UnusedKind::Item { decl, item: *item }))
            .map(|(_, text)| text)
            .collect();
        if names.is_empty() {
            edits.push(delete(view, file, from_keyword(&node, FROM_KW), node.text_range().end()));
            continue;
        }
        edits.extend(rewrite_declaration(view, file, &node, &import.path.value, "import", &names));
    }
    for (decl, reexport) in hir.reexports.iter().enumerate() {
        if reexport.glob {
            continue;
        }
        let Some(node) = declaration(view, file, SyntaxKind::REEXPORT_DECL, decl) else { continue };
        let names = item_texts(&node);
        edits.extend(rewrite_declaration(view, file, &node, &reexport.path.value, "export", &names));
    }
    edits.sort_by_key(|edit| (edit.range.start.line, edit.range.start.character));
    edits
}

// ---- the syntax the edits are made to ----------------------------------------------

/// The `position`-th declaration of `kind` in a file, in the order the compiler
/// lowered them.
fn declaration(view: &View, file: FileId, kind: SyntaxKind, position: usize) -> Option<SyntaxNode> {
    let root = view.compilation.analysis.db.file(file).parse.syntax();
    root.children().filter(|node| node.kind() == kind).nth(position)
}

/// The written text of each item in an import or re-export list.
fn item_texts(node: &SyntaxNode) -> Vec<String> {
    items(node)
        .filter_map(|item| {
            let name = item.name()?;
            Some(match item.alias() {
                Some(alias) => format!("{name} {alias}"),
                None => name,
            })
        })
        .collect()
}

fn items(node: &SyntaxNode) -> impl Iterator<Item = ast::ImportItem> {
    node.children().find_map(ast::ImportList::cast).into_iter().flat_map(|list| list.items().collect::<Vec<_>>())
}

/// Where each item's text starts and ends, without the whitespace around it.
fn item_ranges(node: &SyntaxNode) -> Vec<TextRange> {
    items(node)
        .filter_map(|item| {
            let first = item.name_token()?;
            let last = item.alias_token().unwrap_or_else(|| first.clone());
            Some(TextRange::new(first.text_range().start(), last.text_range().end()))
        })
        .collect()
}

/// The start of a declaration's keyword, so that a comment written above it
/// inside the same node survives the declaration being removed or rewritten.
fn from_keyword(node: &SyntaxNode, keyword: SyntaxKind) -> TextSize {
    node.children_with_tokens()
        .filter_map(|it| it.into_token())
        .find(|it| it.kind() == keyword)
        .map_or(node.text_range().start(), |it| it.text_range().start())
}

fn without_newline(node: &SyntaxNode) -> TextRange {
    let end = node
        .children_with_tokens()
        .filter_map(|it| it.into_token())
        .filter(|it| it.kind() != NEWLINE && !it.kind().is_trivia())
        .last()
        .map_or(node.text_range().end(), |it| it.text_range().end());
    TextRange::new(from_keyword(node, USE_KW), end)
}

/// Replace an import or re-export with the way `piton format` writes it for
/// `names`, or nothing when it already reads that way.
fn rewrite_declaration(
    view: &View,
    file: FileId,
    node: &SyntaxNode,
    path: &str,
    verb: &str,
    names: &[String],
) -> Option<TextEdit> {
    // A comment inside a declaration does not parse, and rewriting the
    // declaration would drop it.
    let start = from_keyword(node, FROM_KW);
    let has_comment = node
        .children_with_tokens()
        .filter_map(|it| it.into_token())
        .any(|it| it.kind() == COMMENT && it.text_range().start() > start);
    if has_comment {
        return None;
    }
    let mut written = piton_fmt::format(&format!("from {path} {verb} {}\n", names.join(", ")));
    let original = &view.text(file)[usize::from(start)..usize::from(node.text_range().end())];
    if !original.ends_with('\n') {
        written.truncate(written.trim_end_matches('\n').len());
    }
    if written == original {
        return None;
    }
    let index = view.line_index(file);
    Some(TextEdit {
        range: index.range(TextRange::new(start, node.text_range().end())),
        new_text: written,
    })
}

fn delete(view: &View, file: FileId, start: TextSize, end: TextSize) -> TextEdit {
    TextEdit { range: view.line_index(file).range(TextRange::new(start, end)), new_text: String::new() }
}
