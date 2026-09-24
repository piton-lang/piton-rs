//! `piton build` — compile the project and write every configured artifact.
//!
//! Without a framework, a build writes the renderer's output to the configured
//! output directory: the entry file, and every file holding something the
//! entry's exports reference, each keeping its place under the source root and
//! taking the renderer's extension. A framework's output replaces it, unless
//! the configuration sets `output` or `renderer` to ask for both.
//!
//! The plan is built and validated in full before anything is written, so a
//! project either produces a consistent set of artifacts or produces
//! diagnostics and no partial output.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use piton_belay::{OutputFile, OutputKind, Plan};
use piton_compile::{module, reach, Compilation, Framework, ModuleId};
use piton_emit::Adapter;

use crate::{project, render, report, EXIT_ERRORS, EXIT_SUCCESS};

/// Where the build records the files it owns, so cleanup never touches a file
/// the build did not write.
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

    if !compilation.project.entry.is_file() {
        report::fail(format!(
            "the entry file `{}` does not exist",
            project::display(&compilation.project.entry, &root)
        ));
        eprintln!("  help: set `entry` in piton.config.pi, or add an index.pi to the root");
        return EXIT_ERRORS;
    }

    let renderer = match render::parse_renderer(&compilation.project.renderer) {
        Ok(renderer) => renderer,
        Err(message) => {
            report::fail(message);
            return EXIT_ERRORS;
        }
    };

    // The renderer's output comes first; a framework adds to it.
    let mut plan = Plan {
        files: if compilation.project.renders {
            rendered_files(&compilation, renderer)
        } else {
            Vec::new()
        },
        diagnostics: Vec::new(),
        targets: Vec::new(),
    };

    let belay = compilation
        .project
        .frameworks
        .iter()
        .find_map(|framework| match framework {
            Framework::Belay(config) => Some(config.clone()),
        });
    if let Some(config) = &belay {
        let framework = piton_belay::plan(&compilation, config);
        plan.files.extend(framework.files);
        plan.targets.extend(framework.targets);
        diagnostics.extend(framework.diagnostics.iter().cloned());
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
    EXIT_SUCCESS
}

/// The renderer's share of the build: one file per module that is emitted.
fn rendered_files(compilation: &Compilation, renderer: Adapter) -> Vec<OutputFile> {
    let project = &compilation.project;
    let mut files = Vec::new();
    for module in emitted_modules(compilation) {
        let source = compilation.graph().get(module).path.clone();
        let destination = output_path(project, &source, renderer);
        let directory = destination
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| project.output_dir.clone());
        let contents = render::render_module(compilation, module, renderer, &directory, Some(&compilation.project.output_dir));
        files.push(OutputFile {
            path: module::relative_path(&project.root, &destination),
            contents,
            kind: OutputKind::Reference,
            target: renderer.as_str(),
            origin: project::display(&source, &project.root),
            sources: vec![source],
        });
    }
    files
}

/// The modules a build emits: the entry, and every module that declares
/// something the entry's exports reach, so each reference has a file to land
/// in. Bundled packages live in the compiler and are never written out.
fn emitted_modules(compilation: &Compilation) -> Vec<ModuleId> {
    let reachability = reach::from_entry(compilation);
    let mut modules: BTreeSet<ModuleId> = BTreeSet::new();
    modules.insert(compilation.resolution.entry);
    for reached in &reachability.reached {
        modules.insert(compilation.store().anchor(reached.anchor).module);
    }
    modules
        .into_iter()
        .filter(|module| !compilation.graph().get(*module).is_package())
        .collect()
}

/// Where a source file's rendered output goes: its place under the source
/// root, inside the output directory, with the renderer's extension. A file
/// outside the source root -- an installed package -- keeps its place under
/// the project root instead, so it cannot land on top of a project file.
fn output_path(project: &piton_compile::Project, source: &Path, renderer: Adapter) -> PathBuf {
    let relative = source
        .strip_prefix(&project.source_root)
        .or_else(|_| source.strip_prefix(&project.root))
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| {
            source
                .file_name()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("index.pi"))
        });
    project
        .output_dir
        .join(relative)
        .with_extension(renderer.extension())
}
