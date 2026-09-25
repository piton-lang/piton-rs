//! References: which anchors the reference tree holds, where each is written,
//! and which heading a link lands on.
//!
//! A reference (`@{Anchor}` or `@{Anchor.property}`) links to the referenced
//! anchor's compiled representation instead of copying it. Every anchor an
//! emitted artifact references is therefore compiled into the target's
//! reference directory, transitively, so a link always has somewhere to go.
//!
//! The reference tree mirrors the source tree: one file per source module,
//! each referenced anchor a heading in its module's file. Two anchors with the
//! same name in different files land in different files, and a link names the
//! file and the heading: `./Button.md#button`, or `./Button.md#color` for a
//! property.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use piton_compile::{module, Compilation};
use piton_core::{title_case, AnchorId, AnchorView, MixedItem, Ref, Value};
use piton_emit::markdown;

/// One place a reference was written.
#[derive(Debug, Clone)]
pub struct Occurrence {
    pub target: Ref,
    /// The anchor whose property holds the reference.
    pub from: AnchorId,
    /// That property.
    pub key: String,
}

/// Why a referenced anchor has no compiled representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unrepresentable {
    /// Abstract anchors declare a shape and are never compiled.
    Abstract,
    /// Anchors of a bundled package have no source tree to mirror.
    Package,
    /// The anchor's module lies outside the project.
    OutsideProject,
}

/// The anchors reached through references from the emitted artifacts.
#[derive(Debug, Default)]
pub struct Closure {
    /// Anchors that get a reference document.
    pub referenced: BTreeSet<AnchorId>,
    /// Every reference written in emitted content, in discovery order.
    pub occurrences: Vec<Occurrence>,
    /// Referenced anchors that cannot be compiled, and why.
    pub unrepresentable: BTreeMap<AnchorId, Unrepresentable>,
}

/// Follows references from `roots` until nothing new is reached.
///
/// The content of an anchor embedded by value (`{Foo}`) is inlined wherever it
/// is embedded, so its references are followed from there too.
pub fn close(compilation: &Compilation, roots: &[AnchorId]) -> Closure {
    let mut closure = Closure::default();
    let mut visited: HashSet<AnchorId> = HashSet::new();
    let mut queue: Vec<AnchorId> = roots.to_vec();
    queue.reverse();

    while let Some(anchor) = queue.pop() {
        if !visited.insert(anchor) {
            continue;
        }
        let mut found = Vec::new();
        let mut embedded = HashSet::new();
        embedded.insert(anchor);
        for (key, value) in compilation.store().anchor(anchor).properties.iter() {
            occurrences_in(compilation, value, anchor, key, &mut found, &mut embedded);
        }
        for occurrence in found {
            let reached = occurrence.target.anchor;
            closure.occurrences.push(occurrence);
            match representable(compilation, reached) {
                Ok(()) => {
                    closure.referenced.insert(reached);
                    if !visited.contains(&reached) {
                        queue.push(reached);
                    }
                }
                Err(reason) => {
                    closure.unrepresentable.insert(reached, reason);
                }
            }
        }
    }
    closure
}

