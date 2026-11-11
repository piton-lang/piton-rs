//! Finds the sources and the projects for the commands.

use std::path::{Path, PathBuf};

use globset::Glob;
use piton_compile::{config, Compilation, Project};

/// Loads the project at the current directory.
pub fn current() -> (Project, piton_core::DiagnosticSink) {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    config::load(&cwd, None)
}

/// Loads the project that contains the current directory. The project is at
/// the nearest `piton.config.pi` in the current directory or in a directory
/// above it. When there is no such file, the project is the current directory.
///
/// A user runs `piton slice` from the directory where they work. Its output
/// is for that directory. Thus it looks upward. Other commands read only the
/// configuration in the current directory.
pub fn enclosing() -> (Project, piton_core::DiagnosticSink) {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let found = cwd
        .ancestors()
        .find_map(|directory| config::find_config(directory));
    config::load(&cwd, found.as_deref())
}

/// Loads a project from a given configuration path.
pub fn from_config(path: Option<&Path>) -> (Project, piton_core::DiagnosticSink) {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    config::load(&cwd, path)
}

/// Compiles the current project, or reports why it cannot.
pub fn compile_project(project: Project) -> Compilation {
    Compilation::build(project)
}

/// Expands paths and globs into `.pi` files.
///
/// A directory expands to all `.pi` files below it. A glob expands to its
/// matches. A file stays as it is.
pub fn collect_sources(inputs: &[PathBuf], fallback_root: &Path) -> Result<Vec<PathBuf>, String> {
    if inputs.is_empty() {
        return Ok(walk(fallback_root));
    }
    let mut out = Vec::new();
    for input in inputs {
        let text = input.to_string_lossy().to_string();
        if text.contains('*') || text.contains('?') || text.contains('[') {
            out.extend(expand_glob(&text)?);
        } else if input.is_dir() {
            out.extend(walk(input));
        } else if input.is_file() {
            out.push(input.clone());
        } else {
            return Err(format!("`{}` does not exist", input.display()));
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Expands a glob relative to the current directory.
pub fn expand_glob(pattern: &str) -> Result<Vec<PathBuf>, String> {
    let glob = Glob::new(pattern)
        .map_err(|error| format!("invalid pattern `{pattern}`: {error}"))?
        .compile_matcher();
    let root = pattern
        .split(['*', '?', '['])
        .next()
        .map(Path::new)
        .and_then(|p| {
            if p.is_dir() {
                Some(p.to_path_buf())
            } else {
                p.parent().map(Path::to_path_buf)
            }
        })
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("."));

    let mut out: Vec<PathBuf> = walk(&root)
        .into_iter()
        .filter(|path| {
            glob.is_match(path)
                || path
                    .strip_prefix("./")
                    .map(|p| glob.is_match(p))
                    .unwrap_or(false)
        })
        .collect();
    out.sort();
    Ok(out)
}

/// All `.pi` files below `root`. It does not go into build and VCS directories.
///
/// The compiler uses the same walk to find the sources that an editor must
/// know. Thus the CLI counts the same set of files that the language server
/// opens.
pub fn walk(root: &Path) -> Vec<PathBuf> {
    piton_compile::module::sources(root)
}

/// Changes a path from the user to the form that the module graph keeps.
///
/// The loader uses absolute paths for a project. Thus the key of a module is
/// an absolute path. A relative path from the command line must join the
/// current directory before it can match a key. Thus
/// `piton reach spec/Thing.pi` finds the same module as the absolute form.
pub fn canonical_target(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    // `canonicalize` also resolves symlinks. Then the result does not match
    // a graph with the written paths as keys. Normalization keeps symlinks.
    piton_compile::module::normalize(&absolute)
}

/// Makes a path shorter for display. The path becomes relative to the project
/// root.
pub fn display(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}
