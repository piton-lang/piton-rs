//! Source files, module resolution, and the file graph.
//!
//! The database is a plain in-memory map. Nothing here is incremental beyond
//! caching parses per file, which is enough for a compiler whose unit of work
//! is a whole project and for a language server that re-parses one file at a
//! time.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use piton_syntax::{ast, Parse};

use crate::hir::{lower_hir, Hir};
use crate::FileId;

/// Where a file's text came from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// A `.pi` file on disk.
    Disk(PathBuf),
    /// A module built into the compiler or contributed by a framework.
    Virtual(String),
}

impl Source {
    pub fn display(&self) -> String {
        match self {
            Source::Disk(path) => path.display().to_string(),
            Source::Virtual(name) => name.clone(),
        }
    }

    pub fn as_path(&self) -> Option<&Path> {
        match self {
            Source::Disk(path) => Some(path),
            Source::Virtual(_) => None,
        }
    }
}

/// One loaded file.
pub struct File {
    pub id: FileId,
    pub source: Source,
    pub text: String,
    pub parse: Parse,
    pub hir: Hir,
}

impl File {
    pub fn root(&self) -> ast::Root {
        self.parse.root()
    }
}

/// Every file the compiler knows about.
#[derive(Default)]
pub struct Db {
    files: Vec<File>,
    by_source: HashMap<Source, FileId>,
    /// Text supplied by an editor, taking precedence over the disk.
    overlays: HashMap<PathBuf, String>,
    /// Sources for `@`-prefixed modules.
    virtual_sources: HashMap<String, String>,
    /// The project root, used for `/`-prefixed absolute imports.
    root: Option<PathBuf>,
}

/// Why a module path could not be resolved.
#[derive(Clone, Debug)]
pub struct ResolveError {
    pub message: String,
}

impl Db {
    pub fn new() -> Db {
        Db::default()
    }

    pub fn set_root(&mut self, root: impl Into<PathBuf>) {
        self.root = Some(root.into());
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Register a module reachable as `@name`.
    pub fn add_virtual_module(&mut self, name: impl Into<String>, source: impl Into<String>) {
        self.virtual_sources.insert(name.into(), source.into());
    }

    /// Replace a file's text with an editor's unsaved buffer.
    pub fn set_overlay(&mut self, path: impl AsRef<Path>, text: impl Into<String>) -> FileId {
        let path = canonical(path.as_ref());
        self.overlays.insert(path.clone(), text.into());
        let source = Source::Disk(path.clone());
        if let Some(&id) = self.by_source.get(&source) {
            let text = self.overlays[&path].clone();
            self.replace(id, text);
            return id;
        }
        self.load_source(source).expect("overlay text is always available")
    }

    pub fn remove_overlay(&mut self, path: &Path) {
        self.overlays.remove(&canonical(path));
    }

    pub fn file(&self, id: FileId) -> &File {
        &self.files[id.0 as usize]
    }

    pub fn files(&self) -> impl Iterator<Item = &File> {
        self.files.iter()
    }

    pub fn file_id(&self, path: &Path) -> Option<FileId> {
        self.by_source.get(&Source::Disk(canonical(path))).copied()
    }

    /// Load a file from disk, or return the id it already has.
    pub fn load(&mut self, path: &Path) -> Result<FileId, ResolveError> {
        self.load_source(Source::Disk(canonical(path)))
    }

    fn load_source(&mut self, source: Source) -> Result<FileId, ResolveError> {
        if let Some(&id) = self.by_source.get(&source) {
            return Ok(id);
        }
        let text = match &source {
            Source::Disk(path) => match self.overlays.get(path) {
                Some(text) => text.clone(),
                None => std::fs::read_to_string(path).map_err(|error| ResolveError {
                    message: format!("cannot read {}: {error}", path.display()),
                })?,
            },
            Source::Virtual(name) => {
                self.virtual_sources.get(name).cloned().ok_or_else(|| ResolveError {
                    message: format!("unknown builtin module `{name}`"),
                })?
            }
        };
        let id = FileId(self.files.len() as u32);
        let parse = piton_syntax::parse(&text);
        let hir = lower_hir(&parse.root());
        self.files.push(File { id, source: source.clone(), text, parse, hir });
        self.by_source.insert(source, id);
        Ok(id)
    }

    /// Re-parse a file after its text changed.
    pub fn replace(&mut self, id: FileId, text: String) {
        let parse = piton_syntax::parse(&text);
        let hir = lower_hir(&parse.root());
        let file = &mut self.files[id.0 as usize];
        file.text = text;
        file.parse = parse;
        file.hir = hir;
    }

    /// Resolve an import specifier written in `from`.
    ///
    /// `@name` is a builtin module, `/a/b` is relative to the project root, and
    /// anything else is relative to the importing file. A directory resolves to
    /// its `index.pi`.
    pub fn resolve(&mut self, from: FileId, spec: &str) -> Result<FileId, ResolveError> {
        if let Some(name) = spec.strip_prefix('@') {
            return self.load_source(Source::Virtual(format!("@{name}")));
        }
        let base = if let Some(rest) = spec.strip_prefix('/') {
            match &self.root {
                Some(root) => root.join(rest),
                None => {
                    return Err(ResolveError {
                        message: format!(
                            "absolute import `{spec}` needs a project root; add a piton.config.pi"
                        ),
                    })
                }
            }
        } else {
            let dir = match self.file(from).source.as_path().and_then(Path::parent) {
                Some(dir) => dir.to_path_buf(),
                None => {
                    return Err(ResolveError {
                        message: format!("`{spec}` cannot be resolved from a builtin module"),
                    })
                }
            };
            dir.join(spec)
        };
        for candidate in module_candidates(&base) {
            if self.overlays.contains_key(&candidate) || candidate.is_file() {
                return self.load_source(Source::Disk(canonical(&candidate)));
            }
        }
        Err(ResolveError { message: format!("cannot find module `{spec}`") })
    }
}

/// How a specifier addresses its target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModuleOrigin {
    /// `./x`, resolved against the importing file.
    Relative,
    /// `/x`, resolved against the project root.
    Root,
    /// `@piton/x`, a module built into the compiler or a framework.
    Builtin,
}

