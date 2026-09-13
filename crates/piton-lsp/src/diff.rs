//! The smallest set of whole-line edits that turns one text into another.
//!
//! Replacing a whole document to apply a formatting change moves the cursor,
//! drops selections, and unfolds everything, even when one line changed. An
//! editor keeps all of those for the lines an edit does not touch, so the edits
//! touch only the lines that differ.

use std::ops::Range as Span;

use tower_lsp::lsp_types::{Position, Range, TextEdit};

/// Above this many line pairs the middle of a change is replaced in one edit
/// rather than compared line by line, to keep a huge rewrite from costing more
/// than it saves.
const MAX_CELLS: usize = 4_000_000;

/// Edits that turn `old` into `new`, each covering a run of whole lines.
pub fn line_edits(old: &str, new: &str) -> Vec<TextEdit> {
    let before: Vec<&str> = old.split_inclusive('\n').collect();
    let after: Vec<&str> = new.split_inclusive('\n').collect();

    let prefix = before.iter().zip(&after).take_while(|(a, b)| a == b).count();
    let room = before.len().min(after.len()) - prefix;
    let suffix =
        before.iter().rev().zip(after.iter().rev()).take(room).take_while(|(a, b)| a == b).count();
    let old_middle = &before[prefix..before.len() - suffix];
    let new_middle = &after[prefix..after.len() - suffix];
    if old_middle.is_empty() && new_middle.is_empty() {
        return Vec::new();
    }

    let hunks = match old_middle.len().saturating_mul(new_middle.len()) <= MAX_CELLS {
        true => hunks(old_middle, new_middle),
        false => vec![(0..old_middle.len(), 0..new_middle.len())],
    };
    hunks
        .into_iter()
        .map(|(removed, inserted)| TextEdit {
            range: Range {
                start: position(old, &before, prefix + removed.start),
                end: position(old, &before, prefix + removed.end),
            },
            new_text: new_middle[inserted].concat(),
        })
        .collect()
}

/// The start of line `line`, or the end of the text when the line is past it.
fn position(text: &str, lines: &[&str], line: usize) -> Position {
    if line < lines.len() || text.is_empty() || text.ends_with('\n') {
        return Position { line: line as u32, character: 0 };
    }
    // The last line has no newline, so nothing starts after it; its end is the
    // end of the document.
    let last = lines.len() - 1;
    Position { line: last as u32, character: lines[last].encode_utf16().count() as u32 }
}

/// The runs of lines that differ, from a longest common subsequence.
fn hunks(old: &[&str], new: &[&str]) -> Vec<(Span<usize>, Span<usize>)> {
    let (rows, columns) = (old.len(), new.len());
    // `table[i][j]` is the length of the common subsequence of `old[i..]` and
    // `new[j..]`, stored row by row.
    let mut table = vec![0u32; (rows + 1) * (columns + 1)];
    let at = |i: usize, j: usize| i * (columns + 1) + j;
    for i in (0..rows).rev() {
        for j in (0..columns).rev() {
            table[at(i, j)] = match old[i] == new[j] {
                true => table[at(i + 1, j + 1)] + 1,
                false => table[at(i + 1, j)].max(table[at(i, j + 1)]),
            };
        }
    }

    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    let (mut removed_from, mut inserted_from) = (0, 0);
    while i < rows && j < columns {
        if old[i] == new[j] {
            if removed_from < i || inserted_from < j {
                out.push((removed_from..i, inserted_from..j));
            }
            i += 1;
            j += 1;
            removed_from = i;
            inserted_from = j;
        } else if table[at(i + 1, j)] >= table[at(i, j + 1)] {
            i += 1;
        } else {
            j += 1;
        }
    }
    if removed_from < rows || inserted_from < columns {
        out.push((removed_from..rows, inserted_from..columns));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::line_index::LineIndex;

    /// Apply edits the way an editor does, last first.
    fn apply(text: &str, mut edits: Vec<TextEdit>) -> String {
        let mut out = text.to_string();
        edits.sort_by_key(|edit| std::cmp::Reverse((edit.range.start.line, edit.range.start.character)));
        for edit in edits {
            let index = LineIndex::new(&out);
            let start = u32::from(index.offset(edit.range.start)) as usize;
            let end = u32::from(index.offset(edit.range.end)) as usize;
            out.replace_range(start..end, &edit.new_text);
        }
        out
    }

    #[test]
    fn identical_texts_need_no_edits() {
        assert!(line_edits("a\nb\n", "a\nb\n").is_empty());
        assert!(line_edits("", "").is_empty());
    }

    #[test]
    fn only_the_lines_that_changed_are_touched() {
        let old = "one\ntwo\nthree\nfour\nfive\n";
        let new = "one\nTWO\nthree\nfour\nFIVE\n";
        let edits = line_edits(old, new);
        assert_eq!(edits.len(), 2, "{edits:?}");
        assert_eq!(edits[0].range.start.line, 1);
        assert_eq!(edits[1].range.start.line, 4);
        assert_eq!(apply(old, edits), new);
    }

    #[test]
    fn insertions_deletions_and_a_missing_final_newline_all_apply() {
        for (old, new) in [
            ("a\nb\nc\n", "a\nc\n"),
            ("a\nc\n", "a\nb\nc\n"),
            ("a\nb", "a\nb\n"),
            ("a\nb\n", "a\nb"),
            ("", "x\n"),
            ("x\n", ""),
            ("anchor A:\n  name: v\n  other: w", "anchor A:\n    name: v\n    other: w\n"),
            ("héllo\nwörld", "héllo\nworld\n"),
        ] {
            assert_eq!(apply(old, line_edits(old, new)), new, "{old:?} -> {new:?}");
        }
    }
}
