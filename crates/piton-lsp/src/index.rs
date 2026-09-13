//! The symbol model every navigation and refactoring feature shares.
//!
//! Each place a name is written is resolved once, when the model is built, to
//! the symbol it means. Definition, references, highlighting, hover, rename,
//! and unused imports then only compare symbols, so they cannot disagree about
//! what a name refers to.
//!
//! The rules are the ones `spec/scope/lsp/model` states, and every one of them
//! mirrors the compiler: where the compiler resolves a name, so does this, in
//! the same order, and where it would not, nothing here does either.

use std::collections::HashMap;

use piton_core::hir::{Expr, Node, Property, Segment, TypeExpr};
use piton_core::types;
use piton_core::value::AnchorId;
use piton_core::FileId;
use piton_syntax::{TextRange, TextSize};

use crate::world::View;

pub use crate::model::{collect, keys, Holder, Member, Model};

/// One symbol, identified by what declared it rather than by its spelling.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Sym {
    Anchor(AnchorId),
    Var { file: FileId, index: usize },
    /// A user-defined keyword, identified by the anchor whose `as` declares it.
    Keyword(AnchorId),
    /// The second name of an import or re-export item.
    Alias { file: FileId, item: AliasItem },
    Module(FileId),
    /// A property, identified by its family: the lowest anchor id in it.
    Property { family: AnchorId, name: String },
    /// A key of a dictionary value, identified by the symbol whose value it is.
    Key { owner: Box<Sym>, name: String },
    Builtin(String),
}

/// Which item of which declaration an alias was written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AliasItem {
    Import { decl: usize, item: usize },
    Reexport { decl: usize, item: usize },
}

/// What a place does with the symbol it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Declaration,
    Reference,
    /// The first name of an import or re-export item. It names the symbol and
    /// follows a rename, but it is not a use of the symbol in its file.
    ImportName,
}

/// How a member access reached its symbol, which decides the declaration it
/// goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// On an anchor: the declaration nearest in its chain.
    Anchor(AnchorId),
    /// Through `super`: the declaration the anchor's bases provide.
    Super(AnchorId),
    /// A key of one dictionary: that key's declaration.
    Dictionary { file: FileId, range: TextRange },
}

#[derive(Clone, Debug)]
pub struct Occurrence {
    pub range: TextRange,
    pub sym: Sym,
    pub role: Role,
    pub access: Option<Access>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelfKind {
    SelfRef,
    This,
    Super,
}

/// `self`, `this`, or `super`, and the anchor it is written in.
#[derive(Clone, Copy, Debug)]
pub struct SelfReference {
    pub range: TextRange,
    pub kind: SelfKind,
    pub anchor: AnchorId,
}

/// Whatever is under the cursor.
#[derive(Clone, Debug)]
pub enum Located {
    Symbol(Occurrence),
    SelfReference(SelfReference),
}

impl Located {
    pub fn range(&self) -> TextRange {
        match self {
            Located::Symbol(occurrence) => occurrence.range,
            Located::SelfReference(reference) => reference.range,
        }
    }
}

// ---- the index ------------------------------------------------------------------

/// Every resolved place in one project.
#[derive(Default)]
pub struct Index {
    occurrences: HashMap<FileId, Vec<Occurrence>>,
    self_references: HashMap<FileId, Vec<SelfReference>>,
    sites: HashMap<Sym, Vec<(FileId, usize)>>,
    /// For each nested key, the keys written beside each of its declarations.
    siblings: HashMap<Sym, Vec<Vec<String>>>,
}

impl Index {
    pub fn build(view: &View) -> Index {
        let model = Model::new(view);
        let mut builder = Builder { model: &model, index: Index::default() };
        let files: Vec<FileId> = view.compilation.analysis.db.files().map(|file| file.id).collect();
        for file in files {
            builder.file(file);
        }
        let mut index = builder.index;
        for (file, occurrences) in index.occurrences.iter_mut() {
            occurrences.sort_by_key(|it| (it.range.start(), it.range.end()));
            occurrences.dedup_by(|a, b| a.range == b.range && a.sym == b.sym && a.role == b.role);
            for (position, occurrence) in occurrences.iter().enumerate() {
                index.sites.entry(occurrence.sym.clone()).or_default().push((*file, position));
            }
        }
        for references in index.self_references.values_mut() {
            references.sort_by_key(|it| it.range.start());
        }
        index
    }

