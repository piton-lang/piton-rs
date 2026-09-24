//! Compiler diagnostics.
//!
//! Every phase of the compiler reports through the same structure so `piton
//! check`, `piton build`, and the language server can all render the same
//! information without translating between representations.

use std::fmt;
use std::path::{Path, PathBuf};

/// How serious a diagnostic is.
///
/// `check` exits non-zero when any [`Severity::Error`] was reported; warnings
/// are informational and never fail a build on their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A byte range within a single source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }

    pub fn empty(at: usize) -> Self {
        Span { start: at, end: at }
    }

    pub fn cover(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    pub fn contains(self, offset: usize) -> bool {
        offset >= self.start && offset < self.end
    }

    pub fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
}

impl From<std::ops::Range<usize>> for Span {
    fn from(r: std::ops::Range<usize>) -> Self {
        Span::new(r.start, r.end)
    }
}

/// A secondary location attached to a diagnostic, used for "originally declared
/// here" style breadcrumbs.
#[derive(Debug, Clone)]
pub struct Label {
    pub file: PathBuf,
    pub span: Span,
    pub message: String,
}

impl Label {
    pub fn new(file: impl Into<PathBuf>, span: Span, message: impl Into<String>) -> Self {
        Label {
            file: file.into(),
            span,
            message: message.into(),
        }
    }
}

/// A single compiler message.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Stable machine-readable identifier, e.g. `unresolved-import`.
    pub code: String,
    pub message: String,
    pub file: PathBuf,
    pub span: Span,
    pub labels: Vec<Label>,
    /// Optional actionable suggestion rendered after the message.
    pub help: Option<String>,
    /// The source anchor and property that produced the failure, when the
    /// failure happened during resolution or generation rather than parsing.
    pub origin: Option<String>,
}

impl Diagnostic {
    pub fn new(
        severity: Severity,
        code: impl Into<String>,
        message: impl Into<String>,
        file: impl Into<PathBuf>,
        span: Span,
    ) -> Self {
        Diagnostic {
            severity,
            code: code.into(),
            message: message.into(),
            file: file.into(),
            span,
            labels: Vec::new(),
            help: None,
            origin: None,
        }
    }

    pub fn error(
        code: impl Into<String>,
        message: impl Into<String>,
        file: impl Into<PathBuf>,
        span: Span,
    ) -> Self {
        Diagnostic::new(Severity::Error, code, message, file, span)
    }

    pub fn warning(
        code: impl Into<String>,
        message: impl Into<String>,
        file: impl Into<PathBuf>,
        span: Span,
    ) -> Self {
        Diagnostic::new(Severity::Warning, code, message, file, span)
    }

