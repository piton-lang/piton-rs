//! A workspace written out of one string, for testing the editor features.
//!
//! ```text
//! //- /piton.config.pi
//! use @piton/config
//! ...
//! //- /spec/Main.pi
//! anchor A:
//!     title: $0Hi
//! ```
//!
//! Each `//- /path` line starts a file. The file holding `$0` is opened as a
//! buffer, the way an editor would have it, with the marker taken out; the
//! marker is the cursor every position-taking helper uses.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use piton_lsp::convert::{offset_to_position, path_to_url, position_to_offset};
use piton_lsp::features;
use piton_lsp::world::{ClientCaps, World};
use piton_lsp::{TOKEN_MODIFIERS, TOKEN_TYPES};
use tower_lsp::lsp_types::*;

/// The configuration most workspaces need: sources under `spec/`, entered at
/// `spec/index.pi`.
pub const CONFIG: &str = "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n";

pub struct Workspace {
    pub dir: PathBuf,
    pub world: World,
    /// The file holding the cursor, relative to `dir`.
    pub file: String,
    pub offset: usize,
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

impl Workspace {
    pub fn new(spec: &str) -> Workspace {
        Workspace::with_caps(spec, ClientCaps::default())
    }

    pub fn with_caps(spec: &str, caps: ClientCaps) -> Workspace {
        let dir = std::env::temp_dir().join(format!(
            "piton-lsp-ws-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");

        let mut files: Vec<(String, String)> = Vec::new();
        for line in spec.trim_start_matches('\n').split_inclusive('\n') {
            if let Some(path) = line.strip_prefix("//- /") {
                files.push((path.trim().to_string(), String::new()));
            } else if let Some((_, text)) = files.last_mut() {
                text.push_str(line);
            }
        }
        if !files.iter().any(|(path, _)| path == "piton.config.pi") {
            files.push(("piton.config.pi".into(), CONFIG.into()));
        }

        let mut cursor = None;
        for (path, text) in &mut files {
            if let Some(offset) = text.find("$0") {
                text.replace_range(offset..offset + 2, "");
                cursor = Some((path.clone(), offset));
            }
            let full = dir.join(&*path);
            std::fs::create_dir_all(full.parent().expect("parent")).expect("dirs");
            std::fs::write(&full, &*text).expect("write");
        }
        let (file, offset) = cursor.unwrap_or_else(|| (files[0].0.clone(), 0));

        let mut world = World::new(&dir);
        world.caps = caps;
        let path = dir.join(&file);
        let text = std::fs::read_to_string(&path).expect("readable");
        world.open_document(path.clone(), text, 7);
        world.recompile(Some(&path));
        Workspace {
            dir,
            world,
            file,
            offset,
        }
    }

    pub fn path(&self, relative: &str) -> PathBuf {
        self.dir.join(relative)
    }

    pub fn uri(&self, relative: &str) -> Url {
        path_to_url(&self.path(relative)).expect("url")
    }

    pub fn text(&self, relative: &str) -> String {
        self.world.text(&self.path(relative)).expect("text")
    }

    /// The cursor, as a position in its file.
    pub fn position(&self) -> Position {
        offset_to_position(&self.text(&self.file), self.offset)
    }

    /// The position of the first `needle` in a file, plus `delta` bytes.
    pub fn at(&self, relative: &str, needle: &str, delta: usize) -> Position {
        let text = self.text(relative);
        let offset = text
            .find(needle)
            .unwrap_or_else(|| panic!("`{needle}` not in {relative}"));
        offset_to_position(&text, offset + delta)
    }

    /// The text a range covers in a file.
    pub fn slice(&self, relative: &str, range: Range) -> String {
        let text = self.text(relative);
        let start = position_to_offset(&text, range.start);
        let end = position_to_offset(&text, range.end);
        text[start..end].to_string()
    }

    /// `file:line` of a location, the line counted from 1.
    pub fn place(&self, location: &Location) -> String {
        let path = location.uri.to_file_path().expect("a file");
        format!(
            "{}:{}",
            path.strip_prefix(&self.dir).unwrap_or(&path).display(),
            location.range.start.line + 1
        )
    }

    pub fn hover(&self) -> Option<Hover> {
        features::hover(&self.world, &self.uri(&self.file), self.position())
    }

    /// The hover's Markdown and the text its range covers.
    pub fn hover_text(&self) -> Option<(String, String)> {
        let hover = self.hover()?;
        let HoverContents::Markup(markup) = hover.contents else {
            panic!("expected markup");
        };
        let covered = hover
            .range
            .map(|range| self.slice(&self.file, range))
            .unwrap_or_default();
        Some((markup.value, covered))
    }

    pub fn completion(&self) -> Vec<CompletionItem> {
        match features::completion(&self.world, &self.uri(&self.file), self.position()) {
            Some(CompletionResponse::Array(items)) => items,
            Some(CompletionResponse::List(list)) => list.items,
            None => Vec::new(),
        }
    }

    pub fn labels(&self) -> Vec<String> {
        let mut labels: Vec<String> = self.completion().into_iter().map(|item| item.label).collect();
        labels.sort();
        labels
    }

    /// Every semantic token of a file: line and column (0-based, UTF-16), the
    /// text, the type and the modifiers.
    pub fn tokens(&self, relative: &str) -> Vec<(u32, u32, String, String, Vec<String>)> {
        let Some(SemanticTokensResult::Tokens(tokens)) =
            features::semantic_tokens(&self.world, &self.uri(relative))
        else {
            return Vec::new();
        };
        let text = self.text(relative);
        let lines: Vec<&str> = text.lines().collect();
        let (mut line, mut column) = (0u32, 0u32);
        let mut out = Vec::new();
        for token in tokens.data {
            if token.delta_line > 0 {
                line += token.delta_line;
                column = token.delta_start;
            } else {
                column += token.delta_start;
            }
            let units: Vec<u16> = lines[line as usize].encode_utf16().collect();
            let written =
                String::from_utf16_lossy(&units[column as usize..(column + token.length) as usize]);
            let modifiers = TOKEN_MODIFIERS
                .iter()
                .enumerate()
                .filter(|(index, _)| token.token_modifiers_bitset & (1 << index) != 0)
                .map(|(_, modifier)| modifier.as_str().to_string())
                .collect();
            out.push((
                line,
                column,
                written,
                TOKEN_TYPES[token.token_type as usize].as_str().to_string(),
                modifiers,
            ));
        }
        out
    }

    pub fn inlay_hints(&self, relative: &str) -> Vec<InlayHint> {
        features::inlay_hints(
            &self.world,
            &self.uri(relative),
            Range::new(Position::new(0, 0), Position::new(u32::MAX, 0)),
        )
        .unwrap_or_default()
    }
}

/// An inlay hint's label as one string.
pub fn hint_label(hint: &InlayHint) -> String {
    match &hint.label {
        InlayHintLabel::String(text) => text.clone(),
        InlayHintLabel::LabelParts(parts) => parts.iter().map(|part| part.value.as_str()).collect(),
    }
}

/// Applies edits to a text, back to front.
pub fn apply(text: &str, edits: &[TextEdit]) -> String {
    let mut sorted: Vec<&TextEdit> = edits.iter().collect();
    sorted.sort_by_key(|edit| (edit.range.start.line, edit.range.start.character));
    let mut out = text.to_string();
    for edit in sorted.into_iter().rev() {
        let start = position_to_offset(&out, edit.range.start);
        let end = position_to_offset(&out, edit.range.end);
        out.replace_range(start..end, &edit.new_text);
    }
    out
}

/// The edits a workspace edit makes, per file relative to `dir`, whichever
/// form it came in.
pub fn edits_by_file(dir: &Path, edit: &WorkspaceEdit) -> Vec<(String, Vec<TextEdit>)> {
    let relative = |uri: &Url| {
        let path = uri.to_file_path().expect("a file");
        path.strip_prefix(dir).unwrap_or(&path).display().to_string()
    };
    let mut out = Vec::new();
    if let Some(changes) = &edit.changes {
        for (uri, edits) in changes {
            out.push((relative(uri), edits.clone()));
        }
    }
    if let Some(DocumentChanges::Edits(changes)) = &edit.document_changes {
        for change in changes {
            let edits = change
                .edits
                .iter()
                .map(|edit| match edit {
                    OneOf::Left(edit) => edit.clone(),
                    OneOf::Right(annotated) => annotated.text_edit.clone(),
                })
                .collect();
            out.push((relative(&change.text_document.uri), edits));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}
