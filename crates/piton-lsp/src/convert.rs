//! Conversions between byte offsets and LSP positions.
//!
//! LSP counts columns in UTF-16 code units, and the specification corpus is full
//! of typographic quotes and em dashes, so this cannot be a byte count.

use piton_core::Span;
use tower_lsp::lsp_types::{Position, Range, Url};

/// How the client counts columns.
///
/// LSP's default is UTF-16 code units; a client may offer UTF-8 or UTF-32
/// instead (`general.positionEncodings`), and the server picks one at
/// `initialize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encoding {
    Utf8,
    #[default]
    Utf16,
    Utf32,
}

impl Encoding {
    fn width(self, ch: char) -> usize {
        match self {
            Encoding::Utf8 => ch.len_utf8(),
            Encoding::Utf16 => ch.len_utf16(),
            Encoding::Utf32 => 1,
        }
    }
}

/// Where each line of a text starts, so a position is a binary search and a
/// walk along one line rather than a walk from the top of the file.
#[derive(Debug, Clone)]
pub struct LineIndex<'a> {
    text: &'a str,
    starts: Vec<usize>,
    encoding: Encoding,
}

impl<'a> LineIndex<'a> {
    pub fn new(text: &'a str, encoding: Encoding) -> LineIndex<'a> {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(index, _)| index + 1));
        LineIndex {
            text,
            starts,
            encoding,
        }
    }

    pub fn text(&self) -> &'a str {
        self.text
    }

    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    /// The byte offset each line starts at; the first is always 0.
    pub fn line_starts(&self) -> &[usize] {
        &self.starts
    }

    /// The line holding `offset`, counting from 0.
    pub fn line_of(&self, offset: usize) -> usize {
        let offset = offset.min(self.text.len());
        self.starts.partition_point(|&start| start <= offset) - 1
    }

    /// The text of a line, without its line break.
    pub fn line_text(&self, line: usize) -> &'a str {
        let Some(&start) = self.starts.get(line) else {
            return "";
        };
        let end = self
            .starts
            .get(line + 1)
            .map(|next| next - 1)
            .unwrap_or(self.text.len());
        self.text[start..end].trim_end_matches('\r')
    }

    /// Converts a byte offset into a position. An offset inside a character
    /// counts from the start of that character.
    pub fn position(&self, offset: usize) -> Position {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = self.line_of(offset);
        let before = &self.text[self.starts[line]..offset];
        let character = if before.is_ascii() || self.encoding == Encoding::Utf8 {
            before.len()
        } else {
            before.chars().map(|ch| self.encoding.width(ch)).sum()
        };
        Position {
            line: line as u32,
            character: character as u32,
        }
    }

    /// Converts a position into a byte offset, clamping out-of-range input: a
    /// line past the end is the end of the text, and a column past the end of
    /// its line is the end of that line. A column inside a character (half a
    /// surrogate pair, a byte inside a UTF-8 sequence) lands on its start.
    pub fn offset(&self, position: Position) -> usize {
        let Some(&start) = self.starts.get(position.line as usize) else {
            return self.text.len();
        };
        let line = self.line_text(position.line as usize);
        let wanted = position.character as usize;
        let mut units = 0usize;
        for (offset, ch) in line.char_indices() {
            let width = self.encoding.width(ch);
            if units + width > wanted {
                return start + offset;
            }
            units += width;
        }
        start + line.len()
    }

    pub fn range(&self, span: Span) -> Range {
        Range {
            start: self.position(span.start),
            end: self.position(span.end.max(span.start)),
        }
    }

    pub fn span(&self, range: Range) -> Span {
        let start = self.offset(range.start);
        let end = self.offset(range.end);
        Span::new(start.min(end), end.max(start))
    }
}

/// Converts a byte offset into an LSP position, in UTF-16.
pub fn offset_to_position(text: &str, offset: usize) -> Position {
    LineIndex::new(text, Encoding::Utf16).position(offset)
}

/// Converts an LSP position, in UTF-16, into a byte offset, clamping
/// out-of-range input.
pub fn position_to_offset(text: &str, position: Position) -> usize {
    LineIndex::new(text, Encoding::Utf16).offset(position)
}

/// Converts a source span into an LSP range.
pub fn span_to_range(text: &str, span: Span) -> Range {
    LineIndex::new(text, Encoding::Utf16).range(span)
}

/// Converts a file path into a URL, for locations the editor can open.
pub fn path_to_url(path: &std::path::Path) -> Option<Url> {
    Url::from_file_path(path).ok()
}

/// Converts a URL into a file path.
pub fn url_to_path(url: &Url) -> Option<std::path::PathBuf> {
    url.to_file_path().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_count_utf16_units() {
        let text = "anchor A:\n    note: \u{201c}quoted\u{201d} and \u{1F600}\n";
        // Four spaces, `note: ` , eight units of quoted text, ` and `, then an
        // emoji that is a surrogate pair and counts as two units.
        let end_of_line = text.rfind('\n').expect("trailing newline");
        let position = offset_to_position(text, end_of_line);
        assert_eq!(position.line, 1);
        assert_eq!(position.character, 25);
    }

    #[test]
    fn offsets_round_trip() {
        let text = "one\ntwo\u{2014}dash\nthree\n";
        for offset in 0..text.len() {
            if !text.is_char_boundary(offset) {
                continue;
            }
            let position = offset_to_position(text, offset);
            assert_eq!(position_to_offset(text, position), offset, "at {offset}");
        }
    }

    #[test]
    fn out_of_range_positions_clamp() {
        let text = "a\nb\n";
        assert_eq!(
            position_to_offset(
                text,
                Position {
                    line: 99,
                    character: 99
                }
            ),
            text.len()
        );
    }

    #[test]
    fn a_column_past_the_end_of_a_line_is_its_end() {
        let index = LineIndex::new("ab\ncd\n", Encoding::Utf16);
        assert_eq!(index.offset(Position::new(0, 9)), 2);
    }

    #[test]
    fn each_encoding_counts_its_own_units() {
        let text = "x: \u{e9}\u{1F600}!\n";
        let bang = text.find('!').expect("bang");
        assert_eq!(LineIndex::new(text, Encoding::Utf8).position(bang).character, 9);
        assert_eq!(LineIndex::new(text, Encoding::Utf16).position(bang).character, 6);
        assert_eq!(LineIndex::new(text, Encoding::Utf32).position(bang).character, 5);
        for encoding in [Encoding::Utf8, Encoding::Utf16, Encoding::Utf32] {
            let index = LineIndex::new(text, encoding);
            assert_eq!(index.offset(index.position(bang)), bang, "{encoding:?}");
        }
    }

    #[test]
    fn half_a_surrogate_pair_lands_on_the_character() {
        let text = "\u{1F600}x";
        let index = LineIndex::new(text, Encoding::Utf16);
        assert_eq!(index.offset(Position::new(0, 1)), 0);
        assert_eq!(index.offset(Position::new(0, 2)), 4);
    }

    #[test]
    fn a_crlf_line_ends_before_its_carriage_return() {
        let index = LineIndex::new("ab\r\ncd", Encoding::Utf16);
        assert_eq!(index.line_text(0), "ab");
        assert_eq!(index.offset(Position::new(0, 9)), 2);
        assert_eq!(index.position(5), Position::new(1, 1));
    }
}
