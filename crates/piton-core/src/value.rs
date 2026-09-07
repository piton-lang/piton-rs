//! Piton's value model and its operator semantics.
//!
//! Piton has no runtime, so a value is simply the result of compiling a
//! declaration. The interesting behaviour lives in the operators: `+`
//! concatenates and deduplicates, `++` concatenates and keeps duplicates, and
//! mixing unrelated types produces a mixed list rather than an error.

use std::fmt;
use std::sync::Arc;

use indexmap::IndexMap;

/// A dictionary keeps declaration order.
pub type Dict = IndexMap<String, Value>;

/// Identifies one anchor declaration in the workspace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AnchorId(pub u32);

/// A number, remembering how it was written so `1.0` serializes as `1.0`.
#[derive(Clone, Debug)]
pub struct Num {
    pub value: f64,
    pub text: Option<String>,
}

impl Num {
    pub fn new(value: f64) -> Num {
        Num { value, text: None }
    }

    /// Parse a numeric literal, keeping its spelling minus digit separators.
    pub fn literal(text: &str) -> Num {
        let clean = text.replace('_', "");
        Num { value: clean.parse().unwrap_or(f64::NAN), text: Some(clean) }
    }
}

impl fmt::Display for Num {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.text {
            Some(text) => f.write_str(text),
            None if self.value == self.value.trunc() && self.value.abs() < 1e15 => {
                write!(f, "{}", self.value as i64)
            }
            None => write!(f, "{}", self.value),
        }
    }
}

/// A list value. Implicit lists come from a block that mixed shapes; their
/// dictionary elements stay addressable through the owning path.
#[derive(Clone, Debug)]
pub struct List {
    pub items: Vec<Value>,
    pub implicit: bool,
}

impl List {
    pub fn explicit(items: Vec<Value>) -> List {
        List { items, implicit: false }
    }
    pub fn implicit(items: Vec<Value>) -> List {
        List { items, implicit: true }
    }
}

/// A compiled anchor. Anchors resolve by reference, so the identity survives
/// being passed around.
#[derive(Clone, Debug)]
pub struct Anchor {
    pub id: AnchorId,
    pub name: String,
    pub props: Arc<Dict>,
}

/// Any Piton value.
#[derive(Clone, Debug)]
pub enum Value {
    Null,
    Bool(bool),
    Number(Num),
    Str(String),
    List(List),
    Dict(Dict),
    Anchor(Anchor),
}

impl Value {
    pub fn number(value: f64) -> Value {
        Value::Number(Num::new(value))
    }

    pub fn string(text: impl Into<String>) -> Value {
        Value::Str(text.into())
    }

    pub fn list(items: Vec<Value>) -> Value {
        Value::List(List::explicit(items))
    }

    /// The name used in diagnostics and by the `simple`/`complex` constraints.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::Str(_) => "string",
            Value::List(_) => "list",
            Value::Dict(_) => "dictionary",
            Value::Anchor(_) => "anchor",
        }
    }

    /// `string`, `number`, `boolean`, and `null` are the simple types.
    pub fn is_simple(&self) -> bool {
        matches!(self, Value::Null | Value::Bool(_) | Value::Number(_) | Value::Str(_))
    }

    /// `list`, `dictionary`, and `anchor` are the complex types.
    pub fn is_complex(&self) -> bool {
        !self.is_simple()
    }

    /// Zero, the empty string, `false`, and `null` are falsy.
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(value) => *value,
            Value::Number(num) => num.value != 0.0,
            Value::Str(text) => !text.is_empty(),
            Value::List(list) => !list.items.is_empty(),
            Value::Dict(dict) => !dict.is_empty(),
            Value::Anchor(_) => true,
        }
    }

    /// Look up a dotted path segment.
    ///
    /// Dictionaries declared directly inside an implicit list stay addressable,
    /// which is why a list can answer a lookup at all.
    pub fn field(&self, name: &str) -> Option<&Value> {
        match self {
            Value::Dict(dict) => dict.get(name),
            Value::Anchor(anchor) => anchor.props.get(name),
            Value::List(list) if list.implicit => list.items.iter().find_map(|item| match item {
                Value::Dict(dict) => dict.get(name),
                _ => None,
            }),
            _ => None,
        }
    }

    /// The plain-text rendering used by string interpolation and by `:: string`.
    pub fn to_literal(&self) -> String {
        match self {
            Value::Null => "null".to_string(),
            Value::Bool(value) => value.to_string(),
            Value::Number(num) => num.to_string(),
            Value::Str(text) => text.clone(),
            _ => String::new(),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => a.value == b.value,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::List(a), Value::List(b)) => a.items == b.items,
            (Value::Dict(a), Value::Dict(b)) => a == b,
            (Value::Anchor(a), Value::Anchor(b)) => a.id == b.id,
            _ => false,
        }
    }
}

/// Something an operator could not do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpError(pub String);

type OpResult = Result<Value, OpError>;

/// Arithmetic and comparison operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Concat,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Concat => "++",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
        }
    }
}

