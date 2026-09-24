//! Semantic slices: one anchor, property, or variable, and everything it takes
//! to understand it.
//!
//! A slice answers "what part of the specification does someone need in order
//! to implement this one thing?" It is not the files near the thing, or even
//! the files it imports: it is the declarations the thing semantically depends
//! on -- what it extends, what its values read, what they refer to or embed,
//! and what their types name -- followed transitively and each included once.
//!
//! It is built in three stages, so each can be reused on its own:
//!
//! 1. A [`Target`] names the starting point, like `ui.pi#SaveButton.color`,
//!    and [`resolve_in`] or [`find`] turns it into an [`Entity`].
//! 2. [`slice`] walks the dependency graph from that entity into a [`Slice`],
//!    which records what was selected and why, and says nothing about output.
//! 3. A renderer, like [`markdown::render`], writes the slice out.

pub mod markdown;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use piton_core::{AnchorId, Diagnostic, Label, MixedItem, Ref, Span, Value};
use piton_syntax::ast::{self, TypeConstraint, TypeName};

use crate::eval::Read;
use crate::module::ModuleId;
use crate::store::{Symbol, VariableId};
use crate::Compilation;

/// Something a slice can hold.
///
/// These are identities, not names: two anchors called `Button` in different
/// files are different entities, and the traversal deduplicates by them.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Entity {
    /// A whole anchor: every property it has, and every anchor it extends.
    Anchor(AnchorId),
    /// One property of an anchor, as that anchor sees it.
    Property(AnchorId, String),
    /// A top-level variable.
    Variable(VariableId),
}

impl Entity {
    /// The entity's name the way Piton writes it: `Button`, `Button.color`,
    /// or the variable's name.
    pub fn display(&self, compilation: &Compilation) -> String {
        let store = compilation.store();
        match self {
            Entity::Anchor(anchor) => store.anchor(*anchor).name.clone(),
            Entity::Property(anchor, name) => format!("{}.{name}", store.anchor(*anchor).name),
            Entity::Variable(variable) => store.variable(*variable).name.clone(),
        }
    }

    /// The module the entity is declared in.
    pub fn module(&self, compilation: &Compilation) -> ModuleId {
        let store = compilation.store();
        match self {
            Entity::Anchor(anchor) | Entity::Property(anchor, _) => store.anchor(*anchor).module,
            Entity::Variable(variable) => store.variable(*variable).module,
        }
    }
}

/// Why one entity is in a slice because of another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Relation {
    /// An anchor extends another, or a property is declared by a base too.
    Inherits,
    /// A type constraint names an anchor.
    Constrains,
    /// A value was computed from another value, like `${Document.name}`.
    Reads,
    /// A value holds a reference, `@{...}`.
    References,
    /// A value embeds a copy of an anchor, `{...}`.
    Composes,
}

impl Relation {
    pub fn as_str(self) -> &'static str {
        match self {
            Relation::Inherits => "inherits",
            Relation::Constrains => "constrains",
            Relation::Reads => "reads",
            Relation::References => "references",
            Relation::Composes => "composes",
        }
    }
}

/// One dependency the traversal followed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Edge {
    pub from: Entity,
    pub to: Entity,
    pub relation: Relation,
}

/// One declaration in a slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Anchor {
        anchor: AnchorId,
        /// True when the slice needs the whole anchor, not only some of its
        /// properties.
        whole: bool,
        /// The properties the slice needs, in the anchor's own order.
        properties: Vec<String>,
    },
    Variable(VariableId),
}

/// A target and the dependency closure it needs, independent of how it is
/// rendered.
#[derive(Debug, Clone)]
pub struct Slice {
    pub target: Entity,
    /// Every declaration in the slice, each once: the target's first, then
    /// the rest in the order the traversal reached them.
    pub entries: Vec<Entry>,
    /// Every dependency followed, in the order it was found. An anchor having
    /// its own properties is structure rather than a dependency, and is not
    /// listed.
    pub edges: Vec<Edge>,
    /// How the build reaches the target from the entry: each property that
    /// leads one step further down, outermost first. Empty when the target
    /// is one of the entry's exports, a variable, or not reached at all.
    pub chain: Vec<Entity>,
}

