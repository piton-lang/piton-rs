//! Files a skill or an agent brings from its own source tree.
//!
//! A skill's `files` names a folder copied beside its `SKILL.md`; when the
//! folder holds a `SKILL.md` of its own, that file is the skill, used as it
//! is. An agent's `file` names its native definition, used as it is. Either
//! is one path for every target, or a dictionary from target id to path, so a
//! package can carry a tool's own build of the same skill for each tool.
//!
//! Paths are relative to the `.pi` file that declares the property, or to the
//! source root when they start with `/`, the way imports resolve. A copied
//! file is bytes: nothing in it is rendered, linked or rewritten.

use std::path::{Path, PathBuf};

use piton_compile::{module, Compilation};
use piton_core::{Span, Value};

use crate::adapter::{Adapter, Target};
use crate::construct::{Construct, ConstructKind};
use crate::{error_at, property_site, warning_at, Draft};

/// The property naming a construct's vendored files, if it can have one.
pub(crate) fn property(kind: ConstructKind) -> Option<&'static str> {
    match kind {
        ConstructKind::Skill => Some("files"),
        ConstructKind::Agent => Some("file"),
        _ => None,
    }
}

/// A file copied into the output as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copied {
    /// The file it was copied from, absolute.
    pub source: PathBuf,
    /// Its bytes when they are not UTF-8 text; text is in `contents`.
    pub bytes: Option<Vec<u8>>,
    pub executable: bool,
}

/// One file read from a vendored folder.
pub(crate) struct Entry {
    /// Path inside the folder, with `/` separators.
    pub relative: String,
    pub text: String,
    pub copied: Copied,
}

/// What a construct's vendored files say about one target.
pub(crate) enum Choice {
    /// Nothing vendored: Belay renders the construct.
    Render,
    /// Use these files, absolute.
    Path(PathBuf),
    /// The dictionary names `null` for the target: it gets no output.
    Omit,
    /// The value has a problem, already reported.
    Invalid,
}

/// What a construct names for `target`.
pub(crate) fn choose(
    compilation: &Compilation,
    item: &Construct,
    target: &Target,
    plan: &mut Draft,
) -> Choice {
    match path_for(compilation, item, target, plan) {
        Some(Ok(path)) => Choice::Path(path),
        Some(Err(true)) => Choice::Omit,
        Some(Err(false)) => Choice::Invalid,
        None => Choice::Render,
    }
}

/// The path a construct names for `target`, absolute; `Err(true)` when it
/// names `null` for it, and `Err(false)` when the value has a problem, which
/// is reported. `None` when it names nothing.
fn path_for(
    compilation: &Compilation,
    item: &Construct,
    target: &Target,
    plan: &mut Draft,
) -> Option<Result<PathBuf, bool>> {
    let key = property(item.kind)?;
    let value = item.properties.get(key)?;
    let site = property_site(compilation, item.anchor, key);
    let origin = format!("{}.{key}", item.name);
    let written = match value {
        Value::Null => return None,
        Value::Str(text) if text.is_plain() => text.render_plain(compilation),
        Value::Dict(_) | Value::Mixed(_) => match entries(value)
            .into_iter()
            .find(|(id, _)| id == target.id)
        {
            Some((_, Value::Str(text))) if text.is_plain() => text.render_plain(compilation),
            Some((_, Value::Null)) => return Some(Err(true)),
            Some(_) => {
                plan.diagnostics.push(
                    error_at(
                        "invalid-vendored-path",
                        format!("`{key}.{}` on `{}` must be a path", target.id, item.name),
                        site,
                    )
                    .with_origin(origin),
                );
                return Some(Err(false));
            }
            None => {
                plan.diagnostics.push(
                    error_at(
                        "missing-vendored-target",
                        format!(
                            "`{}.{key}` names a path for some targets but not for {}, which this project builds",
                            item.name, target.id
                        ),
                        site,
                    )
                    .with_origin(origin)
                    .with_help(format!(
                        "add `{}: <path>` under `{key}`, or `{}: null` to leave it out of that target",
                        target.id, target.id
                    )),
                );
                return Some(Err(false));
            }
        },
        _ => {
            plan.diagnostics.push(
                error_at(
                    "invalid-vendored-path",
                    format!(
                        "`{key}` on `{}` must be a path, or a dictionary from target id to path",
                        item.name
                    ),
                    site,
                )
                .with_origin(origin),
            );
            return Some(Err(false));
        }
    };
    let written = written.trim();
    let declaring = declaring_module(compilation, item, key);
    let path = match written.strip_prefix('/') {
        Some(rooted) => compilation.project.source_root.join(rooted),
        None => declaring.parent().unwrap_or(Path::new("")).join(written),
    };
    let path = module::normalize(&path);
    if !path.starts_with(&compilation.project.root) {
        plan.diagnostics.push(
            error_at(
                "vendored-outside-project",
                format!(
                    "`{written}` on `{}` resolves to `{}`, outside the project",
                    item.name,
                    path.display()
                ),
                site,
            )
            .with_origin(origin),
        );
        return Some(Err(false));
    }
    Some(Ok(path))
}

