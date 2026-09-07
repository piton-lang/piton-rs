//! Name resolution: what each file can see, and what it hands to others.
//!
//! Piton scoping is small: a file's top-level names are its own declarations
//! plus whatever it imported, `export` publishes a name, and `use` brings in
//! only the user-defined keywords a module exports.

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use piton_syntax::TextRange;

use crate::db::Db;
use crate::diag::{Diagnostic, Diagnostics};
use crate::hir::Spanned;
use crate::value::AnchorId;
use crate::FileId;

/// Where an anchor was declared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnchorLoc {
    pub file: FileId,
    pub index: usize,
}

/// Something a name can refer to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Symbol {
    Anchor(AnchorId),
    Var { file: FileId, index: usize },
}

/// What one file can see.
#[derive(Clone, Debug, Default)]
pub struct FileScope {
    /// Every name usable in an expression in this file.
    pub names: IndexMap<String, Symbol>,
    /// Every user-defined keyword usable in this file.
    pub keywords: IndexMap<String, AnchorId>,
    /// The names this file publishes.
    pub exports: IndexMap<String, Symbol>,
    /// Where each imported name was written, for go-to-definition.
    pub import_ranges: HashMap<String, TextRange>,
}

/// The resolved view of a whole workspace.
pub struct Analysis {
    pub db: Db,
    anchors: Vec<AnchorLoc>,
    anchor_ids: HashMap<(FileId, usize), AnchorId>,
    scopes: HashMap<FileId, FileScope>,
    modules: HashMap<(FileId, String), FileId>,
    pub diagnostics: Diagnostics,
}

/// Load every file reachable from `entries` through `from`/`use`.
pub fn load_graph(db: &mut Db, entries: &[FileId]) -> (HashMap<(FileId, String), FileId>, Vec<Diagnostic>) {
    let mut modules = HashMap::new();
    let mut diagnostics = Vec::new();
    let mut queue: Vec<FileId> = entries.to_vec();
    let mut seen: HashSet<FileId> = entries.iter().copied().collect();

    while let Some(file) = queue.pop() {
        let specs: Vec<Spanned<String>> = {
            let hir = &db.file(file).hir;
            hir.imports
                .iter()
                .map(|it| it.path.clone())
                .chain(hir.reexports.iter().map(|it| it.path.clone()))
                .chain(hir.uses.iter().cloned())
                .collect()
        };
        for spec in specs {
            if modules.contains_key(&(file, spec.value.clone())) {
                continue;
            }
            match db.resolve(file, &spec.value) {
                Ok(target) => {
                    modules.insert((file, spec.value.clone()), target);
                    if seen.insert(target) {
                        queue.push(target);
                    }
                }
                Err(error) => diagnostics.push(Diagnostic::error(
                    "unresolved-module",
                    file,
                    spec.range,
                    error.message,
                )),
            }
        }
    }
    (modules, diagnostics)
}

impl Analysis {
    /// Resolve every loaded file.
    pub fn new(db: Db, modules: HashMap<(FileId, String), FileId>) -> Analysis {
        let mut anchors = Vec::new();
        let mut anchor_ids = HashMap::new();
        for file in db.files() {
            for index in 0..file.hir.anchors.len() {
                anchor_ids.insert((file.id, index), AnchorId(anchors.len() as u32));
                anchors.push(AnchorLoc { file: file.id, index });
            }
        }
        let mut analysis =
            Analysis { db, anchors, anchor_ids, scopes: HashMap::new(), modules, diagnostics: Diagnostics::default() };
        let files: Vec<FileId> = analysis.db.files().map(|file| file.id).collect();
        for file in &files {
            analysis.compute_exports(*file, &mut HashSet::new());
        }
        for file in files {
            analysis.compute_keywords(file);
        }
        analysis
    }

    pub fn anchor_loc(&self, id: AnchorId) -> AnchorLoc {
        self.anchors[id.0 as usize]
    }