/// A module a partially typed `from`/`use` specifier could mean.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleCandidate {
    /// The last segment, which is what a reader is scanning for.
    pub name: String,
    /// The complete specifier this produces.
    pub specifier: String,
    /// The text that replaces the typed specifier from `replace_from` on.
    pub insert: String,
    /// Byte offset into the typed text where `insert` begins.
    pub replace_from: usize,
    /// True when this can be imported: a `.pi` file, or a directory with an
    /// `index.pi`.
    pub importable: bool,
    /// True when there is more path to type after it.
    pub directory: bool,
    pub origin: ModuleOrigin,
}

impl Db {
    /// Every builtin module name, for completing `@`-prefixed specifiers.
    pub fn virtual_modules(&self) -> impl Iterator<Item = &str> {
        self.virtual_sources.keys().map(String::as_str)
    }

    /// Resolve a specifier against the files already loaded, without loading.
    ///
    /// [`Db::resolve`] is the compiler's path, and needs `&mut` because it
    /// loads. Tools that only want to know what a specifier points at — a
    /// document link, a completion, a go-to-definition on a half-typed line —
    /// use this, so there is still only one description of how a path resolves.
    pub fn lookup_module(&self, from: FileId, spec: &str) -> Option<FileId> {
        if let Some(name) = spec.strip_prefix('@') {
            return self.by_source.get(&Source::Virtual(format!("@{name}"))).copied();
        }
        let base = if let Some(rest) = spec.strip_prefix('/') {
            self.root.as_ref()?.join(rest)
        } else {
            self.file(from).source.as_path()?.parent()?.join(spec)
        };
        module_candidates(&base)
            .into_iter()
            .find_map(|candidate| self.by_source.get(&Source::Disk(canonical(&candidate))).copied())
    }

