//! Serialising values to the data formats `piton compile` targets directly.
//!
//! Markdown is not here: it belongs to whichever framework is producing prose,
//! and the core compiler stays out of that.

use crate::value::{Dict, Num, Value};

/// Convert a value to `serde_json`'s model. Anchors become their properties.
pub fn to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(flag) => serde_json::Value::Bool(*flag),
        Value::Number(number) => number_to_json(number),
        Value::Str(text) => serde_json::Value::String(text.clone()),
        Value::List(list) => serde_json::Value::Array(list.items.iter().map(to_json).collect()),
        Value::Dict(dict) => dict_to_json(dict),
        Value::Anchor(anchor) => dict_to_json(&anchor.props),
    }
}

/// Piton has one number type, but JSON has two spellings for it, and `3` is the
/// one every reader expects for a whole number. A value that lost nothing on
/// the way to an integer is written as one; everything else stays a float.
fn number_to_json(number: &Num) -> serde_json::Value {
    if number.value.is_finite() && number.value.fract() == 0.0 && !number.spelled_as_float() {
        if let Some(exact) = to_i64(number.value) {
            return serde_json::Value::Number(exact.into());
        }
    }
    serde_json::Number::from_f64(number.value)
        .map(serde_json::Value::Number)
        .unwrap_or(serde_json::Value::Null)
}

/// `f64` covers every `i64` only up to 2^53; past that the cast would invent
/// precision the value never had.
fn to_i64(value: f64) -> Option<i64> {
    (value.abs() <= 9_007_199_254_740_992.0).then_some(value as i64)
}

fn dict_to_json(dict: &Dict) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (key, value) in dict {
        map.insert(key.clone(), to_json(value));
    }
    serde_json::Value::Object(map)
}

/// Render a value as JSON text.
pub fn to_json_string(value: &Value, pretty: bool) -> String {
    let json = to_json(value);
    if pretty {
        serde_json::to_string_pretty(&json).unwrap_or_default()
    } else {
        serde_json::to_string(&json).unwrap_or_default()
    }
}

/// Render a value as YAML text.
pub fn to_yaml(value: &Value) -> String {
    let mut out = String::new();
    write_yaml(value, 0, &mut out, true);
    out
}

fn write_yaml(value: &Value, indent: usize, out: &mut String, at_line_start: bool) {
    let pad = "  ".repeat(indent);
    match value {
        Value::List(list) if !list.items.is_empty() => {
            if !at_line_start {
                out.push('\n');
            }
            for item in &list.items {
                out.push_str(&pad);
                out.push_str("- ");
                write_nested(item, indent + 1, out);
            }
        }
        Value::Dict(dict) if !dict.is_empty() => {
            if !at_line_start {
                out.push('\n');
            }
            write_mapping(dict, indent, out);
        }
        Value::Anchor(anchor) if !anchor.props.is_empty() => {
            if !at_line_start {
                out.push('\n');
            }
            write_mapping(&anchor.props, indent, out);
        }
        scalar => {
            out.push_str(&scalar_yaml(scalar));
            out.push('\n');
        }
    }
}

fn write_mapping(dict: &Dict, indent: usize, out: &mut String) {
    let pad = "  ".repeat(indent);
    for (key, value) in dict {
        out.push_str(&pad);
        out.push_str(&yaml_key(key));
        out.push(':');
        if is_scalar(value) {
            out.push(' ');
            out.push_str(&scalar_yaml(value));
            out.push('\n');
        } else {
            write_yaml(value, indent + 1, out, false);
        }
    }
}

/// Write a value that follows a `- ` marker already on the line.
fn write_nested(value: &Value, indent: usize, out: &mut String) {
    if is_scalar(value) {
        out.push_str(&scalar_yaml(value));
        out.push('\n');
        return;
    }
    out.push('\n');
    write_yaml(value, indent, out, true);
}

fn is_scalar(value: &Value) -> bool {
    match value {
        Value::List(list) => list.items.is_empty(),
        Value::Dict(dict) => dict.is_empty(),
        Value::Anchor(anchor) => anchor.props.is_empty(),
        _ => true,
    }
}

fn scalar_yaml(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::Str(text) => quote_yaml(text),
        Value::List(_) => "[]".to_string(),
        _ => "{}".to_string(),
    }
}

fn yaml_key(key: &str) -> String {
    if key.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') && !key.is_empty() {
        key.to_string()
    } else {
        format!("{key:?}")
    }
}

/// Quote a scalar only when YAML would otherwise misread it.
fn quote_yaml(text: &str) -> String {
    let ambiguous = text.is_empty()
        || text.trim() != text
        || text.contains('\n')
        || text.contains(": ")
        || text.ends_with(':')
        || text.parse::<f64>().is_ok()
        || matches!(text, "true" | "false" | "null" | "yes" | "no" | "~")
        || text.starts_with(['-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%', '@', '`']);
    if ambiguous {
        format!("{text:?}")
    } else {
        text.to_string()
    }
}
