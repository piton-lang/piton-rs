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

/// A module a partially typed `from`/`use` specifier could mean.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleCandidate {
    /// The specifier to insert, complete with the prefix already typed.
    pub specifier: String,
    /// The last segment, for display.
    pub name: String,
    /// True when this is importable: a `.pi` file, or a directory with an
    /// `index.pi`.
    pub importable: bool,
    /// True when there is more path to type after this.
    pub directory: bool,
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

    /// Complete a partially typed module specifier written in `from`.
    ///
    /// This lives beside [`Db::resolve`] on purpose: how a specifier turns into
    /// a file is one rule, and the language server should not reimplement it.
    pub fn complete_specifier(&self, from: FileId, partial: &str) -> Vec<ModuleCandidate> {
        if partial.starts_with('@') {
            let mut out: Vec<ModuleCandidate> = self
                .virtual_modules()
                .filter(|name| name.starts_with(partial))
                .map(|name| ModuleCandidate {
                    specifier: name.to_string(),
                    name: name.to_string(),
                    importable: true,
                    directory: false,
                })
                .collect();
            out.sort_by(|a, b| a.specifier.cmp(&b.specifier));
            return out;
        }

        // Split what has been typed into the directory part and the partial leaf.
        let (typed, leaf) = match partial.rfind('/') {
            Some(at) => (&partial[..=at], &partial[at + 1..]),
            None => ("", partial),
        };
        // A specifier with no prefix yet still has to come out as `./name`.
        let written = if typed.is_empty() { "./" } else { typed };

        let base = if partial.starts_with('/') {
            match &self.root {
                Some(root) => root.join(typed.trim_start_matches('/')),
                None => return Vec::new(),
            }
        } else {
            match self.file(from).source.as_path().and_then(Path::parent) {
                Some(directory) => directory.join(typed),
                None => return Vec::new(),
            }
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
                out.push(ModuleCandidate {
                    specifier: format!("{written}{name}"),
                    name,
                    importable: path.join("index.pi").is_file(),
                    directory: true,
                });
            } else if path.extension().is_some_and(|ext| ext == "pi") {
                // Specifiers omit the extension, and `index.pi` is the directory.
                let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                if stem == "index" || current.as_deref() == Some(&canonical(&path)) {
                    continue;
                }
                out.push(ModuleCandidate {
                    specifier: format!("{written}{stem}"),
                    name: stem,
                    importable: true,
                    directory: false,
                });
            }
        }
        out.sort_by(|a, b| a.specifier.cmp(&b.specifier));
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
