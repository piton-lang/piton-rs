//! Module resolution, inheritance, evaluation, and analysis for Piton.

pub mod config;
pub mod digest;
pub mod eval;
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

pub use config::{BelayConfig, Framework, Project};
pub use module::{ModuleGraph, ModuleId};
pub use packages::{Dependency, Lock, PackageDecl, Pin};
pub use resolve::Resolution;
pub use store::{AnchorDef, Store, Symbol, VariableDef, VariableId};

/// A fully compiled project: modules parsed, names resolved, values evaluated.
pub struct Compilation {
    pub project: Project,
    pub resolution: Resolution,
    pub values: HashMap<AnchorId, Properties>,
    pub variables: HashMap<VariableId, Value>,
    /// What each property and variable read while it was evaluated.
    pub reads: eval::Reads,
    pub diagnostics: DiagnosticSink,
}

impl Compilation {
    /// Compiles a project from its entry point.
    pub fn build(project: Project) -> Compilation {
        let resolution = resolve::resolve(&project.entry, project.roots());
        Compilation::from_resolution(project, resolution)
    }

    /// Compiles every source under the project root, not only what the entry
    /// reaches, with the given buffers in place of the files they shadow.
    ///
    /// This is what an editor wants and what a build does not. A file nothing
    /// imports is not part of the program, so `build` is right to leave it out;
    /// but it is open on screen, and having no diagnostics, no symbols and
    /// nothing to jump to until someone remembers to import it is the editor
    /// being wrong about the project rather than the project being empty.
    ///
    /// `also` carries buffers from outside the root, which are being edited
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

    /// Compiles an already-resolved graph. The language server uses this so it
    /// can substitute unsaved buffers.
    pub fn from_resolution(project: Project, mut resolution: Resolution) -> Compilation {
        let outcome = eval::evaluate(&resolution);

        let mut diagnostics = DiagnosticSink::new();
        // Syntax problems belong to the compilation as much as anything the
        // resolver or evaluator finds.
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

    /// The source text of a loaded module, for rendering diagnostics.
    pub fn source_of(&self, path: &Path) -> Option<&str> {
        self.resolution
            .graph
            .id_for(path)
            .map(|id| self.resolution.graph.get(id).source.as_str())
    }

    /// Every anchor exported from the entry module, in declaration order.
    pub fn entry_exports(&self) -> Vec<AnchorId> {
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

    /// Everything a module compiles to, in the order it should be emitted.
    ///
    /// A compiled file is an object with one key per export. Anything that
    /// isn't exported is left out, and so are abstract anchors, which declare a
    /// shape and have no value of their own. Being exported is enough on its
    /// own: an `index.pi` that only forwards its directory's anchors compiles
    /// to those anchors.
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

    /// Records one name's compiled value, skipping what has none.
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

    /// Finds an anchor by name anywhere in the compilation.
    pub fn find_anchor(&self, name: &str) -> Option<AnchorId> {
        self.resolution
            .store
            .anchors
            .iter()
            .find(|def| def.name == name)
            .map(|def| def.id)
    }

    /// The path of the module an anchor was declared in.
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