/// Reports target ids in a per-target dictionary that no adapter has. Run once
/// per construct, not once per target.
pub(crate) fn check_targets(compilation: &Compilation, item: &Construct, plan: &mut Draft) {
    let Some(key) = property(item.kind) else {
        return;
    };
    let Some(value) = item.properties.get(key) else {
        return;
    };
    for (id, _) in entries(value) {
        if Adapter::by_target(&id).is_none() {
            let known: Vec<String> = Adapter::all().iter().map(|a| format!("`{}`", a.target_id)).collect();
            plan.diagnostics.push(
                error_at(
                    "unknown-vendored-target",
                    format!("`{key}.{id}` on `{}` names no target", item.name),
                    property_site(compilation, item.anchor, key),
                )
                .with_origin(format!("{}.{key}", item.name))
                .with_help(format!("the targets are {}", known.join(", "))),
            );
        }
    }
}

/// Warns when a per-target path will not be used, because the target reads
/// another target's copy of the skill.
pub(crate) fn report_unused(
    compilation: &Compilation,
    item: &Construct,
    target: &Target,
    peer: &Target,
    plan: &mut Draft,
) {
    let Some(key) = property(item.kind) else {
        return;
    };
    let Some(value) = item.properties.get(key) else {
        return;
    };
    let entries = entries(value);
    let path = |id: &str| entries.iter().find(|(k, _)| k == id).map(|(_, v)| v.clone());
    let (Some(own), Some(theirs)) = (path(target.id), path(peer.id)) else {
        return;
    };
    if own == theirs {
        return;
    }
    plan.diagnostics.push(
        warning_at(
            "unused-vendored-files",
            format!(
                "{} reads {}'s copy of the skill `{}`, so the files `{key}.{}` names are not written",
                target.anchor_name, peer.anchor_name, item.name, target.id
            ),
            property_site(compilation, item.anchor, key),
        )
        .with_origin(format!("{}.{key}", item.name))
        .with_help(
            "set `crossDiscovery: separate` in the belay-config if each tool's tree is deployed apart"
                .to_string(),
        ),
    );
}

/// Every file under `folder`, sorted by path. Symbolic links are refused:
/// what they point at could be anywhere.
pub(crate) fn read_folder(
    compilation: &Compilation,
    item: &Construct,
    folder: &Path,
    plan: &mut Draft,
) -> Option<Vec<Entry>> {
    let key = property(item.kind).unwrap_or("files");
    let site = property_site(compilation, item.anchor, key);
    if !folder.is_dir() {
        plan.diagnostics.push(
            error_at(
                "missing-vendored-files",
                format!("`{}` names `{}`, which is not a directory", item.name, shown(compilation, folder)),
                site,
            )
            .with_origin(format!("{}.{key}", item.name)),
        );
        return None;
    }
    let mut entries = Vec::new();
    let mut pending = vec![folder.to_path_buf()];
    let mut ok = true;
    while let Some(directory) = pending.pop() {
        let listing = match std::fs::read_dir(&directory) {
            Ok(listing) => listing,
            Err(error) => {
                plan.diagnostics.push(unreadable(compilation, item, &directory, &error, site.clone()));
                ok = false;
                continue;
            }
        };
        for entry in listing.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                plan.diagnostics.push(
                    error_at(
                        "unsupported-vendored-file",
                        format!(
                            "`{}` is a symbolic link; `{}` can only bring regular files",
                            shown(compilation, &path),
                            item.name
                        ),
                        site.clone(),
                    )
                    .with_origin(format!("{}.{key}", item.name)),
                );
                ok = false;
            } else if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                match read_file(&path) {
                    Ok((text, copied)) => {
                        let relative = path
                            .strip_prefix(folder)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .replace('\\', "/");
                        entries.push(Entry { relative, text, copied });
                    }
                    Err(error) => {
                        plan.diagnostics.push(unreadable(compilation, item, &path, &error, site.clone()));
                        ok = false;
                    }
                }
            }
        }
    }
    entries.sort_by(|a, b| a.relative.cmp(&b.relative));
    ok.then_some(entries)
}

