//! Reachability analysis.
//!
//! The compiler compiles only the source that the configured entrypoints
//! reach. Thus a user must know what is reachable and what is not to
//! understand a specbase. `piton reach` reports both.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use piton_core::{AnchorId, MixedItem, Value};

use crate::eval::Read;
use crate::module::ModuleId;
use crate::store::{Symbol, VariableId};
use crate::Compilation;

/// The kinds of edge the traversal follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    Import,
    Reference,
    Inheritance,
    Composition,
}

impl EdgeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EdgeKind::Import => "imports",
            EdgeKind::Reference => "references",
            EdgeKind::Inheritance => "inheritance",
            EdgeKind::Composition => "composition",
        }
    }
}

/// One reachable anchor and the path that found it.
#[derive(Debug, Clone)]
pub struct Reached {
    pub anchor: AnchorId,
    /// The number of edges from the nearest root.
    pub depth: usize,
    /// The shortest path of anchor names from a root.
    pub path: Vec<String>,
    /// The edge that arrived here. `None` for a root, because no edge reached
    /// it.
    pub via: Option<EdgeKind>,
}

/// A source file that no module imports.
#[derive(Debug, Clone)]
pub struct Unloaded {
    pub path: PathBuf,
    /// The number of anchors that the file declares. The file gets a parse for
    /// this, but no compilation. Thus these anchors have no id and count as
    /// unreachable.
    pub anchors: usize,
}

/// The result of a reachability query.
#[derive(Debug, Clone, Default)]
pub struct Reachability {
    pub reached: Vec<Reached>,
    pub unreachable: Vec<AnchorId>,
    /// Modules that hold an item that the roots reach. The module declares the
    /// item or exports it again.
    pub modules: HashSet<ModuleId>,
    /// Modules that the compiler loaded, but that hold no item that the roots reach.
    pub unreachable_modules: Vec<PathBuf>,
    /// Source files that no module imports. Thus the compilation never opened them.
    pub unloaded_modules: Vec<Unloaded>,
    /// The number of modules of the specbase that the compiler loaded, without packages.
    pub loaded: usize,
}

impl Reachability {
    pub fn contains(&self, anchor: AnchorId) -> bool {
        self.reached.iter().any(|entry| entry.anchor == anchor)
    }

    /// Anchors that files declare when no module imports those files.
    pub fn unloaded_anchors(&self) -> usize {
        self.unloaded_modules.iter().map(|module| module.anchors).sum()
    }

    /// All unreachable anchors. These are the loaded anchors that the walk did
    /// not reach, and the anchors in files that no module imports.
    pub fn unreachable_anchors(&self) -> usize {
        self.unreachable.len() + self.unloaded_anchors()
    }
}

/// Computes reachability from a set of root anchors. It follows references,
/// inheritance and composition.
///
/// A build compiles this set. An anchor that a module only imports, with no
/// use, is not part of the output.
pub fn from_roots(compilation: &Compilation, roots: &[AnchorId]) -> Reachability {
    traverse(compilation, roots, &[])
}

/// Computes reachability from root anchors and root modules. It follows
/// imports, and also references, inheritance and composition.
///
/// `piton reach` asks a wider question than a build. It asks which parts of
/// the specbase a start point can see. A module sees each anchor that it
/// imports, whether or not an item uses it. Thus a root module reaches each
/// anchor that it imports or exports again. It is one edge from the module,
/// via imports. The report shows such an anchor via imports only when no other
/// edge reaches it. Thus the more specific edge wins, if there is one.
pub fn from_roots_with_imports(
    compilation: &Compilation,
    roots: &[AnchorId],
    modules: &[ModuleId],
) -> Reachability {
    traverse(compilation, roots, modules)
}

/// Computes reachability from the entry, and follows imports. The exports of
/// the entry are the roots. The walk reaches all imports of the entry module
/// through the entry.
pub fn from_entry_with_imports(compilation: &Compilation) -> Reachability {
    from_entry_with_imports_and(compilation, &[])
}

/// The same as [`from_entry_with_imports`], but with `extra` roots in addition
/// to the exports of the entry. These are the anchors that plugins reach.
pub fn from_entry_with_imports_and(compilation: &Compilation, extra: &[AnchorId]) -> Reachability {
    let mut roots = compilation.entry_exports();
    add_roots(&mut roots, extra);
    let mut result = traverse(compilation, &roots, &[compilation.resolution.entry]);
    result.unloaded_modules = unloaded(compilation);
    result
}

