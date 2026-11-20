//! The prose reflow of `piton format`.
//!
//! Piton joins the lines of a paragraph with one space. Thus the line breaks in
//! a paragraph are not part of the value. The formatter can move them. This
//! module joins each paragraph and wraps it again at [`WRAP_COLUMN`].
//!
//! A new line must stay prose. A line that starts with `- `, `+ `, `word:` or
//! `//` changes the structure. Thus the wrapper does not start a line at such
//! a word. If the wrapper cannot make all lines prose, it does not change the
//! paragraph.
//!
//! A long `key: value` line moves its value to an indented block below the key.
//! The value does not change, because the text on the key line and the prose
//! directly below it are one paragraph.
//!
//! A list item keeps its first words on the `- ` line. The other lines go
//! below it, one indent level deeper than the dash. Prose under an item is
//! more of that item, as prose under a key is more of its value.

use std::path::Path;

use crate::ast::{Block, BlockItem, Item, Paragraph, ProseLine, ValueNode};
use crate::format::{INDENT, WRAP_COLUMN};
use crate::parser;

/// The smallest text width of a wrapped line. A deep indent does not make
/// lines shorter than this width.
const MIN_MEASURE: usize = 40;

struct Edit {
    start: usize,
    end: usize,
    text: String,
}

/// Reflows each paragraph of a source in the canonical indentation.
///
/// The function does not change a source with parse errors.
pub(crate) fn reflow(source: &str, path: &Path) -> String {
    let parse = parser::parse(source, path);
    if parse.has_errors() {
        return source.to_string();
    }
    let mut edits = Vec::new();
    for item in &parse.file.items {
        match item {
            Item::Anchor(anchor) => visit_block(source, &anchor.body, &mut edits),
            Item::Variable(variable) => visit_value(source, &variable.value, Owner::Key, &mut edits),
            _ => {}
        }
    }
    if edits.is_empty() {
        return source.to_string();
    }
    edits.sort_by_key(|edit| edit.start);
    let mut out = String::with_capacity(source.len());
    let mut cursor = 0;
    for edit in edits {
        if edit.start < cursor {
            // Two edits overlap. The first edit stays. This must not occur.
            continue;
        }
        out.push_str(&source[cursor..edit.start]);
        out.push_str(&edit.text);
        cursor = edit.end;
    }
    out.push_str(&source[cursor..]);
    out
}

fn visit_block(source: &str, block: &Block, edits: &mut Vec<Edit>) {
    for item in &block.items {
        match item {
            BlockItem::Property(property) => visit_value(source, &property.value, Owner::Key, edits),
            BlockItem::ListItem(item) => visit_value(source, &item.value, Owner::Item, edits),
            BlockItem::Prose(paragraph) => {
                if let Some(edit) = paragraph_edit(source, paragraph) {
                    edits.push(edit);
                }
            }
            BlockItem::Merge(_) | BlockItem::Escape(_) | BlockItem::Pass(_) => {}
        }
    }
}

/// The line that holds the start of a value.
#[derive(Clone, Copy)]
enum Owner {
    /// A `key: value` line of a property or a variable. Long text moves to
    /// a block below the key.
    Key,
    /// A `- value` line. The first words stay on the line of the dash.
    Item,
}

