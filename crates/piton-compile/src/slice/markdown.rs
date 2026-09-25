//! Renders a slice as one Markdown document, for pasting into a prompt.
//!
//! Values are written with the same Markdown rules `piton compile` uses. Two
//! things differ, because the reader is holding one document rather than a
//! tree of files: a reference is written as the name it points at, since the
//! declaration it names is further down the same page; and an embedded anchor
//! is written the same way rather than copied in, since the slice already
//! holds it once.
//!
//! A slice can cite either the source or a compiled output. [`render`] says
//! which `.pi` file each declaration is in. [`render_compiled`] says where a
//! build compiled it instead, and turns every name it cites into a link there,
//! for an agent that reads the compiled documents rather than the source.
//!
//! Headings name declarations exactly as the source does -- `SaveButton`,
//! `SaveButton.color` -- so the names used in the text can be found.

use std::collections::HashMap;
use std::path::Path;

use piton_core::{AnchorId, Mixed, MixedItem, Ref, Value};
use piton_emit::markdown::{self as emit, LinkResolver as _};

use super::{Entity, Entry, Relation, Slice};
use crate::eval::constraint_label;
use crate::store::VariableId;
use crate::Compilation;

/// Renders `slice` citing the source, with each path written relative to
/// `base`, the directory the reader is in: `../ui.pi` when `base` is a
/// sibling of the file's directory.
pub fn render(compilation: &Compilation, slice: &Slice, base: &Path) -> String {
    Writer {
        compilation,
        slice,
        cite: Cite::Source(base),
    }
    .render()
}

/// Renders `slice` citing a compiled output: `links` says where each anchor,
/// or property on one, was compiled to.
///
/// An anchor with no document of its own that is embedded by value, `{...}`,
/// is cited where it was inlined: the section of the property that embeds it.
///
/// The source is never cited. Anything the slice reaches that has no
/// compiled location of its own was still compiled, into something that
/// does: an abstract anchor into the anchors that extend it, a value that is
/// read into whatever reads it. Such an anchor is described without a place.
pub fn render_compiled(
    compilation: &Compilation,
    slice: &Slice,
    links: &dyn emit::LinkResolver,
) -> String {
    let located = Located::new(compilation, links);
    Writer {
        compilation,
        slice,
        cite: Cite::Compiled { links: &located },
    }
    .render()
}

/// Where `target` was compiled under `links`: its own document, or the
/// section of whatever embeds it.
pub fn compiled_location(
    compilation: &Compilation,
    links: &dyn emit::LinkResolver,
    target: &Ref,
) -> Option<String> {
    Located::new(compilation, links).link(target)
}

/// A compiled output's locations, extended to anchors that only exist
/// inlined into another's document.
struct Located<'a> {
    links: &'a dyn emit::LinkResolver,
    /// Every property in the compilation that embeds each anchor, `{...}`,
    /// in declaration order. What embeds an anchor is upstream of it, so the
    /// slice itself never holds it.
    embedders: HashMap<AnchorId, Vec<(AnchorId, String)>>,
}

