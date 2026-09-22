//! Locating sources and projects for the commands.

use std::path::{Path, PathBuf};

use globset::Glob;
use piton_compile::{config, Compilation, Project};

/// Loads the project rooted at the current working directory.
pub fn current() -> (Project, piton_core::DiagnosticSink) {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    config::load(&cwd, None)
}

/// Loads a project from an explicit configuration path.
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
/// A directory expands to every `.pi` file beneath it; a glob expands to its
/// matches; a file is taken as-is.
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

/// Every `.pi` file beneath `root`, skipping build and VCS directories.
pub fn walk(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if !matches!(name.as_ref(), "target" | ".git" | "node_modules" | ".piton") {
                    stack.push(path);
                }
            } else if path.extension().is_some_and(|extension| extension == "pi") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Resolves a path the user typed to the form the module graph stores.
///
/// A project is loaded from absolute paths, so a module is keyed by one. A
/// relative path typed on the command line has to be resolved against the
/// working directory before it will match anything, which is what makes
/// `piton reach spec/Thing.pi` find the same module as the absolute form.
pub fn canonical_target(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    // `canonicalize` also resolves symlinks, which would stop the result
    // matching a graph keyed by the path as written; normalizing does not.
    piton_compile::module::normalize(&absolute)
}

/// Shortens a path for display relative to the project root.
pub fn display(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}
