//! Markdown serialization.
//!
//! This implements Belay's common content rules: anchor names and prose-bearing
//! property names become word-separated titles, heading levels represent the
//! hierarchy, and structured data falls back to an indentation-based form.
//!
//! There are two rendering modes and the choice between them is what makes the
//! output readable. At *document* level a property becomes a heading and its
//! value becomes the section body. Inside a list item or a fenced block there
//! are no headings available, so the same value renders as indented
//! `key: value` lines.

use std::path::Path;

use piton_core::{
    format_number, title_case, AnchorId, AnchorView, Mixed, MixedItem, Properties, Ref, Text, Value,
};

/// Markdown allows six heading levels; past that Belay uses a bold label.
const MAX_HEADING_LEVEL: usize = 6;

/// Resolves a reference to a link target relative to the file being written.
pub trait LinkResolver {
    /// The link target for a reference, or `None` when the target has no
    /// compiled representation in this output.
    fn link(&self, target: &Ref) -> Option<String>;
}

/// A resolver that produces no links; references fall back to their name.
pub struct NoLinks;

impl LinkResolver for NoLinks {
    fn link(&self, _: &Ref) -> Option<String> {
        None
    }
}

/// Links to wherever the anchor was rendered when each source file compiles to
/// a Markdown file next to it, which is what `piton compile` does: a reference
/// to Button from a file next to it links to `./Button.md#button`, and one to a
/// property links to that property's heading.
pub struct MirroredLinks<'a> {
    /// Directory of the file being written.
    pub from_directory: &'a Path,
    /// Root the source tree is measured from.
    pub source_root: &'a Path,
    pub anchors: &'a dyn AnchorView,
}

impl LinkResolver for MirroredLinks<'_> {
    fn link(&self, target: &Ref) -> Option<String> {
        let file = crate::reference_file(self.anchors, target, "md");
        Some(format!("{file}#{}", crate::reference_fragment(self.anchors, target)))
    }
}

/// Everything the renderer needs beyond the value itself.
pub struct Context<'a> {
    pub anchors: &'a dyn AnchorView,
    pub links: &'a dyn LinkResolver,
}

impl Context<'_> {
    fn render_text(&self, text: &Text) -> String {
        text.render_with(|target| self.reference_markup(target))
    }

    /// A reference renders as a Markdown link to the referenced anchor's
    /// compiled representation, with the anchor's name as the link text, or
    /// `Anchor.property` for a property. Access stays lazy: the link is not an
    /// include.
    fn reference_markup(&self, target: &Ref) -> String {
        let name = target.display(self.anchors);
        match self.links.link(target) {
            Some(link) => format!("[{name}]({link})"),
            None => name,
        }
    }
}

/// Renders a whole anchor as a document, starting at heading level 1.
pub fn document(anchor: AnchorId, context: &Context<'_>) -> String {
    let mut out = String::new();
    out.push_str(&heading(1, &title_case(context.anchors.name(anchor))));
    out.push('\n');
    render_properties(context.anchors.properties(anchor), 2, context, &mut out);
    finish(out)
}

/// Renders a document with the anchor's title and the given properties, for a
/// caller that leaves some of them out.
pub fn document_with(anchor: AnchorId, properties: &Properties, context: &Context<'_>) -> String {
    let mut out = String::new();
    out.push_str(&heading(1, &title_case(context.anchors.name(anchor))));
    out.push('\n');
    render_properties(properties, 2, context, &mut out);
    finish(out)
}

/// Renders a set of properties as sections, without a document title.
pub fn sections(properties: &Properties, level: usize, context: &Context<'_>) -> String {
    let mut out = String::new();
    render_properties(properties, level, context, &mut out);
    finish(out)
}

/// Renders interpolated text, turning references into links relative to the
/// file being written.
pub fn inline_text(text: &Text, context: &Context<'_>) -> String {
    context.render_text(text)
}

/// Renders a value as a standalone body with no heading of its own.
pub fn body(value: &Value, level: usize, context: &Context<'_>) -> String {
    let mut out = String::new();
    render_value(value, level, context, &mut out);
    finish(out)
}

fn finish(mut out: String) -> String {
    while out.ends_with('\n') {
        out.pop();
    }
    out.push('\n');
    out
}

fn heading(level: usize, title: &str) -> String {
    if level <= MAX_HEADING_LEVEL {
        format!("{} {title}\n", "#".repeat(level))
    } else {
        // Past six levels Markdown has no heading left, so the hierarchy
        // continues with a bold label.
        format!("**{title}**\n")
    }
}

fn render_properties(
    properties: &Properties,
    level: usize,
    context: &Context<'_>,
    out: &mut String,
) {
    for (name, value) in properties {
        out.push_str(&heading(level, &title_case(name)));
        out.push('\n');
        render_value(value, level, context, out);
    }
}

