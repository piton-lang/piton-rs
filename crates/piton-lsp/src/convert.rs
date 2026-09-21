//! Conversions between byte offsets and LSP positions.
//!
//! LSP counts columns in UTF-16 code units, and the specification corpus is full
//! of typographic quotes and em dashes, so this cannot be a byte count.

use piton_core::Span;
use tower_lsp::lsp_types::{Position, Range, Url};

/// Converts a byte offset into an LSP position.
pub fn offset_to_position(text: &str, offset: usize) -> Position {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let line_start = before.rfind('\n').map(|index| index + 1).unwrap_or(0);
    let character = text[line_start..offset].encode_utf16().count();
    Position {
        line: line as u32,
        character: character as u32,
    }
}

/// Converts an LSP position into a byte offset, clamping out-of-range input.
pub fn position_to_offset(text: &str, position: Position) -> usize {
    let mut line_start = 0usize;
    for _ in 0..position.line {
        match text[line_start..].find('\n') {
            Some(index) => line_start += index + 1,
            None => return text.len(),
        }
    }
    let line_end = text[line_start..]
        .find('\n')
        .map(|index| line_start + index)
        .unwrap_or(text.len());

    let mut utf16 = 0usize;
    for (offset, ch) in text[line_start..line_end].char_indices() {
        if utf16 >= position.character as usize {
            return line_start + offset;
        }
        utf16 += ch.len_utf16();
    }
    line_end
}

/// Converts a source span into an LSP range.
pub fn span_to_range(text: &str, span: Span) -> Range {
    Range {
        start: offset_to_position(text, span.start),
        end: offset_to_position(text, span.end.max(span.start)),
    }
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
}
