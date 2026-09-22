//! Module loading and path resolution.
//!
//! A module is a `.pi` file. A directory becomes a module when it contains an
//! `index.pi`, so an import may point at a directory rather than a file.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use piton_core::{Diagnostic, Span};
use piton_syntax::ast::SourceFile;
use piton_syntax::parser::{self, Parse};

use crate::packages;
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

/// Every Piton source file beneath a directory.
///
/// The compiler does not need this -- it follows imports from the entry, and a
/// file nothing imports is not part of the program. An editor does: the file is
/// on screen before anything imports it. So does anything offering a path to
/// import, since the point of that is to reach somewhere the project does not
/// reach yet.
///
/// Bounded, because callers run it on a keystroke. A tree with more files than
/// this is one where walking to the end helps nobody.
pub fn sources(root: &Path) -> Vec<PathBuf> {
    const LIMIT: usize = 5000;
    // Build output and dependency directories hold no source anyone edits.
    //
    // `tethers` is the interesting one: an installed package *is* Piton source,
    // and it is committed with the project, but it is not the project's source.
    // Counting it, formatting it, or checking it would all report on someone
    // else's work -- and formatting it would rewrite files the lock file has a
    // digest of, which is how an update ends up refusing to run.
    const SKIP: [&str; 5] = ["target", ".git", "node_modules", ".piton", packages::TETHERS];

    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if found.len() >= LIMIT {
                found.sort();
                return found;
            }
            let path = entry.path();
            let name = entry.file_name();
            if path.is_dir() {
                if !SKIP.contains(&name.to_string_lossy().as_ref()) {
                    stack.push(path);
                }
            } else if path
                .extension()
                .is_some_and(|extension| extension == piton_syntax::language::EXTENSION)
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// The two roots module resolution needs.
///
/// They differ: absolute imports are written against the configured source
/// root, while `tethers/` sits beside `piton.config.pi` at the project root.
/// Carrying them together keeps every caller from having to remember which of
/// the two a given lookup wants.
#[derive(Debug, Clone, Copy)]
pub struct Roots<'a> {
    pub source_root: &'a Path,
    pub project_root: &'a Path,
}

impl<'a> Roots<'a> {
    /// Roots for a tree that is its own project, which is what a single file
    /// compiled without configuration has.
    pub fn flat(root: &'a Path) -> Roots<'a> {
        Roots {
            source_root: root,
            project_root: root,
        }
    }
}

/// Where a module path is being resolved from.
pub struct ResolutionContext<'a> {
    /// Directory of the file containing the import.
    pub from_directory: &'a Path,
    /// The project's source root, which absolute paths resolve against.
    pub source_root: &'a Path,
    /// The project root, which `tethers/` sits beside. A path written as a bare
    /// name is looked for there before it is read as a relative location.
    pub project_root: &'a Path,
}

impl<'a> ResolutionContext<'a> {
    pub fn new(from_directory: &'a Path, roots: Roots<'a>) -> Self {
        ResolutionContext {
            from_directory,
            source_root: roots.source_root,
            project_root: roots.project_root,
        }
    }

    /// A context for a tree that is its own project, where a bare name can only
    /// name a package installed beneath that same tree.
    pub fn flat(from_directory: &'a Path, root: &'a Path) -> Self {
        ResolutionContext::new(from_directory, Roots::flat(root))
    }
}

/// Resolves a written module path to a file on disk, or to a bundled package.
///
/// Relative paths resolve against the importing file's directory, absolute
/// paths against the project's configured root, and `@`-prefixed paths name a
/// bundled package. A bare name whose leading segments match an installed
/// package resolves inside that package. A path that names a directory resolves
/// to its `index.pi`.
pub fn resolve(text: &str, context: &ResolutionContext<'_>) -> Result<PathBuf, ResolveError> {
    if text.starts_with('@') {
        if prelude::is_package(text) {
            return Ok(PathBuf::from(text));
        }
        return Err(ResolveError::UnknownPackage(text.to_string()));
    }

    // An installed package stands in for the directory it was installed into,
    // and the rest of the path then resolves as any other path does.
    let installed = packages::is_bare_path(text)
        .then(|| packages::resolve_prefix(context.project_root, text))
        .flatten();

    let base = match &installed {
        Some(path) => path.clone(),
        None if text.starts_with('/') => context.source_root.join(&text[1..]),
        None => context.from_directory.join(text),
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

/// Every module path a project can import by name: the bundled packages, and
/// whatever is installed in `tethers/`.
///
/// Completion and diagnostics both need the same list, so neither can offer a
/// package the other would reject.
pub fn importable_packages(project_root: &Path) -> Vec<String> {
    let mut names: Vec<String> = prelude::PACKAGE_ROOTS
        .iter()
        .map(|name| name.to_string())
        .collect();
    names.extend(packages::installed_names(project_root));
    names
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
        let context = ResolutionContext::flat(Path::new("."), Path::new("."));
        assert_eq!(
            resolve("@piton/belay", &context).unwrap(),
            PathBuf::from("@piton/belay")
        );
        assert!(resolve("@piton/nope", &context).is_err());
    }
}
