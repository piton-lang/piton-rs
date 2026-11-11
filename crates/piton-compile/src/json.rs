//! Piton values as JSON, the way a plugin sees them.
//!
//! Plugins run outside the compiler, so what they read from it has to be
//! plain data. Strings are their plain text, with references written as the
//! names they point at. An anchor that appears as a value is not inlined,
//! which could repeat a large part of the project, but named, as
//! `{"$anchor": "Name"}`; a reference is `{"$reference": "Name.path"}`.

use piton_core::{AnchorView, MixedItem, Properties, Value};
use serde_json::{Map, Number, Value as Json};

/// One value, as JSON.
pub fn value(value: &Value, anchors: &dyn AnchorView) -> Json {
    match value {
        Value::Null => Json::Null,
        Value::Bool(flag) => Json::Bool(*flag),
        Value::Number(number) => number_json(*number),
        Value::Str(text) => Json::String(text.render_plain(anchors)),
        Value::List(items) => Json::Array(items.iter().map(|item| self::value(item, anchors)).collect()),
        Value::Dict(properties) => Json::Object(properties_map(properties, anchors)),
        Value::Mixed(mixed) => Json::Array(
            mixed
                .items
                .iter()
                .map(|item| match item {
                    MixedItem::Text(text) => Json::String(text.render_plain(anchors)),
                    MixedItem::List(items) => {
                        Json::Array(items.iter().map(|item| self::value(item, anchors)).collect())
                    }
                    MixedItem::Entry(key, value) => {
                        let mut entry = Map::new();
                        entry.insert(key.clone(), self::value(value, anchors));
                        Json::Object(entry)
                    }
                    MixedItem::Value(value) => self::value(value, anchors),
                })
                .collect(),
        ),
        Value::Anchor(anchor) => {
            let mut named = Map::new();
            named.insert("$anchor".into(), Json::String(anchors.name(*anchor).to_string()));
            Json::Object(named)
        }
        Value::Reference(reference) => {
            let mut name = anchors.name(reference.anchor).to_string();
            for part in &reference.path {
                name.push('.');
                name.push_str(part);
            }
            let mut named = Map::new();
            named.insert("$reference".into(), Json::String(name));
            Json::Object(named)
        }
    }
}

/// An anchor's or dictionary's properties, as a JSON object in their order.
pub fn properties_map(properties: &Properties, anchors: &dyn AnchorView) -> Map<String, Json> {
    properties
        .iter()
        .map(|(key, item)| (key.clone(), value(item, anchors)))
        .collect()
}

/// A whole number is written without a fraction, as Piton writes it.
fn number_json(number: f64) -> Json {
    if number.fract() == 0.0 && number.abs() < 9.0e15 {
        Json::Number(Number::from(number as i64))
    } else {
        Number::from_f64(number).map(Json::Number).unwrap_or(Json::Null)
    }
}
