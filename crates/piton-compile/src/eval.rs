//! Value evaluation.
//!
//! Piton has no runtime, so "evaluation" means folding a declaration down to the
//! value an adapter will serialize. Values are produced lazily and memoized:
//! anchors resolve by identity, so `${A}` inside `B` never forces `A`'s
//! properties, which is what makes circular *references* legal while circular
//! *values* stay an error.

use std::collections::HashMap;
use std::path::PathBuf;

use piton_core::{
    format_number, AnchorId, Diagnostic, Mixed, MixedItem, Properties, Span, Text, Value,
};
use piton_syntax::ast::{
    self, BinaryOp, Block, BlockItem, Expr, ExprKind, MergeOp, Paragraph, ProseLine, ProseSegment,
    Property, Sigil, TypeConstraint, TypeName, UnaryOp, ValueNode,
};

use crate::module::ModuleId;
use crate::resolve::Resolution;
use crate::store::{Symbol, VariableId};

/// Where an expression is being evaluated from.
#[derive(Debug, Clone, Copy)]
struct Context {
    module: ModuleId,
    /// The anchor whose body the expression is written in. `this` binds here.
    this: Option<AnchorId>,
    /// The most derived anchor in the chain. `self` binds here.
    derived: Option<AnchorId>,
    /// The right-most base of `this`. `super` binds here.
    super_anchor: Option<AnchorId>,
}

impl Context {
    fn file(module: ModuleId) -> Context {
        Context {
            module,
            this: None,
            derived: None,
            super_anchor: None,
        }
    }
}

/// A value together with whether it was written as a quoted literal. Quoted
/// values are explicitly strings and never coerce to another type.
#[derive(Debug, Clone)]
struct Evaluated {
    value: Value,
    quoted: bool,
}

impl Evaluated {
    fn plain(value: Value) -> Evaluated {
        Evaluated {
            value,
            quoted: false,
        }
    }
}

pub struct Outcome {
    pub anchors: HashMap<AnchorId, Properties>,
    pub variables: HashMap<VariableId, Value>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Evaluates every anchor and variable in the resolution.
pub fn evaluate(resolution: &Resolution) -> Outcome {
    let mut evaluator = Evaluator {
        resolution,
        properties: HashMap::new(),
        property_stack: Vec::new(),
        variables: HashMap::new(),
        variable_stack: Vec::new(),
        anchors: HashMap::new(),
        diagnostics: Vec::new(),
    };

    for index in 0..resolution.store.variables.len() {
        evaluator.variable_value(VariableId(index as u32));
    }
    for index in 0..resolution.store.anchors.len() {
        let id = AnchorId(index as u32);
        let properties = evaluator.anchor_properties(id);
        evaluator.anchors.insert(id, properties);
    }

    Outcome {
        anchors: evaluator.anchors,
        variables: evaluator.variables,
        diagnostics: evaluator.diagnostics,
    }
}

struct Evaluator<'a> {
    resolution: &'a Resolution,
    /// Cache keyed by the derived anchor, because `self` changes what a property
    /// evaluates to.
    properties: HashMap<(AnchorId, String), Value>,
    property_stack: Vec<(AnchorId, String)>,
    variables: HashMap<VariableId, Value>,
    variable_stack: Vec<VariableId>,
    anchors: HashMap<AnchorId, Properties>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Evaluator<'a> {
    fn path(&self, module: ModuleId) -> PathBuf {
        self.resolution.graph.get(module).path.clone()
    }

    fn error(&mut self, code: &str, message: impl Into<String>, module: ModuleId, span: Span) {
        let path = self.path(module);
        self.diagnostics
            .push(Diagnostic::error(code, message, path, span));
    }

    fn warn_with_help(
        &mut self,
        code: &str,
        message: impl Into<String>,
        help: impl Into<String>,
        module: ModuleId,
        span: Span,
    ) {
        let path = self.path(module);
        self.diagnostics
            .push(Diagnostic::warning(code, message, path, span).with_help(help));
    }

    // -- anchors --------------------------------------------------------

    fn anchor_properties(&mut self, anchor: AnchorId) -> Properties {
        if let Some(existing) = self.anchors.get(&anchor) {
            return existing.clone();
        }
        let names: Vec<String> = self
            .resolution
            .store
            .anchor(anchor)
            .slots
            .keys()
            .cloned()
            .collect();
        let mut properties = Properties::new();
        for name in names {
            // An abstract slot declares a shape, not a value. It contributes
            // ordering and a constraint, and nothing to the output.
            let declared = self
                .resolution
                .store
                .anchor(anchor)
                .slots
                .get(&name)
                .is_some_and(|slot| slot.has_value);
            if !declared {
                continue;
            }
            let value = self.property_value(anchor, &name);
            properties.insert(name, value);
        }
        properties
    }

