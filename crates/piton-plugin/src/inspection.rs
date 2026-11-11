//! The findings of the inspect hook of a plugin, and the edits that it
//! proposes.
//!
//! A plugin never changes a source file itself. Its inspect hook returns
//! findings. Each finding has the edits that can fix it. The host and the
//! user decide if they apply the edits. This module checks each edit. The
//! path must stay in the project. The path must also agree with the edits
//! that the capabilities of the plugin list.

use std::path::{Path, PathBuf};

use serde_json::Value as Json;

use crate::manifest::Manifest;
use crate::output::is_safe;

/// One file that a finding changes. It holds the full contents before and
/// after the change.
#[derive(Debug, Clone, PartialEq)]
pub struct Edit {
    /// Relative to the project root, with forward slashes.
    pub path: String,
    /// The current contents, or None when the file does not exist.
    pub before: Option<Vec<u8>>,
    pub after: Vec<u8>,
}

/// A problem that is out of date, and the edits that fix it.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub message: String,
    pub edits: Vec<Edit>,
}

/// Reads the edits that a plugin proposed, `[{path, contents}]`, and checks
/// each edit. This function removes an edit that changes nothing.
pub fn read_edits(manifest: &Manifest, edits: &Json, root: &Path) -> Result<Vec<Edit>, String> {
    let globs = manifest.capabilities.edit_globs();
    let mut out = Vec::new();
    for edit in edits.as_array().into_iter().flatten() {
        let path = edit
            .get("path")
            .and_then(Json::as_str)
            .ok_or("every edit needs a path")?
            .trim_start_matches("./")
            .to_string();
        let contents = edit
            .get("contents")
            .and_then(Json::as_str)
            .ok_or_else(|| format!("the edit of `{path}` has no contents"))?;
        // This code makes an absolute path in the project relative to the
        // project.
        let path = match Path::new(&path).strip_prefix(root) {
            Ok(relative) if Path::new(&path).is_absolute() => crate::output::slash(relative),
            _ => path,
        };
        if !is_safe(&path) {
            return Err(format!("plugin `{}` proposes to edit `{path}`, which is outside the project", manifest.name));
        }
        if !globs.is_match(&path) {
            return Err(format!(
                "plugin `{}` proposes to edit `{path}`, which is not among the edits its declaration lists under capabilities.edits",
                manifest.name
            ));
        }
        let before = std::fs::read(root.join(&path)).ok();
        if before.as_deref() == Some(contents.as_bytes()) {
            continue;
        }
        out.push(Edit { path, before, after: contents.as_bytes().to_vec() });
    }
    Ok(out)
}

/// Reads the result of an inspect hook, `{findings: [{message, edits}]}`.
pub fn read(manifest: &Manifest, inspection: &Json, root: &Path) -> Result<Vec<Finding>, String> {
    let mut findings = Vec::new();
    for finding in inspection.get("findings").and_then(Json::as_array).into_iter().flatten() {
        let message = finding
            .get("message")
            .and_then(Json::as_str)
            .ok_or("every finding needs a message")?
            .to_string();
        let edits = read_edits(manifest, finding.get("edits").unwrap_or(&Json::Null), root)?;
        findings.push(Finding { message, edits });
    }
    Ok(findings)
}

/// Writes edits. It writes each full file through a staging file next to it.
/// Thus a write that stops never leaves half a file. Returns the paths that
/// it changed.
pub fn apply(edits: &[Edit], root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let plan = crate::output::BuildPlan {
        files: edits
            .iter()
            .map(|edit| crate::output::OutputFile {
                path: edit.path.clone(),
                data: edit.after.clone(),
                executable: false,
                origin: String::new(),
                owner: String::new(),
                label: String::new(),
                group: None,
            })
            .collect(),
        targets: Vec::new(),
    };
    crate::output::write(&plan, root)
}
