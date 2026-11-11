//! Workspace state.
//!
//! The server keeps the open buffers and one compiled view of the project.
//! Analysis always runs over the full reachable program. Only then does a
//! question about inheritance or reachability have a true answer.
//!
//! The server does not compile when no change occurred. It records the
//! buffers that a compilation used. A request for a new view does nothing
//! until one of these buffers (or the project on disk) changes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use piton_compile::{config, Compilation, Project};
use piton_core::Diagnostic;
use piton_emit::Adapter;
use piton_syntax::language;

use crate::analysis::Analysis;
use crate::index::Index;

/// The location where `piton compile` writes, and the format.
///
/// The values come from `output` and `renderer` in the project configuration.
/// The defaults come from the specification: `./dist`, as JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputSettings {
    pub directory: PathBuf,
    pub renderer: Adapter,
}

impl OutputSettings {
    /// The compiled file that a module makes. It is the output directory, then
    /// the path of the module relative to the source root, with the extension
    /// of the renderer.
    pub fn output_for(&self, project: &Project, module: &Path) -> PathBuf {
        self.output_with(project, module, self.renderer)
    }

    /// The same, for a renderer that is not the configured renderer.
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

/// The output directory and renderer from the project configuration
/// (`output` and `renderer` in `piton.config.pi`). When the configuration
/// does not give them, they are `./dist` and JSON.
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

/// The capabilities that the client gave at `initialize`. The server reads
/// them one time. All are off by default, because a client that gives no
/// capabilities supports none.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClientCaps {
    /// `textDocument.definition.linkSupport`: the answer is a `LocationLink`.
    pub link_support: bool,
    /// `workspace.workspaceEdit.documentChanges`: versioned edits.
    pub document_changes: bool,
    /// `completionItem.insertReplaceSupport`.
    pub insert_replace: bool,
    /// `workspace.semanticTokens.refreshSupport`.
    pub semantic_refresh: bool,
    /// `workspace.inlayHint.refreshSupport`.
    pub inlay_refresh: bool,
    /// `workspace.codeLens.refreshSupport`.
    pub lens_refresh: bool,
}

