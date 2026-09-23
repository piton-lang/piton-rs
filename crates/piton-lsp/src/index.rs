//! A per-module map from source offsets to the things they name.
//!
//! Almost every editor feature is the same question asked from a different
//! angle: what is at this position, and where else does it appear? Building one
//! index answers both, and it answers them from the resolved program rather
//! than from raw syntax, so hovering a keyword finds the anchor it aliases.

use std::collections::HashMap;
use std::path::PathBuf;

use piton_compile::{Compilation, ModuleId, Symbol};
use piton_core::{AnchorId, Span};
use piton_syntax::ast::{self, BlockItem, Expr, ExprKind, Item, ProseSegment, ValueNode};

/// What a span in the source refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Anchor(AnchorId),
    Variable(piton_compile::VariableId),
    /// A property of an anchor, identified by the anchor that declares it.
    Property(AnchorId, String),
    /// A key nested inside a property's dictionary: the anchor, then the path
    /// from the property down, so `config: a: 1` is `(Anchor, [config, a])`.
    /// Never confused with the anchor's own properties.
    Key(AnchorId, Vec<String>),
    /// A user-defined keyword and the anchor it aliases.
    Keyword(String, Option<AnchorId>),
    /// A module path in a `use` or `from` declaration.
    Module(PathBuf),
    /// A name that did not resolve; still useful for rename and completion.
    Unresolved(String),
}

/// Whether an occurrence declares the thing or merely mentions it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Definition,
    Reference,
}

/// One named occurrence in the source.
#[derive(Debug, Clone)]
pub struct Occurrence {
    pub span: Span,
    pub target: Target,
    pub role: Role,
    /// The text as written, which is what rename replaces.
    pub text: String,
}

/// Every occurrence in one module, plus structural ranges for folding.
#[derive(Debug, Default)]
pub struct ModuleIndex {
    pub occurrences: Vec<Occurrence>,
    /// Declaration spans, used for folding and selection ranges.
    pub structures: Vec<(Span, String)>,
}

impl ModuleIndex {
    /// The innermost occurrence covering `offset`.
    pub fn at(&self, offset: usize) -> Option<&Occurrence> {
        self.occurrences
            .iter()
            .filter(|occurrence| occurrence.span.contains(offset))
            .min_by_key(|occurrence| occurrence.span.len())
    }
}

/// The index for every loaded module.
#[derive(Debug, Default)]
pub struct Index {
    modules: HashMap<ModuleId, ModuleIndex>,
}

impl Index {
    pub fn get(&self, module: ModuleId) -> Option<&ModuleIndex> {
        self.modules.get(&module)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ModuleId, &ModuleIndex)> {
        self.modules.iter()
    }

    /// Every occurrence of `target` across the whole program.
    pub fn occurrences_of(&self, target: &Target) -> Vec<(ModuleId, &Occurrence)> {
        self.matching(|candidate| candidate == target)
    }

    /// Every occurrence satisfying `predicate`.
    pub fn matching<F>(&self, predicate: F) -> Vec<(ModuleId, &Occurrence)>
    where
        F: Fn(&Target) -> bool,
    {
        let mut out = Vec::new();
        for (module, index) in &self.modules {
            for occurrence in &index.occurrences {
                if predicate(&occurrence.target) {
                    out.push((*module, occurrence));
                }
            }
        }
        out
    }

    /// Builds the index for a compiled program.
    pub fn build(compilation: &Compilation) -> Index {
        let mut modules = HashMap::new();
        for module in compilation.graph().iter() {
            let mut index = ModuleIndex::default();
            let mut builder = Builder {
                compilation,
                module: module.id,
                index: &mut index,
            };
            builder.walk_file(module.ast());
            modules.insert(module.id, index);
        }
        Index { modules }
    }
}

struct Builder<'a> {
    compilation: &'a Compilation,
    module: ModuleId,
    index: &'a mut ModuleIndex,
}

