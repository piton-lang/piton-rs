//! `piton reach` — report the parts that the entrypoints can see and cannot see.
//!
//! The report shows the reachable parts, the unreachable parts, the depth of
//! each reachable anchor, and the path to it. `--no-paths` and
//! `--no-unreachable` remove those parts. The traversal follows imports,
//! references, inheritance, and composition.

use std::path::{Path, PathBuf};

use piton_compile::{reach, AnchorDef, Compilation, ModuleId, Symbol};

use anstream::println;

use crate::style::{self, paint};
use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

/// The item that a command-line target names.
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
        // Without a configuration, the working directory is the project. A
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

    // Without targets, the roots of the project are the entry exports and
    // the anchors that plugins reach. An example is the tests that the test
    // plugin runs.
    let plugin_roots: Vec<(PathBuf, String)> = if targets.is_empty() {
        plugin_roots(&project)
    } else {
        Vec::new()
    };

    let mut compilation = project::compile_project(project.clone());
    // A file that no file imports is not in the graph of the project. But a
    // user can ask about it. Thus compile the workspace with that file.
    if files
        .iter()
        .any(|file| compilation.graph().id_for(file).is_none())
    {
        compilation =
            Compilation::build_workspace(project.clone(), Default::default(), &files);
    }
    // A file that no file imports can contain an anchor that a plugin reaches.
    // Thus compile it with the project, as if a file imports it. Then its
    // anchors can be roots. Other files without imports stay "never imported".
    let mut holding: Vec<PathBuf> = Vec::new();
    for (file, _) in &plugin_roots {
        let canonical = project::canonical_target(file);
        if compilation.graph().id_for(&canonical).is_none() && !holding.contains(&canonical) {
            holding.push(canonical);
        }
    }
    if !holding.is_empty() {
        compilation = Compilation::build_including(project, &holding);
    }

    let reachability = if targets.is_empty() {
        let extra = reach::locate(&compilation, &plugin_roots);
        reach::from_entry_with_imports_and(&compilation, &extra)
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

    // The groups are by the source that declares each anchor.
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
            // No path reached a root. The walk starts at a root.
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

        // The groups are by source, the same as the reachable half. Thus a
        // file with only dead anchors shows as one entry. It does not show as
        // a list of names and then again as the file at the bottom.
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
        // A file can have no reachable parts and also no declarations. An
        // example is an index with only dead exports. The report must name
        // that file.
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
            // No file imports these files. Thus the compilation never opened
            // them. The parser reads them only to count their declarations.
            let detail = format!(
                "(never imported, {} {})",
                module.anchors,
                report::plural(module.anchors, "anchor", "anchors"),
            );
            println!(
                "  {}  {}",
                paint(style::WARNING, project::display(&module.path, &root)),
                paint(style::DIM, detail)
            );
        }
    }

    // This code counts each line above one time. Anchors and modules have
    // separate counts because they are different questions. A dead anchor is
    // a declaration to delete. A dead module is a file to delete. The compiler
    // never read a file that no file imports. Its anchors still count as unreachable.
    // Thus the headline cannot show "clean" when the list shows dead files.
    let mut anchors = format!(
        "{} {} reachable, {} unreachable",
        reachability.reached.len(),
        report::plural(reachability.reached.len(), "anchor", "anchors"),
        reachability.unreachable_anchors(),
    );
    let never_imported = reachability.unloaded_modules.len();
    if never_imported > 0 {
        anchors.push_str(&format!(
            " ({} in {never_imported} never-imported {})",
            reachability.unloaded_anchors(),
            report::plural(never_imported, "file", "files"),
        ));
    }
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

/// All anchors that a module declares. The walk of a file starts at them.
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

/// The anchors that the plugins of the project reach, by file and name. When
/// a plugin cannot tell, a warning reports it. The other plugins still count.
fn plugin_roots(project: &piton_compile::Project) -> Vec<(PathBuf, String)> {
    let (manifests, _) = piton_plugin::discover(project);
    let (roots, failures) = piton_plugin::reach::roots(project, &manifests, || {
        std::rc::Rc::new(crate::plugins::CliServices { project: project.clone() })
    });
    for failure in failures {
        report::warn(failure);
    }
    roots.into_iter().map(|root| (root.file, root.name)).collect()
}
