//! Loads modules and resolves paths.
//!
//! A module is a `.pi` file. A directory becomes a module when it contains an
//! `index.pi`. Thus an import can name a directory and not a file.

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
    /// The canonical path that identifies the module. Bundled packages use
    /// their package path, for example `@piton/belay`.
    pub path: PathBuf,
    pub source: String,
    pub parse: Parse,
}

impl Module {
    pub fn ast(&self) -> &SourceFile {
        &self.parse.file
    }

    /// The directory that is the base for relative imports in this module.
    pub fn directory(&self) -> PathBuf {
        // A package root takes the place of the index file of that package.
        // Thus a relative import in it resolves from the package directory.
        // It does not resolve from the parent directory.
        let name = self.path.to_string_lossy();
        if prelude::PACKAGE_ROOTS.iter().any(|root| *root == name) {
            return self.path.clone();
        }
        self.path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// True when the module comes from a bundled package and not from the
    /// sources of the project.
    pub fn is_package(&self) -> bool {
        prelude::is_package_path(&self.path.to_string_lossy())
    }
}

/// All modules that a compilation loaded.
#[derive(Default)]
pub struct ModuleGraph {
    modules: Vec<Module>,
    by_path: HashMap<PathBuf, ModuleId>,
    /// Text that replaces the text on disk. The language server uses this.
    /// Thus the analysis shows the unsaved buffers.
    overrides: HashMap<PathBuf, String>,
}

impl ModuleGraph {
    /// Creates a graph that reads the given buffers. It does not read the files
    /// that the buffers shadow.
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

    /// Loads a module. When the path repeats, it uses the module that it
    /// loaded before. Thus circular imports cause no problem. The second visit
    /// finds the module in the graph.
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

