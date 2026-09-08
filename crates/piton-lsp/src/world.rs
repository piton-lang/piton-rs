//! The language server's view of the workspace.
//!
//! There is no incremental engine: a change re-analyses the project. Piton
//! projects are documents, not million-line codebases, and a full rebuild keeps
//! every feature answering from exactly the state the compiler would produce.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use piton_core::builtin;
use piton_core::compile::{compile, Compilation};
use piton_core::db::Db;
use piton_core::framework::Frameworks;
use piton_core::project::Project;
use piton_core::FileId;

use crate::line_index::LineIndex;

/// Builds the set of frameworks this server should use.
pub type Registry = fn() -> Frameworks;

/// One fully analysed view of the workspace.
pub struct Snapshot {
    pub compilation: Compilation,
    /// The project this snapshot was analysed against.
    pub project: Project,
    line_indices: HashMap<FileId, LineIndex>,
}

impl Snapshot {
    /// Shorten a path for display, relative to the project when possible.
    pub fn display_path(&self, path: &std::path::Path) -> String {
        path.strip_prefix(&self.project.base)
            .or_else(|_| path.strip_prefix(&self.project.root))
            .unwrap_or(path)
            .display()
            .to_string()
    }
}

impl Snapshot {
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

/// The mutable server state.
pub struct Workspace {
    registry: Registry,
    roots: Vec<PathBuf>,
    open: HashMap<PathBuf, String>,
    snapshot: Option<Arc<Snapshot>>,
}

impl Workspace {
    pub fn new(registry: Registry) -> Workspace {
        Workspace { registry, roots: Vec::new(), open: HashMap::new(), snapshot: None }
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

    /// The files the editor currently has open.
    pub fn open_paths(&self) -> Vec<PathBuf> {
        self.open.keys().cloned().collect()
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
        let cwd = self
            .roots
            .first()
            .cloned()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();

        let mut frameworks = (self.registry)();
        let loaded = Project::load(&cwd, &frameworks);
        if let Some(configuration) = &loaded.compilation {
            frameworks.configure(
                &loaded.project,
                configuration,
                &loaded.project.framework_configs,
            );
        }

        let mut db = Db::new();
        for module in builtin::modules().into_iter().chain(frameworks.modules()) {
            db.add_virtual_module(module.name, module.source);
        }
        db.set_root(&loaded.project.root);

        // Unsaved buffers win over the disk.
        for (path, text) in &self.open {
            db.set_overlay(path, text.clone());
        }

        let mut entries: Vec<FileId> = Vec::new();
        for path in self.source_files(&loaded.project) {
            if let Ok(id) = db.load(&path) {
                entries.push(id);
            }
        }
        entries.sort();
        entries.dedup();

        let compilation = compile(db, entries, &frameworks);
        let line_indices = compilation
            .analysis
            .db
            .files()
            .map(|file| (file.id, LineIndex::new(&file.text)))
            .collect();
        Snapshot { compilation, project: loaded.project, line_indices }
    }

    /// Everything worth analysing.
    ///
    /// A project that declares a `root` has said what belongs to it. Nothing
    /// outside is a project source — generated output and test fixtures very
    /// much included — so reporting problems in those files would be noise
    /// about code the project does not build.
    fn source_files(&self, project: &Project) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = Vec::new();
        let search_roots = match project.config_path {
            Some(_) => vec![project.root.clone()],
            None if !self.roots.is_empty() => self.roots.clone(),
            None => vec![project.root.clone()],
        };
        for root in search_roots {
            if !root.is_dir() {
                continue;
            }
            for entry in walkdir::WalkDir::new(&root)
                .max_depth(24)
                .into_iter()
                .filter_entry(|entry| !is_ignored(entry.file_name().to_string_lossy().as_ref()))
                .filter_map(Result::ok)
            {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "pi") {
                    paths.push(path.to_path_buf());
                }
            }
        }
        // The configuration sits outside the root it declares, and is still
        // worth diagnostics while it is being edited.
        paths.extend(project.config_path.clone());
        // A file someone has open is always analysed, wherever it lives.
        paths.extend(self.open.keys().cloned());
        paths.sort();
        paths.dedup();
        paths
    }
}

fn is_ignored(name: &str) -> bool {
    matches!(name, "node_modules" | "target" | "dist" | "build") || name.starts_with('.')
        && name != "."
}
