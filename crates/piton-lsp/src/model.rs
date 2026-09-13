//! The compiler's rules for scopes, inheritance, and members, asked about one
//! project at a time.
//!
//! Every answer here is what `piton-core` does when it compiles, worked out in
//! the same order, so the symbol model built on it cannot disagree with a build.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use piton_core::hir::{Expr, Node, Property, TypeExpr};
use piton_core::resolve::{Analysis, Symbol};
use piton_core::value::AnchorId;
use piton_core::FileId;

use crate::index::{Access, AliasItem, Sym};
use crate::world::View;

/// How far a chain of references is followed before giving up, so that a value
/// defined in terms of itself cannot loop.
const MAX_DEPTH: usize = 32;

// ---- questions the compiler answers ------------------------------------------------

/// A property as the compiler resolves it on one anchor.
#[derive(Clone, Copy, Debug)]
pub struct Slot<'a> {
    /// The anchor whose body wrote the declaration that is used.
    pub owner: AnchorId,
    pub property: &'a Property,
    /// The effective constraint chain, with the anchor that wrote it.
    pub constraints: Option<(AnchorId, &'a [TypeExpr])>,
}

/// Something whose members a `.` can reach.
#[derive(Clone, Debug)]
pub enum Holder<'a> {
    Anchor(AnchorId),
    Super(AnchorId),
    Dictionary { owner: Sym, file: FileId, anchor: Option<AnchorId>, node: &'a Node },
}

/// A member a `.` reached, and what its value is written as.
#[derive(Clone, Debug)]
pub struct Member<'a> {
    pub sym: Sym,
    pub access: Access,
    pub property: &'a Property,
    /// The file and anchor the value is written in.
    pub file: FileId,
    pub anchor: Option<AnchorId>,
}

/// The compiler's rules for scopes, inheritance, and members, asked about one
/// project. It borrows the compilation and caches what it works out, so it is
/// made for a question and dropped afterwards.
pub struct Model<'a> {
    pub view: &'a View,
    bases: HashMap<AnchorId, Vec<AnchorId>>,
    subtypes: HashMap<AnchorId, Vec<AnchorId>>,
    own: RefCell<HashMap<AnchorId, Rc<Vec<&'a Property>>>>,
    visible: RefCell<HashMap<AnchorId, Rc<HashSet<String>>>>,
    families: RefCell<HashMap<(AnchorId, String), AnchorId>>,
    members: RefCell<HashMap<(AnchorId, String), Rc<Vec<AnchorId>>>>,
    origins: RefCell<HashMap<(FileId, String), Option<Sym>>>,
    resolving: RefCell<HashSet<(FileId, String)>>,
}

impl<'a> Model<'a> {
    pub fn new(view: &'a View) -> Model<'a> {
        let analysis = &view.compilation.analysis;
        let mut bases = HashMap::new();
        let mut subtypes: HashMap<AnchorId, Vec<AnchorId>> = HashMap::new();
        for id in analysis.anchor_ids() {
            let direct = analysis.bases(id);
            for base in &direct {
                subtypes.entry(*base).or_default().push(id);
            }
            bases.insert(id, direct);
        }
        Model {
            view,
            bases,
            subtypes,
            own: RefCell::default(),
            visible: RefCell::default(),
            families: RefCell::default(),
            members: RefCell::default(),
            origins: RefCell::default(),
            resolving: RefCell::default(),
        }
    }

    pub fn analysis(&self) -> &'a Analysis {
        &self.view.compilation.analysis
    }

    pub fn file_of(&self, id: AnchorId) -> FileId {
        self.analysis().anchor_loc(id).file
    }

    pub fn bases(&self, id: AnchorId) -> &[AnchorId] {
        self.bases.get(&id).map(Vec::as_slice).unwrap_or_default()
    }