impl<'a> Located<'a> {
    fn new(compilation: &Compilation, links: &'a dyn emit::LinkResolver) -> Located<'a> {
        let mut embedders: HashMap<AnchorId, Vec<(AnchorId, String)>> = HashMap::new();
        for def in &compilation.store().anchors {
            for (name, value) in def.properties.iter() {
                let mut embedded = Vec::new();
                embedded_in(value, &mut embedded);
                for anchor in embedded {
                    let holders = embedders.entry(anchor).or_default();
                    let holder = (def.id, name.clone());
                    if !holders.contains(&holder) {
                        holders.push(holder);
                    }
                }
            }
        }
        Located { links, embedders }
    }

    /// Where `target` was compiled, and whether that is inside another
    /// anchor's section because it was embedded there.
    fn find(&self, target: &Ref) -> Option<(String, bool)> {
        if let Some(link) = self.links.link(target) {
            return Some((link, false));
        }
        let mut seen = vec![target.anchor];
        self.embedding(target.anchor, &target.path, &mut seen)
            .map(|link| (link, true))
    }

    /// The section of the first property that embeds `anchor` and was
    /// itself compiled somewhere, following embeddings outward.
    ///
    /// `path` is what is wanted inside `anchor`, and grows with each step
    /// out, so the link lands on the deepest heading the build wrote:
    /// `Cli.commands` inside `Tooling.cli` becomes `Tooling.cli.commands`.
    fn embedding(
        &self,
        anchor: AnchorId,
        path: &[String],
        seen: &mut Vec<AnchorId>,
    ) -> Option<String> {
        for (holder, name) in self.embedders.get(&anchor).into_iter().flatten() {
            let property = Ref {
                anchor: *holder,
                path: std::iter::once(name.clone())
                    .chain(path.iter().cloned())
                    .collect(),
            };
            if let Some(link) = self.links.link(&property) {
                return Some(link);
            }
            if !seen.contains(holder) {
                seen.push(*holder);
                if let Some(link) = self.embedding(*holder, &property.path, seen) {
                    return Some(link);
                }
            }
        }
        None
    }

    fn embedded(&self, anchor: AnchorId) -> bool {
        matches!(self.find(&Ref::anchor(anchor)), Some((_, true)))
    }
}

impl emit::LinkResolver for Located<'_> {
    fn link(&self, target: &Ref) -> Option<String> {
        self.find(target).map(|(link, _)| link)
    }
}

/// What the document points its reader at.
#[derive(Clone, Copy)]
enum Cite<'a> {
    Source(&'a Path),
    Compiled { links: &'a Located<'a> },
}

struct Writer<'a> {
    compilation: &'a Compilation,
    slice: &'a Slice,
    cite: Cite<'a>,
}

