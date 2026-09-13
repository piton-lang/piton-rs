//! Moving files, and the imports that have to follow them.
//!
//! A file's specifiers and the specifiers that name it are the one part of a
//! Piton project that a move silently breaks: nothing in the file itself is
//! wrong afterwards, it simply says `./Tool` about a directory it no longer
//! sits in. The editor asks what should change before it moves anything, and
//! this answers — for every project that can see either end of the move, so a
//! file shared between two of them does not come out correct in one and broken
//! in the other.
//!
//! A rewritten specifier keeps the style it was written in. A project that
//! addresses its modules from the root says `/lib/Tool` on purpose, and a move
//! is no occasion to rewrite that as `../../lib/Tool`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use piton_core::db::canonical;
use piton_core::specifier::Style;
use piton_core::types;
use piton_core::FileId;
use piton_syntax::is_identifier;
use piton_syntax::kind::SyntaxKind::{IMPORT_DECL, PATH, REEXPORT_DECL, USE_DECL};
use piton_syntax::kind::RESERVED_KEYWORDS;
use piton_syntax::{TextRange, TextSize};
use tower_lsp::lsp_types::{Position, Range, TextEdit, Url, WorkspaceEdit};

use crate::index::{Located, Model, SelfKind, Sym};
use crate::navigation;
use crate::world::{Snapshot, View};

/// A move the editor is about to make: where a path is now, and where it goes.
///
/// A directory is one entry, not one per file beneath it, so every path is
/// tested against it as a prefix as well as for equality.
#[derive(Clone, Debug)]
pub struct Move {
    pub from: PathBuf,
    pub to: PathBuf,
}

/// Where `path` ends up once `moves` have been applied.
///
/// `None` when nothing touches it, which is the common case and lets a caller
/// skip the work rather than compare a path with itself.
pub fn destination(moves: &[Move], path: &Path) -> Option<PathBuf> {
    for entry in moves {
        if path == entry.from {
            return Some(entry.to.clone());
        }
        if let Ok(rest) = path.strip_prefix(&entry.from) {
            return Some(entry.to.join(rest));
        }
    }
    None
}

/// Where the file that will be at `path` once `moves` are made is now.
fn source_of(moves: &[Move], path: &Path) -> Option<PathBuf> {
    for entry in moves {
        if path == entry.to {
            return Some(entry.from.clone());
        }
        if let Ok(rest) = path.strip_prefix(&entry.to) {
            return Some(entry.from.join(rest));
        }
    }
    None
}

/// Whether a module file will be at `path` once `moves` are made.
///
/// A rewritten specifier is checked against the file system as it is about to
/// be, not as it is: a file that is moving away is not there to resolve to, and
/// the place it is moving to is.
fn exists_after(view: &View, moves: &[Move], path: &Path) -> bool {
    let db = &view.compilation.analysis.db;
    let path = canonical(path);
    if let Some(source) = source_of(moves, &path) {
        return db.exists(&source);
    }
    if moves.iter().any(|entry| path.starts_with(&entry.from)) {
        return false;
    }
    db.exists(&path)
}

/// Every edit the moves require, across every project that can see them.
///
/// Edits are addressed to the files as they are now: the editor applies them
/// before it moves anything, so a file that is itself moving is still at its
/// old URI when its own relative imports are rewritten.
#[cfg(test)]
pub fn move_edits(snapshot: &Snapshot, moves: &[Move]) -> WorkspaceEdit {
    let changes = plan(snapshot, moves)
        .into_iter()
        .map(|(url, rewrites)| (url, rewrites.into_iter().map(|it| it.edit).collect()))
        .collect();
    WorkspaceEdit { changes: Some(changes), ..WorkspaceEdit::default() }
}

/// One specifier rewritten, and the text it replaces.
///
/// The original is kept because a later move in the same batch can make an
/// earlier rewrite unnecessary, and undoing it means writing that text back.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Rewrite {
    edit: TextEdit,
    original: String,
}

type Plan = HashMap<Url, Vec<Rewrite>>;

fn plan(snapshot: &Snapshot, moves: &[Move]) -> Plan {
    let moves: Vec<Move> = moves
        .iter()
        .map(|it| Move { from: canonical(&it.from), to: canonical(&it.to) })
        .collect();
    let mut changes = Plan::new();
    for view in snapshot.views() {
        for file in view.compilation.analysis.db.files().map(|it| it.id).collect::<Vec<_>>() {
            collect_file(view, file, &moves, &mut changes);
        }
    }
    for edits in changes.values_mut() {
        edits.sort_by_key(|it| (it.edit.range.start.line, it.edit.range.start.character));
        // One file analysed by two projects yields the same edit twice.
        edits.dedup();
    }
    changes
}

