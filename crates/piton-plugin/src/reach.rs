//! The anchors plugins reach themselves.
//!
//! Some anchors are used without anything in the specbase reaching them: a
//! test is run by the test plugin because it is exported, though no import or
//! reference leads to it. A plugin whose declaration says `reach: true` names
//! such anchors from its reach hook, and `piton reach` and the editor count
//! them as roots beside the entry, so they are neither unreachable nor unused.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use piton_compile::Project;
use serde_json::Value as Json;

use crate::host::Services;
use crate::manifest::Manifest;

/// One anchor a plugin reaches: the file that declares it and its name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Root {
    /// Absolute.
    pub file: PathBuf,
    pub name: String,
    /// The plugin that reaches it.
    pub plugin: String,
}

/// Reads what a reach hook returned, `{anchors: [{file, name}]}`. A relative
/// file is read from the project root.
pub fn read(manifest: &Manifest, reached: &Json, root: &Path) -> Result<Vec<Root>, String> {
    let mut out = Vec::new();
    for anchor in reached.get("anchors").and_then(Json::as_array).into_iter().flatten() {
        let name = anchor
            .get("name")
            .and_then(Json::as_str)
            .ok_or("every reached anchor needs a name")?
            .to_string();
        let file = anchor
            .get("file")
            .and_then(Json::as_str)
            .ok_or_else(|| format!("the reached anchor `{name}` has no file"))?;
        let file = Path::new(file);
        let file = if file.is_absolute() { file.to_path_buf() } else { root.join(file) };
        out.push(Root { file, name, plugin: manifest.name.clone() });
    }
    Ok(out)
}

/// Runs the reach hook of every plugin that declares one, and gathers the
/// anchors they reach, sorted. A plugin that fails is named in the messages
/// returned, and the others still count.
pub fn roots(
    project: &Project,
    manifests: &[Manifest],
    services: impl Fn() -> Rc<dyn Services>,
) -> (Vec<Root>, Vec<String>) {
    let mut out = Vec::new();
    let mut failures = Vec::new();
    if project.config_path.is_none() {
        return (out, failures);
    }
    for manifest in manifests.iter().filter(|manifest| manifest.reach) {
        let result = crate::host::run_reach(manifests, &manifest.name, services())
            .map_err(|failure| failure.to_string())
            .and_then(|reached| read(manifest, &reached, &project.root));
        match result {
            Ok(found) => out.extend(found),
            Err(message) => {
                failures.push(format!("plugin `{}` could not say what it reaches: {message}", manifest.name))
            }
        }
    }
    out.sort();
    out.dedup();
    (out, failures)
}
