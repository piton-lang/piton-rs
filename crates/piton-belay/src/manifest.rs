//! The build manifest: which files the build generated.
//!
//! The manifest is what makes output ownership checkable. A file at a planned
//! path that the previous build did not record was written by someone else,
//! so it is neither overwritten nor, later, cleaned up.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use crate::Plan;

/// Where the manifest lives, relative to the project root: a `.piton`
/// directory beside `piton.config.pi`.
pub const MANIFEST: &str = ".piton/manifest.json";

/// The manifest document for a plan: `{"generated": [paths], "targets":
/// {targetId: documentationChecked}}`.
pub fn document(plan: &Plan) -> String {
    document_with(&plan.manifest(), &plan.targets)
}

pub(crate) fn document_with(paths: &[String], targets: &[(String, String)]) -> String {
    let mut out = String::from("{\n  \"generated\": [\n");
    for (index, path) in paths.iter().enumerate() {
        out.push_str("    ");
        out.push_str(&piton_emit::json::quote(path));
        if index + 1 < paths.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]");
    if !targets.is_empty() {
        out.push_str(",\n  \"targets\": {\n");
        for (index, (id, checked)) in targets.iter().enumerate() {
            out.push_str("    ");
            out.push_str(&piton_emit::json::quote(id));
            out.push_str(": ");
            out.push_str(&piton_emit::json::quote(checked));
            if index + 1 < targets.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("  }");
    }
    out.push_str("\n}\n");
    out
}

/// Reads the paths a manifest's `generated` array records.
pub fn paths(document: &str) -> Vec<String> {
    string_array(document, "generated")
}

/// Reads the strings of the first JSON array stored under `key`.
pub(crate) fn string_array(document: &str, key: &str) -> Vec<String> {
    let quoted = format!("\"{key}\"");
    let Some(start) = document.find(&quoted) else {
        return Vec::new();
    };
    let rest = &document[start + quoted.len()..];
    if !rest.trim_start().starts_with(':') {
        return Vec::new();
    }
    let Some(open) = rest.find('[') else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut chars = rest[open + 1..].chars();
    while let Some(ch) = chars.next() {
        match ch {
            ']' => break,
            '"' => {
                let mut value = String::new();
                while let Some(ch) = chars.next() {
                    match ch {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => value.push('\n'),
                            Some('t') => value.push('\t'),
                            Some('r') => value.push('\r'),
                            Some('u') => {
                                let code: String = chars.by_ref().take(4).collect();
                                if let Some(c) =
                                    u32::from_str_radix(&code, 16).ok().and_then(char::from_u32)
                                {
                                    value.push(c);
                                }
                            }
                            Some(other) => value.push(other),
                            None => break,
                        },
                        other => value.push(other),
                    }
                }
                out.push(value);
            }
            _ => {}
        }
    }
    out
}

/// The paths the manifest at `root` records.
pub fn previous(root: &Path) -> BTreeSet<String> {
    std::fs::read_to_string(root.join(MANIFEST))
        .map(|text| paths(&text).into_iter().collect())
        .unwrap_or_default()
}

/// True for a recorded path that stays inside the project and outside the
/// directories Belay never touches.
pub(crate) fn is_safe(path: &str) -> bool {
    let path = Path::new(path);
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        && !path.starts_with(".claude/worktrees")
        && !path.starts_with(".git")
}

/// Removes directories left empty by removing `path`, up to (not including)
/// the project root.
pub(crate) fn prune_empty_parents(root: &Path, path: &Path) {
    let mut current: Option<PathBuf> = path.parent().map(Path::to_path_buf);
    while let Some(directory) = current {
        if directory.as_os_str().is_empty() {
            break;
        }
        let absolute = root.join(&directory);
        let empty = std::fs::read_dir(&absolute)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
        if !empty || std::fs::remove_dir(&absolute).is_err() {
            break;
        }
        current = directory.parent().map(Path::to_path_buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_generated_array_is_read() {
        let text = "{\n  \"generated\": [\n    \"a.md\",\n    \"b \\\"c\\\".md\"\n  ],\n  \"targets\": {\n    \"codex\": {\n      \"adapter\": \"CodexAdapter\",\n      \"documentationChecked\": \"2026-09-21\"\n    }\n  }\n}\n";
        assert_eq!(paths(text), vec!["a.md".to_string(), "b \"c\".md".to_string()]);
    }

    #[test]
    fn unsafe_paths_are_refused() {
        assert!(is_safe(".claude/skills/x/SKILL.md"));
        assert!(!is_safe("../outside.md"));
        assert!(!is_safe("/etc/passwd"));
        assert!(!is_safe(".claude/worktrees/agent/README.md"));
    }
}
