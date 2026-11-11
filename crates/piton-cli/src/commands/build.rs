//! `piton build` — compile the project and write every configured artifact.
//!
//! Without a plugin that builds, a build writes the renderer's output to the
//! configured output directory: the entry file, and every file holding
//! something the entry's exports reference, each keeping its place under the
//! source root and taking the renderer's extension. A plugin's build, like
//! Belay's, replaces it, unless the configuration sets `output` or `renderer`
//! to ask for both.
//!
//! The plan is built and validated in full before anything is written, so a
//! project either produces a consistent set of artifacts or produces
//! diagnostics and no partial output.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use piton_compile::{module, reach, Compilation, ModuleId};
use piton_emit::Adapter;
use piton_plugin::output::{self, BuildPlan, OutputFile};

use anstream::println;

use crate::style::{self, paint};
use crate::{project, render, report, EXIT_ERRORS, EXIT_SUCCESS};

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
        report::help("set `entry` in piton.config.pi, or add an index.pi to the root");
        return EXIT_ERRORS;
    }

    let renderer = match render::parse_renderer(&compilation.project.renderer) {
        Ok(renderer) => renderer,
        Err(message) => {
            report::fail(message);
            return EXIT_ERRORS;
        }
    };

    // Every plugin with a build plans its share; none writes anything.
    let (_, _, manifests, _) = crate::plugins::load();
    let builders = piton_plugin::builders(&compilation.project, &manifests);
    let (plugin_plans, failures) = crate::plugins::plan_builds(&compilation.project, &manifests);
    for failure in &failures {
        report::fail(failure);
    }
    if !failures.is_empty() {
        report::stopped("build stopped; no files were written");
        return EXIT_ERRORS;
    }

    // A plugin's build replaces the renderer's output, unless the
    // configuration asks for the renderer's too.
    let mut plan = BuildPlan {
        files: if builders.is_empty() || compilation.project.renders {
            rendered_files(&compilation, renderer)
        } else {
            Vec::new()
        },
        targets: Vec::new(),
    };
    for plugin_plan in plugin_plans {
        plan.files.extend(plugin_plan.files);
        plan.targets.extend(plugin_plan.targets);
        diagnostics.extend(plugin_plan.diagnostics);
    }

    // The whole plan is checked before anything is written: paths stay in
    // the project, inside what each plugin may write, producers don't
    // collide, and nothing the build didn't write is overwritten.
    let previous = output::previous(&root);
    let outputs = |owner: &str| {
        manifests
            .iter()
            .find(|manifest| manifest.name == owner)
            .map(|manifest| manifest.capabilities.output_globs())
    };
    for problem in output::check(&plan, &root, &previous, &outputs) {
        diagnostics.push(piton_core::Diagnostic::error(
            "output-collision",
            problem,
            compilation.project.config_path.clone().unwrap_or_default(),
            piton_core::Span::default(),
        ));
    }

    diagnostics.sort();
    let had_errors = report::diagnostics(
        &diagnostics,
        &|path| compilation.source_of(path).map(str::to_string),
        &root,
    );

    if had_errors {
        report::summary(&diagnostics);
        report::stopped("build stopped; no files were written");
        return EXIT_ERRORS;
    }
    // A clean build says so in its outcome; the count is only for problems.
    if !diagnostics.is_empty() {
        report::summary(&diagnostics);
    }

    if dry_run {
        for file in &plan.files {
            println!(
                "{} {}",
                report::added(&file.path),
                paint(style::DIM, format!("({})", file.label))
            );
        }
        report::done(format!(
            "{} {} would be written",
            plan.files.len(),
            report::plural(plan.files.len(), "file", "files")
        ));
        return EXIT_SUCCESS;
    }

    let manifest_path = root.join(output::MANIFEST);
    let written = match output::write(&plan, &root) {
        Ok(written) => written,
        Err(error) => {
            report::fail(format!("cannot write output: {error}"));
            return EXIT_ERRORS;
        }
    };

    let removed = output::clean_stale(&previous, &plan, &root);

    if let Some(parent) = manifest_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(error) = std::fs::write(&manifest_path, output::manifest(&plan)) {
        report::fail(format!("cannot write the build manifest: {error}"));
        return EXIT_ERRORS;
    }

    for path in &written {
        println!("{}", report::added(path.display()));
    }
    for path in &removed {
        println!("{}", report::removed(path.display()));
    }
    let mut outcome = format!(
        "wrote {} {}",
        written.len(),
        report::plural(written.len(), "file", "files")
    );
    if !removed.is_empty() {
        outcome.push_str(&format!(", removed {}", removed.len()));
    }
    report::done(outcome);
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
            path: output::slash(&module::relative_path(&project.root, &destination)),
            data: contents.into_bytes(),
            executable: false,
            origin: project::display(&source, &project.root),
            owner: "renderer".to_string(),
            label: format!("reference, {}", renderer.as_str()),
            group: None,
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