    fn property_value(&mut self, anchor: AnchorId, name: &str) -> Value {
        let key = (anchor, name.to_string());
        if let Some(cached) = self.properties.get(&key) {
            return cached.clone();
        }
        if self.property_stack.contains(&key) {
            let def = self.resolution.store.anchor(anchor);
            let cycle: Vec<String> = self
                .property_stack
                .iter()
                .map(|(a, n)| format!("{}.{n}", self.resolution.store.anchor(*a).name))
                .collect();
            let message = format!(
                "`{}.{name}` depends on its own value ({})",
                def.name,
                cycle.join(" -> ")
            );
            let (module, span) = (def.module, def.name_span);
            self.error("cyclic-value", message, module, span);
            return Value::Null;
        }

        let Some(slot) = self
            .resolution
            .store
            .anchor(anchor)
            .slots
            .get(name)
            .cloned()
        else {
            return Value::Null;
        };
        if !slot.has_value {
            // Reading an unimplemented abstract slot is already reported where
            // the concrete anchor is declared; do not pile a type error on top.
            return Value::Null;
        }

        let owner = self.resolution.store.anchor(slot.owner);
        let owner_module = owner.module;
        let owner_item = owner.item;
        let super_anchor = owner.bases.last().copied();
        let Some(ast::Item::Anchor(decl)) = self
            .resolution
            .graph
            .get(owner_module)
            .ast()
            .items
            .get(owner_item)
        else {
            return Value::Null;
        };
        let Some(property) = decl.body.properties().nth(slot.index).cloned() else {
            return Value::Null;
        };

        let context = Context {
            module: owner_module,
            this: Some(slot.owner),
            derived: Some(anchor),
            super_anchor,
        };

        self.property_stack.push(key.clone());
        let evaluated = self.value_node(&property.value, &context);
        let value = self.constrain(
            evaluated,
            &slot.constraints,
            &format!(
                "{}.{name}",
                self.resolution.store.anchor(anchor).name
            ),
            owner_module,
            property.name_span,
        );
        self.property_stack.pop();

        self.properties.insert(key, value.clone());
        value
    }

    fn variable_value(&mut self, id: VariableId) -> Value {
        if let Some(cached) = self.variables.get(&id) {
            return cached.clone();
        }
        if self.variable_stack.contains(&id) {
            let def = self.resolution.store.variable(id);
            let (name, module, span) = (def.name.clone(), def.module, def.name_span);
            self.error(
                "cyclic-value",
                format!("`{name}` depends on its own value"),
                module,
                span,
            );
            return Value::Null;
        }
        let def = self.resolution.store.variable(id).clone();
        let Some(ast::Item::Variable(decl)) =
            self.resolution.graph.get(def.module).ast().items.get(def.item)
        else {
            return Value::Null;
        };
        let decl = decl.clone();

        self.variable_stack.push(id);
        let evaluated = self.value_node(&decl.value, &Context::file(def.module));
        let value = self.constrain(
            evaluated,
            &def.constraints,
            &def.name,
            def.module,
            decl.name_span,
        );
        self.variable_stack.pop();

        self.variables.insert(id, value.clone());
        value
    }

    // -- values ---------------------------------------------------------

    fn value_node(&mut self, node: &ValueNode, context: &Context) -> Evaluated {
        if let Some(items) = &node.inline_list {
            let values = items
                .iter()
                .map(|item| self.value_node(item, context).value)
                .collect();
            return Evaluated::plain(Value::List(values));
        }

        let mut pieces: Vec<Piece> = Vec::new();
        if let Some(block) = &node.block {
            collect_pieces(block, &mut pieces);
        }

        if let Some(inline) = &node.inline {
            if !inline.is_blank() {
                match pieces.first_mut() {
                    // A same-line fragment and the indented prose under it are
                    // one paragraph, so `description: :` followed by text reads
                    // as `: text`.
                    Some(Piece::Prose(lines)) => lines.insert(0, inline.clone()),
                    _ => pieces.insert(0, Piece::Prose(vec![inline.clone()])),
                }
            }
        }

        self.pieces_to_value(&pieces, context, node.span)
    }

