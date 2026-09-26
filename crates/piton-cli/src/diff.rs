//! Line-by-line diffs of files a command changed, computed in the compiler so
//! no diff program has to be installed.
//!
//! Each changed region is printed with its old and new line numbers beside
//! every line, the way a code review shows it:
//!
//! ```text
//! tethers/piton/scope/belay/Belay.pi
//!   24  24 │  export anchor Belay:
//!   25     │- description: LOCAL EDIT
//!       25 │+ description:
//! ```

use anstream::println;
use similar::{ChangeTag, TextDiff};

use crate::packages::FileChange;
use crate::style::{self, paint};

/// Lines of unchanged context shown around each change.
const CONTEXT: usize = 3;

/// Prints each change line by line, with paths shown under `prefix`: removed
/// lines red with only their old number, added lines green with only their
/// new one, and unchanged context with both. Separate regions of one file are
/// split by a `⋯` line.
pub fn print(prefix: &str, changes: &[FileChange]) {
    for change in changes {
        let path = format!("{prefix}/{}", change.path);
        let status = match (&change.before, &change.after) {
            (None, _) => " (new file)",
            (_, None) => " (deleted)",
            _ => "",
        };
        println!();
        println!("{}{}", paint(style::HEADING, &path), paint(style::DIM, status));

        let before = change.before.as_deref().map(std::str::from_utf8);
        let after = change.after.as_deref().map(std::str::from_utf8);
        let (Ok(old), Ok(new)) = (before.unwrap_or(Ok("")), after.unwrap_or(Ok(""))) else {
            println!("  {}", paint(style::DIM, "binary file differs"));
            continue;
        };

        let diff = TextDiff::from_lines(old, new);
        // Wide enough for the largest line number in either version.
        let width = old.lines().count().max(new.lines().count()).max(1).to_string().len();
        let number = |index: Option<usize>| match index {
            Some(index) => format!("{:>width$}", index + 1),
            None => " ".repeat(width),
        };

        for (group, operations) in diff.grouped_ops(CONTEXT).iter().enumerate() {
            if group > 0 {
                println!("{}", paint(style::DIM, format!("{} ⋯", " ".repeat(width * 2 + 1))));
            }
            for operation in operations {
                for line in diff.iter_changes(operation) {
                    let text = line.value().trim_end_matches(['\n', '\r']);
                    let gutter = paint(
                        style::DIM,
                        format!("{} {} │", number(line.old_index()), number(line.new_index())),
                    );
                    match line.tag() {
                        ChangeTag::Delete => {
                            println!("{gutter}{}", paint(style::ERROR_LINE, format!("-{text}")))
                        }
                        ChangeTag::Insert => {
                            println!("{gutter}{}", paint(style::SUCCESS_LINE, format!("+{text}")))
                        }
                        ChangeTag::Equal => println!("{gutter} {text}"),
                    }
                }
            }
        }
    }
}
