//! Piton values as JSON, in the form that a plugin sees.
//!
//! Plugins run outside the compiler. Thus the data that they read from the
//! compiler must be plain data. A string becomes its plain text. Each
//! reference in it becomes the name of its target. An anchor in a value
//! position does not become inline, because that can repeat a large part of
//! the project. The anchor becomes its name, as `{"$anchor": "Name"}`. A
//! reference becomes `{"$reference": "Name.path"}`.

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

/// The properties of an anchor or dictionary, as a JSON object in their order.
pub fn properties_map(properties: &Properties, anchors: &dyn AnchorView) -> Map<String, Json> {
    properties
        .iter()
        .map(|(key, item)| (key.clone(), value(item, anchors)))
        .collect()
}

/// A whole number has no fraction in the output. Piton writes it the same way.
fn number_json(number: f64) -> Json {
    if number.fract() == 0.0 && number.abs() < 9.0e15 {
        Json::Number(Number::from(number as i64))
    } else {
        Number::from_f64(number).map(Json::Number).unwrap_or(Json::Null)
    }
}

/// One value, as JSON. This JSON keeps the data that [`value`] removes.
/// Text tells which references it holds. A dictionary is different from a
/// block with keys and other items. Keys keep their order, whatever their
/// form. An anchor or a reference has its id.
///
/// Each value is an object with a `kind`. These are the kinds:
/// - `null`, `boolean` and `number` have a `value`.
/// - `text` has its plain `text` and its `references`. Each reference is
///   `{id, path}`.
/// - `list` has its `items`.
/// - `dictionary` has its `entries`. Each entry is `[key, value]`.
/// - `mixed` has its `items`. A key in the block becomes
///   `{kind: "entry", key, value}`.
/// - `anchor` has its `id` and `name`.
/// - `reference` has its `id`, `path` and the `name` in the source.
pub fn typed(value: &Value, anchors: &dyn AnchorView) -> Json {
    match value {
        Value::Null => serde_json::json!({ "kind": "null" }),
        Value::Bool(flag) => serde_json::json!({ "kind": "boolean", "value": flag }),
        Value::Number(number) => serde_json::json!({ "kind": "number", "value": number_json(*number) }),
        Value::Str(text) => typed_text(text, anchors),
        Value::List(items) => serde_json::json!({
            "kind": "list",
            "items": items.iter().map(|item| typed(item, anchors)).collect::<Vec<_>>(),
        }),
        Value::Dict(properties) => serde_json::json!({
            "kind": "dictionary",
            "entries": typed_properties(properties, anchors),
        }),
        Value::Mixed(mixed) => serde_json::json!({
            "kind": "mixed",
            "items": mixed
                .items
                .iter()
                .map(|item| match item {
                    MixedItem::Text(text) => typed_text(text, anchors),
                    MixedItem::List(items) => serde_json::json!({
                        "kind": "list",
                        "items": items.iter().map(|item| typed(item, anchors)).collect::<Vec<_>>(),
                    }),
                    MixedItem::Entry(key, value) => serde_json::json!({
                        "kind": "entry",
                        "key": key,
                        "value": typed(value, anchors),
                    }),
                    MixedItem::Value(value) => typed(value, anchors),
                })
                .collect::<Vec<_>>(),
        }),
        Value::Anchor(anchor) => serde_json::json!({
            "kind": "anchor",
            "id": anchor.0,
            "name": anchors.name(*anchor),
        }),
        Value::Reference(reference) => serde_json::json!({
            "kind": "reference",
            "id": reference.anchor.0,
            "path": reference.path,
            "name": reference.display(anchors),
        }),
    }
}

/// Properties as `[key, value]` pairs in their order. Each value is [`typed`].
pub fn typed_properties(properties: &Properties, anchors: &dyn AnchorView) -> Json {
    Json::Array(
        properties
            .iter()
            .map(|(key, item)| serde_json::json!([key, typed(item, anchors)]))
            .collect(),
    )
}

fn typed_text(text: &piton_core::Text, anchors: &dyn AnchorView) -> Json {
    serde_json::json!({
        "kind": "text",
        "text": text.render_plain(anchors),
        "references": text
            .refs()
            .map(|target| serde_json::json!({ "id": target.anchor.0, "path": target.path }))
            .collect::<Vec<_>>(),
    })
}
