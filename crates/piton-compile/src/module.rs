//! Module loading and path resolution.
//!
//! A module is a `.pi` file. A directory becomes a module when it contains an
//! `index.pi`, so an import may point at a directory rather than a file.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use piton_core::{Diagnostic, Span};
use piton_syntax::ast::SourceFile;
use piton_syntax::parser::{self, Parse};

use crate::prelude;

/// Identifies one loaded module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleId(pub u32);

/// A loaded and parsed source file.
pub struct Module {
    pub id: ModuleId,
    /// The canonical path used as the module's identity. Bundled packages use
    /// their package path, e.g. `@piton/belay`.
    pub path: PathBuf,
    pub source: String,
    pub parse: Parse,
}

impl Module {
    pub fn ast(&self) -> &SourceFile {
        &self.parse.file
    }

    /// The directory imports inside this module resolve relative to.
    pub fn directory(&self) -> PathBuf {
        // A package root stands in for that package's index file, so a relative
        // import written in it resolves from inside the package rather than
        // from beside it.
        let name = self.path.to_string_lossy();
        if prelude::PACKAGE_ROOTS.iter().any(|root| *root == name) {
            return self.path.clone();
        }
        self.path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// True when the module comes from a bundled package rather than the
    /// project's own sources.
    pub fn is_package(&self) -> bool {
        prelude::is_package_path(&self.path.to_string_lossy())
    }
}

/// Every module loaded for a compilation.
#[derive(Default)]
pub struct ModuleGraph {
    modules: Vec<Module>,
    by_path: HashMap<PathBuf, ModuleId>,
    /// Text supplied in place of what is on disk. The language server uses this
    /// so analysis reflects unsaved buffers.
    overrides: HashMap<PathBuf, String>,
}

impl ModuleGraph {
    /// Creates a graph that reads the given buffers instead of the files they
    /// shadow.
    pub fn with_overrides(overrides: HashMap<PathBuf, String>) -> ModuleGraph {
        ModuleGraph {
            overrides,
            ..ModuleGraph::default()
        }
    }

    pub fn get(&self, id: ModuleId) -> &Module {
        &self.modules[id.0 as usize]
    }

    pub fn iter(&self) -> impl Iterator<Item = &Module> {
        self.modules.iter()
    }

    pub fn len(&self) -> usize {
        self.modules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    pub fn id_for(&self, path: &Path) -> Option<ModuleId> {
        self.by_path.get(path).copied()
    }

    /// Loads a module, reusing an already loaded one when the path repeats.
    /// Circular imports are therefore harmless: the second visit finds the
    /// module already registered.
    pub fn load(
        &mut self,
        path: &Path,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<ModuleId> {
        if let Some(existing) = self.by_path.get(path) {
            return Some(*existing);
        }
        if let Some(text) = self.overrides.get(path).cloned() {
            return Some(self.insert(path.to_path_buf(), text));
        }
        let key = path.to_string_lossy().to_string();
        let source = match prelude::package_source(&key) {
            Some(builtin) => builtin.to_string(),
            None => match std::fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) => {
                    diagnostics.push(Diagnostic::error(
                        "unreadable-module",
                        format!("cannot read `{}`: {error}", path.display()),
                        path,
                        Span::default(),
                    ));
                    return None;
                }
            },
        };
        Some(self.insert(path.to_path_buf(), source))
    }

    /// Registers a module whose text is already in hand, replacing any previous
    /// copy. The language server uses this to swap in unsaved editor buffers.
    pub fn insert(&mut self, path: PathBuf, source: String) -> ModuleId {
        let parse = parser::parse(&source, &path);
        if let Some(existing) = self.by_path.get(&path).copied() {
            self.modules[existing.0 as usize] = Module {
                id: existing,
                path,
                source,
                parse,
            };
            return existing;
        }
        let id = ModuleId(self.modules.len() as u32);
        self.by_path.insert(path.clone(), id);
        self.modules.push(Module {
            id,
            path,
            source,
            parse,
        });
        id
    }
}

/// Where a module path is being resolved from.
pub struct ResolutionContext<'a> {
    /// Directory of the file containing the import.
    pub from_directory: &'a Path,
    /// The project's source root, which absolute paths resolve against.
    pub source_root: &'a Path,
}

