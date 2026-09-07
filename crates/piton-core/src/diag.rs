//! Diagnostics produced by every stage of the compiler.

use piton_syntax::TextRange;

use crate::FileId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

/// One problem, located precisely enough for an editor to underline it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub file: FileId,
    pub range: TextRange,
}

impl Diagnostic {
    pub fn error(code: &'static str, file: FileId, range: TextRange, message: impl Into<String>) -> Diagnostic {
        Diagnostic { severity: Severity::Error, code, message: message.into(), file, range }
    }

    pub fn warning(code: &'static str, file: FileId, range: TextRange, message: impl Into<String>) -> Diagnostic {
        Diagnostic { severity: Severity::Warning, code, message: message.into(), file, range }
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

/// Collects diagnostics without letting callers worry about ordering.
#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    items: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn push(&mut self, diagnostic: Diagnostic) {
        if !self.items.contains(&diagnostic) {
            self.items.push(diagnostic);
        }
    }

    pub fn extend(&mut self, other: impl IntoIterator<Item = Diagnostic>) {
        for diagnostic in other {
            self.push(diagnostic);
        }
    }

    pub fn has_errors(&self) -> bool {
        self.items.iter().any(Diagnostic::is_error)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.items.iter()
    }

    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }
}
