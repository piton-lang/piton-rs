//! Rendering construct content into target-specific containers.
//!
//! The Markdown body rules are common to every target; what differs is the
//! frontmatter or document format wrapped around them.

use indexmap::IndexMap;
use piton_core::{Properties, Value};
use piton_emit::markdown;

/// An ordered set of frontmatter fields.
pub type Fields = IndexMap<String, FieldValue>;

/// A frontmatter value. Keeping these typed means the encoder can quote each
/// one according to the format rather than guessing from its text.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    Text(String),
    Bool(bool),
    Number(f64),
    List(Vec<String>),
    Map(IndexMap<String, FieldValue>),
}

impl FieldValue {
    /// Converts a resolved Piton value into a frontmatter field, if the value
    /// has a representation there.
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

/// Encodes fields as a YAML frontmatter block, delimiters included.
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

/// Encodes a TOML string, using a multi-line basic string for text with
/// newlines so a whole instruction body stays readable.
///
/// Control characters other than tab and newline are not allowed raw in
/// either form, so they are escaped; so is any quote that would otherwise run
/// into a closing delimiter.
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
                // Escape a quote that starts a run of three, or that ends the
                // text and would merge with the closing delimiter.
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

/// Quotes a YAML scalar when its bare form would be read as something other
/// than the same string.
///
/// Both YAML 1.1 and 1.2 readers are in use, so anything either would resolve
/// to a boolean, null, number or timestamp is quoted, whatever its case.
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

/// True for anything a YAML reader could take as a number: decimal, float,
/// exponent, hex, octal, binary, sexagesimal, `.inf` and `.nan`, with
/// underscores and a sign allowed.
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
    // Sexagesimal integers such as 1:30, which YAML 1.1 reads as 90.
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

/// Serializes the properties that have no defined place in the artifact, as a
/// run of sections after the primary body, starting at `level`.
pub fn remainder(properties: &Properties, level: usize, context: &markdown::Context<'_>) -> String {
    if properties.is_empty() {
        return String::new();
    }
    markdown::sections(properties, level, context)
}

/// Joins a primary body with its serialized remainder.
pub fn body(primary: &str, remainder: &str) -> String {
    let primary = primary.trim_end();
    let remainder = remainder.trim_end();
    match (primary.is_empty(), remainder.is_empty()) {
        (true, true) => String::new(),
        (true, false) => format!("{remainder}\n"),
        (false, true) => format!("{primary}\n"),
        (false, false) => format!("{primary}\n\n{remainder}\n"),
    }
}

/// Builds an agent's role introduction: the body begins with `You are a`
/// followed by the role, as the specification words it.
pub fn role_introduction(role: &str) -> String {
    format!("You are a {}", role.trim())
}

/// Builds a skill's discovery description: the description, then `Use when`
/// and the `useWhen` text, separated by single spaces.
pub fn discovery_description(description: &str, use_when: &str) -> String {
    let parts: Vec<String> = [
        description.trim().to_string(),
        if use_when.trim().is_empty() {
            String::new()
        } else {
            format!("Use when {}", use_when.trim())
        },
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect();
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_descriptions_follow_the_description_with_use_when() {
        assert_eq!(
            discovery_description(
                "Reads the spec for the Piton language and answers questions.",
                "Explicitly Invoked"
            ),
            "Reads the spec for the Piton language and answers questions. Use when Explicitly Invoked"
        );
        assert_eq!(
            discovery_description("Does a thing", "asked"),
            "Does a thing Use when asked"
        );
    }

    #[test]
    fn a_missing_description_leaves_no_leading_space() {
        assert_eq!(discovery_description("", "asked"), "Use when asked");
    }

    #[test]
    fn role_introductions_follow_the_specification_literally() {
        assert_eq!(role_introduction("careful reviewer"), "You are a careful reviewer");
    }

    #[test]
    fn yaml_scalars_that_would_change_type_are_quoted_whatever_their_case() {
        for text in ["True", "NO", "Off", "Null", "y", "0x1F", "0o17", "017", "1_000", "1e3", ".Inf", "2026-09-21", "1:30"] {
            assert!(yaml_scalar(text).starts_with('"'), "{text}");
        }
        for text in ["build-tooling", "Reads the spec.", "x-release"] {
            assert_eq!(yaml_scalar(text), text);
        }
        assert_eq!(yaml_scalar("a\u{1}b"), "\"a\\x01b\"");
    }

    #[test]
    fn toml_strings_escape_control_characters() {
        assert_eq!(toml_string("a\u{1}b"), "\"a\\u0001b\"");
        assert_eq!(toml_string("a\nb\u{8}"), "\"\"\"\na\nb\\u0008\"\"\"");
        assert_eq!(toml_string("a\nsaid \"\"\"x\""), "\"\"\"\na\nsaid \\\"\"\"x\\\"\"\"\"");
    }

    #[test]
    fn frontmatter_quotes_only_what_needs_it() {
        let mut fields = Fields::new();
        fields.insert("name".into(), FieldValue::Text("build-tooling".into()));
        fields.insert("disable-model-invocation".into(), FieldValue::Bool(true));
        assert_eq!(
            frontmatter(&fields),
            "---\nname: build-tooling\ndisable-model-invocation: true\n---\n"
        );
    }

    #[test]
    fn nested_frontmatter_indents() {
        let mut policy = IndexMap::new();
        policy.insert("allow_implicit_invocation".to_string(), FieldValue::Bool(false));
        let mut fields = Fields::new();
        fields.insert("policy".into(), FieldValue::Map(policy));
        assert_eq!(
            frontmatter(&fields),
            "---\npolicy:\n  allow_implicit_invocation: false\n---\n"
        );
    }

    #[test]
    fn multiline_toml_strings_use_a_literal_block() {
        let rendered = toml_string("first\nsecond\n");
        assert_eq!(rendered, "\"\"\"\nfirst\nsecond\n\"\"\"");
    }

    #[test]
    fn toml_documents_put_tables_last() {
        let mut fields = Fields::new();
        fields.insert("name".into(), FieldValue::Text("reviewer".into()));
        let mut table = IndexMap::new();
        table.insert("mode".to_string(), FieldValue::Text("subagent".into()));
        fields.insert("options".into(), FieldValue::Map(table));
        fields.insert("model".into(), FieldValue::Text("fast".into()));
        assert_eq!(
            toml_document(&fields),
            "name = \"reviewer\"\nmodel = \"fast\"\n\n[options]\nmode = \"subagent\"\n"
        );
    }
}
