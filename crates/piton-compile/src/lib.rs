//! Module resolution, inheritance, evaluation, and analysis for Piton.

pub mod config;
pub mod digest;
pub mod eval;
pub mod incremental;
pub mod json;
pub mod loc;
pub mod module;
pub mod packages;
pub mod prelude;
pub mod reach;
pub mod resolve;
pub mod slice;
pub mod store;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use piton_core::{AnchorId, AnchorView, DiagnosticSink, Properties, Value};

pub use config::Project;
pub use module::{ModuleGraph, ModuleId};
pub use packages::{Dependency, Lock, PackageDecl, Pin};
pub use resolve::Resolution;
pub use store::{AnchorDef, Store, Symbol, VariableDef, VariableId};

/// A compiled project. The modules have a parse, the names have a resolution
/// and the values have an evaluation.
pub struct Compilation {
    pub project: Project,
    pub resolution: Resolution,
    pub variables: HashMap<VariableId, Value>,
    /// The data that each property and variable read during its evaluation.
    pub reads: eval::Reads,
    pub diagnostics: DiagnosticSink,
    /// The diagnostics of the evaluation, each with the item that the
    /// evaluator evaluated at the time. A later compilation that uses the
    /// value of the item again uses its diagnostics again too.
    pub evaluation: Vec<(Option<eval::Read>, piton_core::Diagnostic)>,
    /// The number of anchors that the evaluation evaluated again, when it
    /// took the other values from the compilation before. `None` when it
    /// evaluated all anchors.
    pub reevaluated: Option<usize>,
    /// When the evaluation took values from the compilation before, the
    /// modules that an edit changed, or that can see other names through
    /// their imports. Each other module has the same text and the same names.
    /// An editor can keep what it found in them before.
    pub affected: Option<std::collections::HashSet<ModuleId>>,
    /// The duration of each phase of the compilation, in order. Only the
    /// compilations of the editor record them. The editor can log them.
    pub timings: Vec<(&'static str, std::time::Duration)>,
}

impl Compilation {
    /// Compiles a project from its entry point.
    pub fn build(project: Project) -> Compilation {
        let resolution = resolve::resolve(&project.entry, project.roots());
        Compilation::from_resolution(project, resolution)
    }

    /// The same as [`Compilation::build`], but the parses come from `cache`
    /// when the text of a file did not change.
    pub fn build_cached(project: Project, cache: &std::sync::Arc<module::ParseCache>) -> Compilation {
        let graph = ModuleGraph::with_cache(Default::default(), cache.clone());
        let resolution = resolve::resolve_with(&project.entry, project.roots(), graph);
        Compilation::from_resolution(project, resolution)
    }

    /// Compiles all sources under the project root, not only the sources that
    /// the entry reaches. The given buffers replace the files at their paths.
    ///
    /// An editor must have this, but a build does not. A file that no module
    /// imports is not part of the program. Thus `build` correctly omits it.
    /// But the file is open on the screen. Without this function, it has no
    /// diagnostics, no symbols and no jump targets until a person imports it.
    /// That is an error of the editor, and not an empty project.
    ///
    /// `also` holds buffers from outside the root. The user edits them,
    /// if the project claims them or not.
    pub fn build_workspace(
        project: Project,
        overrides: std::collections::HashMap<PathBuf, String>,
        also: &[PathBuf],
    ) -> Compilation {
        let resolution = Self::resolve_workspace(&project, ModuleGraph::with_overrides(overrides), also);
        Compilation::from_resolution(project, resolution)
    }

    /// The same as [`Compilation::build_workspace`], but the parses come from
    /// `cache` when the text of a file did not change. The cache then keeps
    /// only the files of this compilation.
    pub fn build_workspace_cached(
        project: Project,
        overrides: std::collections::HashMap<PathBuf, String>,
        also: &[PathBuf],
        cache: &std::sync::Arc<module::ParseCache>,
        previous: Option<&Compilation>,
    ) -> Compilation {
        let started = std::time::Instant::now();
        // The sources and the installed packages, parsed on all cores. Only
        // the first compilation has many files to parse.
        let mut files = module::sources(&project.source_root);
        for name in packages::installed_names(&project.root) {
            files.extend(module::sources(&packages::package_directory(&project.root, &name)));
        }
        files.extend(also.iter().cloned());
        cache.prewarm(&files, &overrides);
        let parsed = started.elapsed();
        let started = std::time::Instant::now();
        let graph = ModuleGraph::with_cache(overrides, cache.clone());
        let resolution = Self::resolve_workspace(&project, graph, also);
        cache.keep_only(&resolution.graph);
        let resolved = started.elapsed();
        let started = std::time::Instant::now();
        // The compilation before is only useful for the same project.
        let previous = previous.filter(|previous| previous.project.root == project.root);
        let mut compilation = Compilation::from_resolution_after(project, resolution, previous);
        compilation.timings = vec![
            ("parse", parsed),
            ("load and resolve", resolved),
            ("evaluate", started.elapsed()),
        ];
        compilation
    }

