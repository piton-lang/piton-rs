//! JSON serialization.
//!
//! Anchors serialize as their resolved content. References do not: a reference
//! must not silently become an embedded copy or a bare name, so it serializes
//! as an explicit `$ref` object naming the anchor and the file it came from.

use piton_core::{format_number, AnchorId, AnchorView, Properties, Value};

/// How much to indent each nesting level.
const INDENT: usize = 2;

/// Renders a value as JSON.
pub fn value(value: &Value, anchors: &dyn AnchorView) -> String {
    let mut out = String::new();
    write_value(value, anchors, 0, &mut out, &mut Vec::new());
    out
}

/// Renders a map of top-level declarations as a JSON object.
pub fn declarations(map: &Properties, anchors: &dyn AnchorView) -> String {
    let mut out = String::new();
    write_value(&Value::Dict(map.clone()), anchors, 0, &mut out, &mut Vec::new());
    out.push('\n');
    out
}

fn write_value(
    value: &Value,
    anchors: &dyn AnchorView,
    depth: usize,
    out: &mut String,
    stack: &mut Vec<AnchorId>,
) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&format_number(*n)),
        Value::Str(text) => out.push_str(&quote(&text.render_plain(anchors))),
        Value::List(items) => write_array(items, anchors, depth, out, stack),
        Value::Mixed(_) => write_array(&value.as_list_items(), anchors, depth, out, stack),
        Value::Dict(map) => write_object(map, anchors, depth, out, stack),
        Value::Anchor(id) => {
            if stack.contains(id) {
                // A value that embeds itself cannot be written out; emit the
                // reference form instead of recursing forever.
                out.push_str(&reference(*id, anchors));
                return;
            }
            stack.push(*id);
            write_object(anchors.properties(*id), anchors, depth, out, stack);
            stack.pop();
        }
        Value::Reference(id) => out.push_str(&reference(*id, anchors)),
    }
}

fn reference(id: AnchorId, anchors: &dyn AnchorView) -> String {
    format!(
        "{{\"$ref\": {}}}",
        quote(&format!(
            "{}#{}",
            anchors.source_path(id).display(),
            anchors.name(id)
        ))
    )
}

fn write_array(
    items: &[Value],
    anchors: &dyn AnchorView,
    depth: usize,
    out: &mut String,
    stack: &mut Vec<AnchorId>,
) {
    if items.is_empty() {
        out.push_str("[]");
        return;
    }
    let pad = " ".repeat((depth + 1) * INDENT);
    out.push_str("[\n");
    for (index, item) in items.iter().enumerate() {
        out.push_str(&pad);
        write_value(item, anchors, depth + 1, out, stack);
        if index + 1 < items.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str(&" ".repeat(depth * INDENT));
    out.push(']');
}

fn write_object(
    map: &Properties,
    anchors: &dyn AnchorView,
    depth: usize,
    out: &mut String,
    stack: &mut Vec<AnchorId>,
) {
    if map.is_empty() {
        out.push_str("{}");
        return;
    }
    let pad = " ".repeat((depth + 1) * INDENT);
    out.push_str("{\n");
    for (index, (key, value)) in map.iter().enumerate() {
        out.push_str(&pad);
        out.push_str(&quote(key));
        out.push_str(": ");
        write_value(value, anchors, depth + 1, out, stack);
        if index + 1 < map.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str(&" ".repeat(depth * INDENT));
    out.push('}');
}

/// Quotes and escapes a JSON string.
pub fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use piton_core::{EmptyAnchors, Mixed, MixedItem};

    fn props(pairs: Vec<(&str, Value)>) -> Properties {
        let mut map = Properties::new();
        for (name, value) in pairs {
            map.insert(name.to_string(), value);
        }
        map
    }

    #[test]
    fn scalars_use_the_documented_forms() {
        assert_eq!(value(&Value::Number(42.0), &EmptyAnchors), "42");
        assert_eq!(value(&Value::Number(3.14), &EmptyAnchors), "3.14");
        assert_eq!(value(&Value::Bool(true), &EmptyAnchors), "true");
        assert_eq!(value(&Value::Null, &EmptyAnchors), "null");
    }

    #[test]
    fn mixed_blocks_group_adjacent_entries() {
        let mixed = Value::Mixed(Mixed::new(vec![
            MixedItem::Text(piton_core::Text::plain("This is a string")),
            MixedItem::List(vec![Value::string("This"), Value::string("Is")]),
            MixedItem::Entry(
                "nestedDictionary".into(),
                Value::Dict(props(vec![("deeplyNested", Value::string("x"))])),
            ),
        ]));
        let rendered = value(&mixed, &EmptyAnchors);
        assert_eq!(
            rendered,
            "[\n  \"This is a string\",\n  [\n    \"This\",\n    \"Is\"\n  ],\n  {\n    \"nestedDictionary\": {\n      \"deeplyNested\": \"x\"\n    }\n  }\n]"
        );
    }

    #[test]
    fn strings_escape_control_characters() {
        assert_eq!(quote("a\"b\nc"), "\"a\\\"b\\nc\"");
    }
}