impl Writer<'_> {
    fn render(&self) -> String {
        let mut out = String::new();

        let target = self.slice.target.display(self.compilation);
        out.push_str(&format!("# {target}\n\n"));
        let from = match self.location(&self.slice.target) {
            Some(location) => format!(", from `{location}` and the declarations it reaches"),
            None => String::new(),
        };
        out.push_str(&format!(
            "The part of the specification `{target}` depends on{from}. Each \
declaration appears once, and the names used in the text refer to the \
sections below.\n\n"
        ));
        if !self.slice.chain.is_empty() {
            let links: Vec<String> = self.slice.chain.iter().map(|link| self.name(link)).collect();
            out.push_str(&format!(
                "The entry reaches it through {}, which are included too.\n\n",
                series(&links)
            ));
        }

        for entry in &self.slice.entries {
            match entry {
                Entry::Anchor {
                    anchor,
                    whole,
                    properties,
                } => self.anchor(*anchor, *whole, properties, &mut out),
                Entry::Variable(variable) => self.variable(*variable, &mut out),
            }
        }

        while out.ends_with('\n') {
            out.pop();
        }
        out.push('\n');
        out
    }

    fn source_path(&self, base: &Path, module: crate::ModuleId) -> String {
        let path = &self.compilation.graph().get(module).path;
        // A bundled package's file is not on disk, so it has no path from
        // anywhere and is written as its name.
        if !path.is_absolute() {
            return path.display().to_string();
        }
        piton_emit::relative_link(base, path)
    }

    /// Where the reader finds `entity`: its source file, or the place it was
    /// compiled to. A variable is never compiled on its own -- its value is
    /// part of whatever reads it -- and neither is an anchor with no document
    /// of its own, so neither has a compiled location.
    fn location(&self, entity: &Entity) -> Option<String> {
        match self.cite {
            Cite::Source(base) => Some(self.source_path(base, entity.module(self.compilation))),
            Cite::Compiled { links } => match entity {
                Entity::Variable(_) => None,
                _ => links.link(&reference(entity)),
            },
        }
    }

    /// A name the text mentions: a code span, or a link to where it was
    /// compiled.
    fn name(&self, entity: &Entity) -> String {
        let name = entity.display(self.compilation);
        match (self.cite, entity) {
            (Cite::Compiled { links, .. }, Entity::Anchor(_) | Entity::Property(..)) => {
                match links.link(&reference(entity)) {
                    Some(link) => format!("[`{name}`]({link})"),
                    None => format!("`{name}`"),
                }
            }
            _ => format!("`{name}`"),
        }
    }

    fn anchor(&self, anchor: AnchorId, whole: bool, properties: &[String], out: &mut String) {
        let store = self.compilation.store();
        let def = store.anchor(anchor);
        out.push_str(&format!("## {}\n\n", def.name));

        let kind = if def.is_abstract { "Abstract anchor" } else { "Anchor" };
        let place = match self.cite {
            Cite::Compiled { links, .. } if links.embedded(anchor) => "embedded in",
            _ => "in",
        };
        let mut facts = match self.location(&Entity::Anchor(anchor)) {
            Some(location) => vec![format!("{kind} {place} `{location}`")],
            // With nowhere of its own, what it says is compiled into the
            // anchors that extend it, or that read or name it.
            None if def.is_abstract => {
                vec![format!("{kind}, compiled into the anchors that extend it")]
            }
            None => vec![format!("{kind} with no compiled document of its own")],
        };
        if def.keyword != "anchor" {
            facts.push(format!("declared as a `{}`", def.keyword));
        }
        if !def.bases.is_empty() {
            let bases: Vec<String> = def
                .bases
                .iter()
                .map(|base| self.name(&Entity::Anchor(*base)))
                .collect();
            facts.push(format!("extending {}", bases.join(", ")));
        }
        if let Some(alias) = &def.alias {
            facts.push(format!("defining the keyword `{alias}`"));
        }
        let mut sentence = format!("{}.", facts.join(", "));
        if !whole && properties.len() < def.slots.len() {
            sentence.push_str(" Only the properties this slice needs are shown.");
        }
        paragraph(&sentence, out);

        for name in properties {
            self.property(anchor, name, out);
        }
    }

    fn property(&self, anchor: AnchorId, name: &str, out: &mut String) {
        let store = self.compilation.store();
        let def = store.anchor(anchor);
        let Some(slot) = def.slots.get(name) else {
            return;
        };
        let entity = Entity::Property(anchor, name.to_string());
        out.push_str(&format!("### {}.{name}\n\n", def.name));

        let mut facts: Vec<String> = Vec::new();
        if !slot.constraints.is_empty() {
            facts.push(format!("Type: {}.", constraints(&slot.constraints)));
        }

        // A property the anchor only inherits says where from, and when the
        // declaration it inherits is already in the slice with the same value,
        // the value is not written out twice.
        let mut repeated = false;
        if slot.owner != anchor {
            let owner = store.anchor(slot.owner);
            let cited = self.name(&Entity::Anchor(slot.owner));
            let same = self
                .slice
                .contains(&Entity::Property(slot.owner, name.to_string()))
                && owner.properties.get(name) == def.properties.get(name);
            if same && slot.has_value {
                facts.push(format!("Inherited from {cited}, with the same value."));
                repeated = true;
            } else {
                facts.push(format!("Inherited from {cited}."));
            }
        } else {
            let overridden = self.targets(&entity, Relation::Inherits);
            if !overridden.is_empty() {
                facts.push(format!("Overrides {}.", overridden.join(", ")));
            }
        }

        let reads = self.targets(&entity, Relation::Reads);
        if !reads.is_empty() {
            facts.push(format!("Reads {}.", reads.join(", ")));
        }
        if !slot.has_value {
            facts.push("No value; an anchor that extends this one supplies it.".to_string());
        }
        paragraph(&facts.join(" "), out);

        if slot.has_value && !repeated {
            if let Some(value) = def.properties.get(name) {
                self.value(value, 3, out);
            }
        }
    }

    fn variable(&self, variable: VariableId, out: &mut String) {
        let def = self.compilation.store().variable(variable);
        let entity = Entity::Variable(variable);
        out.push_str(&format!("## {}\n\n", def.name));

        let mut facts = vec![match self.location(&entity) {
            Some(location) => format!("Variable in `{location}`."),
            None => "Variable, compiled into whatever reads it.".to_string(),
        }];
        if !def.constraints.is_empty() {
            facts.push(format!("Type: {}.", constraints(&def.constraints)));
        }
        let reads = self.targets(&entity, Relation::Reads);
        if !reads.is_empty() {
            facts.push(format!("Reads {}.", reads.join(", ")));
        }
        paragraph(&facts.join(" "), out);

        if let Some(value) = &def.value {
            self.value(value, 2, out);
        }
    }

    /// The entities `from` depends on through `relation`, as they are cited.
    fn targets(&self, from: &Entity, relation: Relation) -> Vec<String> {
        self.slice
            .edges_from(from)
            .filter(|edge| edge.relation == relation)
            .map(|edge| self.name(&edge.to))
            .collect()
    }

    fn value(&self, value: &Value, level: usize, out: &mut String) {
        let links: &dyn emit::LinkResolver = match self.cite {
            Cite::Source(_) => &emit::NoLinks,
            Cite::Compiled { links, .. } => links,
        };
        let context = emit::Context {
            anchors: self.compilation,
            links,
        };
        let rendered = emit::body(&by_name(value), level, &context);
        if !rendered.trim().is_empty() {
            out.push_str(&rendered);
            out.push('\n');
        }
    }
}

