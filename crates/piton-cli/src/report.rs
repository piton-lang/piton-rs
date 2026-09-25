//! Rendering diagnostics, problems, and outcomes for the terminal.
//!
//! What a command produced, and how it went, is printed as it happens.
//! Problems -- diagnostics, errors, warnings, and the help and notes under
//! them -- are held back and printed together once the command is done, with
//! the count of them last, so they are the last thing on the screen whatever
//! the command printed before them. [`flush`] prints them; `main` calls it.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use piton_core::{diagnostics, DiagnosticSink};

use crate::style::{self, paint};

/// The problems held back until the command is done, as styled text.
static DEFERRED: Mutex<String> = Mutex::new(String::new());

/// Whether the command printed an outcome, so the problems after it are set
/// apart by a blank line.
static OUTCOME: AtomicBool = AtomicBool::new(false);

/// Holds `text` back until [`flush`].
fn defer(text: impl AsRef<str>) {
    let mut deferred = DEFERRED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    deferred.push_str(text.as_ref());
}

/// Prints the problems held back, last. Called once, when the command is done.
pub fn flush() {
    let deferred = std::mem::take(&mut *DEFERRED.lock().unwrap_or_else(|poisoned| poisoned.into_inner()));
    if deferred.is_empty() {
        return;
    }
    if OUTCOME.load(Ordering::Relaxed) {
        anstream::eprintln!();
    }
    anstream::eprint!("{deferred}");
}

/// Holds diagnostics with source context back until the command is done,
/// returning whether any were errors.
pub fn diagnostics(sink: &DiagnosticSink, sources: &dyn Fn(&Path) -> Option<String>, root: &Path) -> bool {
    let mut cache: HashMap<PathBuf, Option<String>> = HashMap::new();
    for diagnostic in sink.iter() {
        let source = cache
            .entry(diagnostic.file.clone())
            .or_insert_with(|| sources(&diagnostic.file));
        let rendered = diagnostics::render(diagnostic, source.as_deref(), Some(root));
        defer(styled(&rendered));
    }
    sink.has_errors()
}

/// Holds back a one-line count of the problems found, which ends the output.
pub fn summary(sink: &DiagnosticSink) {
    let errors = sink.error_count();
    let warnings = sink.warning_count();
    if errors == 0 && warnings == 0 {
        defer(format!("{}\n", paint(style::SUCCESS, "no problems found")));
        return;
    }
    let mut parts = Vec::new();
    if errors > 0 {
        parts.push(paint(
            style::ERROR,
            format!("{errors} {}", plural(errors, "error", "errors")),
        ));
    }
    if warnings > 0 {
        parts.push(paint(
            style::WARNING,
            format!("{warnings} {}", plural(warnings, "warning", "warnings")),
        ));
    }
    defer(format!("{}\n", parts.join(", ")));
}

pub fn plural<'a>(count: usize, one: &'a str, many: &'a str) -> &'a str {
    if count == 1 {
        one
    } else {
        many
    }
}

/// An error that is not tied to a source location, printed with the other
/// problems when the command is done.
pub fn fail(message: impl std::fmt::Display) {
    defer(format!("{} {message}\n", paint(style::ERROR, "error:")));
}

/// A warning that is not tied to a source location, printed with the other
/// problems when the command is done.
pub fn warn(message: impl std::fmt::Display) {
    defer(format!("{} {message}\n", paint(style::WARNING, "warning:")));
}

/// A suggestion under the error or warning before it.
pub fn help(message: impl std::fmt::Display) {
    defer(format!("  {} {message}\n", paint(style::HELP, "help:")));
}

/// A detail under the error or warning before it.
pub fn note(message: impl std::fmt::Display) {
    defer(format!("  {} {message}\n", paint(style::NOTE, "note:")));
}

/// Prints how the command went, on stderr: `✓` and the message in green.
/// This is the line to read first, so it stands out.
pub fn done(message: impl std::fmt::Display) {
    OUTCOME.store(true, Ordering::Relaxed);
    anstream::eprintln!(
        "{} {}",
        paint(style::SUCCESS, "✓"),
        paint(style::HEADING, message)
    );
}

/// Prints that the command stopped short, on stderr: `✗` and the message in
/// red, before the problems that stopped it.
pub fn stopped(message: impl std::fmt::Display) {
    OUTCOME.store(true, Ordering::Relaxed);
    anstream::eprintln!(
        "{} {}",
        paint(style::ERROR, "✗"),
        paint(style::HEADING, message)
    );
}

/// A path the command wrote, for stdout. On a terminal it is marked as added
/// with its directory dimmed; piped, it is the bare path.
pub fn added(path: impl std::fmt::Display) -> String {
    let path = path.to_string();
    marked(path.clone(), "+", style::SUCCESS).unwrap_or(path)
}

/// A path the command removed, for stdout: marked like [`added`] on a
/// terminal, and `removed <path>` piped.
pub fn removed(path: impl std::fmt::Display) -> String {
    let path = path.to_string();
    marked(path.clone(), "-", style::ERROR).unwrap_or_else(|| format!("removed {path}"))
}

/// `path` marked for a terminal, or None when stdout isn't one.
fn marked(path: String, mark: &str, mark_style: anstyle::Style) -> Option<String> {
    if !std::io::stdout().is_terminal() {
        return None;
    }
    let (parent, name) = match path.rsplit_once('/') {
        Some((parent, name)) => (format!("{parent}/"), name.to_string()),
        None => (String::new(), path.clone()),
    };
    let mut out = String::new();
    let _ = write!(
        out,
        "  {} {}{name}",
        paint(mark_style, mark),
        paint(style::DIM, parent)
    );
    Some(out)
}

/// A rendered diagnostic with colour: its severity, the gutter and carets
/// under the quoted source, and the labels beneath it. The text is unchanged,
/// so it reads the same when the colour is stripped.
fn styled(rendered: &str) -> String {
    let mut severity = style::ERROR;
    let mut out = String::new();
    for (index, line) in rendered.lines().enumerate() {
        if index == 0 {
            let (label, rest) = line.split_once(':').unwrap_or((line, ""));
            if label == "warning" {
                severity = style::WARNING;
            }
            out.push_str(&paint(severity, format!("{label}:")));
            match rest.rsplit_once(" [") {
                Some((message, code)) => {
                    out.push_str(message);
                    out.push_str(&paint(style::DIM, format!(" [{code}")));
                }
                None => out.push_str(rest),
            }
        } else if let Some((gutter, text)) = line.split_once(" | ") {
            out.push_str(&paint(style::GUTTER, format!("{gutter} |")));
            out.push(' ');
            if text.trim_start().starts_with('^') && text.trim().chars().all(|c| c == '^') {
                out.push_str(&paint(severity, text));
            } else {
                out.push_str(text);
            }
        } else if let Some((label, rest)) = line.trim_start().split_once(": ") {
            let label_style = match label {
                "help" => style::HELP,
                "note" => style::NOTE,
                _ => style::DIM,
            };
            out.push_str(&format!("  {} {rest}", paint(label_style, format!("{label}:"))));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}
