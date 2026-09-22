//! `piton reach` — report what the entrypoints can and cannot see.

use std::path::Path;

use piton_compile::reach;

use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

pub fn run(targets: &[String], show_unreachable: bool, show_paths: bool) -> u8 {
    let (project, _) = project::current();
    if project.config_path.is_none() && targets.is_empty() {
        report::fail("no piton.config.pi found; name a file or anchor to start from");
        return EXIT_ERRORS;
    }
    let root = project.root.clone();
    let compilation = project::compile_project(project);

    let reachability = if targets.is_empty() {
        reach::from_entry(&compilation)
    } else {
        let mut roots = Vec::new();
        for target in targets {
            let path = project::canonical_target(Path::new(target));
            if path.is_file() {
                match compilation.graph().id_for(&path) {
                    Some(module) => {
                        let module_reach = reach::from_module(&compilation, module);
                        roots.extend(module_reach.reached.iter().map(|r| r.anchor));
                        continue;
                    }
                    None => {
                        report::fail(format!(
                            "`{target}` is not part of the project; nothing imports it"
                        ));
                        return EXIT_ERRORS;
                    }
                }
            }
            match compilation.find_anchor(target) {
                Some(anchor) => roots.push(anchor),
                None => {
                    report::fail(format!("no file or anchor named `{target}`"));
                    return EXIT_ERRORS;
                }
            }
        }
        reach::from_roots(&compilation, &roots)
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

    println!("reachable");
    for (source, mut entries) in by_source {
        entries.sort_by_key(|entry| entry.depth);
        println!("  {source}");
        for entry in entries {
            let name = compilation.store().anchor(entry.anchor).name.clone();
            // A root was not reached from anywhere; it is where the reading
            // starts.
            let via = match entry.via {
                Some(kind) => format!("via {}", kind.as_str()),
                None => "root".to_string(),
            };
            if show_paths {
                println!(
                    "    {name}  depth {}  {via}  [{}]",
                    entry.depth,
                    entry.path.join(" -> ")
                );
            } else {
                println!("    {name}  depth {}  {via}", entry.depth);
            }
        }
    }

    if show_unreachable {
        println!("\nunreachable");
        if reachability.unreachable.is_empty()
            && reachability.unreachable_modules.is_empty()
            && reachability.unloaded_modules.is_empty()
        {
            println!("  nothing");
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
                println!("  {source}  (nothing here is reached)");
            } else {
                println!("  {source}");
            }
            for name in names {
                println!("    {name}");
            }
        }

        for module in &reachability.unloaded_modules {
            // Nothing imports these, so compilation never opened them.
            println!("  {}  (never imported)", project::display(module, &root));
        }
    }

    // Every line printed above is counted here, and counted once. Anchors and
    // modules are tallied apart because they are different questions: a dead
    // anchor is something to delete, a dead module is a file to delete, and a
    // file nothing imports was never read at all.
    println!(
        "\n{} {} reachable, {} unreachable",
        reachability.reached.len(),
        report::plural(reachability.reached.len(), "anchor", "anchors"),
        reachability.unreachable.len(),
    );
    println!(
        "{} {} loaded, {} carrying nothing reachable, {} never imported",
        reachability.loaded,
        report::plural(reachability.loaded, "module", "modules"),
        reachability.unreachable_modules.len(),
        reachability.unloaded_modules.len(),
    );
    EXIT_SUCCESS
}