/// How long a batch of moves waits for another request to join it.
///
/// An editor moving a selection of several files asks about each in turn, back
/// to back; a request that arrives this long after the last is a move of its
/// own.
const BATCH_WINDOW: Duration = Duration::from_secs(3);

/// Moves the editor has asked about and not yet made.
///
/// An editor moving several files asks about one, applies the answer to its
/// buffers, and asks about the next before it tells the server that anything
/// was edited or moved. Answering each request from the workspace as the
/// server last saw it measures ranges against text the editor has already
/// rewritten — `./SelectTool` replaced over `./tools/SelectTool` came out as
/// `../SelectToolctTool` — and ignores the files the earlier requests moved.
///
/// So every request in a batch is planned together with the ones before it,
/// against the workspace as it stood when the batch began, and the editor is
/// sent only what takes the rewrites it already holds to the ones it should.
pub struct Batch {
    base: Arc<Snapshot>,
    moves: Vec<Move>,
    /// Every rewrite already handed to the editor, addressed to `base`'s text.
    handed: Plan,
    last: Instant,
}

impl Batch {
    pub fn new(base: Arc<Snapshot>) -> Batch {
        Batch { base, moves: Vec::new(), handed: Plan::new(), last: Instant::now() }
    }

    pub fn is_open(&self) -> bool {
        self.last.elapsed() < BATCH_WINDOW
    }

    /// Join `moves` to the batch, and answer with the edits they add.
    pub fn add(&mut self, moves: &[Move]) -> WorkspaceEdit {
        self.moves.extend(moves.iter().cloned());
        let wanted = plan(&self.base, &self.moves);
        let changes = rebase(&self.handed, &wanted);
        self.handed = wanted;
        self.last = Instant::now();
        WorkspaceEdit { changes: Some(changes), ..WorkspaceEdit::default() }
    }

    /// Strike off the moves the editor reports it has made. True once none are
    /// left, and the workspace on disk can be trusted again.
    pub fn confirm(&mut self, moves: &[Move]) -> bool {
        for done in moves {
            let (from, to) = (canonical(&done.from), canonical(&done.to));
            self.moves.retain(|it| canonical(&it.from) != from || canonical(&it.to) != to);
        }
        self.moves.is_empty()
    }
}

/// The edits that take the editor from the rewrites it holds to the ones wanted.
///
/// Both plans address the text as it was before the batch began. The editor's
/// text has the held rewrites applied, so each range is shifted by the ones
/// before it on its line, and spans the text the editor has there now.
fn rebase(held: &Plan, wanted: &Plan) -> HashMap<Url, Vec<TextEdit>> {
    fn text_at<'a>(plan: &'a [Rewrite], range: Range, original: &'a str) -> &'a str {
        plan.iter().find(|it| it.edit.range == range).map_or(original, |it| &it.edit.new_text)
    }
    fn width(text: &str) -> i64 {
        text.encode_utf16().count() as i64
    }

    let mut out = HashMap::new();
    let urls: HashSet<&Url> = held.keys().chain(wanted.keys()).collect();
    for url in urls {
        let held = held.get(url).map(Vec::as_slice).unwrap_or_default();
        let wanted = wanted.get(url).map(Vec::as_slice).unwrap_or_default();
        let mut sites: Vec<(Range, &str)> =
            held.iter().chain(wanted).map(|it| (it.edit.range, it.original.as_str())).collect();
        sites.sort_by_key(|(range, _)| (range.start.line, range.start.character));
        sites.dedup_by_key(|(range, _)| *range);

        let mut edits = Vec::new();
        for (range, original) in sites {
            let (now, next) = (text_at(held, range, original), text_at(wanted, range, original));
            if now == next {
                continue;
            }
            let shift: i64 = held
                .iter()
                .filter(|it| {
                    it.edit.range.start.line == range.start.line
                        && it.edit.range.end.character <= range.start.character
                })
                .map(|it| {
                    let replaced = it.edit.range.end.character - it.edit.range.start.character;
                    width(&it.edit.new_text) - i64::from(replaced)
                })
                .sum();
            let start = (i64::from(range.start.character) + shift) as u32;
            edits.push(TextEdit {
                range: Range {
                    start: Position { line: range.start.line, character: start },
                    end: Position { line: range.start.line, character: start + width(now) as u32 },
                },
                new_text: next.to_string(),
            });
        }
        if !edits.is_empty() {
            out.insert(url.clone(), edits);
        }
    }
    out
}