    /// Resolves each source below the source root and each file in `also`.
    fn resolve_workspace(project: &Project, graph: ModuleGraph, also: &[PathBuf]) -> Resolution {
        let mut roots = module::sources(&project.source_root);
        roots.extend(also.iter().cloned());
        roots.sort();
        roots.dedup();
        resolve::resolve_including(&project.entry, &roots, project.roots(), graph)
    }

    /// Compiles the project from its entry, and also the files in `also`. The
    /// files in `also` compile as if a module imported them. No other source
    /// under the root compiles. This function compiles a file that no module
    /// imports when it holds an anchor that a plugin reaches. Thus the other
    /// files still count as files that no module imports.
    pub fn build_including(project: Project, also: &[PathBuf]) -> Compilation {
        let mut roots: Vec<PathBuf> = also.to_vec();
        roots.sort();
        roots.dedup();
        let graph = ModuleGraph::with_overrides(Default::default());
        let resolution = resolve::resolve_including(&project.entry, &roots, project.roots(), graph);
        Compilation::from_resolution(project, resolution)
    }

    /// Compiles a graph that has a resolution. The language server uses this
    /// to put unsaved buffers in place of files.
    pub fn from_resolution(project: Project, resolution: Resolution) -> Compilation {
        Compilation::from_resolution_after(project, resolution, None)
    }

    /// The same as [`Compilation::from_resolution`]. When `previous` has the
    /// same items, the values that no edit can change come from it, and
    /// only the other values are evaluated. See [`incremental`].
    pub fn from_resolution_after(
        project: Project,
        mut resolution: Resolution,
        previous: Option<&Compilation>,
    ) -> Compilation {
        let mut reevaluated = None;
        let mut affected = None;
        let outcome = match previous.and_then(|previous| Some((previous, incremental::plan(previous, &resolution)?))) {
            Some((previous, plan)) => {
                reevaluated = Some(plan.dirty_anchors.len());
                affected = Some(plan.modules.clone());
                let reuse = eval::Reuse {
                    store: &previous.resolution.store,
                    reads: &previous.reads,
                    diagnostics: &previous.evaluation,
                    dirty_anchors: &plan.dirty_anchors,
                    dirty_variables: &plan.dirty_variables,
                };
                let outcome = eval::evaluate_again(&resolution, &reuse);
                if std::env::var_os("PITON_VERIFY_INCREMENTAL").is_some() {
                    verify(&resolution, &outcome);
                }
                outcome
            }
            None => eval::evaluate(&resolution),
        };

        let mut diagnostics = DiagnosticSink::new();
        // Syntax problems are part of the compilation. The problems from the
        // resolver and the evaluator are also part of it.
        for module in resolution.graph.iter() {
            diagnostics.extend(module.parse.diagnostics.iter().cloned());
        }
        diagnostics.extend(std::mem::take(&mut resolution.diagnostics));
        diagnostics.extend(outcome.diagnostics.iter().map(|(_, diagnostic)| diagnostic.clone()));

        // Each anchor keeps its values in the store. A second copy took time
        // and memory in a large project.
        for (anchor, properties) in outcome.anchors {
            resolution.store.anchor_mut(anchor).properties = properties;
        }
        for (variable, value) in &outcome.variables {
            resolution.store.variable_mut(*variable).value = Some(value.clone());
        }

        diagnostics.sort();

        Compilation {
            project,
            resolution,
            variables: outcome.variables,
            reads: outcome.reads,
            diagnostics,
            evaluation: outcome.diagnostics,
            reevaluated,
            affected,
            timings: Vec::new(),
        }
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.has_errors()
    }

    pub fn store(&self) -> &Store {
        &self.resolution.store
    }

    pub fn graph(&self) -> &ModuleGraph {
        &self.resolution.graph
    }

    /// The source text of a loaded module. Diagnostics use it for their output.
    pub fn source_of(&self, path: &Path) -> Option<&str> {
        self.resolution
            .graph
            .id_for(path)
            .map(|id| self.resolution.graph.get(id).source.as_str())
    }

