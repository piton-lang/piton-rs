//! Typed fields for target metadata. The formats are YAML frontmatter, YAML
//! documents and TOML. A real encoder encodes each format. Thus text with
//! quotes, colons or new lines is always valid in the output.
//!
//! Belay writes its frontmatter and agent definitions with these fields. The
//! plugin host gives them to all plugins.

use indexmap::IndexMap;
use piton_core::Value;

/// An ordered set of frontmatter fields.
pub type Fields = IndexMap<String, FieldValue>;

/// A frontmatter value. The values have types. Thus the encoder quotes each
/// value as the format requires. It does not guess from the text.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    Text(String),
    Bool(bool),
    Number(f64),
    List(Vec<String>),
    Map(IndexMap<String, FieldValue>),
}

impl FieldValue {
    /// Converts a resolved Piton value into a frontmatter field. It does this
    /// only when the value has a representation in frontmatter.
    pub fn from_value(value: &Value, anchors: &dyn piton_core::AnchorView) -> Option<FieldValue> {
        match value {
            // A reference has no representation in target metadata.
            Value::Str(text) if !text.is_plain() => None,
            Value::Str(text) => Some(FieldValue::Text(text.render_plain(anchors))),
            Value::Bool(b) => Some(FieldValue::Bool(*b)),
            Value::Number(n) => Some(FieldValue::Number(*n)),
            Value::Null => None,
            Value::List(items) => {
                let mut out = Vec::new();
                for item in items {
                    match item {
                        Value::Str(text) if text.is_plain() => {
                            out.push(text.render_plain(anchors))
                        }
                        Value::Number(n) => out.push(piton_core::format_number(*n)),
                        Value::Bool(b) => out.push(b.to_string()),
                        _ => return None,
                    }
                }
                Some(FieldValue::List(out))
            }
            Value::Dict(map) => {
                let mut out = IndexMap::new();
                for (key, item) in map {
                    out.insert(key.clone(), FieldValue::from_value(item, anchors)?);
                }
                Some(FieldValue::Map(out))
            }
            _ => None,
        }
    }
}

/// Encodes fields as a YAML frontmatter block, with the delimiters.
pub fn frontmatter(fields: &Fields) -> String {
    let mut out = String::from("---\n");
    write_yaml_fields(fields, 0, &mut out);
    out.push_str("---\n");
    out
}

/// Encodes fields as a standalone YAML document.
pub fn yaml_document(fields: &Fields) -> String {
    let mut out = String::new();
    write_yaml_fields(fields, 0, &mut out);
    out
}

fn write_yaml_fields(fields: &Fields, indent: usize, out: &mut String) {
    let pad = " ".repeat(indent);
    for (key, value) in fields {
        match value {
            FieldValue::Text(text) => {
                out.push_str(&format!("{pad}{key}: {}\n", yaml_scalar(text)));
            }
            FieldValue::Bool(b) => out.push_str(&format!("{pad}{key}: {b}\n")),
            FieldValue::Number(n) => {
                out.push_str(&format!("{pad}{key}: {}\n", piton_core::format_number(*n)))
            }
            FieldValue::List(items) => {
                if items.is_empty() {
                    out.push_str(&format!("{pad}{key}: []\n"));
                } else {
                    out.push_str(&format!("{pad}{key}:\n"));
                    for item in items {
                        out.push_str(&format!("{pad}  - {}\n", yaml_scalar(item)));
                    }
                }
            }
            FieldValue::Map(map) => {
                out.push_str(&format!("{pad}{key}:\n"));
                write_yaml_fields(map, indent + 2, out);
            }
        }
    }
}

/// Encodes fields as a TOML document.
pub fn toml_document(fields: &Fields) -> String {
    let mut out = String::new();
    let mut tables: Vec<(&String, &IndexMap<String, FieldValue>)> = Vec::new();
    for (key, value) in fields {
        match value {
            FieldValue::Map(map) => tables.push((key, map)),
            other => {
                out.push_str(&format!("{key} = {}\n", toml_value(other)));
            }
        }
    }
    for (key, map) in tables {
        out.push_str(&format!("\n[{key}]\n"));
        for (inner_key, value) in map {
            out.push_str(&format!("{inner_key} = {}\n", toml_value(value)));
        }
    }
    out
}