    /// The symbol or self-reference at `offset`.
    ///
    /// An offset between two tokens belongs to both, and the one on the right
    /// wins, because that is the one the cursor sits in front of.
    pub fn locate(&self, file: FileId, offset: TextSize) -> Option<Located> {
        let symbol = self
            .in_file(file)
            .iter()
            .filter(|it| it.range.start() <= offset && offset <= it.range.end())
            .max_by_key(|it| it.range.start());
        let reference = self
            .self_references
            .get(&file)
            .into_iter()
            .flatten()
            .filter(|it| it.range.start() <= offset && offset <= it.range.end())
            .max_by_key(|it| it.range.start());
        match (symbol, reference) {
            (Some(symbol), Some(reference)) if reference.range.start() > symbol.range.start() => {
                Some(Located::SelfReference(*reference))
            }
            (Some(symbol), _) => Some(Located::Symbol(symbol.clone())),
            (None, Some(reference)) => Some(Located::SelfReference(*reference)),
            (None, None) => None,
        }
    }

    pub fn in_file(&self, file: FileId) -> &[Occurrence] {
        self.occurrences.get(&file).map(Vec::as_slice).unwrap_or_default()
    }

    /// Every place a symbol is written, with the file it is written in.
    pub fn sites<'i>(&'i self, sym: &Sym) -> impl Iterator<Item = (FileId, &'i Occurrence)> + 'i {
        self.sites
            .get(sym)
            .into_iter()
            .flatten()
            .map(|(file, position)| (*file, &self.occurrences[file][*position]))
    }

    pub fn declarations(&self, sym: &Sym) -> Vec<(FileId, TextRange)> {
        self.sites(sym)
            .filter(|(_, it)| it.role == Role::Declaration)
            .map(|(file, it)| (file, it.range))
            .collect()
    }

    /// The keys written beside each declaration of a nested key.
    pub fn siblings(&self, sym: &Sym) -> &[Vec<String>] {
        self.siblings.get(sym).map(Vec::as_slice).unwrap_or_default()
    }
}

// ---- building it -----------------------------------------------------------------

struct Builder<'m, 'a> {
    model: &'m Model<'a>,
    index: Index,
}

impl<'a> Builder<'_, 'a> {
    fn push(&mut self, file: FileId, range: TextRange, sym: Sym, role: Role, access: Option<Access>) {
        self.index.occurrences.entry(file).or_default().push(Occurrence { range, sym, role, access });
    }

