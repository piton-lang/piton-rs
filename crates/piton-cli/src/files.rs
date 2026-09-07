//! Turning command-line arguments into a list of Piton files.

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use globset::Glob;
use walkdir::WalkDir;

/// Every `.pi` file under a directory, in a stable order.
pub fn walk(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "pi"))
        .collect();
    files.sort();
    files
}

/// Expand file paths, directories, and glob patterns into concrete files.
pub fn resolve(patterns: &[String]) -> Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = Vec::new();
    for pattern in patterns {
        let path = Path::new(pattern);
        if path.is_file() {
            out.push(path.to_path_buf());
            continue;
        }
        if path.is_dir() {
            out.extend(walk(path));
            continue;
        }
        if pattern.contains('*') || pattern.contains('?') {
            out.extend(expand_glob(pattern)?);
            continue;
        }
        bail!("no such file or directory: {pattern}");
    }
    out.sort();
    out.dedup();
    if out.is_empty() {
        bail!("no Piton files matched");
    }
    Ok(out)
}

/// Match a glob against the tree beneath its first literal directory.
fn expand_glob(pattern: &str) -> Result<Vec<PathBuf>> {
    let glob = Glob::new(pattern)?.compile_matcher();
    let root = pattern
        .split(['*', '?'])
        .next()
        .map(Path::new)
        .and_then(|prefix| prefix.parent().map(Path::to_path_buf))
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("."));
    Ok(walk(&root).into_iter().filter(|path| glob.is_match(path)).collect())
}
