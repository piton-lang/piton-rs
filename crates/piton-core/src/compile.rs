//! The compile driver: load, resolve, validate, evaluate.

use std::collections::HashMap;
use std::path::Path;

use crate::db::{Db, Source};
use crate::diag::Diagnostics;
use crate::eval::Evaluator;
use crate::framework::Frameworks;
use crate::resolve::{load_graph, Analysis, Symbol};
use crate::validate::validate;
use crate::value::{Anchor, AnchorId, Value};
use crate::{hir, FileId};

/// Everything one build produced.
pub struct Compilation {
    pub analysis: Analysis,
    /// Compiled anchors, keyed by id.
    pub anchors: HashMap<AnchorId, Anchor>,
    /// Compiled top-level variables.
    pub vars: HashMap<(FileId, usize), Value>,
    /// Anchors reached during evaluation, in discovery order.
    pub reached: Vec<AnchorId>,
    /// The files the build started from.
    pub entries: Vec<FileId>,
    pub diagnostics: Diagnostics,
}

/// Load, resolve, validate, and evaluate everything reachable from `entries`.
pub fn compile(mut db: Db, entries: Vec<FileId>, frameworks: &Frameworks) -> Compilation {
    let (modules, load_errors) = load_graph(&mut db, &entries);
    let mut analysis = Analysis::new(db, modules);
    let mut diagnostics = std::mem::take(&mut analysis.diagnostics);
    diagnostics.extend(load_errors);
    diagnostics.extend(validate(&analysis).into_vec());

    let mut anchors = HashMap::new();
    let mut vars = HashMap::new();
    let reached;
    {
        let mut evaluator = Evaluator::new(&analysis, frameworks);
        let files: Vec<FileId> = evaluator.analysis.db.files().map(|file| file.id).collect();
        for file in files {
            let anchor_count = evaluator.analysis.db.file(file).hir.anchors.len();
            let var_count = evaluator.analysis.db.file(file).hir.vars.len();
            for index in 0..anchor_count {
                if let Some(id) = evaluator.analysis.anchor_id(file, index) {
                    if !evaluator.analysis.anchor_def(id).is_abstract {
                        let anchor = evaluator.anchor(id);
                        anchors.insert(id, anchor);
                    }
                }
            }
            for index in 0..var_count {
                let value = evaluator.var(file, index);
                vars.insert((file, index), value);
            }
        }
        diagnostics.extend(evaluator.diagnostics.into_vec());
        reached = evaluator.reached;
    }

    Compilation { analysis, anchors, vars, reached, entries, diagnostics }
}

impl Compilation {
    pub fn def(&self, id: AnchorId) -> &hir::AnchorDef {
        self.analysis.anchor_def(id)
    }

    pub fn file_of(&self, id: AnchorId) -> FileId {
        self.analysis.anchor_loc(id).file
    }

    pub fn source_path(&self, id: AnchorId) -> Option<&Path> {
        self.analysis.db.file(self.file_of(id)).source.as_path()
    }

    pub fn anchor(&self, id: AnchorId) -> Option<&Anchor> {
        self.anchors.get(&id)
    }

    /// Find a named anchor exported by a module, e.g. `@piton/belay` / `Agent`.
    pub fn lookup(&self, module: &str, name: &str) -> Option<AnchorId> {
        let file = self
            .analysis
            .db
            .files()
            .find(|file| file.source == Source::Virtual(module.to_string()))?;
        match self.analysis.scope(file.id).exports.get(name) {
            Some(Symbol::Anchor(id)) => Some(*id),
            _ => None,
        }
    }

    /// Concrete anchors whose inheritance chain reaches `base`, in file order.
    pub fn implementors(&self, base: AnchorId) -> Vec<AnchorId> {
        self.analysis
            .implementors(base)
            .into_iter()
            .filter(|id| self.anchors.contains_key(id))
            .collect()
    }

    /// Every top-level value the build produced, for a whole-file compile.
    pub fn file_values(&self, file: FileId) -> Vec<(String, Value)> {
        let hir = &self.analysis.db.file(file).hir;
        let mut out = Vec::new();
        for (index, var) in hir.vars.iter().enumerate() {
            if let Some(value) = self.vars.get(&(file, index)) {
                out.push((var.name.clone(), value.clone()));
            }
        }
        for (index, def) in hir.anchors.iter().enumerate() {
            if def.is_abstract {
                continue;
            }
            if let Some(id) = self.analysis.anchor_id(file, index) {
                if let Some(anchor) = self.anchors.get(&id) {
                    out.push((def.name.clone(), Value::Anchor(anchor.clone())));
                }
            }
        }
        out
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.has_errors()
    }
}