/// Rewrite the specifiers written in one file, if the move changes any of them.
fn collect_file(view: &View, file: FileId, moves: &[Move], changes: &mut Plan) {
    let Some(here) = view.path_of(file).map(Path::to_path_buf) else { return };
    let after_move = destination(moves, &here).unwrap_or_else(|| here.clone());
    let Some(from_dir) = after_move.parent() else { return };
    let Ok(url) = Url::from_file_path(&here) else { return };

    let root = view.compilation.analysis.db.file(file).parse.syntax();
    for token in root.descendants_with_tokens().filter_map(|it| it.into_token()) {
        if token.kind() != PATH {
            continue;
        }
        if !token.parent().is_some_and(|parent| {
            matches!(parent.kind(), IMPORT_DECL | REEXPORT_DECL | USE_DECL)
        }) {
            continue;
        }
        let spec = token.text();
        let db = &view.compilation.analysis.db;
        // A builtin module is not a file and never moves.
        let style = db.specifier_style(spec, &|path| db.exists(path));
        if style == Style::Builtin {
            continue;
        }
        let Some(target) = view.compilation.analysis.module(file, spec) else { continue };
        let Some(target_path) = view.path_of(target).map(Path::to_path_buf) else { continue };
        let target_after = destination(moves, &target_path).unwrap_or_else(|| target_path.clone());
        // Neither end moved, so whatever this says it still says.
        if target_after == target_path && after_move == here {
            continue;
        }
        // The same style, reaching the same file where it is going. When the
        // style cannot reach it any more, the import is left for the compiler
        // to report rather than rewritten to somewhere else.
        let exists = |path: &Path| exists_after(view, moves, path);
        let Some(rewritten) = db.write_specifier(&style, from_dir, &target_after, &exists) else {
            continue;
        };
        if rewritten == spec {
            continue;
        }
        changes.entry(url.clone()).or_default().push(Rewrite {
            edit: TextEdit {
                range: view.line_index(file).range(token.text_range()),
                new_text: rewritten,
            },
            original: spec.to_string(),
        });
    }
}

// ---- renaming a symbol ------------------------------------------------------------

/// Why a rename cannot be made, as a sentence the editor shows the author.
pub type Refusal = String;

/// The name a rename would change under the cursor, and how it is spelled.
pub fn prepare_rename(view: &View, file: FileId, offset: TextSize) -> Result<(TextRange, String), Refusal> {
    let located = navigation::locate(view, file, offset)
        .ok_or_else(|| "There is no name here that can be renamed.".to_string())?;
    let occurrence = match located {
        Located::SelfReference(reference) => {
            let word = match reference.kind {
                SelfKind::SelfRef => "self",
                SelfKind::This => "this",
                SelfKind::Super => "super",
            };
            return Err(format!("`{word}` always means the anchor it is written in, and cannot be renamed."));
        }
        Located::Symbol(occurrence) => occurrence,
    };
    renameable(view, &view.model(), &occurrence.sym)?;
    Ok((occurrence.range, view.text(file)[occurrence.range].to_string()))
}

/// Refuse the symbols no edit can rename.
fn renameable(view: &View, model: &Model, sym: &Sym) -> Result<(), Refusal> {
    match sym {
        Sym::Builtin(name) => return Err(format!("`{name}` is a built-in type and cannot be renamed.")),
        Sym::Module(_) => {
            return Err("A module is renamed by moving its file, and the imports that name it follow the move."
                .to_string())
        }
        _ => {}
    }
    for (file, _) in view.index().declarations(sym) {
        if view.is_virtual(file) {
            let module = view.compilation.analysis.db.file(file).source.display();
            let name = model.name_of(sym).unwrap_or_default();
            return Err(format!("`{name}` is declared by `{module}`, which cannot be edited."));
        }
    }
    Ok(())
}