impl Slice {
    /// The dependencies `from` has in this slice.
    pub fn edges_from<'a>(&'a self, from: &'a Entity) -> impl Iterator<Item = &'a Edge> + 'a {
        self.edges.iter().filter(move |edge| edge.from == *from)
    }

    /// Whether the slice holds `entity`, directly or as part of a whole anchor.
    pub fn contains(&self, entity: &Entity) -> bool {
        self.entries.iter().any(|entry| match (entry, entity) {
            (Entry::Anchor { anchor, whole, .. }, Entity::Anchor(other)) => {
                anchor == other && *whole
            }
            (
                Entry::Anchor {
                    anchor, properties, ..
                },
                Entity::Property(other, name),
            ) => anchor == other && properties.contains(name),
            (Entry::Variable(variable), Entity::Variable(other)) => variable == other,
            _ => false,
        })
    }
}

// ---------------------------------------------------------------------------
// Targets
// ---------------------------------------------------------------------------

/// What a slice starts from, as someone writes it: `ui.pi#SaveButton`,
/// `ui.pi#SaveButton.color`, or a bare `SaveButton` to look up across the
/// project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// The file the name is looked up in, when one is given.
    pub file: Option<PathBuf>,
    /// The anchor or variable.
    pub name: String,
    /// The property path after the name; empty for the whole declaration.
    pub path: Vec<String>,
}

impl Target {
    pub fn parse(text: &str) -> Result<Target, String> {
        let (file, rest) = match text.rsplit_once('#') {
            Some(("", _)) => return Err(format!("`{text}` has no file before `#`")),
            Some((file, rest)) => (Some(PathBuf::from(file)), rest),
            None if text.ends_with(".pi") => {
                return Err(format!(
                    "`{text}` names a file but nothing in it; write `{text}#Name`"
                ))
            }
            None => (None, text),
        };
        let mut parts = rest.split('.');
        let name = parts.next().unwrap_or_default().to_string();
        let path: Vec<String> = parts.map(str::to_string).collect();
        if name.is_empty() || path.iter().any(String::is_empty) {
            return Err(format!(
                "`{rest}` is not a name to slice; write `Name` or `Name.property`"
            ));
        }
        Ok(Target { file, name, path })
    }

    /// `Name` or `Name.property`, without the file.
    pub fn display(&self) -> String {
        let mut out = self.name.clone();
        for part in &self.path {
            out.push('.');
            out.push_str(part);
        }
        out
    }
}

/// Resolves a target the way `module` sees its name: declared there or
/// imported into it.
pub fn resolve_in(
    compilation: &Compilation,
    module: ModuleId,
    target: &Target,
) -> Result<Entity, Diagnostic> {
    let resolution = &compilation.resolution;
    match resolution.lookup(module, &target.name) {
        Some(symbol) => entity_for(compilation, symbol, target),
        None => {
            let path = &compilation.graph().get(module).path;
            let names: Vec<String> = resolution
                .visible_names(module)
                .into_iter()
                .map(|(name, _)| name)
                .collect();
            let mut diagnostic = Diagnostic::error(
                "unresolved-symbol",
                format!(
                    "`{}` is not declared in or imported into this file",
                    target.name
                ),
                path,
                Span::default(),
            );
            if let Some(near) = crate::resolve::closest(&target.name, &names) {
                diagnostic = diagnostic.with_help(format!("did you mean `{near}`?"));
            }
            Err(diagnostic)
        }
    }
}

/// Resolves a target with no file by looking its name up across the whole
/// compilation. A name declared in more than one file is ambiguous and has to
/// be given with its file.
///
/// Bundled packages are only searched when nothing in the project matches, so
/// a project's own `Button` is never ambiguous with a package's.
pub fn find(compilation: &Compilation, target: &Target) -> Result<Entity, Diagnostic> {
    let store = compilation.store();
    let graph = compilation.graph();
    let declared = |packaged: bool| -> Vec<(Symbol, PathBuf, Span)> {
        let anchors = store
            .anchors
            .iter()
            .filter(|def| def.name == target.name)
            .map(|def| (Symbol::Anchor(def.id), def.module, def.name_span));
        let variables = store
            .variables
            .iter()
            .filter(|def| def.name == target.name)
            .map(|def| (Symbol::Variable(def.id), def.module, def.name_span));
        let mut out: Vec<(Symbol, PathBuf, Span)> = anchors
            .chain(variables)
            .filter(|(_, module, _)| graph.get(*module).is_package() == packaged)
            .map(|(symbol, module, span)| (symbol, graph.get(module).path.clone(), span))
            .collect();
        out.sort_by(|a, b| (&a.1, a.2.start).cmp(&(&b.1, b.2.start)));
        out
    };

    let mut candidates = declared(false);
    if candidates.is_empty() {
        candidates = declared(true);
    }

    match candidates.as_slice() {
        [] => {
            let names: Vec<String> = store
                .anchors
                .iter()
                .map(|def| def.name.clone())
                .chain(store.variables.iter().map(|def| def.name.clone()))
                .collect();
            let mut diagnostic = Diagnostic::error(
                "unresolved-symbol",
                format!(
                    "nothing named `{}` is declared in this project",
                    target.name
                ),
                &compilation.project.entry,
                Span::default(),
            );
            if let Some(near) = crate::resolve::closest(&target.name, &names) {
                diagnostic = diagnostic.with_help(format!("did you mean `{near}`?"));
            }
            Err(diagnostic)
        }
        [(symbol, _, _)] => entity_for(compilation, *symbol, target),
        [(_, first, span), rest @ ..] => {
            let mut diagnostic = Diagnostic::error(
                "ambiguous-target",
                format!("`{}` is declared in more than one file", target.name),
                first,
                *span,
            )
            .with_help(format!(
                "name the file it is in, like `{}#{}`",
                display_path(first, &compilation.project.root),
                target.display()
            ));
            for (_, path, span) in rest {
                diagnostic = diagnostic.with_label(Label::new(
                    path.clone(),
                    *span,
                    format!("`{}` is also declared here", target.name),
                ));
            }
            Err(diagnostic)
        }
    }
}

