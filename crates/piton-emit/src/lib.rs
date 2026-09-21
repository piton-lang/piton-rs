//! Output adapters: JSON, YAML, and Markdown.
//!
//! An adapter turns resolved values into one concrete representation. The
//! Markdown adapter carries Belay's content rules; JSON and YAML are structural.

pub mod json;
pub mod markdown;
pub mod yaml;

use std::path::Path;
use std::str::FromStr;

use piton_core::{AnchorView, Properties};

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
pub fn render(
    adapter: Adapter,
    declarations: &Properties,
    anchors: &dyn AnchorView,
    context: MarkdownContext<'_>,
) -> String {
    match adapter {
        Adapter::Json => json::declarations(declarations, anchors),
        Adapter::Yaml => yaml::declarations(declarations, anchors),
        Adapter::Markdown => {
            let links = markdown::MirroredLinks {
                from_directory: context.from_directory,
                source_root: context.source_root,
                anchors,
            };
            let context = markdown::Context {
                anchors,
                links: &links,
            };
            markdown::sections(declarations, 1, &context)
        }
    }
}

/// Where a Markdown document is being written, so links can be made relative.
#[derive(Clone, Copy)]
pub struct MarkdownContext<'a> {
    pub from_directory: &'a Path,
    pub source_root: &'a Path,
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
