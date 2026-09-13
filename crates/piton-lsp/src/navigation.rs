//! Following a name: definitions, references, highlights, implementations, and
//! the same symbol as another project sees it.
//!
//! Every answer is read from the symbol model in [`crate::index`], so these
//! features agree with each other, and with the compiler, about what a name is.

use std::sync::Arc;

use piton_core::value::AnchorId;
use piton_core::FileId;
use piton_syntax::{TextRange, TextSize};
use tower_lsp::lsp_types::DocumentHighlightKind;

use crate::index::{Access, Located, Model, Role, SelfKind, Sym};
use crate::world::View;

/// Whatever is under the cursor.
pub fn locate(view: &View, file: FileId, offset: TextSize) -> Option<Located> {
    view.index().locate(file, offset)
}

/// Where the thing under the cursor in `file` is defined.
pub fn definitions(view: &View, file: FileId, located: &Located) -> Vec<(FileId, TextRange)> {
    let model = view.model();
    let found = match located {
        Located::SelfReference(reference) => match reference.kind {
            SelfKind::SelfRef | SelfKind::This => vec![anchor_name(&model, reference.anchor)],
            SelfKind::Super => {
                model.bases(reference.anchor).iter().rev().map(|base| anchor_name(&model, *base)).collect()
            }
        },
        Located::Symbol(occurrence) => match (&occurrence.sym, occurrence.role, occurrence.access) {
            // A key written in a body is its own definition.
            (Sym::Property { .. } | Sym::Key { .. }, Role::Declaration, _) => vec![(file, occurrence.range)],
            (Sym::Property { name, .. }, _, Some(Access::Anchor(id))) => model
                .slot(id, name)
                .map(|slot| (model.file_of(slot.owner), slot.property.name_range))
                .into_iter()
                .collect(),
            (Sym::Property { name, .. }, _, Some(Access::Super(id))) => model
                .base_slot(id, name)
                .map(|slot| (model.file_of(slot.owner), slot.property.name_range))
                .into_iter()
                .collect(),
            (Sym::Key { .. }, _, Some(Access::Dictionary { file, range })) => vec![(file, range)],
            (sym, _, _) => symbol_definitions(&model, sym),
        },
    };
    // A builtin or framework module has no file to open.
    found.into_iter().filter(|(file, _)| !view.is_virtual(*file)).collect()
}

fn symbol_definitions(model: &Model, sym: &Sym) -> Vec<(FileId, TextRange)> {
    let analysis = model.analysis();
    match sym {
        Sym::Anchor(id) => vec![anchor_name(model, *id)],
        Sym::Keyword(id) => analysis
            .anchor_def(*id)
            .keyword
            .as_ref()
            .map(|keyword| (model.file_of(*id), keyword.range))
            .into_iter()
            .collect(),
        Sym::Var { file, index } => analysis
            .db
            .file(*file)
            .hir
            .vars
            .get(*index)
            .map(|variable| (*file, variable.name_range))
            .into_iter()
            .collect(),
        // An alias goes to what it names, as TypeScript does.
        Sym::Alias { .. } => model
            .resolve_alias(sym.clone())
            .map(|target| symbol_definitions(model, &target))
            .unwrap_or_default(),
        Sym::Module(file) => vec![(*file, TextRange::empty(TextSize::new(0)))],
        Sym::Property { .. } | Sym::Key { .. } => model.view.index().declarations(sym),
        Sym::Builtin(_) => Vec::new(),
    }
}

fn anchor_name(model: &Model, id: AnchorId) -> (FileId, TextRange) {
    (model.file_of(id), model.analysis().anchor_def(id).name_range)
}

/// Every place a symbol is written in one project, optionally with its
/// declarations. `self`, `this`, and `super` are never among them.
pub fn references(view: &View, sym: &Sym, include_declarations: bool) -> Vec<(FileId, TextRange)> {
    view.index()
        .sites(sym)
        .filter(|(_, occurrence)| include_declarations || occurrence.role != Role::Declaration)
        .map(|(file, occurrence)| (file, occurrence.range))
        .collect()
}

