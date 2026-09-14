//! The language server's view of the workspace.
//!
//! There is no incremental engine: a change re-analyses the workspace. Piton
//! projects are documents, not million-line codebases, and a full rebuild keeps
//! every feature answering from exactly the state the compiler would produce.
//!
//! A workspace is not a project. A directory an editor is pointed at may hold
//! several of them side by side, or none at all, so the projects in it are
//! discovered rather than assumed, and each is analysed on its own.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use piton_core::builtin;
use piton_core::compile::{compile, Compilation};
use piton_core::db::{canonical, Db, SourceCache};
use piton_core::framework::Frameworks;
use piton_core::project::{Loaded, Project, CONFIG_FILE};
use piton_core::FileId;

use tower_lsp::lsp_types::WorkspaceEdit;

use crate::index::{Index, Model};
use crate::line_index::LineIndex;
use crate::refactor::{Batch, Move};

/// Builds the set of frameworks this server should use.
pub type Registry = fn() -> Frameworks;

/// How deep a workspace is searched, both for projects and for their sources.
const MAX_DEPTH: usize = 24;

/// One project, fully analysed.
///
/// A project is a closed world: its own module root, its own frameworks, its
/// own database. Two projects in one workspace therefore never see each
/// other's files, which is what lets `/lib/Tool` mean a different file in each
/// of them — and what stops a symbol in one resolving against the other.
pub struct View {
    pub compilation: Compilation,
    /// The project this view was analysed against.
    pub project: Project,
    /// The frameworks the project was compiled with, which own the builtin
    /// values a name can mean.
    pub frameworks: Frameworks,
    line_indices: HashMap<FileId, LineIndex>,
    /// The symbol model, built the first time a feature needs it and then
    /// shared by every request against this analysis.
    index: std::sync::OnceLock<Index>,
}

impl View {
    pub fn index(&self) -> &Index {
        self.index.get_or_init(|| Index::build(self))
    }

    /// The compiler's rules, ready to answer questions about this project.
    pub fn model(&self) -> Model<'_> {
        Model::new(self)
    }

    /// Whether a framework supplies a value under `name`, which then wins over
    /// anything else the name could mean.
    pub fn is_builtin_value(&self, name: &str) -> bool {
        self.frameworks.builtin_value(name).is_some()
    }

    /// Whether a file is a builtin or framework module, with no file behind it.
    pub fn is_virtual(&self, file: FileId) -> bool {
        self.compilation.analysis.db.file(file).source.as_path().is_none()
    }

    /// Shorten a path for display, relative to the project when possible.
    pub fn display_path(&self, path: &std::path::Path) -> String {
        path.strip_prefix(&self.project.base)
            .or_else(|_| path.strip_prefix(&self.project.root))
            .unwrap_or(path)
            .display()
            .to_string()
    }

    pub fn line_index(&self, file: FileId) -> &LineIndex {
        &self.line_indices[&file]
    }

    pub fn file_for(&self, path: &Path) -> Option<FileId> {
        self.compilation.analysis.db.file_id(path)
    }

    pub fn path_of(&self, file: FileId) -> Option<&Path> {
        self.compilation.analysis.db.file(file).source.as_path()
    }

    pub fn text(&self, file: FileId) -> &str {
        &self.compilation.analysis.db.file(file).text
    }
}

/// One fully analysed view of the workspace: a compilation per project.
pub struct Snapshot {
    /// Most specific project first, so the innermost owner of a nested path
    /// answers before the outer one.
    views: Vec<Arc<View>>,
}

impl Snapshot {
    pub fn views(&self) -> &[Arc<View>] {
        &self.views
    }