    /// All anchors that the entry module exports, in declaration order.
    /// None when the entry did not load.
    pub fn entry_exports(&self) -> Vec<AnchorId> {
        if !self.resolution.has_entry {
            return Vec::new();
        }
        let entry = self.resolution.entry;
        self.resolution
            .exported_names(entry)
            .into_iter()
            .filter_map(|name| {
                match self
                    .resolution
                    .lookup_export(entry, &name, &mut Default::default())
                {
                    Some(Symbol::Anchor(anchor)) => Some(anchor),
                    _ => None,
                }
            })
            .collect()
    }

    /// All the output of a module, in the order of emission.
    ///
    /// A compiled file is an object with one key for each export. The output
    /// does not include items that the module does not export. It also does
    /// not include abstract anchors. An abstract anchor declares a shape and
    /// has no value. An export alone is sufficient. Thus an `index.pi` that only
    /// gives the anchors of its directory compiles to those anchors.
    pub fn compiled_surface(&self, module: ModuleId) -> Properties {
        let mut out = Properties::new();
        for name in self.resolution.exported_names(module) {
            if let Some(symbol) =
                self.resolution
                    .lookup_export(module, &name, &mut Default::default())
            {
                self.emit_symbol(&mut out, &name, symbol);
            }
        }
        out
    }

    /// Records the compiled value of one name. It ignores a name with no value.
    fn emit_symbol(&self, out: &mut Properties, name: &str, symbol: Symbol) {
        let value = match symbol {
            Symbol::Anchor(anchor) => {
                if self.store().anchor(anchor).is_abstract {
                    return;
                }
                Value::Anchor(anchor)
            }
            Symbol::Variable(variable) => self
                .store()
                .variable(variable)
                .value
                .clone()
                .unwrap_or(Value::Null),
        };
        out.insert(name.to_string(), value);
    }

    /// Finds an anchor by name in all of the compilation.
    pub fn find_anchor(&self, name: &str) -> Option<AnchorId> {
        self.resolution
            .store
            .anchors
            .iter()
            .find(|def| def.name == name)
            .map(|def| def.id)
    }

    /// The location where an anchor names `property` directly. None when the
    /// property comes from a base or is not there. A plugin puts its
    /// diagnostics at this location.
    pub fn property_span(&self, anchor: AnchorId, property: &str) -> Option<piton_core::Span> {
        let def = self.resolution.store.anchor(anchor);
        match self.resolution.graph.get(def.module).ast().items.get(def.item) {
            Some(piton_syntax::ast::Item::Anchor(decl)) => decl
                .body
                .properties()
                .find(|written| written.name == property)
                .map(|written| written.name_span),
            _ => None,
        }
    }

    /// The path of the module that declares an anchor.
    pub fn anchor_module_path(&self, anchor: AnchorId) -> PathBuf {
        let def = self.resolution.store.anchor(anchor);
        self.resolution.graph.get(def.module).path.clone()
    }
}

impl AnchorView for Compilation {
    fn name(&self, id: AnchorId) -> &str {
        &self.resolution.store.anchor(id).name
    }

    fn properties(&self, id: AnchorId) -> &Properties {
        &self.resolution.store.anchor(id).properties
    }

    fn source_path(&self, id: AnchorId) -> &Path {
        let def = self.resolution.store.anchor(id);
        &self.resolution.graph.get(def.module).path
    }

    fn is_abstract(&self, id: AnchorId) -> bool {
        self.resolution.store.anchor(id).is_abstract
    }
}

/// Compares an evaluation that used values again with a full evaluation, and
/// reports each difference on stderr. `PITON_VERIFY_INCREMENTAL` turns this
/// on, for tests and for a person who suspects a stale value.
fn verify(resolution: &Resolution, outcome: &eval::Outcome) {
    let full = eval::evaluate(resolution);
    let store = &resolution.store;
    for (anchor, properties) in &full.anchors {
        if outcome.anchors.get(anchor) != Some(properties) {
            eprintln!("incremental evaluation differs for anchor `{}`", store.anchor(*anchor).name);
        }
    }
    for (variable, value) in &full.variables {
        if outcome.variables.get(variable) != Some(value) {
            eprintln!("incremental evaluation differs for variable `{}`", store.variable(*variable).name);
        }
    }
    let describe = |diagnostics: &[(Option<eval::Read>, piton_core::Diagnostic)]| {
        let mut out: Vec<String> = diagnostics
            .iter()
            .map(|(_, diagnostic)| format!("{} {:?} {} {}", diagnostic.file.display(), diagnostic.span, diagnostic.code, diagnostic.message))
            .collect();
        out.sort();
        out.dedup();
        out
    };
    let (expected, found) = (describe(&full.diagnostics), describe(&outcome.diagnostics));
    if expected != found {
        eprintln!("incremental evaluation differs in its diagnostics:\n  full: {expected:?}\n  incremental: {found:?}");
    }
}
