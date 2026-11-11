//! Edits the `dependencies` list of `piton.config.pi` in place.
//!
//! `piton tether <source>` records the source that it installed. `piton remove`
//! removes the source again. Thus the configuration and `tethers/` agree about
//! the dependencies of the project. The file is the source of a person. Thus
//! the edit keeps comments, blank lines, order and all other properties as
//! they were. The parser finds the `piton-config` anchor and its `dependencies`
//! property. The edit replaces whole lines at those spans. It never makes the
//! configuration again.

use std::path::Path;

use piton_compile::config::CONFIG_KEYWORD;
use piton_syntax::ast::{AnchorDecl, Item, Property};
use piton_syntax::parser;

/// Adds `- <source>` to the `dependencies` of the configuration. When there is
/// no list, it makes the list. Returns the new text. Returns `None` when the
/// list contains the source.
pub fn add_dependency(text: &str, path: &Path, source: &str) -> Result<Option<String>, String> {
    let parse = parser::parse(text, path);
    let anchor = config_anchor(&parse.file.items)?;
    let lines = Lines::new(text);

    if let Some(property) = dependencies(anchor) {
        if property.value.inline_list.is_some() {
            return Err(
                "`dependencies` is written as an inline list; add the repository by hand".into(),
            );
        }
        let start = lines.index_of(property.span.start);
        let end = lines.extent(start);
        if listed(&lines, start, end).iter().any(|(_, _, url)| url == source) {
            return Ok(None);
        }
        // Below the last line of the list, at the same indentation as the
        // current items.
        let item_indent = listed(&lines, start, end)
            .first()
            .map(|(line, _, _)| lines.indent(*line))
            .unwrap_or_else(|| lines.indent(start) + 4);
        let entry = format!("{}- {source}\n", " ".repeat(item_indent));
        let last = last_content_line(&lines, start, end);
        return Ok(Some(insert_after_line(text, &lines, last, &entry)));
    }

    // There is no list. Thus the edit adds a list at the end of the anchor
    // body. The list has the same indentation as the current properties.
    let head = lines.index_of(anchor.span.start);
    let body_indent = anchor
        .body
        .properties()
        .next()
        .map(|property| lines.indent(lines.index_of(property.span.start)))
        .unwrap_or(4);
    let end = lines.extent(head);
    let last = last_content_line(&lines, head, end);
    let entry = format!(
        "\n{indent}dependencies:\n{indent}    - {source}\n",
        indent = " ".repeat(body_indent)
    );
    Ok(Some(insert_after_line(text, &lines, last, &entry)))
}

/// Removes `source`, and the pin below it, from `dependencies`. When the list
/// becomes empty, it removes the full property. Returns the new text. Returns
/// `None` when the list does not contain the source.
pub fn remove_dependency(text: &str, path: &Path, source: &str) -> Result<Option<String>, String> {
    let parse = parser::parse(text, path);
    let anchor = config_anchor(&parse.file.items)?;
    let lines = Lines::new(text);
    let Some(property) = dependencies(anchor) else {
        return Ok(None);
    };
    let start = lines.index_of(property.span.start);
    let end = lines.extent(start);
    let items = listed(&lines, start, end);
    let Some(position) = items.iter().position(|(_, _, url)| url == source) else {
        return Ok(None);
    };

    let (from, to) = if items.len() == 1 {
        // The list will be empty. Thus the edit removes the property. It also
        // removes the blank line between the property and the text before it.
        let mut from = start;
        if from > 0 && lines.is_blank(from - 1) {
            from -= 1;
        }
        (from, last_content_line(&lines, start, end) + 1)
    } else {
        let (line, item_end, _) = items[position];
        (line, item_end)
    };
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..lines.start(from)]);
    out.push_str(&text[lines.start(to)..]);
    Ok(Some(out))
}

/// The configuration anchor. It is the first anchor with `piton-config`.
fn config_anchor(items: &[Item]) -> Result<&AnchorDecl, String> {
    items
        .iter()
        .find_map(|item| match item {
            Item::Anchor(anchor) if anchor.keyword == CONFIG_KEYWORD => Some(anchor),
            _ => None,
        })
        .ok_or_else(|| "piton.config.pi declares no `piton-config` anchor".to_string())
}

fn dependencies(anchor: &AnchorDecl) -> Option<&Property> {
    anchor
        .body
        .properties()
        .find(|property| property.name == "dependencies")
}

/// The list items of a property on lines `start..end`. For each item, it gives
/// the first line, the line after the last line, and the URL of the item. A pin
/// with more indentation below an item is part of that item.
fn listed(lines: &Lines<'_>, start: usize, end: usize) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let mut line = start + 1;
    while line < end {
        let body = lines.body(line);
        if let Some(rest) = body.strip_prefix('-') {
            let url = strip_comment(rest).trim().to_string();
            let item_end = lines.extent(line).min(end);
            out.push((line, item_end, url));
            line = item_end;
        } else {
            line += 1;
        }
    }
    out
}

fn strip_comment(text: &str) -> &str {
    // A URL has `//` after its scheme. Thus a comment marker starts a comment
    // only when whitespace comes before it.
    let bytes = text.as_bytes();
    for index in 0..bytes.len().saturating_sub(1) {
        if bytes[index] == b'/'
            && bytes[index + 1] == b'/'
            && (index == 0 || bytes[index - 1] == b' ' || bytes[index - 1] == b'\t')
        {
            return &text[..index];
        }
    }
    text
}

