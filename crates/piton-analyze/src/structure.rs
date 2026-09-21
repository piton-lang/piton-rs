//! Structural analysis.
//!
//! Piton structure is deterministic knowledge, and it is what keeps the
//! linguistic side honest: two sentences that say incompatible things about "the
//! button" only matter if the structure says they are talking about the same
//! button.
//!
//! The evidence is ordered local before global, so a pair inside one anchor
//! counts for more than a pair at opposite ends of the project.

use piton_core::{AnchorId, Span};
use piton_compile::{Compilation, ModuleId};

/// Where a statement was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Site {
    pub anchor: AnchorId,
    pub module: ModuleId,
    /// Dotted path of the property the statement was written in.
    pub property: String,
    pub span: Span,
}

/// The structural relationship between two sites, strongest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Relation {
    /// Different files with no connection between them.
    Unrelated,
    /// The same project, nothing closer.
    SameProject,
    /// Sibling files in one directory.
    SameDirectory,
    /// One file imports the other, directly or not.
    ImportPath,
    /// The same source file.
    SameFile,
    /// One anchor references or embeds the other.
    Composed,
    /// The two anchors share a base.
    SharedBase,
    /// One anchor inherits from the other.
    Inheritance,
    /// The same anchor, different properties.
    SameAnchor,
    /// The same property of the same anchor.
    SameProperty,
}

impl Relation {
    /// How strongly this relation says the two sites share a conceptual scope.
    pub fn confidence(self) -> f64 {
        match self {
            Relation::SameProperty => 1.0,
            Relation::SameAnchor => 0.9,
            Relation::Inheritance => 0.85,
            Relation::SharedBase => 0.7,
            Relation::Composed => 0.65,
            Relation::SameFile => 0.55,
            Relation::ImportPath => 0.4,
            Relation::SameDirectory => 0.35,
            Relation::SameProject => 0.15,
            Relation::Unrelated => 0.0,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Relation::SameProperty => "the same property of the same anchor",
            Relation::SameAnchor => "the same anchor",
            Relation::Inheritance => "one anchor inherits from the other",
            Relation::SharedBase => "the anchors share a base",
            Relation::Composed => "one anchor references the other",
            Relation::SameFile => "the same file",
            Relation::ImportPath => "one file imports the other",
            Relation::SameDirectory => "the same directory",
            Relation::SameProject => "the same project",
            Relation::Unrelated => "no structural relationship",
        }
    }
}

/// Precomputed structural facts, so a pairwise query is cheap.
pub struct Structure<'a> {
    compilation: &'a Compilation,
    /// Anchors each anchor references or embeds, transitively through values.
    composed: Vec<Vec<AnchorId>>,
}

impl<'a> Structure<'a> {
    pub fn build(compilation: &'a Compilation) -> Structure<'a> {
        let count = compilation.store().anchors.len();
        let mut composed = vec![Vec::new(); count];
        for index in 0..count {
            let anchor = AnchorId(index as u32);
            let mut targets = Vec::new();
            for value in &compilation.store().anchor(anchor).properties {
                collect(value.1, &mut targets);
            }
            targets.sort();
            targets.dedup();
            composed[index] = targets;
        }
        Structure {
            compilation,
            composed,
        }
    }

    /// The strongest relationship between two sites.
    pub fn relate(&self, left: &Site, right: &Site) -> Relation {
        if left.anchor == right.anchor {
            return if left.property == right.property {
                Relation::SameProperty
            } else {
                Relation::SameAnchor
            };
        }

        let store = self.compilation.store();
        if store.inherits_from(left.anchor, right.anchor)
            || store.inherits_from(right.anchor, left.anchor)
        {
            return Relation::Inheritance;
        }
        if self.shares_a_base(left.anchor, right.anchor) {
            return Relation::SharedBase;
        }
        if self.composed[left.anchor.0 as usize].contains(&right.anchor)
            || self.composed[right.anchor.0 as usize].contains(&left.anchor)
        {
            return Relation::Composed;
        }
        if left.module == right.module {
            return Relation::SameFile;
        }
        if self.imports(left.module, right.module) || self.imports(right.module, left.module) {
            return Relation::ImportPath;
        }

        let left_path = self.compilation.graph().get(left.module).path.clone();
        let right_path = self.compilation.graph().get(right.module).path.clone();
        if left_path.parent() == right_path.parent() {
            return Relation::SameDirectory;
        }
        Relation::SameProject
    }

    fn shares_a_base(&self, left: AnchorId, right: AnchorId) -> bool {
        let store = self.compilation.store();
        let left_chain = store.base_chain(left);
        let right_chain = store.base_chain(right);
        left_chain
            .iter()
            .any(|base| *base != left && right_chain.contains(base) && *base != right)
    }

    /// True when `from` reaches `to` through its imports, at any depth.
    fn imports(&self, from: ModuleId, to: ModuleId) -> bool {
        let mut seen = vec![from];
        let mut queue = vec![from];
        while let Some(module) = queue.pop() {
            for next in &self.compilation.resolution.scope(module).dependencies {
                if *next == to {
                    return true;
                }
                if !seen.contains(next) {
                    seen.push(*next);
                    queue.push(*next);
                }
            }
        }
        false
    }
}

fn collect(value: &piton_core::Value, out: &mut Vec<AnchorId>) {
    use piton_core::{MixedItem, Value};
    match value {
        Value::Anchor(id) | Value::Reference(id) => out.push(*id),
        Value::Str(text) => out.extend(text.references()),
        Value::List(items) => items.iter().for_each(|item| collect(item, out)),
        Value::Dict(map) => map.values().for_each(|item| collect(item, out)),
        Value::Mixed(mixed) => {
            for item in &mixed.items {
                match item {
                    MixedItem::Text(text) => out.extend(text.references()),
                    MixedItem::List(items) => items.iter().for_each(|item| collect(item, out)),
                    MixedItem::Entry(_, value) => collect(value, out),
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relations_are_ordered_local_before_global() {
        assert!(Relation::SameProperty > Relation::SameAnchor);
        assert!(Relation::SameAnchor > Relation::Inheritance);
        assert!(Relation::Inheritance > Relation::SameFile);
        assert!(Relation::SameFile > Relation::SameDirectory);
        assert!(Relation::SameDirectory > Relation::SameProject);
    }

    #[test]
    fn confidence_falls_with_distance() {
        let ordered = [
            Relation::SameProperty,
            Relation::SameAnchor,
            Relation::Inheritance,
            Relation::SharedBase,
            Relation::Composed,
            Relation::SameFile,
            Relation::ImportPath,
            Relation::SameDirectory,
            Relation::SameProject,
            Relation::Unrelated,
        ];
        for pair in ordered.windows(2) {
            assert!(
                pair[0].confidence() > pair[1].confidence(),
                "{:?} should outrank {:?}",
                pair[0],
                pair[1]
            );
        }
    }
}