/// All anchors that a module gets from other modules. These are its imports,
/// and the anchors that it exports again without a declaration.
fn imported_anchors(compilation: &Compilation, module: ModuleId) -> Vec<AnchorId> {
    let resolution = &compilation.resolution;
    let declared = &resolution.scope(module).declarations;
    let mut out: Vec<AnchorId> = Vec::new();
    for (name, symbol) in resolution.visible_names(module) {
        if let (false, Symbol::Anchor(anchor)) = (declared.contains_key(&name), symbol) {
            out.push(anchor);
        }
    }
    for name in resolution.exported_names(module) {
        if declared.contains_key(&name) {
            continue;
        }
        if let Some(Symbol::Anchor(anchor)) =
            resolution.lookup_export(module, &name, &mut HashSet::new())
        {
            out.push(anchor);
        }
    }
    let mut seen = HashSet::new();
    out.retain(|anchor| seen.insert(*anchor));
    out
}

type Pending = (AnchorId, usize, Vec<String>, Option<EdgeKind>);

fn traverse(compilation: &Compilation, roots: &[AnchorId], modules: &[ModuleId]) -> Reachability {
    let mut result = Reachability::default();
    let mut seen: HashMap<AnchorId, usize> = HashMap::new();
    let mut queue: std::collections::VecDeque<Pending> = Default::default();

    for root in roots {
        if seen.contains_key(root) {
            continue;
        }
        seen.insert(*root, 0);
        let name = compilation.resolution.store.anchor(*root).name.clone();
        queue.push_back((*root, 0, vec![name], None));
    }
    walk(compilation, &mut queue, &mut seen, &mut result);

    // The walk follows imports last. An anchor can also have a reference, an
    // extension or a composition. Then the report shows that edge.
    for module in modules {
        result.modules.insert(*module);
        let label = compilation
            .resolution
            .graph
            .get(*module)
            .path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        for anchor in imported_anchors(compilation, *module) {
            if seen.contains_key(&anchor) {
                continue;
            }
            seen.insert(anchor, 1);
            let name = compilation.resolution.store.anchor(anchor).name.clone();
            queue.push_back((anchor, 1, vec![label.clone(), name], Some(EdgeKind::Import)));
        }
    }
    walk(compilation, &mut queue, &mut seen, &mut result);
    finish(compilation, seen, result)
}

/// Walks breadth-first from the queued anchors. It records each anchor one time.
fn walk(
    compilation: &Compilation,
    queue: &mut std::collections::VecDeque<Pending>,
    seen: &mut HashMap<AnchorId, usize>,
    result: &mut Reachability,
) {
    while let Some((anchor, depth, path, via)) = queue.pop_front() {
        result.reached.push(Reached {
            anchor,
            depth,
            path: path.clone(),
            via,
        });
        let def = compilation.resolution.store.anchor(anchor);
        result.modules.insert(def.module);

        let mut neighbours: Vec<(AnchorId, EdgeKind)> = Vec::new();
        for base in &def.bases {
            neighbours.push((*base, EdgeKind::Inheritance));
        }
        for value in def.properties.values() {
            collect_targets(value, &mut neighbours);
        }
        read_targets(compilation, anchor, &mut neighbours, result);

        for (next, kind) in neighbours {
            if seen.contains_key(&next) {
                continue;
            }
            seen.insert(next, depth + 1);
            let mut next_path = path.clone();
            next_path.push(compilation.resolution.store.anchor(next).name.clone());
            queue.push_back((next, depth + 1, next_path, Some(kind)));
        }
    }
}