    fn pieces_to_value(
        &mut self,
        pieces: &[Piece],
        context: &Context,
        span: Span,
    ) -> Evaluated {
        let has_prose = pieces
            .iter()
            .any(|p| matches!(p, Piece::Prose(_) | Piece::Fence(_) | Piece::Escape(_)));
        let has_list = pieces
            .iter()
            .any(|p| matches!(p, Piece::ListItem(_) | Piece::Merge(..)));
        let has_property = pieces.iter().any(|p| matches!(p, Piece::Property(_)));

        match (has_prose, has_list, has_property) {
            (false, false, false) => Evaluated::plain(Value::Null),
            (true, false, false) => self.prose_value(pieces, context),
            (false, true, false) => {
                Evaluated::plain(Value::List(self.list_value(pieces, context, span)))
            }
            (false, false, true) => {
                let mut map = Properties::new();
                for piece in pieces {
                    if let Piece::Property(property) = piece {
                        let value = self.property_in_place(property, context);
                        map.insert(property.name.clone(), value);
                    }
                }
                Evaluated::plain(Value::Dict(map))
            }
            // Mixing kinds produces an implicit list. Dictionary keys declared
            // directly in the block stay addressable, so they are kept as named
            // entries rather than folded into an anonymous map.
            _ => {
                let mut items: Vec<MixedItem> = Vec::new();
                let mut run: Vec<Piece> = Vec::new();
                let mut list_run: Vec<Piece> = Vec::new();

                macro_rules! flush_prose {
                    () => {
                        if !run.is_empty() {
                            let value = self.prose_value(&run, context).value;
                            if let Value::Str(text) = value {
                                items.push(MixedItem::Text(text));
                            }
                            run.clear();
                        }
                    };
                }
                macro_rules! flush_list {
                    () => {
                        if !list_run.is_empty() {
                            let values = self.list_value(&list_run, context, span);
                            items.push(MixedItem::List(values));
                            list_run.clear();
                        }
                    };
                }

                for piece in pieces {
                    match piece {
                        Piece::Prose(_) | Piece::Fence(_) | Piece::Escape(_) => {
                            flush_list!();
                            run.push(piece.clone());
                        }
                        Piece::ListItem(_) | Piece::Merge(..) => {
                            flush_prose!();
                            list_run.push(piece.clone());
                        }
                        Piece::Property(property) => {
                            flush_prose!();
                            flush_list!();
                            let value = self.property_in_place(property, context);
                            items.push(MixedItem::Entry(property.name.clone(), value));
                        }
                    }
                }
                flush_prose!();
                flush_list!();
                Evaluated::plain(Value::Mixed(Mixed::new(items)))
            }
        }
    }

    /// Evaluates a property declared inside a value block, applying its own
    /// constraints.
    fn property_in_place(&mut self, property: &Property, context: &Context) -> Value {
        let evaluated = self.value_node(&property.value, context);
        self.constrain(
            evaluated,
            &property.constraints,
            &property.name,
            context.module,
            property.name_span,
        )
    }

    fn list_value(&mut self, pieces: &[Piece], context: &Context, span: Span) -> Vec<Value> {
        let mut items: Vec<Value> = Vec::new();
        for piece in pieces {
            match piece {
                Piece::ListItem(item) => {
                    let value = self.value_node(&item.value, context);
                    match value.value {
                        // `- text` with an indented list under it contributes
                        // the text and the nested list as siblings, which is how
                        // `[Level 1, [Level 2]]` is written in block form.
                        Value::Mixed(mixed) => {
                            for entry in split_nested_list(mixed) {
                                items.push(entry);
                            }
                        }
                        other => items.push(other),
                    }
                }
                Piece::Merge(op, merge) => {
                    let operand = self.prose_line_value(&merge.value, context);
                    let mut extra = match &operand.value {
                        Value::List(values) => values.clone(),
                        Value::Mixed(_) => operand.value.as_list_items(),
                        Value::Null => Vec::new(),
                        other => vec![other.clone()],
                    };
                    items.append(&mut extra);
                    if *op == MergeOp::Merge {
                        // The merge operator concatenates in operand order and
                        // then removes duplicates, keeping the last occurrence.
                        items = dedup_keep_last(items);
                    }
                }
                _ => {}
            }
        }
        let _ = span;
        items
    }

    fn prose_value(&mut self, pieces: &[Piece], context: &Context) -> Evaluated {
        // A value written as exactly one interpolation keeps its intrinsic type.
        if let [Piece::Prose(lines)] = pieces {
            if lines.len() == 1 {
                if let Some(interpolation) = lines[0].sole_interpolation() {
                    return self.interpolation_value(interpolation, context);
                }
            }
        }

        // Each paragraph is considered on its own, because a paragraph that
        // *is* a quoted string is a quoted literal whose quotes are syntax,
        // while quotes inside a sentence are ordinary punctuation.
        let mut paragraphs: Vec<Text> = Vec::new();
        let mut quoted_paragraphs: Vec<bool> = Vec::new();
        for piece in pieces {
            match piece {
                Piece::Prose(lines) => {
                    let mut text = Text::empty();
                    for (index, line) in lines.iter().enumerate() {
                        if index > 0 {
                            text.push_literal(" ");
                        }
                        text.push_text(&self.prose_line_text(line, context));
                    }
                    let trimmed = text.trim();
                    match unquote_literal(&trimmed) {
                        Some(inner) => {
                            paragraphs.push(inner);
                            quoted_paragraphs.push(true);
                        }
                        None => {
                            paragraphs.push(trimmed);
                            quoted_paragraphs.push(false);
                        }
                    }
                }
                Piece::Fence(fence) => {
                    let mut text = Text::empty();
                    let ticks = "`".repeat(fence.ticks.max(3));
                    text.push_literal(format!("{ticks}{}\n", fence.info));
                    for line in &fence.lines {
                        text.push_literal(line);
                        text.push_literal("\n");
                    }
                    text.push_literal(ticks);
                    paragraphs.push(text);
                    quoted_paragraphs.push(false);
                }
                Piece::Escape(block) => {
                    // The delimiters were consumed by the parser; what is left
                    // is taken exactly as written, with no interpolation and no
                    // literal inference.
                    let mut text = Text::empty();
                    for (index, line) in block.lines.iter().enumerate() {
                        if index > 0 {
                            text.push_literal("\n");
                        }
                        text.push_literal(line);
                    }
                    paragraphs.push(text);
                    quoted_paragraphs.push(false);
                }
                _ => {}
            }
        }

        let mut combined = Text::empty();
        for (index, paragraph) in paragraphs.iter().enumerate() {
            if index > 0 {
                // A blank line in the source is how an explicit line break is
                // written, so paragraphs join with a newline rather than a
                // space.
                combined.push_literal("\n");
            }
            combined.push_text(paragraph);
        }

        // A value that is nothing but a quoted literal is explicitly a string
        // and never coerces, so it skips literal inference entirely.
        if paragraphs.len() == 1 && quoted_paragraphs[0] {
            return Evaluated {
                value: Value::Str(combined),
                quoted: true,
            };
        }

        let single_line = paragraphs.len() <= 1
            && combined
                .as_plain()
                .is_none_or(|text| !text.contains('\n'));
        infer(combined, single_line)
    }

