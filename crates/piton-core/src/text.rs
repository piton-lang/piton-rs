//! Interpolated text.
//!
//! Most Piton strings are plain text, but a string may embed `@{...}`
//! references, and a reference has to survive until serialization: the Markdown
//! adapter turns it into a link relative to the file being written, while a
//! structured adapter has to represent identity some other way. Resolving it to
//! a name during evaluation would throw that away, so a string keeps its
//! segments.

use std::fmt;

use crate::value::{AnchorId, AnchorView};

/// One piece of a string value.
#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    Literal(String),
    /// An anchor whose identity is preserved until output serialization.
    Reference(AnchorId),
}

/// A string value, possibly containing embedded references.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Text {
    segments: Vec<Segment>,
}

impl Text {
    pub fn empty() -> Text {
        Text::default()
    }

    pub fn plain(text: impl Into<String>) -> Text {
        let text = text.into();
        if text.is_empty() {
            Text::default()
        } else {
            Text {
                segments: vec![Segment::Literal(text)],
            }
        }
    }

    pub fn reference(id: AnchorId) -> Text {
        Text {
            segments: vec![Segment::Reference(id)],
        }
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    pub fn into_segments(self) -> Vec<Segment> {
        self.segments
    }

    pub fn is_empty(&self) -> bool {
        self.segments.iter().all(|segment| match segment {
            Segment::Literal(text) => text.is_empty(),
            Segment::Reference(_) => false,
        })
    }

    /// True when the text carries no references and can be treated as a plain
    /// `&str`.
    pub fn is_plain(&self) -> bool {
        !self
            .segments
            .iter()
            .any(|s| matches!(s, Segment::Reference(_)))
    }

    /// Borrows the text as a plain string when it holds at most one literal.
    pub fn as_plain(&self) -> Option<&str> {
        match self.segments.as_slice() {
            [] => Some(""),
            [Segment::Literal(text)] => Some(text),
            _ => None,
        }
    }

    pub fn references(&self) -> impl Iterator<Item = AnchorId> + '_ {
        self.segments.iter().filter_map(|s| match s {
            Segment::Reference(id) => Some(*id),
            Segment::Literal(_) => None,
        })
    }

    pub fn push_literal(&mut self, text: impl AsRef<str>) {
        let text = text.as_ref();
        if text.is_empty() {
            return;
        }
        match self.segments.last_mut() {
            Some(Segment::Literal(existing)) => existing.push_str(text),
            _ => self.segments.push(Segment::Literal(text.to_string())),
        }
    }

    pub fn push_reference(&mut self, id: AnchorId) {
        self.segments.push(Segment::Reference(id));
    }

    pub fn push_text(&mut self, other: &Text) {
        for segment in &other.segments {
            match segment {
                Segment::Literal(text) => self.push_literal(text),
                Segment::Reference(id) => self.push_reference(*id),
            }
        }
    }

    pub fn concat(mut self, other: &Text) -> Text {
        self.push_text(other);
        self
    }

    /// Renders the text with every reference replaced by the referenced
    /// anchor's source name. This is the plain-text fallback used when a target
    /// has no distinct reference representation inside prose.
    pub fn render_plain(&self, anchors: &dyn AnchorView) -> String {
        let mut out = String::new();
        for segment in &self.segments {
            match segment {
                Segment::Literal(text) => out.push_str(text),
                Segment::Reference(id) => out.push_str(anchors.name(*id)),
            }
        }
        out
    }

    /// Renders the text with each reference replaced by `render`'s output.
    pub fn render_with(&self, mut render: impl FnMut(AnchorId) -> String) -> String {
        let mut out = String::new();
        for segment in &self.segments {
            match segment {
                Segment::Literal(text) => out.push_str(text),
                Segment::Reference(id) => out.push_str(&render(*id)),
            }
        }
        out
    }

    /// Applies `f` to every literal run, leaving references untouched.
    pub fn map_literals(&self, mut f: impl FnMut(&str) -> String) -> Text {
        Text {
            segments: self
                .segments
                .iter()
                .map(|segment| match segment {
                    Segment::Literal(text) => Segment::Literal(f(text)),
                    Segment::Reference(id) => Segment::Reference(*id),
                })
                .collect(),
        }
    }

    /// Drops leading and trailing whitespace from the outermost literal runs.
    pub fn trim(&self) -> Text {
        let mut segments = self.segments.clone();
        if let Some(Segment::Literal(first)) = segments.first_mut() {
            *first = first.trim_start().to_string();
        }
        if let Some(Segment::Literal(last)) = segments.last_mut() {
            *last = last.trim_end().to_string();
        }
        segments.retain(|segment| !matches!(segment, Segment::Literal(t) if t.is_empty()));
        Text { segments }
    }
}

impl From<&str> for Text {
    fn from(value: &str) -> Self {
        Text::plain(value)
    }
}

impl From<String> for Text {
    fn from(value: String) -> Self {
        Text::plain(value)
    }
}

impl fmt::Display for Text {
    /// Displays references as `@{anchor#n}`; use [`Text::render_plain`] when a
    /// real name is available.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for segment in &self.segments {
            match segment {
                Segment::Literal(text) => f.write_str(text)?,
                Segment::Reference(id) => write!(f, "@{{{id}}}")?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_runs_coalesce() {
        let mut text = Text::plain("a");
        text.push_literal("b");
        assert_eq!(text.segments().len(), 1);
        assert_eq!(text.as_plain(), Some("ab"));
    }

    #[test]
    fn references_break_plain_view() {
        let mut text = Text::plain("see ");
        text.push_reference(AnchorId(7));
        assert!(!text.is_plain());
        assert_eq!(text.as_plain(), None);
        assert_eq!(text.render_with(|id| format!("<{}>", id.0)), "see <7>");
    }

    #[test]
    fn trim_touches_only_outer_literals() {
        let mut text = Text::plain("  a ");
        text.push_reference(AnchorId(1));
        text.push_literal(" b  ");
        let trimmed = text.trim();
        assert_eq!(trimmed.render_with(|_| "R".into()), "a R b");
    }
}
