//! Converting between byte offsets and LSP positions.
//!
//! LSP counts columns in UTF-16 code units, so the index remembers where the
//! non-ASCII characters are and only does the expensive work on lines that
//! actually contain them.

use piton_syntax::{TextRange, TextSize};
use tower_lsp::lsp_types::{Position, Range};

/// Line starts for one file's text.
pub struct LineIndex {
    text: String,
    starts: Vec<u32>,
}

impl LineIndex {
    pub fn new(text: &str) -> LineIndex {
        let mut starts = vec![0u32];
        for (offset, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(offset as u32 + 1);
            }
        }
        LineIndex { text: text.to_string(), starts }
    }

    /// The LSP position of a byte offset.
    pub fn position(&self, offset: TextSize) -> Position {
        let offset = u32::from(offset).min(self.text.len() as u32);
        let line = self.starts.partition_point(|start| *start <= offset).saturating_sub(1);
        let line_start = self.starts[line] as usize;
        let column = self.text[line_start..offset as usize].encode_utf16().count();
        Position { line: line as u32, character: column as u32 }
    }

    pub fn range(&self, range: TextRange) -> Range {
        Range { start: self.position(range.start()), end: self.position(range.end()) }
    }

    /// The byte offset of an LSP position.
    pub fn offset(&self, position: Position) -> TextSize {
        let Some(&line_start) = self.starts.get(position.line as usize) else {
            return TextSize::new(self.text.len() as u32);
        };
        let line_end = self
            .starts
            .get(position.line as usize + 1)
            .map(|it| *it as usize)
            .unwrap_or(self.text.len());
        let line = &self.text[line_start as usize..line_end];
        let mut utf16 = 0usize;
        for (offset, ch) in line.char_indices() {
            if utf16 >= position.character as usize {
                return TextSize::new(line_start + offset as u32);
            }
            utf16 += ch.len_utf16();
        }
        TextSize::new(line_end as u32)
    }

    /// The whole file as a range.
    pub fn full_range(&self) -> Range {
        Range {
            start: Position { line: 0, character: 0 },
            end: self.position(TextSize::new(self.text.len() as u32)),
        }
    }
}