    /// Renders one prose line as text, stringifying interpolations.
    fn prose_line_text(&mut self, line: &ProseLine, context: &Context) -> Text {
        let mut text = Text::empty();
        for segment in &line.segments {
            match segment {
                ProseSegment::Text(raw) | ProseSegment::Literal(raw) => text.push_literal(raw),
                ProseSegment::Interpolation(interpolation) => {
                    let evaluated = self.interpolation_value(interpolation, context);
                    match evaluated.value {
                        Value::Str(inner) => text.push_text(&inner),
                        Value::Reference(id) => text.push_reference(id),
                        other => {
                            let rendered =
                                self.stringify(&other, context, interpolation.span);
                            text.push_text(&rendered);
                        }
                    }
                }
            }
        }
        text
    }

    fn prose_line_value(&mut self, line: &ProseLine, context: &Context) -> Evaluated {
        if let Some(interpolation) = line.sole_interpolation() {
            return self.interpolation_value(interpolation, context);
        }
        let text = self.prose_line_text(line, context);
        infer(text.trim(), true)
    }

    fn interpolation_value(
        &mut self,
        interpolation: &ast::Interpolation,
        context: &Context,
    ) -> Evaluated {
        let value = self.expr(&interpolation.expr, context);
        match interpolation.sigil {
            Sigil::Standard => value,
            Sigil::Stringify => {
                let text = self.stringify(&value.value, context, interpolation.span);
                Evaluated {
                    value: Value::Str(text),
                    quoted: true,
                }
            }
            Sigil::Numeric => {
                let number = self.numeric(&value.value, context, interpolation.span);
                Evaluated::plain(number)
            }
            Sigil::Reference => Evaluated::plain(self.reference(
                &value.value,
                context,
                interpolation.span,
            )),
        }
    }

    // -- expressions ----------------------------------------------------

    fn expr(&mut self, expr: &Expr, context: &Context) -> Evaluated {
        match &expr.kind {
            ExprKind::Number(n) => Evaluated::plain(Value::Number(*n)),
            ExprKind::Bool(b) => Evaluated::plain(Value::Bool(*b)),
            ExprKind::Null => Evaluated::plain(Value::Null),
            ExprKind::Quoted(text) => Evaluated {
                value: Value::Str(Text::plain(text.clone())),
                quoted: true,
            },
            ExprKind::Error => Evaluated::plain(Value::Null),
            ExprKind::Paren(inner) => self.expr(inner, context),
            ExprKind::This => match context.this {
                Some(anchor) => Evaluated::plain(Value::Anchor(anchor)),
                None => {
                    self.error(
                        "invalid-this",
                        "`this` is only meaningful inside an anchor",
                        context.module,
                        expr.span,
                    );
                    Evaluated::plain(Value::Null)
                }
            },
            ExprKind::SelfRef => match context.derived {
                Some(anchor) => Evaluated::plain(Value::Anchor(anchor)),
                None => {
                    self.error(
                        "invalid-self",
                        "`self` is only meaningful inside an anchor",
                        context.module,
                        expr.span,
                    );
                    Evaluated::plain(Value::Null)
                }
            },
            ExprKind::Super => match context.super_anchor {
                Some(anchor) => Evaluated::plain(Value::Anchor(anchor)),
                None => {
                    self.error(
                        "invalid-super",
                        "`super` needs a base anchor; this anchor does not extend anything",
                        context.module,
                        expr.span,
                    );
                    Evaluated::plain(Value::Null)
                }
            },
            ExprKind::Name(name) => match self.resolution.lookup(context.module, name) {
                Some(Symbol::Anchor(anchor)) => Evaluated::plain(Value::Anchor(anchor)),
                Some(Symbol::Variable(variable)) => {
                    Evaluated::plain(self.variable_value(variable))
                }
                None => {
                    self.error(
                        "unresolved-symbol",
                        format!("`{name}` is not in scope"),
                        context.module,
                        expr.span,
                    );
                    Evaluated::plain(Value::Null)
                }
            },
            ExprKind::Field(base, field) => {
                let base_value = self.expr(base, context).value;
                self.field(&base_value, &field.value, context, expr.span)
            }
            ExprKind::List(items) => Evaluated::plain(Value::List(
                items.iter().map(|item| self.expr(item, context).value).collect(),
            )),
            ExprKind::Unary(op, operand) => {
                let value = self.expr(operand, context).value;
                let result = match op {
                    UnaryOp::Not => Value::Bool(!value.is_truthy()),
                    UnaryOp::Negate => match value {
                        Value::Number(n) => Value::Number(-n),
                        other => {
                            self.error(
                                "invalid-operand",
                                format!("`-` needs a number, found {}", other.kind()),
                                context.module,
                                expr.span,
                            );
                            Value::Null
                        }
                    },
                };
                Evaluated::plain(result)
            }
            ExprKind::Ternary(condition, consequent, alternative) => {
                // Only the selected branch is evaluated.
                if self.expr(condition, context).value.is_truthy() {
                    self.expr(consequent, context)
                } else {
                    self.expr(alternative, context)
                }
            }
            ExprKind::Binary(op, lhs, rhs) => {
                let left = self.expr(lhs, context);
                let right = self.expr(rhs, context);
                Evaluated::plain(self.binary(*op, left, right, context, expr.span))
            }
        }
    }