/// The anchors `value` embeds by value, `{...}`, at any depth.
fn embedded_in(value: &Value, out: &mut Vec<AnchorId>) {
    match value {
        Value::Anchor(anchor) => out.push(*anchor),
        Value::List(items) => items.iter().for_each(|item| embedded_in(item, out)),
        Value::Dict(map) => map.iter().for_each(|(_, value)| embedded_in(value, out)),
        Value::Mixed(mixed) => {
            for item in &mixed.items {
                match item {
                    MixedItem::List(items) => items.iter().for_each(|item| embedded_in(item, out)),
                    MixedItem::Entry(_, value) | MixedItem::Value(value) => embedded_in(value, out),
                    MixedItem::Text(_) => {}
                }
            }
        }
        _ => {}
    }
}

/// The reference that points at an anchor or a property.
fn reference(entity: &Entity) -> Ref {
    match entity {
        Entity::Anchor(anchor) => Ref::anchor(*anchor),
        Entity::Property(anchor, name) => Ref {
            anchor: *anchor,
            path: vec![name.clone()],
        },
        Entity::Variable(_) => unreachable!("a variable is never referenced"),
    }
}

fn constraints(constraints: &[piton_syntax::ast::TypeConstraint]) -> String {
    constraints
        .iter()
        .map(|constraint| format!("`{}`", constraint_label(constraint)))
        .collect::<Vec<_>>()
        .join(" or ")
}

/// `a`, `a and b`, or `a, b, and c`.
fn series(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

fn paragraph(text: &str, out: &mut String) {
    if text.is_empty() {
        return;
    }
    out.push_str(text);
    out.push_str("\n\n");
}

/// The value with every embedded anchor written as its name. The slice holds
/// each anchor it embeds as a declaration of its own, so copying it in again
/// would only say the same thing twice.
fn by_name(value: &Value) -> Value {
    match value {
        Value::Anchor(anchor) => Value::Reference(Ref::anchor(*anchor)),
        Value::List(items) => Value::List(items.iter().map(by_name).collect()),
        Value::Dict(map) => Value::Dict(
            map.iter()
                .map(|(key, value)| (key.clone(), by_name(value)))
                .collect(),
        ),
        Value::Mixed(mixed) => Value::Mixed(Mixed::new(
            mixed
                .items
                .iter()
                .map(|item| match item {
                    MixedItem::List(items) => MixedItem::List(items.iter().map(by_name).collect()),
                    MixedItem::Entry(key, value) => MixedItem::Entry(key.clone(), by_name(value)),
                    MixedItem::Value(value) => MixedItem::Value(by_name(value)),
                    MixedItem::Text(text) => MixedItem::Text(text.clone()),
                })
                .collect(),
        )),
        other => other.clone(),
    }
}