    /// The project that analysed `path`, and the id the file has there.
    ///
    /// Source sets overlap once a project borrows another's files as a
    /// library, so the project whose root holds the path answers first — the
    /// deepest, since views are ordered that way — and a borrower never speaks
    /// for a file it does not own. A path under no root that analysed it, such
    /// as a configuration beside its root, falls to whichever view has it. A
    /// file no project analysed has no view at all, and every feature that
    /// needs one declines to answer.
    pub fn locate(&self, path: &Path) -> Option<(&Arc<View>, FileId)> {
        let canonical_path = canonical(path);
        self.views
            .iter()
            .filter(|view| canonical_path.starts_with(canonical(&view.project.root)))
            .find_map(|view| Some((view, view.file_for(path)?)))
            .or_else(|| self.views.iter().find_map(|view| Some((view, view.file_for(path)?))))
    }
}

/// The mutable server state.
pub struct Workspace {
    registry: Registry,
    roots: Vec<PathBuf>,
    open: HashMap<PathBuf, String>,
    snapshot: Option<Arc<Snapshot>>,
    /// Moves the editor has asked about and not yet reported making.
    batch: Option<Batch>,
    /// Every file read from disk, shared by every analysis so that a file that
    /// has not changed is not read again.
    sources: Arc<SourceCache>,
}

impl Workspace {
    pub fn new(registry: Registry) -> Workspace {
        Workspace {
            registry,
            roots: Vec::new(),
            open: HashMap::new(),
            snapshot: None,
            batch: None,
            sources: Arc::new(SourceCache::default()),
        }
    }

    /// Something outside the editor says these files changed.
    ///
    /// The cache already notices a changed size or modification time; this is
    /// for the change that keeps both, which a watcher can still report.
    pub fn changed_on_disk(&mut self, paths: &[PathBuf]) {
        for path in paths {
            self.sources.forget(&canonical(path));
        }
        self.snapshot = None;
    }

    /// The edits `moves` need, asked for before the editor makes them.
    ///
    /// A request made while an earlier one is still unconfirmed joins its
    /// batch, because the editor is holding that answer already; see [`Batch`].
    pub fn will_move(&mut self, moves: &[Move]) -> WorkspaceEdit {
        if !self.batch.as_ref().is_some_and(Batch::is_open) {
            let base = self.snapshot();
            self.batch = Some(Batch::new(base));
        }
        self.batch.as_mut().expect("a batch was just opened").add(moves)
    }

    pub fn set_roots(&mut self, roots: Vec<PathBuf>) {
        self.roots = roots;
        self.snapshot = None;
    }

    pub fn open(&mut self, path: PathBuf, text: String) {
        self.open.insert(path, text);
        self.snapshot = None;
    }

    pub fn close(&mut self, path: &Path) {
        self.open.remove(path);
        self.snapshot = None;
    }

    pub fn invalidate(&mut self) {
        self.snapshot = None;
    }

    /// Follow a set of moves with the buffers the editor has open.
    ///
    /// An open buffer is keyed by path, and after a move the editor talks about
    /// the file by its new one. Without re-keying, the unsaved text stays
    /// attached to a path nothing will ask about again while the file at the
    /// new path is read from the disk it has not been written to yet.
    pub fn moved(&mut self, moves: &[Move]) {
        if self.batch.as_mut().is_some_and(|batch| batch.confirm(moves)) {
            self.batch = None;
        }
        let open = std::mem::take(&mut self.open);
        self.open = open
            .into_iter()
            .map(|(path, text)| match crate::refactor::destination(moves, &path) {
                Some(destination) => (destination, text),
                None => (path, text),
            })
            .collect();
        self.snapshot = None;
    }

    /// Analyse the workspace, reusing the last result when nothing changed.
    pub fn snapshot(&mut self) -> Arc<Snapshot> {
        if let Some(snapshot) = &self.snapshot {
            return snapshot.clone();
        }
        let snapshot = Arc::new(self.analyze());
        self.snapshot = Some(snapshot.clone());
        snapshot
    }