/// Renders a value at document level, where headings are available.
fn render_value(value: &Value, level: usize, context: &Context<'_>, out: &mut String) {
    match value {
        Value::Str(text) => paragraph(&context.render_text(text), out),
        Value::Number(n) => paragraph(&format_number(*n), out),
        Value::Bool(b) => paragraph(if *b { "true" } else { "false" }, out),
        Value::Null => paragraph("null", out),
        Value::Reference(target) => paragraph(&context.reference_markup(target), out),
        Value::List(items) => {
            if items.is_empty() {
                return;
            }
            paragraph(&render_list(items, 0, context), out);
        }
        // A pure dictionary is data rather than document structure, so it reads
        // better fenced than as a run of near-empty headings.
        Value::Dict(map) if value.is_pure_dictionary() => {
            if map.is_empty() {
                return;
            }
            paragraph(&format!("```\n{}\n```", render_map(map, 0, context)), out);
        }
        Value::Dict(map) => render_properties(map, level + 1, context, out),
        Value::Anchor(id) => {
            render_properties(context.anchors.properties(*id), level + 1, context, out)
        }
        Value::Mixed(mixed) => render_mixed(mixed, level, context, out),
    }
}

/// An implicit mixed block keeps the order it was written in: prose stays prose,
/// nested lists stay lists, and dictionary keys become the next heading level.
fn render_mixed(mixed: &Mixed, level: usize, context: &Context<'_>, out: &mut String) {
    // A pure dictionary embedded in mixed content has no heading to carry its
    // key, so the key goes inside a fenced structure with it. Consecutive ones
    // share a single fence.
    let mut fenced: Properties = Properties::new();

    macro_rules! flush_fenced {
        () => {
            if !fenced.is_empty() {
                let map = std::mem::take(&mut fenced);
                paragraph(&format!("```\n{}\n```", render_map(&map, 0, context)), out);
            }
        };
    }

    for item in &mixed.items {
        match item {
            MixedItem::Text(text) => {
                flush_fenced!();
                paragraph(&context.render_text(text), out);
            }
            MixedItem::List(items) => {
                flush_fenced!();
                if !items.is_empty() {
                    paragraph(&render_list(items, 0, context), out);
                }
            }
            MixedItem::Entry(name, value) if value.is_pure_dictionary() => {
                fenced.insert(name.clone(), value.clone());
            }
            MixedItem::Entry(name, value) => {
                flush_fenced!();
                out.push_str(&heading(level + 1, &title_case(name)));
                out.push('\n');
                render_value(value, level + 1, context, out);
            }
            // A value dropped into the text, such as the copy of an anchor,
            // renders in place.
            MixedItem::Value(value) => {
                flush_fenced!();
                render_value(value, level, context, out);
            }
        }
    }
    flush_fenced!();
}

fn paragraph(text: &str, out: &mut String) {
    if text.is_empty() {
        return;
    }
    out.push_str(text);
    out.push_str("\n\n");
}

/// Renders a list as Markdown, with nested structure indented beneath it.
pub fn render_list(items: &[Value], indent: usize, context: &Context<'_>) -> String {
    let pad = " ".repeat(indent);
    let mut lines: Vec<String> = Vec::new();
    for item in items {
        match item {
            Value::Dict(map) if !map.is_empty() => {
                let rendered = render_map(map, indent + 2, context);
                // The first key shares the bullet's line.
                lines.push(format!("{pad}- {}", rendered.trim_start()));
            }
            Value::Anchor(id) => {
                let properties = context.anchors.properties(*id);
                if properties.is_empty() {
                    lines.push(format!("{pad}- {}", context.anchors.name(*id)));
                } else {
                    let rendered = render_map(properties, indent + 2, context);
                    lines.push(format!("{pad}- {}", rendered.trim_start()));
                }
            }
            Value::List(nested) if !nested.is_empty() => {
                lines.push(render_list(nested, indent + 2, context));
            }
            Value::Mixed(_) => {
                let flattened = item.as_list_items();
                lines.push(render_list(&flattened, indent + 2, context));
            }
            other => {
                let text = scalar(other, context);
                lines.push(format!("{pad}- {}", indent_continuations(&text, indent + 2)));
            }
        }
    }
    lines.join("\n")
}

/// Renders a dictionary as indentation-based `key: value` lines.
pub fn render_map(map: &Properties, indent: usize, context: &Context<'_>) -> String {
    let pad = " ".repeat(indent);
    let mut lines: Vec<String> = Vec::new();
    for (name, value) in map {
        match value {
            Value::Dict(inner) if !inner.is_empty() => {
                lines.push(format!("{pad}{name}:"));
                lines.push(render_map(inner, indent + 2, context));
            }
            Value::Anchor(id) => {
                let properties = context.anchors.properties(*id);
                if properties.is_empty() {
                    lines.push(format!("{pad}{name}: {}", context.anchors.name(*id)));
                } else {
                    lines.push(format!("{pad}{name}:"));
                    lines.push(render_map(properties, indent + 2, context));
                }
            }
            Value::List(items) => {
                lines.push(format!("{pad}{name}:"));
                if !items.is_empty() {
                    lines.push(render_list(items, indent + 2, context));
                }
            }
            Value::Mixed(_) => {
                lines.push(format!("{pad}{name}:"));
                let items = value.as_list_items();
                if !items.is_empty() {
                    lines.push(render_list(&items, indent + 2, context));
                }
            }
            other => {
                let text = scalar(other, context);
                lines.push(format!(
                    "{pad}{name}: {}",
                    indent_continuations(&text, indent + 2)
                ));
            }
        }
    }
    lines.join("\n")
}

