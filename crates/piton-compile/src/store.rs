//! Declaration storage.
//!
//! Anchors and variables are registered once and then referred to by id, so the
//! resolver can build graphs over them without fighting the borrow checker for
//! access to the syntax trees they came from.

use indexmap::IndexMap;
use piton_core::{AnchorId, Properties, Span, Value};
use piton_syntax::ast::TypeConstraint;

use crate::module::ModuleId;

/// Identifies a top-level variable declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VariableId(pub u32);

/// What a name binds to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Symbol {
    Anchor(AnchorId),
    Variable(VariableId),
}

/// Where a property's declaration lives, so the evaluator can find its syntax
/// and report the originating anchor when something fails.
#[derive(Debug, Clone)]
pub struct Slot {
    /// The anchor whose body declares the value used for this property.
    pub owner: AnchorId,
    /// Index into `owner`'s body properties.
    pub index: usize,
    /// Constraints in effect, taken from the right-most declaration that
    /// supplied any.
    pub constraints: Vec<TypeConstraint>,
    /// False when only an abstract slot was declared and no value was given.
    pub has_value: bool,
    pub span: Span,
}

/// A registered anchor declaration.
#[derive(Debug, Clone)]
pub struct AnchorDef {
    pub id: AnchorId,
    pub module: ModuleId,
    /// Index into the module's top-level items.
    pub item: usize,
    pub name: String,
    pub name_span: Span,
    pub span: Span,
    pub exported: bool,
    pub is_abstract: bool,
    /// `anchor`, or the user keyword that introduced the declaration.
    pub keyword: String,
    /// The keyword this anchor declares with `as`.
    pub alias: Option<String>,
    /// Bases in inheritance order: the keyword's anchor first, then `extends`.
    pub bases: Vec<AnchorId>,
    /// Properties in output order, each pointing at the declaration that wins.
    pub slots: IndexMap<String, Slot>,
    /// Resolved values, filled during evaluation.
    pub properties: Properties,
}

impl AnchorDef {
    /// Every anchor in this one's inheritance chain, nearest first.
    pub fn is_concrete(&self) -> bool {
        !self.is_abstract
    }
}

/// A registered top-level variable.
#[derive(Debug, Clone)]
pub struct VariableDef {
    pub id: VariableId,
    pub module: ModuleId,
    pub item: usize,
    pub name: String,
    pub name_span: Span,
    pub span: Span,
    pub exported: bool,
    pub constraints: Vec<TypeConstraint>,
    pub value: Option<Value>,
}

/// All declarations in a compilation.
#[derive(Default)]
pub struct Store {
    pub anchors: Vec<AnchorDef>,
    pub variables: Vec<VariableDef>,
}

impl Store {
    pub fn anchor(&self, id: AnchorId) -> &AnchorDef {
        &self.anchors[id.0 as usize]
    }

    pub fn anchor_mut(&mut self, id: AnchorId) -> &mut AnchorDef {
        &mut self.anchors[id.0 as usize]
    }

    pub fn variable(&self, id: VariableId) -> &VariableDef {
        &self.variables[id.0 as usize]
    }

    pub fn variable_mut(&mut self, id: VariableId) -> &mut VariableDef {
        &mut self.variables[id.0 as usize]
    }

    pub fn push_anchor(&mut self, mut def: AnchorDef) -> AnchorId {
        let id = AnchorId(self.anchors.len() as u32);
        def.id = id;
        self.anchors.push(def);
        id
    }

    pub fn push_variable(&mut self, mut def: VariableDef) -> VariableId {
        let id = VariableId(self.variables.len() as u32);
        def.id = id;
        self.variables.push(def);
        id
    }

    /// True when `candidate` appears anywhere in `anchor`'s inheritance chain,
    /// which is what the `extends T` constraint form tests.
    pub fn inherits_from(&self, anchor: AnchorId, candidate: AnchorId) -> bool {
        if anchor == candidate {
            return true;
        }
        let mut stack = self.anchor(anchor).bases.clone();
        let mut seen = vec![anchor];
        while let Some(base) = stack.pop() {
            if base == candidate {
                return true;
            }
            if seen.contains(&base) {
                continue;
            }
            seen.push(base);
            stack.extend(self.anchor(base).bases.iter().copied());
        }
        false
    }

    /// The chain an anchor inherits from, nearest base last, in the order
    /// property collisions are resolved.
    pub fn base_chain(&self, anchor: AnchorId) -> Vec<AnchorId> {
        let mut out = Vec::new();
        let mut seen = Vec::new();
        collect_chain(self, anchor, &mut out, &mut seen);
        out
    }
}

fn collect_chain(store: &Store, anchor: AnchorId, out: &mut Vec<AnchorId>, seen: &mut Vec<AnchorId>) {
    if seen.contains(&anchor) {
        return;
    }
    seen.push(anchor);
    for base in &store.anchor(anchor).bases {
        collect_chain(store, *base, out, seen);
    }
    out.push(anchor);
}