    pub fn with_label(mut self, label: Label) -> Self {
        self.labels.push(label);
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn with_origin(mut self, origin: impl Into<String>) -> Self {
        self.origin = Some(origin.into());
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

/// Collects diagnostics across a compilation.
#[derive(Debug, Default, Clone)]
pub struct DiagnosticSink {
    items: Vec<Diagnostic>,
}

impl DiagnosticSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.items.push(diagnostic);
    }

    pub fn extend(&mut self, other: impl IntoIterator<Item = Diagnostic>) {
        self.items.extend(other);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.items.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn has_errors(&self) -> bool {
        self.items.iter().any(Diagnostic::is_error)
    }

    pub fn error_count(&self) -> usize {
        self.items.iter().filter(|d| d.is_error()).count()
    }

    pub fn warning_count(&self) -> usize {
        self.items
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count()
    }

    pub fn as_slice(&self) -> &[Diagnostic] {
        &self.items
    }

    /// Sorts by file then position so output is stable between runs.
    pub fn sort(&mut self) {
        self.items.sort_by(|a, b| {
            a.file
                .cmp(&b.file)
                .then(a.span.start.cmp(&b.span.start))
                .then(a.code.cmp(&b.code))
                .then(a.message.cmp(&b.message))
        });
        self.items.dedup_by(|a, b| {
            a.file == b.file && a.span == b.span && a.code == b.code && a.message == b.message
        });
    }
}

impl IntoIterator for DiagnosticSink {
    type Item = Diagnostic;
    type IntoIter = std::vec::IntoIter<Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

/// Translates byte offsets into 1-based line/column pairs for display.
pub struct LineIndex {
    line_starts: Vec<usize>,
    len: usize,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        LineIndex {
            line_starts,
            len: text.len(),
        }
    }

    /// Returns a zero-based `(line, column)` pair, with the column counted in
    /// bytes from the start of the line.
    pub fn line_col(&self, offset: usize) -> (usize, usize) {
        let offset = offset.min(self.len);
        let line = match self.line_starts.binary_search(&offset) {
            Ok(exact) => exact,
            Err(next) => next - 1,
        };
        (line, offset - self.line_starts[line])
    }

    pub fn line_start(&self, line: usize) -> Option<usize> {
        self.line_starts.get(line).copied()
    }

    pub fn offset(&self, line: usize, col: usize) -> usize {
        let start = self.line_starts.get(line).copied().unwrap_or(self.len);
        (start + col).min(self.len)
    }
}

/// Renders a diagnostic the way the CLI prints it.
pub fn render(diagnostic: &Diagnostic, source: Option<&str>, root: Option<&Path>) -> String {
    let path = display_path(&diagnostic.file, root);
    let mut out = String::new();
    if let Some(src) = source {
        let index = LineIndex::new(src);
        let (line, col) = index.line_col(diagnostic.span.start);
        out.push_str(&format!(
            "{}: {}:{}:{}: {} [{}]\n",
            diagnostic.severity,
            path,
            line + 1,
            col + 1,
            diagnostic.message,
            diagnostic.code
        ));
        if let Some(start) = index.line_start(line) {
            let end = index
                .line_start(line + 1)
                .unwrap_or(src.len())
                .min(src.len());
            let text = src[start..end].trim_end_matches(['\n', '\r']);
            let number = format!("{}", line + 1);
            out.push_str(&format!("  {} | {}\n", number, text));
            let width = diagnostic.span.len().max(1);
            let caret_col = text
                .char_indices()
                .take_while(|(i, _)| *i < col)
                .count();
            out.push_str(&format!(
                "  {} | {}{}\n",
                " ".repeat(number.len()),
                " ".repeat(caret_col),
                "^".repeat(width.min(text.len().saturating_sub(caret_col).max(1)))
            ));
        }
    } else {
        out.push_str(&format!(
            "{}: {}: {} [{}]\n",
            diagnostic.severity, path, diagnostic.message, diagnostic.code
        ));
    }
    if let Some(origin) = &diagnostic.origin {
        out.push_str(&format!("  origin: {origin}\n"));
    }
    for label in &diagnostic.labels {
        out.push_str(&format!(
            "  note: {} ({})\n",
            label.message,
            display_path(&label.file, root)
        ));
    }
    if let Some(help) = &diagnostic.help {
        out.push_str(&format!("  help: {help}\n"));
    }
    out
}

fn display_path(path: &Path, root: Option<&Path>) -> String {
    match root.and_then(|r| path.strip_prefix(r).ok()) {
        Some(rel) => rel.display().to_string(),
        None => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_index_maps_offsets() {
        let index = LineIndex::new("ab\ncd\n\nef");
        assert_eq!(index.line_col(0), (0, 0));
        assert_eq!(index.line_col(3), (1, 0));
        assert_eq!(index.line_col(6), (2, 0));
        assert_eq!(index.line_col(7), (3, 0));
    }

    #[test]
    fn sink_sorts_and_dedups() {
        let mut sink = DiagnosticSink::new();
        sink.push(Diagnostic::error("b", "second", "z.pi", Span::new(5, 6)));
        sink.push(Diagnostic::error("a", "first", "a.pi", Span::new(0, 1)));
        sink.push(Diagnostic::error("a", "first", "a.pi", Span::new(0, 1)));
        sink.sort();
        assert_eq!(sink.len(), 2);
        assert_eq!(sink.as_slice()[0].message, "first");
    }
}
