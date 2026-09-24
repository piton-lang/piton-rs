//! Line counting.
//!
//! `piton loc` reports total, code, comment, and blank lines. A line that holds
//! both code and a trailing comment counts as code, so the categories sum to the
//! total.

use std::path::{Path, PathBuf};

use piton_syntax::prose;

/// Counts for one file or for a whole project.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub total: usize,
    pub code: usize,
    pub comments: usize,
    pub blank: usize,
}

impl Counts {
    pub fn add(&mut self, other: Counts) {
        self.total += other.total;
        self.code += other.code;
        self.comments += other.comments;
        self.blank += other.blank;
    }
}

/// Counts grouped by file, which is the grouping `piton loc` reports.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub files: Vec<(PathBuf, Counts)>,
    pub summary: Counts,
}

impl Report {
    pub fn push(&mut self, path: PathBuf, counts: Counts) {
        self.summary.add(counts);
        self.files.push((path, counts));
    }
}

/// Counts the lines in one source text.
///
/// A multi-line escape block is content, so its lines count as code even when
/// they look like comments.
pub fn count(source: &str) -> Counts {
    let mut counts = Counts::default();
    let mut escape_run: Option<usize> = None;

    for line in source.lines() {
        counts.total += 1;
        let trimmed = line.trim();
        let backslashes = (!trimmed.is_empty() && trimmed.chars().all(|c| c == '\\'))
            .then(|| trimmed.chars().count());

        // A multi-line escape block is content, whatever it contains.
        if let Some(run) = escape_run {
            counts.code += 1;
            if backslashes == Some(run) {
                escape_run = None;
            }
            continue;
        }
        if let Some(run) = backslashes {
            escape_run = Some(run);
            counts.code += 1;
            continue;
        }

        if trimmed.is_empty() {
            counts.blank += 1;
            continue;
        }
        let (code, comment) = prose::split_comment(trimmed);
        if code.trim().is_empty() && comment.is_some() {
            counts.comments += 1;
        } else {
            counts.code += 1;
        }
    }

    // A file that does not end in a newline still has that last line.
    if source.ends_with('\n') || source.is_empty() {
        // `lines()` already handled it.
    }
    counts
}

/// Counts a set of files.
pub fn count_files(paths: &[PathBuf], root: Option<&Path>) -> Report {
    let mut report = Report::default();
    for path in paths {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let display = match root.and_then(|r| path.strip_prefix(r).ok()) {
            Some(relative) => relative.to_path_buf(),
            None => path.clone(),
        };
        report.push(display, count(&source));
    }
    report.files.sort_by(|a, b| a.0.cmp(&b.0));
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_sum_to_the_total() {
        let source = "// a comment\n\nanchor A:\n    value: 1 // trailing\n";
        let counts = count(source);
        assert_eq!(counts.total, 4);
        assert_eq!(counts.comments, 1);
        assert_eq!(counts.blank, 1);
        assert_eq!(counts.code, 2);
        assert_eq!(counts.code + counts.comments + counts.blank, counts.total);
    }

    #[test]
    fn escape_block_content_counts_as_code() {
        let source = "anchor A:\n    body:\n        \\\\\\\n        // not a comment\n\n        \\\\\\\n";
        let counts = count(source);
        assert_eq!(counts.comments, 0);
        assert_eq!(counts.blank, 0);
        assert_eq!(counts.total, 6);
    }

    #[test]
    fn a_code_fence_is_just_text() {
        // Code blocks aren't special, so a comment line inside one is a comment.
        let source = "anchor A:\n    body:\n        ```\n        // a comment\n        ```\n";
        assert_eq!(count(source).comments, 1);
    }
}