    fn field(
        &mut self,
        base: &Value,
        name: &str,
        context: &Context,
        span: Span,
    ) -> Evaluated {
        match base {
            Value::Anchor(anchor) | Value::Reference(anchor) => {
                if self
                    .resolution
                    .store
                    .anchor(*anchor)
                    .slots
                    .contains_key(name)
                {
                    Evaluated::plain(self.property_value(*anchor, name))
                } else {
                    let anchor_name = self.resolution.store.anchor(*anchor).name.clone();
                    self.error(
                        "unknown-property",
                        format!("`{anchor_name}` has no property `{name}`"),
                        context.module,
                        span,
                    );
                    Evaluated::plain(Value::Null)
                }
            }
            Value::Dict(map) => match map.get(name) {
                Some(value) => Evaluated::plain(value.clone()),
                None => {
                    self.error(
                        "unknown-property",
                        format!("no key `{name}` in this dictionary"),
                        context.module,
                        span,
                    );
                    Evaluated::plain(Value::Null)
                }
            },
            Value::Mixed(mixed) => match mixed.get(name) {
                Some(value) => Evaluated::plain(value.clone()),
                None => {
                    self.error(
                        "unknown-property",
                        format!("no key `{name}` in this block"),
                        context.module,
                        span,
                    );
                    Evaluated::plain(Value::Null)
                }
            },
            Value::List(_) => {
                self.error(
                    "list-access",
                    "lists do not support member access; a list is for merging, not indexing",
                    context.module,
                    span,
                );
                Evaluated::plain(Value::Null)
            }
            other => {
                self.error(
                    "invalid-access",
                    format!("cannot read `{name}` from {}", other.kind()),
                    context.module,
                    span,
                );
                Evaluated::plain(Value::Null)
            }
        }
    }

    fn binary(
        &mut self,
        op: BinaryOp,
        left: Evaluated,
        right: Evaluated,
        context: &Context,
        span: Span,
    ) -> Value {
        use BinaryOp::*;
        match op {
            And => Value::Bool(left.value.is_truthy() && right.value.is_truthy()),
            Or => Value::Bool(left.value.is_truthy() || right.value.is_truthy()),
            Equal => Value::Bool(equal(&left.value, &right.value)),
            NotEqual => Value::Bool(!equal(&left.value, &right.value)),
            Less | LessEqual | Greater | GreaterEqual => {
                match compare(&left.value, &right.value) {
                    Some(ordering) => Value::Bool(match op {
                        Less => ordering.is_lt(),
                        LessEqual => ordering.is_le(),
                        Greater => ordering.is_gt(),
                        _ => ordering.is_ge(),
                    }),
                    None => {
                        self.error(
                            "invalid-operand",
                            format!(
                                "cannot compare {} with {}",
                                left.value.kind(),
                                right.value.kind()
                            ),
                            context.module,
                            span,
                        );
                        Value::Null
                    }
                }
            }
            Subtract | Multiply | Divide | Modulo => {
                match (&left.value, &right.value) {
                    (Value::Number(a), Value::Number(b)) => {
                        if matches!(op, Divide | Modulo) && *b == 0.0 {
                            self.error(
                                "division-by-zero",
                                format!("`{}` by zero", op.symbol()),
                                context.module,
                                span,
                            );
                            return Value::Null;
                        }
                        Value::Number(match op {
                            Subtract => a - b,
                            Multiply => a * b,
                            Divide => a / b,
                            _ => a % b,
                        })
                    }
                    (a, b) => {
                        self.error(
                            "invalid-operand",
                            format!(
                                "`{}` needs two numbers, found {} and {}",
                                op.symbol(),
                                a.kind(),
                                b.kind()
                            ),
                            context.module,
                            span,
                        );
                        Value::Null
                    }
                }
            }
            Add => match (&left.value, &right.value) {
                (Value::Number(a), Value::Number(b)) => Value::Number(a + b),
                (Value::List(_) | Value::Mixed(_), Value::List(_) | Value::Mixed(_)) => {
                    let mut items = left.value.as_list_items();
                    items.extend(right.value.as_list_items());
                    Value::List(dedup_keep_last(items))
                }
                (Value::Dict(a), Value::Dict(b)) => {
                    let mut merged = a.clone();
                    for (key, value) in b {
                        merged.insert(key.clone(), value.clone());
                    }
                    Value::Dict(merged)
                }
                _ => {
                    // Anything else concatenates as text, which is how
                    // `2 + Hello` becomes `2Hello`.
                    let mut text = self.stringify(&left.value, context, span);
                    let right_text = self.stringify(&right.value, context, span);
                    text.push_text(&right_text);
                    Value::Str(text)
                }
            },
            Concat => match (&left.value, &right.value) {
                (Value::List(_) | Value::Mixed(_), Value::List(_) | Value::Mixed(_)) => {
                    let mut items = left.value.as_list_items();
                    items.extend(right.value.as_list_items());
                    Value::List(items)
                }
                (Value::Dict(a), Value::Dict(b)) => {
                    let mut merged = a.clone();
                    for (key, value) in b {
                        merged.insert(key.clone(), value.clone());
                    }
                    Value::Dict(merged)
                }
                _ => {
                    let mut text = self.stringify(&left.value, context, span);
                    let right_text = self.stringify(&right.value, context, span);
                    text.push_text(&right_text);
                    Value::Str(text)
                }
            },
        }
    }