/// Resolves a written module path to a file on disk, or to a bundled package.
///
/// Relative paths resolve against the importing file's directory, absolute
/// paths against the project's configured root, and `@`-prefixed paths name a
/// bundled package. A path that names a directory resolves to its `index.pi`.
pub fn resolve(text: &str, context: &ResolutionContext<'_>) -> Result<PathBuf, ResolveError> {
    if text.starts_with('@') {
        if prelude::is_package(text) {
            return Ok(PathBuf::from(text));
        }
        return Err(ResolveError::UnknownPackage(text.to_string()));
    }

    let base = if let Some(rest) = text.strip_prefix('/') {
        context.source_root.join(rest)
    } else {
        context.from_directory.join(text)
    };
    let base = normalize(&base);

    // A relative import written inside a bundled package resolves against that
    // package's virtual filesystem, never against the disk.
    let key = base.to_string_lossy().replace('\\', "/");
    if prelude::is_package_path(&key) {
        if prelude::is_package(&key) {
            return Ok(PathBuf::from(key));
        }
        let index = format!("{key}/index");
        if prelude::is_package(&index) {
            return Ok(PathBuf::from(index));
        }
        return Err(ResolveError::NotFound {
            written: text.to_string(),
            tried: vec![PathBuf::from(key), PathBuf::from(index)],
        });
    }

    // `./Foo` may mean `./Foo.pi` or `./Foo/index.pi`; `.` and `./dir` mean the
    // directory's index.
    let with_extension = if base.extension().is_some_and(|e| e == "pi") {
        base.clone()
    } else {
        base.with_extension("pi")
    };
    if with_extension.is_file() {
        return Ok(with_extension);
    }
    let index = base.join("index.pi");
    if index.is_file() {
        return Ok(normalize(&index));
    }
    Err(ResolveError::NotFound {
        written: text.to_string(),
        tried: vec![with_extension, index],
    })
}

#[derive(Debug)]
pub enum ResolveError {
    UnknownPackage(String),
    NotFound {
        written: String,
        tried: Vec<PathBuf>,
    },
}

impl ResolveError {
    pub fn message(&self) -> String {
        match self {
            ResolveError::UnknownPackage(name) => {
                format!("`{name}` is not a bundled package")
            }
            ResolveError::NotFound { written, tried } => {
                let tried: Vec<String> = tried.iter().map(|p| p.display().to_string()).collect();
                format!("cannot resolve `{written}`; tried {}", tried.join(", "))
            }
        }
    }

    pub fn help(&self) -> Option<String> {
        match self {
            ResolveError::UnknownPackage(_) => Some(format!(
                "bundled packages are {}",
                prelude::PACKAGE_ROOTS
                    .map(|name| format!("`{name}`"))
                    .join(" and ")
            )),
            ResolveError::NotFound { .. } => Some(
                "a directory resolves to its `index.pi`; check the path is relative to this file"
                    .to_string(),
            ),
        }
    }
}

/// Removes `.` and `..` components without touching the filesystem, so paths
/// stay stable whether or not the target exists.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Computes a relative path from `from` (a directory) to `to`.
pub fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let from = normalize(from);
    let to = normalize(to);
    let from_parts: Vec<_> = from.components().collect();
    let to_parts: Vec<_> = to.components().collect();
    let shared = from_parts
        .iter()
        .zip(&to_parts)
        .take_while(|(a, b)| a == b)
        .count();
    let mut out = PathBuf::new();
    for _ in shared..from_parts.len() {
        out.push("..");
    }
    for part in &to_parts[shared..] {
        out.push(part.as_os_str());
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_removes_dot_segments() {
        assert_eq!(
            normalize(Path::new("a/b/../c/./d")),
            PathBuf::from("a/c/d")
        );
    }

    #[test]
    fn relative_paths_walk_up_then_down() {
        assert_eq!(
            relative_path(
                Path::new("out/scope/language"),
                Path::new("out/scope/language/types/Strings.md")
            ),
            PathBuf::from("types/Strings.md")
        );
        assert_eq!(
            relative_path(
                Path::new("out/scope/language/types"),
                Path::new("out/scope/language/Language.md")
            ),
            PathBuf::from("../Language.md")
        );
    }

    #[test]
    fn packages_resolve_to_themselves() {
        let context = ResolutionContext {
            from_directory: Path::new("."),
            source_root: Path::new("."),
        };
        assert_eq!(
            resolve("@piton/belay", &context).unwrap(),
            PathBuf::from("@piton/belay")
        );
        assert!(resolve("@piton/nope", &context).is_err());
    }
}