/// Every edit renaming the symbol at `offset` requires, in every project that
/// can see it, or the reason it cannot be renamed.
///
/// A project is a closed world for resolution, which is what keeps `/lib/Tool`
/// meaning a different file in each of two projects, but a rename is not a
/// resolution question. A file under a shared root is read by every project
/// that names it, and renaming an anchor there while only one project's
/// references follow would leave the others naming something that is gone.
pub fn rename_edits(
    snapshot: &Snapshot,
    path: &Path,
    offset: TextSize,
    new_name: &str,
) -> Result<WorkspaceEdit, Refusal> {
    let (view, file) =
        snapshot.locate(path).ok_or_else(|| "This file is not part of any project.".to_string())?;
    let (_, current) = prepare_rename(view, file, offset)?;
    let Some(Located::Symbol(occurrence)) = navigation::locate(view, file, offset) else {
        return Err("There is no name here that can be renamed.".to_string());
    };
    validate_name(&view.model(), &occurrence.sym, new_name)?;
    if new_name == current {
        return Ok(WorkspaceEdit::default());
    }

    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    for other in snapshot.views() {
        for sym in navigation::counterparts(view, &occurrence.sym, other) {
            let model = other.model();
            renameable(other, &model, &sym)?;
            conflicts(other, &model, &sym, new_name)?;
            for (site, found) in other.index().sites(&sym) {
                let Some(path) = other.path_of(site) else { continue };
                let Ok(url) = Url::from_file_path(path) else { continue };
                changes.entry(url).or_default().push(TextEdit {
                    range: other.line_index(site).range(found.range),
                    new_text: new_name.to_string(),
                });
            }
        }
    }
    // A file two projects both analysed yields every edit twice.
    for edits in changes.values_mut() {
        edits.sort_by_key(|edit| (edit.range.start.line, edit.range.start.character));
        edits.dedup();
    }
    Ok(WorkspaceEdit { changes: Some(changes), ..WorkspaceEdit::default() })
}

/// Whether `name` is a name the symbol can take.
fn validate_name(model: &Model, sym: &Sym, name: &str) -> Result<(), Refusal> {
    if RESERVED_KEYWORDS.contains(&name) {
        return Err(format!("`{name}` is a reserved word."));
    }
    match sym {
        Sym::Keyword(_) => {
            if !is_identifier(name) || name.chars().any(char::is_uppercase) {
                return Err(format!(
                    "`{name}` is not a keyword name: a keyword is lowercase, and may be kebab-case."
                ));
            }
            if types::is_builtin(name) {
                return Err(format!("`{name}` is a built-in type, and cannot be a keyword."));
            }
        }
        _ => {
            if !is_identifier(name) {
                return Err(format!(
                    "`{name}` is not a name: a name starts with a letter or `_`, and continues with \
                     letters, digits, `_`, or a `-` between two of those."
                ));
            }
            if model.anchor_of(sym).is_some() && types::is_builtin(name) {
                return Err(format!(
                    "`{name}` is a built-in type, so an anchor by that name could never be used as one."
                ));
            }
        }
    }
    Ok(())
}

/// Refuse a rename that would take a name something else already has.
fn conflicts(view: &View, model: &Model, sym: &Sym, new_name: &str) -> Result<(), Refusal> {
    let analysis = model.analysis();
    match sym {
        Sym::Anchor(_) | Sym::Var { .. } | Sym::Alias { .. } => {
            let Some(old) = model.name_of(sym) else { return Ok(()) };
            for file in analysis.db.files() {
                let scope = analysis.scope(file.id);
                if scope.names.contains_key(new_name)
                    && scope.names.contains_key(&old)
                    && model.binding(file.id, &old).as_ref() == Some(sym)
                {
                    return Err(format!(
                        "`{new_name}` is already declared or imported in {}.",
                        describe(view, file.id)
                    ));
                }
                if scope.exports.contains_key(new_name) && model.origin(file.id, &old).as_ref() == Some(sym) {
                    return Err(format!("{} already exports something named `{new_name}`.", describe(view, file.id)));
                }
            }
        }
        Sym::Keyword(id) => {
            let Some(old) = model.name_of(sym) else { return Ok(()) };
            for file in analysis.db.files() {
                let keywords = &analysis.scope(file.id).keywords;
                if keywords.get(&old) == Some(id) && keywords.contains_key(new_name) {
                    return Err(format!("{} can already see a keyword `{new_name}`.", describe(view, file.id)));
                }
            }
        }
        Sym::Property { family, name } => {
            for member in model.family_members(*family, name).iter() {
                if model.is_visible(*member, new_name) {
                    return Err(format!(
                        "`{}` already has a property named `{new_name}`.",
                        analysis.anchor_def(*member).name
                    ));
                }
            }
        }
        Sym::Key { name, .. } => {
            if view.index().siblings(sym).iter().any(|keys| keys.iter().any(|it| it == new_name)) {
                return Err(format!("A dictionary that declares `{name}` already has a key named `{new_name}`."));
            }
        }
        Sym::Module(_) | Sym::Builtin(_) => {}
    }
    Ok(())
}

/// How a file is named in a message.
fn describe(view: &View, file: FileId) -> String {
    let source = &view.compilation.analysis.db.file(file).source;
    match source.as_path() {
        Some(path) => format!("`{}`", view.display_path(path)),
        None => format!("`{}`", source.display()),
    }
}