/// Follows the reads of the evaluation of the properties of an anchor.
///
/// The evaluation puts `{PURPOSE}` and `${Other.name}` into the value. Thus
/// the value alone does not show the dependency on `Other` or on the module
/// that declares `PURPOSE`. A read of a variable is also composition. Its
/// module holds a reached item. The walk then reaches all that the variable
/// embeds or reads. A read of a property of another anchor composes that anchor.
fn read_targets(
    compilation: &Compilation,
    anchor: AnchorId,
    neighbours: &mut Vec<(AnchorId, EdgeKind)>,
    result: &mut Reachability,
) {
    let mut pending: Vec<Read> = compilation
        .values
        .get(&anchor)
        .map(|properties| {
            properties
                .keys()
                .map(|name| Read::Property(anchor, name.clone()))
                .collect()
        })
        .unwrap_or_default();
    let mut visited: HashSet<Read> = pending.iter().cloned().collect();
    let mut variables: Vec<VariableId> = Vec::new();
    while let Some(read) = pending.pop() {
        let Some(reads) = compilation.reads.get(&read) else {
            continue;
        };
        for next in reads {
            if !visited.insert(next.clone()) {
                continue;
            }
            match next {
                Read::Variable(variable) => variables.push(*variable),
                Read::Property(other, _) if *other != anchor => {
                    neighbours.push((*other, EdgeKind::Composition));
                    // The walk visits the other anchor separately. It follows
                    // the reads of that anchor from there.
                    continue;
                }
                Read::Property(..) => {}
            }
            pending.push(next.clone());
        }
    }
    for variable in variables {
        let def = compilation.resolution.store.variable(variable);
        result.modules.insert(def.module);
        if let Some(value) = compilation.variables.get(&variable) {
            collect_targets(value, neighbours);
        }
    }
}

/// All items that the walk did not reach, and the counts for each module.
fn finish(
    compilation: &Compilation,
    seen: HashMap<AnchorId, usize>,
    mut result: Reachability,
) -> Reachability {

    let embedded = embedded_modules(compilation);

    for def in &compilation.resolution.store.anchors {
        // Anchors in a bundled package are the vocabulary of the compiler.
        // They are not part of the specbase under analysis.
        if compilation.resolution.graph.get(def.module).is_package() {
            continue;
        }
        if embedded.contains(&def.module) {
            continue;
        }
        if !seen.contains_key(&def.id) {
            result.unreachable.push(def.id);
        }
    }

    // A module is necessary when it declares an item that the roots reach. It
    // is also necessary when it exports that item again. Most of a specbase
    // connects through an index that only exports again. Such an index declares
    // no anchor. If the report calls it unreachable, it calls the connections dead.
    let reached: HashSet<AnchorId> = seen.keys().copied().collect();
    for module in compilation.resolution.graph.iter() {
        if module.is_package() {
            continue;
        }
        result.loaded += 1;
        if embedded.contains(&module.id) || result.modules.contains(&module.id) {
            continue;
        }
        if forwards_reached(compilation, module.id, &reached) {
            result.modules.insert(module.id);
        } else {
            result.unreachable_modules.push(module.path.clone());
        }
    }
    result.unreachable_modules.sort();

    result
}

/// True when a module exports a name that resolves to a reached item.
fn forwards_reached(
    compilation: &Compilation,
    module: ModuleId,
    reached: &HashSet<AnchorId>,
) -> bool {
    compilation
        .resolution
        .exported_names(module)
        .into_iter()
        .any(|name| {
            matches!(
                compilation
                    .resolution
                    .lookup_export(module, &name, &mut HashSet::new()),
                Some(Symbol::Anchor(anchor)) if reached.contains(&anchor)
            )
        })
}

/// Modules the compiler embeds.
///
/// `prelude` compiles some files of the specification into the binary with
/// `include_str!`. A project can also have them on disk. Then it holds each of
/// them two times. One copy is the file. One copy is the package module. All
/// modules import the package copy. Thus the file seems dead. But it is not
/// dead. It is the framework. A report that tells a person to delete it is
/// worse than no report.
fn embedded_modules(compilation: &Compilation) -> HashSet<ModuleId> {
    let packaged: HashSet<&str> = compilation
        .resolution
        .graph
        .iter()
        .filter(|module| module.is_package())
        .map(|module| module.source.as_str())
        .collect();
    if packaged.is_empty() {
        return HashSet::new();
    }
    compilation
        .resolution
        .graph
        .iter()
        .filter(|module| !module.is_package() && packaged.contains(module.source.as_str()))
        .map(|module| module.id)
        .collect()
}

/// Computes reachability from all exports of the entry module.
pub fn from_entry(compilation: &Compilation) -> Reachability {
    from_entry_and(compilation, &[])
}