/// Reads one vendored file: its text, or its bytes when it is not UTF-8.
pub(crate) fn read_file(path: &Path) -> std::io::Result<(String, Copied)> {
    let bytes = std::fs::read(path)?;
    let executable = is_executable(path);
    Ok(match String::from_utf8(bytes) {
        Ok(text) => (
            text,
            Copied {
                source: path.to_path_buf(),
                bytes: None,
                executable,
            },
        ),
        Err(error) => (
            String::new(),
            Copied {
                source: path.to_path_buf(),
                bytes: Some(error.into_bytes()),
                executable,
            },
        ),
    })
}

/// Reports a vendored file that could not be read.
pub(crate) fn unreadable(
    compilation: &Compilation,
    item: &Construct,
    path: &Path,
    error: &std::io::Error,
    site: (PathBuf, Span),
) -> piton_core::Diagnostic {
    let key = property(item.kind).unwrap_or("files");
    error_at(
        "missing-vendored-files",
        format!("`{}` names `{}`, which cannot be read: {error}", item.name, shown(compilation, path)),
        site,
    )
    .with_origin(format!("{}.{key}", item.name))
}

/// Properties a construct supplied as it is cannot also carry: the vendored
/// file is its whole definition, so anything else would be silently ignored.
pub(crate) fn report_conflicts(compilation: &Compilation, item: &Construct, plan: &mut Draft) {
    let key = property(item.kind).unwrap_or("files");
    for (name, value) in &item.properties {
        if name == key || matches!(value, Value::Null) {
            continue;
        }
        plan.diagnostics.push(
            error_at(
                "vendored-conflict",
                format!(
                    "`{}.{name}` would be ignored: the {} is defined by the file `{key}` names",
                    item.name,
                    item.kind.as_str()
                ),
                property_site(compilation, item.anchor, name),
            )
            .with_origin(format!("{}.{name}", item.name))
            .with_help(format!("remove `{name}`, or edit it in the vendored file")),
        );
    }
}

/// Checks that a vendored definition carries the identity Belay gives it.
pub(crate) fn check_identity(
    compilation: &Compilation,
    item: &Construct,
    path: &Path,
    found: Option<String>,
    expected: &str,
    plan: &mut Draft,
) {
    if found.as_deref() == Some(expected) {
        return;
    }
    let key = property(item.kind).unwrap_or("files");
    let message = match found {
        Some(name) => format!(
            "`{}` is named `{name}`, but `{}` is emitted as `{expected}`",
            shown(compilation, path),
            item.name
        ),
        None => format!(
            "`{}` has no `name`, and the {} `{}` is emitted as `{expected}`",
            shown(compilation, path),
            item.kind.as_str(),
            item.name
        ),
    };
    plan.diagnostics.push(
        error_at("vendored-name-mismatch", message, property_site(compilation, item.anchor, key))
            .with_origin(format!("{}.{key}", item.name))
            .with_help(format!(
                "set `name: {expected}` in the vendored file, or rename the anchor to match it"
            )),
    );
}

/// The `name =` or `description =` of a TOML document's top level.
pub(crate) fn toml_field(contents: &str, field: &str) -> Option<String> {
    for line in contents.lines() {
        if line.trim_start().starts_with('[') {
            break;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != field {
            continue;
        }
        let value = value.trim();
        let unquoted = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        return Some(unquoted.to_string());
    }
    None
}

fn declaring_module(compilation: &Compilation, item: &Construct, key: &str) -> PathBuf {
    let def = compilation.store().anchor(item.anchor);
    match def.slots.get(key) {
        Some(slot) => compilation.anchor_module_path(slot.owner),
        None => compilation.anchor_module_path(item.anchor),
    }
}

/// A dictionary's entries, whether it compiled as a dictionary or as the
/// implicit list a block of keys can become.
fn entries(value: &Value) -> Vec<(String, Value)> {
    match value {
        Value::Dict(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        Value::Mixed(mixed) => mixed.entries().map(|(k, v)| (k.to_string(), v.clone())).collect(),
        _ => Vec::new(),
    }
}

fn shown(compilation: &Compilation, path: &Path) -> String {
    path.strip_prefix(&compilation.project.root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_fields_are_read_from_the_top_level() {
        let doc = "name = \"reviewer\"\ndescription = 'Reviews.'\n[extra]\nname = \"other\"\n";
        assert_eq!(toml_field(doc, "name").as_deref(), Some("reviewer"));
        assert_eq!(toml_field(doc, "description").as_deref(), Some("Reviews."));
        assert_eq!(toml_field("[a]\nname = \"x\"", "name"), None);
    }
}