/// Visits a value and the blocks in it.
fn visit_value(source: &str, value: &ValueNode, owner: Owner, edits: &mut Vec<Edit>) {
    let edit = |paragraph| match owner {
        Owner::Key => inline_edit(source, value, paragraph),
        Owner::Item => item_edit(source, value, paragraph),
    };
    let Some(block) = &value.block else {
        if let Some(edit) = edit(None) {
            edits.push(edit);
        }
        return;
    };

    // A merge block calculates its value from its lines. Do not move text into it.
    let merges = block.items.iter().any(|item| matches!(item, BlockItem::Merge(_)));
    let adjacent = match block.items.first() {
        Some(BlockItem::Prose(paragraph)) => {
            let inline_line = value.inline.as_ref().map(|inline| line_start(source, inline.span.start));
            let paragraph_line = line_start(source, paragraph.span.start);
            match inline_line {
                Some(start) if next_line_start(source, start) == paragraph_line => Some(paragraph),
                // The evaluator joins the key line to the first paragraph,
                // also after a blank line. A move adds a paragraph break.
                Some(_) => {
                    visit_block(source, block, edits);
                    return;
                }
                None => None,
            }
        }
        _ => None,
    };

    if !merges && value.inline.is_some() {
        if let Some(edit) = edit(adjacent) {
            edits.push(edit);
            // The edit replaces the first paragraph. The other items stay.
            for item in block.items.iter().skip(usize::from(adjacent.is_some())) {
                visit_item(source, item, edits);
            }
            return;
        }
    }
    visit_block(source, block, edits);
}

fn visit_item(source: &str, item: &BlockItem, edits: &mut Vec<Edit>) {
    let block = Block {
        span: Default::default(),
        items: vec![item.clone()],
    };
    visit_block(source, &block, edits);
}

/// Wraps one paragraph of a block in place.
fn paragraph_edit(source: &str, paragraph: &Paragraph) -> Option<Edit> {
    let first = paragraph.lines.first()?;
    let last = paragraph.lines.last()?;
    let start = line_start(source, first.span.start);
    let indent = first.span.start - start;
    let end = line_end(source, last.span.end);
    let words = words(source, &paragraph.lines)?;
    let text = render(&words, &" ".repeat(indent), indent, false)?;
    if source[start..end] == text {
        return None;
    }
    Some(Edit { start, end, text })
}

/// Moves the text after `key:` into a block below the key. `paragraph` is the
/// prose directly below the key line. It joins the moved text.
///
/// The function moves the text only when the key line is too long, or when
/// prose continues below it.
fn inline_edit(source: &str, value: &ValueNode, paragraph: Option<&Paragraph>) -> Option<Edit> {
    let inline = value.inline.as_ref()?;
    if inline.is_blank() || value.inline_list.is_some() {
        return None;
    }
    let start = line_start(source, inline.span.start);
    let key_end = line_end(source, inline.span.end);
    let key_line = &source[start..key_end];
    let width = key_line.trim_end_matches('\n').chars().count();
    if paragraph.is_none() && width <= WRAP_COLUMN {
        return None;
    }

    let mut lines = vec![inline.clone()];
    let (indent, end) = match paragraph {
        Some(paragraph) => {
            let first = paragraph.lines.first()?;
            lines.extend(paragraph.lines.iter().cloned());
            let indent = first.span.start - line_start(source, first.span.start);
            (indent, line_end(source, paragraph.lines.last()?.span.end))
        }
        None => {
            let key_indent = key_line.len() - key_line.trim_start().len();
            (key_indent + INDENT, key_end)
        }
    };

    let words = words(source, &lines)?;
    // A text with no break point stays on the key line.
    if paragraph.is_none() && words.len() < 2 {
        return None;
    }
    let body = render(&words, &" ".repeat(indent), indent, false)?;
    let header = source[start..inline.span.start].trim_end();
    let text = format!("{header}\n{body}");
    Some(Edit { start, end, text })
}

