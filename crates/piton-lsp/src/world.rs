//! Workspace state.
//!
//! The server keeps the open buffers and one compiled view of the project.
//! Analysis always runs over the whole reachable program, because that is the
//! only way a question about inheritance or reachability has a true answer.
//!
//! What it avoids is compiling when nothing changed: the buffers a compilation
//! was built from are remembered, and a request for a fresh view is a no-op
//! until one of them (or the project on disk) moves.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use piton_compile::{config, Compilation, Project};
use piton_core::Diagnostic;
use piton_emit::Adapter;
use piton_syntax::language;

use crate::analysis::Analysis;
use crate::index::Index;

/// Where `piton compile` writes, and in which format.
///
/// Read from the project configuration's `output` and `renderer`. The defaults
/// are the specification's: `./dist`, as JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputSettings {
    pub directory: PathBuf,
    pub renderer: Adapter,
}

impl OutputSettings {
    /// The compiled file a module produces: the output directory, then the
    /// module's path relative to the source root, with the renderer's
    /// extension.
    pub fn output_for(&self, project: &Project, module: &Path) -> PathBuf {
        self.output_with(project, module, self.renderer)
    }

    /// The same, for a renderer other than the configured one.
    pub fn output_with(&self, project: &Project, module: &Path, adapter: Adapter) -> PathBuf {
        let relative = module
            .strip_prefix(&project.source_root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| {
                module
                    .file_name()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("output"))
            });
        self.directory
            .join(relative)
            .with_extension(adapter.extension())
    }
}

/// The output directory and renderer the project is configured with
/// (`output` and `renderer` in `piton.config.pi`; `./dist` and JSON when left
/// out).
pub fn output_settings(project: &Project) -> OutputSettings {
    OutputSettings {
        directory: project.output_dir.clone(),
        renderer: project.renderer.parse::<Adapter>().unwrap_or(Adapter::Json),
    }
}

/// Whether a path is the project configuration file.
pub fn is_config_file(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name == language::CONFIG_FILE)
}

pub struct World {
    /// The workspace folder the server was pointed at, so the configuration
    /// can be read again when it changes.
    pub root: PathBuf,
    pub project: Project,
    pub output: OutputSettings,
    /// What reading the configuration reported: unknown settings, a bad
    /// renderer, a missing `piton-config` anchor.
    pub config_diagnostics: Vec<Diagnostic>,
    /// Unsaved editor buffers, keyed by path.
    pub documents: HashMap<PathBuf, String>,
    pub compilation: Option<Compilation>,
    pub index: Index,
    /// Import, composition, conflict, and source-map analysis of `compilation`.
    pub analysis: Analysis,
    /// Paths that had diagnostics last time, so cleared files get an empty
    /// publish rather than stale squiggles.
    pub last_reported: Vec<PathBuf>,
    /// The buffers `compilation` was built from.
    compiled_documents: HashMap<PathBuf, String>,
    /// Set when something outside the buffers changed: the configuration, a
    /// file on disk, a closed buffer.
    stale: bool,
    /// The file most recently edited, which stands in for the project when
    /// there is no configuration.
    focus: Option<PathBuf>,
}

impl World {
    pub fn new(root: &Path) -> World {
        let (project, diagnostics) = config::load(root, None);
        let output = output_settings(&project);
        World {
            root: root.to_path_buf(),
            project,
            output,
            config_diagnostics: diagnostics.iter().cloned().collect(),
            documents: HashMap::new(),
            compilation: None,
            index: Index::default(),
            analysis: Analysis::default(),
            last_reported: Vec::new(),
            compiled_documents: HashMap::new(),
            stale: true,
            focus: None,
        }
    }

    /// Reads `piton.config.pi` again.
    ///
    /// The configuration names the source root and the entry point, so a change
    /// to it is a change to which files are the project at all -- not something
    /// a recompile of the old project would notice.
    pub fn reload_project(&mut self) {
        let (project, diagnostics) = config::load(&self.root, None);
        self.output = output_settings(&project);
        self.config_diagnostics = diagnostics.iter().cloned().collect();
        self.project = project;
        self.stale = true;
    }

    pub fn set_document(&mut self, path: PathBuf, text: String) {
        self.focus = Some(path.clone());
        self.documents.insert(path, text);
    }

    pub fn close_document(&mut self, path: &Path) {
        self.documents.remove(path);
        if self.focus.as_deref() == Some(path) {
            self.focus = None;
        }
        self.stale = true;
    }

    /// Marks the compilation out of date for a reason the buffers do not show,
    /// such as a file written on disk.
    pub fn invalidate(&mut self) {
        self.stale = true;
    }

    /// Whether the compilation no longer matches the buffers or the disk.
    pub fn needs_compile(&self) -> bool {
        self.stale || self.compilation.is_none() || self.compiled_documents != self.documents
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

    /// Recompiles only when something changed since the last compilation.
    /// Returns whether it compiled.
    pub fn refresh(&mut self, focus: Option<&Path>) -> bool {
        if !self.needs_compile() {
            return false;
        }
        self.recompile(focus);
        true
    }

    /// Recompiles the project with the current buffers in place.
    ///
    /// Every source under the project root is loaded, not only what the entry
    /// point reaches. Reachability is a question `piton reach` answers, not a
    /// way to decide which files exist: a file nothing imports yet is a file
    /// someone is in the middle of writing, and it needs diagnostics, symbols,
    /// and somewhere to jump to like any other.
    pub fn recompile(&mut self, focus: Option<&Path>) {
        if let Some(path) = focus {
            self.focus = Some(path.to_path_buf());
        }
        let mut project = self.project.clone();
        if project.config_path.is_none() {
            // With no configuration there is no project to speak of, so the
            // file being edited is the whole of it. With nothing being edited
            // either, there is nothing to compile: compiling the workspace
            // directory as though it were an entry point would only produce
            // noise (and would race the first `didOpen`).
            let focus = self
                .focus
                .clone()
                .filter(|path| self.documents.contains_key(path) || path.is_file())
                .or_else(|| {
                    let mut open: Vec<&PathBuf> = self.documents.keys().collect();
                    open.sort();
                    open.first().map(|path| (*path).clone())
                });
            match focus {
                Some(path) => project = Project::for_file(&path),
                None => return,
            }
        }

        // A buffer open from outside the project root is still being edited,
        // and so is the file the edit just arrived for.
        let mut outside: Vec<PathBuf> = self.documents.keys().cloned().collect();
        outside.extend(focus.map(Path::to_path_buf));

        let compilation = Compilation::build_workspace(project, self.documents.clone(), &outside);

        self.index = Index::build(&compilation);
        self.analysis = crate::analysis::analyze(&compilation, &self.index, &self.output);
        self.compilation = Some(compilation);
        self.compiled_documents = self.documents.clone();
        self.stale = false;
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
                // A diagnostic with no file has nowhere to be shown.
                if diagnostic.file.as_os_str().is_empty() {
                    continue;
                }
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
        let extra = self
            .analysis
            .diagnostics
            .iter()
            .chain(&self.config_diagnostics);
        for diagnostic in extra {
            if diagnostic.file.to_string_lossy().starts_with('@') {
                continue;
            }
            let list = out.entry(diagnostic.file.clone()).or_default();
            // The configuration is read on its own and, when it is open, also
            // compiled with the workspace, so the same problem can arrive
            // twice.
            let repeated = list.iter().any(|seen| {
                seen.code == diagnostic.code
                    && seen.span == diagnostic.span
                    && seen.message == diagnostic.message
            });
            if !repeated {
                list.push(diagnostic.clone());
            }
        }
        out
    }
}
