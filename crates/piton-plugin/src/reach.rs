//! The anchors that plugins reach themselves.
//!
//! The specbase does not reach some anchors, but Piton uses them. An example
//! is a test. The test plugin runs the test because the test has an export. No
//! import or reference goes to the test. A plugin can say `reach: true` in its
//! declaration. Its reach hook then names these anchors. `piton reach` and the
//! editor count them as roots, together with the entry. Thus they are not
//! unreachable and not unused.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use piton_compile::Project;
use serde_json::Value as Json;

use crate::host::Services;
use crate::manifest::Manifest;

/// One anchor that a plugin reaches. It holds the file that declares the
/// anchor, and the name of the anchor.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Root {
    /// Absolute.
    pub file: PathBuf,
    pub name: String,
    /// The plugin that reaches it.
    pub plugin: String,
}

/// Reads the result of a reach hook, `{anchors: [{file, name}]}`. This
/// function reads a relative file from the project root.
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

/// Runs the reach hook of each plugin that declares one. It collects the
/// anchors that the hooks reach, and sorts them. The returned messages name
/// each plugin that fails. The other plugins still count.
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