/// The uses of the symbol under the cursor in its own file.
pub fn highlights(view: &View, file: FileId, located: &Located) -> Vec<(TextRange, DocumentHighlightKind)> {
    let Located::Symbol(target) = located else { return Vec::new() };
    view.index()
        .in_file(file)
        .iter()
        .filter(|occurrence| occurrence.sym == target.sym)
        .map(|occurrence| {
            let kind = match occurrence.role {
                Role::Declaration => DocumentHighlightKind::WRITE,
                Role::Reference | Role::ImportName => DocumentHighlightKind::READ,
            };
            (occurrence.range, kind)
        })
        .collect()
}

/// The concrete anchors, or concrete declarations of a property, that fill in
/// the shape under the cursor.
pub fn implementations(view: &View, located: &Located) -> Vec<(FileId, TextRange)> {
    let Located::Symbol(occurrence) = located else { return Vec::new() };
    let model = view.model();
    let analysis = model.analysis();
    let found: Vec<(FileId, TextRange)> = match &occurrence.sym {
        Sym::Property { family, name } => model
            .family_members(*family, name)
            .iter()
            .filter(|member| !analysis.anchor_def(**member).is_abstract)
            .flat_map(|member| {
                model
                    .own(*member)
                    .iter()
                    .filter(|property| property.name == *name)
                    .map(|property| (model.file_of(*member), property.name_range))
                    .collect::<Vec<_>>()
            })
            .collect(),
        Sym::Keyword(id) => implementors(&model, *id),
        sym => model.anchor_of(sym).map(|id| implementors(&model, id)).unwrap_or_default(),
    };
    found.into_iter().filter(|(file, _)| !view.is_virtual(*file)).collect()
}

fn implementors(model: &Model, id: AnchorId) -> Vec<(FileId, TextRange)> {
    model.analysis().implementors(id).into_iter().map(|it| anchor_name(model, it)).collect()
}

/// The anchor the cursor is on, for the type hierarchy.
pub fn anchor_at(view: &View, file: FileId, offset: TextSize) -> Option<AnchorId> {
    match locate(view, file, offset)? {
        Located::SelfReference(reference) => Some(reference.anchor),
        Located::Symbol(occurrence) => match occurrence.sym {
            Sym::Keyword(id) => Some(id),
            sym => view.model().anchor_of(&sym),
        },
    }
}

/// The symbols in `to` that are `sym` in `from`.
///
/// Symbols are matched on where they are declared, the one fact two projects
/// that read the same file agree on, since each compilation numbers its own
/// anchors. A symbol with several declarations, such as a property whose
/// family spans files, can be more than one symbol in a project that does not
/// see every file that joins them.
pub fn counterparts(from: &Arc<View>, sym: &Sym, to: &Arc<View>) -> Vec<Sym> {
    if Arc::ptr_eq(from, to) {
        return vec![sym.clone()];
    }
    let from_db = &from.compilation.analysis.db;
    let to_db = &to.compilation.analysis.db;
    let same_file = |file: FileId| {
        let source = &from_db.file(file).source;
        to_db.files().find(|candidate| &candidate.source == source).map(|candidate| candidate.id)
    };
    match sym {
        Sym::Builtin(_) => return vec![sym.clone()],
        Sym::Module(file) => return same_file(*file).map(Sym::Module).into_iter().collect(),
        _ => {}
    }
    let mut out: Vec<Sym> = Vec::new();
    for (file, range) in from.index().declarations(sym) {
        let Some(there) = same_file(file) else { continue };
        let found = to
            .index()
            .in_file(there)
            .iter()
            .find(|occurrence| occurrence.range == range && occurrence.role == Role::Declaration);
        if let Some(found) = found {
            if !out.contains(&found.sym) {
                out.push(found.sym.clone());
            }
        }
    }
    out
}

/// Whether a workspace symbol search matches a name: every character of the
/// query appears in the name, in order, ignoring case.
pub fn matches_query(query: &str, name: &str) -> bool {
    let mut remaining = name.chars().flat_map(char::to_lowercase);
    query.chars().flat_map(char::to_lowercase).all(|wanted| remaining.any(|it| it == wanted))
}
