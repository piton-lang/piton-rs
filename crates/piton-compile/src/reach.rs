//! Reachability analysis.
//!
//! Only source reachable from the configured entrypoints is compiled, so
//! knowing what is reachable -- and what is not -- is part of understanding a
//! specbase. `piton reach` reports both.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use piton_core::{AnchorId, MixedItem, Value};

use crate::module::ModuleId;
use crate::store::Symbol;
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

/// One reachable anchor and how it was found.
#[derive(Debug, Clone)]
pub struct Reached {
    pub anchor: AnchorId,
    /// Number of edges from the nearest root.
    pub depth: usize,
    /// The shortest path of anchor names from a root.
    pub path: Vec<String>,
    /// The edge that arrived here, or `None` for a root, which was not
    /// reached from anywhere.
    pub via: Option<EdgeKind>,
}

/// The result of a reachability query.
#[derive(Debug, Clone, Default)]
pub struct Reachability {
    pub reached: Vec<Reached>,
    pub unreachable: Vec<AnchorId>,
    /// Modules that carry something the roots reach, either by declaring it or
    /// by exporting it onward.
    pub modules: HashSet<ModuleId>,
    /// Modules that were loaded but carry nothing the roots reach.
    pub unreachable_modules: Vec<PathBuf>,
    /// Source files nothing imports, so compilation never even opened them.
    pub unloaded_modules: Vec<PathBuf>,
    /// How many modules of the specbase were loaded, packages aside.
    pub loaded: usize,
}

impl Reachability {
    pub fn contains(&self, anchor: AnchorId) -> bool {
        self.reached.iter().any(|entry| entry.anchor == anchor)
    }

    pub fn depth_of(&self, anchor: AnchorId) -> Option<usize> {
        self.reached
            .iter()
            .find(|entry| entry.anchor == anchor)
            .map(|entry| entry.depth)
    }
}

/// Computes reachability from a set of root anchors.
pub fn from_roots(compilation: &Compilation, roots: &[AnchorId]) -> Reachability {
    let mut result = Reachability::default();
    let mut seen: HashMap<AnchorId, usize> = HashMap::new();
    let mut queue: std::collections::VecDeque<(AnchorId, usize, Vec<String>, Option<EdgeKind>)> =
        Default::default();

    for root in roots {
        if seen.contains_key(root) {
            continue;
        }
        seen.insert(*root, 0);
        let name = compilation.resolution.store.anchor(*root).name.clone();
        queue.push_back((*root, 0, vec![name], None));
    }

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

    let embedded = embedded_modules(compilation);

    for def in &compilation.resolution.store.anchors {
        // Anchors inside a bundled package are the compiler's own vocabulary,
        // not part of the specbase being analyzed.
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

    // A module earns its place either by declaring something the roots reach or
    // by handing it on. An index that does nothing but re-export is how most of
    // a specbase is held together: it declares no anchor of its own, and
    // calling it unreachable for that reason is calling the wiring dead.
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

/// Whether a module exports a name that resolves to something reached.
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
/// `prelude` compiles some of the specification's own files into the binary
/// with `include_str!`, so a project that also has them on disk holds each of
/// them twice: once as the file, once as the package module it became.
/// Everything imports the package copy, which leaves the file looking dead --
/// and it is the opposite of dead. It is the framework, and a report that
/// invites someone to delete it is worse than no report.
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

/// Computes reachability from everything the entry module exports.
pub fn from_entry(compilation: &Compilation) -> Reachability {
    let roots = compilation.entry_exports();
    let mut result = from_roots(compilation, &roots);
    result.unloaded_modules = unloaded_sources(compilation);
    result
}

/// Source files beneath the project root that no import chain reaches.
///
/// A module nothing imports never enters the graph at all, so it cannot show up
/// as "loaded but unreached". Finding it takes a look at the filesystem, and it
/// is usually the answer someone is after when they ask what is unreachable.
pub fn unloaded_sources(compilation: &Compilation) -> Vec<PathBuf> {
    let root = &compilation.project.source_root;
    let mut found = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if !matches!(name.as_ref(), "target" | ".git" | "node_modules" | ".piton") {
                    stack.push(path);
                }
            } else if path.extension().is_some_and(|extension| extension == "pi")
                && compilation.graph().id_for(&path).is_none()
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Computes reachability from every anchor a module declares.
pub fn from_module(compilation: &Compilation, module: ModuleId) -> Reachability {
    let roots: Vec<AnchorId> = compilation
        .resolution
        .scope(module)
        .declarations
        .values()
        .filter_map(|symbol| match symbol {
            Symbol::Anchor(anchor) => Some(*anchor),
            _ => None,
        })
        .collect();
    from_roots(compilation, &roots)
}

/// Collects the anchors a value depends on. A value that embeds an anchor is a
/// composition edge; a value that links to one is a reference edge.
fn collect_targets(value: &Value, out: &mut Vec<(AnchorId, EdgeKind)>) {
    match value {
        Value::Anchor(id) => out.push((*id, EdgeKind::Composition)),
        Value::Reference(id) => out.push((*id, EdgeKind::Reference)),
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
                    MixedItem::Entry(_, value) => collect_targets(value, out),
                }
            }
        }
        _ => {}
    }
}
