//! YAML serialization.
//!
//! The value model maps onto YAML directly. Scalars are quoted only when
//! leaving them bare would change how YAML reads them, which keeps prose
//! readable while staying unambiguous.

use piton_core::{format_number, AnchorId, AnchorView, Properties, Value};

/// Renders a map of top-level declarations as a YAML document.
pub fn declarations(map: &Properties, anchors: &dyn AnchorView) -> String {
    let mut out = String::new();
    write_map(map, anchors, 0, &mut out, &mut Vec::new());
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Renders a single value as a YAML document.
pub fn value(value: &Value, anchors: &dyn AnchorView) -> String {
    let mut out = String::new();
    match value {
        Value::Dict(map) => write_map(map, anchors, 0, &mut out, &mut Vec::new()),
        Value::Anchor(id) => {
            write_map(anchors.properties(*id), anchors, 0, &mut out, &mut vec![*id])
        }
        Value::List(items) => write_list(items, anchors, 0, &mut out, &mut Vec::new()),
        Value::Mixed(_) => write_list(
            &value.as_list_items(),
            anchors,
            0,
            &mut out,
            &mut Vec::new(),
        ),
        scalar => out.push_str(&render_scalar(scalar, anchors)),
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn write_map(
    map: &Properties,
    anchors: &dyn AnchorView,
    indent: usize,
    out: &mut String,
    stack: &mut Vec<AnchorId>,
) {
    let pad = " ".repeat(indent);
    if map.is_empty() {
        out.push_str(&format!("{pad}{{}}\n"));
        return;
    }
    for (key, value) in map {
        out.push_str(&pad);
        out.push_str(&render_key(key));
        match value {
            Value::Dict(inner) if !inner.is_empty() => {
                out.push_str(":\n");
                write_map(inner, anchors, indent + 2, out, stack);
            }
            Value::Anchor(id) => {
                if stack.contains(id) {
                    out.push_str(&format!(": {}\n", quote(&reference_text(*id, anchors))));
                    continue;
                }
                let properties = anchors.properties(*id);
                if properties.is_empty() {
                    out.push_str(": {}\n");
                } else {
                    out.push_str(":\n");
                    stack.push(*id);
                    write_map(properties, anchors, indent + 2, out, stack);
                    stack.pop();
                }
            }
            Value::List(items) if !items.is_empty() => {
                out.push_str(":\n");
                write_list(items, anchors, indent, out, stack);
            }
            Value::List(_) => out.push_str(": []\n"),
            Value::Mixed(_) => {
                let items = value.as_list_items();
                if items.is_empty() {
                    out.push_str(": []\n");
                } else {
                    out.push_str(":\n");
                    write_list(&items, anchors, indent, out, stack);
                }
            }
            Value::Dict(_) => out.push_str(": {}\n"),
            scalar => {
                out.push_str(": ");
                out.push_str(&render_scalar(scalar, anchors));
                out.push('\n');
            }
        }
    }
}

fn write_list(
    items: &[Value],
    anchors: &dyn AnchorView,
    indent: usize,
    out: &mut String,
    stack: &mut Vec<AnchorId>,
) {
    let pad = " ".repeat(indent);
    for item in items {
        match item {
            Value::Dict(map) if !map.is_empty() => {
                let mut nested = String::new();
                write_map(map, anchors, indent + 2, &mut nested, stack);
                push_block(out, &pad, &nested);
            }
            Value::Anchor(id) => {
                if stack.contains(id) {
                    out.push_str(&format!(
                        "{pad}- {}\n",
                        quote(&reference_text(*id, anchors))
                    ));
                    continue;
                }
                stack.push(*id);
                let mut nested = String::new();
                write_map(anchors.properties(*id), anchors, indent + 2, &mut nested, stack);
                stack.pop();
                push_block(out, &pad, &nested);
            }
            Value::List(nested) if !nested.is_empty() => {
                out.push_str(&format!("{pad}-\n"));
                write_list(nested, anchors, indent + 2, out, stack);
            }
            Value::Mixed(_) => {
                let flattened = item.as_list_items();
                out.push_str(&format!("{pad}-\n"));
                write_list(&flattened, anchors, indent + 2, out, stack);
            }
            scalar => {
                out.push_str(&format!("{pad}- {}\n", render_scalar(scalar, anchors)));
            }
        }
    }
}

/// Writes a block under a `- ` bullet, moving the first line onto the bullet.
fn push_block(out: &mut String, pad: &str, block: &str) {
    let mut lines = block.lines();
    match lines.next() {
        Some(first) => out.push_str(&format!("{pad}- {}\n", first.trim_start())),
        None => {
            out.push_str(&format!("{pad}- {{}}\n"));
            return;
        }
    }
    for line in lines {
        out.push_str(line);
        out.push('\n');
    }
}

fn reference_text(id: AnchorId, anchors: &dyn AnchorView) -> String {
    format!("{}#{}", anchors.source_path(id).display(), anchors.name(id))
}

fn render_scalar(value: &Value, anchors: &dyn AnchorView) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        Value::Number(n) => format_number(*n),
        Value::Str(text) => quote(&text.render_plain(anchors)),
        Value::Reference(id) => quote(&format!("!ref {}", reference_text(*id, anchors))),
        _ => "null".to_string(),
    }
}

fn render_key(key: &str) -> String {
    if key.is_empty()
        || key.chars().any(|c| {
            c.is_whitespace() || matches!(c, ':' | '#' | '{' | '}' | '[' | ']' | ',' | '&' | '*' | '!' | '|' | '>' | '\'' | '"' | '%' | '@' | '`')
        })
    {
        quote(key)
    } else {
        key.to_string()
    }
}

/// Quotes a scalar, using a double-quoted form when the bare text would be
/// ambiguous or would not survive a round trip.
pub fn quote(text: &str) -> String {
    let needs_quotes = text.is_empty()
        || text.contains('\n')
        || text.starts_with(' ')
        || text.ends_with(' ')
        || text.starts_with(['-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%', '@', '`'])
        || text.contains(": ")
        || text.ends_with(':')
        || text.contains(" #")
        || matches!(
            text,
            "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "~"
        )
        || text.parse::<f64>().is_ok();

    if !needs_quotes {
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
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use piton_core::EmptyAnchors;

    fn props(pairs: Vec<(&str, Value)>) -> Properties {
        let mut map = Properties::new();
        for (name, value) in pairs {
            map.insert(name.to_string(), value);
        }
        map
    }

    #[test]
    fn plain_prose_stays_unquoted() {
        let map = props(vec![("description", Value::string("Adds two numbers"))]);
        assert_eq!(
            declarations(&map, &EmptyAnchors),
            "description: Adds two numbers\n"
        );
    }

    #[test]
    fn ambiguous_scalars_are_quoted() {
        let map = props(vec![
            ("a", Value::string("true")),
            ("b", Value::string("42")),
            ("c", Value::string("+")),
            ("d", Value::string("a: b")),
            ("e", Value::Bool(true)),
        ]);
        assert_eq!(
            declarations(&map, &EmptyAnchors),
            "a: \"true\"\nb: \"42\"\nc: +\nd: \"a: b\"\ne: true\n"
        );
    }

    #[test]
    fn lists_nest_under_their_key() {
        let map = props(vec![(
            "items",
            Value::List(vec![Value::string("one"), Value::string("two")]),
        )]);
        assert_eq!(
            declarations(&map, &EmptyAnchors),
            "items:\n- one\n- two\n"
        );
    }

    #[test]
    fn dictionaries_in_lists_start_on_the_bullet() {
        let entry = props(vec![
            ("description", Value::string("Adds")),
            ("symbol", Value::string("+")),
        ]);
        let map = props(vec![("operators", Value::List(vec![Value::Dict(entry)]))]);
        assert_eq!(
            declarations(&map, &EmptyAnchors),
            "operators:\n- description: Adds\n  symbol: +\n"
        );
    }
}
