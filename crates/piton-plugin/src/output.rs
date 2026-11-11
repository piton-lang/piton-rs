//! Writing what a build produces.
//!
//! Nothing that takes part in a build writes files itself. The renderer and
//! every plugin with a build return what they would write, and this module
//! checks all of it together, writes it, records what it wrote, and removes
//! what an earlier build wrote and this one doesn't. So every producer gets
//! the same guarantees: paths stay inside the project and inside what each
//! producer may write, two producers never fight over a file, a file the
//! build didn't write is never overwritten or deleted, and a build that stops
//! never leaves half a file behind.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use globset::GlobSet;

/// Where the build records the files it owns, relative to the project root.
pub const MANIFEST: &str = ".piton/manifest.json";

/// One file a build writes.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputFile {
    /// Relative to the project root, with forward slashes.
    pub path: String,
    pub data: Vec<u8>,
    /// Whether to set the execute permission, for a copied script.
    pub executable: bool,
    /// What it was made from, for diagnostics: an anchor or a source file.
    pub origin: String,
    /// Who produced it: `renderer`, or a plugin's name.
    pub owner: String,
    /// A short description for `--dry-run`, like `skill, claude-code`.
    pub label: String,
    /// A name the editor's preview selects outputs by, like a Belay target
    /// id; None when the producer gives none.
    pub group: Option<String>,
}

/// Everything one build writes.
#[derive(Debug, Clone, Default)]
pub struct BuildPlan {
    pub files: Vec<OutputFile>,
    /// Labels recorded in the manifest, like each target's documentation
    /// date, in the order producers gave them.
    pub targets: Vec<(String, String)>,
}

