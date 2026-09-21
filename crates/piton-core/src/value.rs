//! The resolved value model.
//!
//! Piton has no runtime: compilation produces values, and adapters serialize
//! those values. The model therefore only needs to describe what the language
//! can *describe*, and it needs to keep enough structure for the Markdown
//! adapter to tell prose from data.

use std::fmt;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;

use crate::text::Text;

/// Identifies one anchor declaration within a compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AnchorId(pub u32);

impl fmt::Display for AnchorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "anchor#{}", self.0)
    }
}

/// An ordered map of properties. Declaration order is part of the compiled
/// output, so insertion order is preserved everywhere.
pub type Properties = IndexMap<String, Value>;

/// One element of an implicit mixed block.
///
/// A block that combines prose, list items, and dictionary keys becomes an
/// implicit list. The dictionary keys declared directly in such a block remain
/// addressable, so they are kept as named entries rather than being folded into
/// an anonymous map.
#[derive(Debug, Clone, PartialEq)]
pub enum MixedItem {
    /// A run of prose.
    Text(Text),
    /// An explicit list appearing inside the block.
    List(Vec<Value>),
    /// A `key: value` pair declared directly in the block.
    Entry(String, Value),
}

/// The contents of an implicit mixed block, in source order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mixed {
    pub items: Vec<MixedItem>,
}

impl Mixed {
    pub fn new(items: Vec<MixedItem>) -> Self {
        Mixed { items }
    }

    /// Looks up a directly declared dictionary key.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.items.iter().find_map(|item| match item {
            MixedItem::Entry(name, value) if name == key => Some(value),
            _ => None,
        })
    }

    pub fn entries(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.items.iter().filter_map(|item| match item {
            MixedItem::Entry(name, value) => Some((name.as_str(), value)),
            _ => None,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// A fully resolved Piton value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    /// Piton has a single numeric type; int and float are the same thing.
    Number(f64),
    /// Explicit line breaks inside a string block are stored as `\n`.
    Str(Text),
    List(Vec<Value>),
    Dict(Properties),
    /// An implicit list produced by mixing prose, lists, and dictionary keys.
    Mixed(Mixed),
    /// An anchor resolved by identity. Serializing it inlines its content.
    Anchor(AnchorId),
    /// A reference produced by `@{...}`. Serializing it produces a link, not a
    /// copy of the referenced content.
    Reference(AnchorId),
}

impl Value {
    pub fn string(text: impl Into<String>) -> Value {
        Value::Str(Text::plain(text))
    }

    pub fn text(text: Text) -> Value {
        Value::Str(text)
    }

    /// The name used in diagnostics and in `simple`/`complex` checks.
    pub fn kind(&self) -> ValueKind {
        match self {
            Value::Null => ValueKind::Null,
            Value::Bool(_) => ValueKind::Boolean,
            Value::Number(_) => ValueKind::Number,
            Value::Str(_) => ValueKind::String,
            Value::List(_) | Value::Mixed(_) => ValueKind::List,
            Value::Dict(_) => ValueKind::Dictionary,
            Value::Anchor(_) => ValueKind::Anchor,
            Value::Reference(_) => ValueKind::Reference,
        }
    }

    pub fn is_simple(&self) -> bool {
        matches!(
            self,
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::Str(_)
        )
    }

    pub fn is_complex(&self) -> bool {
        matches!(
            self,
            Value::List(_) | Value::Mixed(_) | Value::Dict(_) | Value::Anchor(_)
        )
    }

    /// Truthiness, used only by the ternary operator.
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::List(items) => !items.is_empty(),
            Value::Mixed(m) => !m.is_empty(),
            Value::Dict(d) => !d.is_empty(),
            Value::Anchor(_) | Value::Reference(_) => true,
        }
    }

    /// Flattens a mixed block into plain list elements, grouping consecutive
    /// dictionary entries into a single dictionary the way JSON output does.
    pub fn as_list_items(&self) -> Vec<Value> {
        match self {
            Value::List(items) => items.clone(),
            Value::Mixed(mixed) => {
                let mut out = Vec::new();
                let mut pending: Option<Properties> = None;
                for item in &mixed.items {
                    match item {
                        MixedItem::Entry(key, value) => {
                            pending
                                .get_or_insert_with(Properties::new)
                                .insert(key.clone(), value.clone());
                        }
                        other => {
                            if let Some(dict) = pending.take() {
                                out.push(Value::Dict(dict));
                            }
                            match other {
                                MixedItem::Text(text) => out.push(Value::Str(text.clone())),
                                MixedItem::List(items) => out.push(Value::List(items.clone())),
                                MixedItem::Entry(..) => unreachable!(),
                            }
                        }
                    }
                }
                if let Some(dict) = pending.take() {
                    out.push(Value::Dict(dict));
                }
                out
            }
            other => vec![other.clone()],
        }
    }

    /// Looks up a property for the `.` operator. Anchors are resolved through
    /// `anchors`; list elements are intentionally not addressable.
    pub fn property<'a>(&'a self, key: &str, anchors: &'a dyn AnchorView) -> Option<&'a Value> {
        match self {
            Value::Dict(map) => map.get(key),
            Value::Mixed(mixed) => mixed.get(key),
            Value::Anchor(id) | Value::Reference(id) => anchors.properties(*id).get(key),
            _ => None,
        }
    }

    /// True when the value is a dictionary that, recursively, contains only
    /// dictionaries and simple scalars.
    ///
    /// Belay renders these "pure dictionaries" as indentation-based structures
    /// inside a code fence rather than as a run of headings, because a map of
    /// data reads better as data than as document structure.
    pub fn is_pure_dictionary(&self) -> bool {
        match self {
            Value::Dict(map) => map.values().all(|v| v.is_simple() || v.is_pure_dictionary()),
            _ => false,
        }
    }
}

