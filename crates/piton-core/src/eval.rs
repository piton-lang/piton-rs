//! Evaluation.
//!
//! Anchors are compiled property by property, lazily, so that `self.x` inside
//! an anchor does not have to compile the whole anchor first. Two contexts
//! travel with every expression: `self`, the most-derived anchor being
//! compiled, and `this`, the anchor the expression was written in.

use std::collections::HashMap;
use std::sync::Arc;

use indexmap::IndexMap;
use piton_syntax::TextRange;

use crate::diag::{Diagnostic, Diagnostics};
use crate::framework::{self, Frameworks};
use crate::hir::{BinaryOp, Element, Expr, Literal, Node, Property, Segment, Text, TypeExpr};
use crate::resolve::{Analysis, Symbol};
use crate::types::{constrain, TypeContext};
use crate::value::{self, Anchor, AnchorId, BinOp, Dict, List, Num, Value};
use crate::FileId;

/// Where one property definition lives in the inheritance chain.
#[derive(Clone, Debug)]
struct Slot {
    /// The anchor whose body wrote this definition.
    owner: AnchorId,
    index: usize,
    /// The right-most non-empty constraint list found in the chain.
    constraints: Option<(AnchorId, Vec<TypeExpr>)>,
}

type PropertyMap = IndexMap<String, Slot>;

/// Identifies a property evaluation for caching and cycle detection.
type PropKey = (AnchorId, String, AnchorId);

#[derive(Clone, Debug, PartialEq, Eq)]
enum Frame {
    Var(FileId, usize),
    Prop(PropKey),
    Anchor(AnchorId),
}

/// What `self` and `this` point at while an expression is evaluated.
#[derive(Clone, Copy, Debug)]
pub struct Context {
    pub file: FileId,
    pub self_anchor: Option<AnchorId>,
    pub this_anchor: Option<AnchorId>,
}

impl Context {
    pub fn file(file: FileId) -> Context {
        Context { file, self_anchor: None, this_anchor: None }
    }
}

/// Evaluates a resolved workspace.
pub struct Evaluator<'a> {
    pub analysis: &'a Analysis,
    frameworks: &'a Frameworks,
    vars: HashMap<(FileId, usize), Value>,
    props: HashMap<PropKey, Value>,
    anchors: HashMap<AnchorId, Anchor>,
    property_maps: HashMap<AnchorId, PropertyMap>,
    own_props: HashMap<AnchorId, Arc<Vec<Property>>>,
    stack: Vec<Frame>,
    pub diagnostics: Diagnostics,
    /// Anchors reached during evaluation, in discovery order.
    pub reached: Vec<AnchorId>,
}

