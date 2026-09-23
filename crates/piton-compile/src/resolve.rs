//! Name resolution: modules, imports, exports, keywords, and inheritance.

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use piton_core::{AnchorId, Diagnostic, Label, Span};
use piton_syntax::ast::{self, FromKind, Item};

use crate::module::{self, ModuleGraph, ModuleId, ResolutionContext};
use crate::store::{AnchorDef, Slot, Store, Symbol, VariableDef};

/// How a name leaves a module.
#[derive(Debug, Clone)]
enum ExportTarget {
    /// A declaration in this module.
    Local(Symbol),
    /// A name taken from another module, possibly renamed.
    Forwarded { module: ModuleId, name: String },
}

/// Everything known about one module's names.
#[derive(Default)]
pub struct ModuleScope {
    /// Declarations written in this module.
    pub declarations: IndexMap<String, Symbol>,
    /// Local bindings introduced by `from ... import`.
    imports: HashMap<String, (ModuleId, String, Span)>,
    /// Named exports, in declaration order.
    exports: IndexMap<String, ExportTarget>,
    /// Modules whose exports are re-exported wholesale.
    export_globs: Vec<ModuleId>,
    /// Modules named by `use`, which contribute keywords.
    pub used: Vec<ModuleId>,
    /// Keywords usable in this module: keyword text to the anchor it aliases.
    pub keywords: IndexMap<String, AnchorId>,
    /// Modules this one imports from, for reachability reporting.
    pub dependencies: Vec<ModuleId>,
}

/// The resolved program.
pub struct Resolution {
    pub graph: ModuleGraph,
    pub store: Store,
    pub scopes: HashMap<ModuleId, ModuleScope>,
    pub entry: ModuleId,
    pub diagnostics: Vec<Diagnostic>,
}

impl Resolution {
    pub fn scope(&self, module: ModuleId) -> &ModuleScope {
        static EMPTY: std::sync::OnceLock<ModuleScope> = std::sync::OnceLock::new();
        self.scopes
            .get(&module)
            .unwrap_or_else(|| EMPTY.get_or_init(ModuleScope::default))
    }

    /// Looks a name up as it would be seen from inside `module`.
    pub fn lookup(&self, module: ModuleId, name: &str) -> Option<Symbol> {
        let scope = self.scope(module);
        if let Some(symbol) = scope.declarations.get(name) {
            return Some(*symbol);
        }
        if let Some((source, original, _)) = scope.imports.get(name) {
            return self.lookup_export(*source, original, &mut HashSet::new());
        }
        None
    }

    /// Resolves an exported name, following re-exports.
    pub fn lookup_export(
        &self,
        module: ModuleId,
        name: &str,
        visiting: &mut HashSet<(ModuleId, String)>,
    ) -> Option<Symbol> {
        let key = (module, name.to_string());
        if !visiting.insert(key) {
            return None;
        }
        let scope = self.scope(module);
        match scope.exports.get(name) {
            Some(ExportTarget::Local(symbol)) => Some(*symbol),
            Some(ExportTarget::Forwarded {
                module: source,
                name: original,
            }) => self.lookup_export(*source, original, visiting),
            None => {
                for glob in &scope.export_globs {
                    if let Some(symbol) = self.lookup_export(*glob, name, visiting) {
                        return Some(symbol);
                    }
                }
                None
            }
        }
    }

    /// Every name a module can write unqualified: its own declarations, then
    /// the names it imported.
    ///
    /// `lookup` answers this one name at a time, which is what a reference
    /// needs. Completion needs the other direction -- everything in scope --
    /// and the imports it has to include are private to the scope, so the list
    /// is built here rather than guessed from the syntax.
    pub fn visible_names(&self, module: ModuleId) -> Vec<(String, Symbol)> {
        let scope = self.scope(module);
        let mut out: Vec<(String, Symbol)> = scope
            .declarations
            .iter()
            .map(|(name, symbol)| (name.clone(), *symbol))
            .collect();
        for (local, (source, original, _)) in &scope.imports {
            // A name that fails to resolve is a diagnostic elsewhere; here it
            // is simply not something to offer.
            if let Some(symbol) = self.lookup_export(*source, original, &mut HashSet::new()) {
                out.push((local.clone(), symbol));
            }
        }
        out.sort_by(|left, right| left.0.cmp(&right.0));
        out.dedup_by(|left, right| left.0 == right.0);
        out
    }