/// Wraps the text of a list item. The first words stay after the dash, and
/// the other lines go one indent level deeper than the dash. `paragraph` is
/// the prose directly below the item line. It joins the text of the item.
fn item_edit(source: &str, value: &ValueNode, paragraph: Option<&Paragraph>) -> Option<Edit> {
    let inline = value.inline.as_ref()?;
    if inline.is_blank() || value.inline_list.is_some() {
        return None;
    }
    let start = line_start(source, inline.span.start);
    let item_end = line_end(source, inline.span.end);
    let width = source[start..item_end].trim_end_matches('\n').chars().count();
    if paragraph.is_none() && width <= WRAP_COLUMN {
        return None;
    }
    let lead = &source[start..inline.span.start];
    let dash_indent = lead.len() - lead.trim_start().len();
    let prefix = format!("{}- ", " ".repeat(dash_indent));

    let mut lines = vec![inline.clone()];
    let end = match paragraph {
        Some(paragraph) => {
            lines.extend(paragraph.lines.iter().cloned());
            line_end(source, paragraph.lines.last()?.span.end)
        }
        None => item_end,
    };
    let words = words(source, &lines)?;
    let text = render(&words, &prefix, dash_indent + INDENT, true)?;
    if source[start..end] == text {
        return None;
    }
    Some(Edit { start, end, text })
}

/// Splits the lines of a paragraph into words. A word ends at a single space.
/// A space in an interpolation, in an inline escape, or in a run of spaces is
/// part of its word. Thus the joined words give the same value.
///
/// It returns `None` when a line ends with `\`. A space after that backslash
/// opens an escape, and that changes the value.
fn words(source: &str, lines: &[ProseLine]) -> Option<Vec<String>> {
    let mut words = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let text = &source[line.span.start..line.span.end];
        if index + 1 < lines.len() && text.ends_with('\\') {
            return None;
        }
        if line.escapes.len() % 2 != 0 {
            return None;
        }
        let mut protected: Vec<(usize, usize)> = line
            .escapes
            .chunks(2)
            .map(|pair| (pair[0].start - line.span.start, pair[1].end - line.span.start))
            .collect();
        protected.extend(brace_regions(text)?);

        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let mut word_start = 0;
        for (index, (offset, ch)) in chars.iter().copied().enumerate() {
            if ch != ' ' {
                continue;
            }
            let before = index > 0 && chars[index - 1].1 != ' ';
            let after = chars.get(index + 1).is_some_and(|(_, c)| *c != ' ');
            let free = !protected.iter().any(|(from, to)| offset > *from && offset < *to);
            if before && after && free {
                words.push(text[word_start..offset].to_string());
                word_start = offset + 1;
            }
        }
        words.push(text[word_start..].to_string());
    }
    words.retain(|word| !word.is_empty());
    Some(words)
}

/// The regions between balanced braces. It returns `None` when a brace is not
/// balanced. A break in an expression changes the expression.
fn brace_regions(text: &str) -> Option<Vec<(usize, usize)>> {
    let mut regions = Vec::new();
    let mut depth = 0usize;
    let mut open = 0usize;
    for (offset, ch) in text.char_indices() {
        match ch {
            '{' => {
                if depth == 0 {
                    open = offset;
                }
                depth += 1;
            }
            '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    regions.push((open, offset + 1));
                }
            }
            _ => {}
        }
    }
    (depth == 0).then_some(regions)
}

/// Puts the words on lines of [`WRAP_COLUMN`] columns or fewer. The first
/// line starts with `prefix`. Each other line has `indent` spaces. A word that
/// is longer than a line gets a line of its own.
///
/// `item` is true when the first line is the line of a list item. Text after
/// a dash is always prose, but a `[...]` there is an inline list.
///
/// It returns `None` when it cannot make each line prose.
fn render(words: &[String], prefix: &str, indent: usize, item: bool) -> Option<String> {
    let first = WRAP_COLUMN.saturating_sub(prefix.chars().count()).max(MIN_MEASURE);
    let rest = WRAP_COLUMN.saturating_sub(indent).max(MIN_MEASURE);
    let safe = |index: usize, text: &str| {
        if index == 0 && item {
            !(text.starts_with('[') && text.ends_with(']'))
        } else {
            is_prose(text)
        }
    };
    // A word in `blocked` must not start a line, because that line starts
    // like structure. Each pass of the loop blocks one more word. Thus the
    // loop ends after one pass for each word, or earlier.
    let mut blocked = vec![false; words.len()];
    let lines = loop {
        let lines = layout(words, (first, rest), &blocked);
        let unsafe_start = lines
            .iter()
            .enumerate()
            .find(|(index, (start, end))| !safe(*index, &words[*start..*end].join(" ")))
            .map(|(_, (start, _))| *start);
        match unsafe_start {
            None => break lines,
            Some(0) => return None,
            Some(start) if blocked[start] => return None,
            Some(start) => blocked[start] = true,
        }
    };

    let pad = " ".repeat(indent);
    let mut out = String::new();
    for (index, (start, end)) in lines.into_iter().enumerate() {
        let text = words[start..end].join(" ");
        out.push_str(if index == 0 { prefix } else { &pad });
        out.push_str(&text);
        out.push('\n');
    }
    Some(out)
}

