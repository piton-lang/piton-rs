//! Semantic slices. A slice is one anchor, property, or variable, with all
//! that a reader needs to understand it.
//!
//! A slice answers this question. Which part of the specification does a
//! person need for this one item? A slice is not the files near the
//! item. It is also not the files that the item imports. It is the
//! declarations that the item semantically depends on. These are the anchors
//! that it extends and the declarations that its values read. They are also
//! the targets of references and embeds, and the names in its types. The
//! slice follows these transitively and includes each one one time.
//!
//! The code makes a slice in three stages. You can use each stage alone:
//!
//! 1. A [`Target`] names the start point, like `ui.pi#SaveButton.color`.
//!    [`resolve_in`] or [`find`] changes it into an [`Entity`].
//! 2. [`slice`] walks the dependency graph from that entity into a [`Slice`].
//!    The [`Slice`] records the selected items and the reasons. It does not
//!    know about output.
//! 3. A renderer, like [`markdown::render`], writes the slice.

pub mod markdown;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use piton_core::{AnchorId, Diagnostic, Label, MixedItem, Ref, Span, Value};
use piton_syntax::ast::{self, TypeConstraint, TypeName};

use crate::eval::Read;
use crate::module::ModuleId;
use crate::store::{Symbol, VariableId};
use crate::Compilation;

/// An item that a slice can hold.
///
/// These are identities, not names. Two anchors with the name `Button` in
/// different files are different entities. The traversal removes duplicates
/// by identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Entity {
    /// A whole anchor. This includes all its properties and all the anchors
    /// that it extends.
    Anchor(AnchorId),
    /// One property of an anchor, as that anchor has it.
    Property(AnchorId, String),
    /// A top-level variable.
    Variable(VariableId),
}

impl Entity {
    /// The name of the entity as Piton writes it. Examples are `Button`,
    /// `Button.color`, or the name of the variable.
    pub fn display(&self, compilation: &Compilation) -> String {
        let store = compilation.store();
        match self {
            Entity::Anchor(anchor) => store.anchor(*anchor).name.clone(),
            Entity::Property(anchor, name) => format!("{}.{name}", store.anchor(*anchor).name),
            Entity::Variable(variable) => store.variable(*variable).name.clone(),
        }
    }

    /// The module that declares the entity.
    pub fn module(&self, compilation: &Compilation) -> ModuleId {
        let store = compilation.store();
        match self {
            Entity::Anchor(anchor) | Entity::Property(anchor, _) => store.anchor(*anchor).module,
            Entity::Variable(variable) => store.variable(*variable).module,
        }
    }
}

/// The reason that one entity causes another entity to be in a slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Relation {
    /// An anchor extends another anchor, or a base also declares a property.
    Inherits,
    /// A type constraint names an anchor.
    Constrains,
    /// A value comes from another value, like `${Document.name}`.
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

/// One dependency that the traversal followed.
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
        /// True when the slice needs the whole anchor, and not only some of
        /// its properties.
        whole: bool,
        /// The properties that the slice needs, in the order of the anchor.
        properties: Vec<String>,
    },
    Variable(VariableId),
}

/// A target and the dependency closure that it needs. It does not depend on
/// the output format.
#[derive(Debug, Clone)]
pub struct Slice {
    pub target: Entity,
    /// All declarations in the slice, each one time. The target declaration
    /// is first. The others follow in the order that the traversal found them.
    pub entries: Vec<Entry>,
    /// All dependencies that the traversal followed, in the order it found
    /// them. The link from an anchor to its own properties is structure. It
    /// is not a dependency, thus this list does not include it.
    pub edges: Vec<Edge>,
    /// The path from the entry to the target in the build. Each property
    /// goes one step lower. The outermost property is first. The list is
    /// empty when the target is an export of the entry or a variable. It is
    /// also empty when the build does not reach the target.
    pub chain: Vec<Entity>,
}

impl Slice {
    /// The dependencies of `from` in this slice.
    pub fn edges_from<'a>(&'a self, from: &'a Entity) -> impl Iterator<Item = &'a Edge> + 'a {
        self.edges.iter().filter(move |edge| edge.from == *from)
    }

    /// Tells if the slice holds `entity`, directly or in a whole anchor.
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

/// The start point of a slice, as a person writes it. Examples are
/// `ui.pi#SaveButton` and `ui.pi#SaveButton.color`. A bare `SaveButton`
/// causes a search of the full project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// The file in which to find the name, if the target gives one.
    pub file: Option<PathBuf>,
    /// The anchor or variable.
    pub name: String,
    /// The property path after the name. It is empty for the whole
    /// declaration.
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

/// Resolves a target as `module` sees its name. `module` declares the
/// name or imports it.
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

/// Resolves a target without a file. It finds the name in the full
/// compilation. A name in more than one file is ambiguous. The target must
/// then give the file.
///
/// The search includes bundled packages only when no project name matches.
/// Thus a `Button` in the project is never ambiguous with a `Button` in a
/// package.
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

/// The entity that a resolved name and property path identify.
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

/// Collects `target` and all its transitive dependencies. It also collects
/// the chain from the entry to the target.
///
/// The walk is breadth-first. Thus the direct dependencies of the target come
/// before their own dependencies. The walk visits the dependencies of each
/// entity in a fixed order. First come the inherited anchors. Then come the
/// types of its constraints. Then come the items that it reads. Last come the
/// targets of references and embeds in its value. Each group keeps the
/// written order. The same source always gives the same slice. The walk
/// tracks entities by identity. Thus a cycle stops when it returns to an
/// entity. The slice still records the edge that closed the cycle.
///
/// The chain comes after the dependencies. Each link is only that one
/// property. Its other references are context that the target does not need.
/// If the walk follows them from the entry, it includes all items.
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

/// The shortest path in the build from the entry to `anchor`. The path is a
/// list of properties. For a command in the CLI list, the path is
/// `Specification.Tooling`, `Tooling.cli`, `Cli.commands`.
///
/// The walk starts at the exports of the entry, in order. It follows the
/// references and embeds in the value of each property, in the written
/// order. Thus the same source always gives the same chain.
pub fn chain(compilation: &Compilation, anchor: AnchorId) -> Vec<Entity> {
    let store = compilation.store();
    let roots = compilation.entry_exports();
    if roots.contains(&anchor) {
        return Vec::new();
    }
    // The property through which the walk first found each anchor.
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
    /// The needs of an entity. Each one has the relation that makes it a
    /// dependency. `None` marks structure, which is the properties of a
    /// whole anchor.
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
                // All bases that also declare the property, the winner first.
                // An inherited value or constraint comes from these bases.
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

    /// The property as the body of `anchor` declares it, if it does.
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

    /// Anchors that types name. The search starts in the module that
    /// writes the constraint.
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

    /// The items that the evaluation of `read` read, in the source order.
    /// Evaluation is deterministic. Thus that order is also deterministic.
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

    /// The anchors and properties that a value embeds or refers to, in the
    /// order that they occur.
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

    /// A reference to a property needs that property. A reference to an
    /// anchor needs the full anchor. A path that goes below one property
    /// needs only its first property.
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

    /// Puts the visited entities into declarations. The order is the order
    /// in which the walk first found each declaration.
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