impl<'a> Evaluator<'a> {
    pub fn new(analysis: &'a Analysis, frameworks: &'a Frameworks) -> Evaluator<'a> {
        Evaluator {
            analysis,
            frameworks,
            vars: HashMap::new(),
            props: HashMap::new(),
            anchors: HashMap::new(),
            property_maps: HashMap::new(),
            own_props: HashMap::new(),
            stack: Vec::new(),
            diagnostics: Diagnostics::default(),
            reached: Vec::new(),
        }
    }

    // ---- top-level entry points ------------------------------------------

    /// Compile an anchor into its dictionary of resolved properties.
    pub fn anchor(&mut self, id: AnchorId) -> Anchor {
        if let Some(anchor) = self.anchors.get(&id) {
            return anchor.clone();
        }
        let def = self.analysis.anchor_def(id);
        let name = def.name.clone();
        let range = def.name_range;
        let file = self.analysis.anchor_loc(id).file;
        if !self.reached.contains(&id) {
            self.reached.push(id);
        }
        if self.enter(Frame::Anchor(id), file, range, &format!("anchor `{name}`")) {
            return Anchor { id, name, props: Arc::new(Dict::new()) };
        }

        let mut props = Dict::new();
        for key in self.property_map(id).keys().cloned().collect::<Vec<_>>() {
            let value = self.property(id, &key, id);
            props.insert(key, value);
        }
        self.stack.pop();

        let anchor = Anchor { id, name, props: Arc::new(props) };
        self.anchors.insert(id, anchor.clone());
        anchor
    }

    /// Evaluate a top-level variable.
    pub fn var(&mut self, file: FileId, index: usize) -> Value {
        if let Some(value) = self.vars.get(&(file, index)) {
            return value.clone();
        }
        let def = self.analysis.db.file(file).hir.vars[index].clone();
        if self.enter(Frame::Var(file, index), file, def.name_range, &format!("`{}`", def.name)) {
            return Value::Null;
        }
        let mut value = self.node(&def.body, Context::file(file));
        if !def.body.is_empty() {
            value = self.apply_constraints(value, &def.constraints, file, def.name_range);
        }
        self.stack.pop();
        self.vars.insert((file, index), value.clone());
        value
    }

    /// Look up one property of an anchor, resolving `self` to `self_anchor`.
    pub fn property(&mut self, scope: AnchorId, name: &str, self_anchor: AnchorId) -> Value {
        let key = (scope, name.to_string(), self_anchor);
        if let Some(value) = self.props.get(&key) {
            return value.clone();
        }
        let map = self.property_map(scope);
        let Some(slot) = map.get(name).cloned() else {
            return Value::Null;
        };
        let owner_file = self.analysis.anchor_loc(slot.owner).file;
        let props = self.owned_properties(slot.owner);
        let Some(property) = props.get(slot.index).cloned() else { return Value::Null };

        if self.enter(Frame::Prop(key.clone()), owner_file, property.name_range, &format!("`{name}`"))
        {
            return Value::Null;
        }
        let context = Context {
            file: owner_file,
            self_anchor: Some(self_anchor),
            this_anchor: Some(slot.owner),
        };
        let mut value = self.node(&property.node, context);
        // `name:: type` with no value declares a shape rather than assigning
        // one, so there is nothing to check the constraint against.
        if !property.node.is_empty() {
            if let Some((constraint_owner, constraints)) = &slot.constraints {
                let file = self.analysis.anchor_loc(*constraint_owner).file;
                value = self.apply_constraints(value, constraints, file, property.name_range);
            }
        }
        self.stack.pop();
        self.props.insert(key, value.clone());
        value
    }

    // ---- nodes -------------------------------------------------------------

    pub fn node(&mut self, node: &Node, context: Context) -> Value {
        match node {
            Node::Empty => Value::Null,
            Node::Value(expr) => self.expr(expr, context),
            Node::Dict(properties) => Value::Dict(self.dict(properties, context)),
            Node::Mixed(nodes) => {
                let items = nodes.iter().map(|node| self.node(node, context)).collect();
                Value::List(List::implicit(items))
            }
            Node::List(elements) => self.list(elements, context),
        }
    }

    fn dict(&mut self, properties: &[Property], context: Context) -> Dict {
        let mut dict = Dict::new();
        for property in properties {
            let mut value = self.node(&property.node, context);
            if !property.node.is_empty() {
                value = self.apply_constraints(
                    value,
                    &property.constraints,
                    context.file,
                    property.name_range,
                );
            }
            dict.insert(property.name.clone(), value);
        }
        dict
    }

    /// Build a list, folding `+` and `++` spreads into the accumulated value.
    fn list(&mut self, elements: &[Element], context: Context) -> Value {
        let mut items: Vec<Value> = Vec::new();
        for element in elements {
            let value = self.node(&element.node, context);
            match element.spread {
                None => items.push(value),
                Some(dedup) => match value {
                    Value::List(_) => {
                        let joined =
                            value::concat(Value::List(List::explicit(items)), value, dedup);
                        items = match joined {
                            Value::List(list) => list.items,
                            other => vec![other],
                        };
                    }
                    other => items.push(other),
                },
            }
        }
        Value::list(items)
    }

    // ---- expressions ---------------------------------------------------------

    pub fn expr(&mut self, expr: &Expr, context: Context) -> Value {
        match expr {
            Expr::Error { .. } => Value::Null,
            Expr::Literal { value, .. } => match value {
                Literal::Number(text) => Value::Number(Num::literal(text)),
                Literal::String(text) => Value::Str(text.clone()),
                Literal::Bool(flag) => Value::Bool(*flag),
                Literal::Null => Value::Null,
            },
            Expr::Text(text) => self.text(text, context),
            Expr::Array { elements, .. } => {
                Value::list(elements.iter().map(|it| self.expr(it, context)).collect())
            }
            Expr::Unary { operand, range } => match self.expr(operand, context) {
                Value::Number(number) => Value::number(-number.value),
                other => {
                    self.error(context.file, *range, format!("cannot negate {}", other.type_name()));
                    Value::Null
                }
            },
            Expr::Ternary { condition, then, otherwise, .. } => {
                if self.expr(condition, context).is_truthy() {
                    self.expr(then, context)
                } else {
                    self.expr(otherwise, context)
                }
            }
            Expr::Binary { op, lhs, rhs, range } => self.binary(*op, lhs, rhs, *range, context),
            Expr::Name { name, range } => self.name(name, *range, context),
            Expr::Field { base, name, range, .. } => self.field(base, name, *range, context),
        }
    }

    fn binary(
        &mut self,
        op: BinaryOp,
        lhs: &Expr,
        rhs: &Expr,
        range: TextRange,
        context: Context,
    ) -> Value {
        // Logical operators short-circuit, so the right side may never run.
        if matches!(op, BinaryOp::And | BinaryOp::Or) {
            let left = self.expr(lhs, context);
            let take_right = match op {
                BinaryOp::And => left.is_truthy(),
                _ => !left.is_truthy(),
            };
            return if take_right { self.expr(rhs, context) } else { left };
        }
        let left = self.expr(lhs, context);
        let right = self.expr(rhs, context);
        let value_op = match op {
            BinaryOp::Add => BinOp::Add,
            BinaryOp::Concat => BinOp::Concat,
            BinaryOp::Sub => BinOp::Sub,
            BinaryOp::Mul => BinOp::Mul,
            BinaryOp::Div => BinOp::Div,
            BinaryOp::Rem => BinOp::Rem,
            BinaryOp::Eq => BinOp::Eq,
            BinaryOp::Ne => BinOp::Ne,
            BinaryOp::Lt => BinOp::Lt,
            BinaryOp::Le => BinOp::Le,
            BinaryOp::Gt => BinOp::Gt,
            BinaryOp::Ge => BinOp::Ge,
            BinaryOp::And | BinaryOp::Or => unreachable!(),
        };
        match value::apply(value_op, left, right) {
            Ok(value) => value,
            Err(error) => {
                self.error(context.file, range, error.0);
                Value::Null
            }
        }
    }

    fn name(&mut self, name: &str, range: TextRange, context: Context) -> Value {
        match name {
            "self" => match context.self_anchor {
                Some(id) => Value::Anchor(self.anchor(id)),
                None => self.undefined(name, range, context),
            },
            "this" => match context.this_anchor {
                Some(id) => Value::Dict(self.anchor_dict(id, context)),
                None => self.undefined(name, range, context),
            },
            "super" => match context.this_anchor {
                Some(id) => Value::Dict(self.super_dict(id, context)),
                None => self.undefined(name, range, context),
            },
            _ => {
                if let Some(value) = self.frameworks.builtin_value(name) {
                    return value;
                }
                match self.analysis.scope(context.file).names.get(name).copied() {
                    Some(Symbol::Anchor(id)) => Value::Anchor(self.anchor(id)),
                    Some(Symbol::Var { file, index }) => self.var(file, index),
                    None => self.undefined(name, range, context),
                }
            }
        }
    }

    /// `self.x`, `this.x`, and `super.x` resolve one property without
    /// compiling the whole anchor, which is what makes recursion terminate.
    fn field(&mut self, base: &Expr, name: &str, range: TextRange, context: Context) -> Value {
        if let Expr::Name { name: root, .. } = base {
            let scope = match (root.as_str(), context.self_anchor, context.this_anchor) {
                ("self", Some(id), _) => Some((id, id)),
                ("this", Some(self_id), Some(id)) => Some((id, self_id)),
                ("this", None, Some(id)) => Some((id, id)),
                ("super", self_id, Some(id)) => {
                    let map = self.base_property_map(id);
                    let Some(slot) = map.get(name).cloned() else {
                        self.error(
                            context.file,
                            range,
                            format!("no base of this anchor defines `{name}`"),
                        );
                        return Value::Null;
                    };
                    let self_id = self_id.unwrap_or(slot.owner);
                    return self.property(slot.owner, name, self_id);
                }
                _ => None,
            };
            if let Some((scope, self_id)) = scope {
                if self.property_map(scope).contains_key(name) {
                    return self.property(scope, name, self_id);
                }
                self.error(context.file, range, format!("`{root}` has no property `{name}`"));
                return Value::Null;
            }
        }
        let value = self.expr(base, context);
        match value.field(name) {
            Some(found) => found.clone(),
            None => {
                self.error(
                    context.file,
                    range,
                    format!("{} has no property `{name}`", value.type_name()),
                );
                Value::Null
            }
        }
    }

    // ---- prose ------------------------------------------------------------------

    /// Assemble prose. Lines inside a paragraph join with a space; paragraphs
    /// join with a newline. Interpolating a complex value splits the result
    /// into a mixed list, which is what the frameworks want to see.
    fn text(&mut self, text: &Text, context: Context) -> Value {
        let mut parts: Vec<Value> = Vec::new();
        for (paragraph_index, paragraph) in text.paragraphs.iter().enumerate() {
            if paragraph_index > 0 {
                parts.push(Value::string("\n"));
            }
            for (line_index, line) in paragraph.iter().enumerate() {
                if line_index > 0 {
                    parts.push(Value::string(" "));
                }
                for segment in &line.segments {
                    match segment {
                        Segment::Literal(literal) => parts.push(Value::string(literal.clone())),
                        Segment::Interpolation(interpolation) => {
                            let value = self.expr(&interpolation.expr, context);
                            parts.push(self.interpolate(
                                &interpolation.sigil,
                                value,
                                interpolation.range,
                                context,
                            ));
                        }
                    }
                }
            }
        }
        join_parts(parts)
    }

    fn interpolate(
        &mut self,
        sigil: &str,
        value: Value,
        range: TextRange,
        context: Context,
    ) -> Value {
        if !self.frameworks.knows_sigil(sigil) {
            self.diagnostics.push(Diagnostic::warning(
                "unknown-sigil",
                context.file,
                range,
                format!("no framework defines the `{sigil}{{}}` sigil; treating it as `${{}}`"),
            ));
        }
        let anchor_source = match &value {
            Value::Anchor(anchor) => {
                let file = self.analysis.anchor_loc(anchor.id).file;
                self.analysis.db.file(file).source.as_path().map(|path| path.to_path_buf())
            }
            _ => None,
        };
        let request = framework::Interpolation {
            sigil,
            value: &value,
            owner: context.self_anchor,
            anchor_source,
        };
        if let Some(rendered) = self.frameworks.interpolate(&request) {
            return rendered;
        }
        if value.is_simple() {
            Value::Str(value.to_literal())
        } else {
            value
        }
    }

    // ---- inheritance ---------------------------------------------------------------

    /// The properties an anchor has, merged left to right with the child last.
    fn property_map(&mut self, id: AnchorId) -> PropertyMap {
        if let Some(map) = self.property_maps.get(&id) {
            return map.clone();
        }
        // Insert a placeholder so an inheritance cycle terminates.
        self.property_maps.insert(id, PropertyMap::new());
        let mut map = self.base_property_map(id);
        for (index, property) in self.owned_properties(id).iter().enumerate() {
            let inherited = map.get(&property.name).and_then(|slot| slot.constraints.clone());
            let constraints = if property.constraints.is_empty() {
                inherited
            } else {
                Some((id, property.constraints.clone()))
            };
            map.insert(property.name.clone(), Slot { owner: id, index, constraints });
        }
        self.property_maps.insert(id, map.clone());
        map
    }

    /// The merged properties of an anchor's bases, without its own.
    fn base_property_map(&mut self, id: AnchorId) -> PropertyMap {
        let mut map = PropertyMap::new();
        for base in self.analysis.bases(id) {
            for (name, slot) in self.property_map(base) {
                let inherited = map.get(&name).and_then(|existing| existing.constraints.clone());
                let constraints = slot.constraints.clone().or(inherited);
                map.insert(name, Slot { constraints, ..slot });
            }
        }
        map
    }

    /// Compile every property visible through `anchor`, keeping `self` fixed.
    fn anchor_dict(&mut self, id: AnchorId, context: Context) -> Dict {
        let self_anchor = context.self_anchor.unwrap_or(id);
        let mut dict = Dict::new();
        for name in self.property_map(id).keys().cloned().collect::<Vec<_>>() {
            let value = self.property(id, &name, self_anchor);
            dict.insert(name, value);
        }
        dict
    }

    fn super_dict(&mut self, id: AnchorId, context: Context) -> Dict {
        let self_anchor = context.self_anchor.unwrap_or(id);
        let mut dict = Dict::new();
        for (name, slot) in self.base_property_map(id) {
            let value = self.property(slot.owner, &name, self_anchor);
            dict.insert(name, value);
        }
        dict
    }

    /// The properties written in an anchor's own body.
    fn owned_properties(&mut self, id: AnchorId) -> Arc<Vec<Property>> {
        if let Some(properties) = self.own_props.get(&id) {
            return properties.clone();
        }
        let mut properties = Vec::new();
        collect_properties(&self.analysis.anchor_def(id).body, &mut properties);
        let properties = Arc::new(properties);
        self.own_props.insert(id, properties.clone());
        properties
    }

    // ---- plumbing --------------------------------------------------------------------

    fn apply_constraints(
        &mut self,
        value: Value,
        constraints: &[TypeExpr],
        file: FileId,
        range: TextRange,
    ) -> Value {
        if constraints.is_empty() {
            return value;
        }
        let types = Types { analysis: self.analysis, file };
        match constrain(value.clone(), constraints, &types) {
            Ok(coerced) => coerced,
            Err(message) => {
                self.error(file, range, message);
                value
            }
        }
    }

    /// Push a cycle-detection frame, reporting and refusing when it repeats.
    fn enter(&mut self, frame: Frame, file: FileId, range: TextRange, what: &str) -> bool {
        if self.stack.contains(&frame) {
            self.error(file, range, format!("{what} refers to itself"));
            return true;
        }
        self.stack.push(frame);
        false
    }

    fn undefined(&mut self, name: &str, range: TextRange, context: Context) -> Value {
        self.error(context.file, range, format!("cannot find `{name}` in this scope"));
        Value::Null
    }

    fn error(&mut self, file: FileId, range: TextRange, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::error("eval", file, range, message));
    }
}