/// The last non-blank line in `start..end`.
fn last_content_line(lines: &Lines<'_>, start: usize, end: usize) -> usize {
    (start..end)
        .rev()
        .find(|line| !lines.is_blank(*line))
        .unwrap_or(start)
}

fn insert_after_line(text: &str, lines: &Lines<'_>, line: usize, entry: &str) -> String {
    let at = lines.start(line + 1);
    let mut out = String::with_capacity(text.len() + entry.len() + 1);
    out.push_str(&text[..at]);
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(entry);
    out.push_str(&text[at..]);
    out
}

/// A text split into lines, with byte offsets.
struct Lines<'a> {
    text: &'a str,
    /// The start byte offset of each line. One more entry marks the end.
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    fn new(text: &'a str) -> Lines<'a> {
        let mut starts = vec![0];
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' && index + 1 < text.len() {
                starts.push(index + 1);
            }
        }
        starts.push(text.len());
        Lines { text, starts }
    }

    fn count(&self) -> usize {
        self.starts.len() - 1
    }

    fn start(&self, line: usize) -> usize {
        self.starts[line.min(self.count())]
    }

    fn raw(&self, line: usize) -> &'a str {
        let end = self.start(line + 1);
        self.text[self.start(line)..end].trim_end_matches(['\n', '\r'])
    }

    fn body(&self, line: usize) -> &'a str {
        self.raw(line).trim()
    }

    fn is_blank(&self, line: usize) -> bool {
        self.body(line).is_empty()
    }

    fn indent(&self, line: usize) -> usize {
        let raw = self.raw(line);
        raw.len() - raw.trim_start_matches([' ', '\t']).len()
    }

    /// The line that contains a byte offset.
    fn index_of(&self, offset: usize) -> usize {
        match self.starts[..self.count()].binary_search(&offset) {
            Ok(line) => line,
            Err(next) => next.saturating_sub(1),
        }
    }

    /// The line after the block that starts at `line`. All subsequent lines
    /// with more indentation are part of the block. Blank lines between them
    /// are also part of the block.
    fn extent(&self, line: usize) -> usize {
        let indent = self.indent(line);
        let mut end = line + 1;
        let mut probe = line + 1;
        while probe < self.count() {
            if self.is_blank(probe) {
                probe += 1;
                continue;
            }
            if self.indent(probe) <= indent {
                break;
            }
            probe += 1;
            end = probe;
        }
        end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = "use @piton/config

export piton-config App:
    root: ./spec

    dependencies:
        - https://example.test/a
            tag: 1.0
        - https://example.test/b // the other one

piton-package Pkg:
    root: ./spec
";

    fn add(text: &str, source: &str) -> Option<String> {
        add_dependency(text, Path::new("piton.config.pi"), source).expect("add")
    }

    fn remove(text: &str, source: &str) -> Option<String> {
        remove_dependency(text, Path::new("piton.config.pi"), source).expect("remove")
    }

    #[test]
    fn a_new_dependency_goes_at_the_end_of_the_list() {
        let added = add(CONFIG, "https://example.test/c").expect("changed");
        assert_eq!(
            added,
            CONFIG.replace(
                "        - https://example.test/b // the other one\n",
                "        - https://example.test/b // the other one\n        - https://example.test/c\n"
            )
        );
    }

    #[test]
    fn a_listed_dependency_is_not_added_twice() {
        assert!(add(CONFIG, "https://example.test/a").is_none());
        assert!(add(CONFIG, "https://example.test/b").is_none());
    }

    #[test]
    fn a_missing_list_is_created() {
        let text = "use @piton/config\n\nexport piton-config App:\n    root: ./spec\n    entry: ./spec/index.pi\n\nanchor Other:\n    a: 1\n";
        let added = add(text, "https://example.test/a").expect("changed");
        assert_eq!(
            added,
            "use @piton/config\n\nexport piton-config App:\n    root: ./spec\n    entry: ./spec/index.pi\n\n    dependencies:\n        - https://example.test/a\n\nanchor Other:\n    a: 1\n"
        );
    }

    #[test]
    fn a_missing_list_is_created_at_the_end_of_the_file() {
        let text = "export piton-config App:\n    root: ./spec";
        let added = add(text, "https://example.test/a").expect("changed");
        assert_eq!(
            added,
            "export piton-config App:\n    root: ./spec\n\n    dependencies:\n        - https://example.test/a\n"
        );
    }

    #[test]
    fn removing_takes_the_pin_with_it() {
        let removed = remove(CONFIG, "https://example.test/a").expect("changed");
        assert_eq!(
            removed,
            CONFIG.replace("        - https://example.test/a\n            tag: 1.0\n", "")
        );
    }

    #[test]
    fn removing_the_last_dependency_removes_the_list() {
        let once = remove(CONFIG, "https://example.test/a").expect("changed");
        let twice = remove(&once, "https://example.test/b").expect("changed");
        assert_eq!(
            twice,
            "use @piton/config\n\nexport piton-config App:\n    root: ./spec\n\npiton-package Pkg:\n    root: ./spec\n"
        );
        assert!(remove(&twice, "https://example.test/b").is_none());
    }
}
