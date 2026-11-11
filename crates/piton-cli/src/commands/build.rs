//! `piton build` — compile the project and write all configured artifacts.
//!
//! When no plugin builds, a build writes the renderer output to the output
//! directory in the configuration. It writes the entry file. It also writes
//! each file that declares a reference target of the entry exports. Each file
//! keeps its place under the source root and gets the renderer extension. A
//! plugin build, for example the Belay build, replaces this output. If the
//! configuration sets `output` or `renderer`, the build writes the two outputs.
//!
//! The build makes and validates the full plan before it writes a file. Thus
//! a project gives a consistent set of artifacts, or it gives diagnostics and
//! no partial output.

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

    // Each plugin with a build plans its part. No plugin writes a file.
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

    // A plugin build replaces the renderer output. If the configuration asks
    // for the renderer output, the build also writes it.
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

    // The build checks the full plan before it writes a file. Paths must stay
    // in the project. Each path must be in the area that its plugin can write.
    // Two producers must not write the same path. The build must not overwrite
    // a file that it did not write.
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
    // A clean build tells this in its outcome. The count is only for problems.
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

/// The renderer part of the build. It is one file for each emitted module.
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

/// The modules that a build emits. These are the entry, and each module that
/// declares a target of the entry exports. Thus each reference has a file to
/// go to. Bundled packages are in the compiler, and the build never writes them.
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

/// The location of the rendered output of a source file. It is the place of
/// the file under the source root, in the output directory, with the renderer
/// extension. A file outside the source root is an installed package. It keeps
/// its place under the project root. Thus it cannot replace a project file.
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
