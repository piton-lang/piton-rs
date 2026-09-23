//! Output adapters: JSON, YAML, and Markdown.
//!
//! An adapter turns resolved values into one concrete representation. The
//! Markdown adapter carries Belay's content rules; JSON and YAML are structural.

pub mod json;
pub mod markdown;
pub mod yaml;

use std::path::Path;
use std::str::FromStr;

use piton_core::{title_case, AnchorId, AnchorView, Properties, Ref};

/// The output formats `piton compile` can select.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adapter {
    Json,
    Yaml,
    Markdown,
}

impl Adapter {
    pub fn extension(self) -> &'static str {
        match self {
            Adapter::Json => "json",
            Adapter::Yaml => "yaml",
            Adapter::Markdown => "md",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Adapter::Json => "json",
            Adapter::Yaml => "yaml",
            Adapter::Markdown => "markdown",
        }
    }

    pub const ALL: [Adapter; 3] = [Adapter::Json, Adapter::Yaml, Adapter::Markdown];
}

impl FromStr for Adapter {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.to_ascii_lowercase().as_str() {
            "json" => Ok(Adapter::Json),
            "yaml" | "yml" => Ok(Adapter::Yaml),
            "markdown" | "md" => Ok(Adapter::Markdown),
            other => Err(format!(
                "unknown adapter `{other}`; valid options are json, yaml, markdown"
            )),
        }
    }
}

impl std::fmt::Display for Adapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Renders a file's top-level declarations with the selected adapter.
///
/// `context.from_directory` is where the output is written, so references
/// can point at the other files relative to it.
pub fn render(
    adapter: Adapter,
    declarations: &Properties,
    anchors: &dyn AnchorView,
    context: MarkdownContext<'_>,
) -> String {
    render_mapped(adapter, declarations, anchors, context, None)
}

/// Renders like [`render`], for output written into its own tree: a source
/// file at `source_root/a/B.pi` is written to `output_root/a/B.<ext>`, so a
/// reference to it points there.
pub fn render_mapped(
    adapter: Adapter,
    declarations: &Properties,
    anchors: &dyn AnchorView,
    context: MarkdownContext<'_>,
    output_root: Option<&Path>,
) -> String {
    let located = Located {
        anchors,
        directory: context.from_directory,
        source_root: context.source_root,
        output_root,
    };
    match adapter {
        Adapter::Json => json::declarations(declarations, &located),
        Adapter::Yaml => yaml::declarations(declarations, &located),
        Adapter::Markdown => {
            let links = markdown::MirroredLinks {
                from_directory: context.from_directory,
                source_root: context.source_root,
                anchors: &located,
            };
            let context = markdown::Context {
                anchors: &located,
                links: &links,
            };
            markdown::sections(declarations, 1, &context)
        }
    }
}

/// Where a document is being written, so references can be made relative.
#[derive(Clone, Copy)]
pub struct MarkdownContext<'a> {
    pub from_directory: &'a Path,
    pub source_root: &'a Path,
}

/// The same anchors, seen from a file being written in `directory`.
pub struct Located<'a> {
    pub anchors: &'a dyn AnchorView,
    pub directory: &'a Path,
    pub source_root: &'a Path,
    /// Where the output tree starts, when it isn't next to the sources.
    pub output_root: Option<&'a Path>,
}

impl AnchorView for Located<'_> {
    fn name(&self, id: AnchorId) -> &str {
        self.anchors.name(id)
    }
    fn properties(&self, id: AnchorId) -> &Properties {
        self.anchors.properties(id)
    }
    fn source_path(&self, id: AnchorId) -> &Path {
        self.anchors.source_path(id)
    }
    fn is_abstract(&self, id: AnchorId) -> bool {
        self.anchors.is_abstract(id)
    }
    fn output_directory(&self) -> Option<&Path> {
        Some(self.directory)
    }
    fn output_path(&self, source: &Path) -> std::path::PathBuf {
        match self.output_root {
            Some(root) => match source.strip_prefix(self.source_root) {
                Ok(relative) => root.join(relative),
                Err(_) => source.to_path_buf(),
            },
            None => source.to_path_buf(),
        }
    }
}

