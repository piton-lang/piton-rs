//! Name resolution: modules, imports, exports, keywords, and inheritance.

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use piton_core::{AnchorId, Diagnostic, Label, Span};
use piton_syntax::ast::{self, FromKind, Item};

use crate::module::{self, ModuleGraph, ModuleId, ResolutionContext};
use crate::store::{AnchorDef, Slot, Store, Symbol, VariableDef};

/// The way a module exports a name.
#[derive(Debug, Clone)]
enum ExportTarget {
    /// A declaration in this module.
    Local(Symbol),
    /// A name from a different module, possibly with a new name.
    Forwarded { module: ModuleId, name: String },
}

/// One location from which a module exports. This is a declaration, a direct
/// re-export, or an `export *`.
#[derive(Debug, Clone)]
struct ExportSite {
    /// The exported name, or `None` for `export *`. `export *` exports all the
    /// names that the module of the glob exports.
    name: Option<String>,
    target: ExportTarget,
    span: Span,
    /// The text for the source in a message. This is the module path as the source
    /// wrote it, or "this file" for a declaration.
    source: String,
}

/// All the known data about the names of one module.
#[derive(Default)]
pub struct ModuleScope {
    /// Declarations in this module.
    pub declarations: IndexMap<String, Symbol>,
    /// Local bindings that `from ... import` adds.
    imports: HashMap<String, (ModuleId, String, Span)>,
    /// Named exports, in declaration order.
    exports: IndexMap<String, ExportTarget>,
    /// Modules from which this module re-exports all exports.
    export_globs: Vec<ModuleId>,
    /// All the ways this module exports a name, in precedence order. Thus the
    /// resolver can report two exports with one name. It does not lose one.
    export_sites: Vec<ExportSite>,
    /// Modules that `use` names. These modules give keywords.
    pub used: Vec<ModuleId>,
    /// Keywords that this module can use. Each maps keyword text to the anchor that it aliases.
    pub keywords: IndexMap<String, AnchorId>,
    /// Modules from which this module imports, for reachability reports.
    pub dependencies: Vec<ModuleId>,
}

/// The resolved program.
pub struct Resolution {
    pub graph: ModuleGraph,
    pub store: Store,
    pub scopes: HashMap<ModuleId, ModuleScope>,
    pub entry: ModuleId,
    /// False when the resolver did not load the entry. Then `entry` names no module
    /// of its own. Only the workspace of the editor still resolves the files
    /// around it. Thus there is no start point for reachability.
    pub has_entry: bool,
    pub diagnostics: Vec<Diagnostic>,
}

impl Resolution {
    pub fn scope(&self, module: ModuleId) -> &ModuleScope {
        static EMPTY: std::sync::OnceLock<ModuleScope> = std::sync::OnceLock::new();
        self.scopes
            .get(&module)
            .unwrap_or_else(|| EMPTY.get_or_init(ModuleScope::default))
    }

    /// Finds a name as `module` sees it from inside.
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

    /// Resolves an exported name. It follows re-exports.
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

    /// All the names that a module can write unqualified. First its own declarations,
    /// then the names it imported.
    ///
    /// `lookup` gives one name at a time. A reference needs this. Completion needs the
    /// other direction, which is all the names in scope. The imports that completion
    /// must include are private to the scope. Thus this function builds the list. It
    /// does not guess the list from the syntax.
    pub fn visible_names(&self, module: ModuleId) -> Vec<(String, Symbol)> {
        let scope = self.scope(module);
        let mut out: Vec<(String, Symbol)> = scope
            .declarations
            .iter()
            .map(|(name, symbol)| (name.clone(), *symbol))
            .collect();
        for (local, (source, original, _)) in &scope.imports {
            // A name that does not resolve causes a diagnostic at a different
            // location. Here, it is only not a name to offer.
            if let Some(symbol) = self.lookup_export(*source, original, &mut HashSet::new()) {
                out.push((local.clone(), symbol));
            }
        }
        out.sort_by(|left, right| left.0.cmp(&right.0));
        out.dedup_by(|left, right| left.0 == right.0);
        out
    }

    /// All the names that a module exports, also the names that come through globs.
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

/// Loads all the modules that `entry` reaches and resolves names across them.
pub fn resolve(entry: &std::path::Path, roots: module::Roots<'_>) -> Resolution {
    resolve_with(entry, roots, ModuleGraph::default())
}

/// Resolves from `entry`. The graph can contain editor
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
            has_entry: false,
            diagnostics,
        };
    };
    resolve_from(graph, entry_id, roots, diagnostics)
}

