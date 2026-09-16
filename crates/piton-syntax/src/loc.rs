//! Counting the lines of a Piton file.
//!
//! A line count is more interesting here than in most languages, because most
//! of a Piton file is meant to be prose. Separating the prose from the
//! structure around it says something a single number cannot: whether a
//! document is mostly saying things, or mostly organising them.
//!
//! Each line is classified by what is written on it, using the syntax tree
//! rather than by matching text, so a `//` inside a URL is not a comment and a
//! sentence that happens to contain a colon is not a key.

use crate::kind::SyntaxKind;
use crate::{NodeOrToken, SyntaxNode};

/// What a line holds.
///
/// The order is the precedence used when a line holds more than one thing:
/// prose wins, because a line that carries words is a line of words whatever
/// else is on it. `name: a sentence` and `- a row of a table` are prose; the
/// key and the bullet are how the prose is filed, not what the line says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LineKind {
    /// Nothing at all.
    Blank,
    /// Only a comment.
    Comment,
    /// Structure: a declaration, a key with no text, an import, an expression.
    Code,
    /// A line carrying text meant to be read.
    Prose,
}

/// How many lines of each kind a file holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub total: usize,
    pub code: usize,
    pub prose: usize,
    pub comment: usize,
    pub blank: usize,
}

impl Counts {
    /// Add another file's lines to this total.
    pub fn add(&mut self, other: Counts) {
        self.total += other.total;
        self.code += other.code;
        self.prose += other.prose;
        self.comment += other.comment;
        self.blank += other.blank;
    }
}

/// Count the lines of one file.
pub fn count(source: &str) -> Counts {
    let lines = classify(source);
    let mut counts = Counts { total: lines.len(), ..Counts::default() };
    for kind in lines {
        match kind {
            LineKind::Blank => counts.blank += 1,
            LineKind::Comment => counts.comment += 1,
            LineKind::Prose => counts.prose += 1,
            LineKind::Code => counts.code += 1,
        }
    }
    counts
}

/// Classify every line of a file.
pub fn classify(source: &str) -> Vec<LineKind> {
    let total = source.lines().count();
    let mut lines = vec![LineKind::Blank; total];
    if total == 0 {
        return lines;
    }
    let starts = line_starts(source);
    let root = crate::parse(source).syntax();
    mark(&root, &starts, &mut lines);
    lines
}

fn mark(node: &SyntaxNode, starts: &[usize], lines: &mut [LineKind]) {
    for element in node.descendants_with_tokens() {
        let NodeOrToken::Token(token) = element else { continue };
        let kind = match token.kind() {
            SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE | SyntaxKind::BLANK => continue,
            // A comment claims a line only if nothing else is written on it.
            SyntaxKind::COMMENT => LineKind::Comment,
            _ if in_prose(&token) => LineKind::Prose,
            _ => LineKind::Code,
        };
        let line = line_of(starts, usize::from(token.text_range().start()));
        if let Some(slot) = lines.get_mut(line) {
            // Prose beats code beats a comment; see `LineKind`.
            *slot = (*slot).max(kind);
        }
    }
}

/// True when a token is part of a text value, a fenced code block included:
/// what a fence holds is written into the string, not read as Piton.
fn in_prose(token: &crate::SyntaxToken) -> bool {
    token
        .parent()
        .map(|parent| {
            parent
                .ancestors()
                .any(|node| matches!(node.kind(), SyntaxKind::TEXT_VALUE | SyntaxKind::CODE_BLOCK))
        })
        .unwrap_or(false)
}

fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(source.match_indices('\n').map(|(at, _)| at + 1));
    starts
}

fn line_of(starts: &[usize], offset: usize) -> usize {
    starts.partition_point(|start| *start <= offset).saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_file_has_no_lines() {
        assert_eq!(count(""), Counts::default());
    }

    #[test]
    fn each_kind_of_line_is_counted_once() {
        let source = "\
// a comment

anchor A:
    name: a key with a sentence after it
    note:
        A line of prose.
        And another.

        After a blank line.
    items:
        - one
        - two
";
        let counts = count(source);
        assert_eq!(counts.total, 12);
        assert_eq!(counts.comment, 1);
        assert_eq!(counts.blank, 2);
        // `anchor A:`, `note:`, and `items:` — the keys that carry no words.
        assert_eq!(counts.code, 3, "{counts:?}");
        // The three lines of the string, the key with a sentence after it, and
        // the two list items, which are rows of a table and not scaffolding.
        assert_eq!(counts.prose, 6, "{counts:?}");
        assert_eq!(counts.code + counts.prose + counts.comment + counts.blank, counts.total);
    }

    #[test]
    fn a_line_is_classified_by_what_is_written_not_by_matching_text() {
        // A `//` inside a URL is not a comment, and a trailing comment does not
        // stop the line counting as what is written before it.
        let lines = classify("url: see https://example.com/a//b now\nx: 1 // note\n");
        assert_eq!(lines, vec![LineKind::Prose, LineKind::Code]);

        // A sentence with a colon in the middle is prose, not a key.
        let lines = classify("note:\n    Read this: it matters.\n");
        assert_eq!(lines, vec![LineKind::Code, LineKind::Prose]);
    }

    #[test]
    fn a_line_is_prose_when_it_carries_words_and_code_when_it_does_not() {
        // A table written as list items is the substance of a document.
        let table = "\
catalogue:
    - file.new - New - Ctrl+N - empties the buffer
    - file.open - Open - Ctrl+O - the open dialog
";
        assert_eq!(classify(table), vec![LineKind::Code, LineKind::Prose, LineKind::Prose]);

        // A value that is not words is structure: a number, a literal, an
        // expression, a type, an import.
        let structure = "\
count: 42
flag: true
computed: {1 + 2}
typed:: string
from ./other import Thing
spread:
    + {super.items}
";
        assert!(
            classify(structure).iter().all(|kind| *kind == LineKind::Code),
            "{:?}",
            classify(structure)
        );
    }

    #[test]
    fn a_file_with_no_final_newline_still_counts_its_last_line() {
        assert_eq!(count("x: 1").total, 1);
        assert_eq!(count("x: 1").code, 1);
    }

    #[test]
    fn totals_add_up_across_files() {
        let mut total = Counts::default();
        total.add(count("x: 1\n"));
        total.add(count("// c\n\ny: 2\n"));
        assert_eq!(total.total, 4);
        assert_eq!(total.code, 2);
        assert_eq!(total.comment, 1);
        assert_eq!(total.blank, 1);
    }
}