    fn analyze(&self) -> Snapshot {
        let workspace_root = self
            .roots
            .first()
            .cloned()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();

        // Every configuration is read before anything is compiled: which open
        // buffer belongs to which project cannot be decided until every
        // declared root is known.
        let mut loaded: Vec<(Loaded, Frameworks)> = Vec::new();
        for config in self.configs(&workspace_root) {
            let directory = config.parent().unwrap_or(&workspace_root).to_path_buf();
            let mut frameworks = (self.registry)();
            let project = Project::load(&directory, &frameworks);
            if let Some(configuration) = &project.compilation {
                frameworks.configure(
                    &project.project,
                    configuration,
                    &project.project.framework_configs,
                );
            }
            loaded.push((project, frameworks));
        }

        let claims: Vec<Claim> = loaded
            .iter()
            .map(|(it, _)| Claim {
                root: canonical(&it.project.root),
                config: it.project.config_path.as_deref().map(canonical),
            })
            .collect();

        let mut views: Vec<Arc<View>> = Vec::new();
        for (index, (project, frameworks)) in loaded.into_iter().enumerate() {
            // A configuration the compiler would refuse says nothing reliable
            // about where this project's code lives or what its rooted imports
            // mean. Analysing the sources anyway buries the one problem the
            // author can act on under every import it breaks, so the project
            // reports its configuration and nothing else until that is fixed.
            let mut sources = match project.diagnostics.has_errors() {
                true => Vec::new(),
                false => {
                    let mut sources = walk(&project.project.root);
                    sources.extend(
                        self.open
                            .keys()
                            .filter(|path| owner(&claims, path) == Some(index))
                            .cloned(),
                    );
                    sources
                }
            };
            // The configuration sits outside the root it declares, and is still
            // worth diagnostics while it is being edited.
            sources.extend(project.project.config_path.clone());
            views.push(Arc::new(self.build(project, frameworks, sources)));
        }

        // What belongs to no project: a buffer open outside every root, and —
        // when the workspace declares no project at all — the whole tree. This
        // is the fallback, so it takes only what the projects left behind.
        //
        // These files are analysed without a root. A directory an editor
        // happened to open is not a project, and treating it as one would make
        // `/a/b` mean whatever that directory contains; the compiler already
        // says what it should say here, which is that a rooted import needs a
        // piton.config.pi.
        let mut orphans: Vec<PathBuf> =
            self.open.keys().filter(|path| owner(&claims, path).is_none()).cloned().collect();
        if claims.is_empty() {
            let search: &[PathBuf] = match self.roots.is_empty() {
                true => std::slice::from_ref(&workspace_root),
                false => &self.roots,
            };
            for root in search {
                orphans.extend(walk(root));
            }
        }
        if !orphans.is_empty() {
            let bare = Loaded::bare(&workspace_root);
            views.push(Arc::new(self.build(bare, (self.registry)(), orphans)));
        }

        // A nested project answers for its own files before the project it sits
        // inside does, and the rootless fallback answers last of all.
        views.sort_by_key(|view| std::cmp::Reverse(view.project.root.components().count()));
        Snapshot { views }
    }

