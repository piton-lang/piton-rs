//! Core value model, diagnostics, and naming rules shared by every Piton crate.

pub mod diagnostics;
pub mod names;
pub mod text;
pub mod value;

pub use diagnostics::{Diagnostic, DiagnosticSink, Label, LineIndex, Severity, Span};
pub use names::{is_valid_artifact_name, is_valid_keyword, kebab_case, split_words, title_case};
pub use text::{Segment, Text};
pub use value::{
    format_number, AnchorId, AnchorView, EmptyAnchors, Mixed, MixedItem, Properties, Value,
    ValueKind,
};