pub struct World {
    /// The workspace folder of the server. The server uses it to read the
    /// configuration again when the configuration changes.
    pub root: PathBuf,
    pub project: Project,
    pub output: OutputSettings,
    /// The problems that the configuration read found. Examples are unknown
    /// settings, a bad renderer, and no `piton-config` anchor.
    pub config_diagnostics: Vec<Diagnostic>,
    /// Editor buffers that the user did not save, by path.
    pub documents: HashMap<PathBuf, String>,
    /// The version that the client gave to each open buffer.
    pub versions: HashMap<PathBuf, i32>,
    /// The features that the client supports.
    pub caps: ClientCaps,
    /// The count of compilations. An answer made for one compilation can use
    /// it to know that a question comes after a different compilation.
    pub generation: u64,
    pub compilation: Option<Compilation>,
    pub index: Index,
    /// Import, composition, conflict, and source-map analysis of `compilation`.
    pub analysis: Analysis,
    /// Paths that had diagnostics the last time. Thus files without problems
    /// get an empty publish, not old squiggles.
    pub last_reported: Vec<PathBuf>,
    /// The buffers that `compilation` used, and their versions.
    compiled_documents: HashMap<PathBuf, String>,
    compiled_versions: HashMap<PathBuf, i32>,
    /// The server sets it when a change occurs outside the buffers. Examples
    /// are the configuration, a file on disk, and a closed buffer.
    stale: bool,
    /// The plans of each plugin build at the last save.
    pub plugin_plans: Vec<piton_plugin::plan::PluginPlan>,
    /// The items that each plugin reached at the last save, by file and name.
    pub plugin_reached: Vec<(PathBuf, String)>,
    /// The server sets it when a save or the configuration makes new plans
    /// necessary. A keystroke never sets it.
    plans_stale: bool,
    /// The file that the user edited last. It is the project when there is no
    /// configuration.
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
            versions: HashMap::new(),
            caps: ClientCaps::default(),
            generation: 0,
            compilation: None,
            index: Index::default(),
            analysis: Analysis::default(),
            last_reported: Vec::new(),
            compiled_documents: HashMap::new(),
            compiled_versions: HashMap::new(),
            stale: true,
            plugin_plans: Vec::new(),
            plugin_reached: Vec::new(),
            plans_stale: true,
            focus: None,
        }
    }

    /// The user saved a file. The next refresh plans the plugin builds again.
    pub fn saved(&mut self) {
        self.plans_stale = true;
        self.stale = true;
    }

    /// Reads `piton.config.pi` again.
    ///
    /// The configuration names the source root and the entry point. Thus a
    /// change to it changes the set of files in the project. A recompile of the
    /// old project would not see this change.
    pub fn reload_project(&mut self) {
        self.plans_stale = true;
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

    /// An open buffer, at the version number that the client gave it.
    pub fn open_document(&mut self, path: PathBuf, text: String, version: i32) {
        self.versions.insert(path.clone(), version);
        self.set_document(path, text);
    }

    /// The version of the buffer that the current compilation read, for a file
    /// that was open at that time.
    pub fn compiled_version(&self, path: &Path) -> Option<i32> {
        self.compiled_versions.get(path).copied()
    }

    /// The current version of an open buffer.
    pub fn version(&self, path: &Path) -> Option<i32> {
        self.versions.get(path).copied()
    }

    pub fn close_document(&mut self, path: &Path) {
        self.documents.remove(path);
        self.versions.remove(path);
        if self.focus.as_deref() == Some(path) {
            self.focus = None;
        }
        self.stale = true;
    }

    /// Marks the compilation as old for a reason that the buffers do not show.
    /// An example is a file that changed on disk.
    pub fn invalidate(&mut self) {
        self.stale = true;
    }

    /// Whether the compilation is different from the buffers or the disk.
    pub fn needs_compile(&self) -> bool {
        self.stale || self.compilation.is_none() || self.compiled_documents != self.documents
    }

    /// The current text of a file. This is the open buffer if there is one.
    /// If not, it is the text that the compilation loaded. If not, it is the
    /// file on disk.
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

    /// Recompiles only when a change occurred after the last compilation.
    /// Returns whether it compiled.
    pub fn refresh(&mut self, focus: Option<&Path>) -> bool {
        if !self.needs_compile() {
            return false;
        }
        self.recompile(focus);
        true
    }

    /// Recompiles the project with the current buffers.
    ///
    /// The server loads all sources below the project root, not only the
    /// sources that the entry point reaches. `piton reach` answers questions
    /// about reachability. Reachability does not decide which files exist. A
    /// file that no file imports yet is a file that a person writes now. It
    /// needs diagnostics, symbols, and jump targets, as all files do.
    pub fn recompile(&mut self, focus: Option<&Path>) {
        if let Some(path) = focus {
            self.focus = Some(path.to_path_buf());
        }
        let mut project = self.project.clone();
        if project.config_path.is_none() {
            // When there is no configuration, there is no project. Thus the
            // file that the user edits is the full project. When the user
            // edits no file, there is nothing to compile. A compile of the
            // workspace directory as an entry point would only make noise.
            // It would also race the first `didOpen`.
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

        // The user still edits a buffer that is open from outside the project
        // root. The user also edits the file that the edit came for.
        let mut outside: Vec<PathBuf> = self.documents.keys().cloned().collect();
        outside.extend(focus.map(Path::to_path_buf));

        let compilation = Compilation::build_workspace(project, self.documents.clone(), &outside);

        if self.plans_stale {
            self.plugin_plans = crate::plugins::plan(&self.project);
            self.plugin_reached = crate::plugins::reached(&self.project);
            self.plans_stale = false;
        }
        self.index = Index::build(&compilation);
        self.analysis = crate::analysis::analyze(
            &compilation,
            &self.index,
            &self.output,
            &self.plugin_plans,
            &self.plugin_reached,
        );
        self.compilation = Some(compilation);
        self.compiled_documents = self.documents.clone();
        self.compiled_versions = self.versions.clone();
        self.generation += 1;
        self.stale = false;
    }

    /// Diagnostics, in groups by the file that they belong to.
    pub fn diagnostics_by_file(&self) -> HashMap<PathBuf, Vec<Diagnostic>> {
        let mut out: HashMap<PathBuf, Vec<Diagnostic>> = HashMap::new();
        // Each open buffer gets an entry. Thus when a problem goes away, its
        // squiggle also goes away.
        for path in self.documents.keys() {
            out.entry(path.clone()).or_default();
        }
        for path in &self.last_reported {
            out.entry(path.clone()).or_default();
        }
        if let Some(compilation) = &self.compilation {
            for diagnostic in compilation.diagnostics.iter() {
                // A diagnostic without a file has no location to show.
                if diagnostic.file.as_os_str().is_empty() {
                    continue;
                }
                if diagnostic.file.to_string_lossy().starts_with('@') {
                    // The user cannot fix problems inside a bundled package.
                    // Also, these problems have no editable file.
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
            // The server reads the configuration separately. When it is open,
            // the server also compiles it with the workspace. Thus the same
            // problem can occur two times.
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