/// Adapter that lets the constraint checker ask about anchors.
struct Types<'a> {
    analysis: &'a Analysis,
    file: FileId,
}

impl TypeContext for Types<'_> {
    fn resolve_anchor(&self, name: &str) -> Option<AnchorId> {
        match self.analysis.scope(self.file).names.get(name) {
            Some(Symbol::Anchor(id)) => Some(*id),
            _ => None,
        }
    }
    fn direct_bases(&self, id: AnchorId) -> Vec<AnchorId> {
        self.analysis.bases(id)
    }
    fn ancestors(&self, id: AnchorId) -> Vec<AnchorId> {
        self.analysis.ancestors(id)
    }
}

fn collect_properties(node: &Node, out: &mut Vec<Property>) {
    match node {
        Node::Dict(properties) => out.extend(properties.iter().cloned()),
        Node::Mixed(nodes) => nodes.iter().for_each(|node| collect_properties(node, out)),
        _ => {}
    }
}

/// Merge adjacent text parts, leaving complex values as their own elements.
fn join_parts(parts: Vec<Value>) -> Value {
    let mut merged: Vec<Value> = Vec::new();
    for part in parts {
        match (merged.last_mut(), &part) {
            (Some(Value::Str(accumulated)), Value::Str(text)) => accumulated.push_str(text),
            _ => merged.push(part),
        }
    }
    match merged.len() {
        0 => Value::string(""),
        1 => merged.pop().unwrap(),
        _ => {
            // Whitespace between a word and an interpolated structure is layout,
            // not content, so it is trimmed once the parts stay separate.
            let items: Vec<Value> = merged
                .into_iter()
                .map(|value| match value {
                    Value::Str(text) => Value::Str(text.trim().to_string()),
                    other => other,
                })
                .filter(|value| !matches!(value, Value::Str(text) if text.is_empty()))
                .collect();
            Value::List(List::implicit(items))
        }
    }
}
