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
use piton_core::FileId;
use piton_syntax::kind::SyntaxKind::{IMPORT_DECL, PATH, REEXPORT_DECL, USE_DECL};
use tower_lsp::lsp_types::{Position, Range, TextEdit, Url, WorkspaceEdit};

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

/// How a specifier addresses its target.
///
/// Read from the text rather than from where it resolved to, because it is the
/// author's choice of address that a rewrite has to preserve.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Style {
    Relative,
    Root,
    Shared,
}

fn style_of(spec: &str) -> Option<Style> {
    match spec {
        // A builtin module is not a file and never moves.
        _ if spec.starts_with('@') => None,
        _ if spec.starts_with("//") => Some(Style::Shared),
        _ if spec.starts_with('/') => Some(Style::Root),
        _ => Some(Style::Relative),
    }
}

/// The specifier text that reaches `target` from `from_dir`, in `style`.
///
/// `None` when the style cannot reach it any more — a file moved out from under
/// the root it was addressed through — and the caller then leaves the import
/// alone rather than writing a path that resolves somewhere else.
fn specifier(view: &View, style: Style, from_dir: &Path, target: &Path) -> Option<String> {
    // Specifiers name modules, not files: `index.pi` is its directory, and no
    // specifier carries the extension.
    let target = match target.file_name().and_then(|it| it.to_str()) {
        Some("index.pi") => target.parent()?.to_path_buf(),
        _ => target.with_extension(""),
    };
    match style {
        Style::Relative => {
            let rest = relative(from_dir, &target)?;
            Some(match rest.starts_with("..") {
                true => rest,
                false => format!("./{rest}"),
            })
        }
        Style::Root => {
            if let Some(rest) = under(&view.project.root, &target) {
                return Some(format!("/{rest}"));
            }
            // A rooted specifier may have been reaching a library rather than
            // the root, and that is still the address it should keep.
            view.project.libraries.iter().find_map(|(name, path)| {
                let rest = under(path, &target)?;
                Some(match rest.is_empty() {
                    true => format!("/{name}"),
                    false => format!("/{name}/{rest}"),
                })
            })
        }
        Style::Shared => {
            let rest = under(view.project.shared_root.as_deref()?, &target)?;
            Some(format!("//{rest}"))
        }
    }
}

/// `target` written relative to `base`, or `None` when it is not underneath.
///
/// Both ends are normalised first. A configured path keeps the shape it was
/// written in — `../shared`, `./spec` — and a `..` left in the middle of one is
/// a prefix of nothing, so the honest answer to "is this file under that root"
/// would otherwise be no for every file.
fn under(base: &Path, target: &Path) -> Option<String> {
    let rest = canonical(target).strip_prefix(canonical(base)).ok()?.to_path_buf();
    Some(join(&rest))
}

/// `target` written relative to `from_dir`, with `..` for each level up.
fn relative(from_dir: &Path, target: &Path) -> Option<String> {
    let from: Vec<_> = from_dir.components().collect();
    let to: Vec<_> = target.components().collect();
    let shared = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    // Nothing in common at all means two different roots, which no relative
    // path spans.
    if shared == 0 {
        return None;
    }
    let mut parts: Vec<String> = vec!["..".to_string(); from.len() - shared];
    parts.extend(to[shared..].iter().map(|it| it.as_os_str().to_string_lossy().to_string()));
    match parts.is_empty() {
        true => None,
        false => Some(parts.join("/")),
    }
}

fn join(path: &Path) -> String {
    path.components()
        .map(|it| it.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
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
        let Some(style) = style_of(spec) else { continue };
        let Some(target) = view.compilation.analysis.module(file, spec) else { continue };
        let Some(target_path) = view.path_of(target).map(Path::to_path_buf) else { continue };
        let target_after = destination(moves, &target_path).unwrap_or_else(|| target_path.clone());
        // Neither end moved, so whatever this says it still says.
        if target_after == target_path && after_move == here {
            continue;
        }
        let Some(rewritten) = specifier(view, style, from_dir, &target_after) else { continue };
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

/// Every edit renaming the symbol at `offset` requires, across every project.
///
/// A project is a closed world for resolution — that is what keeps `/lib/Tool`
/// meaning a different file in each of two projects — but a rename is not a
/// resolution question. A file under a shared root is read by every project
/// that names it, and renaming an anchor there while only one project's
/// references follow leaves the others referring to a name that is gone.
///
/// The declaration is what the projects are matched on: where a thing is
/// written is the one fact about it they all agree on, since the ids each
/// compilation hands out are its own.
pub fn rename_edits(
    snapshot: &Snapshot,
    path: &Path,
    offset: piton_syntax::TextSize,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    let (view, file) = snapshot.locate(path)?;
    let resolved = crate::navigation::resolve(view, file, offset)?;
    let declaration = crate::navigation::definition(view, &resolved.target)
        .and_then(|(file, range)| Some((view.path_of(file)?.to_path_buf(), range)));

    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    for other in snapshot.views() {
        let target = if std::sync::Arc::ptr_eq(other, view) {
            resolved.target.clone()
        } else {
            // The same declaration, resolved again in this project's own terms.
            // A project that cannot see the file it is written in has nothing
            // to rename.
            let Some((declared_in, range)) = &declaration else { continue };
            let Some(here) = other.file_for(declared_in) else { continue };
            match crate::navigation::resolve(other, here, range.start()) {
                Some(it) => it.target,
                None => continue,
            }
        };
        let mut sites = crate::navigation::references(other, &target);
        if let Some(site) = crate::navigation::definition(other, &target) {
            if !sites.contains(&site) {
                sites.push(site);
            }
        }
        for (site_file, range) in sites {
            let Some(path) = other.path_of(site_file) else { continue };
            let Ok(url) = Url::from_file_path(path) else { continue };
            changes.entry(url).or_default().push(TextEdit {
                range: other.line_index(site_file).range(range),
                new_text: new_name.to_string(),
            });
        }
    }
    // A file two projects both analysed yields every edit twice.
    for edits in changes.values_mut() {
        edits.sort_by_key(|edit| (edit.range.start.line, edit.range.start.character));
        edits.dedup();
    }
    Some(WorkspaceEdit { changes: Some(changes), ..WorkspaceEdit::default() })
}
