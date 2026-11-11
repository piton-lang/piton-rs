//! Links between generated Markdown documents.
//!
//! A reference is written as a link to the heading its target lands on, in
//! whichever file the target was written to, relative to the file being
//! written. Which anchor goes in which file is the caller's decision; this
//! module turns that layout into headings, fragments and links. Belay uses it
//! for its reference tree, and the plugin host offers it to every plugin as a
//! link space.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use piton_core::{title_case, AnchorId, AnchorView, Ref};

use crate::markdown;

/// A heading in a generated file, with the fragment a link uses for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub level: usize,
    pub title: String,
    pub slug: String,
}

/// The headings one anchor contributes to its module's file.
#[derive(Debug, Clone)]
struct Chunk {
    anchor: AnchorId,
    headings: Vec<Heading>,
}

/// The headings of one reference file, grouped by the anchor they belong to.
#[derive(Debug, Clone, Default)]
pub struct FileIndex {
    chunks: Vec<Chunk>,
}

impl FileIndex {
    /// Indexes a file made of one rendered document per anchor, in order.
    ///
    /// Fragments are assigned the way GitHub assigns them: a repeated slug
    /// gains `-1`, `-2` and so on across the whole file, so a link to the
    /// second anchor's `Description` lands on that anchor's heading.
    pub fn build(parts: &[(AnchorId, &str)]) -> FileIndex {
        let mut counts: HashMap<String, usize> = HashMap::new();
        let chunks = parts
            .iter()
            .map(|(anchor, text)| Chunk {
                anchor: *anchor,
                headings: scan_headings(text)
                    .into_iter()
                    .map(|(level, title)| {
                        let base = crate::heading_slug(&title);
                        let count = counts.entry(base.clone()).or_insert(0);
                        let slug = if *count == 0 {
                            base.clone()
                        } else {
                            format!("{base}-{count}")
                        };
                        *count += 1;
                        Heading { level, title, slug }
                    })
                    .collect(),
            })
            .collect();
        FileIndex { chunks }
    }

    /// Every fragment the file defines.
    pub fn slugs(&self) -> impl Iterator<Item = &str> {
        self.chunks
            .iter()
            .flat_map(|chunk| chunk.headings.iter().map(|h| h.slug.as_str()))
    }

    /// The fragment a reference links to: the anchor's own heading, or the
    /// heading of the property it names. A property rendered without a heading
    /// of its own -- inside a list or a fenced structure -- links to the
    /// nearest heading above it.
    pub fn fragment(&self, target: &Ref, anchors: &dyn AnchorView) -> Option<String> {
        let chunk = self.chunks.iter().find(|chunk| chunk.anchor == target.anchor)?;
        let headings = &chunk.headings;
        let title = title_case(anchors.name(target.anchor));
        let mut position = headings.iter().position(|h| h.title == title)?;
        for part in &target.path {
            let wanted = title_case(part);
            let level = headings[position].level;
            let found = headings[position + 1..]
                .iter()
                .take_while(|h| h.level > level)
                .position(|h| h.level == level + 1 && h.title == wanted);
            match found {
                Some(offset) => position += 1 + offset,
                None => break,
            }
        }
        Some(headings[position].slug.clone())
    }
}

/// The ATX headings of a Markdown text, skipping fenced code.
pub fn scan_headings(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut fence: Option<&str> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let marker = if trimmed.starts_with("```") {
            Some("```")
        } else if trimmed.starts_with("~~~") {
            Some("~~~")
        } else {
            None
        };
        if let Some(marker) = marker {
            match fence {
                Some(open) if open == marker => fence = None,
                None => fence = Some(marker),
                _ => {}
            }
            continue;
        }
        if fence.is_some() || line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        let level = line.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&level) {
            let rest = &line[level..];
            if rest.is_empty() || rest.starts_with(' ') {
                out.push((level, rest.trim().trim_end_matches('#').trim().to_string()));
            }
        }
    }
    out
}

/// Resolves references to links relative to the file being written.
pub struct Links<'a> {
    pub from_directory: PathBuf,
    pub anchors: &'a dyn AnchorView,
    /// Each documented anchor's file.
    pub locations: &'a HashMap<AnchorId, PathBuf>,
    /// Each reference file's headings; absent while the files are first
    /// rendered to find their headings.
    pub index: Option<&'a HashMap<PathBuf, FileIndex>>,
    /// References with no planned location, reported after rendering.
    pub misses: &'a RefCell<Vec<Ref>>,
    /// Every link written, with the directory it is relative to, so that
    /// validation checks the links Belay wrote and not the ones an author
    /// did.
    pub generated: &'a RefCell<HashSet<(PathBuf, String)>>,
}

impl Links<'_> {
    fn record(&self, link: String) -> String {
        self.generated
            .borrow_mut()
            .insert((self.from_directory.clone(), link.clone()));
        link
    }
}

impl markdown::LinkResolver for Links<'_> {
    fn link(&self, target: &Ref) -> Option<String> {
        let Some(location) = self.locations.get(&target.anchor) else {
            self.misses.borrow_mut().push(target.clone());
            return None;
        };
        let file = relative_path_link(&self.from_directory, location);
        let fragment = match self.index {
            // A construct's own output, like a SKILL.md, is not a reference
            // document with headings to land on, so the link is to the file.
            Some(index) => match index.get(location) {
                Some(headings) => headings.fragment(target, self.anchors),
                None => return Some(self.record(file)),
            },
            None => None,
        }
        .unwrap_or_else(|| crate::reference_fragment(self.anchors, target));
        Some(self.record(format!("{file}#{fragment}")))
    }
}

/// A relative path from a directory to a file, starting with `./` or `../`
/// so it reads as a path: `./Button.md`, `../types/Lists.md`.
///
/// Both paths are project-relative and compared as written, never through the
/// filesystem.
pub fn relative_path_link(from_directory: &Path, target: &Path) -> String {
    let link = crate::relative_link(from_directory, target);
    if link.starts_with("../") {
        link
    } else {
        format!("./{link}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_in_fences_are_not_headings() {
        let text = "# Title\n\n```\n# not a heading\n```\n\n## Sub\n\nbody\n";
        assert_eq!(
            scan_headings(text),
            vec![(1, "Title".to_string()), (2, "Sub".to_string())]
        );
    }

    #[test]
    fn repeated_slugs_are_numbered_across_the_file() {
        let index = FileIndex::build(&[
            (AnchorId(1), "# First\n\n## Description\n"),
            (AnchorId(2), "# Second\n\n## Description\n"),
        ]);
        let slugs: Vec<&str> = index.slugs().collect();
        assert_eq!(slugs, vec!["first", "description", "second", "description-1"]);
    }

    #[test]
    fn relative_links_read_as_paths() {
        assert_eq!(relative_path_link(Path::new("a/b"), Path::new("a/b/C.md")), "./C.md");
        assert_eq!(relative_path_link(Path::new("a/b"), Path::new("a/d/C.md")), "../d/C.md");
        assert_eq!(relative_path_link(Path::new("a"), Path::new("a/b/C.md")), "./b/C.md");
    }
}
