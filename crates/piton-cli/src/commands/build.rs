//! `piton build` — compile the project and write every configured artifact.
//!
//! The plan is built and validated in full before anything is written, so a
//! project either produces a consistent set of artifacts or produces
//! diagnostics and no partial output.

use std::path::{Path, PathBuf};

use piton_compile::Framework;

use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

/// Where the build records the files it owns, so cleanup never touches a file
/// Belay did not write.
const MANIFEST: &str = ".piton/manifest.json";

pub fn run(config: Option<&Path>, dry_run: bool) -> u8 {
    let (project, mut diagnostics) = project::from_config(config);
    if project.config_path.is_none() {
        report::fail("no piton.config.pi found; a build needs a project configuration");
        return EXIT_ERRORS;
    }
    let root = project.root.clone();
    let compilation = project::compile_project(project);
    diagnostics.extend(compilation.diagnostics.iter().cloned());

    let belay = compilation
        .project
        .frameworks
        .iter()
        .find_map(|framework| match framework {
            Framework::Belay(config) => Some(config.clone()),
        });

    let mut plan = piton_belay::Plan::default();
    if let Some(config) = &belay {
        plan = piton_belay::plan(&compilation, config);
        diagnostics.extend(plan.diagnostics.iter().cloned());
    } else {
        eprintln!("no framework configured; nothing to build");
    }

    diagnostics.sort();
    let had_errors = report::diagnostics(
        &diagnostics,
        &|path| compilation.source_of(path).map(str::to_string),
        &root,
    );

    if had_errors {
        report::summary(&diagnostics);
        eprintln!("build stopped; no files were written");
        return EXIT_ERRORS;
    }

    if dry_run {
        for file in &plan.files {
            println!(
                "{} ({}, {})",
                file.path.display(),
                file.kind.as_str(),
                file.target
            );
        }
        eprintln!(
            "{} {} would be written",
            plan.files.len(),
            report::plural(plan.files.len(), "file", "files")
        );
        return EXIT_SUCCESS;
    }

    let manifest_path = root.join(MANIFEST);
    let previous = std::fs::read_to_string(&manifest_path)
        .map(|text| piton_belay::manifest_paths(&text))
        .unwrap_or_default();

    let written = match piton_belay::write(&plan, &root) {
        Ok(written) => written,
        Err(error) => {
            report::fail(format!("cannot write output: {error}"));
            return EXIT_ERRORS;
        }
    };

    let removed = piton_belay::clean_stale(&previous, &plan, &root);

    if let Some(parent) = manifest_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(error) = std::fs::write(&manifest_path, piton_belay::manifest_document(&plan)) {
        report::fail(format!("cannot write the build manifest: {error}"));
        return EXIT_ERRORS;
    }

    for path in &written {
        println!("{}", path.display());
    }
    for path in &removed {
        println!("removed {}", path.display());
    }
    report::summary(&diagnostics);
    eprintln!(
        "wrote {} {}",
        written.len(),
        report::plural(written.len(), "file", "files")
    );
    let _ = PathBuf::new();
    EXIT_SUCCESS
}
