//! `piton reach` — report what the entrypoints can and cannot see.

use std::path::PathBuf;

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
            let path = PathBuf::from(target);
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
            if show_paths {
                println!(
                    "    {name}  depth {}  via {}  [{}]",
                    entry.depth,
                    entry.via.as_str(),
                    entry.path.join(" -> ")
                );
            } else {
                println!("    {name}  depth {}  via {}", entry.depth, entry.via.as_str());
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
        for anchor in &reachability.unreachable {
            let def = compilation.store().anchor(*anchor);
            let path = compilation.anchor_module_path(*anchor);
            println!("  {}  {}", def.name, project::display(&path, &root));
        }
        for module in &reachability.unreachable_modules {
            println!("  (module) {}", project::display(module, &root));
        }
        for module in &reachability.unloaded_modules {
            // Nothing imports these, so compilation never opened them.
            println!("  (never imported) {}", project::display(module, &root));
        }
    }

    println!(
        "\n{} reachable, {} unreachable, {} never imported, {} {} loaded",
        reachability.reached.len(),
        reachability.unreachable.len(),
        reachability.unloaded_modules.len(),
        reachability.modules.len(),
        report::plural(reachability.modules.len(), "module", "modules")
    );
    EXIT_SUCCESS
}