/// Where the file holding a reference's target is, relative to the file being
/// written, with the output's extension: `./Button.json`.
pub fn reference_file(anchors: &dyn AnchorView, target: &Ref, extension: &str) -> String {
    let source = anchors
        .output_path(anchors.source_path(target.anchor))
        .with_extension(extension);
    match anchors.output_directory() {
        Some(directory) => relative_file(directory, &source),
        None => format!(
            "./{}",
            source
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default()
        ),
    }
}

/// How JSON and YAML write a reference: the path to the other file, a colon,
/// and the dot path to the value. So `../file.json:Anchor.property`.
pub fn reference_location(anchors: &dyn AnchorView, target: &Ref, extension: &str) -> String {
    format!(
        "{}:{}",
        reference_file(anchors, target, extension),
        target.display(anchors)
    )
}

/// The fragment a Markdown heading gets, the way GitHub builds it: lowercase,
/// spaces become hyphens, and punctuation is dropped.
pub fn heading_slug(title: &str) -> String {
    let mut out = String::new();
    for ch in title.trim().chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            out.extend(ch.to_lowercase());
        } else if ch == ' ' {
            out.push('-');
        }
    }
    out
}

/// The heading fragment a reference links to: the property's heading, or the
/// anchor's own.
pub fn reference_fragment(anchors: &dyn AnchorView, target: &Ref) -> String {
    let name = target
        .path
        .last()
        .map(String::as_str)
        .unwrap_or_else(|| anchors.name(target.anchor));
    heading_slug(&title_case(name))
}

/// A relative path from a directory to a file, always starting with `./` or
/// `../` so it reads as a path.
pub fn relative_file(from_directory: &Path, target: &Path) -> String {
    let canonical = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let from = canonical(from_directory);
    let target = match (target.parent(), target.file_name()) {
        (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => canonical(parent).join(name),
        _ => target.to_path_buf(),
    };
    let link = relative_link(&from, &target);
    if link.starts_with("../") {
        link
    } else {
        format!("./{link}")
    }
}

/// Builds a relative Markdown link from one directory to a target path.
///
/// Links always carry an explicit `./` or `../` prefix only when they need one;
/// a sibling file is written as a bare name, which is how the reference tree
/// reads.
pub fn relative_link(from_directory: &Path, target: &Path) -> String {
    let from: Vec<_> = from_directory
        .components()
        .filter(|c| !matches!(c, std::path::Component::CurDir))
        .collect();
    let to: Vec<_> = target
        .components()
        .filter(|c| !matches!(c, std::path::Component::CurDir))
        .collect();
    let shared = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = Vec::new();
    for _ in shared..from.len() {
        parts.push("..".to_string());
    }
    for component in &to[shared..] {
        parts.push(component.as_os_str().to_string_lossy().to_string());
    }
    if parts.is_empty() {
        target
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    } else {
        parts.join("/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapters_parse_from_their_names() {
        assert_eq!(Adapter::from_str("json").unwrap(), Adapter::Json);
        assert_eq!(Adapter::from_str("MD").unwrap(), Adapter::Markdown);
        assert!(Adapter::from_str("toml").is_err());
    }

    #[test]
    fn headings_slug_like_github() {
        assert_eq!(heading_slug("My Property"), "my-property");
        assert_eq!(heading_slug("Button"), "button");
    }

    #[test]
    fn relative_files_start_with_a_dot() {
        assert_eq!(relative_file(Path::new("/p/a"), Path::new("/p/a/B.json")), "./B.json");
        assert_eq!(relative_file(Path::new("/p/a"), Path::new("/p/c/B.json")), "../c/B.json");
    }

    #[test]
    fn sibling_links_are_bare_names() {
        assert_eq!(
            relative_link(Path::new("scope/language/types"), Path::new("scope/language/types/Escaping.md")),
            "Escaping.md"
        );
    }

    #[test]
    fn links_walk_up_and_across() {
        assert_eq!(
            relative_link(
                Path::new("scope/language/types"),
                Path::new("scope/language/variables/TypeCoercion.md")
            ),
            "../variables/TypeCoercion.md"
        );
        assert_eq!(
            relative_link(
                Path::new("scope/language"),
                Path::new("scope/language/overview/Overview.md")
            ),
            "overview/Overview.md"
        );
    }
}
