//! Value evaluation.
//!
//! Piton has no runtime. Thus "evaluation" means to reduce a declaration to the
//! value that an adapter will serialize. The evaluator makes values only when a read
//! needs them, and keeps each value for the next read. Anchors resolve by identity.
//! Thus `${A}` inside `B` does not force the properties of `A`. For this reason,
//! circular *references* are correct, but circular *values* stay an error.

use std::collections::HashMap;
use std::path::PathBuf;

use indexmap::IndexSet;
use piton_core::{
    format_number, AnchorId, Diagnostic, Mixed, MixedItem, Properties, Ref, Span, Text, Value,
};
use piton_syntax::ast::{
    self, BinaryOp, Block, BlockItem, Expr, ExprKind, MergeOp, Paragraph, ProseLine, ProseSegment,
    Property, Sigil, TypeConstraint, TypeName, UnaryOp, ValueNode,
};

use crate::module::ModuleId;
use crate::resolve::Resolution;
use crate::store::{Slot, Symbol, VariableId};

/// The location from which the evaluator evaluates an expression.
#[derive(Debug, Clone, Copy)]
struct Context {
    module: ModuleId,
    /// The anchor that has the body that contains the expression. `this` binds here.
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

/// A value and the form in which the source wrote it.
#[derive(Debug, Clone)]
struct Evaluated {
    value: Value,
    /// True for a string that the source marks as a string. This is a `"..."` inside an
    /// expression, or the result of `${...}`. It never coerces to a different type.
    quoted: bool,
    /// The source text of a literal, if the evaluator inferred the value from one.
    ///
    /// A coerced literal keeps its source text exactly. Thus `x:: string: 1.0` is
    /// "1.0", but the number that the evaluator read prints as `1`.
    literal: Option<String>,
}

impl Evaluated {
    fn plain(value: Value) -> Evaluated {
        Evaluated {
            value,
            quoted: false,
            literal: None,
        }
    }

    fn quoted(value: Value) -> Evaluated {
        Evaluated {
            value,
            quoted: true,
            literal: None,
        }
    }
}

/// A value that evaluation can read. This is a property as one anchor sees it, or a
/// top-level variable.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Read {
    Property(AnchorId, String),
    Variable(VariableId),
}

/// The properties and variables that each property and variable read during its
/// evaluation, in the order of the reads.
///
/// Evaluation folds `${Other.name}` into plain text. After evaluation, the
/// value does not show its origin. Some tools must know the dependencies of a
/// declaration, for example `piton slice`. These tools use this record. They do not
/// guess from the result.
pub type Reads = HashMap<Read, IndexSet<Read>>;

pub struct Outcome {
    pub anchors: HashMap<AnchorId, Properties>,
    pub variables: HashMap<VariableId, Value>,
    pub reads: Reads,
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
        reads: Reads::new(),
        frames: Vec::new(),
        muted: Vec::new(),
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
        reads: evaluator.reads,
        diagnostics: evaluator.diagnostics,
    }
}