    /// The properties an anchor writes in its own body, in order.
    pub fn own(&self, id: AnchorId) -> Rc<Vec<&'a Property>> {
        if let Some(found) = self.own.borrow().get(&id) {
            return found.clone();
        }
        let mut out = Vec::new();
        collect(&self.analysis().anchor_def(id).body, &mut out);
        let out = Rc::new(out);
        self.own.borrow_mut().insert(id, out.clone());
        out
    }

    /// The names of every property visible on an anchor.
    pub fn visible(&self, id: AnchorId) -> Rc<HashSet<String>> {
        if let Some(found) = self.visible.borrow().get(&id) {
            return found.clone();
        }
        let mut names: HashSet<String> = self.own(id).iter().map(|it| it.name.clone()).collect();
        for ancestor in self.analysis().ancestors(id) {
            names.extend(self.own(ancestor).iter().map(|it| it.name.clone()));
        }
        let names = Rc::new(names);
        self.visible.borrow_mut().insert(id, names.clone());
        names
    }

    pub fn is_visible(&self, id: AnchorId, name: &str) -> bool {
        self.visible(id).contains(name)
    }

    /// The representative of the family `name` belongs to on `id`: the lowest
    /// anchor id among the anchors that share the property.
    pub fn family(&self, id: AnchorId, name: &str) -> AnchorId {
        let key = (id, name.to_string());
        if let Some(found) = self.families.borrow().get(&key) {
            return *found;
        }
        let mut members = vec![id];
        let mut seen: HashSet<AnchorId> = HashSet::from([id]);
        let mut at = 0;
        while at < members.len() {
            let current = members[at];
            at += 1;
            let joining: Vec<AnchorId> = self
                .bases(current)
                .iter()
                .copied()
                .filter(|base| self.is_visible(*base, name))
                .chain(self.subtypes.get(&current).into_iter().flatten().copied())
                .collect();
            for anchor in joining {
                if seen.insert(anchor) {
                    members.push(anchor);
                }
            }
        }
        let representative = members.iter().copied().min_by_key(|it| it.0).unwrap_or(id);
        {
            let mut families = self.families.borrow_mut();
            for member in &members {
                families.insert((*member, name.to_string()), representative);
            }
        }
        self.members.borrow_mut().insert((representative, name.to_string()), Rc::new(members));
        representative
    }

    /// Every anchor in a family.
    pub fn family_members(&self, family: AnchorId, name: &str) -> Rc<Vec<AnchorId>> {
        self.family(family, name);
        self.members.borrow().get(&(family, name.to_string())).cloned().unwrap_or_default()
    }

    /// The declaration and constraint the compiler uses for `name` on `id`.
    pub fn slot(&self, id: AnchorId, name: &str) -> Option<Slot<'a>> {
        self.slot_guarded(id, name, &mut Vec::new())
    }

    /// What the bases of `id` provide for `name`, which is what `super` reads.
    pub fn base_slot(&self, id: AnchorId, name: &str) -> Option<Slot<'a>> {
        self.base_slot_guarded(id, name, &mut vec![id])
    }

    fn slot_guarded(&self, id: AnchorId, name: &str, visiting: &mut Vec<AnchorId>) -> Option<Slot<'a>> {
        // An inheritance cycle is reported by the compiler; here it only has to
        // stop, which is what the compiler's placeholder map does too.
        if visiting.contains(&id) {
            return None;
        }
        visiting.push(id);
        let mut current = self.base_slot_guarded(id, name, visiting);
        for property in self.own(id).iter().filter(|it| it.name == name) {
            let constraints = match property.constraints.is_empty() {
                true => current.and_then(|it| it.constraints),
                false => Some((id, property.constraints.as_slice())),
            };
            current = Some(Slot { owner: id, property, constraints });
        }
        visiting.pop();
        current
    }

    fn base_slot_guarded(&self, id: AnchorId, name: &str, visiting: &mut Vec<AnchorId>) -> Option<Slot<'a>> {
        let mut current: Option<Slot<'a>> = None;
        for base in self.bases(id).to_vec() {
            if let Some(slot) = self.slot_guarded(base, name, visiting) {
                let constraints = slot.constraints.or(current.and_then(|it| it.constraints));
                current = Some(Slot { constraints, ..slot });
            }
        }
        current
    }

    /// What a bare name means in `file`.
    ///
    /// The file's anchors, then its variables, then its imports, each replacing
    /// what came before under the same spelling, which is the order the
    /// compiler builds the scope in. A framework's builtin value wins over all
    /// of them, and then only an import of that value from a framework module
    /// is a symbol.
    pub fn binding(&self, file: FileId, name: &str) -> Option<Sym> {
        let analysis = self.analysis();
        if !analysis.scope(file).names.contains_key(name) {
            return None;
        }
        let hir = &analysis.db.file(file).hir;
        let mut bound = None;
        for (position, def) in hir.anchors.iter().enumerate() {
            if def.name == name {
                bound = analysis.anchor_id(file, position).map(Sym::Anchor);
            }
        }
        for (index, def) in hir.vars.iter().enumerate() {
            if def.name == name {
                bound = Some(Sym::Var { file, index });
            }
        }
        for (decl, import) in hir.imports.iter().enumerate() {
            let Some(module) = analysis.module(file, &import.path.value) else { continue };
            for (item, entry) in import.items.iter().enumerate() {
                if entry.local() != name
                    || !analysis.scope(module).exports.contains_key(&entry.name.value)
                {
                    continue;
                }
                bound = match entry.alias {
                    Some(_) => Some(Sym::Alias { file, item: AliasItem::Import { decl, item } }),
                    None => self.origin(module, &entry.name.value),
                };
            }
        }
        if self.view.is_builtin_value(name) {
            return bound.filter(|sym| self.declaring_file(sym).is_some_and(|it| self.view.is_virtual(it)));
        }
        bound
    }

    /// What `module` exports under `name`, followed back to what declared it.
    pub fn origin(&self, module: FileId, name: &str) -> Option<Sym> {
        let key = (module, name.to_string());
        if let Some(found) = self.origins.borrow().get(&key) {
            return found.clone();
        }
        if !self.resolving.borrow_mut().insert(key.clone()) {
            return None;
        }
        let found = self.compute_origin(module, name);
        self.resolving.borrow_mut().remove(&key);
        self.origins.borrow_mut().insert(key, found.clone());
        found
    }

    /// The compiler's export order: what the module declares and exports, then
    /// `export Name`, then each re-export, each replacing what came before.
    fn compute_origin(&self, module: FileId, name: &str) -> Option<Sym> {
        let analysis = self.analysis();
        let scope = analysis.scope(module);
        if !scope.exports.contains_key(name) {
            return None;
        }
        let hir = &analysis.db.file(module).hir;
        let mut found = None;
        let declared_here = match scope.names.get(name) {
            Some(Symbol::Anchor(id)) => {
                analysis.anchor_loc(*id).file == module && analysis.anchor_def(*id).exported
            }
            Some(Symbol::Var { file, index }) => {
                *file == module && analysis.db.file(*file).hir.vars[*index].exported
            }
            None => false,
        };
        if declared_here {
            found = self.binding(module, name);
        }
        if hir.exports.iter().any(|it| it.value == name) && scope.names.contains_key(name) {
            found = self.binding(module, name);
        }
        for (decl, reexport) in hir.reexports.iter().enumerate() {
            let Some(target) = analysis.module(module, &reexport.path.value) else { continue };
            let exported = &analysis.scope(target).exports;
            if reexport.glob {
                if exported.contains_key(name) {
                    found = self.origin(target, name);
                }
                continue;
            }
            for (item, entry) in reexport.items.iter().enumerate() {
                if entry.local() != name || !exported.contains_key(&entry.name.value) {
                    continue;
                }
                found = match entry.alias {
                    Some(_) => Some(Sym::Alias { file: module, item: AliasItem::Reexport { decl, item } }),
                    None => self.origin(target, &entry.name.value),
                };
            }
        }
        found
    }

    /// The symbol an alias names.
    pub fn alias_target(&self, file: FileId, item: AliasItem) -> Option<Sym> {
        let analysis = self.analysis();
        let hir = &analysis.db.file(file).hir;
        let (path, entry) = match item {
            AliasItem::Import { decl, item } => {
                let import = hir.imports.get(decl)?;
                (&import.path.value, import.items.get(item)?)
            }
            AliasItem::Reexport { decl, item } => {
                let reexport = hir.reexports.get(decl)?;
                (&reexport.path.value, reexport.items.get(item)?)
            }
        };
        self.origin(analysis.module(file, path)?, &entry.name.value)
    }

    /// Follow aliases to the symbol they name.
    pub fn resolve_alias(&self, sym: Sym) -> Option<Sym> {
        let mut current = sym;
        for _ in 0..MAX_DEPTH {
            match current {
                Sym::Alias { file, item } => current = self.alias_target(file, item)?,
                other => return Some(other),
            }
        }
        None
    }

    pub fn anchor_of(&self, sym: &Sym) -> Option<AnchorId> {
        match self.resolve_alias(sym.clone())? {
            Sym::Anchor(id) => Some(id),
            _ => None,
        }
    }

    /// The file a symbol with a single declaration is declared in.
    pub fn declaring_file(&self, sym: &Sym) -> Option<FileId> {
        match sym {
            Sym::Anchor(id) | Sym::Keyword(id) => Some(self.file_of(*id)),
            Sym::Var { file, .. } | Sym::Alias { file, .. } | Sym::Module(file) => Some(*file),
            Sym::Property { .. } | Sym::Key { .. } | Sym::Builtin(_) => None,
        }
    }

    /// The name a symbol is spelled with where it is declared.
    pub fn name_of(&self, sym: &Sym) -> Option<String> {
        let analysis = self.analysis();
        Some(match sym {
            Sym::Anchor(id) => analysis.anchor_def(*id).name.clone(),
            Sym::Keyword(id) => analysis.anchor_def(*id).keyword.as_ref()?.value.clone(),
            Sym::Var { file, index } => analysis.db.file(*file).hir.vars.get(*index)?.name.clone(),
            Sym::Alias { file, item } => {
                let hir = &analysis.db.file(*file).hir;
                let entry = match item {
                    AliasItem::Import { decl, item } => hir.imports.get(*decl)?.items.get(*item)?,
                    AliasItem::Reexport { decl, item } => hir.reexports.get(*decl)?.items.get(*item)?,
                };
                entry.alias.as_ref()?.value.clone()
            }
            Sym::Property { name, .. } | Sym::Key { name, .. } | Sym::Builtin(name) => name.clone(),
            Sym::Module(_) => return None,
        })
    }

    // ---- members ------------------------------------------------------------------

    /// What the left side of a `.` holds.
    pub fn holder(&self, file: FileId, anchor: Option<AnchorId>, expr: &'a Expr, depth: usize) -> Option<Holder<'a>> {
        if depth > MAX_DEPTH {
            return None;
        }
        match expr {
            Expr::Name { name, .. } => self.name_holder(file, anchor, name, depth),
            Expr::Field { base, name, .. } => {
                let holder = self.holder(file, anchor, base, depth + 1)?;
                let member = self.member(&holder, name)?;
                self.value_holder(&member, depth + 1)
            }
            _ => None,
        }
    }

    /// What a bare name, or `self`, `this`, or `super`, holds.
    pub fn name_holder(&self, file: FileId, anchor: Option<AnchorId>, name: &str, depth: usize) -> Option<Holder<'a>> {
        match name {
            "self" | "this" => anchor.map(Holder::Anchor),
            "super" => anchor.map(Holder::Super),
            _ => match self.resolve_alias(self.binding(file, name)?)? {
                Sym::Anchor(id) => Some(Holder::Anchor(id)),
                sym @ Sym::Var { file: owner, index } => {
                    let node = &self.analysis().db.file(owner).hir.vars.get(index)?.body;
                    self.node_holder(sym, owner, None, node, depth + 1)
                }
                _ => None,
            },
        }
    }

    /// What a dotted path written in text holds, for completion, where the
    /// expression has not been finished and so has no tree yet.
    pub fn path_holder(&self, file: FileId, anchor: Option<AnchorId>, path: &[String]) -> Option<Holder<'a>> {
        let (first, rest) = path.split_first()?;
        let mut holder = self.name_holder(file, anchor, first, 0)?;
        for segment in rest {
            let member = self.member(&holder, segment)?;
            holder = self.value_holder(&member, 1)?;
        }
        Some(holder)
    }

    /// The member `name` of a holder, when the compiler would find one.
    pub fn member(&self, holder: &Holder<'a>, name: &str) -> Option<Member<'a>> {
        match holder {
            Holder::Anchor(id) => {
                let slot = self.slot(*id, name)?;
                Some(Member {
                    sym: Sym::Property { family: self.family(*id, name), name: name.to_string() },
                    access: Access::Anchor(*id),
                    property: slot.property,
                    file: self.file_of(slot.owner),
                    anchor: Some(slot.owner),
                })
            }
            Holder::Super(id) => {
                let slot = self.base_slot(*id, name)?;
                Some(Member {
                    sym: Sym::Property { family: self.family(*id, name), name: name.to_string() },
                    access: Access::Super(*id),
                    property: slot.property,
                    file: self.file_of(slot.owner),
                    anchor: Some(slot.owner),
                })
            }
            Holder::Dictionary { owner, file, anchor, node } => {
                let key = find_key(node, name)?;
                Some(Member {
                    sym: Sym::Key { owner: Box::new(owner.clone()), name: name.to_string() },
                    access: Access::Dictionary { file: *file, range: key.name_range },
                    property: key,
                    file: *file,
                    anchor: *anchor,
                })
            }
        }
    }

    /// Every member a holder has, with the declaration each one uses.
    pub fn members(&self, holder: &Holder<'a>) -> Vec<Member<'a>> {
        let names: Vec<String> = match holder {
            Holder::Anchor(id) => self.ordered_properties(*id),
            Holder::Super(id) => {
                let mut names = Vec::new();
                for base in self.bases(*id) {
                    for name in self.ordered_properties(*base) {
                        if !names.contains(&name) {
                            names.push(name);
                        }
                    }
                }
                names
            }
            Holder::Dictionary { node, .. } => {
                let mut names = Vec::new();
                for key in keys(node) {
                    if !names.contains(&key.name) {
                        names.push(key.name.clone());
                    }
                }
                names
            }
        };
        names.iter().filter_map(|name| self.member(holder, name)).collect()
    }

    /// The properties visible on an anchor, the most distant base's first and
    /// the anchor's own last, which is the order its compiled value lists them.
    pub fn ordered_properties(&self, id: AnchorId) -> Vec<String> {
        let mut chain = self.analysis().ancestors(id);
        chain.reverse();
        chain.push(id);
        let mut names: Vec<String> = Vec::new();
        for link in chain {
            for property in self.own(link).iter() {
                if !names.contains(&property.name) {
                    names.push(property.name.clone());
                }
            }
        }
        names
    }

    /// What a member's value holds, when it is written as something a further
    /// `.` can reach.
    pub fn value_holder(&self, member: &Member<'a>, depth: usize) -> Option<Holder<'a>> {
        self.node_holder(member.sym.clone(), member.file, member.anchor, &member.property.node, depth)
    }

    fn node_holder(
        &self,
        owner: Sym,
        file: FileId,
        anchor: Option<AnchorId>,
        node: &'a Node,
        depth: usize,
    ) -> Option<Holder<'a>> {
        match node {
            Node::Value(expr) if matches!(expr, Expr::Name { .. } | Expr::Field { .. }) => {
                self.holder(file, anchor, expr, depth + 1)
            }
            Node::Dict(_) | Node::Mixed(_) | Node::Merge(_) => {
                Some(Holder::Dictionary { owner, file, anchor, node })
            }
            _ => None,
        }
    }
}

/// The properties written directly in a body, however its blocks and lists
/// arrange them, which is how the compiler collects an anchor's own.
pub fn collect<'n>(node: &'n Node, out: &mut Vec<&'n Property>) {
    match node {
        Node::Dict(properties) => out.extend(properties.iter()),
        Node::Mixed(nodes) => nodes.iter().for_each(|node| collect(node, out)),
        Node::List(elements) | Node::Merge(elements) => {
            elements.iter().for_each(|element| collect(&element.node, out))
        }
        Node::Empty | Node::Value(_) => {}
    }
}

