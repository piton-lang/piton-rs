//! Type constraints.
//!
//! A `::` chain is tried left to right and the first constraint the value can
//! validly represent wins, which is what makes `x:: string:: number: 42` a
//! string and `x:: number:: string: 42` a number.

use crate::hir::TypeExpr;
use crate::value::{AnchorId, List, Value};

/// What the checker needs to know about anchors to resolve named constraints.
pub trait TypeContext {
    /// Resolve a type name to an anchor declared or imported in `file`.
    fn resolve_anchor(&self, name: &str) -> Option<AnchorId>;
    /// The anchors an anchor names directly as bases.
    fn direct_bases(&self, id: AnchorId) -> Vec<AnchorId>;
    /// Every anchor in an anchor's inheritance chain.
    fn ancestors(&self, id: AnchorId) -> Vec<AnchorId>;
}

/// Apply a `::` constraint chain, coercing where the language allows it.
pub fn constrain(
    value: Value,
    constraints: &[TypeExpr],
    ctx: &dyn TypeContext,
) -> Result<Value, String> {
    if constraints.is_empty() {
        return Ok(value);
    }
    for constraint in constraints {
        if let Some(coerced) = coerce(value.clone(), constraint, ctx) {
            return Ok(coerced);
        }
    }
    let expected: Vec<String> = constraints.iter().map(TypeExpr::render).collect();
    Err(format!("{} does not satisfy {}", value.type_name(), expected.join(" or ")))
}

/// Try one constraint. `None` means the value cannot represent that type.
pub fn coerce(value: Value, constraint: &TypeExpr, ctx: &dyn TypeContext) -> Option<Value> {
    match constraint {
        TypeExpr::ListOf { element, .. } => {
            let Value::List(list) = value else { return None };
            let mut items = Vec::with_capacity(list.items.len());
            for item in list.items {
                items.push(coerce(item, element, ctx)?);
            }
            Some(Value::List(List { items, implicit: list.implicit }))
        }
        TypeExpr::Extends { base, .. } => {
            let TypeExpr::Named { name, .. } = base.as_ref() else { return None };
            let target = ctx.resolve_anchor(name)?;
            let Value::Anchor(anchor) = &value else { return None };
            (anchor.id == target || ctx.ancestors(anchor.id).contains(&target)).then_some(value)
        }
        TypeExpr::Named { name, .. } => coerce_named(value, name, ctx),
    }
}

fn coerce_named(value: Value, name: &str, ctx: &dyn TypeContext) -> Option<Value> {
    match name {
        "any" => Some(value),
        "simple" => value.is_simple().then_some(value),
        "complex" => value.is_complex().then_some(value),
        // Simple values render as text; complex values are never flattened.
        "string" => value.is_simple().then(|| Value::Str(value.to_literal())),
        "number" => matches!(value, Value::Number(_)).then_some(value),
        "boolean" => matches!(value, Value::Bool(_)).then_some(value),
        "null" => matches!(value, Value::Null).then_some(value),
        "list" => matches!(value, Value::List(_)).then_some(value),
        "dictionary" => matches!(value, Value::Dict(_)).then_some(value),
        "anchor" => matches!(value, Value::Anchor(_)).then_some(value),
        // A bare anchor name is satisfied by anchors that implement it directly.
        _ => {
            let target = ctx.resolve_anchor(name)?;
            let Value::Anchor(anchor) = &value else { return None };
            (anchor.id == target || ctx.direct_bases(anchor.id).contains(&target)).then_some(value)
        }
    }
}

/// The built-in constraint names, for completion and validation.
pub fn is_builtin(name: &str) -> bool {
    piton_syntax::kind::BUILTIN_TYPES.contains(&name)
        || matches!(name, "any" | "simple" | "complex")
}