/// Collects the references in a value with the anchor property each was
/// written in, descending into embedded anchors so a reference inside an
/// inlined copy is attributed to where it was written.
fn occurrences_in(
    compilation: &Compilation,
    value: &Value,
    from: AnchorId,
    key: &str,
    out: &mut Vec<Occurrence>,
    embedded: &mut HashSet<AnchorId>,
) {
    let push = |target: &Ref, out: &mut Vec<Occurrence>| {
        out.push(Occurrence {
            target: target.clone(),
            from,
            key: key.to_string(),
        })
    };
    match value {
        Value::Reference(target) => push(target, out),
        Value::Str(text) => text.refs().for_each(|target| push(target, out)),
        Value::List(items) => {
            for item in items {
                occurrences_in(compilation, item, from, key, out, embedded);
            }
        }
        Value::Dict(map) => {
            for item in map.values() {
                occurrences_in(compilation, item, from, key, out, embedded);
            }
        }
        Value::Mixed(mixed) => {
            for item in &mixed.items {
                match item {
                    MixedItem::Text(text) => text.refs().for_each(|target| push(target, out)),
                    MixedItem::List(items) => {
                        for item in items {
                            occurrences_in(compilation, item, from, key, out, embedded);
                        }
                    }
                    MixedItem::Entry(_, value) | MixedItem::Value(value) => {
                        occurrences_in(compilation, value, from, key, out, embedded)
                    }
                }
            }
        }
        Value::Anchor(id) => {
            if embedded.insert(*id) {
                for (inner_key, item) in compilation.store().anchor(*id).properties.iter() {
                    occurrences_in(compilation, item, *id, inner_key, out, embedded);
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn representable(compilation: &Compilation, anchor: AnchorId) -> Result<(), Unrepresentable> {
    let def = compilation.store().anchor(anchor);
    if def.is_abstract {
        return Err(Unrepresentable::Abstract);
    }
    let source = compilation.anchor_module_path(anchor);
    if source.to_string_lossy().starts_with('@') {
        return Err(Unrepresentable::Package);
    }
    if !source.starts_with(&compilation.project.root) {
        return Err(Unrepresentable::OutsideProject);
    }
    Ok(())
}

/// Where a module's reference document goes for one target.
///
/// A module under `shapeRoot` keeps its position relative to it beneath the
/// compiled shape root, so the shape tree mirrors the architecture. Any other
/// module keeps its position relative to the source root (or, outside it, the
/// project root) beneath the reference root.
pub fn document_path(
    source: &Path,
    source_root: &Path,
    project_root: &Path,
    shape_root: Option<&Path>,
    reference_root: &str,
    compiled_shape_root: &str,
) -> PathBuf {
    let (base, relative) = match shape_root.and_then(|root| source.strip_prefix(root).ok()) {
        Some(relative) => (PathBuf::from(compiled_shape_root), relative),
        None => (
            PathBuf::from(reference_root),
            source
                .strip_prefix(source_root)
                .or_else(|_| source.strip_prefix(project_root))
                .unwrap_or(source),
        ),
    };
    module::normalize(&base.join(relative).with_extension("md"))
}

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
                        let base = piton_emit::heading_slug(&title);
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
        let file = relative_file(&self.from_directory, location);
        let fragment = match self.index {
            // A construct's own output, like a SKILL.md, is not a reference
            // document with headings to land on, so the link is to the file.
            Some(index) => match index.get(location) {
                Some(headings) => headings.fragment(target, self.anchors),
                None => return Some(self.record(file)),
            },
            None => None,
        }
        .unwrap_or_else(|| piton_emit::reference_fragment(self.anchors, target));
        Some(self.record(format!("{file}#{fragment}")))
    }
}

/// A relative path from a directory to a file, starting with `./` or `../`
/// so it reads as a path: `./Button.md`, `../types/Lists.md`.
///
/// Both paths are project-relative and compared as written, never through the
/// filesystem.
pub fn relative_file(from_directory: &Path, target: &Path) -> String {
    let link = piton_emit::relative_link(from_directory, target);
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
    fn document_paths_mirror_the_module() {
        let path = document_path(
            Path::new("/p/spec/scope/a/X.pi"),
            Path::new("/p/spec"),
            Path::new("/p"),
            None,
            ".claude/reference",
            ".claude/reference/shape",
        );
        assert_eq!(path, PathBuf::from(".claude/reference/scope/a/X.md"));
        let shape = document_path(
            Path::new("/p/arch/components/Button.pi"),
            Path::new("/p/spec"),
            Path::new("/p"),
            Some(Path::new("/p/arch")),
            ".claude/reference",
            ".claude/reference/shape",
        );
        assert_eq!(shape, PathBuf::from(".claude/reference/shape/components/Button.md"));
    }

    #[test]
    fn relative_files_read_as_paths() {
        assert_eq!(relative_file(Path::new("a/b"), Path::new("a/b/C.md")), "./C.md");
        assert_eq!(relative_file(Path::new("a/b"), Path::new("a/d/C.md")), "../d/C.md");
        assert_eq!(relative_file(Path::new("a"), Path::new("a/b/C.md")), "./b/C.md");
    }
}
