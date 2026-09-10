//! Turning Piton values into the Markdown that agents read.
//!
//! Everything Belay emits is prose, so every value has to become text. Simple
//! values render as themselves, lists become bullets, a *pure dictionary* — one
//! nesting only more key/value pairs — becomes a fenced indented block, and
//! anything else becomes headers whose level tracks depth, falling back to bold
//! past the sixth level.
//!
//! Two things are never indented, however flat they look. An anchor's
//! properties are a document's sections. So are the keys of a dictionary
//! written *among prose*, inside an implicit list: the author put it there as
//! content, not as data, so it reads as a section unless it nests further
//! dictionaries — at which point the shape is the information again and has to
//! be shown as a shape.

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
            .map(|item| content(item, depth))
            .filter(|text| !text.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n\n"),
        Value::List(list) => bullets(&list.items, 0),
        Value::Dict(dict) if dict.is_empty() => String::new(),
        Value::Dict(dict) if is_flat(dict) => fenced(&indented(dict, 0)),
        Value::Dict(dict) => headers(dict, depth),
        Value::Anchor(anchor) if anchor.props.is_empty() => String::new(),
        // No flatness exception here, unlike a dictionary: an anchor's
        // properties are a document's sections, so they are always headers.
        // Collapsing them to `key: value` turned a document whose properties
        // were all prose into one unreadable line.
        Value::Anchor(anchor) => headers(&anchor.props, depth),
        simple => simple.to_literal(),
    }
}

/// Render one element of an implicit list — a value written among prose.
///
/// A dictionary here is a section of the document rather than a block of data,
/// so its keys become headings. It goes back to being data as soon as it nests
/// another dictionary, because then the nesting itself is what it has to say.
fn content(value: &Value, depth: usize) -> String {
    match value {
        Value::Dict(dict) if !dict.is_empty() && !nests_dictionaries(dict) => {
            headers(dict, depth)
        }
        other => block(other, depth),
    }
}

/// Wrap an indentation block in a fence.
///
/// Markdown throws leading whitespace away, so the two spaces before `second:`
/// would render as nothing at all and the shape would be lost. The fence is
/// what makes the structure survive being read as Markdown.
fn fenced(body: &str) -> String {
    format!("```\n{}\n```", body.trim_end())
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
            // `- key: value` is one element, so the marker sits on the first
            // key rather than on a line of its own. A fence cannot be used
            // here: it would end the list.
            Value::Dict(dict) if !dict.is_empty() => {
                lines.push(marked(&pad, &indented(dict, indent + 1)));
            }
            Value::Anchor(anchor) if !anchor.props.is_empty() => {
                lines.push(marked(&pad, &indented(&anchor.props, indent + 1)));
            }
            other => {
                let text = block(other, MAX_HEADER + 1);
                lines.push(format!("{pad}- {}", text.replace('\n', &format!("\n{pad}  "))));
            }
        }
    }
    lines.join("\n")
}

