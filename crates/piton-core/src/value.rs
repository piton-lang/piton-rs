//! The resolved value model.
//!
//! Piton has no runtime. Compilation makes values, and adapters serialize
//! those values. Thus the model must describe only what the language can
//! *describe*. The model must also keep sufficient structure. The Markdown
//! adapter uses this structure to know prose from data.

use std::fmt;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;

use crate::text::Text;

/// Identifies one anchor declaration in a compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AnchorId(pub u32);

impl fmt::Display for AnchorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "anchor#{}", self.0)
    }
}

/// The target of a reference. The target is an anchor, or a property on an
/// anchor.
///
/// For a reference to the anchor, `path` is empty. For a reference to a
/// property, `path` holds the dot path. Thus `@{Button.color}` is `Button` with
/// the path `["color"]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ref {
    pub anchor: AnchorId,
    pub path: Vec<String>,
}

impl Ref {
    pub fn anchor(anchor: AnchorId) -> Ref {
        Ref {
            anchor,
            path: Vec::new(),
        }
    }

    /// `Anchor` or `Anchor.property`, as the source writes the reference.
    pub fn display(&self, anchors: &dyn AnchorView) -> String {
        let mut out = anchors.name(self.anchor).to_string();
        for part in &self.path {
            out.push('.');
            out.push_str(part);
        }
        out
    }
}

impl From<AnchorId> for Ref {
    fn from(anchor: AnchorId) -> Self {
        Ref::anchor(anchor)
    }
}

/// An ordered map of properties. The declaration order is part of the compiled
/// output. Thus all code keeps the insertion order.
pub type Properties = IndexMap<String, Value>;

/// One element of an implicit mixed block.
///
/// A block that combines prose, list items and dictionary keys becomes an
/// implicit list. You can still address the dictionary keys that the block
/// declares directly. Thus they stay as named entries. They do not go into an
/// anonymous map.
#[derive(Debug, Clone, PartialEq)]
pub enum MixedItem {
    /// A run of prose.
    Text(Text),
    /// A literal list in the block.
    List(Vec<Value>),
    /// A `key: value` pair declared directly in the block.
    Entry(String, Value),
    /// A different value in the block. An example is the copy of an anchor that
    /// `{Foo}` puts in the middle of some text.
    Value(Value),
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

    /// Finds a dictionary key that the block declares directly.
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
    /// Piton has one numeric type. Int and float are the same type.
    Number(f64),
    /// The value stores the line breaks in a string block as `\n`.
    Str(Text),
    List(Vec<Value>),
    Dict(Properties),
    /// An implicit list. A block that mixes prose, lists and dictionary keys
    /// makes it.
    Mixed(Mixed),
    /// An anchor that resolves by identity. Serialization inlines its content.
    Anchor(AnchorId),
    /// A reference that `@{...}` makes. Serialization makes a link. It does not
    /// make a copy of the referenced content.
    Reference(Ref),
}

impl Value {
    pub fn string(text: impl Into<String>) -> Value {
        Value::Str(Text::plain(text))
    }

    pub fn text(text: Text) -> Value {
        Value::Str(text)
    }

    /// The name that diagnostics and `simple`/`complex` checks use.
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

    /// Flattens a mixed block into plain list elements. It puts consecutive
    /// dictionary entries into one dictionary, as the JSON output does.
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
                                MixedItem::Value(value) => out.push(value.clone()),
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

    /// Finds a property for the `.` operator. It resolves anchors through
    /// `anchors`. You cannot address list elements, and this is intentional.
    pub fn property<'a>(&'a self, key: &str, anchors: &'a dyn AnchorView) -> Option<&'a Value> {
        match self {
            Value::Dict(map) => map.get(key),
            Value::Mixed(mixed) => mixed.get(key),
            Value::Anchor(id) => anchors.properties(*id).get(key),
            _ => None,
        }
    }

    /// True when the value is a dictionary that contains only dictionaries and
    /// simple scalars, at all levels.
    ///
    /// Belay renders these "pure dictionaries" as indentation-based structures
    /// in a code fence. It does not render them as a run of headings. A map of
    /// data is easier to read as data than as document structure.
    pub fn is_pure_dictionary(&self) -> bool {
        match self {
            Value::Dict(map) => map.values().all(|v| v.is_simple() || v.is_pure_dictionary()),
            _ => false,
        }
    }
}

/// The type names that the language exposes. Constraints and diagnostics use
/// them.
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

/// Read-only access to resolved anchors. Serializers use it to follow anchor
/// values. Thus serializers do not depend on the resolver.
pub trait AnchorView {
    /// The source name of the anchor. It is also the string representation of
    /// the anchor.
    fn name(&self, id: AnchorId) -> &str;
    /// The fully resolved properties of the anchor, in output order.
    fn properties(&self, id: AnchorId) -> &Properties;
    /// The file that declared the anchor.
    fn source_path(&self, id: AnchorId) -> &Path;
    /// True when the declaration of the anchor is `abstract`.
    fn is_abstract(&self, id: AnchorId) -> bool;
    /// The directory of the output file, when there is one. A reference uses
    /// it to make a path relative to that directory.
    fn output_directory(&self) -> Option<&Path> {
        None
    }
    /// The location of the output for a source file. By default it is next to
    /// the source. `piton compile --write` puts it there.
    fn output_path(&self, source: &Path) -> PathBuf {
        source.to_path_buf()
    }
}

/// An [`AnchorView`] with no anchors. Use it to serialize plain data.
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

/// Formats a number as Piton prints it. Integral values lose the `.0` at the
/// end. Thus `42` round-trips as `42`.
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
