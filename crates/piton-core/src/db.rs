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