/// The entity a resolved name and property path point at.
fn entity_for(
    compilation: &Compilation,
    symbol: Symbol,
    target: &Target,
) -> Result<Entity, Diagnostic> {
    let store = compilation.store();
    match symbol {
        Symbol::Variable(variable) => {
            if target.path.is_empty() {
                return Ok(Entity::Variable(variable));
            }
            let def = store.variable(variable);
            Err(Diagnostic::error(
                "unsliceable-target",
                format!(
                    "`{}` is a variable, and a slice starts at a whole variable or an anchor's property",
                    def.name
                ),
                &compilation.graph().get(def.module).path,
                def.name_span,
            )
            .with_help(format!("slice `{}` instead", def.name)))
        }
        Symbol::Anchor(anchor) => {
            let def = store.anchor(anchor);
            let path = &compilation.graph().get(def.module).path;
            let Some(first) = target.path.first() else {
                return Ok(Entity::Anchor(anchor));
            };
            if !def.slots.contains_key(first) {
                let names: Vec<String> = def.slots.keys().cloned().collect();
                let help = match crate::resolve::closest(first, &names) {
                    Some(near) => format!("did you mean `{near}`?"),
                    None if names.is_empty() => format!("`{}` has no properties", def.name),
                    None => format!("its properties are {}", quoted_list(&names)),
                };
                return Err(Diagnostic::error(
                    "unknown-property",
                    format!("`{}` has no property `{first}`", def.name),
                    path,
                    def.name_span,
                )
                .with_help(help));
            }
            if target.path.len() > 1 {
                return Err(Diagnostic::error(
                    "unsliceable-target",
                    format!(
                        "`{}` reaches inside `{}.{first}`, and a slice starts at a whole property",
                        target.display(),
                        def.name
                    ),
                    path,
                    def.name_span,
                )
                .with_help(format!("slice `{}.{first}` instead", def.name)));
            }
            Ok(Entity::Property(anchor, first.clone()))
        }
    }
}