fn toml_value(value: &FieldValue) -> String {
    match value {
        FieldValue::Text(text) => toml_string(text),
        FieldValue::Bool(b) => b.to_string(),
        FieldValue::Number(n) => piton_core::format_number(*n),
        FieldValue::List(items) => {
            let rendered: Vec<String> = items.iter().map(|item| toml_string(item)).collect();
            format!("[{}]", rendered.join(", "))
        }
        FieldValue::Map(_) => "{}".to_string(),
    }
}

/// Encodes a TOML string. Text with newlines uses a multi-line basic string.
/// Thus a full instruction body stays easy to read.
///
/// The two forms do not allow raw control characters, other than tab and newline.
/// Thus the encoder escapes them. It also escapes a quote at the end.
/// If not, that quote joins the last delimiter.
pub fn toml_string(text: &str) -> String {
    let escape = |ch: char, out: &mut String| match ch {
        '\\' => out.push_str("\\\\"),
        '\r' => out.push_str("\\r"),
        c if (c as u32) < 0x20 && c != '\t' && c != '\n' => {
            out.push_str(&format!("\\u{:04X}", c as u32))
        }
        '\u{7f}' => out.push_str("\\u007F"),
        c => out.push(c),
    };
    if text.contains('\n') {
        let mut out = String::from("\"\"\"\n");
        let chars: Vec<char> = text.chars().collect();
        for (index, ch) in chars.iter().enumerate() {
            if *ch == '"' {
                // Escape a quote that starts a run of three quotes. Also escape
                // a quote at the end of the text. It can join the delimiter
                // at the end.
                let next_two = chars.get(index + 1) == Some(&'"') && chars.get(index + 2) == Some(&'"');
                if next_two || index + 1 == chars.len() {
                    out.push_str("\\\"");
                    continue;
                }
                out.push('"');
                continue;
            }
            escape(*ch, &mut out);
        }
        out.push_str("\"\"\"");
        return out;
    }
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\t' => out.push_str("\\t"),
            c => escape(c, &mut out),
        }
    }
    out.push('"');
    out
}

/// Quotes a YAML scalar when a reader does not read the bare form as the same
/// string.
///
/// Tools use both YAML 1.1 and 1.2 readers. Thus the function quotes all text
/// that one of the two readers resolves to a boolean, null, number or timestamp. The
/// letter case has no effect.
pub fn yaml_scalar(text: &str) -> String {
    if !yaml_needs_quotes(text) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                out.push_str(&format!("\\x{:02X}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn yaml_needs_quotes(text: &str) -> bool {
    if text.is_empty()
        || text != text.trim()
        || text.chars().any(|c| (c as u32) < 0x20 || c == '\u{7f}')
        || text.starts_with([
            '-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%',
            '@', '`',
        ])
        || text.contains(": ")
        || text.ends_with(':')
        || text.contains(" #")
    {
        return true;
    }
    let lowered = text.to_ascii_lowercase();
    if matches!(
        lowered.as_str(),
        "true" | "false" | "yes" | "no" | "y" | "n" | "on" | "off" | "null" | "~"
    ) {
        return true;
    }
    looks_numeric(&lowered) || looks_like_timestamp(text)
}

/// True for all text that a YAML reader can read as a number. This includes
/// decimal, float, exponent, hex, octal, binary, sexagesimal, `.inf` and
/// `.nan`. The text can also have underscores and a sign.
fn looks_numeric(lowered: &str) -> bool {
    let unsigned = lowered.trim_start_matches(['+', '-']);
    if matches!(unsigned, ".inf" | ".nan" | "inf" | "nan") {
        return true;
    }
    let digits = unsigned.replace('_', "");
    if digits.is_empty() {
        return false;
    }
    for prefix in ["0x", "0o", "0b"] {
        if let Some(rest) = digits.strip_prefix(prefix) {
            return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_hexdigit());
        }
    }
    if digits.parse::<f64>().is_ok() {
        return true;
    }
    // Sexagesimal integers such as 1:30. YAML 1.1 reads 1:30 as 90.
    digits.contains(':')
        && digits
            .split(':')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit() || c == '.'))
}

/// True for a date such as 2026-09-21, which YAML 1.1 reads as a timestamp.
fn looks_like_timestamp(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 8
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..].iter().take(2).any(u8::is_ascii_digit)
}

