//! This module reads the plan that the build of a plugin returns.
//!
//! The plan is plain data from the code of the plugin. Thus this module
//! checks all of the plan before the host writes it. Files and copies become
//! output files that the plugin owns. The host reads a copy only if the
//! plugin can read files and the source is in the project. Diagnostics keep
//! their source location.

use std::path::{Path, PathBuf};

use piton_core::{Diagnostic, Span};
use serde_json::Value as Json;

use crate::manifest::Manifest;
use crate::output::{is_safe, OutputFile};

/// The plan that the build of a plugin made.
#[derive(Debug, Default)]
pub struct PluginPlan {
    pub files: Vec<OutputFile>,
    pub diagnostics: Vec<Diagnostic>,
    pub targets: Vec<(String, String)>,
    /// The location of each anchor, by name. Commands that cite output use
    /// it.
    pub locations: Vec<(String, String)>,
}

fn text<'a>(value: &'a Json, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Json::as_str)
}

/// Reads a plan. A malformed plan is an error that names the problem. The
/// host then uses no part of the plan.
pub fn read(manifest: &Manifest, plan: &Json, root: &Path) -> Result<PluginPlan, String> {
    let owner = &manifest.name;
    let mut out = PluginPlan::default();
    let label = |item: &Json| text(item, "label").unwrap_or("").to_string();

    for file in plan.get("files").and_then(Json::as_array).into_iter().flatten() {
        let path = text(file, "path").ok_or("every planned file needs a path")?;
        let contents = text(file, "contents").ok_or_else(|| format!("the planned file `{path}` has no contents"))?;
        out.files.push(OutputFile {
            path: path.to_string(),
            data: contents.as_bytes().to_vec(),
            executable: false,
            origin: text(file, "origin").unwrap_or(path).to_string(),
            owner: owner.clone(),
            label: label(file),
            group: text(file, "group").map(str::to_string),
        });
    }

    for copy in plan.get("copies").and_then(Json::as_array).into_iter().flatten() {
        let from = text(copy, "from").ok_or("every planned copy needs a from")?;
        let to = text(copy, "to").ok_or("every planned copy needs a to")?;
        if !manifest.capabilities.read_files {
            return Err(format!(
                "plugin `{owner}` copies `{from}`, but its declaration does not list files: read under capabilities"
            ));
        }
        if !is_safe(from) {
            return Err(format!("plugin `{owner}` copies `{from}`, which is outside the project"));
        }
        let source = root.join(from);
        let metadata = std::fs::symlink_metadata(&source)
            .map_err(|error| format!("plugin `{owner}` copies `{from}`, which can't be read: {error}"))?;
        if !metadata.is_file() {
            return Err(format!("plugin `{owner}` copies `{from}`, which is not a file"));
        }
        #[cfg(unix)]
        let executable = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o111 != 0
        };
        #[cfg(not(unix))]
        let executable = false;
        let data = std::fs::read(&source)
            .map_err(|error| format!("plugin `{owner}` copies `{from}`, which can't be read: {error}"))?;
        out.files.push(OutputFile {
            path: to.to_string(),
            data,
            executable,
            origin: text(copy, "origin").unwrap_or(from).to_string(),
            owner: owner.clone(),
            label: label(copy),
            group: text(copy, "group").map(str::to_string),
        });
    }

    for diagnostic in plan.get("diagnostics").and_then(Json::as_array).into_iter().flatten() {
        let message = text(diagnostic, "message").ok_or("every planned diagnostic needs a message")?;
        let code = text(diagnostic, "code").unwrap_or(owner).to_string();
        let (file, span) = match diagnostic.get("location") {
            Some(location) if location.is_object() => (
                PathBuf::from(text(location, "file").unwrap_or("")),
                Span::new(
                    location.get("start").and_then(Json::as_u64).unwrap_or(0) as usize,
                    location.get("end").and_then(Json::as_u64).unwrap_or(0) as usize,
                ),
            ),
            _ => (manifest.declared_in.clone(), Span::default()),
        };
        let mut built = match text(diagnostic, "severity") {
            Some("warning") => Diagnostic::warning(code, message.to_string(), file, span),
            Some("error") | None => Diagnostic::error(code, message.to_string(), file, span),
            Some(other) => return Err(format!("`{other}` is not a severity; it is error or warning")),
        };
        if let Some(help) = text(diagnostic, "help") {
            built = built.with_help(help.to_string());
        }
        out.diagnostics.push(built);
    }

    for (key, map) in [("targets", &mut out.targets), ("locations", &mut out.locations)] {
        if let Some(entries) = plan.get(key).and_then(Json::as_object) {
            for (name, value) in entries {
                let value = value.as_str().ok_or_else(|| format!("{key} maps names to text"))?;
                map.push((name.clone(), value.to_string()));
            }
        }
    }
    Ok(out)
}
