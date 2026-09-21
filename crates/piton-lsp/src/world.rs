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
            project,
            documents: HashMap::new(),
            compilation: None,
            index: Index::default(),
            last_reported: Vec::new(),
        }
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
    /// A file that the project does not reach is still analyzed, as its own
    /// entry point, so editing it is not a silent no-op.
    pub fn recompile(&mut self, focus: Option<&Path>) {
        let mut project = self.project.clone();
        if project.config_path.is_none() {
            if let Some(path) = focus {
                project = Project::for_file(path);
            }
        }

        let mut compilation =
            Compilation::build_with_overrides(project.clone(), self.documents.clone());

        if let Some(path) = focus {
            if compilation.graph().id_for(path).is_none() {
                // The project does not reach this file; compile it on its own so
                // the editor still gets diagnostics for it.
                let standalone = Project {
                    entry: path.to_path_buf(),
                    ..project
                };
                compilation =
                    Compilation::build_with_overrides(standalone, self.documents.clone());
            }
        }

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