/// A path as the manifest and the checks spell it.
pub fn slash(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Whether a path a producer gave stays inside the project: relative, with no
/// `..`, and not empty.
pub fn is_safe(path: &str) -> bool {
    let candidate = Path::new(path);
    !path.is_empty()
        && !candidate.is_absolute()
        && candidate
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

/// Finds what is wrong with a plan before anything is written. `outputs`
/// gives each producer's allowed paths; a producer it gives none for may
/// write anywhere in the project, which is only the renderer and the
/// compiler's own producers. `previous` is what the last build's manifest
/// recorded.
pub fn check(
    plan: &BuildPlan,
    root: &Path,
    previous: &BTreeSet<String>,
    outputs: &dyn Fn(&str) -> Option<GlobSet>,
) -> Vec<String> {
    let mut problems = Vec::new();
    let mut seen: BTreeMap<&str, &OutputFile> = BTreeMap::new();
    let mut allowed: BTreeMap<&str, Option<GlobSet>> = BTreeMap::new();
    for file in &plan.files {
        if !is_safe(&file.path) {
            problems.push(format!(
                "{} plans `{}`, which is outside the project",
                producer(&file.owner),
                file.path
            ));
            continue;
        }
        let globs = allowed.entry(&file.owner).or_insert_with(|| outputs(&file.owner));
        if let Some(globs) = globs {
            if !globs.is_match(&file.path) {
                problems.push(format!(
                    "{} plans `{}`, which is not among the outputs its declaration lists under capabilities.outputs",
                    producer(&file.owner),
                    file.path
                ));
                continue;
            }
        }
        match seen.get(file.path.as_str()) {
            Some(earlier) if earlier.owner != file.owner => problems.push(format!(
                "{} and {} both plan `{}`",
                producer(&earlier.owner),
                producer(&file.owner),
                file.path
            )),
            Some(earlier) if earlier.data != file.data => problems.push(format!(
                "{} plans `{}` twice, with different contents",
                producer(&file.owner),
                file.path
            )),
            Some(_) => {}
            None => {
                seen.insert(&file.path, file);
                if root.join(&file.path).exists() && !previous.contains(&file.path) {
                    problems.push(format!(
                        "{} plans `{}`, which exists and wasn't written by a build; move it, or delete it if it is not needed",
                        producer(&file.owner),
                        file.path
                    ));
                }
            }
        }
    }
    problems
}

fn producer(owner: &str) -> String {
    if owner == "renderer" {
        "the renderer".to_string()
    } else {
        format!("plugin `{owner}`")
    }
}

/// The files to write, with a path planned twice with the same contents kept
/// once.
fn unique(plan: &BuildPlan) -> Vec<&OutputFile> {
    let mut seen = BTreeSet::new();
    plan.files
        .iter()
        .filter(|file| seen.insert(file.path.as_str()))
        .collect()
}

/// Every path the plan writes, sorted and once each.
pub fn paths(plan: &BuildPlan) -> Vec<String> {
    plan.files
        .iter()
        .map(|file| file.path.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Writes a checked plan, returning the paths written. A file whose contents
/// haven't changed is left alone, so its timestamp stays meaningful. Each
/// file is written through a staging file beside it, then renamed into
/// place, so a build that stops never leaves a half-written file.
pub fn write(plan: &BuildPlan, root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    for file in unique(plan) {
        let destination = root.join(&file.path);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let unchanged = std::fs::read(&destination)
            .map(|existing| existing == file.data)
            .unwrap_or(false);
        if !unchanged {
            let staging = destination.with_file_name(format!(
                ".{}.piton-staging",
                destination
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default()
            ));
            std::fs::write(&staging, &file.data)?;
            if let Err(error) = std::fs::rename(&staging, &destination) {
                let _ = std::fs::remove_file(&staging);
                return Err(error);
            }
        }
        if file.executable {
            make_executable(&destination)?;
        }
        written.push(PathBuf::from(&file.path));
    }
    Ok(written)
}

/// Sets the execute bits a copied script had.
#[cfg(unix)]
fn make_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)?.permissions();
    let mode = permissions.mode();
    if mode & 0o111 != 0o111 {
        permissions.set_mode(mode | ((mode & 0o444) >> 2));
        std::fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Removes what the previous build wrote and this one doesn't, and the
/// directories that leaves empty. Only paths the manifest records are
/// considered, so nothing the build didn't write is ever deleted.
pub fn clean_stale(previous: &BTreeSet<String>, plan: &BuildPlan, root: &Path) -> Vec<PathBuf> {
    let current: BTreeSet<String> = paths(plan).into_iter().collect();
    let mut removed = Vec::new();
    for path in previous {
        if current.contains(path) || !is_safe(path) {
            continue;
        }
        let target = root.join(path);
        if target.is_file() && std::fs::remove_file(&target).is_ok() {
            prune_empty_parents(root, Path::new(path));
            removed.push(PathBuf::from(path));
        }
    }
    removed
}

fn prune_empty_parents(root: &Path, relative: &Path) {
    let mut current = relative.parent();
    while let Some(directory) = current {
        if directory.as_os_str().is_empty() {
            break;
        }
        if std::fs::remove_dir(root.join(directory)).is_err() {
            break;
        }
        current = directory.parent();
    }
}

/// The manifest for a plan: `{"generated": [paths], "targets": {id: label}}`,
/// paths in order.
pub fn manifest(plan: &BuildPlan) -> String {
    let quote = |text: &str| serde_json::Value::String(text.to_string()).to_string();
    let paths = paths(plan);
    let mut out = String::from("{\n  \"generated\": [\n");
    for (index, path) in paths.iter().enumerate() {
        out.push_str("    ");
        out.push_str(&quote(path));
        if index + 1 < paths.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]");
    if !plan.targets.is_empty() {
        out.push_str(",\n  \"targets\": {\n");
        for (index, (id, label)) in plan.targets.iter().enumerate() {
            out.push_str("    ");
            out.push_str(&quote(id));
            out.push_str(": ");
            out.push_str(&quote(label));
            if index + 1 < plan.targets.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("  }");
    }
    out.push_str("\n}\n");
    out
}

/// The paths a manifest document records.
pub fn manifest_paths(document: &str) -> BTreeSet<String> {
    serde_json::from_str::<serde_json::Value>(document)
        .ok()
        .and_then(|value| value.get("generated").cloned())
        .and_then(|generated| generated.as_array().cloned())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// What the last build recorded, or nothing when there is no manifest.
pub fn previous(root: &Path) -> BTreeSet<String> {
    std::fs::read_to_string(root.join(MANIFEST))
        .map(|text| manifest_paths(&text))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, owner: &str, data: &str) -> OutputFile {
        OutputFile {
            path: path.into(),
            data: data.as_bytes().to_vec(),
            executable: false,
            origin: "X".into(),
            owner: owner.into(),
            label: String::new(),
            group: None,
        }
    }

    fn globs(patterns: &[&str]) -> GlobSet {
        let mut builder = globset::GlobSetBuilder::new();
        for pattern in patterns {
            builder.add(globset::GlobBuilder::new(pattern).literal_separator(true).build().unwrap());
        }
        builder.build().unwrap()
    }

    #[test]
    fn a_plan_is_checked_whole() {
        let root = Path::new("/nonexistent-piton-output-test");
        let plan = BuildPlan {
            files: vec![
                file("../escape.md", "docs", "a"),
                file("notes/x.md", "docs", "a"),
                file("docs/a.md", "docs", "a"),
                file("docs/a.md", "docs", "a"),
                file("docs/b.md", "docs", "1"),
                file("docs/b.md", "docs", "2"),
                file("docs/c.md", "other", "c"),
                file("docs/c.md", "docs", "c"),
            ],
            targets: Vec::new(),
        };
        let outputs = |owner: &str| (owner == "docs").then(|| globs(&["docs/**"]));
        let problems = check(&plan, root, &BTreeSet::new(), &outputs);
        assert_eq!(problems.len(), 4, "{problems:#?}");
        assert!(problems[0].contains("outside the project"));
        assert!(problems[1].contains("not among the outputs"));
        assert!(problems[2].contains("twice, with different contents"));
        assert!(problems[3].contains("plugin `other` and plugin `docs` both plan"));
    }

    #[test]
    fn manifests_round_trip_in_belays_format() {
        let plan = BuildPlan {
            files: vec![file("b.md", "x", ""), file("a.md", "x", ""), file("a.md", "x", "")],
            targets: vec![("claude-code".into(), "2026-09-21".into())],
        };
        let document = manifest(&plan);
        assert_eq!(
            document,
            "{\n  \"generated\": [\n    \"a.md\",\n    \"b.md\"\n  ],\n  \"targets\": {\n    \"claude-code\": \"2026-09-21\"\n  }\n}\n"
        );
        assert_eq!(manifest_paths(&document).into_iter().collect::<Vec<_>>(), ["a.md", "b.md"]);
    }

    #[test]
    fn writing_is_whole_and_cleanup_only_takes_what_was_written() {
        let root = std::env::temp_dir().join(format!("piton-output-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("keep")).unwrap();
        std::fs::write(root.join("keep/mine.md"), "user").unwrap();

        let first = BuildPlan {
            files: vec![file("out/a.md", "x", "A"), file("out/deep/b.md", "x", "B")],
            targets: Vec::new(),
        };
        write(&first, &root).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("out/deep/b.md")).unwrap(), "B");

        let previous: BTreeSet<String> =
            ["out/a.md", "out/deep/b.md", "keep/mine.md", "../outside.md"].iter().map(|p| p.to_string()).collect();
        let second = BuildPlan { files: vec![file("out/a.md", "x", "A2")], targets: Vec::new() };
        write(&second, &root).unwrap();
        let mut removed = clean_stale(&previous, &second, &root);
        removed.sort();
        // The manifest says the build owns keep/mine.md, so it goes; a path
        // that would leave the project is never touched.
        assert_eq!(removed, [PathBuf::from("keep/mine.md"), PathBuf::from("out/deep/b.md")]);
        assert!(!root.join("out/deep").exists(), "empty directories are pruned");
        assert_eq!(std::fs::read_to_string(root.join("out/a.md")).unwrap(), "A2");
        let _ = std::fs::remove_dir_all(&root);
    }
}