/// The type names the language exposes, used for constraints and diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueKind {
    String,
    Number,
    Boolean,
    Null,
    List,
    Dictionary,
    Anchor,
    Reference,
}

impl ValueKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ValueKind::String => "string",
            ValueKind::Number => "number",
            ValueKind::Boolean => "boolean",
            ValueKind::Null => "null",
            ValueKind::List => "list",
            ValueKind::Dictionary => "dictionary",
            ValueKind::Anchor => "anchor",
            ValueKind::Reference => "reference",
        }
    }
}

impl fmt::Display for ValueKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Read-only access to resolved anchors, so serializers can follow anchor
/// values without depending on the resolver.
pub trait AnchorView {
    /// The anchor's source name, which is also its string representation.
    fn name(&self, id: AnchorId) -> &str;
    /// The anchor's fully resolved properties, in output order.
    fn properties(&self, id: AnchorId) -> &Properties;
    /// The file the anchor was declared in.
    fn source_path(&self, id: AnchorId) -> &Path;
    /// Whether the anchor was declared `abstract`.
    fn is_abstract(&self, id: AnchorId) -> bool;
}

/// An [`AnchorView`] with no anchors, useful for serializing plain data.
pub struct EmptyAnchors;

impl AnchorView for EmptyAnchors {
    fn name(&self, _: AnchorId) -> &str {
        "<unknown>"
    }
    fn properties(&self, _: AnchorId) -> &Properties {
        static EMPTY: std::sync::OnceLock<Properties> = std::sync::OnceLock::new();
        EMPTY.get_or_init(Properties::new)
    }
    fn source_path(&self, _: AnchorId) -> &Path {
        static EMPTY: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        EMPTY.get_or_init(PathBuf::new)
    }
    fn is_abstract(&self, _: AnchorId) -> bool {
        false
    }
}

/// Formats a number the way Piton prints it: integral values lose the trailing
/// `.0` so `42` round-trips as `42`.
pub fn format_number(n: f64) -> String {
    if n.is_nan() {
        return "null".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if n == n.trunc() && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        let mut s = format!("{n}");
        if s.contains('e') {
            s = format!("{n:?}");
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_print_without_spurious_decimals() {
        assert_eq!(format_number(42.0), "42");
        assert_eq!(format_number(3.14), "3.14");
        assert_eq!(format_number(0.0), "0");
        assert_eq!(format_number(-3.5), "-3.5");
    }

    #[test]
    fn mixed_flattening_groups_adjacent_entries() {
        let mixed = Value::Mixed(Mixed::new(vec![
            MixedItem::Text(crate::text::Text::plain("prose")),
            MixedItem::List(vec![Value::string("a")]),
            MixedItem::Entry("k".into(), Value::Number(1.0)),
            MixedItem::Entry("j".into(), Value::Number(2.0)),
        ]));
        let items = mixed.as_list_items();
        assert_eq!(items.len(), 3);
        assert!(matches!(items[2], Value::Dict(ref d) if d.len() == 2));
    }

    #[test]
    fn pure_dictionary_detection() {
        let mut inner = Properties::new();
        inner.insert("a".into(), Value::Number(1.0));
        let mut outer = Properties::new();
        outer.insert("nested".into(), Value::Dict(inner));
        assert!(Value::Dict(outer.clone()).is_pure_dictionary());

        outer.insert("list".into(), Value::List(vec![Value::Null]));
        assert!(!Value::Dict(outer).is_pure_dictionary());
    }
}
