//! `piton reach` — report what the entrypoints can and cannot see.
//!
//! The report covers what is reachable, what is unreachable, the depth of each
//! reachable anchor, and the path taken to it; `--no-paths` and
//! `--no-unreachable` leave those parts out. The traversal follows imports,
//! references, inheritance, and composition.

use std::path::{Path, PathBuf};

use piton_compile::{reach, AnchorDef, Compilation, ModuleId, Symbol};

use anstream::println;

use crate::style::{self, paint};
use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

/// What a command-line target names.
enum Target {
    Files(Vec<PathBuf>),
    Anchor(String),
}

fn classify(target: &str) -> Result<Target, String> {
    if target.contains('*') || target.contains('?') || target.contains('[') {
        let files = project::expand_glob(target)?;
        if files.is_empty() {
            return Err(format!("`{target}` matched no .pi files"));
        }
        return Ok(Target::Files(
            files.iter().map(|file| project::canonical_target(file)).collect(),
        ));
    }
    let path = project::canonical_target(Path::new(target));
    if path.is_file() {
        return Ok(Target::Files(vec![path]));
    }
    if path.is_dir() {
        let files = project::walk(&path);
        return Ok(Target::Files(files));
    }
    Ok(Target::Anchor(target.to_string()))
}

pub fn run(targets: &[String], show_unreachable: bool, show_paths: bool) -> u8 {
    let (mut project, _) = project::current();

    let mut files: Vec<PathBuf> = Vec::new();
    let mut anchors: Vec<String> = Vec::new();
    for target in targets {
        match classify(target) {
            Ok(Target::Files(found)) => files.extend(found),
            Ok(Target::Anchor(name)) => anchors.push(name),
            Err(message) => {
                report::fail(message);
                return EXIT_ERRORS;
            }
        }
    }

    if project.config_path.is_none() && targets.is_empty() {
        // Without a configuration the working directory is the project, and a
        // directory is a module through its index.
        let index = project.root.join("index.pi");
        if !index.is_file() {
            report::fail(
                "no piton.config.pi or index.pi found; name a file, glob, or anchor to start from",
            );
            return EXIT_ERRORS;
        }
        project = project.with_entry(&index);
    }
    let root = project.root.clone();

    let mut compilation = project::compile_project(project.clone());
    // A file nothing imports is not in the project's graph, but it can still
    // be asked about: compile the workspace with it included.
    if files
        .iter()
        .any(|file| compilation.graph().id_for(file).is_none())
    {
        compilation =
            Compilation::build_workspace(project, Default::default(), &files);
    }

    let reachability = if targets.is_empty() {
        reach::from_entry_with_imports(&compilation)
    } else {
        let mut modules: Vec<ModuleId> = Vec::new();
        let mut roots = Vec::new();
        for file in &files {
            match compilation.graph().id_for(file) {
                Some(module) => {
                    modules.push(module);
                    roots.extend(declared_anchors(&compilation, module));
                }
                None => {
                    report::fail(format!("cannot read `{}`", file.display()));
                    return EXIT_ERRORS;
                }
            }
        }
        for name in &anchors {
            match compilation.find_anchor(name) {
                Some(anchor) => roots.push(anchor),
                None => {
                    report::fail(format!("no file, glob, or anchor named `{name}`"));
                    return EXIT_ERRORS;
                }
            }
        }
        reach::from_roots_with_imports(&compilation, &roots, &modules)
    };

    // Grouped by the source that declares each anchor.
    let mut by_source: std::collections::BTreeMap<String, Vec<&reach::Reached>> =
        Default::default();
    for entry in &reachability.reached {
        let path = compilation.anchor_module_path(entry.anchor);
        by_source
            .entry(project::display(&path, &root))
            .or_default()
            .push(entry);
    }

    println!("{}", paint(style::HEADING, "reachable"));
    for (source, mut entries) in by_source {
        entries.sort_by_key(|entry| entry.depth);
        println!("  {}", paint(style::DIM, &source));
        for entry in entries {
            let name = compilation.store().anchor(entry.anchor).name.clone();
            // A root was not reached from anywhere; it is where the reading
            // starts.
            let via = match entry.via {
                Some(kind) => format!("via {}", kind.as_str()),
                None => "root".to_string(),
            };
            let mut detail = format!("depth {}  {via}", entry.depth);
            if show_paths {
                detail.push_str(&format!("  [{}]", entry.path.join(" -> ")));
            }
            println!("    {}  {}", paint(style::NAME, &name), paint(style::DIM, detail));
        }
    }

    if show_unreachable {
        println!("\n{}", paint(style::WARNING, "unreachable"));
        if reachability.unreachable.is_empty()
            && reachability.unreachable_modules.is_empty()
            && reachability.unloaded_modules.is_empty()
        {
            println!("  {}", paint(style::SUCCESS, "nothing"));
        }

        // Grouped by source, the same as the reachable half, so a file whose
        // every anchor is dead reads as one entry rather than a list of names
        // and then the file again at the bottom.
        let dead_files: std::collections::BTreeSet<String> = reachability
            .unreachable_modules
            .iter()
            .map(|module| project::display(module, &root))
            .collect();

        let mut by_source: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        for anchor in &reachability.unreachable {
            let def = compilation.store().anchor(*anchor);
            let path = compilation.anchor_module_path(*anchor);
            by_source
                .entry(project::display(&path, &root))
                .or_default()
                .push(def.name.clone());
        }
        // A file that carries nothing reachable but declares nothing either --
        // an index whose every export is dead -- still has to be named.
        for file in &dead_files {
            by_source.entry(file.clone()).or_default();
        }

        for (source, names) in by_source {
            if dead_files.contains(&source) {
                println!(
                    "  {}  {}",
                    paint(style::WARNING, &source),
                    paint(style::DIM, "(nothing here is reached)")
                );
            } else {
                println!("  {}", paint(style::DIM, &source));
            }
            for name in names {
                println!("    {}", paint(style::WARNING, name));
            }
        }

        for module in &reachability.unloaded_modules {
            // Nothing imports these, so compilation never opened them.
            println!(
                "  {}  {}",
                paint(style::WARNING, project::display(module, &root)),
                paint(style::DIM, "(never imported)")
            );
        }
    }

    // Every line printed above is counted here, and counted once. Anchors and
    // modules are tallied apart because they are different questions: a dead
    // anchor is something to delete, a dead module is a file to delete, and a
    // file nothing imports was never read at all.
    let anchors = format!(
        "{} {} reachable, {} unreachable",
        reachability.reached.len(),
        report::plural(reachability.reached.len(), "anchor", "anchors"),
        reachability.unreachable.len(),
    );
    let modules = format!(
        "{} {} loaded, {} carrying nothing reachable, {} never imported",
        reachability.loaded,
        report::plural(reachability.loaded, "module", "modules"),
        reachability.unreachable_modules.len(),
        reachability.unloaded_modules.len(),
    );
    println!("\n{}", paint(style::HEADING, anchors));
    println!("{}", paint(style::HEADING, modules));
    EXIT_SUCCESS
}

/// Every anchor a module declares, which is where reading a file starts.
fn declared_anchors(compilation: &Compilation, module: ModuleId) -> Vec<piton_core::AnchorId> {
    compilation
        .resolution
        .scope(module)
        .declarations
        .values()
        .filter_map(|symbol| match symbol {
            Symbol::Anchor(anchor) => Some(*anchor),
            _ => None,
        })
        .filter(|anchor| {
            let def: &AnchorDef = compilation.store().anchor(*anchor);
            def.module == module
        })
        .collect()
}
