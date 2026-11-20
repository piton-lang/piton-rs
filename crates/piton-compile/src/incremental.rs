//! Evaluation again of only the items that an edit changes.
//!
//! The language server compiles all of the project after each keystroke. Most
//! values do not change. The resolver still runs in full. Then the
//! evaluator takes the value of each item from the compilation before, when
//! nothing that the item depends on changed.
//!
//! This is safe only when each id names the same item in the two
//! compilations. Thus the plan compares the two stores first. When they
//! differ in any way that the values can depend on, there is no plan, and the
//! evaluation runs in full.

use std::collections::{HashMap, HashSet, VecDeque};

use piton_core::AnchorId;
use piton_syntax::ast::TypeConstraint;

use crate::eval::Read;
use crate::module::ModuleId;
use crate::resolve::Resolution;
use crate::store::{Store, VariableId};
use crate::Compilation;

/// The items to evaluate again.
pub struct Plan {
    pub dirty_anchors: HashSet<AnchorId>,
    pub dirty_variables: HashSet<VariableId>,
    /// The changed modules, and the modules that can now see other names
    /// through their imports. Each other module has the same text and the
    /// same names as in the compilation before.
    pub modules: HashSet<ModuleId>,
}

/// One node of the dependency graph: an anchor or a variable.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Node {
    Anchor(AnchorId),
    Variable(VariableId),
}

fn node(read: &Read) -> Node {
    match read {
        Read::Property(anchor, _) => Node::Anchor(*anchor),
        Read::Variable(variable) => Node::Variable(*variable),
    }
}

/// The items of `resolution` whose values can differ from the values of
/// `previous`. `None` when the two compilations do not have the same items.
pub fn plan(previous: &Compilation, resolution: &Resolution) -> Option<Plan> {
    let before = &previous.resolution;
    if before.graph.len() != resolution.graph.len() || before.has_entry != resolution.has_entry {
        return None;
    }
    // The modules must have the same ids, and the changed modules are known.
    let mut changed: Vec<ModuleId> = Vec::new();
    for (old, new) in before.graph.iter().zip(resolution.graph.iter()) {
        if old.path != new.path {
            return None;
        }
        if old.source != new.source {
            changed.push(new.id);
        }
    }
    if !same_declarations(&before.store, &resolution.store) {
        return None;
    }
    // A diagnostic of the evaluation before that names no item cannot go
    // with the value of an item. The evaluation then runs in full.
    if previous.evaluation.iter().any(|(frame, _)| frame.is_none()) {
        return None;
    }

    // A module that imports from a changed module can see other names now,
    // but only when the changed module exports other names. Then each item
    // of such a module is old too. Most edits change a value and not the
    // exports. An index exports most of a project again, so most modules
    // import through one.
    let exports_changed = changed
        .iter()
        .any(|module| exports(before, *module) != exports(resolution, *module));
    let mut users: HashMap<ModuleId, Vec<ModuleId>> = HashMap::new();
    for (module, scope) in &resolution.scopes {
        for dependency in scope.dependencies.iter().chain(&scope.used) {
            users.entry(*dependency).or_default().push(*module);
        }
    }
    let mut modules: HashSet<ModuleId> = changed.iter().copied().collect();
    let mut queue: VecDeque<ModuleId> = if exports_changed { changed.into_iter().collect() } else { VecDeque::new() };
    while let Some(module) = queue.pop_front() {
        for user in users.get(&module).into_iter().flatten() {
            if modules.insert(*user) {
                queue.push_back(*user);
            }
        }
    }

    // What depends on each item: each item that read it, and each anchor
    // that inherits a declaration of it.
    let mut dependents: HashMap<Node, Vec<Node>> = HashMap::new();
    for (frame, reads) in &previous.reads {
        for read in reads {
            dependents.entry(node(read)).or_default().push(node(frame));
        }
    }
    for def in &resolution.store.anchors {
        for slot in def.slots.values() {
            if slot.owner != def.id {
                dependents.entry(Node::Anchor(slot.owner)).or_default().push(Node::Anchor(def.id));
            }
        }
    }

    let mut dirty: HashSet<Node> = HashSet::new();
    let mut queue: VecDeque<Node> = VecDeque::new();
    let seeds = resolution
        .store
        .anchors
        .iter()
        .filter(|def| modules.contains(&def.module))
        .map(|def| Node::Anchor(def.id))
        .chain(
            resolution
                .store
                .variables
                .iter()
                .filter(|def| modules.contains(&def.module))
                .map(|def| Node::Variable(def.id)),
        );
    for seed in seeds {
        if dirty.insert(seed) {
            queue.push_back(seed);
        }
    }
    while let Some(item) = queue.pop_front() {
        for dependent in dependents.get(&item).into_iter().flatten() {
            if dirty.insert(*dependent) {
                queue.push_back(*dependent);
            }
        }
    }

    let mut plan = Plan {
        dirty_anchors: HashSet::new(),
        dirty_variables: HashSet::new(),
        modules,
    };
    for item in dirty {
        match item {
            Node::Anchor(anchor) => plan.dirty_anchors.insert(anchor),
            Node::Variable(variable) => plan.dirty_variables.insert(variable),
        };
    }
    // An edit can add or remove a property of an anchor that it evaluates
    // again. Each item that keeps its value must have the same properties as
    // before, or its old value would not fit.
    let kept_same = before.store.anchors.iter().zip(&resolution.store.anchors).all(|(a, b)| {
        plan.dirty_anchors.contains(&b.id) || same_slots(a, b)
    }) && before.store.variables.iter().zip(&resolution.store.variables).all(|(a, b)| {
        plan.dirty_variables.contains(&b.id) || same_constraints(&a.constraints, &b.constraints)
    });
    kept_same.then_some(plan)
}