/// The same as [`from_entry`], but with `extra` roots in addition to the
/// exports of the entry. These are the anchors that plugins reach.
pub fn from_entry_and(compilation: &Compilation, extra: &[AnchorId]) -> Reachability {
    let mut roots = compilation.entry_exports();
    add_roots(&mut roots, extra);
    let mut result = from_roots(compilation, &roots);
    result.unloaded_modules = unloaded(compilation);
    result
}

fn add_roots(roots: &mut Vec<AnchorId>, extra: &[AnchorId]) {
    for anchor in extra {
        if !roots.contains(anchor) {
            roots.push(*anchor);
        }
    }
}

/// The anchors, by file and name, as plugins name the anchors that they reach.
/// This function ignores a name that the file does not declare.
pub fn locate(compilation: &Compilation, named: &[(PathBuf, String)]) -> Vec<AnchorId> {
    let canonical = |path: &std::path::Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut out = Vec::new();
    for (file, name) in named {
        let graph = compilation.graph();
        let Some(module) = graph.id_for(file).or_else(|| graph.id_for(&canonical(file))) else {
            continue;
        };
        let found = compilation
            .store()
            .anchors
            .iter()
            .find(|def| def.module == module && def.name == *name);
        if let Some(def) = found {
            if !out.contains(&def.id) {
                out.push(def.id);
            }
        }
    }
    out
}

/// Source files that no module imports. Each has the number of anchors that it declares.
fn unloaded(compilation: &Compilation) -> Vec<Unloaded> {
    unloaded_sources(compilation)
        .into_iter()
        .map(|path| {
            let anchors = declared_anchor_count(&path);
            Unloaded { path, anchors }
        })
        .collect()
}

/// The number of anchors that a file declares, from its syntax only. When the
/// file does not read, the count is zero.
fn declared_anchor_count(path: &std::path::Path) -> usize {
    let Ok(source) = std::fs::read_to_string(path) else {
        return 0;
    };
    piton_syntax::parser::parse(&source, path)
        .file
        .items
        .iter()
        .filter(|item| matches!(item, piton_syntax::ast::Item::Anchor(_)))
        .count()
}

/// Source files below the project root that no import chain reaches.
///
/// A module that no module imports never goes into the graph. Thus it cannot
/// show as "loaded but unreached". Only a search of the file system finds it.
/// When a user asks what is unreachable, this is usually the answer they need.
pub fn unloaded_sources(compilation: &Compilation) -> Vec<PathBuf> {
    let config = compilation
        .project
        .config_path
        .as_ref()
        .map(|path| std::fs::canonicalize(path).unwrap_or_else(|_| path.clone()));
    crate::module::sources(&compilation.project.source_root)
        .into_iter()
        .filter(|path| compilation.graph().id_for(path).is_none())
        // The compiler reads the configuration, but no module imports it. It is
        // never part of the graph of the specbase, and it is never dead.
        .filter(|path| {
            config.as_ref().is_none_or(|config| {
                std::fs::canonicalize(path).unwrap_or_else(|_| path.clone()) != *config
            })
        })
        .collect()
}

/// Collects the anchors that a value depends on. A value that embeds an anchor
/// is a composition edge. A value that links to an anchor is a reference edge.
pub(crate) fn collect_targets(value: &Value, out: &mut Vec<(AnchorId, EdgeKind)>) {
    match value {
        Value::Anchor(id) => out.push((*id, EdgeKind::Composition)),
        Value::Reference(target) => out.push((target.anchor, EdgeKind::Reference)),
        Value::Str(text) => {
            for id in text.references() {
                out.push((id, EdgeKind::Reference));
            }
        }
        Value::List(items) => {
            for item in items {
                collect_targets(item, out);
            }
        }
        Value::Dict(map) => {
            for item in map.values() {
                collect_targets(item, out);
            }
        }
        Value::Mixed(mixed) => {
            for item in &mixed.items {
                match item {
                    MixedItem::Text(text) => {
                        for id in text.references() {
                            out.push((id, EdgeKind::Reference));
                        }
                    }
                    MixedItem::List(items) => {
                        for item in items {
                            collect_targets(item, out);
                        }
                    }
                    MixedItem::Entry(_, value) | MixedItem::Value(value) => {
                        collect_targets(value, out)
                    }
                }
            }
        }
        _ => {}
    }
}