/// Fills lines with words, as many as fit in `measure` columns. The first
/// number of `measure` is for the first line, the second for the others. A line breaks
/// earlier when the next line starts with a blocked word. When all earlier
/// break points are blocked, the line breaks later and is longer than
/// `measure`. It returns the word range of each line.
fn layout(words: &[String], measure: (usize, usize), blocked: &[bool]) -> Vec<(usize, usize)> {
    let sizes: Vec<usize> = words.iter().map(|word| word.chars().count()).collect();
    let mut lines = Vec::new();
    let mut start = 0;
    while start < words.len() {
        let measure = if lines.is_empty() { measure.0 } else { measure.1 };
        // `fit` is the end of the longest line that fits. It is one word or more.
        let mut fit = start + 1;
        let mut length = sizes[start];
        while fit < words.len() && length + 1 + sizes[fit] <= measure {
            length += 1 + sizes[fit];
            fit += 1;
        }
        let mut end = fit;
        while end < words.len() && blocked[end] && end > start + 1 {
            end -= 1;
        }
        if end < words.len() && blocked[end] {
            end = fit;
            while end < words.len() && blocked[end] {
                end += 1;
            }
        }
        lines.push((start, end));
        start = end;
    }
    lines
}

/// True when the parser reads a line in a block as prose.
fn is_prose(line: &str) -> bool {
    !(line == "pass"
        || line.starts_with("//")
        || parser::list_marker(line).is_some()
        || parser::merge_marker(line).is_some()
        || parser::parse_property_header(line, 0).is_some()
        || parser::misspaced_constraint(line)
        || parser::backslash_run(line).is_some())
}

fn line_start(source: &str, offset: usize) -> usize {
    source[..offset].rfind('\n').map_or(0, |at| at + 1)
}

/// The offset after the line terminator of the line that holds `offset`.
fn line_end(source: &str, offset: usize) -> usize {
    source[offset..]
        .find('\n')
        .map_or(source.len(), |at| offset + at + 1)
}