/// Resolves from `entry`, and also loads `also`.
///
/// A file that no module imports does not go into the graph. This is correct for a
/// compiler, because the file is not part of the program. It is not correct for an
/// editor. The file is open on the screen and must have diagnostics, symbols, and a
/// definition to go to. The entry stays the entry. Reachability starts from
/// the entry. `piton reach` gives the answer to the question of what the project
/// reaches.
pub fn resolve_including(
    entry: &std::path::Path,
    also: &[std::path::PathBuf],
    roots: module::Roots<'_>,
    graph: ModuleGraph,
) -> Resolution {
    let mut graph = graph;
    let mut diagnostics = Vec::new();
    let entry_id = graph.load(entry, &mut diagnostics);

    let mut seeds = Vec::new();
    for path in also {
        // `graph.load` reports a path that it cannot read. There is
        // no file to start the walk with.
        if let Some(id) = graph.load(path, &mut diagnostics) {
            seeds.push(id);
        }
    }

    match entry_id {
        Some(entry_id) => resolve_from_roots(graph, entry_id, &seeds, roots, diagnostics),
        // The resolver reports an entry that is not there. But the open files must
        // still resolve. Without them, there are no completions, symbols, or
        // diagnostics for the files that will fix the project.
        None => match seeds.split_first() {
            Some((first, rest)) => {
                let mut resolution = resolve_from_roots(graph, *first, rest, roots, diagnostics);
                resolution.has_entry = false;
                resolution
            }
            None => Resolution {
                graph,
                store: Store::default(),
                scopes: HashMap::new(),
                entry: ModuleId(0),
                has_entry: false,
                diagnostics,
            },
        },
    }
}

/// Resolves a graph that contains its loaded entry module. The language
/// server uses this function to use again the buffers that it parsed before.
pub fn resolve_from(
    graph: ModuleGraph,
    entry: ModuleId,
    roots: module::Roots<'_>,
    diagnostics: Vec<Diagnostic>,
) -> Resolution {
    resolve_from_roots(graph, entry, &[], roots, diagnostics)
}

/// Resolves from `entry` and from each module in `also`.
pub fn resolve_from_roots(
    mut graph: ModuleGraph,
    entry: ModuleId,
    also: &[ModuleId],
    roots: module::Roots<'_>,
    mut diagnostics: Vec<Diagnostic>,
) -> Resolution {
    // Phase 1: transitively load all referenced modules. A second visit to a module
    // does nothing. Thus circular imports stop.
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

    // Phase 3: connect imports, exports, and `use` edges.
    for id in &module_ids {
        let module_edges = edges.get(id).cloned().unwrap_or_default();
        let module_path = graph.get(*id).path.clone();
        let items: Vec<Item> = graph.get(*id).ast().items.clone();
        let mut edge_cursor = 0usize;
        let mut scope = scopes.remove(id).unwrap_or_default();
        // `export *` comes after all named exports in precedence, on whichever
        // line it is.
        let mut globs: Vec<ExportSite> = Vec::new();

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
                                globs.push(ExportSite {
                                    name: None,
                                    target: ExportTarget::Forwarded {
                                        module: target,
                                        name: String::new(),
                                    },
                                    span: decl.path.span,
                                    source: format!("`{}`", decl.path.text),
                                });
                            } else {
                                for entry in &decl.items {
                                    let forwarded = ExportTarget::Forwarded {
                                        module: target,
                                        name: entry.name.clone(),
                                    };
                                    export(
                                        &mut scope,
                                        entry.local_name(),
                                        forwarded,
                                        entry.span,
                                        format!("`{}`", decl.path.text),
                                    );
                                }
                            }
                        }
                    }
                }
                Item::ReExport(decl) => {
                    if let Some(symbol) = scope.declarations.get(&decl.name) {
                        let local = ExportTarget::Local(*symbol);
                        export(
                            &mut scope,
                            &decl.name,
                            local,
                            decl.name_span,
                            "this file".to_string(),
                        );
                    } else if let Some((source, original, _)) = scope.imports.get(&decl.name) {
                        let forwarded = ExportTarget::Forwarded {
                            module: *source,
                            name: original.clone(),
                        };
                        let source = import_source_label(&items, *source, &module_edges);
                        export(&mut scope, &decl.name, forwarded, decl.name_span, source);
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
        scope.export_sites.extend(globs);
        scopes.insert(*id, scope);
    }

    let mut resolution = Resolution {
        graph,
        store,
        scopes,
        entry,
        has_entry: true,
        diagnostics,
    };

    check_imports_resolve(&mut resolution, &module_ids);
    check_duplicate_exports(&mut resolution, &module_ids);
    populate_keywords(&mut resolution, &module_ids);
    resolve_inheritance(&mut resolution, &module_ids);
    build_slots(&mut resolution);
    report_unimplemented_abstracts(&mut resolution);
    resolution
}

