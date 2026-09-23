//! `piton check` — validate without emitting anything.
//!
//! Checking covers syntax, imports, references, types, inheritance,
//! composition, exports, and circular dependencies. It emits no artifacts, and
//! it exits non-zero when any error was reported.

use std::path::PathBuf;

use piton_compile::Compilation;
use piton_core::DiagnosticSink;

use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

pub fn run(paths: &[PathBuf]) -> u8 {
    if !paths.is_empty() {
        return check_files(paths);
    }
    let (project, diagnostics) = project::current();
    if project.config_path.is_none() {
        // Without a project there is no entry to start from, so every file
        // under the working directory is checked on its own -- the same set
        // `piton loc` and `piton format` work on.
        return check_files(&[]);
    }
    check_project(project, diagnostics)
}

fn check_project(project: piton_compile::Project, mut diagnostics: DiagnosticSink) -> u8 {
    let root = project.root.clone();
    let compilation = project::compile_project(project);
    diagnostics.extend(compilation.diagnostics.iter().cloned());
    diagnostics.sort();

    let had_errors = report::diagnostics(
        &diagnostics,
        &|path| compilation.source_of(path).map(str::to_string),
        &root,
    );
    report::summary(&diagnostics);
    if had_errors {
        EXIT_ERRORS
    } else {
        EXIT_SUCCESS
    }
}

/// Checks specific files. Each file is compiled as its own entry point so a
/// file can be checked without its project.
fn check_files(paths: &[PathBuf]) -> u8 {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let sources = match project::collect_sources(paths, &cwd) {
        Ok(sources) => sources,
        Err(message) => {
            report::fail(message);
            return EXIT_ERRORS;
        }
    };
    if sources.is_empty() {
        report::fail("no .pi files to check");
        return EXIT_ERRORS;
    }

    let (configured, _) = project::current();
    let mut combined = DiagnosticSink::new();
    let mut had_errors = false;

    for source in &sources {
        let source = project::canonical_target(source);
        let project = configured.with_entry(&source);
        let compilation = Compilation::build(project);
        // Only report problems in the requested files; a shared dependency
        // would otherwise be reported once per file that reaches it.
        let relevant: Vec<_> = compilation
            .diagnostics
            .iter()
            .filter(|d| d.file == source)
            .cloned()
            .collect();
        had_errors |= relevant.iter().any(|d| d.is_error());
        combined.extend(relevant);
    }

    combined.sort();
    let cwd_clone = cwd.clone();
    report::diagnostics(
        &combined,
        &|path| std::fs::read_to_string(path).ok(),
        &cwd_clone,
    );
    report::summary(&combined);
    if had_errors {
        EXIT_ERRORS
    } else {
        EXIT_SUCCESS
    }
}
