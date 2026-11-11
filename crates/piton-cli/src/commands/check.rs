//! `piton check` — validate and write nothing.
//!
//! The check examines syntax, imports, references, types, inheritance,
//! composition, exports, and circular dependencies. It emits no artifacts.
//! When it reports an error, it exits with a non-zero code.

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
        // Without a project, there is no entry. Thus the check examines each
        // file below the working directory separately. `piton loc` and
        // `piton format` use the same set of files.
        return check_files(&[]);
    }
    check_project(project, diagnostics)
}

fn check_project(project: piton_compile::Project, mut diagnostics: DiagnosticSink) -> u8 {
    let root = project.root.clone();
    let compilation = project::compile_project(project);
    diagnostics.extend(compilation.diagnostics.iter().cloned());

    // Plugins with a build make their plan. Thus the check also examines their
    // diagnostics. It writes no files.
    let (_, _, manifests, _) = crate::plugins::load();
    let (plans, failures) = crate::plugins::plan_builds(&compilation.project, &manifests);
    for plan in plans {
        diagnostics.extend(plan.diagnostics);
    }
    for failure in &failures {
        report::fail(failure);
    }
    diagnostics.sort();

    let had_errors = report::diagnostics(
        &diagnostics,
        &|path| compilation.source_of(path).map(str::to_string),
        &root,
    );
    report::summary(&diagnostics);
    if had_errors || !failures.is_empty() {
        EXIT_ERRORS
    } else {
        EXIT_SUCCESS
    }
}

/// Checks specific files. Each file compiles as its own entry point. Thus the
/// check can examine a file without its project.
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
        // Report problems only in the requested files. If not, the check
        // reports a shared dependency one time for each file that reaches it.
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
