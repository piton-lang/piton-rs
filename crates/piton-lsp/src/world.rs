//! Workspace state.
//!
//! The server keeps the open buffers and one compiled view of the project.
//! Analysis always runs over the whole reachable program, because that is the
//! only way a question about inheritance or reachability has a true answer.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use piton_compile::{config, Compilation, Project};
use piton_core::Diagnostic;

use crate::index::Index;

pub struct World {
    /// The workspace folder the server was pointed at, so the configuration
    /// can be read again when it changes.
    pub root: PathBuf,
    pub project: Project,
    /// Unsaved editor buffers, keyed by path.
    pub documents: HashMap<PathBuf, String>,
    pub compilation: Option<Compilation>,
    pub index: Index,
    /// Paths that had diagnostics last time, so cleared files get an empty
    /// publish rather than stale squiggles.
    pub last_reported: Vec<PathBuf>,
}

impl World {
    pub fn new(root: &Path) -> World {
        let (project, _) = config::load(root, None);
        World {
            root: root.to_path_buf(),
            project,
            documents: HashMap::new(),
            compilation: None,
            index: Index::default(),
            last_reported: Vec::new(),
        }
    }

    /// Reads `piton.config.pi` again.
    ///
    /// The configuration names the source root and the entry point, so a change
    /// to it is a change to which files are the project at all -- not something
    /// a recompile of the old project would notice.
    pub fn reload_project(&mut self) {
        let (project, _) = config::load(&self.root, None);
        self.project = project;
    }

    pub fn set_document(&mut self, path: PathBuf, text: String) {
        self.documents.insert(path, text);
    }

    pub fn close_document(&mut self, path: &Path) {
        self.documents.remove(path);
    }

    /// The current text of a file: the open buffer if there is one, otherwise
    /// what the compilation loaded, otherwise the file on disk.
    pub fn text(&self, path: &Path) -> Option<String> {
        if let Some(text) = self.documents.get(path) {
            return Some(text.clone());
        }
        if let Some(compilation) = &self.compilation {
            if let Some(source) = compilation.source_of(path) {
                return Some(source.to_string());
            }
        }
        std::fs::read_to_string(path).ok()
    }

    /// Recompiles the project with the current buffers in place.
    ///
    /// Every source under the project root is loaded, not only what the entry
    /// point reaches. Reachability is a question `piton reach` answers, not a
    /// way to decide which files exist: a file nothing imports yet is a file
    /// someone is in the middle of writing, and it needs diagnostics, symbols,
    /// and somewhere to jump to like any other.
    pub fn recompile(&mut self, focus: Option<&Path>) {
        let mut project = self.project.clone();
        if project.config_path.is_none() {
            if let Some(path) = focus {
                // With no configuration there is no project to speak of, so the
                // file being edited is the whole of it.
                project = Project::for_file(path);
            }
        }

        // A buffer open from outside the project root is still being edited,
        // and so is the file the edit just arrived for.
        let mut outside: Vec<PathBuf> = self.documents.keys().cloned().collect();
        outside.extend(focus.map(Path::to_path_buf));

        let compilation =
            Compilation::build_workspace(project, self.documents.clone(), &outside);

        self.index = Index::build(&compilation);
        self.compilation = Some(compilation);
    }

    /// Diagnostics grouped by the file they belong to.
    pub fn diagnostics_by_file(&self) -> HashMap<PathBuf, Vec<Diagnostic>> {
        let mut out: HashMap<PathBuf, Vec<Diagnostic>> = HashMap::new();
        // Every open buffer gets an entry, so clearing a problem clears the
        // squiggle.
        for path in self.documents.keys() {
            out.entry(path.clone()).or_default();
        }
        for path in &self.last_reported {
            out.entry(path.clone()).or_default();
        }
        if let Some(compilation) = &self.compilation {
            for diagnostic in compilation.diagnostics.iter() {
                if diagnostic.file.to_string_lossy().starts_with('@') {
                    // Problems inside a bundled package are not the user's to
                    // fix and have no editable file to attach to.
                    continue;
                }
                out.entry(diagnostic.file.clone())
                    .or_default()
                    .push(diagnostic.clone());
            }
        }
        out
    }
}