/// Apply a binary operator to two already-evaluated values.
pub fn apply(op: BinOp, lhs: Value, rhs: Value) -> OpResult {
    match op {
        BinOp::Add => Ok(concat(lhs, rhs, true)),
        BinOp::Concat => Ok(concat(lhs, rhs, false)),
        BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => arithmetic(op, lhs, rhs),
        BinOp::Eq => Ok(Value::Bool(compare_eq(&lhs, &rhs)?)),
        BinOp::Ne => Ok(Value::Bool(!compare_eq(&lhs, &rhs)?)),
        _ => Ok(Value::Bool(ordering(op, &lhs, &rhs)?)),
    }
}

fn arithmetic(op: BinOp, lhs: Value, rhs: Value) -> OpResult {
    let (Value::Number(a), Value::Number(b)) = (&lhs, &rhs) else {
        return Err(OpError(format!(
            "`{}` needs two numbers but got {} and {}",
            op.symbol(),
            lhs.type_name(),
            rhs.type_name()
        )));
    };
    let value = match op {
        BinOp::Sub => a.value - b.value,
        BinOp::Mul => a.value * b.value,
        BinOp::Div => {
            if b.value == 0.0 {
                return Err(OpError("division by zero".to_string()));
            }
            a.value / b.value
        }
        // Floor division semantics: the result carries the divisor's sign.
        BinOp::Rem => {
            if b.value == 0.0 {
                return Err(OpError("division by zero".to_string()));
            }
            a.value - b.value * (a.value / b.value).floor()
        }
        _ => unreachable!(),
    };
    Ok(Value::number(value))
}

fn compare_eq(lhs: &Value, rhs: &Value) -> Result<bool, OpError> {
    if lhs.type_name() != rhs.type_name() {
        return Err(OpError(format!(
            "cannot compare {} with {}",
            lhs.type_name(),
            rhs.type_name()
        )));
    }
    Ok(lhs == rhs)
}

fn ordering(op: BinOp, lhs: &Value, rhs: &Value) -> Result<bool, OpError> {
    let order = match (lhs, rhs) {
        (Value::Number(a), Value::Number(b)) => a.value.partial_cmp(&b.value),
        (Value::Str(a), Value::Str(b)) => Some(a.cmp(b)),
        _ => {
            return Err(OpError(format!(
                "cannot order {} against {}",
                lhs.type_name(),
                rhs.type_name()
            )))
        }
    };
    let Some(order) = order else { return Ok(false) };
    Ok(match op {
        BinOp::Lt => order.is_lt(),
        BinOp::Le => order.is_le(),
        BinOp::Gt => order.is_gt(),
        BinOp::Ge => order.is_ge(),
        _ => unreachable!(),
    })
}

/// `+` and `++`.
///
/// Numbers add. Strings and numbers concatenate as text. Lists concatenate,
/// deduplicating for `+`. Dictionaries merge shallowly for `+` and deeply for
/// `++`. Anything else becomes a mixed list of the two operands.
pub fn concat(lhs: Value, rhs: Value, dedup: bool) -> Value {
    match (lhs, rhs) {
        (Value::Number(a), Value::Number(b)) => Value::number(a.value + b.value),
        (Value::Str(a), b @ (Value::Str(_) | Value::Number(_))) => {
            Value::Str(format!("{a}{}", b.to_literal()))
        }
        (a @ Value::Number(_), Value::Str(b)) => Value::Str(format!("{}{b}", a.to_literal())),
        (Value::List(a), Value::List(b)) => Value::List(join_lists(a, b, dedup)),
        (Value::Dict(a), Value::Dict(b)) => Value::Dict(merge_dicts(a, b, !dedup)),
        (a, b) => Value::List(List::implicit(vec![a, b])),
    }
}

/// Concatenate two lists, optionally dropping left-hand duplicates so that the
/// right-hand operand wins, as `[1,2,3,4] + [1,2,3]` giving `[4,1,2,3]`.
fn join_lists(left: List, right: List, dedup: bool) -> List {
    let implicit = left.implicit || right.implicit;
    if !dedup {
        let mut items = left.items;
        items.extend(right.items);
        return List { items, implicit };
    }
    let mut items: Vec<Value> =
        left.items.into_iter().filter(|item| !right.items.contains(item)).collect();
    for item in right.items {
        if !items.contains(&item) {
            items.push(item);
        }
    }
    List { items, implicit }
}

/// Merge two dictionaries, right winning. `deep` recurses into nested
/// dictionaries instead of replacing them.
fn merge_dicts(left: Dict, right: Dict, deep: bool) -> Dict {
    let mut merged = left;
    for (key, value) in right {
        match (deep, merged.get(&key)) {
            (true, Some(Value::Dict(existing))) => {
                if let Value::Dict(incoming) = value {
                    let nested = merge_dicts(existing.clone(), incoming, true);
                    merged.insert(key, Value::Dict(nested));
                    continue;
                }
                merged.insert(key, value);
            }
            _ => {
                merged.insert(key, value);
            }
        }
    }
    merged
}