impl Builder<'_> {
    fn push(&mut self, span: Span, target: Target, role: Role, text: impl Into<String>) {
        if span.is_empty() {
            return;
        }
        self.index.occurrences.push(Occurrence {
            span,
            target,
            role,
            text: text.into(),
        });
    }

    fn resolve(&self, name: &str) -> Target {
        match self.compilation.resolution.lookup(self.module, name) {
            Some(Symbol::Anchor(anchor)) => Target::Anchor(anchor),
            Some(Symbol::Variable(variable)) => Target::Variable(variable),
            None => Target::Unresolved(name.to_string()),
        }
    }

    fn walk_file(&mut self, file: &ast::SourceFile) {
        for item in &file.items {
            match item {
                Item::Use(decl) => {
                    self.push(
                        decl.path.span,
                        Target::Module(PathBuf::from(&decl.path.text)),
                        Role::Reference,
                        &decl.path.text,
                    );
                    self.index
                        .structures
                        .push((decl.span, format!("use {}", decl.path.text)));
                }
                Item::From(decl) => {
                    self.push(
                        decl.path.span,
                        Target::Module(PathBuf::from(&decl.path.text)),
                        Role::Reference,
                        &decl.path.text,
                    );
                    for entry in &decl.items {
                        let target = self.resolve(entry.local_name());
                        self.push(entry.name_span, target, Role::Reference, &entry.name);
                    }
                    self.index
                        .structures
                        .push((decl.span, format!("from {}", decl.path.text)));
                }
                Item::ReExport(decl) => {
                    let target = self.resolve(&decl.name);
                    self.push(decl.name_span, target, Role::Reference, &decl.name);
                }
                Item::Variable(decl) => {
                    let target = self.resolve(&decl.name);
                    self.push(decl.name_span, target, Role::Definition, &decl.name);
                    self.index.structures.push((decl.span, decl.name.clone()));
                    // `value:: extends Operator[]` references `Operator` just as
                    // a property constraint does. Skipping it made an import of
                    // that name look unused.
                    for constraint in &decl.constraints {
                        if let ast::TypeName::Named(name) = &constraint.name {
                            let target = self.resolve(name);
                            self.push(constraint.span, target, Role::Reference, name);
                        }
                    }
                    self.walk_value(&decl.value, None, &Place::Unaddressable);
                }
                Item::Anchor(decl) => self.walk_anchor(decl),
            }
        }
    }

    fn walk_anchor(&mut self, decl: &ast::AnchorDecl) {
        let target = self.resolve(&decl.name);
        self.push(decl.name_span, target.clone(), Role::Definition, &decl.name);
        self.index.structures.push((decl.span, decl.name.clone()));

        if decl.keyword != "anchor" {
            let aliased = self
                .compilation
                .resolution
                .scope(self.module)
                .keywords
                .get(&decl.keyword)
                .copied();
            self.push(
                decl.keyword_span,
                Target::Keyword(decl.keyword.clone(), aliased),
                Role::Reference,
                &decl.keyword,
            );
        }
        if let Some(alias) = &decl.alias {
            let anchor = match &target {
                Target::Anchor(anchor) => Some(*anchor),
                _ => None,
            };
            self.push(
                alias.span,
                Target::Keyword(alias.value.clone(), anchor),
                Role::Definition,
                &alias.value,
            );
        }
        for base in &decl.extends {
            let base_target = self.resolve(&base.value);
            self.push(base.span, base_target, Role::Reference, &base.value);
        }

        let owner = match target {
            Target::Anchor(anchor) => Some(anchor),
            _ => None,
        };
        self.walk_block(&decl.body, owner, &Place::Body);
    }

    fn walk_block(&mut self, block: &ast::Block, owner: Option<AnchorId>, place: &Place) {
        for item in &block.items {
            match item {
                BlockItem::Property(property) => {
                    let nested = match (owner, place) {
                        (Some(anchor), Place::Body) => {
                            self.push(
                                property.name_span,
                                Target::Property(anchor, property.name.clone()),
                                Role::Definition,
                                &property.name,
                            );
                            Place::Nested(vec![property.name.clone()])
                        }
                        (Some(anchor), Place::Nested(path)) => {
                            // A key inside a property's dictionary belongs to
                            // that dictionary, not to the anchor.
                            let mut path = path.clone();
                            path.push(property.name.clone());
                            self.push(
                                property.name_span,
                                Target::Key(anchor, path.clone()),
                                Role::Definition,
                                &property.name,
                            );
                            Place::Nested(path)
                        }
                        _ => Place::Unaddressable,
                    };
                    for constraint in &property.constraints {
                        if let ast::TypeName::Named(name) = &constraint.name {
                            let target = self.resolve(name);
                            self.push(constraint.span, target, Role::Reference, name);
                        }
                    }
                    self.index
                        .structures
                        .push((property.span, property.name.clone()));
                    self.walk_value(&property.value, owner, &nested);
                }
                // Anything under a list item is part of a list, and list
                // elements are never addressable.
                BlockItem::ListItem(entry) => {
                    self.walk_value(&entry.value, owner, &Place::Unaddressable)
                }
                BlockItem::Merge(merge) => self.walk_prose(&merge.value, owner),
                BlockItem::Prose(paragraph) => {
                    for line in &paragraph.lines {
                        self.walk_prose(line, owner);
                    }
                }
                BlockItem::Fence(fence) => {
                    self.index
                        .structures
                        .push((fence.span, "fence".to_string()));
                }
                BlockItem::Escape(block) => {
                    // Literal content has no symbols to index, but it folds.
                    self.index
                        .structures
                        .push((block.span, "escape block".to_string()));
                }
                BlockItem::Pass(_) => {}
            }
        }
    }

    fn walk_value(&mut self, value: &ValueNode, owner: Option<AnchorId>, place: &Place) {
        if let Some(line) = &value.inline {
            self.walk_prose(line, owner);
        }
        if let Some(items) = &value.inline_list {
            for item in items {
                self.walk_value(item, owner, &Place::Unaddressable);
            }
        }
        if let Some(block) = &value.block {
            self.walk_block(block, owner, place);
        }
    }

    fn walk_prose(&mut self, line: &ast::ProseLine, owner: Option<AnchorId>) {
        for segment in &line.segments {
            if let ProseSegment::Interpolation(interpolation) = segment {
                self.walk_expr(&interpolation.expr, owner);
            }
        }
    }

    fn walk_expr(&mut self, expr: &Expr, owner: Option<AnchorId>) {
        match &expr.kind {
            ExprKind::Name(name) => {
                let target = self.resolve(name);
                self.push(expr.span, target, Role::Reference, name);
            }
            ExprKind::Field(base, field) => {
                self.walk_expr(base, owner);
                // A field on `this`, `self`, `super` or an anchor names one of
                // its properties (or, further down, a nested key), which is
                // what makes go-to-definition work on it.
                if let Some((anchor, path)) = self.field_target(expr, owner) {
                    let target = if path.len() == 1 {
                        Target::Property(anchor, field.value.clone())
                    } else {
                        Target::Key(anchor, path)
                    };
                    self.push(field.span, target, Role::Reference, &field.value);
                }
            }
            ExprKind::Unary(_, operand) => self.walk_expr(operand, owner),
            ExprKind::Binary(_, lhs, rhs) => {
                self.walk_expr(lhs, owner);
                self.walk_expr(rhs, owner);
            }
            ExprKind::Ternary(condition, consequent, alternative) => {
                self.walk_expr(condition, owner);
                self.walk_expr(consequent, owner);
                self.walk_expr(alternative, owner);
            }
            ExprKind::List(items) => {
                for item in items {
                    self.walk_expr(item, owner);
                }
            }
            ExprKind::Paren(inner) | ExprKind::Nested(_, inner) => self.walk_expr(inner, owner),
            _ => {}
        }
    }

    /// The anchor a field chain reads from and the path below it.
    ///
    /// `this.a` is `(this, [a])`, `Card.config.size` is `(Card, [config,
    /// size])`. A property that holds an anchor starts a new chain at that
    /// anchor, because that is where the next name is looked up.
    fn field_target(&self, expr: &Expr, owner: Option<AnchorId>) -> Option<(AnchorId, Vec<String>)> {
        let ExprKind::Field(base, field) = &expr.kind else {
            return None;
        };
        let (anchor, mut path) = match &base.kind {
            ExprKind::Paren(inner) => return self.field_target(inner, owner),
            ExprKind::Field(..) => self.field_target(base, owner)?,
            _ => (self.field_owner(base, &field.value, owner)?, Vec::new()),
        };
        path.push(field.value.clone());
        let store = self.compilation.store();
        if path.len() == 1 {
            return Some((anchor, path));
        }
        // Re-root at an anchor held by the prefix.
        let mut value = store.anchor(anchor).properties.get(&path[0])?;
        for segment in &path[1..path.len() - 1] {
            value = value.property(segment, self.compilation)?;
        }
        if let piton_core::Value::Anchor(inner) = value {
            return Some((*inner, vec![field.value.clone()]));
        }
        Some((anchor, path))
    }
    /// Which anchor a field access reads from, when that can be known.
    ///
    /// `super.x` reads `x` from everything the anchor inherits, merged the way
    /// inheritance merges it: the right-most base that has `x` supplies it.
    fn field_owner(&self, base: &Expr, field: &str, owner: Option<AnchorId>) -> Option<AnchorId> {
        match &base.kind {
            ExprKind::This | ExprKind::SelfRef => owner,
            ExprKind::Super => owner.and_then(|anchor| super_provider(self.compilation, anchor, field)),
            ExprKind::Name(name) => match self.compilation.resolution.lookup(self.module, name) {
                Some(Symbol::Anchor(anchor)) => Some(anchor),
                _ => None,
            },
            _ => None,
        }
    }
}

/// Where a walk over a block is: directly in an anchor body, inside a
/// property's dictionary (with the path to it), or somewhere no key can be
/// addressed from outside, such as a list.
#[derive(Debug, Clone)]
enum Place {
    Body,
    Nested(Vec<String>),
    Unaddressable,
}

/// The base `super.<name>` reads from: the right-most base with a value for
/// it, else the right-most that declares it at all.
pub fn super_provider(compilation: &Compilation, anchor: AnchorId, name: &str) -> Option<AnchorId> {
    let store = compilation.store();
    let bases = &store.anchor(anchor).bases;
    bases
        .iter()
        .rev()
        .find(|base| {
            store
                .anchor(**base)
                .slots
                .get(name)
                .is_some_and(|slot| slot.has_value)
        })
        .or_else(|| {
            bases
                .iter()
                .rev()
                .find(|base| store.anchor(**base).slots.contains_key(name))
        })
        .copied()
}
