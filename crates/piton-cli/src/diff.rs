//! Unified diffs of files a command changed, coloured on a terminal.

use anstream::println;
use similar::{ChangeTag, TextDiff};

use crate::packages::FileChange;
use crate::style::{self, paint};

/// Prints a unified diff of each change, with paths shown under `prefix`:
/// removed lines red, added lines green, hunk headers cyan. Piped, it is a
/// plain unified diff that `patch` can read.
pub fn print(prefix: &str, changes: &[FileChange]) {
    for change in changes {
        let path = format!("{prefix}/{}", change.path);
        let before = change.before.as_deref().map(std::str::from_utf8);
        let after = change.after.as_deref().map(std::str::from_utf8);
        let (Ok(old), Ok(new)) = (before.unwrap_or(Ok("")), after.unwrap_or(Ok(""))) else {
            println!("{}", paint(style::HEADING, format!("Binary file {path} differs")));
            continue;
        };

        let from = match change.before {
            Some(_) => format!("a/{path}"),
            None => "/dev/null".to_string(),
        };
        let to = match change.after {
            Some(_) => format!("b/{path}"),
            None => "/dev/null".to_string(),
        };
        println!("{}", paint(style::HEADING, format!("--- {from}")));
        println!("{}", paint(style::HEADING, format!("+++ {to}")));

        let diff = TextDiff::from_lines(old, new);
        for hunk in diff.unified_diff().context_radius(3).iter_hunks() {
            println!("{}", paint(style::NAME, hunk.header()));
            for line in hunk.iter_changes() {
                let text = line.value().strip_suffix('\n').unwrap_or(line.value());
                match line.tag() {
                    ChangeTag::Delete => println!("{}", paint(style::ERROR_LINE, format!("-{text}"))),
                    ChangeTag::Insert => println!("{}", paint(style::SUCCESS_LINE, format!("+{text}"))),
                    ChangeTag::Equal => println!(" {text}"),
                }
                if line.missing_newline() {
                    println!("\\ No newline at end of file");
                }
            }
        }
    }
}