    /// Every name a module exports, including names reached through globs.
    pub fn exported_names(&self, module: ModuleId) -> Vec<String> {
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        self.collect_exports(module, &mut names, &mut seen, &mut HashSet::new());
        names
    }

    fn collect_exports(
        &self,
        module: ModuleId,
        out: &mut Vec<String>,
        seen: &mut HashSet<String>,
        visiting: &mut HashSet<ModuleId>,
    ) {
        if !visiting.insert(module) {
            return;
        }
        let scope = self.scope(module);
        for name in scope.exports.keys() {
            if seen.insert(name.clone()) {
                out.push(name.clone());
            }
        }
        for glob in &scope.export_globs {
            self.collect_exports(*glob, out, seen, visiting);
        }
    }
}

/// Loads every module reachable from `entry` and resolves names across them.
pub fn resolve(entry: &std::path::Path, roots: module::Roots<'_>) -> Resolution {
    resolve_with(entry, roots, ModuleGraph::default())
}

/// Resolves starting from `entry` using a graph that may already carry editor
/// buffers.
pub fn resolve_with(
    entry: &std::path::Path,
    roots: module::Roots<'_>,
    graph: ModuleGraph,
) -> Resolution {
    let mut graph = graph;
    let mut diagnostics = Vec::new();
    let Some(entry_id) = graph.load(entry, &mut diagnostics) else {
        return Resolution {
            graph,
            store: Store::default(),
            scopes: HashMap::new(),
            entry: ModuleId(0),
            diagnostics,
        };
    };
    resolve_from(graph, entry_id, roots, diagnostics)
}

/// Resolves starting from `entry`, loading `also` as well.
///
/// A file nothing imports never enters the graph, which is right for a compiler
/// -- it is not part of the program -- and wrong for an editor, where the file
/// is open on screen and still has to have diagnostics, symbols, and a
/// definition to jump to. The entry stays the entry: it is what reachability is
/// measured from, and `piton reach` is where the question of what the project
/// reaches gets answered.
pub fn resolve_including(
    entry: &std::path::Path,
    also: &[std::path::PathBuf],
    roots: module::Roots<'_>,
    graph: ModuleGraph,
) -> Resolution {
    let mut graph = graph;
    let mut diagnostics = Vec::new();
    let Some(entry_id) = graph.load(entry, &mut diagnostics) else {
        return Resolution {
            graph,
            store: Store::default(),
            scopes: HashMap::new(),
            entry: ModuleId(0),
            diagnostics,
        };
    };

    let mut seeds = Vec::new();
    for path in also {
        // A path that cannot be read is reported by `graph.load`, and there is
        // nothing to seed the walk with.
        if let Some(id) = graph.load(path, &mut diagnostics) {
            seeds.push(id);
        }
    }

    resolve_from_roots(graph, entry_id, &seeds, roots, diagnostics)
}

/// Resolves a graph that already has its entry module loaded. The language
/// server uses this to reuse buffers it has already parsed.
pub fn resolve_from(
    graph: ModuleGraph,
    entry: ModuleId,
    roots: module::Roots<'_>,
    diagnostics: Vec<Diagnostic>,
) -> Resolution {
    resolve_from_roots(graph, entry, &[], roots, diagnostics)
}

