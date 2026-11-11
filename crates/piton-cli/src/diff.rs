//! Line-by-line diffs of the files that a command changed. The compiler
//! calculates them. Thus the user does not need a diff program.
//!
//! Each changed region shows its old and new line numbers next to each line.
//! A code review shows them in the same way:
//!
//! ```text
//! tethers/piton/scope/belay/Belay.pi
//!   24  24 │  export anchor Belay:
//!   25     │- description: LOCAL EDIT
//!       25 │+ description:
//! ```

use std::fmt::Write as _;

use similar::{ChangeTag, TextDiff};

use crate::packages::FileChange;
use crate::style::{self, paint};

/// Lines of unchanged context shown around each change.
const CONTEXT: usize = 3;

/// Prints each change line by line. It shows the paths below `prefix`. A
/// removed line is red, with only its old number. An added line is green, with
/// only its new number. Unchanged context shows the two numbers. A `⋯` line
/// divides separate regions of one file.
pub fn print(prefix: &str, changes: &[FileChange]) {
    anstream::print!("{}", render(prefix, changes));
}

/// The same function, on stderr. It is for a command that uses stdout for its
/// own output.
pub fn eprint(prefix: &str, changes: &[FileChange]) {
    anstream::eprint!("{}", render(prefix, changes));
}

/// The diff that [`print`] writes, as text.
pub fn render(prefix: &str, changes: &[FileChange]) -> String {
    let mut out = String::new();
    macro_rules! println {
        () => {{ out.push('\n'); }};
        ($($arg:tt)*) => {{ let _ = writeln!(out, $($arg)*); }};
    }
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
        // Sufficiently wide for the largest line number in the two versions.
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
    out
}
