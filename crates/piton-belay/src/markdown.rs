//! Turning Piton values into the Markdown that agents read.
//!
//! Everything Belay emits is prose, so every value has to become text. Simple
//! values render as themselves, lists become bullets, a dictionary of scalars
//! becomes an indented block, and anything with structure becomes headers whose
//! level tracks depth — falling back to bold past the sixth level.

use piton_core::value::{Dict, Value};

/// The deepest header Markdown has.
const MAX_HEADER: usize = 6;

/// Render a value as a Markdown block starting at header `depth`.
pub fn block(value: &Value, depth: usize) -> String {
    match value {
        Value::List(list) if list.items.is_empty() => String::new(),
        Value::List(list) if list.implicit => list
            .items
            .iter()
            .map(|item| block(item, depth))
            .filter(|text| !text.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n\n"),
        Value::List(list) => bullets(&list.items, 0),
        Value::Dict(dict) if dict.is_empty() => String::new(),
        Value::Dict(dict) if is_flat(dict) => indented(dict, 0),
        Value::Dict(dict) => headers(dict, depth),
        Value::Anchor(anchor) if anchor.props.is_empty() => String::new(),
        Value::Anchor(anchor) if is_flat(&anchor.props) => indented(&anchor.props, 0),
        Value::Anchor(anchor) => headers(&anchor.props, depth),
        simple => simple.to_literal(),
    }
}

/// Render only the properties a caller has not already consumed.
pub fn remainder(props: &Dict, consumed: &[&str], depth: usize) -> String {
    let rest: Dict = props
        .iter()
        .filter(|(key, _)| !consumed.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if rest.is_empty() {
        return String::new();
    }
    headers(&rest, depth)
}

/// One header per key, with the value's block beneath it.
pub fn headers(dict: &Dict, depth: usize) -> String {
    let mut sections = Vec::new();
    for (key, value) in dict {
        let body = block(value, depth + 1);
        let heading = heading(key, depth);
        sections.push(if body.trim().is_empty() {
            heading
        } else {
            format!("{heading}\n\n{}", body.trim_end())
        });
    }
    sections.join("\n\n")
}

/// `## Some Property`, or bold text once headers run out.
pub fn heading(key: &str, depth: usize) -> String {
    let title = humanize(key);
    if depth <= MAX_HEADER {
        format!("{} {title}", "#".repeat(depth.max(1)))
    } else {
        format!("**{title}**")
    }
}

/// A bullet list, nesting deeper lists by indentation.
fn bullets(items: &[Value], indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let mut lines = Vec::new();
    for item in items {
        match item {
            Value::List(nested) if !nested.items.is_empty() => {
                lines.push(bullets(&nested.items, indent + 1));
            }
            Value::Dict(dict) if !dict.is_empty() => {
                lines.push(format!("{pad}-\n{}", indented(dict, indent + 1)));
            }
            Value::Anchor(anchor) if !anchor.props.is_empty() => {
                lines.push(format!("{pad}-\n{}", indented(&anchor.props, indent + 1)));
            }
            other => {
                let text = block(other, MAX_HEADER + 1);
                lines.push(format!("{pad}- {}", text.replace('\n', &format!("\n{pad}  "))));
            }
        }
    }
    lines.join("\n")
}

/// `key:` lines whose indentation mirrors the structure.
fn indented(dict: &Dict, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let mut lines = Vec::new();
    for (key, value) in dict {
        match value {
            Value::Dict(nested) if !nested.is_empty() => {
                lines.push(format!("{pad}{key}:"));
                lines.push(indented(nested, indent + 1));
            }
            Value::Anchor(anchor) if !anchor.props.is_empty() => {
                lines.push(format!("{pad}{key}:"));
                lines.push(indented(&anchor.props, indent + 1));
            }
            Value::List(list) if !list.items.is_empty() => {
                lines.push(format!("{pad}{key}:"));
                lines.push(bullets(&list.items, indent + 1));
            }
            simple => lines.push(format!("{pad}{key}: {}", simple.to_literal())),
        }
    }
    lines.join("\n")
}

/// True when every value nests only more scalars, so indentation reads well.
fn is_flat(dict: &Dict) -> bool {
    dict.values().all(|value| match value {
        Value::Dict(nested) => is_flat(nested),
        other => other.is_simple(),
    })
}

/// `myProperty` becomes `My Property`, `code-root` becomes `Code Root`.
pub fn humanize(name: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in name.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        if ch.is_uppercase() && !current.is_empty() && !current.ends_with(char::is_uppercase) {
            words.push(std::mem::take(&mut current));
        }
        current.push(ch);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
        .iter()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `MyAnchor` becomes `my-anchor`, for file names and front matter.
pub fn kebab(name: &str) -> String {
    let mut out = String::new();
    for (index, ch) in name.chars().enumerate() {
        if ch == '_' || ch == ' ' {
            out.push('-');
        } else if ch.is_uppercase() {
            if index > 0 && !out.ends_with('-') {
                out.push('-');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// A YAML front-matter block, omitting keys with no value.
pub fn front_matter(entries: &[(&str, Option<String>)]) -> String {
    let mut out = String::from("---\n");
    for (key, value) in entries {
        if let Some(value) = value {
            if !value.trim().is_empty() {
                out.push_str(&format!("{key}: {}\n", scalar(value)));
            }
        }
    }
    out.push_str("---\n");
    out
}

/// Quote a front-matter scalar only when YAML needs it.
fn scalar(text: &str) -> String {
    let one_line = text.replace('\n', " ");
    let needs_quotes = one_line.trim() != one_line
        || one_line.contains(": ")
        || one_line.ends_with(':')
        || one_line.starts_with(['-', '[', '{', '#', '&', '*', '!', '|', '>', '\'', '"', '%', '@', '`']);
    if needs_quotes {
        format!("{one_line:?}")
    } else {
        one_line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_become_words() {
        assert_eq!(humanize("myProperty"), "My Property");
        assert_eq!(humanize("code-root"), "Code Root");
        assert_eq!(humanize("useWhen"), "Use When");
        assert_eq!(kebab("ButtonComponent"), "button-component");
        assert_eq!(kebab("myAgent"), "my-agent");
    }

    fn dict(entries: Vec<(&str, Value)>) -> Dict {
        entries.into_iter().map(|(key, value)| (key.to_string(), value)).collect()
    }

    #[test]
    fn simple_values_serialize_as_their_literal() {
        assert_eq!(block(&Value::Bool(false), 1), "false");
        assert_eq!(block(&Value::number(42.0), 1), "42");
        assert_eq!(block(&Value::Null, 1), "null");
        assert_eq!(block(&Value::string("text"), 1), "text");
    }

    #[test]
    fn lists_become_bullets() {
        let list = Value::list(vec![
            Value::string("List"),
            Value::string("of"),
            Value::string("Items"),
        ]);
        assert_eq!(block(&list, 1), "- List\n- of\n- Items");
    }

    #[test]
    fn a_dictionary_of_scalars_indents() {
        let inner = dict(vec![("thirdProperty", Value::string("value"))]);
        let middle = dict(vec![("secondProperty", Value::Dict(inner))]);
        let outer = dict(vec![("firstProperty", Value::Dict(middle))]);
        assert_eq!(
            block(&Value::Dict(outer), 1),
            "firstProperty:\n  secondProperty:\n    thirdProperty: value"
        );
    }

    #[test]
    fn structure_becomes_headers_that_track_depth() {
        let nested = dict(vec![("nested", Value::string("objects"))]);
        let deep = dict(vec![("even", Value::Dict(nested))]);
        let mixed = Value::List(piton_core::value::List::implicit(vec![
            Value::string("String item"),
            Value::list(vec![Value::string("List"), Value::string("of"), Value::string("items")]),
            Value::Dict(dict(vec![("and", Value::Dict(deep))])),
        ]));
        let document = dict(vec![("firstProperty", mixed)]);
        let rendered = block(&Value::Dict(document), 1);
        assert!(rendered.starts_with("# First Property"), "{rendered}");
        assert!(rendered.contains("String item"), "{rendered}");
        assert!(rendered.contains("- List\n- of\n- items"), "{rendered}");
        assert!(rendered.contains("and:\n  even:\n    nested: objects"), "{rendered}");
    }

    #[test]
    fn headers_stop_at_six_levels() {
        assert_eq!(heading("a", 1), "# A");
        assert_eq!(heading("a", 6), "###### A");
        assert_eq!(heading("a", 7), "**A**");
    }
}
