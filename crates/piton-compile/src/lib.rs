//! Module resolution, inheritance, evaluation, and analysis for Piton.

pub mod config;
pub mod digest;
pub mod eval;
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
    pub values: HashMap<AnchorId, Properties>,
    pub variables: HashMap<VariableId, Value>,
    /// The data that each property and variable read during its evaluation.
    pub reads: eval::Reads,
    pub diagnostics: DiagnosticSink,
}

impl Compilation {
    /// Compiles a project from its entry point.
    pub fn build(project: Project) -> Compilation {
        let resolution = resolve::resolve(&project.entry, project.roots());
        Compilation::from_resolution(project, resolution)
    }

    /// Compiles all sources under the project root, not only the sources that
    /// the entry reaches. The given buffers replace the files that they shadow.
    ///
    /// An editor needs this, but a build does not. A file that no module
    /// imports is not part of the program. Thus `build` correctly omits it.
    /// But the file is open on the screen. Without this function, it has no
    /// diagnostics, no symbols and no jump targets until a person imports it.
    /// That is an error of the editor, and not an empty project.
    ///
    /// `also` holds buffers from outside the root. The user edits them,
    /// whether or not the project claims them.
    pub fn build_workspace(
        project: Project,
        overrides: std::collections::HashMap<PathBuf, String>,
        also: &[PathBuf],
    ) -> Compilation {
        let mut roots = module::sources(&project.source_root);
        roots.extend(also.iter().cloned());
        roots.sort();
        roots.dedup();

        let graph = ModuleGraph::with_overrides(overrides);
        let resolution = resolve::resolve_including(&project.entry, &roots, project.roots(), graph);
        Compilation::from_resolution(project, resolution)
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
    pub fn from_resolution(project: Project, mut resolution: Resolution) -> Compilation {
        let outcome = eval::evaluate(&resolution);

        let mut diagnostics = DiagnosticSink::new();
        // Syntax problems are part of the compilation. The problems from the
        // resolver and the evaluator are also part of it.
        for module in resolution.graph.iter() {
            diagnostics.extend(module.parse.diagnostics.iter().cloned());
        }
        diagnostics.extend(std::mem::take(&mut resolution.diagnostics));
        diagnostics.extend(outcome.diagnostics);

        for (anchor, properties) in &outcome.anchors {
            resolution.store.anchor_mut(*anchor).properties = properties.clone();
        }
        for (variable, value) in &outcome.variables {
            resolution.store.variable_mut(*variable).value = Some(value.clone());
        }

        diagnostics.sort();

        Compilation {
            project,
            resolution,
            values: outcome.anchors,
            variables: outcome.variables,
            reads: outcome.reads,
            diagnostics,
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