    // -- conversions ----------------------------------------------------

    fn stringify(&mut self, value: &Value, context: &Context, span: Span) -> Text {
        match value {
            Value::Str(text) => text.clone(),
            Value::Number(n) => Text::plain(format_number(*n)),
            Value::Bool(b) => Text::plain(if *b { "true" } else { "false" }),
            Value::Null => Text::plain("null"),
            // Stringifying an anchor yields its source name.
            Value::Anchor(id) => Text::plain(self.resolution.store.anchor(*id).name.clone()),
            Value::Reference(id) => Text::reference(*id),
            other => {
                self.error(
                    "no-string-form",
                    format!(
                        "{} has no defined string representation",
                        other.kind()
                    ),
                    context.module,
                    span,
                );
                Text::empty()
            }
        }
    }

    fn numeric(&mut self, value: &Value, context: &Context, span: Span) -> Value {
        match value {
            Value::Number(n) => Value::Number(*n),
            Value::Str(text) => match text.as_plain().and_then(parse_number_literal) {
                Some(number) => Value::Number(number),
                None => {
                    self.error(
                        "not-a-number",
                        "the string does not represent a number in its entirety",
                        context.module,
                        span,
                    );
                    Value::Null
                }
            },
            other => {
                self.error(
                    "not-a-number",
                    format!("{} cannot be converted to a number", other.kind()),
                    context.module,
                    span,
                );
                Value::Null
            }
        }
    }

    fn reference(&mut self, value: &Value, context: &Context, span: Span) -> Value {
        match value {
            Value::Anchor(id) | Value::Reference(id) => Value::Reference(*id),
            other => {
                // Reference identity for non-anchor values is listed as an open
                // question in the specification. Rather than fail a build over
                // an unfinished rule, fall back to the value itself and say so.
                //
                // The help matters more than the warning here: `@{...}` asks for
                // a link to a compiled document, and only an anchor has one.
                self.warn_with_help(
                    "reference-not-an-anchor",
                    format!(
                        "`@{{...}}` resolved to {} rather than an anchor, so there is no document to link to; its value is used instead",
                        other.kind()
                    ),
                    reference_help(other),
                    context.module,
                    span,
                );
                other.clone()
            }
        }
    }

    /// Applies type constraints left to right, using the first type the value
    /// can validly represent.
    fn constrain(
        &mut self,
        evaluated: Evaluated,
        constraints: &[TypeConstraint],
        what: &str,
        module: ModuleId,
        span: Span,
    ) -> Value {
        if constraints.is_empty() {
            return evaluated.value;
        }
        for constraint in constraints {
            if let Some(value) = self.coerce(&evaluated, constraint) {
                return value;
            }
        }
        let names: Vec<String> = constraints.iter().map(describe_constraint).collect();
        self.error(
            "type-mismatch",
            format!(
                "`{what}` is {} but is constrained to {}",
                evaluated.value.kind(),
                names.join(" or ")
            ),
            module,
            span,
        );
        evaluated.value
    }

    fn coerce(&self, evaluated: &Evaluated, constraint: &TypeConstraint) -> Option<Value> {
        let value = &evaluated.value;
        if constraint.list {
            let items = match value {
                Value::List(items) => items.clone(),
                Value::Mixed(_) => value.as_list_items(),
                _ => return None,
            };
            let element = TypeConstraint {
                list: false,
                ..constraint.clone()
            };
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(self.coerce(
                    &Evaluated {
                        value: item,
                        quoted: false,
                    },
                    &element,
                )?);
            }
            return Some(Value::List(out));
        }