struct Evaluator<'a> {
    resolution: &'a Resolution,
    /// A cache with the derived anchor as its key. The reason is that `self` changes
    /// the value of a property.
    properties: HashMap<(AnchorId, String), Value>,
    property_stack: Vec<(AnchorId, String)>,
    variables: HashMap<VariableId, Value>,
    variable_stack: Vec<VariableId>,
    anchors: HashMap<AnchorId, Properties>,
    reads: Reads,
    /// The items that the evaluator evaluates now, with the innermost item last. Thus
    /// each read goes to the item that asked for it.
    frames: Vec<Read>,
    /// Frame depths at which the evaluator does not record reads. The reads at these
    /// depths only find the target of a reference.
    muted: Vec<usize>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Evaluator<'a> {
    /// Records that the current item read `read`.
    fn note(&mut self, read: Read) {
        if self.muted.last() == Some(&self.frames.len()) {
            return;
        }
        if let Some(frame) = self.frames.last() {
            if *frame != read {
                self.reads.entry(frame.clone()).or_default().insert(read);
            }
        }
    }

    fn path(&self, module: ModuleId) -> PathBuf {
        self.resolution.graph.get(module).path.clone()
    }

    fn error(&mut self, code: &str, message: impl Into<String>, module: ModuleId, span: Span) {
        let path = self.path(module);
        self.diagnostics
            .push(Diagnostic::error(code, message, path, span));
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
            // An abstract slot declares a shape, not a value. It gives an
            // order and a constraint, but it adds no output.
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
        self.note(Read::Property(anchor, name.to_string()));
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
            // The concrete anchor declaration reports a read of an
            // abstract slot without an implementation. Do not add a type error to it.
            return Value::Null;
        }

        self.property_stack.push(key.clone());
        self.frames.push(Read::Property(anchor, name.to_string()));
        let value = self.slot_value(anchor, &slot, name);
        self.frames.pop();
        self.property_stack.pop();

        self.properties.insert(key, value.clone());
        value
    }

    /// Evaluates the declaration of a slot, with `self` bound to
    /// `derived`.
    ///
    /// Usually this is the slot to which `derived` resolved. But `super.x`
    /// reads the declaration of `x` in a base, while `self` still means the start
    /// anchor of the lookup. The reason is that `self` goes through inheritance.
    fn slot_value(&mut self, derived: AnchorId, slot: &Slot, name: &str) -> Value {
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
            derived: Some(derived),
            super_anchor,
        };

        let evaluated = self.value_node(&property.value, &context);
        self.constrain(
            evaluated,
            &slot.constraints,
            &format!(
                "{}.{name}",
                self.resolution.store.anchor(derived).name
            ),
            owner_module,
            property.name_span,
        )
    }

    /// Reads `name` through `super`. This is all that the anchor inherits, merged
    /// from left to right. The last item wins. Thus the read also finds a property
    /// that only an earlier base has.
    fn super_field(&mut self, context: &Context, name: &str, span: Span) -> Evaluated {
        let Some(this) = context.this else {
            self.error(
                "invalid-super",
                "`super` is only meaningful inside an anchor",
                context.module,
                span,
            );
            return Evaluated::plain(Value::Null);
        };
        let bases = self.resolution.store.anchor(this).bases.clone();
        if bases.is_empty() {
            self.error(
                "invalid-super",
                "`super` needs a base anchor; this anchor does not extend anything",
                context.module,
                span,
            );
            return Evaluated::plain(Value::Null);
        }
        for base in bases.iter().rev() {
            let Some(slot) = self.resolution.store.anchor(*base).slots.get(name).cloned() else {
                continue;
            };
            if !slot.has_value {
                continue;
            }
            self.note(Read::Property(slot.owner, name.to_string()));
            let derived = context.derived.unwrap_or(this);
            let key = (derived, format!("{name}\u{0}super\u{0}{}", slot.owner.0));
            if self.property_stack.contains(&key) {
                let this_name = self.resolution.store.anchor(this).name.clone();
                self.error(
                    "cyclic-value",
                    format!("`{this_name}.{name}` depends on its own value through `super`"),
                    context.module,
                    span,
                );
                return Evaluated::plain(Value::Null);
            }
            self.property_stack.push(key);
            let value = self.slot_value(derived, &slot, name);
            self.property_stack.pop();
            return Evaluated::plain(value);
        }
        let this_name = self.resolution.store.anchor(this).name.clone();
        self.error(
            "unknown-property",
            format!("none of the anchors `{this_name}` extends have a property `{name}`"),
            context.module,
            span,
        );
        Evaluated::plain(Value::Null)
    }

    fn variable_value(&mut self, id: VariableId) -> Value {
        self.note(Read::Variable(id));
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

        if !decl.value.declared {
            // This is `x:: number` with no value. Only an abstract anchor can have a
            // property without a value.
            self.error(
                "missing-value",
                format!(
                    "`{}` has a type constraint but no value; only a property in an abstract anchor can leave its value out",
                    decl.name
                ),
                def.module,
                decl.name_span,
            );
            self.variables.insert(id, Value::Null);
            return Value::Null;
        }

        self.variable_stack.push(id);
        self.frames.push(Read::Variable(id));
        let evaluated = self.value_node(&decl.value, &Context::file(def.module));
        let value = self.constrain(
            evaluated,
            &def.constraints,
            &def.name,
            def.module,
            decl.name_span,
        );
        self.frames.pop();
        self.variable_stack.pop();

        self.variables.insert(id, value.clone());
        value
    }

    // -- values ---------------------------------------------------------

    fn value_node(&mut self, node: &ValueNode, context: &Context) -> Evaluated {
        if node.declared && node.is_empty() {
            self.error(
                "empty-value",
                "a property has to have a value; write `null` if you mean nothing",
                context.module,
                node.span,
            );
            return Evaluated::plain(Value::Null);
        }
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
                    // A fragment on the same line and the indented prose under it are
                    // one paragraph. Thus `description: :` with text after it reads
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
        if pieces.iter().any(|piece| matches!(piece, Piece::Merge(..))) {
            return self.merged_block(pieces, context, span);
        }
        let has_prose = pieces
            .iter()
            .any(|p| matches!(p, Piece::Prose(_) | Piece::Escape(_)));
        let has_list = pieces.iter().any(|p| matches!(p, Piece::ListItem(_)));
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
            // A mix of kinds gives an implicit list. Dictionary keys that the block
            // declares directly stay addressable. Thus they stay as named
            // entries. They do not go into an anonymous map.
            _ => {
                let mut items: Vec<MixedItem> = Vec::new();
                let mut run: Vec<Piece> = Vec::new();
                let mut list_run: Vec<Piece> = Vec::new();

                macro_rules! flush_prose {
                    () => {
                        if !run.is_empty() {
                            match self.prose_value(&run, context).value {
                                Value::Str(text) => items.push(MixedItem::Text(text)),
                                Value::Mixed(mixed) => items.extend(mixed.items),
                                Value::Null => {}
                                other => items.push(MixedItem::Value(other)),
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
                        Piece::Prose(_) | Piece::Escape(_) => {
                            flush_list!();
                            run.push(piece.clone());
                        }
                        Piece::ListItem(_) => {
                            flush_prose!();
                            list_run.push(piece.clone());
                        }
                        Piece::Merge(..) => unreachable!("merged blocks are handled first"),
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

    /// Builds a block that has `+` or `++` lines in it.
    ///
    /// The evaluator builds blocks from top to bottom. A line can start with `+` or `++`.
    /// This line takes all the lines above it and combines them with its value. This
    /// is the same as the operator in an expression. Lines after it continue to add
    /// to the result. Thus `+ {super.items}` and then `- D` give
    /// the items of the base and then D.
    fn merged_block(&mut self, pieces: &[Piece], context: &Context, span: Span) -> Evaluated {
        let mut accumulated: Option<Value> = None;
        let mut segment: Vec<Piece> = Vec::new();
        for piece in pieces {
            match piece {
                Piece::Merge(op, merge) => {
                    if !segment.is_empty() {
                        let value = self.pieces_to_value(&segment, context, span).value;
                        accumulated = Some(continue_block(accumulated, value));
                        segment.clear();
                    }
                    let operand = self.prose_line_value(&merge.value, context).value;
                    accumulated = Some(match accumulated {
                        // A block that contains only `+ {x}` is the same as `{x}`.
                        None => operand,
                        Some(previous) => {
                            let operator = match op {
                                MergeOp::Merge => BinaryOp::Add,
                                MergeOp::Concat => BinaryOp::Concat,
                            };
                            let operand = match (&previous, operand) {
                                // A merge of no value into a list gives the same list.
                                (Value::List(_) | Value::Mixed(_), Value::Null) => {
                                    Value::List(Vec::new())
                                }
                                // A merge of a single value into a list adds one item.
                                (Value::List(_) | Value::Mixed(_), other)
                                    if !matches!(other, Value::List(_) | Value::Mixed(_)) =>
                                {
                                    Value::List(vec![other])
                                }
                                (_, other) => other,
                            };
                            self.binary(
                                operator,
                                Evaluated::plain(previous),
                                Evaluated::plain(operand),
                                context,
                                merge.span,
                            )
                        }
                    });
                }
                other => segment.push(other.clone()),
            }
        }
        if !segment.is_empty() {
            let value = self.pieces_to_value(&segment, context, span).value;
            accumulated = Some(continue_block(accumulated, value));
        }
        Evaluated::plain(accumulated.unwrap_or(Value::Null))
    }

    /// Evaluates a property that a value block declares, and applies the
    /// constraints of that property.
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
                        // `- text` with an indented list under it gives the text
                        // and the nested list as siblings. This is the block form of
                        // `[Level 1, [Level 2]]`.
                        Value::Mixed(mixed) => {
                            for entry in split_nested_list(mixed) {
                                items.push(entry);
                            }
                        }
                        other => items.push(other),
                    }
                }
                _ => {}
            }
        }
        let _ = span;
        items
    }

    fn prose_value(&mut self, pieces: &[Piece], context: &Context) -> Evaluated {
        // A value that is exactly one interpolation keeps its intrinsic type.
        if let [Piece::Prose(lines)] = pieces {
            if lines.len() == 1 {
                if let Some(interpolation) = lines[0].sole_interpolation() {
                    return self.interpolation_value(interpolation, context);
                }
            }
        }

        let mut paragraphs: Vec<Vec<Part>> = Vec::new();
        for piece in pieces {
            match piece {
                Piece::Prose(lines) => {
                    let mut parts = Vec::new();
                    for (index, line) in lines.iter().enumerate() {
                        if index > 0 {
                            parts.push(Part::Text(Text::plain(" ")));
                        }
                        parts.extend(self.prose_line_parts(line, context));
                    }
                    if parts.iter().any(|part| matches!(part, Part::Value(_))) {
                        paragraphs.push(parts);
                        continue;
                    }
                    paragraphs.push(vec![Part::Text(parts_text(parts).trim())]);
                }
                Piece::Escape(block) => {
                    // The parser consumed the delimiters. The evaluator takes the
                    // other text exactly as it is, with no interpolation and no
                    // literal inference.
                    let mut text = Text::empty();
                    for (index, line) in block.lines.iter().enumerate() {
                        if index > 0 {
                            text.push_literal("\n");
                        }
                        text.push_literal(line);
                    }
                    paragraphs.push(vec![Part::Text(text)]);
                }
                _ => {}
            }
        }

        // A list, dictionary, or anchor in text with `{x}` changes the
        // full value into an implicit list. The list holds the text around it, and the value.
        if paragraphs.iter().flatten().any(|part| matches!(part, Part::Value(_))) {
            let mut items: Vec<MixedItem> = Vec::new();
            let mut buffer = Text::empty();
            for (index, paragraph) in paragraphs.into_iter().enumerate() {
                if index > 0 {
                    buffer.push_literal("\n");
                }
                for part in paragraph {
                    match part {
                        Part::Text(text) => buffer.push_text(&text),
                        Part::Value(value) => {
                            push_mixed(&mut items, Value::Str(std::mem::take(&mut buffer).trim()));
                            push_mixed(&mut items, value);
                        }
                    }
                }
            }
            push_mixed(&mut items, Value::Str(buffer.trim()));
            return Evaluated::plain(Value::Mixed(Mixed::new(items)));
        }

        let count = paragraphs.len();
        let mut combined = Text::empty();
        for (index, paragraph) in paragraphs.into_iter().enumerate() {
            if index > 0 {
                // A blank line in the source gives a hard line break.
                // Thus paragraphs join with a newline, not with a
                // space.
                combined.push_literal("\n");
            }
            combined.push_text(&parts_text(paragraph));
        }

        let single_line = count <= 1
            && combined
                .as_plain()
                .is_none_or(|text| !text.contains('\n'));
        infer(combined, single_line)
    }

    /// Splits one prose line into text and the values that `{x}` puts into it.
    ///
    /// A simple value goes into the text. A list, dictionary, or anchor
    /// stays whole, because it changes the text around it into an implicit list.
    fn prose_line_parts(&mut self, line: &ProseLine, context: &Context) -> Vec<Part> {
        let mut parts = Vec::new();
        for segment in &line.segments {
            match segment {
                ProseSegment::Text(raw) | ProseSegment::Literal(raw) => {
                    parts.push(Part::Text(Text::plain(raw.clone())));
                }
                ProseSegment::Interpolation(interpolation) => {
                    let evaluated = self.interpolation_value(interpolation, context);
                    match evaluated.value {
                        Value::Str(inner) => parts.push(Part::Text(inner)),
                        Value::Reference(target) => parts.push(Part::Text(Text::reference(target))),
                        value @ (Value::List(_)
                        | Value::Mixed(_)
                        | Value::Dict(_)
                        | Value::Anchor(_)) => parts.push(Part::Value(value)),
                        other => {
                            let rendered = self.stringify(&other, context, interpolation.span);
                            parts.push(Part::Text(rendered));
                        }
                    }
                }
            }
        }
        parts
    }

    fn prose_line_value(&mut self, line: &ProseLine, context: &Context) -> Evaluated {
        if let Some(interpolation) = line.sole_interpolation() {
            return self.interpolation_value(interpolation, context);
        }
        self.prose_value(&[Piece::Prose(vec![line.clone()])], context)
    }

    fn interpolation_value(
        &mut self,
        interpolation: &ast::Interpolation,
        context: &Context,
    ) -> Evaluated {
        self.sigil_value(interpolation.sigil, &interpolation.expr, context, interpolation.span)
    }

    /// Applies the operation of a sigil to the expression inside it.
    fn sigil_value(&mut self, sigil: Sigil, expr: &Expr, context: &Context, span: Span) -> Evaluated {
        match sigil {
            Sigil::Standard => self.expr(expr, context),
            Sigil::Stringify => {
                let value = self.expr(expr, context).value;
                let text = match &value {
                    // A named list or dictionary becomes its name, for example
                    // `Anchor.propertyName`. At the top of a file, it becomes only
                    // the variable name.
                    Value::List(_) | Value::Mixed(_) | Value::Dict(_) => {
                        match self.expr_name(expr, context) {
                            Some(name) => Text::plain(name),
                            None => {
                                self.error(
                                    "no-string-form",
                                    format!(
                                        "this {} has no name, so it has no string form",
                                        value.kind()
                                    ),
                                    context.module,
                                    span,
                                );
                                Text::empty()
                            }
                        }
                    }
                    _ => self.stringify(&value, context, span),
                };
                Evaluated::quoted(Value::Str(text))
            }
            Sigil::Numeric => {
                let value = self.expr(expr, context).value;
                Evaluated::plain(self.numeric(&value, context, span))
            }
            Sigil::Reference => match self.reference_target(expr, context) {
                Some(target) => Evaluated::plain(Value::Reference(target)),
                None => {
                    let path = self.path(context.module);
                    self.diagnostics.push(
                        Diagnostic::error(
                            "invalid-reference",
                            "`@{...}` has to point at an anchor or a property on one",
                            path,
                            span,
                        )
                        .with_help("to put a value into the text instead, use `${...}` or `{...}`"),
                    );
                    Evaluated::plain(Value::Null)
                }
            },
        }
    }

    /// The name of a list or dictionary, for `${...}`.
    fn expr_name(&mut self, expr: &Expr, context: &Context) -> Option<String> {
        match &expr.kind {
            ExprKind::Paren(inner) => self.expr_name(inner, context),
            ExprKind::Name(name) => Some(name.clone()),
            ExprKind::Field(base, field) => {
                let base_name = match &base.kind {
                    ExprKind::This => context.this.map(|a| self.anchor_name(a)),
                    ExprKind::SelfRef => context.derived.map(|a| self.anchor_name(a)),
                    ExprKind::Super => context.super_anchor.map(|a| self.anchor_name(a)),
                    _ => self.expr_name(base, context),
                }?;
                Some(format!("{base_name}.{}", field.value))
            }
            _ => None,
        }
    }

    fn anchor_name(&self, anchor: AnchorId) -> String {
        self.resolution.store.anchor(anchor).name.clone()
    }

    /// Finds the target of `@{...}` and does not copy it. The target is an anchor, or a
    /// property on an anchor.
    fn reference_target(&mut self, expr: &Expr, context: &Context) -> Option<Ref> {
        match &expr.kind {
            ExprKind::Paren(inner) | ExprKind::Nested(Sigil::Reference, inner) => {
                self.reference_target(inner, context)
            }
            ExprKind::This => context.this.map(Ref::anchor),
            ExprKind::SelfRef => context.derived.map(Ref::anchor),
            ExprKind::Super => context.super_anchor.map(Ref::anchor),
            ExprKind::Name(name) => match self.resolution.lookup(context.module, name) {
                Some(Symbol::Anchor(anchor)) => Some(Ref::anchor(anchor)),
                Some(Symbol::Variable(variable)) => match self.variable_value(variable) {
                    Value::Anchor(anchor) => Some(Ref::anchor(anchor)),
                    Value::Reference(target) => Some(target),
                    _ => None,
                },
                None => {
                    self.error(
                        "unresolved-symbol",
                        format!("`{name}` is not in scope"),
                        context.module,
                        expr.span,
                    );
                    None
                }
            },
            ExprKind::Field(base, field) => {
                let mut target = self.reference_target(base, context)?;
                // The property must exist. The read also reports a property that is not
                // there, as all other access does. The only purpose of the read is to
                // find the target of the reference. Thus it is not a dependency
                // on the value.
                self.muted.push(self.frames.len());
                let value = if target.path.is_empty() {
                    let anchor = target.anchor;
                    self.field(&Value::Anchor(anchor), &field.value, context, field.span).value
                } else {
                    let holder = self.ref_value(&target);
                    self.field(&holder, &field.value, context, field.span).value
                };
                self.muted.pop();
                match value {
                    // A property that holds an anchor points to that anchor.
                    Value::Anchor(anchor) => Some(Ref::anchor(anchor)),
                    Value::Reference(inner) => Some(inner),
                    _ => {
                        target.path.push(field.value.clone());
                        Some(target)
                    }
                }
            }
            _ => match self.expr(expr, context).value {
                Value::Anchor(anchor) => Some(Ref::anchor(anchor)),
                Value::Reference(target) => Some(target),
                _ => None,
            },
        }
    }

    /// The value at the target of a reference.
    fn ref_value(&mut self, target: &Ref) -> Value {
        let mut value = Value::Anchor(target.anchor);
        for part in &target.path {
            value = match value {
                Value::Anchor(anchor) => self.property_value(anchor, part),
                Value::Dict(map) => map.get(part).cloned().unwrap_or(Value::Null),
                Value::Mixed(mixed) => mixed.get(part).cloned().unwrap_or(Value::Null),
                _ => Value::Null,
            };
        }
        value
    }

    // -- expressions ----------------------------------------------------

    fn expr(&mut self, expr: &Expr, context: &Context) -> Evaluated {
        match &expr.kind {
            ExprKind::Number(n) => Evaluated::plain(Value::Number(*n)),
            ExprKind::Bool(b) => Evaluated::plain(Value::Bool(*b)),
            ExprKind::Null => Evaluated::plain(Value::Null),
            ExprKind::Quoted(text) => Evaluated::quoted(Value::Str(Text::plain(text.clone()))),
            ExprKind::Nested(sigil, inner) => self.sigil_value(*sigil, inner, context, expr.span),
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
            ExprKind::Field(base, field) if matches!(base.kind, ExprKind::Super) => {
                self.super_field(context, &field.value, expr.span)
            }
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
                    UnaryOp::Not => match value {
                        Value::Bool(b) => Value::Bool(!b),
                        other => {
                            self.error(
                                "invalid-operand",
                                format!("`!` needs a boolean, found {}", other.kind()),
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
                // The evaluator evaluates only the selected branch. Piton has no truthiness.
                // Thus the condition must be a boolean.
                match self.expr(condition, context).value {
                    Value::Bool(true) => self.expr(consequent, context),
                    Value::Bool(false) => self.expr(alternative, context),
                    other => {
                        self.error(
                            "invalid-condition",
                            format!(
                                "the condition of `? :` has to be a boolean, found {}",
                                other.kind()
                            ),
                            context.module,
                            condition.span,
                        );
                        Evaluated::plain(Value::Null)
                    }
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
            Value::Anchor(anchor) => {
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
            And | Or => match (&left.value, &right.value) {
                (Value::Bool(a), Value::Bool(b)) => {
                    Value::Bool(if op == And { *a && *b } else { *a || *b })
                }
                (a, b) => {
                    self.error(
                        "invalid-operand",
                        format!(
                            "`{}` needs two booleans, found {} and {}",
                            op.symbol(),
                            a.kind(),
                            b.kind()
                        ),
                        context.module,
                        span,
                    );
                    Value::Null
                }
            },
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
                // This is a shallow merge. The result has the keys from both sides. The right side wins.
                (Value::Dict(a), Value::Dict(b)) => {
                    let mut merged = a.clone();
                    for (key, value) in b {
                        merged.insert(key.clone(), value.clone());
                    }
                    Value::Dict(merged)
                }
                (a, b) if is_text(a) && is_text(b) && (is_string(a) || is_string(b)) => {
                    // One side is a string and the other side is a simple value. The
                    // evaluator changes the value into a string first: `2 + "Hello"` is `2Hello`.
                    let mut text = self.stringify(a, context, span);
                    let right_text = self.stringify(b, context, span);
                    text.push_text(&right_text);
                    Value::Str(text)
                }
                (a, b) if (is_string(a) && b.is_complex()) || (a.is_complex() && is_string(b)) => {
                    // A list, dictionary, or anchor next to a string does not
                    // change into text. It makes an implicit list. This is the same as
                    // `{x}` in the middle of some text.
                    let mut items = Vec::new();
                    push_mixed(&mut items, a.clone());
                    push_mixed(&mut items, b.clone());
                    Value::Mixed(Mixed::new(items))
                }
                (a, b) => {
                    self.error(
                        "invalid-operand",
                        format!("`+` cannot combine {} and {}", a.kind(), b.kind()),
                        context.module,
                        span,
                    );
                    Value::Null
                }
            },
            Concat => match (&left.value, &right.value) {
                (Value::List(_) | Value::Mixed(_), Value::List(_) | Value::Mixed(_)) => {
                    let mut items = left.value.as_list_items();
                    items.extend(right.value.as_list_items());
                    Value::List(items)
                }
                // This is a deep merge. Dictionaries under the same key also merge, at
                // all levels. In other cases, the right side wins.
                (Value::Dict(a), Value::Dict(b)) => Value::Dict(deep_merge(a, b)),
                // On strings, `++` joins the strings with a line break.
                (a, b) if is_text(a) && is_text(b) && (is_string(a) || is_string(b)) => {
                    let mut text = self.stringify(a, context, span);
                    text.push_literal("\n");
                    let right_text = self.stringify(b, context, span);
                    text.push_text(&right_text);
                    Value::Str(text)
                }
                (a, b) => {
                    self.error(
                        "invalid-operand",
                        format!("`++` cannot combine {} and {}", a.kind(), b.kind()),
                        context.module,
                        span,
                    );
                    Value::Null
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
            // The string form of an anchor is its source name.
            Value::Anchor(id) => Text::plain(self.resolution.store.anchor(*id).name.clone()),
            Value::Reference(target) => Text::reference(target.clone()),
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

    /// Applies type constraints from left to right. It uses the first type that can hold
    /// the value.
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
            if let Some(value) = self.coerce(&evaluated, constraint, module) {
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

    fn coerce(
        &self,
        evaluated: &Evaluated,
        constraint: &TypeConstraint,
        module: ModuleId,
    ) -> Option<Value> {
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
                out.push(self.coerce(&Evaluated::plain(item), &element, module)?);
            }
            return Some(Value::List(out));
        }

        match &constraint.name {
            TypeName::Any => Some(value.clone()),
            TypeName::Simple => value.is_simple().then(|| value.clone()),
            TypeName::Complex => value.is_complex().then(|| value.clone()),
            TypeName::String => match value {
                Value::Str(_) => Some(value.clone()),
                // Numbers can coerce into strings. A literal keeps its source text
                // exactly. For an expression, the evaluator coerces the result.
                Value::Number(n) => Some(Value::Str(Text::plain(
                    evaluated
                        .literal
                        .clone()
                        .unwrap_or_else(|| format_number(*n)),
                ))),
                // Booleans and null never coerce.
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
                let target = match self.resolution.lookup(module, name) {
                    Some(Symbol::Anchor(target)) => target,
                    _ => {
                        self.resolution
                            .store
                            .anchors
                            .iter()
                            .find(|candidate| candidate.name == *name)?
                            .id
                    }
                };
                let store = &self.resolution.store;
                if constraint.extends {
                    // Each concrete anchor that has A in its chain satisfies
                    // `extends A`.
                    (!store.anchor(*anchor).is_abstract && store.inherits_from(*anchor, target))
                        .then(|| value.clone())
                } else {
                    direct_match(store, *anchor, target).then(|| value.clone())
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
    Escape(ast::EscapeBlock),
    ListItem(ast::ListItem),
    Merge(MergeOp, ast::MergeItem),
    Property(Property),
}

fn collect_pieces(block: &Block, out: &mut Vec<Piece>) {
    for item in &block.items {
        match item {
            BlockItem::Prose(Paragraph { lines, .. }) => out.push(Piece::Prose(lines.clone())),
            BlockItem::Escape(block) => out.push(Piece::Escape(block.clone())),
            BlockItem::ListItem(item) => out.push(Piece::ListItem(item.clone())),
            BlockItem::Merge(merge) => out.push(Piece::Merge(merge.op, merge.clone())),
            BlockItem::Property(property) => out.push(Piece::Property(property.clone())),
            BlockItem::Pass(_) => {}
        }
    }
}

/// Content with an indent under a list item is not part of that item. It becomes the
/// next item. Thus `- text` with a nested list under it gives the text and
/// the list as siblings. A dictionary with an indent under an item is one
/// dictionary after that item.
fn split_nested_list(mixed: Mixed) -> Vec<Value> {
    Value::Mixed(mixed).as_list_items()
}

/// Adds the value after a `+` line to all the values that the block built before it.
fn continue_block(accumulated: Option<Value>, next: Value) -> Value {
    let Some(previous) = accumulated else {
        return next;
    };
    match (previous, next) {
        (previous, Value::Null) => previous,
        (Value::List(mut items), Value::List(more)) => {
            items.extend(more);
            Value::List(items)
        }
        (previous @ Value::Mixed(_), Value::List(more)) => {
            let mut items = previous.as_list_items();
            items.extend(more);
            Value::List(items)
        }
        // More text after a joined string adds to the string, as a next line does.
        (Value::Str(mut text), Value::Str(more)) => {
            text.push_literal(" ");
            text.push_text(&more);
            Value::Str(text)
        }
        (Value::Dict(mut map), Value::Dict(more)) => {
            for (key, value) in more {
                map.insert(key, value);
            }
            Value::Dict(map)
        }
        (previous, next) => {
            let mut items = Vec::new();
            push_mixed(&mut items, previous);
            push_mixed(&mut items, next);
            Value::Mixed(Mixed::new(items))
        }
    }
}

/// Adds a value to an implicit list. Text stays as text and lists stay as lists.
fn push_mixed(items: &mut Vec<MixedItem>, value: Value) {
    match value {
        Value::Null => {}
        Value::Str(text) => {
            if !text.is_empty() {
                items.push(MixedItem::Text(text));
            }
        }
        Value::List(values) => items.push(MixedItem::List(values)),
        Value::Mixed(mixed) => items.extend(mixed.items),
        other => items.push(MixedItem::Value(other)),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// One piece of a prose value: text, or a value `{x}` dropped into it.
enum Part {
    Text(Text),
    Value(Value),
}

/// Joins text parts. Call it only when the parts contain no values.
fn parts_text(parts: Vec<Part>) -> Text {
    let mut text = Text::empty();
    for part in parts {
        if let Part::Text(piece) = part {
            text.push_text(&piece);
        }
    }
    text
}

fn is_string(value: &Value) -> bool {
    matches!(value, Value::Str(_))
}

/// A value that `+` can change into text: a simple value, or a reference.
fn is_text(value: &Value) -> bool {
    value.is_simple() || matches!(value, Value::Reference(_))
}

/// A plain anchor type only matches anchors that directly implement it. If an
/// anchor has more than one abstract, only the abstract that wins counts.
/// This is the abstract of the keyword, or else the right-most abstract.
fn direct_match(store: &crate::store::Store, anchor: AnchorId, target: AnchorId) -> bool {
    if anchor == target {
        return !store.anchor(anchor).is_abstract;
    }
    let def = store.anchor(anchor);
    if store.anchor(target).is_abstract {
        def.bases
            .iter()
            .rev()
            .find(|base| store.anchor(**base).is_abstract)
            .is_some_and(|winner| *winner == target)
    } else {
        def.bases.contains(&target)
    }
}

fn deep_merge(left: &Properties, right: &Properties) -> Properties {
    let mut merged = left.clone();
    for (key, value) in right {
        let combined = match (merged.get(key), value) {
            (Some(Value::Dict(a)), Value::Dict(b)) => Value::Dict(deep_merge(a, b)),
            _ => value.clone(),
        };
        merged.insert(key.clone(), combined);
    }
    merged
}

/// Applies literal inference to text that has no type constraint.
///
/// Quotes are ordinary characters. Thus `"false"` is the string `"false"`,
/// with the quotes.
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
    match plain {
        "true" => Evaluated::plain(Value::Bool(true)),
        "false" => Evaluated::plain(Value::Bool(false)),
        "null" => Evaluated::plain(Value::Null),
        other => match parse_number_literal(other) {
            Some(number) => Evaluated {
                value: Value::Number(number),
                quoted: false,
                literal: Some(other.trim().to_string()),
            },
            None => Evaluated::plain(Value::Str(text)),
        },
    }
}

/// Parses a number only when the full string is a number. It accepts the underscore
/// separators that the language allows.
///
/// Decimals must start with a 0. Thus `.5` is not a number. There is
/// no exponent notation.
fn parse_number_literal(text: &str) -> Option<f64> {
    piton_syntax::literal::parse_number_literal(text)
}

fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Mixed(_), _) | (_, Value::Mixed(_)) => {
            let left = if matches!(a, Value::Mixed(_)) { Value::List(a.as_list_items()) } else { a.clone() };
            let right = if matches!(b, Value::Mixed(_)) { Value::List(b.as_list_items()) } else { b.clone() };
            left == right
        }
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

/// Keeps the concatenation order. It removes duplicates and keeps the
/// last occurrence. Thus a merge of `[A, B, C]` into `[A, B, C, D]` gives
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

/// This function is public. Thus the type checker can describe constraints in
/// the same way.
pub fn constraint_label(constraint: &TypeConstraint) -> String {
    describe_constraint(constraint)
}

// ---------------------------------------------------------------------------
// Probes (editor tools)
// ---------------------------------------------------------------------------

/// One `+` or `++` line of a block, at the time the compiler built the block.
#[derive(Debug, Clone)]
pub struct BlockStep {
    /// The `+`/`++` line.
    pub span: Span,
    pub op: MergeOp,
    /// The combination of all lines above the line. `None` if there were no lines.
    pub before: Option<Value>,
    /// The value of the line.
    pub operand: Value,
    /// The result of the combination of the two.
    pub after: Value,
}

/// Evaluates single expressions and block steps after a compile. It uses the
/// values that the compile produced.
///
/// This is for editor tools. For example, a hover can explain how `+` combined two inputs.
/// The hover uses the rules of the compiler, not a second copy of them. The probe
/// reports no diagnostics. If a result causes a diagnostic, the probe returns `None`.
pub struct Probe<'a> {
    evaluator: Evaluator<'a>,
}

impl<'a> Probe<'a> {
    /// A probe that starts with the values that `resolution` stores at this time.
    /// `Compilation::from_resolution` supplies these values. Thus the probe evaluates no value two times.
    pub fn new(resolution: &'a Resolution) -> Probe<'a> {
        let mut evaluator = Evaluator {
            resolution,
            properties: HashMap::new(),
            property_stack: Vec::new(),
            variables: HashMap::new(),
            variable_stack: Vec::new(),
            anchors: HashMap::new(),
            reads: Reads::new(),
            frames: Vec::new(),
            muted: Vec::new(),
            diagnostics: Vec::new(),
        };
        for def in &resolution.store.anchors {
            for (name, value) in &def.properties {
                evaluator
                    .properties
                    .insert((def.id, name.clone()), value.clone());
            }
            evaluator.anchors.insert(def.id, def.properties.clone());
        }
        for def in &resolution.store.variables {
            if let Some(value) = &def.value {
                evaluator.variables.insert(def.id, value.clone());
            }
        }
        Probe { evaluator }
    }

    fn context(&self, module: ModuleId, this: Option<AnchorId>) -> Context {
        Context {
            module,
            this,
            derived: this,
            super_anchor: this.and_then(|anchor| {
                self.evaluator.resolution.store.anchor(anchor).bases.last().copied()
            }),
        }
    }

    fn checked(&mut self, run: impl FnOnce(&mut Evaluator<'a>) -> Value) -> Option<Value> {
        let before = self.evaluator.diagnostics.len();
        let value = run(&mut self.evaluator);
        let failed = self.evaluator.diagnostics.len() > before;
        self.evaluator.diagnostics.truncate(before);
        (!failed).then_some(value)
    }

    /// The value of `expr` in `module`, inside `this` when `expr` is in
    /// an anchor body.
    pub fn expr(&mut self, module: ModuleId, this: Option<AnchorId>, expr: &Expr) -> Option<Value> {
        let context = self.context(module, this);
        self.checked(|evaluator| evaluator.expr(expr, &context).value)
    }

    /// The value of one prose line, with its interpolations.
    pub fn line(
        &mut self,
        module: ModuleId,
        this: Option<AnchorId>,
        line: &ProseLine,
    ) -> Option<Value> {
        let context = self.context(module, this);
        self.checked(|evaluator| evaluator.prose_line_value(line, &context).value)
    }

    /// Combines two values with `+` or `++` exactly as an expression does.
    pub fn combine(
        &mut self,
        module: ModuleId,
        op: BinaryOp,
        left: Value,
        right: Value,
    ) -> Option<Value> {
        let context = self.context(module, None);
        self.checked(|evaluator| {
            evaluator.binary(
                op,
                Evaluated::plain(left),
                Evaluated::plain(right),
                &context,
                Span::default(),
            )
        })
    }

    /// The steps that built a value block with `+`/`++` lines, one entry for each line.
    ///
    /// It does the same as `Evaluator::merged_block`, and records each step.
    pub fn block_steps(
        &mut self,
        module: ModuleId,
        this: Option<AnchorId>,
        node: &ValueNode,
    ) -> Vec<BlockStep> {
        let context = self.context(module, this);
        let mut pieces: Vec<Piece> = Vec::new();
        if let Some(block) = &node.block {
            collect_pieces(block, &mut pieces);
        }
        if let Some(inline) = &node.inline {
            if !inline.is_blank() {
                match pieces.first_mut() {
                    Some(Piece::Prose(lines)) => lines.insert(0, inline.clone()),
                    _ => pieces.insert(0, Piece::Prose(vec![inline.clone()])),
                }
            }
        }
        let before_diagnostics = self.evaluator.diagnostics.len();
        let evaluator = &mut self.evaluator;
        let span = node.span;
        let mut steps = Vec::new();
        let mut accumulated: Option<Value> = None;
        let mut segment: Vec<Piece> = Vec::new();
        for piece in &pieces {
            match piece {
                Piece::Merge(op, merge) => {
                    if !segment.is_empty() {
                        let value = evaluator.pieces_to_value(&segment, &context, span).value;
                        accumulated = Some(continue_block(accumulated, value));
                        segment.clear();
                    }
                    let operand = evaluator.prose_line_value(&merge.value, &context).value;
                    let before = accumulated.clone();
                    let after = match accumulated.take() {
                        None => operand.clone(),
                        Some(previous) => {
                            let operator = match op {
                                MergeOp::Merge => BinaryOp::Add,
                                MergeOp::Concat => BinaryOp::Concat,
                            };
                            let right = match (&previous, operand.clone()) {
                                (Value::List(_) | Value::Mixed(_), Value::Null) => {
                                    Value::List(Vec::new())
                                }
                                (Value::List(_) | Value::Mixed(_), other)
                                    if !matches!(other, Value::List(_) | Value::Mixed(_)) =>
                                {
                                    Value::List(vec![other])
                                }
                                (_, other) => other,
                            };
                            evaluator.binary(
                                operator,
                                Evaluated::plain(previous),
                                Evaluated::plain(right),
                                &context,
                                merge.span,
                            )
                        }
                    };
                    accumulated = Some(after.clone());
                    steps.push(BlockStep {
                        span: merge.span,
                        op: *op,
                        before,
                        operand,
                        after,
                    });
                }
                other => segment.push(other.clone()),
            }
        }
        self.evaluator.diagnostics.truncate(before_diagnostics);
        steps
    }
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
        // Quotes are only characters.
        let quoted = infer(Text::plain("\"false\""), true);
        assert_eq!(quoted.value, Value::Str(Text::plain("\"false\"")));
        assert!(matches!(infer(Text::plain(".5"), true).value, Value::Str(_)));
        assert!(matches!(infer(Text::plain("1e3"), true).value, Value::Str(_)));
        assert_eq!(infer(Text::plain("1.0"), true).literal.as_deref(), Some("1.0"));
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