    pub fn anchor_def(&self, id: AnchorId) -> &crate::hir::AnchorDef {
        let loc = self.anchor_loc(id);
        &self.db.file(loc.file).hir.anchors[loc.index]
    }

    pub fn anchor_id(&self, file: FileId, index: usize) -> Option<AnchorId> {
        self.anchor_ids.get(&(file, index)).copied()
    }

    pub fn anchor_ids(&self) -> impl Iterator<Item = AnchorId> + '_ {
        (0..self.anchors.len() as u32).map(AnchorId)
    }

    pub fn scope(&self, file: FileId) -> &FileScope {
        static EMPTY: std::sync::OnceLock<FileScope> = std::sync::OnceLock::new();
        self.scopes.get(&file).unwrap_or_else(|| EMPTY.get_or_init(FileScope::default))
    }

    pub fn module(&self, file: FileId, spec: &str) -> Option<FileId> {
        self.modules.get(&(file, spec.to_string())).copied()
    }

    /// The bases of an anchor, resolved in inheritance order.
    pub fn bases(&self, id: AnchorId) -> Vec<AnchorId> {
        let loc = self.anchor_loc(id);
        let def = self.anchor_def(id);
        let scope = self.scope(loc.file);
        let mut bases = Vec::new();
        // A declaring keyword contributes the left-most base.
        if let Some(keyword) = &def.via_keyword {
            bases.extend(scope.keywords.get(&keyword.value).copied());
        }
        for name in &def.bases {
            if let Some(Symbol::Anchor(base)) = scope.names.get(&name.value) {
                bases.push(*base);
            }
        }
        bases
    }

    /// Every anchor in the inheritance chain, nearest first, without duplicates.
    pub fn ancestors(&self, id: AnchorId) -> Vec<AnchorId> {
        let mut out = Vec::new();
        let mut stack = self.bases(id);
        while let Some(base) = stack.pop() {
            if out.contains(&base) {
                continue;
            }
            out.push(base);
            stack.extend(self.bases(base));
        }
        out
    }

    /// Concrete anchors whose chain reaches `id`.
    pub fn implementors(&self, id: AnchorId) -> Vec<AnchorId> {
        self.anchor_ids()
            .filter(|other| {
                *other != id
                    && !self.anchor_def(*other).is_abstract
                    && self.ancestors(*other).contains(&id)
            })
            .collect()
    }

    /// Anchors that directly name `id` as a base.
    pub fn subtypes(&self, id: AnchorId) -> Vec<AnchorId> {
        self.anchor_ids().filter(|other| self.bases(*other).contains(&id)).collect()
    }

    // ---- scope construction ---------------------------------------------

    /// Resolve one file's visible names and then what it publishes.
    ///
    /// Imports are resolved first because `export Name` may republish an
    /// imported name, so exports depend on the file's own bindings.
    fn compute_exports(&mut self, file: FileId, visiting: &mut HashSet<FileId>) {
        if self.scopes.contains_key(&file) || !visiting.insert(file) {
            return;
        }
        let names = self.compute_names(file, visiting);
        let mut exports: IndexMap<String, Symbol> = IndexMap::new();

        // Anything declared with `export` in this file.
        for (name, symbol) in &names {
            let exported = match symbol {
                Symbol::Anchor(id) => {
                    let location = self.anchor_loc(*id);
                    location.file == file && self.anchor_def(*id).exported
                }
                Symbol::Var { file: owner, index } => {
                    *owner == file && self.db.file(*owner).hir.vars[*index].exported
                }
            };
            if exported {
                exports.insert(name.clone(), *symbol);
            }
        }

        // `export Name` republishes a name the file already has in scope.
        for name in self.db.file(file).hir.exports.clone() {
            match names.get(&name.value) {
                Some(symbol) => {
                    exports.insert(name.value.clone(), *symbol);
                }
                None => self.diagnostics.push(Diagnostic::error(
                    "unknown-export",
                    file,
                    name.range,
                    format!("`{}` is not declared or imported in this file", name.value),
                )),
            }
        }

        // `from PATH export ...` imports and republishes in one statement.
        for reexport in self.db.file(file).hir.reexports.clone() {
            let Some(target) = self.module(file, &reexport.path.value) else { continue };
            self.compute_exports(target, visiting);
            let from = self.exports_of(target);
            if reexport.glob {
                exports.extend(from);
                continue;
            }
            for item in &reexport.items {
                match from.get(&item.name.value) {
                    Some(symbol) => {
                        exports.insert(item.local().to_string(), *symbol);
                    }
                    None => self.diagnostics.push(Diagnostic::error(
                        "unknown-import",
                        file,
                        item.name.range,
                        format!(
                            "`{}` is not exported by `{}`",
                            item.name.value, reexport.path.value
                        ),
                    )),
                }
            }
        }

        visiting.remove(&file);
        let scope = self.scopes.entry(file).or_default();
        scope.names = names;
        scope.exports = exports;
    }

    /// The file's own declarations plus everything it imports.
    fn compute_names(
        &mut self,
        file: FileId,
        visiting: &mut HashSet<FileId>,
    ) -> IndexMap<String, Symbol> {
        let mut names = self.locals(file);
        let mut import_ranges = HashMap::new();
        for import in self.db.file(file).hir.imports.clone() {
            let Some(target) = self.module(file, &import.path.value) else { continue };
            self.compute_exports(target, visiting);
            let exports = self.exports_of(target);
            for item in &import.items {
                match exports.get(&item.name.value) {
                    Some(symbol) => {
                        names.insert(item.local().to_string(), *symbol);
                        import_ranges.insert(item.local().to_string(), item.name.range);
                    }
                    None => self.diagnostics.push(Diagnostic::error(
                        "unknown-import",
                        file,
                        item.name.range,
                        format!("`{}` is not exported by `{}`", item.name.value, import.path.value),
                    )),
                }
            }
        }
        self.scopes.entry(file).or_default().import_ranges = import_ranges;
        names
    }

    fn exports_of(&self, file: FileId) -> IndexMap<String, Symbol> {
        self.scopes.get(&file).map(|scope| scope.exports.clone()).unwrap_or_default()
    }

    /// The keywords a file may declare anchors with.
    fn compute_keywords(&mut self, file: FileId) {
        let mut keywords: IndexMap<String, AnchorId> = IndexMap::new();
        for index in 0..self.db.file(file).hir.anchors.len() {
            let def = &self.db.file(file).hir.anchors[index];
            if let (Some(keyword), Some(id)) = (def.keyword.clone(), self.anchor_id(file, index)) {
                keywords.insert(keyword.value, id);
            }
        }
        // `use` brings in only keywords, never values.
        for spec in self.db.file(file).hir.uses.clone() {
            let Some(target) = self.module(file, &spec.value) else { continue };
            for symbol in self.exports_of(target).values() {
                if let Symbol::Anchor(id) = symbol {
                    if let Some(keyword) = self.anchor_def(*id).keyword.clone() {
                        keywords.insert(keyword.value, *id);
                    }
                }
            }
        }
        self.scopes.entry(file).or_default().keywords = keywords;
    }

    /// Names a file declares itself.
    fn locals(&self, file: FileId) -> IndexMap<String, Symbol> {
        let mut locals = IndexMap::new();
        let hir = &self.db.file(file).hir;
        for (index, def) in hir.anchors.iter().enumerate() {
            if let Some(id) = self.anchor_id(file, index) {
                locals.insert(def.name.clone(), Symbol::Anchor(id));
            }
        }
        for (index, def) in hir.vars.iter().enumerate() {
            locals.insert(def.name.clone(), Symbol::Var { file, index });
        }
        locals
    }
}
