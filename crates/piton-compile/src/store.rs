//! Stores declarations.
//!
//! The store records each anchor and variable one time. Other code then uses
//! its id. Thus the resolver can make graphs over them. It does not need access
//! to their syntax trees, and so it has no conflict with the borrow checker.

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

/// The location of the declaration of a property. The evaluator uses it to
/// find the syntax. When an error occurs, the evaluator reports the source anchor.
#[derive(Debug, Clone)]
pub struct Slot {
    /// The anchor that has the body that declares the value of this property.
    pub owner: AnchorId,
    /// The index into the body properties of `owner`.
    pub index: usize,
    /// The constraints in effect. They come from the right-most declaration
    /// that has constraints.
    pub constraints: Vec<TypeConstraint>,
    /// False when the declaration gives only an abstract slot and no value.
    pub has_value: bool,
    pub span: Span,
}

/// An anchor declaration in the store.
#[derive(Debug, Clone)]
pub struct AnchorDef {
    pub id: AnchorId,
    pub module: ModuleId,
    /// The index into the top-level items of the module.
    pub item: usize,
    pub name: String,
    pub name_span: Span,
    pub span: Span,
    pub exported: bool,
    pub is_abstract: bool,
    /// `anchor`, or the user keyword at the start of the declaration.
    pub keyword: String,
    /// The keyword this anchor declares with `as`.
    pub alias: Option<String>,
    /// Bases in inheritance order. First come the `extends` bases from left to
    /// right. The anchor of the keyword is always last. Thus it wins collisions.
    pub bases: Vec<AnchorId>,
    /// Properties in output order. Each property refers to the declaration that wins.
    pub slots: IndexMap<String, Slot>,
    /// Resolved values. The evaluator fills them.
    pub properties: Properties,
}

impl AnchorDef {
    /// All anchors in the inheritance chain of this anchor, nearest first.
    pub fn is_concrete(&self) -> bool {
        !self.is_abstract
    }
}

/// A top-level variable in the store.
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

    /// True when `candidate` is in the inheritance chain of `anchor`. The
    /// `extends T` constraint form makes this test.
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

    /// The chain of bases of an anchor, nearest base last. This is the order
    /// in which the resolver resolves property collisions.
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