/// The keys a member access can reach on a value written as `node`.
pub fn keys(node: &Node) -> Vec<&Property> {
    let dictionaries: Vec<&[Property]> = match node {
        Node::Dict(properties) => vec![properties],
        Node::Mixed(nodes) => nodes
            .iter()
            .filter_map(|node| match node {
                Node::Dict(properties) => Some(properties.as_slice()),
                _ => None,
            })
            .collect(),
        Node::Merge(elements) => elements
            .iter()
            .filter_map(|element| match &element.node {
                Node::Dict(properties) => Some(properties.as_slice()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    dictionaries.into_iter().flatten().collect()
}

/// The key a member access finds, exactly as a compiled value answers it.
///
/// Within one dictionary a repeated key keeps its last value. A mixed block is
/// an implicit list, which answers from the first dictionary in it that has the
/// key. A merge folds its dictionaries left to right, so the last one wins.
fn find_key<'n>(node: &'n Node, name: &str) -> Option<&'n Property> {
    let last_in = |properties: &'n [Property]| properties.iter().rev().find(|it| it.name == name);
    match node {
        Node::Dict(properties) => last_in(properties),
        Node::Mixed(nodes) => nodes.iter().find_map(|node| match node {
            Node::Dict(properties) => last_in(properties),
            _ => None,
        }),
        Node::Merge(elements) => elements.iter().rev().find_map(|element| match &element.node {
            Node::Dict(properties) => last_in(properties),
            _ => None,
        }),
        _ => None,
    }
}