/// Put a `- ` marker over the indentation of an already-indented block's first
/// line, so the block reads as one list element.
fn marked(pad: &str, body: &str) -> String {
    // `indented` opened the block one level in, which is exactly the width of
    // the `- ` the marker needs.
    match body.strip_prefix(&format!("{pad}  ")) {
        Some(rest) => format!("{pad}- {rest}"),
        None => format!("{pad}-\n{body}"),
    }
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

/// True when a dictionary holds another dictionary, so its shape is the point.
fn nests_dictionaries(dict: &Dict) -> bool {
    dict.values().any(|value| match value {
        Value::Dict(nested) => !nested.is_empty(),
        Value::Anchor(anchor) => !anchor.props.is_empty(),
        _ => false,
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
    fn a_pure_dictionary_indents_inside_a_fence() {
        // A pure dictionary "will compile down into text that follows that
        // exact shape inside a code block". Without the fence Markdown eats
        // the indentation and the shape is gone.
        let inner = dict(vec![("third", Value::string("Hello, World"))]);
        let middle = dict(vec![("second", Value::Dict(inner))]);
        let outer = dict(vec![("first", Value::Dict(middle))]);
        assert_eq!(
            block(&Value::Dict(outer), 1),
            "```\nfirst:\n  second:\n    third: Hello, World\n```"
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

    fn anchor(entries: Vec<(&str, Value)>) -> Value {
        Value::Anchor(piton_core::value::Anchor {
            id: piton_core::value::AnchorId(0),
            name: "ThemeEngine".to_string(),
            props: std::sync::Arc::new(dict(entries)),
        })
    }

    #[test]
    fn anchor_properties_are_headers_even_when_every_value_is_scalar() {
        // Spec 25.5: anchor properties serialize as headers whose level matches
        // depth — with no exception for an anchor that happens to look flat.
        // This used to collapse to `pitch: A single source ...`.
        let value = anchor(vec![("pitch", Value::string("A single source for UI styling rules."))]);
        assert_eq!(block(&value, 2), "## Pitch\n\nA single source for UI styling rules.");
    }

    #[test]
    fn a_pure_dictionary_inside_an_anchor_still_indents() {
        // Spec 25.4 still applies to a dictionary *nested* in an anchor.
        let spacing = dict(vec![("step", Value::number(4.0)), ("gutter", Value::number(12.0))]);
        let value = anchor(vec![
            ("spacing", Value::Dict(spacing)),
            ("units", Value::string("points")),
        ]);
        assert_eq!(
            block(&value, 2),
            "## Spacing\n\n```\nstep: 4\ngutter: 12\n```\n\n## Units\n\npoints"
        );
    }

    fn implicit(items: Vec<Value>) -> Value {
        Value::List(piton_core::value::List::implicit(items))
    }

    #[test]
    fn a_dictionary_written_among_prose_becomes_a_section() {
        // The specification's `MyAnchor` example: `third:` sits beside prose
        // inside `second:`, and comes out as a heading with its text beneath,
        // not as a `third: Third Text` line.
        let third = Value::Dict(dict(vec![("third", Value::string("Third Text"))]));
        let second =
            Value::Dict(dict(vec![("second", implicit(vec![Value::string("Second Text"), third]))]));
        let first = implicit(vec![Value::string("First Text"), second]);
        let value = anchor(vec![("first", first)]);
        assert_eq!(
            block(&value, 2),
            "## First\n\nFirst Text\n\n### Second\n\nSecond Text\n\n#### Third\n\nThird Text"
        );
    }

    #[test]
    fn a_nested_dictionary_among_prose_stays_a_shape() {
        // The specification's kitchen-sink example: `with:` nests further, so
        // the nesting is the information and it is shown as a fenced shape.
        let nested = dict(vec![("nested", Value::string("dictionary"))]);
        let a = dict(vec![("a", Value::Dict(nested))]);
        let with = Value::Dict(dict(vec![("with", Value::Dict(a))]));
        let value = anchor(vec![("third", implicit(vec![Value::string("This is the third"), with]))]);
        assert_eq!(
            block(&value, 2),
            "## Third\n\nThis is the third\n\n```\nwith:\n  a:\n    nested: dictionary\n```"
        );
    }

    #[test]
    fn a_dictionary_list_element_carries_the_marker() {
        let entry = Value::Dict(dict(vec![
            ("name", Value::string("Save")),
            ("key", Value::string("Ctrl+S")),
        ]));
        assert_eq!(
            block(&Value::list(vec![entry, Value::string("plain")]), 1),
            "- name: Save\n  key: Ctrl+S\n- plain"
        );
    }

    #[test]
    fn headers_stop_at_six_levels() {
        assert_eq!(heading("a", 1), "# A");
        assert_eq!(heading("a", 6), "###### A");
        assert_eq!(heading("a", 7), "**A**");
    }
}