/// Warns about an abstract anchor that no anchor implements. The resolver does not
/// warn about an exported abstract anchor, because a different file or project can implement it.
/// It also does not warn about an anchor from a bundled package.
fn report_unimplemented_abstracts(resolution: &mut Resolution) {
    let store = &resolution.store;
    let mut diagnostics = Vec::new();
    for def in &store.anchors {
        if !def.is_abstract || def.exported {
            continue;
        }
        let module = resolution.graph.get(def.module);
        if module.is_package() {
            continue;
        }
        let implemented = store
            .anchors
            .iter()
            .any(|other| !other.is_abstract && other.id != def.id && store.inherits_from(other.id, def.id));
        if !implemented {
            diagnostics.push(
                Diagnostic::warning(
                    "unimplemented-abstract",
                    format!("nothing implements `{}`", def.name),
                    &module.path,
                    def.name_span,
                )
                .with_help("extend it with a concrete anchor, or export it if another file or project implements it"),
            );
        }
    }
    resolution.diagnostics.extend(diagnostics);
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
        scope.export_sites.push(ExportSite {
            name: Some(name.to_string()),
            target: ExportTarget::Local(symbol),
            span,
            source: "this file".to_string(),
        });
    }
}

/// Records a named export. The module keeps the first export of a name.
/// The resolver also records a later export as a site, so that it can report it.
fn export(scope: &mut ModuleScope, name: &str, target: ExportTarget, span: Span, source: String) {
    scope
        .exports
        .entry(name.to_string())
        .or_insert_with(|| target.clone());
    scope.export_sites.push(ExportSite {
        name: Some(name.to_string()),
        target,
        span,
        source,
    });
}

/// The text for `export Name` when `Name` came from an import. This is the module
/// path on the import that brought it in.
fn import_source_label(
    items: &[Item],
    source: ModuleId,
    edges: &[(String, Option<ModuleId>, Span)],
) -> String {
    let mut cursor = 0usize;
    for item in items {
        if let Item::Use(_) | Item::From(_) = item {
            if let (Item::From(decl), Some((_, Some(target), _))) = (item, edges.get(cursor)) {
                if *target == source {
                    return format!("`{}`", decl.path.text);
                }
            }
            cursor += 1;
        }
    }
    "an import".to_string()
}

/// Reports a name that a module exports two times with two different meanings.
///
/// Lookup keeps the first site in precedence order. This order is a declaration, then
/// a direct export, then the first `export *`. Thus the report is about each later
/// site with a different sense. Lookup dropped that site.
fn check_duplicate_exports(resolution: &mut Resolution, modules: &[ModuleId]) {
    let mut diagnostics = Vec::new();
    for id in modules {
        let module = resolution.graph.get(*id);
        if module.is_package() {
            continue;
        }
        let module_path = module.path.clone();

        // All the (name, sense, site) values that this module exports, in precedence order.
        let mut candidates: Vec<(String, Symbol, &ExportSite)> = Vec::new();
        for site in &resolution.scope(*id).export_sites {
            match (&site.name, &site.target) {
                (Some(name), ExportTarget::Local(symbol)) => {
                    candidates.push((name.clone(), *symbol, site));
                }
                (Some(name), ExportTarget::Forwarded { module, name: original }) => {
                    // A different location reports an export that does not resolve.
                    if let Some(symbol) =
                        resolution.lookup_export(*module, original, &mut HashSet::new())
                    {
                        candidates.push((name.clone(), symbol, site));
                    }
                }
                (None, ExportTarget::Forwarded { module, .. }) => {
                    for name in resolution.exported_names(*module) {
                        if let Some(symbol) =
                            resolution.lookup_export(*module, &name, &mut HashSet::new())
                        {
                            candidates.push((name, symbol, site));
                        }
                    }
                }
                (None, ExportTarget::Local(_)) => {}
            }
        }

        let mut kept: HashMap<&str, (Symbol, &ExportSite)> = HashMap::new();
        for (name, symbol, site) in &candidates {
            let Some((first_symbol, first_site)) = kept.get(name.as_str()) else {
                kept.insert(name, (*symbol, site));
                continue;
            };
            if first_symbol == symbol {
                continue;
            }
            let alias = format!("{}{name}", alias_prefix(&site.source));
            let help = if site.name.is_none() {
                format!(
                    "list what {} exports instead of `export *`, leaving `{name}` out or aliasing it (`{name} {alias}`)",
                    site.source
                )
            } else {
                format!("export one of them under an alias (`{name} {alias}`)")
            };
            diagnostics.push(
                Diagnostic::error(
                    "duplicate-export",
                    format!(
                        "`{name}` is exported from both {} and {}",
                        first_site.source, site.source
                    ),
                    &module_path,
                    site.span,
                )
                .with_label(Label::new(
                    module_path.clone(),
                    first_site.span,
                    format!("`{name}` from {} is the one exported", first_site.source),
                ))
                .with_help(help),
            );
        }
    }
    resolution.diagnostics.extend(diagnostics);
}