fn next_line_start(source: &str, start: usize) -> usize {
    line_end(source, start)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flow(source: &str) -> String {
        reflow(source, Path::new("test.pi"))
    }

    const LONG: &str = "Piton joins the lines of a paragraph with one space, so the formatter can move the breaks.";

    #[test]
    fn a_long_paragraph_wraps_at_eighty_columns() {
        let source = format!("anchor A:\n    text:\n        {LONG}\n");
        let out = flow(&source);
        assert!(out.lines().all(|line| line.chars().count() <= 80), "{out}");
        assert_eq!(
            out,
            "anchor A:\n    text:\n        Piton joins the lines of a paragraph with one space, so the formatter\n        can move the breaks.\n"
        );
        assert_eq!(flow(&out), out);
    }

    #[test]
    fn short_lines_join_into_full_lines() {
        let source = "anchor A:\n    text:\n        one\n        two\n        three\n";
        assert_eq!(flow(source), "anchor A:\n    text:\n        one two three\n");
    }

    #[test]
    fn a_blank_line_keeps_its_paragraph_break() {
        let source = "anchor A:\n    text:\n        one\n\n        two\n";
        assert_eq!(flow(source), source);
    }

    #[test]
    fn a_long_inline_value_moves_below_its_key() {
        let source = format!("anchor A:\n    text: {LONG}\n");
        assert_eq!(
            flow(&source),
            "anchor A:\n    text:\n        Piton joins the lines of a paragraph with one space, so the formatter\n        can move the breaks.\n"
        );
    }

    #[test]
    fn a_short_inline_value_stays() {
        let source = "anchor A:\n    text: short\n";
        assert_eq!(flow(source), source);
    }

    #[test]
    fn an_inline_value_joins_the_prose_below_it() {
        let source = "anchor A:\n    text: one\n        two\n";
        assert_eq!(flow(source), "anchor A:\n    text:\n        one two\n");
    }

    #[test]
    fn an_inline_value_before_a_blank_line_stays() {
        let source = "anchor A:\n    text: one\n\n        two\n";
        assert_eq!(flow(source), source);
    }

    #[test]
    fn a_line_never_starts_with_structure() {
        // `- 5` at the start of a line is a list item. `Note:` is a key.
        let words = "word ".repeat(13);
        let source = format!("anchor A:\n    text:\n        {words}- 5 and Note: here\n");
        let out = flow(&source);
        for line in out.lines().skip(2) {
            assert!(is_prose(line.trim_start()), "{out}");
            assert!(line.chars().count() <= 80, "{out}");
        }
        assert_eq!(flow(&out), out);
    }

    #[test]
    fn a_blocked_word_moves_down_and_the_rest_refills() {
        // The greedy fill puts `Emul:` at the start of line two. That is a key.
        let source = "anchor A:\n    text:\n        All Emul integration is in the plugin. The compiler holds no code\n        for Emul: the plugin's declaration lists emul under modules, and its\n        module hook gives the Piton module.\n";
        let out = flow(source);
        assert!(out.lines().all(|line| line.chars().count() <= 80), "{out}");
        assert!(out.contains("\n        for Emul: "), "{out}");
    }

    #[test]
    fn expressions_and_escapes_do_not_break() {
        let filler = "word ".repeat(12);
        let source = format!(
            "anchor A:\n    text:\n        {filler}{{a + b}} and \\ x y z \\ end\n"
        );
        let out = flow(&source);
        assert!(out.contains("{a + b}"), "{out}");
        assert!(out.contains("\\ x y z \\"), "{out}");
    }

    #[test]
    fn a_run_of_spaces_stays() {
        let source = "anchor A:\n    text:\n        one  two\n        three\n";
        assert_eq!(flow(source), "anchor A:\n    text:\n        one  two three\n");
    }

    #[test]
    fn a_long_list_item_wraps_under_its_dash() {
        let source = format!("anchor A:\n    items:\n        - {LONG}\n        - short\n");
        let out = flow(&source);
        assert_eq!(
            out,
            "anchor A:\n    items:\n        - Piton joins the lines of a paragraph with one space, so the formatter\n            can move the breaks.\n        - short\n"
        );
        assert_eq!(flow(&out), out);
    }

    #[test]
    fn a_list_item_joins_the_prose_below_it() {
        let source = "anchor A:\n    items:\n        - one\n            two\n        - three\n";
        assert_eq!(flow(source), "anchor A:\n    items:\n        - one two\n        - three\n");
    }

    #[test]
    fn a_list_item_keeps_a_nested_list() {
        let source = format!("anchor A:\n    items:\n        - {LONG}\n            - nested\n");
        let out = flow(&source);
        assert!(out.ends_with("            can move the breaks.\n            - nested\n"), "{out}");
    }

    #[test]
    fn escape_blocks_stay() {
        let source = format!("anchor A:\n    body:\n        \\\\\\\n        {LONG}\n        \\\\\\\n");
        assert_eq!(flow(&source), source);
    }
}