    fn file(&mut self, file: FileId) {
        let analysis = self.model.analysis();
        let hir = &analysis.db.file(file).hir;
        let scope = analysis.scope(file);

        for (position, def) in hir.anchors.iter().enumerate() {
            let Some(id) = analysis.anchor_id(file, position) else { continue };
            self.push(file, def.name_range, Sym::Anchor(id), Role::Declaration, None);
            if let Some(keyword) = &def.keyword {
                self.push(file, keyword.range, Sym::Keyword(id), Role::Declaration, None);
            }
            if let Some(keyword) = &def.via_keyword {
                if let Some(declared) = scope.keywords.get(&keyword.value) {
                    self.push(file, keyword.range, Sym::Keyword(*declared), Role::Reference, None);
                }
            }
            for base in &def.bases {
                if let Some(sym) = self.model.binding(file, &base.value) {
                    if self.model.anchor_of(&sym).is_some() {
                        self.push(file, base.range, sym, Role::Reference, None);
                    }
                }
            }
            self.anchor_body(file, id, &def.body);
        }

        for (index, def) in hir.vars.iter().enumerate() {
            let sym = Sym::Var { file, index };
            self.push(file, def.name_range, sym.clone(), Role::Declaration, None);
            for constraint in &def.constraints {
                self.type_expr(file, constraint, false);
            }
            self.value(file, None, Some(&sym), &def.body);
        }

        for (decl, import) in hir.imports.iter().enumerate() {
            let module = analysis.module(file, &import.path.value);
            if let Some(module) = module {
                self.push(file, import.path.range, Sym::Module(module), Role::Reference, None);
            }
            for (item, entry) in import.items.iter().enumerate() {
                let Some(origin) = module.and_then(|it| self.model.origin(it, &entry.name.value))
                else {
                    continue;
                };
                self.push(file, entry.name.range, origin, Role::ImportName, None);
                if let Some(alias) = &entry.alias {
                    let sym = Sym::Alias { file, item: AliasItem::Import { decl, item } };
                    self.push(file, alias.range, sym, Role::Declaration, None);
                }
            }
        }

        for (decl, reexport) in hir.reexports.iter().enumerate() {
            let module = analysis.module(file, &reexport.path.value);
            if let Some(module) = module {
                self.push(file, reexport.path.range, Sym::Module(module), Role::Reference, None);
            }
            for (item, entry) in reexport.items.iter().enumerate() {
                let Some(origin) = module.and_then(|it| self.model.origin(it, &entry.name.value))
                else {
                    continue;
                };
                self.push(file, entry.name.range, origin, Role::ImportName, None);
                if let Some(alias) = &entry.alias {
                    let sym = Sym::Alias { file, item: AliasItem::Reexport { decl, item } };
                    self.push(file, alias.range, sym, Role::Declaration, None);
                }
            }
        }

        for export in &hir.exports {
            if let Some(sym) = self.model.binding(file, &export.value) {
                self.push(file, export.range, sym, Role::Reference, None);
            }
        }

        for spec in &hir.uses {
            if let Some(module) = analysis.module(file, &spec.value) {
                self.push(file, spec.range, Sym::Module(module), Role::Reference, None);
            }
        }
    }

    /// An anchor's own body, where every key at any depth of its blocks and
    /// lists is one of the anchor's properties, exactly as the compiler
    /// collects them.
    fn anchor_body(&mut self, file: FileId, id: AnchorId, node: &'a Node) {
        match node {
            Node::Empty => {}
            Node::Value(expr) => self.expr(file, Some(id), expr),
            Node::Dict(properties) => {
                for property in properties {
                    let sym =
                        Sym::Property { family: self.model.family(id, &property.name), name: property.name.clone() };
                    self.push(file, property.name_range, sym.clone(), Role::Declaration, Some(Access::Anchor(id)));
                    for constraint in &property.constraints {
                        self.type_expr(file, constraint, false);
                    }
                    self.value(file, Some(id), Some(&sym), &property.node);
                }
            }
            Node::Mixed(nodes) => nodes.iter().for_each(|node| self.anchor_body(file, id, node)),
            Node::List(elements) | Node::Merge(elements) => {
                elements.iter().for_each(|element| self.anchor_body(file, id, &element.node))
            }
        }
    }

    /// A value, whose keys belong to `owner` when a member access can reach
    /// them, which is exactly when the compiled value is a dictionary.
    fn value(&mut self, file: FileId, anchor: Option<AnchorId>, owner: Option<&Sym>, node: &'a Node) {
        match node {
            Node::Empty => {}
            Node::Value(expr) => self.expr(file, anchor, expr),
            Node::Dict(properties) => {
                let names = properties.iter().map(|it| it.name.clone()).collect::<Vec<_>>();
                self.keys(file, anchor, owner, properties, &names);
            }
            Node::Mixed(nodes) => {
                let names = addressable_names(node);
                for node in nodes {
                    match node {
                        Node::Dict(properties) => self.keys(file, anchor, owner, properties, &names),
                        other => self.value(file, anchor, None, other),
                    }
                }
            }
            Node::Merge(elements) => {
                let names = addressable_names(node);
                for element in elements {
                    match &element.node {
                        Node::Dict(properties) => self.keys(file, anchor, owner, properties, &names),
                        other => self.value(file, anchor, None, other),
                    }
                }
            }
            // An explicit list is not a dictionary, so the keys inside its
            // items are nobody's members; their values are still walked.
            Node::List(elements) => {
                elements.iter().for_each(|element| self.value(file, anchor, None, &element.node))
            }
        }
    }