/// Resolves from `entry` and from every module in `also`.
pub fn resolve_from_roots(
    mut graph: ModuleGraph,
    entry: ModuleId,
    also: &[ModuleId],
    roots: module::Roots<'_>,
    mut diagnostics: Vec<Diagnostic>,
) -> Resolution {
    // Phase 1: transitively load every referenced module. Visiting a module a
    // second time is a no-op, so circular imports simply terminate.
    let mut queue = vec![entry];
    queue.extend_from_slice(also);
    let mut loaded = HashSet::new();
    let mut edges: HashMap<ModuleId, Vec<(String, Option<ModuleId>, Span)>> = HashMap::new();

    while let Some(id) = queue.pop() {
        if !loaded.insert(id) {
            continue;
        }
        let directory = graph.get(id).directory();
        let module_path = graph.get(id).path.clone();
        let paths: Vec<(String, Span)> = graph
            .get(id)
            .ast()
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Use(decl) => Some((decl.path.text.clone(), decl.path.span)),
                Item::From(decl) => Some((decl.path.text.clone(), decl.path.span)),
                _ => None,
            })
            .collect();

        let mut resolved = Vec::new();
        for (text, span) in paths {
            let context = ResolutionContext::new(&directory, roots);
            match module::resolve(&text, &context) {
                Ok(path) => match graph.load(&path, &mut diagnostics) {
                    Some(target) => {
                        resolved.push((text, Some(target), span));
                        queue.push(target);
                    }
                    None => resolved.push((text, None, span)),
                },
                Err(error) => {
                    let mut diagnostic = Diagnostic::error(
                        "unresolved-module",
                        error.message(),
                        &module_path,
                        span,
                    );
                    if let Some(help) = error.help() {
                        diagnostic = diagnostic.with_help(help);
                    }
                    diagnostics.push(diagnostic);
                    resolved.push((text, None, span));
                }
            }
        }
        edges.insert(id, resolved);
    }

    // Phase 2: register declarations.
    let mut store = Store::default();
    let mut scopes: HashMap<ModuleId, ModuleScope> = HashMap::new();
    let mut module_ids: Vec<ModuleId> = loaded.iter().copied().collect();
    module_ids.sort();

    for id in &module_ids {
        let mut scope = ModuleScope::default();
        let module_path = graph.get(*id).path.clone();
        for (index, item) in graph.get(*id).ast().items.iter().enumerate() {
            match item {
                Item::Anchor(decl) => {
                    let anchor = store.push_anchor(AnchorDef {
                        id: AnchorId(0),
                        module: *id,
                        item: index,
                        name: decl.name.clone(),
                        name_span: decl.name_span,
                        span: decl.span,
                        exported: decl.exported,
                        is_abstract: decl.is_abstract,
                        keyword: decl.keyword.clone(),
                        alias: decl.alias.as_ref().map(|a| a.value.clone()),
                        bases: Vec::new(),
                        slots: IndexMap::new(),
                        properties: Default::default(),
                    });
                    declare(
                        &mut scope,
                        &decl.name,
                        Symbol::Anchor(anchor),
                        decl.exported,
                        decl.name_span,
                        &module_path,
                        &mut diagnostics,
                    );
                }
                Item::Variable(decl) => {
                    let variable = store.push_variable(VariableDef {
                        id: crate::store::VariableId(0),
                        module: *id,
                        item: index,
                        name: decl.name.clone(),
                        name_span: decl.name_span,
                        span: decl.span,
                        exported: decl.exported,
                        constraints: decl.constraints.clone(),
                        value: None,
                    });
                    declare(
                        &mut scope,
                        &decl.name,
                        Symbol::Variable(variable),
                        decl.exported,
                        decl.name_span,
                        &module_path,
                        &mut diagnostics,
                    );
                }
                _ => {}
            }
        }
        scopes.insert(*id, scope);
    }

    // Phase 3: wire imports, exports, and `use` edges.
    for id in &module_ids {
        let module_edges = edges.get(id).cloned().unwrap_or_default();
        let module_path = graph.get(*id).path.clone();
        let items: Vec<Item> = graph.get(*id).ast().items.clone();
        let mut edge_cursor = 0usize;
        let mut scope = scopes.remove(id).unwrap_or_default();

        for item in &items {
            match item {
                Item::Use(decl) => {
                    let target = module_edges.get(edge_cursor).and_then(|e| e.1);
                    edge_cursor += 1;
                    if let Some(target) = target {
                        scope.used.push(target);
                        scope.dependencies.push(target);
                    }
                    let _ = decl;
                }
                Item::From(decl) => {
                    let target = module_edges.get(edge_cursor).and_then(|e| e.1);
                    edge_cursor += 1;
                    let Some(target) = target else { continue };
                    scope.dependencies.push(target);
                    match decl.kind {
                        FromKind::Import => {
                            for entry in &decl.items {
                                scope.imports.insert(
                                    entry.local_name().to_string(),
                                    (target, entry.name.clone(), entry.name_span),
                                );
                            }
                        }
                        FromKind::Export => {
                            if decl.star {
                                scope.export_globs.push(target);
                            } else {
                                for entry in &decl.items {
                                    scope.exports.insert(
                                        entry.local_name().to_string(),
                                        ExportTarget::Forwarded {
                                            module: target,
                                            name: entry.name.clone(),
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
                Item::ReExport(decl) => {
                    if let Some(symbol) = scope.declarations.get(&decl.name) {
                        scope
                            .exports
                            .insert(decl.name.clone(), ExportTarget::Local(*symbol));
                    } else if let Some((source, original, _)) = scope.imports.get(&decl.name) {
                        scope.exports.insert(
                            decl.name.clone(),
                            ExportTarget::Forwarded {
                                module: *source,
                                name: original.clone(),
                            },
                        );
                    } else {
                        diagnostics.push(Diagnostic::error(
                            "unresolved-export",
                            format!("`{}` is not declared or imported in this file", decl.name),
                            &module_path,
                            decl.name_span,
                        ));
                    }
                }
                _ => {}
            }
        }
        scopes.insert(*id, scope);
    }

    let mut resolution = Resolution {
        graph,
        store,
        scopes,
        entry,
        diagnostics,
    };

    check_imports_resolve(&mut resolution, &module_ids);
    populate_keywords(&mut resolution, &module_ids);
    resolve_inheritance(&mut resolution, &module_ids);
    build_slots(&mut resolution);
    resolution
}

fn declare(
    scope: &mut ModuleScope,
    name: &str,
    symbol: Symbol,
    exported: bool,
    span: Span,
    module_path: &std::path::Path,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if scope.declarations.contains_key(name) {
        diagnostics.push(Diagnostic::error(
            "duplicate-declaration",
            format!("`{name}` is declared more than once in this file"),
            module_path,
            span,
        ));
        return;
    }
    scope.declarations.insert(name.to_string(), symbol);
    if exported {
        scope
            .exports
            .insert(name.to_string(), ExportTarget::Local(symbol));
    }
}

/// Reports imports that name something the source module does not export.
fn check_imports_resolve(resolution: &mut Resolution, modules: &[ModuleId]) {
    let mut diagnostics = Vec::new();
    for id in modules {
        let module_path = resolution.graph.get(*id).path.clone();
        let entries: Vec<(String, ModuleId, String, Span)> = resolution
            .scope(*id)
            .imports
            .iter()
            .map(|(local, (module, name, span))| {
                (local.clone(), *module, name.clone(), *span)
            })
            .collect();
        for (local, source, name, span) in entries {
            if resolution
                .lookup_export(source, &name, &mut HashSet::new())
                .is_none()
            {
                let source_path = resolution.graph.get(source).path.clone();
                let available = resolution.exported_names(source);
                let mut diagnostic = Diagnostic::error(
                    "unresolved-import",
                    format!(
                        "`{name}` is not exported by `{}`",
                        source_path.display()
                    ),
                    &module_path,
                    span,
                );
                if let Some(suggestion) = closest(&name, &available) {
                    diagnostic = diagnostic
                        .with_help(format!("did you mean `{suggestion}`?"));
                } else if available.is_empty() {
                    diagnostic =
                        diagnostic.with_help("that module does not export anything".to_string());
                }
                let _ = local;
                diagnostics.push(diagnostic);
            }
        }
    }
    resolution.diagnostics.extend(diagnostics);
}

/// Fills each module's keyword table from the modules it `use`s.
///
/// `use` brings in keywords and nothing else, which is why a file still has to
/// import the symbols it references.
fn populate_keywords(resolution: &mut Resolution, modules: &[ModuleId]) {
    for id in modules {
        let mut keywords: IndexMap<String, AnchorId> = IndexMap::new();

        // A file can always use keywords it declares itself.
        for symbol in resolution.scope(*id).declarations.values() {
            if let Symbol::Anchor(anchor) = symbol {
                if let Some(alias) = resolution.store.anchor(*anchor).alias.clone() {
                    keywords.insert(alias, *anchor);
                }
            }
        }

        let used = resolution.scope(*id).used.clone();
        for source in used {
            for name in resolution.exported_names(source) {
                if let Some(Symbol::Anchor(anchor)) =
                    resolution.lookup_export(source, &name, &mut HashSet::new())
                {
                    if let Some(alias) = resolution.store.anchor(anchor).alias.clone() {
                        keywords.insert(alias, anchor);
                    }
                }
            }
        }

        if let Some(scope) = resolution.scopes.get_mut(id) {
            scope.keywords = keywords;
        }
    }
}

/// Turns keyword and `extends` names into anchor ids.
fn resolve_inheritance(resolution: &mut Resolution, modules: &[ModuleId]) {
    let mut diagnostics = Vec::new();
    let mut assignments: Vec<(AnchorId, Vec<AnchorId>)> = Vec::new();

    for id in modules {
        let module_path = resolution.graph.get(*id).path.clone();
        let anchor_ids: Vec<AnchorId> = resolution
            .scope(*id)
            .declarations
            .values()
            .filter_map(|symbol| match symbol {
                Symbol::Anchor(anchor) => Some(*anchor),
                _ => None,
            })
            .collect();

        for anchor in anchor_ids {
            let (keyword, item) = {
                let def = resolution.store.anchor(anchor);
                (def.keyword.clone(), def.item)
            };
            let Item::Anchor(decl) = &resolution.graph.get(*id).ast().items[item] else {
                continue;
            };
            let decl = decl.clone();
            let mut bases = Vec::new();

            for base in &decl.extends {
                match resolution.lookup(*id, &base.value) {
                    Some(Symbol::Anchor(base_id)) => bases.push(base_id),
                    Some(Symbol::Variable(_)) => diagnostics.push(Diagnostic::error(
                        "invalid-base",
                        format!("`{}` is a variable, not an anchor", base.value),
                        &module_path,
                        base.span,
                    )),
                    None => diagnostics.push(Diagnostic::error(
                        "unresolved-base",
                        format!("`{}` is not in scope", base.value),
                        &module_path,
                        base.span,
                    )),
                }
            }

            if keyword != "anchor" {
                match resolution.scope(*id).keywords.get(&keyword).copied() {
                    // A user keyword is sugar for `extends`, and its anchor is
                    // always the last in the inheritance chain, so it wins any
                    // collision with what's in `extends`.
                    Some(base) => bases.push(base),
                    None => diagnostics.push(
                        Diagnostic::error(
                            "unknown-keyword",
                            format!("`{keyword}` is not a keyword in scope"),
                            &module_path,
                            decl.keyword_span,
                        )
                        .with_help(
                            "add a `use` declaration for the module that exports it".to_string(),
                        ),
                    ),
                }
            }

            assignments.push((anchor, bases));
        }
    }

    for (anchor, bases) in assignments {
        resolution.store.anchor_mut(anchor).bases = bases;
    }

    // Inheritance cycles would make property resolution non-terminating.
    let count = resolution.store.anchors.len();
    let cycles: Vec<(AnchorId, Vec<AnchorId>)> = (0..count)
        .map(|index| AnchorId(index as u32))
        .filter_map(|anchor| find_cycle(&resolution.store, anchor).map(|cycle| (anchor, cycle)))
        .collect();
    for (anchor, cycle) in cycles {
        let def = resolution.store.anchor(anchor);
        let path = resolution.graph.get(def.module).path.clone();
        // Show the cycle that caused the error: `A -> B -> A`.
        let chain: Vec<&str> = std::iter::once(anchor)
            .chain(cycle.iter().copied())
            .map(|id| resolution.store.anchor(id).name.as_str())
            .collect();
        let mut diagnostic = Diagnostic::error(
            "inheritance-cycle",
            format!("`{}` inherits from itself ({})", def.name, chain.join(" -> ")),
            &path,
            def.name_span,
        );
        for step in cycle.iter().filter(|id| **id != anchor) {
            let step_def = resolution.store.anchor(*step);
            let step_path = resolution.graph.get(step_def.module).path.clone();
            diagnostic = diagnostic.with_label(Label::new(
                step_path,
                step_def.name_span,
                format!("`{}` is part of the cycle", step_def.name),
            ));
        }
        diagnostics.push(diagnostic);
    }
    // Clear bases only after every cycle is described, so each anchor on a
    // cycle reports the whole loop.
    for index in 0..count {
        let anchor = AnchorId(index as u32);
        if has_cycle(&resolution.store, anchor) {
            resolution.store.anchor_mut(anchor).bases.clear();
        }
    }

    // An anchor can implement more than one abstract. If two of them put type
    // constraints on the same property and the types don't overlap at all,
    // nothing could satisfy both, so that is an error. If they do overlap,
    // the right-most one wins, like any other inheritance.
    for index in 0..count {
        let anchor = AnchorId(index as u32);
        let def = resolution.store.anchor(anchor).clone();
        let abstracts: Vec<AnchorId> = def
            .bases
            .iter()
            .copied()
            .filter(|b| resolution.store.anchor(*b).is_abstract)
            .collect();
        if abstracts.len() < 2 {
            continue;
        }
        let path = resolution.graph.get(def.module).path.clone();
        let mut seen: Vec<(String, AnchorId, Vec<ast::TypeConstraint>)> = Vec::new();
        for base in &abstracts {
            for (name, constraints) in declared_constraints(resolution, *base) {
                for (other_name, other, other_constraints) in &seen {
                    if *other_name == name
                        && *other != *base
                        && !constraints_overlap(&resolution.store, &constraints, other_constraints)
                    {
                        diagnostics.push(Diagnostic::error(
                            "conflicting-abstracts",
                            format!(
                                "`{}` implements `{}` and `{}`, which constrain `{name}` to types that don't overlap",
                                def.name,
                                resolution.store.anchor(*other).name,
                                resolution.store.anchor(*base).name,
                            ),
                            &path,
                            def.name_span,
                        ));
                    }
                }
                seen.push((name, *base, constraints));
            }
        }
    }

    resolution.diagnostics.extend(diagnostics);
}

/// The type constraints an anchor's whole chain puts on each property.
fn declared_constraints(resolution: &Resolution, anchor: AnchorId) -> Vec<(String, Vec<ast::TypeConstraint>)> {
    let mut out: Vec<(String, Vec<ast::TypeConstraint>)> = Vec::new();
    for member in resolution.store.base_chain(anchor).into_iter().chain(std::iter::once(anchor)) {
        let def = resolution.store.anchor(member);
        let Item::Anchor(decl) = &resolution.graph.get(def.module).ast().items[def.item] else {
            continue;
        };
        for property in decl.body.properties() {
            if property.constraints.is_empty() {
                continue;
            }
            match out.iter_mut().find(|(name, _)| *name == property.name) {
                Some(entry) => entry.1 = property.constraints.clone(),
                None => out.push((property.name.clone(), property.constraints.clone())),
            }
        }
    }
    out
}

/// True when some value could satisfy both sets of constraints.
fn constraints_overlap(store: &Store, left: &[ast::TypeConstraint], right: &[ast::TypeConstraint]) -> bool {
    left.iter()
        .any(|a| right.iter().any(|b| constraint_overlap(store, a, b)))
}

fn constraint_overlap(store: &Store, a: &ast::TypeConstraint, b: &ast::TypeConstraint) -> bool {
    use ast::TypeName::*;
    let family = |c: &ast::TypeConstraint| -> &'static str {
        if c.list {
            return "list";
        }
        match &c.name {
            String | Number | Boolean | Null => "simple",
            Reference => "reference",
            List => "list",
            Dictionary => "dictionary",
            Anchor | Named(_) => "anchor",
            Any => "any",
            Simple => "simple*",
            Complex => "complex*",
        }
    };
    if matches!(a.name, Any) && !a.list || matches!(b.name, Any) && !b.list {
        return true;
    }
    if a.list && b.list {
        let element = |c: &ast::TypeConstraint| ast::TypeConstraint { list: false, ..c.clone() };
        return constraint_overlap(store, &element(a), &element(b));
    }
    let (fa, fb) = (family(a), family(b));
    let is_complex = |f: &str| matches!(f, "list" | "dictionary" | "anchor");
    match (fa, fb) {
        ("simple*", f) | (f, "simple*") => f == "simple" || f == "simple*",
        ("complex*", f) | (f, "complex*") => is_complex(f) || f == "complex*",
        ("simple", "simple") => a.name == b.name,
        ("anchor", "anchor") => match (&a.name, &b.name) {
            (Named(x), Named(y)) => {
                x == y || {
                    let find = |n: &str| store.anchors.iter().find(|d| d.name == n).map(|d| d.id);
                    match (find(x), find(y)) {
                        (Some(x), Some(y)) => store.inherits_from(x, y) || store.inherits_from(y, x),
                        _ => true,
                    }
                }
            }
            _ => true,
        },
        (x, y) => x == y,
    }
}

/// The chain of bases that leads from `anchor` back to itself, if there is one.
fn find_cycle(store: &Store, anchor: AnchorId) -> Option<Vec<AnchorId>> {
    fn walk(
        store: &Store,
        current: AnchorId,
        target: AnchorId,
        path: &mut Vec<AnchorId>,
        seen: &mut Vec<AnchorId>,
    ) -> bool {
        for base in &store.anchor(current).bases {
            path.push(*base);
            if *base == target {
                return true;
            }
            if !seen.contains(base) {
                seen.push(*base);
                if walk(store, *base, target, path, seen) {
                    return true;
                }
            }
            path.pop();
        }
        false
    }
    let mut path = Vec::new();
    walk(store, anchor, anchor, &mut path, &mut Vec::new()).then_some(path)
}

fn has_cycle(store: &Store, anchor: AnchorId) -> bool {
    fn walk(store: &Store, current: AnchorId, target: AnchorId, seen: &mut Vec<AnchorId>) -> bool {
        for base in &store.anchor(current).bases {
            if *base == target {
                return true;
            }
            if seen.contains(base) {
                continue;
            }
            seen.push(*base);
            if walk(store, *base, target, seen) {
                return true;
            }
        }
        false
    }
    walk(store, anchor, anchor, &mut Vec::new())
}

/// Computes each anchor's property slots.
///
/// Bases contribute their slots left to right, so the right-most declaration
/// wins a collision, and the anchor's own declarations win over all of them.
/// Order follows the first declaration of each name, which is why an inherited
/// property keeps the base's position even when the child overrides its value.
fn build_slots(resolution: &mut Resolution) {
    let count = resolution.store.anchors.len();
    let mut done: HashSet<AnchorId> = HashSet::new();
    let mut diagnostics = Vec::new();
    for index in 0..count {
        build_slots_for(
            resolution,
            AnchorId(index as u32),
            &mut done,
            &mut Vec::new(),
            &mut diagnostics,
        );
    }
    resolution.diagnostics.extend(diagnostics);
}

fn build_slots_for(
    resolution: &mut Resolution,
    anchor: AnchorId,
    done: &mut HashSet<AnchorId>,
    visiting: &mut Vec<AnchorId>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if done.contains(&anchor) || visiting.contains(&anchor) {
        return;
    }
    visiting.push(anchor);
    let bases = resolution.store.anchor(anchor).bases.clone();
    for base in &bases {
        build_slots_for(resolution, *base, done, visiting, diagnostics);
    }
    visiting.pop();

    let mut slots: IndexMap<String, Slot> = IndexMap::new();
    for base in &bases {
        for (name, slot) in resolution.store.anchor(*base).slots.clone() {
            match slots.get_mut(&name) {
                Some(existing) => {
                    // Left to right, last in line wins -- for values and for
                    // type constraints alike.
                    let inherited_constraints = slot.constraints.clone();
                    *existing = slot;
                    if existing.constraints.is_empty() {
                        existing.constraints = inherited_constraints;
                    }
                }
                None => {
                    slots.insert(name, slot);
                }
            }
        }
    }

    let def = resolution.store.anchor(anchor).clone();
    let module = resolution.graph.get(def.module);
    let Item::Anchor(decl) = &module.ast().items[def.item] else {
        return;
    };
    let module_path = module.path.clone();

    for (index, property) in decl.body.properties().enumerate() {
        let has_value = property.value.declared;
        // A child can't redeclare a constraint it inherits; it can only give
        // the property a value.
        if !property.constraints.is_empty() {
            if let Some(inherited) = slots.get(&property.name) {
                if !inherited.constraints.is_empty() {
                    let owner = resolution.store.anchor(inherited.owner);
                    let owner_path = resolution.graph.get(owner.module).path.clone();
                    diagnostics.push(
                        Diagnostic::error(
                            "redeclared-constraint",
                            format!(
                                "`{}` already has a type constraint from `{}`; write `{}: <value>` without `::`",
                                property.name, owner.name, property.name
                            ),
                            &module_path,
                            property.name_span,
                        )
                        .with_label(Label::new(
                            owner_path,
                            inherited.span,
                            "the inherited constraint".to_string(),
                        )),
                    );
                }
            }
        }
        let constraints = if property.constraints.is_empty() {
            slots
                .get(&property.name)
                .map(|slot| slot.constraints.clone())
                .unwrap_or_default()
        } else {
            property.constraints.clone()
        };
        let slot = Slot {
            owner: anchor,
            index,
            constraints,
            has_value,
            span: property.name_span,
        };
        match slots.get_mut(&property.name) {
            Some(existing) => {
                if has_value || !existing.has_value {
                    *existing = slot;
                }
            }
            None => {
                slots.insert(property.name.clone(), slot);
            }
        }
    }

    // Report anything in an anchor body that is not a property: it is parsed
    // but never emitted, and silently dropping it hides real mistakes.
    for item in &decl.body.items {
        if matches!(item, ast::BlockItem::Property(_) | ast::BlockItem::Pass(_)) {
            continue;
        }
        if let Some(key) = invalid_key(item) {
            diagnostics.push(
                Diagnostic::error(
                    "invalid-key",
                    format!("`{key}` is not a valid key"),
                    &module_path,
                    item.span(),
                )
                .with_help("keys can have letters, numbers, underscores, and hyphens; nothing else, and no spaces"),
            );
            continue;
        }
        let (message, help) = describe_non_property(item, &def.name);
        diagnostics.push(
            Diagnostic::warning("non-property-in-anchor", message, &module_path, item.span())
                .with_help(help),
        );
    }

    // A concrete anchor must supply a value for every abstract slot it inherits.
    if def.is_concrete() {
        for (name, slot) in &slots {
            if slot.has_value {
                continue;
            }
            let owner = resolution.store.anchor(slot.owner);
            let owner_path = resolution.graph.get(owner.module).path.clone();
            if slot.owner == anchor {
                // Declared right here with a constraint and no value. Only an
                // abstract anchor can leave a property without one.
                diagnostics.push(Diagnostic::error(
                    "missing-value",
                    format!(
                        "`{name}` has a type constraint but no value; only a property in an abstract anchor can leave its value out"
                    ),
                    &module_path,
                    slot.span,
                ));
                continue;
            }
            diagnostics.push(
                Diagnostic::error(
                    "unimplemented-property",
                    format!(
                        "`{}` does not define `{name}`, required by `{}`",
                        def.name, owner.name
                    ),
                    &module_path,
                    def.name_span,
                )
                .with_label(Label::new(
                    owner_path,
                    slot.span,
                    format!("`{name}` is declared here"),
                )),
            );
        }
    }

    resolution.store.anchor_mut(anchor).slots = slots;
    done.insert(anchor);
}

/// The key of a line in an anchor body that is written like a property but
/// uses characters a key can't have, like `a.b: 1`.
fn invalid_key(item: &ast::BlockItem) -> Option<String> {
    let ast::BlockItem::Prose(paragraph) = item else {
        return None;
    };
    let [line] = paragraph.lines.as_slice() else {
        return None;
    };
    let text: String = line
        .segments
        .iter()
        .map(|segment| match segment {
            ast::ProseSegment::Text(text) => text.clone(),
            _ => " ".to_string(),
        })
        .collect();
    let (key, _) = text.split_once(": ").or_else(|| text.strip_suffix(':').map(|k| (k, "")))?;
    let valid = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
    (!key.is_empty() && !key.contains(char::is_whitespace) && !key.chars().all(valid))
        .then(|| key.to_string())
}

/// Explains why one line of an anchor body is not a property.
///
/// The causes are different enough that one shared message helps nobody: a
/// reserved word needs renaming, while a stray code fence needs indenting under
/// a property.
fn describe_non_property(item: &ast::BlockItem, anchor: &str) -> (String, String) {
    match item {
        ast::BlockItem::Fence(_) => (
            format!("a code fence in the body of `{anchor}` has no property to attach to, so it will not be emitted"),
            "indent it under a property, so it becomes that property's value".to_string(),
        ),
        ast::BlockItem::ListItem(_) | ast::BlockItem::Merge(_) => (
            format!("a list item in the body of `{anchor}` has no property to attach to, so it will not be emitted"),
            "an anchor body holds `key: value` properties; put the list under one".to_string(),
        ),
        _ => (
            format!("a line in the body of `{anchor}` is not a property and will not be emitted"),
            "an anchor body holds `key: value` properties; a key is any text without spaces, followed by a colon and a space"
                .to_string(),
        ),
    }
}

/// Finds the closest candidate by edit distance, for "did you mean" help.
fn closest<'a>(name: &str, candidates: &'a [String]) -> Option<&'a str> {
    let mut best: Option<(usize, &str)> = None;
    for candidate in candidates {
        let distance = edit_distance(name, candidate);
        let limit = (name.len() / 3).max(2);
        if distance <= limit && best.is_none_or(|(d, _)| distance < d) {
            best = Some((distance, candidate.as_str()));
        }
    }
    best.map(|(_, name)| name)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_distance_is_symmetric_and_zero_for_equal() {
        assert_eq!(edit_distance("abc", "abc"), 0);
        assert_eq!(edit_distance("abc", "abd"), 1);
        assert_eq!(edit_distance("Operator", "Operators"), 1);
    }
}