/// What a module exports: each name, and the item that it names.
fn exports(resolution: &Resolution, module: ModuleId) -> Vec<(String, Option<crate::store::Symbol>)> {
    resolution
        .exported_names(module)
        .into_iter()
        .map(|name| {
            let symbol = resolution.lookup_export(module, &name, &mut HashSet::new());
            (name, symbol)
        })
        .collect()
}

/// Tells if two stores declare the same anchors and variables, with the same
/// ids, names, kinds and bases. Thus each id names the same item in the two.
/// Positions in the text do not count, because an edit moves them. The
/// properties of an anchor can differ, when the anchor is evaluated again.
fn same_declarations(old: &Store, new: &Store) -> bool {
    if old.anchors.len() != new.anchors.len() || old.variables.len() != new.variables.len() {
        return false;
    }
    let anchors = old.anchors.iter().zip(&new.anchors).all(|(a, b)| {
        a.module == b.module
            && a.item == b.item
            && a.name == b.name
            && a.exported == b.exported
            && a.is_abstract == b.is_abstract
            && a.keyword == b.keyword
            && a.alias == b.alias
            && a.bases == b.bases
    });
    let variables = old
        .variables
        .iter()
        .zip(&new.variables)
        .all(|(a, b)| a.module == b.module && a.item == b.item && a.name == b.name && a.exported == b.exported);
    anchors && variables
}

/// Tells if an anchor has the same properties in two stores: the same names
/// in the same order, from the same declarations, with the same types.
fn same_slots(a: &crate::store::AnchorDef, b: &crate::store::AnchorDef) -> bool {
    a.slots.len() == b.slots.len()
        && a.slots.iter().zip(&b.slots).all(|((name_a, slot_a), (name_b, slot_b))| {
            name_a == name_b
                && slot_a.owner == slot_b.owner
                && slot_a.index == slot_b.index
                && slot_a.has_value == slot_b.has_value
                && same_constraints(&slot_a.constraints, &slot_b.constraints)
        })
}

fn same_constraints(a: &[TypeConstraint], b: &[TypeConstraint]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| a.extends == b.extends && a.list == b.list && a.name == b.name)
}