    fn keys(
        &mut self,
        file: FileId,
        anchor: Option<AnchorId>,
        owner: Option<&Sym>,
        properties: &'a [Property],
        names: &[String],
    ) {
        for property in properties {
            let sym = owner.map(|owner| Sym::Key { owner: Box::new(owner.clone()), name: property.name.clone() });
            if let Some(sym) = &sym {
                let access = Access::Dictionary { file, range: property.name_range };
                self.push(file, property.name_range, sym.clone(), Role::Declaration, Some(access));
                self.index.siblings.entry(sym.clone()).or_default().push(names.to_vec());
            }
            for constraint in &property.constraints {
                self.type_expr(file, constraint, false);
            }
            self.value(file, anchor, sym.as_ref(), &property.node);
        }
    }

    /// A `::` constraint. A built-in name is a built-in type, as the checker
    /// reads it, except inside `extends`, which only ever names an anchor.
    fn type_expr(&mut self, file: FileId, constraint: &TypeExpr, inside_extends: bool) {
        match constraint {
            TypeExpr::Named { name, range } => {
                if !inside_extends && types::is_builtin(name) {
                    self.push(file, *range, Sym::Builtin(name.clone()), Role::Reference, None);
                } else if let Some(sym) = self.model.binding(file, name) {
                    if self.model.anchor_of(&sym).is_some() {
                        self.push(file, *range, sym, Role::Reference, None);
                    }
                }
            }
            TypeExpr::ListOf { element, .. } => self.type_expr(file, element, inside_extends),
            TypeExpr::Extends { base, .. } => self.type_expr(file, base, true),
        }
    }

    fn expr(&mut self, file: FileId, anchor: Option<AnchorId>, expr: &'a Expr) {
        match expr {
            Expr::Name { name, range } => {
                let kind = match name.as_str() {
                    "self" => Some(SelfKind::SelfRef),
                    "this" => Some(SelfKind::This),
                    "super" => Some(SelfKind::Super),
                    _ => None,
                };
                match (kind, anchor) {
                    (Some(kind), Some(anchor)) => self
                        .index
                        .self_references
                        .entry(file)
                        .or_default()
                        .push(SelfReference { range: *range, kind, anchor }),
                    (Some(_), None) => {}
                    (None, _) => {
                        if let Some(sym) = self.model.binding(file, name) {
                            self.push(file, *range, sym, Role::Reference, None);
                        }
                    }
                }
            }
            Expr::Field { base, name, name_range, .. } => {
                self.expr(file, anchor, base);
                if let Some(member) =
                    self.model.holder(file, anchor, base, 0).and_then(|holder| self.model.member(&holder, name))
                {
                    self.push(file, *name_range, member.sym, Role::Reference, Some(member.access));
                }
            }
            Expr::Text(text) => {
                for line in text.paragraphs.iter().flatten() {
                    for segment in &line.segments {
                        if let Segment::Interpolation(interpolation) = segment {
                            self.expr(file, anchor, &interpolation.expr);
                        }
                    }
                }
            }
            Expr::Array { elements, .. } => elements.iter().for_each(|it| self.expr(file, anchor, it)),
            Expr::Binary { lhs, rhs, .. } => {
                self.expr(file, anchor, lhs);
                self.expr(file, anchor, rhs);
            }
            Expr::Unary { operand, .. } => self.expr(file, anchor, operand),
            Expr::Ternary { condition, then, otherwise, .. } => {
                self.expr(file, anchor, condition);
                self.expr(file, anchor, then);
                self.expr(file, anchor, otherwise);
            }
            Expr::Literal { .. } | Expr::Error { .. } => {}
        }
    }
}

/// Every key a member access could reach on a mixed or merged value.
fn addressable_names(node: &Node) -> Vec<String> {
    keys(node).iter().map(|it| it.name.clone()).collect()
}