        match &constraint.name {
            TypeName::Any => Some(value.clone()),
            TypeName::Simple => value.is_simple().then(|| value.clone()),
            TypeName::Complex => value.is_complex().then(|| value.clone()),
            TypeName::String => match value {
                Value::Str(_) => Some(value.clone()),
                // Unquoted literals coerce when their syntax fits the target.
                Value::Number(n) => Some(Value::Str(Text::plain(format_number(*n)))),
                Value::Bool(b) => Some(Value::Str(Text::plain(if *b { "true" } else { "false" }))),
                _ => None,
            },
            TypeName::Number => match value {
                Value::Number(_) => Some(value.clone()),
                Value::Str(text) if !evaluated.quoted => text
                    .as_plain()
                    .and_then(parse_number_literal)
                    .map(Value::Number),
                _ => None,
            },
            TypeName::Boolean => match value {
                Value::Bool(_) => Some(value.clone()),
                Value::Str(text) if !evaluated.quoted => match text.as_plain() {
                    Some("true") => Some(Value::Bool(true)),
                    Some("false") => Some(Value::Bool(false)),
                    _ => None,
                },
                _ => None,
            },
            TypeName::Null => match value {
                Value::Null => Some(Value::Null),
                Value::Str(text) if !evaluated.quoted && text.as_plain() == Some("null") => {
                    Some(Value::Null)
                }
                _ => None,
            },
            TypeName::List => {
                matches!(value, Value::List(_) | Value::Mixed(_)).then(|| value.clone())
            }
            TypeName::Dictionary => {
                matches!(value, Value::Dict(_) | Value::Mixed(_)).then(|| value.clone())
            }
            TypeName::Anchor => matches!(value, Value::Anchor(_)).then(|| value.clone()),
            TypeName::Reference => matches!(value, Value::Reference(_)).then(|| value.clone()),
            TypeName::Named(name) => {
                let Value::Anchor(anchor) = value else {
                    return None;
                };
                let target = self
                    .resolution
                    .store
                    .anchors
                    .iter()
                    .find(|candidate| candidate.name == *name)?;
                if constraint.extends {
                    // `extends A` is satisfied by any anchor whose chain
                    // includes A.
                    self.resolution
                        .store
                        .inherits_from(*anchor, target.id)
                        .then(|| value.clone())
                } else {
                    // A bare anchor name is satisfied by a direct implementer.
                    self.resolution
                        .store
                        .anchor(*anchor)
                        .bases
                        .contains(&target.id)
                        .then(|| value.clone())
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Block pieces
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Piece {
    Prose(Vec<ProseLine>),
    Fence(ast::Fence),
    Escape(ast::EscapeBlock),
    ListItem(ast::ListItem),
    Merge(MergeOp, ast::MergeItem),
    Property(Property),
}

fn collect_pieces(block: &Block, out: &mut Vec<Piece>) {
    for item in &block.items {
        match item {
            BlockItem::Prose(Paragraph { lines, .. }) => out.push(Piece::Prose(lines.clone())),
            BlockItem::Fence(fence) => out.push(Piece::Fence(fence.clone())),
            BlockItem::Escape(block) => out.push(Piece::Escape(block.clone())),
            BlockItem::ListItem(item) => out.push(Piece::ListItem(item.clone())),
            BlockItem::Merge(merge) => out.push(Piece::Merge(merge.op, merge.clone())),
            BlockItem::Property(property) => out.push(Piece::Property(property.clone())),
            BlockItem::Pass(_) => {}
        }
    }
}

/// A list item that carries both text and a nested list contributes them as two
/// siblings of the enclosing list.
fn split_nested_list(mixed: Mixed) -> Vec<Value> {
    let mut out = Vec::new();
    for item in mixed.items {
        match item {
            MixedItem::Text(text) => out.push(Value::Str(text)),
            MixedItem::List(values) => out.push(Value::List(values)),
            MixedItem::Entry(key, value) => {
                let mut map = Properties::new();
                map.insert(key, value);
                out.push(Value::Dict(map));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// What to do about an `@{...}` that did not resolve to an anchor.
fn reference_help(value: &Value) -> String {
    match value {
        // A dictionary usually means the declaration is missing its `anchor`
        // keyword, so it became an exported variable instead.
        Value::Dict(_) | Value::Mixed(_) => {
            "if the target is meant to be an anchor, declare it with `anchor`; only anchors compile to a document that can be linked"
                .to_string()
        }
        // Reading a property gives a value, not the anchor that holds it.
        _ => "`@{...}` links to an anchor's compiled document; to insert this value as text, use `${...}`"
            .to_string(),
    }
}

/// Returns the contents of a text that is entirely one double-quoted literal.
///
/// Text carrying a reference is never a plain literal, so it is left alone.
fn unquote_literal(text: &Text) -> Option<Text> {
    let plain = text.as_plain()?;
    if plain.len() >= 2 && plain.starts_with('"') && plain.ends_with('"') {
        return Some(Text::plain(plain[1..plain.len() - 1].to_string()));
    }
    None
}

/// Applies literal inference to text that was not constrained to a type.
fn infer(text: Text, single_line: bool) -> Evaluated {
    let Some(plain) = text.as_plain() else {
        return Evaluated::plain(Value::Str(text));
    };
    if plain.is_empty() {
        return Evaluated::plain(Value::Null);
    }
    if !single_line {
        return Evaluated::plain(Value::Str(text));
    }
    // A quoted value is explicitly a string and keeps its text without quotes.
    if plain.len() >= 2 && plain.starts_with('"') && plain.ends_with('"') {
        return Evaluated {
            value: Value::Str(Text::plain(plain[1..plain.len() - 1].to_string())),
            quoted: true,
        };
    }
    let value = match plain {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        other => match parse_number_literal(other) {
            Some(number) => Value::Number(number),
            None => Value::Str(text),
        },
    };
    Evaluated::plain(value)
}

/// Parses a number only when the whole string is one, honouring the underscore
/// separators the language allows.
fn parse_number_literal(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut seen_digit = false;
    for (index, ch) in text.char_indices() {
        match ch {
            '0'..='9' => seen_digit = true,
            '_' | '.' => {}
            '-' | '+' if index == 0 => {}
            _ => return None,
        }
    }
    if !seen_digit {
        return None;
    }
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();
    cleaned.parse::<f64>().ok()
}

fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Str(x), Value::Str(y)) => x == y,
        _ => a == b,
    }
}

fn compare(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.partial_cmp(y),
        (Value::Str(x), Value::Str(y)) => match (x.as_plain(), y.as_plain()) {
            (Some(x), Some(y)) => Some(x.cmp(y)),
            _ => None,
        },
        _ => None,
    }
}

/// Concatenation order is preserved and duplicates are removed by keeping the
/// last occurrence, so merging `[A, B, C]` into `[A, B, C, D]` yields
/// `[D, A, B, C]`.
fn dedup_keep_last(items: Vec<Value>) -> Vec<Value> {
    let mut keep = vec![true; items.len()];
    let mut seen: Vec<&Value> = Vec::new();
    for index in (0..items.len()).rev() {
        if seen.iter().any(|value| equal(value, &items[index])) {
            keep[index] = false;
        } else {
            seen.push(&items[index]);
        }
    }
    items
        .iter()
        .enumerate()
        .filter(|(index, _)| keep[*index])
        .map(|(_, value)| value.clone())
        .collect()
}

fn describe_constraint(constraint: &TypeConstraint) -> String {
    let mut out = String::new();
    if constraint.extends {
        out.push_str("extends ");
    }
    out.push_str(constraint.name.as_str());
    if constraint.list {
        out.push_str("[]");
    }
    out
}

/// Exposed so the type checker can describe constraints the same way.
pub fn constraint_label(constraint: &TypeConstraint) -> String {
    describe_constraint(constraint)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number(v: f64) -> Value {
        Value::Number(v)
    }

    #[test]
    fn merge_keeps_the_last_occurrence() {
        let items = vec![
            Value::string("A"),
            Value::string("B"),
            Value::string("C"),
            Value::string("D"),
            Value::string("A"),
            Value::string("B"),
            Value::string("C"),
        ];
        let merged = dedup_keep_last(items);
        let rendered: Vec<String> = merged
            .iter()
            .map(|v| match v {
                Value::Str(t) => t.as_plain().unwrap_or_default().to_string(),
                _ => String::new(),
            })
            .collect();
        assert_eq!(rendered, vec!["D", "A", "B", "C"]);
    }

    #[test]
    fn inference_follows_the_documented_table() {
        assert!(matches!(
            infer(Text::plain("true"), true).value,
            Value::Bool(true)
        ));
        assert!(matches!(
            infer(Text::plain("true story"), true).value,
            Value::Str(_)
        ));
        assert_eq!(infer(Text::plain("42"), true).value, number(42.0));
        assert!(matches!(
            infer(Text::plain("42 things"), true).value,
            Value::Str(_)
        ));
        assert!(matches!(infer(Text::plain("null"), true).value, Value::Null));
        let quoted = infer(Text::plain("\"false\""), true);
        assert!(quoted.quoted);
        assert_eq!(
            quoted.value,
            Value::Str(Text::plain("false"))
        );
    }

    #[test]
    fn multiline_text_is_never_inferred_as_a_literal() {
        assert!(matches!(
            infer(Text::plain("42"), false).value,
            Value::Str(_)
        ));
    }

    #[test]
    fn number_literals_must_be_whole() {
        assert_eq!(parse_number_literal("1_200_000.00"), Some(1_200_000.0));
        assert_eq!(parse_number_literal("42 things"), None);
        assert_eq!(parse_number_literal("2026-09-21"), None);
        assert_eq!(parse_number_literal("0.14"), Some(0.14));
    }
}
