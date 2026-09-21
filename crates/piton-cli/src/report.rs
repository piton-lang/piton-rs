//! Rendering diagnostics and summaries for the terminal.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use piton_core::{diagnostics, DiagnosticSink};

/// Prints diagnostics with source context, returning whether any were errors.
pub fn diagnostics(sink: &DiagnosticSink, sources: &dyn Fn(&Path) -> Option<String>, root: &Path) -> bool {
    let mut cache: HashMap<PathBuf, Option<String>> = HashMap::new();
    for diagnostic in sink.iter() {
        let source = cache
            .entry(diagnostic.file.clone())
            .or_insert_with(|| sources(&diagnostic.file));
        eprint!(
            "{}",
            diagnostics::render(diagnostic, source.as_deref(), Some(root))
        );
    }
    sink.has_errors()
}

/// Prints a one-line summary of how many problems were found.
pub fn summary(sink: &DiagnosticSink) {
    let errors = sink.error_count();
    let warnings = sink.warning_count();
    if errors == 0 && warnings == 0 {
        eprintln!("no problems found");
        return;
    }
    let mut parts = Vec::new();
    if errors > 0 {
        parts.push(format!("{errors} {}", plural(errors, "error", "errors")));
    }
    if warnings > 0 {
        parts.push(format!(
            "{warnings} {}",
            plural(warnings, "warning", "warnings")
        ));
    }
    eprintln!("{}", parts.join(", "));
}

pub fn plural<'a>(count: usize, one: &'a str, many: &'a str) -> &'a str {
    if count == 1 {
        one
    } else {
        many
    }
}

/// Prints an error that is not tied to a source location.
pub fn fail(message: impl std::fmt::Display) {
    eprintln!("error: {message}");
}
