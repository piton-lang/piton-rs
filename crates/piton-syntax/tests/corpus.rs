//! Parses every `.pi` file in the repository.
//!
//! The specification is written in Piton, so the specification itself is the
//! parser's acceptance suite: it must parse without errors and the lossless tree
//! must reproduce every byte.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

fn piton_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if !matches!(name.as_ref(), "target" | ".git" | "node_modules") {
                    stack.push(path);
                }
            } else if path.extension().is_some_and(|e| e == "pi") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[test]
fn the_specification_parses_cleanly() {
    let root = repo_root();
    let files = piton_files(&root);
    assert!(files.len() > 100, "expected the spec corpus, found {}", files.len());

    let mut failures = Vec::new();
    for path in &files {
        let source = std::fs::read_to_string(path).expect("readable");
        let parse = piton_syntax::parser::parse(&source, path);

        if parse.syntax().text().to_string() != source {
            failures.push(format!("{}: lossless round-trip failed", path.display()));
        }
        for diagnostic in parse.diagnostics.iter().filter(|d| d.is_error()) {
            failures.push(format!(
                "{}: {} [{}]",
                path.strip_prefix(&root).unwrap_or(path).display(),
                diagnostic.message,
                diagnostic.code
            ));
        }
    }
    assert!(failures.is_empty(), "{} problems:\n{}", failures.len(), failures.join("\n"));
}