fn quoted_list(names: &[String]) -> String {
    names
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn display_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

// ---------------------------------------------------------------------------
// Traversal
// ---------------------------------------------------------------------------

/// Collects `target` and everything it depends on, transitively, and the
/// chain the entry reaches it through.
///
/// The walk is breadth-first, so what the target depends on directly comes
/// before what that depends on in turn, and each entity's dependencies are
/// visited in a fixed order: what it inherits, the types it is constrained
/// to, what it reads, then what its value refers to or embeds, each in the
/// order it is written. The same source always produces the same slice. Entities
/// are tracked by identity, so a cycle ends the first time it comes back
/// around, and the edge that closed it is still recorded.
///
/// The chain comes after the dependencies. Each link is only that one
/// property: what else it refers to is context the target does not need, and
/// following it from the entry would bring in everything.
pub fn slice(compilation: &Compilation, target: Entity) -> Slice {
    let walker = Walker { compilation };
    let mut seen: HashSet<Entity> = HashSet::new();
    let mut queue: VecDeque<Entity> = VecDeque::new();
    let mut order: Vec<Entity> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();
    let mut recorded: HashSet<Edge> = HashSet::new();

    seen.insert(target.clone());
    queue.push_back(target.clone());
    while let Some(entity) = queue.pop_front() {
        for (next, relation) in walker.dependencies(&entity) {
            if let Some(relation) = relation {
                let edge = Edge {
                    from: entity.clone(),
                    to: next.clone(),
                    relation,
                };
                if recorded.insert(edge.clone()) {
                    edges.push(edge);
                }
            }
            if seen.insert(next.clone()) {
                queue.push_back(next);
            }
        }
        order.push(entity);
    }

    let chain = match &target {
        Entity::Anchor(anchor) | Entity::Property(anchor, _) => chain(compilation, *anchor),
        Entity::Variable(_) => Vec::new(),
    };
    for link in &chain {
        if !seen.contains(link) {
            order.push(link.clone());
        }
    }

    Slice {
        target,
        entries: walker.entries(&order),
        edges,
        chain,
    }
}

/// The shortest way the build gets from the entry to `anchor`, as the
/// properties it passes through: `Specification.Tooling`, `Tooling.cli`,
/// `Cli.commands` for a command the CLI lists.
///
/// The walk starts at the entry's exports, in order, and follows what each
/// property's value refers to or embeds, in the order it is written, so the
/// same source always gives the same chain.
pub fn chain(compilation: &Compilation, anchor: AnchorId) -> Vec<Entity> {
    let store = compilation.store();
    let roots = compilation.entry_exports();
    if roots.contains(&anchor) {
        return Vec::new();
    }
    // The property each anchor was first reached through.
    let mut via: HashMap<AnchorId, (AnchorId, String)> = HashMap::new();
    let mut queue: VecDeque<AnchorId> = roots.iter().copied().collect();
    let mut seen: HashSet<AnchorId> = roots.iter().copied().collect();
    'walk: while let Some(current) = queue.pop_front() {
        for (name, value) in store.anchor(current).properties.iter() {
            let mut targets = Vec::new();
            crate::reach::collect_targets(value, &mut targets);
            for (next, _) in targets {
                if seen.insert(next) {
                    via.insert(next, (current, name.clone()));
                    if next == anchor {
                        break 'walk;
                    }
                    queue.push_back(next);
                }
            }
        }
    }

    let mut out = Vec::new();
    let mut at = anchor;
    while let Some((holder, name)) = via.get(&at) {
        out.push(Entity::Property(*holder, name.clone()));
        at = *holder;
    }
    out.reverse();
    out
}

struct Walker<'a> {
    compilation: &'a Compilation,
}