/// A prefix for a suggested alias, from the source module.
/// For example, `./strataglyph` gives `StrataglyphRoles`.
fn alias_prefix(source: &str) -> String {
    if !source.starts_with('`') {
        return "Other".to_string();
    }
    let segment = source
        .trim_matches('`')
        .rsplit(['/', '\\'])
        .find(|segment| !segment.is_empty() && *segment != "." && *segment != "..")
        .unwrap_or_default()
        .trim_end_matches(".pi");
    let mut chars = segment.chars().filter(|c| c.is_alphanumeric());
    match chars.next() {
        Some(first) if first.is_alphabetic() => first.to_uppercase().chain(chars).collect(),
        _ => "Other".to_string(),
    }
}

/// Reports imports that name an item that the source module does not export.
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

/// Fills the keyword table of each module from the modules that it `use`s.
///
/// `use` brings in keywords only. Thus a file must still
/// import the symbols that it references.
fn populate_keywords(resolution: &mut Resolution, modules: &[ModuleId]) {
    for id in modules {
        let mut keywords: IndexMap<String, AnchorId> = IndexMap::new();

        // A file can always use the keywords that it declares.
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

/// Changes keyword names and `extends` names into anchor ids.
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
                    // A user keyword is a short form of `extends`. Its anchor is
                    // always the last in the inheritance chain. Thus it wins each
                    // collision with the names in `extends`.
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

    // Inheritance cycles cause property resolution to not stop.
    let count = resolution.store.anchors.len();
    let cycles: Vec<(AnchorId, Vec<AnchorId>)> = (0..count)
        .map(|index| AnchorId(index as u32))
        .filter_map(|anchor| find_cycle(&resolution.store, anchor).map(|cycle| (anchor, cycle)))
        .collect();
    for (anchor, cycle) in cycles {
        let def = resolution.store.anchor(anchor);
        let path = resolution.graph.get(def.module).path.clone();
        // Show the cycle that caused the error, for example `A -> B -> A`.
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
    // Clear the bases only after the resolver describes all cycles. Thus each anchor
    // on a cycle reports the full loop.
    for index in 0..count {
        let anchor = AnchorId(index as u32);
        if has_cycle(&resolution.store, anchor) {
            resolution.store.anchor_mut(anchor).bases.clear();
        }
    }

    // An anchor can implement more than one abstract. Two abstracts can put type
    // constraints on the same property. If the types do not overlap at all,
    // no value can satisfy both. Thus that is an error. If the types overlap,
    // the right-most abstract wins, as in all other inheritance.
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

/// The type constraints that the full chain of an anchor puts on each property.
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

/// True when a value can satisfy both sets of constraints.
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

/// The chain of bases from `anchor` back to `anchor`, if this chain exists.
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

/// Calculates the property slots of each anchor.
///
/// Bases give their slots from left to right. Thus the right-most declaration
/// wins a collision. The own declarations of the anchor win over all of them.
/// The order follows the first declaration of each name. Thus an inherited
/// property keeps the position from the base, also when the child overrides its value.
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
                    // From left to right, the last item wins. This applies to values and to
                    // type constraints.
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
        // A child cannot declare again a constraint that it inherits. It can only give
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

    // Report each item in an anchor body that is not a property. The parser parses it,
    // but the compiler never writes it. Without a report, the user does not see the mistake.
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

    // A concrete anchor must give a value for each abstract slot that it inherits.
    if def.is_concrete() {
        for (name, slot) in &slots {
            if slot.has_value {
                continue;
            }
            let owner = resolution.store.anchor(slot.owner);
            let owner_path = resolution.graph.get(owner.module).path.clone();
            if slot.owner == anchor {
                // The anchor declares it here with a constraint and no value. Only an
                // abstract anchor can have a property without a value.
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

/// The key of a line in an anchor body. The line looks like a property, but
/// the key has characters that a key cannot have, for example `a.b: 1`.
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
/// The causes are very different. Thus one shared message does not help. A
/// stray list item needs a property above it. A line of prose needs
/// a key.
fn describe_non_property(item: &ast::BlockItem, anchor: &str) -> (String, String) {
    match item {
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
pub(crate) fn closest<'a>(name: &str, candidates: &'a [String]) -> Option<&'a str> {
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