    /// Complete a partially typed module specifier written in `from` or `use`.
    ///
    /// This lives beside [`Db::resolve`] on purpose: how a specifier turns into
    /// a file is one rule, and the language server should not reimplement it.
    ///
    /// With nothing typed yet, all three ways of addressing a module are
    /// offered together, because a specifier that could have been written
    /// against the project root is not discoverable otherwise.
    pub fn complete_specifier(&self, from: FileId, partial: &str) -> Vec<ModuleCandidate> {
        let mut out = Vec::new();
        match partial.chars().next() {
            Some('@') => out.extend(self.builtin_candidates(partial)),
            Some('/') => out.extend(self.path_candidates(from, partial, ModuleOrigin::Root)),
            Some('.') => out.extend(self.path_candidates(from, partial, ModuleOrigin::Relative)),
            Some(_) => {
                // Something is typed but it names no prefix; treat it as a leaf
                // of the current directory.
                out.extend(self.path_candidates(from, partial, ModuleOrigin::Relative));
            }
            None => {
                out.extend(self.path_candidates(from, partial, ModuleOrigin::Relative));
                if self.root.is_some() {
                    out.extend(self.path_candidates(from, partial, ModuleOrigin::Root));
                }
                out.extend(self.builtin_candidates(partial));
            }
        }
        out.sort_by(|a, b| {
            (a.origin, !a.importable, a.name.to_lowercase())
                .cmp(&(b.origin, !b.importable, b.name.to_lowercase()))
        });
        out
    }

    fn builtin_candidates(&self, partial: &str) -> Vec<ModuleCandidate> {
        self.virtual_modules()
            .filter(|name| name.starts_with(partial))
            .map(|name| ModuleCandidate {
                name: name.to_string(),
                specifier: name.to_string(),
                insert: name.to_string(),
                replace_from: 0,
                importable: true,
                directory: false,
                origin: ModuleOrigin::Builtin,
            })
            .collect()
    }

    /// Read one directory and offer what is in it.
    fn path_candidates(
        &self,
        from: FileId,
        partial: &str,
        origin: ModuleOrigin,
    ) -> Vec<ModuleCandidate> {
        // Split what has been typed into the part that is already a directory
        // and the leaf still being written.
        let (typed, leaf) = match partial.rfind('/') {
            Some(at) => (&partial[..=at], &partial[at + 1..]),
            None => ("", partial),
        };
        // With no prefix typed, the candidate has to supply one itself.
        let written = match (typed.is_empty(), origin) {
            (false, _) => typed.to_string(),
            (true, ModuleOrigin::Root) => "/".to_string(),
            (true, _) => "./".to_string(),
        };
        let replace_from = if typed.is_empty() { 0 } else { typed.len() };

        let base = match origin {
            ModuleOrigin::Root => match &self.root {
                Some(root) => root.join(written.trim_start_matches('/')),
                None => return Vec::new(),
            },
            _ => match self.file(from).source.as_path().and_then(Path::parent) {
                Some(directory) => directory.join(&written),
                None => return Vec::new(),
            },
            
        };

        let Ok(entries) = std::fs::read_dir(&base) else { return Vec::new() };
        // A file cannot import itself, so it is never a candidate.
        let current = self.file(from).source.as_path().map(canonical);
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || !name.starts_with(leaf) {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                if IGNORED_DIRECTORIES.contains(&name.as_str()) {
                    continue;
                }
                let importable = path.join("index.pi").is_file();
                // A plain directory has to be descended into, so it completes
                // with the separator already typed.
                let leaf = if importable { name.clone() } else { format!("{name}/") };
                let specifier = format!("{written}{leaf}");
                out.push(ModuleCandidate {
                    // Replacing from `replace_from` has to reproduce the
                    // specifier, which is what supplies `./` or `/` when
                    // nothing has been typed yet.
                    insert: specifier[replace_from..].to_string(),
                    specifier,
                    name,
                    replace_from,
                    importable,
                    directory: true,
                    origin,
                });
            } else if path.extension().is_some_and(|ext| ext == "pi") {
                // Specifiers omit the extension, and `index.pi` is the directory.
                let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                if stem == "index" || current.as_deref() == Some(&canonical(&path)) {
                    continue;
                }
                let specifier = format!("{written}{stem}");
                out.push(ModuleCandidate {
                    insert: specifier[replace_from..].to_string(),
                    specifier,
                    name: stem,
                    replace_from,
                    importable: true,
                    directory: false,
                    origin,
                });
            }
        }
        out
    }
}

/// Directories that never hold Piton sources.
const IGNORED_DIRECTORIES: &[&str] = &["node_modules", "target", "dist", "build"];

/// The file names a module specifier may resolve to, in order of preference.
fn module_candidates(base: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if base.extension().is_some_and(|ext| ext == "pi") {
        candidates.push(base.to_path_buf());
    } else {
        candidates.push(base.with_extension("pi"));
        candidates.push(base.join("index.pi"));
    }
    candidates
}

/// Normalise a path without requiring it to exist.
pub fn canonical(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut out = PathBuf::new();
    for part in absolute.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}