impl Walker<'_> {
    /// What an entity needs, each with the relation that makes it a
    /// dependency. `None` marks structure: the properties of a whole anchor.
    fn dependencies(&self, entity: &Entity) -> Vec<(Entity, Option<Relation>)> {
        let store = self.compilation.store();
        let mut out: Vec<(Entity, Option<Relation>)> = Vec::new();
        match entity {
            Entity::Anchor(anchor) => {
                let def = store.anchor(*anchor);
                for name in def.slots.keys() {
                    out.push((Entity::Property(*anchor, name.clone()), None));
                }
                for base in &def.bases {
                    out.push((Entity::Anchor(*base), Some(Relation::Inherits)));
                }
            }
            Entity::Property(anchor, name) => {
                let def = store.anchor(*anchor);
                if !def.slots.contains_key(name) {
                    return out;
                }
                // Every base that declares the property too, winner first:
                // that is where an inherited value or constraint comes from.
                for base in store.base_chain(*anchor).into_iter().rev() {
                    if base != *anchor && self.declaration(base, name).is_some() {
                        out.push((
                            Entity::Property(base, name.clone()),
                            Some(Relation::Inherits),
                        ));
                    }
                }
                if let Some(property) = self.declaration(*anchor, name) {
                    self.constraint_types(&property.constraints, def.module, &mut out);
                }
                self.reads(Read::Property(*anchor, name.clone()), &mut out);
                if let Some(value) = def.properties.get(name) {
                    self.value_targets(value, &mut out);
                }
            }
            Entity::Variable(variable) => {
                let def = store.variable(*variable);
                self.constraint_types(&def.constraints, def.module, &mut out);
                self.reads(Read::Variable(*variable), &mut out);
                if let Some(value) = &def.value {
                    self.value_targets(value, &mut out);
                }
            }
        }
        out
    }

    /// The property as `anchor`'s own body declares it, if it does.
    fn declaration(&self, anchor: AnchorId, name: &str) -> Option<&ast::Property> {
        let def = self.compilation.store().anchor(anchor);
        let ast::Item::Anchor(decl) = self
            .compilation
            .graph()
            .get(def.module)
            .ast()
            .items
            .get(def.item)?
        else {
            return None;
        };
        decl.body
            .properties()
            .find(|property| property.name == name)
    }

    /// Anchors named as types, looked up where the constraint is written.
    fn constraint_types(
        &self,
        constraints: &[TypeConstraint],
        module: ModuleId,
        out: &mut Vec<(Entity, Option<Relation>)>,
    ) {
        for constraint in constraints {
            if let TypeName::Named(name) = &constraint.name {
                if let Some(Symbol::Anchor(anchor)) =
                    self.compilation.resolution.lookup(module, name)
                {
                    out.push((Entity::Anchor(anchor), Some(Relation::Constrains)));
                }
            }
        }
    }

    /// What evaluating `read` read, in the order the source reads it.
    /// Evaluation is deterministic, so that order is too.
    fn reads(&self, read: Read, out: &mut Vec<(Entity, Option<Relation>)>) {
        let Some(reads) = self.compilation.reads.get(&read) else {
            return;
        };
        for read in reads {
            let entity = match read {
                Read::Property(anchor, name) => Entity::Property(*anchor, name.clone()),
                Read::Variable(variable) => Entity::Variable(*variable),
            };
            out.push((entity, Some(Relation::Reads)));
        }
    }

    /// The anchors and properties a value embeds or refers to, in the order
    /// they appear.
    fn value_targets(&self, value: &Value, out: &mut Vec<(Entity, Option<Relation>)>) {
        match value {
            Value::Anchor(anchor) => out.push((Entity::Anchor(*anchor), Some(Relation::Composes))),
            Value::Reference(target) => {
                out.push((self.referenced(target), Some(Relation::References)))
            }
            Value::Str(text) => {
                for target in text.refs() {
                    out.push((self.referenced(target), Some(Relation::References)));
                }
            }
            Value::List(items) => {
                for item in items {
                    self.value_targets(item, out);
                }
            }
            Value::Dict(map) => {
                for item in map.values() {
                    self.value_targets(item, out);
                }
            }
            Value::Mixed(mixed) => {
                for item in &mixed.items {
                    match item {
                        MixedItem::Text(text) => {
                            for target in text.refs() {
                                out.push((self.referenced(target), Some(Relation::References)));
                            }
                        }
                        MixedItem::List(items) => {
                            for item in items {
                                self.value_targets(item, out);
                            }
                        }
                        MixedItem::Entry(_, value) | MixedItem::Value(value) => {
                            self.value_targets(value, out)
                        }
                    }
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }

    /// A reference to a property needs that property; a reference to an
    /// anchor needs all of it. A path that goes deeper than one property still
    /// needs only the property it starts at.
    fn referenced(&self, target: &Ref) -> Entity {
        match target.path.first() {
            Some(name)
                if self
                    .compilation
                    .store()
                    .anchor(target.anchor)
                    .slots
                    .contains_key(name) =>
            {
                Entity::Property(target.anchor, name.clone())
            }
            _ => Entity::Anchor(target.anchor),
        }
    }

    /// Groups the visited entities into declarations, in the order each
    /// declaration was first reached.
    fn entries(&self, order: &[Entity]) -> Vec<Entry> {
        let store = self.compilation.store();
        let mut entries: Vec<Entry> = Vec::new();
        let mut anchors: HashMap<AnchorId, usize> = HashMap::new();
        let mut entry_for = |entries: &mut Vec<Entry>, anchor: AnchorId| -> usize {
            *anchors.entry(anchor).or_insert_with(|| {
                entries.push(Entry::Anchor {
                    anchor,
                    whole: false,
                    properties: Vec::new(),
                });
                entries.len() - 1
            })
        };

        for entity in order {
            match entity {
                Entity::Anchor(anchor) => {
                    let index = entry_for(&mut entries, *anchor);
                    if let Entry::Anchor { whole, .. } = &mut entries[index] {
                        *whole = true;
                    }
                }
                Entity::Property(anchor, name) => {
                    let index = entry_for(&mut entries, *anchor);
                    if let Entry::Anchor { properties, .. } = &mut entries[index] {
                        if !properties.contains(name) {
                            properties.push(name.clone());
                        }
                    }
                }
                Entity::Variable(variable) => entries.push(Entry::Variable(*variable)),
            }
        }

        for entry in &mut entries {
            if let Entry::Anchor {
                anchor,
                whole,
                properties,
            } = entry
            {
                let slots = &store.anchor(*anchor).slots;
                if *whole {
                    *properties = slots.keys().cloned().collect();
                } else {
                    properties.sort_by_key(|name| slots.get_index_of(name));
                }
            }
        }
        entries
    }
}
