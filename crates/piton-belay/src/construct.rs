//! Recognizing Belay constructs.
//!
//! The four constructs are ordinary Piton anchors. What makes one a skill is
//! that its inheritance chain reaches Belay's `Skill` anchor, so recognition is
//! a question about the resolved chain rather than about a keyword spelling.

use piton_compile::{Compilation, Symbol};
use piton_core::{AnchorId, Properties, Value};

use crate::prelude;

/// Which of the four agentic constructs an anchor is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConstructKind {
    Instruction,
    Skill,
    Command,
    Agent,
}

impl ConstructKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ConstructKind::Instruction => "instruction",
            ConstructKind::Skill => "skill",
            ConstructKind::Command => "command",
            ConstructKind::Agent => "agent",
        }
    }

    /// Properties that have a defined place in the generated artifact and are
    /// therefore not repeated in the serialized remainder.
    pub fn primary_properties(self) -> &'static [&'static str] {
        match self {
            ConstructKind::Instruction => &["description", "prompt"],
            ConstructKind::Skill => &["description", "prompt", "useWhen"],
            ConstructKind::Command => &["description", "prompt"],
            ConstructKind::Agent => &["description", "role", "prompt"],
        }
    }
}

/// A recognized construct, with its resolved content.
#[derive(Debug, Clone)]
pub struct Construct {
    pub anchor: AnchorId,
    pub kind: ConstructKind,
    pub name: String,
    pub properties: Properties,
}

impl Construct {
    /// A prose property rendered for a document body, with references turned
    /// into links relative to the file being written.
    pub fn text(&self, key: &str, context: &piton_emit::markdown::Context<'_>) -> Option<String> {
        match self.properties.get(key) {
            Some(Value::Str(text)) => Some(piton_emit::markdown::inline_text(text, context)),
            _ => None,
        }
    }

    /// A prose property rendered for metadata, where a Markdown link would not
    /// be read as one. References become the referenced anchor's name.
    pub fn metadata_text(&self, key: &str, anchors: &dyn piton_core::AnchorView) -> Option<String> {
        match self.properties.get(key) {
            Some(Value::Str(text)) => Some(text.render_plain(anchors)),
            _ => None,
        }
    }

    /// Properties beyond the ones with a defined place, plus any native option
    /// names the adapter consumes separately.
    pub fn remainder(&self, consumed: &[&str]) -> Properties {
        let primary = self.kind.primary_properties();
        self.properties
            .iter()
            .filter(|(name, _)| {
                !primary.contains(&name.as_str()) && !consumed.contains(&name.as_str())
            })
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    }
}

/// Finds the anchor ids of Belay's four abstract constructs.
#[derive(Debug, Default, Clone, Copy)]
pub struct ConstructAnchors {
    pub instruction: Option<AnchorId>,
    pub skill: Option<AnchorId>,
    pub command: Option<AnchorId>,
    pub agent: Option<AnchorId>,
}

impl ConstructAnchors {
    /// Locates the constructs exported by the bundled Belay package.
    pub fn find(compilation: &Compilation) -> ConstructAnchors {
        let mut found = ConstructAnchors::default();
        let Some(module) = compilation
            .graph()
            .id_for(std::path::Path::new(prelude::BELAY_PACKAGE))
        else {
            return found;
        };
        let lookup = |name: &str| -> Option<AnchorId> {
            match compilation
                .resolution
                .lookup_export(module, name, &mut Default::default())
            {
                Some(Symbol::Anchor(anchor)) => Some(anchor),
                _ => None,
            }
        };
        found.instruction = lookup("Instruction");
        found.skill = lookup("Skill");
        found.command = lookup("Command");
        found.agent = lookup("Agent");
        found
    }

    /// Classifies an anchor by walking its inheritance chain.
    pub fn classify(
        &self,
        compilation: &Compilation,
        anchor: AnchorId,
    ) -> Option<ConstructKind> {
        let store = compilation.store();
        // Order matters only in that every construct also inherits Construct;
        // each of the four is distinct, so at most one matches.
        for (candidate, kind) in [
            (self.skill, ConstructKind::Skill),
            (self.command, ConstructKind::Command),
            (self.agent, ConstructKind::Agent),
            (self.instruction, ConstructKind::Instruction),
        ] {
            if let Some(base) = candidate {
                if anchor != base && store.inherits_from(anchor, base) {
                    return Some(kind);
                }
            }
        }
        None
    }
}

/// Collects every concrete construct among `anchors`.
pub fn collect(compilation: &Compilation, anchors: &[AnchorId]) -> Vec<Construct> {
    let constructs = ConstructAnchors::find(compilation);
    let mut found: Vec<Construct> = anchors
        .iter()
        .filter(|anchor| !compilation.store().anchor(**anchor).is_abstract)
        .filter_map(|anchor| {
            let kind = constructs.classify(compilation, *anchor)?;
            let def = compilation.store().anchor(*anchor);
            Some(Construct {
                anchor: *anchor,
                kind,
                name: def.name.clone(),
                properties: def.properties.clone(),
            })
        })
        .collect();
    // Output order must not depend on traversal order.
    found.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.cmp(&b.name)));
    found
}