fn scalar(value: &Value, context: &Context<'_>) -> String {
    match value {
        Value::Str(text) => context.render_text(text),
        Value::Number(n) => format_number(*n),
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        Value::Null => "null".to_string(),
        Value::Reference(target) => context.reference_markup(target),
        Value::Dict(_) | Value::Anchor(_) | Value::List(_) | Value::Mixed(_) => String::new(),
    }
}

/// Indents every line after the first so a multi-line value stays inside its
/// list item or map entry.
fn indent_continuations(text: &str, indent: usize) -> String {
    let pad = " ".repeat(indent);
    let mut lines = text.lines();
    let mut out = String::new();
    if let Some(first) = lines.next() {
        out.push_str(first);
    }
    for line in lines {
        out.push('\n');
        out.push_str(&pad);
        out.push_str(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use piton_core::EmptyAnchors;

    fn context() -> Context<'static> {
        Context {
            anchors: &EmptyAnchors,
            links: &NoLinks,
        }
    }

    fn props(pairs: Vec<(&str, Value)>) -> Properties {
        let mut map = Properties::new();
        for (name, value) in pairs {
            map.insert(name.to_string(), value);
        }
        map
    }

    #[test]
    fn property_names_become_word_separated_titles() {
        let rendered = sections(
            &props(vec![("orderOfPrecedence", Value::string("text"))]),
            2,
            &context(),
        );
        assert_eq!(rendered, "## Order Of Precedence\n\ntext\n");
    }

    #[test]
    fn pure_dictionaries_are_fenced() {
        let inner = props(vec![
            ("sourceName", Value::string("myProperty")),
            ("renderedTitle", Value::string("My Property")),
        ]);
        let rendered = sections(
            &props(vec![("examples", Value::Dict(inner))]),
            2,
            &context(),
        );
        assert_eq!(
            rendered,
            "## Examples\n\n```\nsourceName: myProperty\nrenderedTitle: My Property\n```\n"
        );
    }

    #[test]
    fn dictionaries_holding_lists_become_headings() {
        let inner = props(vec![
            ("description", Value::string("what it does")),
            ("requirements", Value::List(vec![Value::string("one")])),
        ]);
        let rendered = sections(&props(vec![("output", Value::Dict(inner))]), 2, &context());
        assert_eq!(
            rendered,
            "## Output\n\n### Description\n\nwhat it does\n\n### Requirements\n\n- one\n"
        );
    }

    #[test]
    fn lists_of_dictionaries_indent_their_keys() {
        let entry = props(vec![
            ("description", Value::string("Adds two numbers together")),
            ("symbol", Value::string("+")),
        ]);
        let rendered = sections(
            &props(vec![("operators", Value::List(vec![Value::Dict(entry)]))]),
            2,
            &context(),
        );
        assert_eq!(
            rendered,
            "## Operators\n\n- description: Adds two numbers together\n  symbol: +\n"
        );
    }

    #[test]
    fn nested_maps_indent_by_two() {
        let deep = props(vec![("source", Value::string("a/b.pi"))]);
        let middle = props(vec![("matchingDirectory", Value::Dict(deep))]);
        let entry = props(vec![("examples", Value::Dict(middle))]);
        let rendered = render_map(&entry, 2, &context());
        assert_eq!(
            rendered,
            "  examples:\n    matchingDirectory:\n      source: a/b.pi"
        );
    }

    #[test]
    fn beyond_six_levels_titles_become_bold_labels() {
        assert_eq!(heading(7, "Deep"), "**Deep**\n");
        assert_eq!(heading(6, "Deep"), "###### Deep\n");
    }

    #[test]
    fn multiline_values_stay_inside_their_entry() {
        let entry = props(vec![("note", Value::string("first\nsecond"))]);
        assert_eq!(render_map(&entry, 0, &context()), "note: first\n  second");
    }

    #[test]
    fn primitives_render_as_text() {
        let map = props(vec![
            ("booleanValue", Value::Bool(false)),
            ("numericValue", Value::Number(42.0)),
            ("missing", Value::Null),
        ]);
        assert_eq!(
            render_map(&map, 0, &context()),
            "booleanValue: false\nnumericValue: 42\nmissing: null"
        );
    }
}