    /// Records a module from text that the caller gives. This replaces the
    /// previous copy. The language server uses this for unsaved editor buffers.
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

/// All Piton source files below a directory.
///
/// The compiler does not need this. It follows imports from the entry. A file
/// that no module imports is not part of the program. An editor needs this,
/// because the file is on the screen before a module imports it. A tool that
/// suggests a path to import also needs it. Its purpose is to reach a file
/// that the project does not reach at this time.
///
/// This function has no limit. The format, check and index tools must give a
/// result for each file. The walk can stop early. Then a tool can approve
/// a tree from only the first files that the directory list gave.
pub fn sources(root: &Path) -> Vec<PathBuf> {
    sources_up_to(root, usize::MAX)
}

/// The first `limit` Piton source files below a directory. This is for callers
/// that run on each keystroke. For a tree with more files, a full walk helps
/// no one. Thus this function does not specify the set of files that it keeps.
pub fn sources_up_to(root: &Path, limit: usize) -> Vec<PathBuf> {
    // Build output and dependency directories hold no source that a person edits.
    //
    // `tethers` is the important one. An installed package *is* Piton source.
    // The project commits it, but it is not the source of the project. A count,
    // a format or a check of it reports on the work of another person. Also, a
    // format rewrites files that have a digest in the lock file. Then an update
    // refuses to run.
    const SKIP: [&str; 5] = ["target", ".git", "node_modules", ".piton", packages::TETHERS];

    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if found.len() >= limit {
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

/// The two roots that module resolution needs.
///
/// The two roots are different. Absolute imports start at the configured source
/// root. But `tethers/` is next to `piton.config.pi` at the project root. This
/// type keeps the two roots together. Thus a caller does not need to know which
/// root each search needs.
#[derive(Debug, Clone, Copy)]
pub struct Roots<'a> {
    pub source_root: &'a Path,
    pub project_root: &'a Path,
}

impl<'a> Roots<'a> {
    /// Roots for a tree that is its own project. A single file that compiles
    /// without configuration has these roots.
    pub fn flat(root: &'a Path) -> Roots<'a> {
        Roots {
            source_root: root,
            project_root: root,
        }
    }
}

/// The start location for the resolution of a module path.
pub struct ResolutionContext<'a> {
    /// The directory of the file that contains the import.
    pub from_directory: &'a Path,
    /// The source root of the project. Absolute paths resolve from it.
    pub source_root: &'a Path,
    /// The project root. `tethers/` is next to it. For a path that is a bare
    /// name, the resolver first searches there. Then it reads the path as a
    /// relative location.
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

    /// A context for a tree that is its own project. In it, a bare name can only
    /// name a package that the tree holds below it.
    pub fn flat(from_directory: &'a Path, root: &'a Path) -> Self {
        ResolutionContext::new(from_directory, Roots::flat(root))
    }
}

/// Resolves a module path from the source to a file on disk or to a bundled
/// package.
///
/// Relative paths resolve from the directory of the file with the import. Absolute
/// paths resolve from the configured root of the project. Paths with an `@`
/// prefix name a bundled package. A bare name can start with the segments of an
/// installed package. Then it resolves in that package. A path that names a
/// directory resolves to its `index.pi`.
pub fn resolve(text: &str, context: &ResolutionContext<'_>) -> Result<PathBuf, ResolveError> {
    if text.starts_with('@') {
        if prelude::is_package(text) {
            return Ok(PathBuf::from(text));
        }
        return Err(ResolveError::UnknownPackage(text.to_string()));
    }

    // An installed package takes the place of its install directory. The
    // remainder of the path then resolves as all other paths do.
    let installed = packages::is_bare_path(text)
        .then(|| packages::resolve_prefix(context.project_root, text))
        .flatten();

    let base = match &installed {
        Some(path) => path.clone(),
        None if text.starts_with('/') => context.source_root.join(&text[1..]),
        None => context.from_directory.join(text),
    };
    let base = normalize(&base);

    // A relative import in a bundled package resolves in the virtual file system
    // of that package. It never resolves on the disk.
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

    // `.` and `..`, alone or at the end of a path, always name a directory.
    // Thus they mean its index. They never mean a file in the same directory with the same
    // name as the directory.
    // A package name alone is its directory, for the same reason.
    let index = base.join("index.pi");
    if names_directory(text) || (installed.is_some() && !text.contains('/')) {
        if index.is_file() {
            return Ok(normalize(&index));
        }
        return Err(ResolveError::NotFound {
            written: text.to_string(),
            tried: vec![normalize(&index)],
        });
    }

    // `./Foo` can mean `./Foo.pi` or `./Foo/index.pi`.
    let with_extension = if base.extension().is_some_and(|e| e == "pi") {
        base.clone()
    } else {
        base.with_extension("pi")
    };
    if with_extension.is_file() {
        return Ok(with_extension);
    }
    if index.is_file() {
        return Ok(normalize(&index));
    }
    Err(ResolveError::NotFound {
        written: text.to_string(),
        tried: vec![with_extension, index],
    })
}

/// True when a path can only name a directory. This is `.` or `..` alone, or a
/// path that has one of them as its last segment.
fn names_directory(text: &str) -> bool {
    let trimmed = text.trim_end_matches('/');
    let last = trimmed.rsplit('/').next().unwrap_or(trimmed);
    matches!(last, "." | "..") || (trimmed.is_empty() && !text.is_empty())
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

/// Removes `.` and `..` components. It does not use the file system. Thus the
/// paths are stable, if the target exists or not.
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

    fn scratch(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "piton-module-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("temp dir");
        directory
    }

    #[test]
    fn a_dot_path_is_the_directory_index_never_a_sibling_file() {
        let root = scratch("dots");
        std::fs::create_dir_all(root.join("a/b")).expect("dirs");
        std::fs::write(root.join("a/index.pi"), "x: 1\n").expect("write");
        // A file in the same directory with the same name as the directory. `..` must not select it.
        std::fs::write(root.join("a.pi"), "x: 2\n").expect("write");
        std::fs::write(root.join("a/b/index.pi"), "x: 3\n").expect("write");
        std::fs::write(root.join("a/b.pi"), "x: 4\n").expect("write");

        let from = root.join("a/b");
        let context = ResolutionContext::flat(&from, &root);
        assert_eq!(resolve("..", &context).unwrap(), root.join("a/index.pi"));
        assert_eq!(resolve("../", &context).unwrap(), root.join("a/index.pi"));
        assert_eq!(resolve(".", &context).unwrap(), root.join("a/b/index.pi"));
        assert_eq!(resolve("../b/.", &context).unwrap(), root.join("a/b/index.pi"));
        assert_eq!(resolve("../b/..", &context).unwrap(), root.join("a/index.pi"));
        // Without the dot, a file wins over a directory of the same name.
        assert_eq!(resolve("../b", &context).unwrap(), root.join("a/b.pi"));

        std::fs::remove_file(root.join("a/b/index.pi")).expect("remove");
        assert!(resolve(".", &context).is_err(), "no index, no module");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_package_without_an_index_is_still_a_location() {
        let root = scratch("package");
        std::fs::create_dir_all(root.join("tethers/my-package")).expect("dirs");
        std::fs::write(root.join("tethers/my-package/Foo.pi"), "export x: 1\n").expect("write");
        std::fs::create_dir_all(root.join("spec")).expect("dirs");

        let from = root.join("spec");
        let context = ResolutionContext::new(
            &from,
            Roots {
                source_root: &from,
                project_root: &root,
            },
        );
        assert_eq!(
            resolve("my-package/Foo", &context).unwrap(),
            root.join("tethers/my-package/Foo.pi")
        );
        assert!(resolve("my-package", &context).is_err(), "nothing to import by name alone");
        let _ = std::fs::remove_dir_all(&root);
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