    /// Compile one project's sources into a view.
    fn build(&self, loaded: Loaded, frameworks: Frameworks, sources: Vec<PathBuf>) -> View {
        let mut db = Db::new();
        db.set_source_cache(self.sources.clone());
        for module in builtin::modules().into_iter().chain(frameworks.modules()) {
            db.add_virtual_module(module.name, module.source);
        }
        if loaded.project.has_root() {
            db.set_root(&loaded.project.root);
        }
        db.set_libraries(loaded.project.libraries.clone());
        db.set_shared_root(loaded.project.shared_root.clone());

        let mut sources = sources;
        sources.sort();
        sources.dedup();

        // Unsaved buffers win over the disk, in a file this project only
        // borrows through a library as much as in one it owns; otherwise the
        // borrower analyses what was last saved while the editor shows more.
        for path in &sources {
            if let Some(text) = self.open.get(path) {
                db.set_overlay(path, text.clone());
            }
        }
        for (path, text) in &self.open {
            if sources.binary_search(path).is_err() {
                db.add_overlay(path, text.clone());
            }
        }

        let mut entries: Vec<FileId> = Vec::new();
        for path in &sources {
            if let Ok(id) = db.load(path) {
                entries.push(id);
            }
        }
        entries.sort();
        entries.dedup();

        let mut compilation = compile(db, entries, &frameworks);
        // A configuration that cannot be honoured is reported where it is
        // written, rather than as the pile of unresolved imports it causes.
        for diagnostic in loaded.diagnostics_in(&compilation.analysis.db) {
            compilation.diagnostics.push(diagnostic);
        }

        let line_indices = compilation
            .analysis
            .db
            .files()
            .map(|file| (file.id, LineIndex::new(&file.text)))
            .collect();
        View {
            compilation,
            project: loaded.project,
            frameworks,
            line_indices,
            index: std::sync::OnceLock::new(),
        }
    }

    /// Every project configuration the workspace contains.
    ///
    /// A workspace root may sit inside a project, hold one, or hold several
    /// side by side, so the search goes both ways. Downward it stops at each
    /// configuration it finds: everything below that directory belongs to the
    /// project declared there, not to a third one.
    fn configs(&self, workspace_root: &Path) -> Vec<PathBuf> {
        let roots: Vec<&Path> = match self.roots.is_empty() {
            true => vec![workspace_root],
            false => self.roots.iter().map(PathBuf::as_path).collect(),
        };
        let mut found = Vec::new();
        for root in roots {
            found.extend(Project::find_config(root));
            configs_below(root, MAX_DEPTH, &mut found);
        }
        found.sort();
        found.dedup();
        found
    }
}

/// What one project claims as its own.
struct Claim {
    root: PathBuf,
    /// The configuration declaring the project, which may sit outside the root
    /// it declares and still belong to nothing else.
    config: Option<PathBuf>,
}

/// The project a path belongs to: the one whose root is its longest prefix.
///
/// Projects can nest, and the deeper one owns the file; a path under no root at
/// all belongs to none of them, which is what leaves it to the bare fallback.
fn owner(claims: &[Claim], path: &Path) -> Option<usize> {
    let path = canonical(path);
    if let Some(index) = claims.iter().position(|it| it.config.as_deref() == Some(&*path)) {
        return Some(index);
    }
    claims
        .iter()
        .enumerate()
        .filter(|(_, claim)| path.starts_with(&claim.root))
        .max_by_key(|(_, claim)| claim.root.components().count())
        .map(|(index, _)| index)
}

/// Collect the configurations at or below `dir`.
fn configs_below(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let config = dir.join(CONFIG_FILE);
    if config.is_file() {
        out.push(config);
        return;
    }
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if !is_ignored(entry.file_name().to_string_lossy().as_ref())
            && entry.path().is_dir()
        {
            configs_below(&entry.path(), depth - 1, out);
        }
    }
}

/// Every `.pi` file under `root`.
///
/// A project that declares a `root` has said what belongs to it. Nothing
/// outside is a project source — generated output and test fixtures very much
/// included — so reporting problems in those files would be noise about code
/// the project does not build.
fn walk(root: &Path) -> Vec<PathBuf> {
    if !root.is_dir() {
        return Vec::new();
    }
    walkdir::WalkDir::new(root)
        .max_depth(MAX_DEPTH)
        .into_iter()
        .filter_entry(|entry| !is_ignored(entry.file_name().to_string_lossy().as_ref()))
        .filter_map(Result::ok)
        .map(|entry| entry.into_path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "pi"))
        .collect()
}

fn is_ignored(name: &str) -> bool {
    matches!(name, "node_modules" | "target" | "dist" | "build") || name.starts_with('.')
        && name != "."
}
