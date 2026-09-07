//! Rendering diagnostics for a terminal.

use std::path::Path;

use piton_core::db::Db;
use piton_core::diag::{Diagnostic, Severity};
use piton_syntax::TextRange;

/// Print every diagnostic and answer whether any of them was an error.
pub fn report(db: &Db, diagnostics: &[Diagnostic]) -> bool {
    let mut errors = 0usize;
    let mut warnings = 0usize;
    for diagnostic in diagnostics {
        match diagnostic.severity {
            Severity::Error => errors += 1,
            Severity::Warning => warnings += 1,
        }
        print_one(db, diagnostic);
    }
    if errors + warnings > 0 {
        eprintln!("{}", summary(errors, warnings));
    }
    errors > 0
}

fn summary(errors: usize, warnings: usize) -> String {
    let mut parts = Vec::new();
    if errors > 0 {
        parts.push(format!("{errors} error{}", plural(errors)));
    }
    if warnings > 0 {
        parts.push(format!("{warnings} warning{}", plural(warnings)));
    }
    parts.join(", ")
}

fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

fn print_one(db: &Db, diagnostic: &Diagnostic) {
    let label = match diagnostic.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };
    let file = db.file(diagnostic.file);
    let (line, column) = position(&file.text, diagnostic.range);
    eprintln!(
        "{label}[{}]: {}\n  --> {}:{line}:{column}",
        diagnostic.code,
        diagnostic.message,
        display_path(&file.source.display())
    );
    if let Some(snippet) = snippet(&file.text, diagnostic.range) {
        eprintln!("{snippet}");
    }
}

/// One-based line and column of a range's start.
pub fn position(text: &str, range: TextRange) -> (usize, usize) {
    let offset = usize::from(range.start()).min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(1, |it| it.chars().count() + 1);
    (line, column)
}

/// The offending line with a caret run beneath it.
fn snippet(text: &str, range: TextRange) -> Option<String> {
    let start = usize::from(range.start()).min(text.len());
    let line_start = text[..start].rfind('\n').map_or(0, |it| it + 1);
    let line_end = text[start..].find('\n').map_or(text.len(), |it| start + it);
    let line = &text[line_start..line_end];
    if line.trim().is_empty() {
        return None;
    }
    let column = text[line_start..start].chars().count();
    let width = text[start..usize::from(range.end()).min(line_end)].chars().count().max(1);
    Some(format!("   | {line}\n   | {}{}", " ".repeat(column), "^".repeat(width)))
}

/// Shorten an absolute path to something relative to the working directory.
fn display_path(path: &str) -> String {
    let Ok(cwd) = std::env::current_dir() else { return path.to_string() };
    Path::new(path)
        .strip_prefix(&cwd)
        .map(|it| it.display().to_string())
        .unwrap_or_else(|_| path.to_string())
}
