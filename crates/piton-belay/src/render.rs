//! Rendering construct content into target-specific containers.
//!
//! The Markdown body rules are common to every target; what differs is the
//! frontmatter or document format wrapped around them.

use indexmap::IndexMap;
use piton_core::{Properties, Value};
use piton_emit::{markdown, yaml};

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
            Value::Str(text) => Some(FieldValue::Text(text.render_plain(anchors))),
            Value::Bool(b) => Some(FieldValue::Bool(*b)),
            Value::Number(n) => Some(FieldValue::Number(*n)),
            Value::Null => None,
            Value::List(items) => {
                let mut out = Vec::new();
                for item in items {
                    match item {
                        Value::Str(text) => out.push(text.render_plain(anchors)),
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

fn write_yaml_fields(fields: &Fields, indent: usize, out: &mut String) {
    let pad = " ".repeat(indent);
    for (key, value) in fields {
        match value {
            FieldValue::Text(text) => {
                out.push_str(&format!("{pad}{key}: {}\n", yaml::quote(text)));
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
                        out.push_str(&format!("{pad}  - {}\n", yaml::quote(item)));
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

/// Encodes a TOML string, using a multi-line literal for text with newlines so
/// a whole instruction body stays readable.
pub fn toml_string(text: &str) -> String {
    if text.contains('\n') {
        let escaped = text.replace('\\', "\\\\").replace("\"\"\"", "\\\"\\\"\\\"");
        return format!("\"\"\"\n{escaped}\"\"\"");
    }
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Serializes the properties that have no defined place in the artifact, as a
/// run of top-level sections after the primary body.
pub fn remainder(properties: &Properties, context: &markdown::Context<'_>) -> String {
    if properties.is_empty() {
        return String::new();
    }
    markdown::sections(properties, 1, context)
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

/// Builds an agent's role introduction.
///
/// The body begins with `You are a` followed by the role; a role that already
/// carries its own article keeps it rather than gaining a second one.
pub fn role_introduction(role: &str) -> String {
    let trimmed = role.trim();
    let lowered = trimmed.to_ascii_lowercase();
    if lowered.starts_with("a ")
        || lowered.starts_with("an ")
        || lowered.starts_with("the ")
    {
        format!("You are {trimmed}")
    } else {
        format!("You are a {trimmed}")
    }
}

/// Builds a skill's discovery description: the description, then `Use when`
/// and the `useWhen` text.
pub fn discovery_description(description: &str, use_when: &str) -> String {
    let description = description.trim();
    let use_when = use_when.trim();
    if use_when.is_empty() {
        return description.to_string();
    }
    let separator = if description.ends_with(['.', '!', '?']) || description.is_empty() {
        ""
    } else {
        "."
    };
    format!("{description}{separator} Use when {use_when}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_descriptions_match_the_generated_skills() {
        assert_eq!(
            discovery_description(
                "Reads the spec for the Piton language and answers questions.",
                "Explicitly Invoked"
            ),
            "Reads the spec for the Piton language and answers questions. Use when Explicitly Invoked"
        );
    }

    #[test]
    fn a_description_without_terminal_punctuation_gains_one() {
        assert_eq!(
            discovery_description("Does a thing", "asked"),
            "Does a thing. Use when asked"
        );
    }

    #[test]
    fn role_introductions_do_not_double_the_article() {
        assert_eq!(role_introduction("careful reviewer"), "You are a careful reviewer");
        assert_eq!(role_introduction("an architect"), "You are an architect");
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
