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
use piton_syntax::ast::{
    self, BlockItem, Expr, ExprKind, Item, ProseSegment, ValueNode,
};

/// What a span in the source refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Anchor(AnchorId),
    Variable(piton_compile::VariableId),
    /// A property of an anchor, identified by the anchor that declares it.
    Property(AnchorId, String),
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
                    self.walk_value(&decl.value, None);
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
        self.walk_block(&decl.body, owner);
    }

    fn walk_block(&mut self, block: &ast::Block, owner: Option<AnchorId>) {
        for item in &block.items {
            match item {
                BlockItem::Property(property) => {
                    if let Some(anchor) = owner {
                        self.push(
                            property.name_span,
                            Target::Property(anchor, property.name.clone()),
                            Role::Definition,
                            &property.name,
                        );
                    }
                    for constraint in &property.constraints {
                        if let ast::TypeName::Named(name) = &constraint.name {
                            let target = self.resolve(name);
                            self.push(constraint.span, target, Role::Reference, name);
                        }
                    }
                    self.index
                        .structures
                        .push((property.span, property.name.clone()));
                    self.walk_value(&property.value, owner);
                }
                BlockItem::ListItem(entry) => self.walk_value(&entry.value, owner),
                BlockItem::Merge(merge) => self.walk_prose(&merge.value, owner),
                BlockItem::Prose(paragraph) => {
                    for line in &paragraph.lines {
                        self.walk_prose(line, owner);
                    }
                }
                BlockItem::Fence(fence) => {
                    self.index.structures.push((fence.span, "fence".to_string()));
                }
                BlockItem::Pass(_) => {}
            }
        }
    }

    fn walk_value(&mut self, value: &ValueNode, owner: Option<AnchorId>) {
        if let Some(line) = &value.inline {
            self.walk_prose(line, owner);
        }
        if let Some(items) = &value.inline_list {
            for item in items {
                self.walk_value(item, owner);
            }
        }
        if let Some(block) = &value.block {
            self.walk_block(block, owner);
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
                // A field on `this` or `self` names a property of the enclosing
                // anchor, which is what makes go-to-definition work on it.
                if let Some(anchor) = self.field_owner(base, owner) {
                    self.push(
                        field.span,
                        Target::Property(anchor, field.value.clone()),
                        Role::Reference,
                        &field.value,
                    );
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
            ExprKind::Paren(inner) => self.walk_expr(inner, owner),
            _ => {}
        }
    }

    /// Which anchor a field access reads from, when that can be known.
    fn field_owner(&self, base: &Expr, owner: Option<AnchorId>) -> Option<AnchorId> {
        match &base.kind {
            ExprKind::This | ExprKind::SelfRef => owner,
            ExprKind::Super => owner
                .and_then(|anchor| self.compilation.store().anchor(anchor).bases.last().copied()),
            ExprKind::Name(name) => {
                match self.compilation.resolution.lookup(self.module, name) {
                    Some(Symbol::Anchor(anchor)) => Some(anchor),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}
