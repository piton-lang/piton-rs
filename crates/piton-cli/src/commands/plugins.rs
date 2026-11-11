//! `piton plugins` — the plugins a project has and what each may do.

use anstream::println;

use crate::plugins;
use crate::style::{self, paint};
use crate::{project, report, EXIT_SUCCESS};

pub fn run() -> u8 {
    let (project, config_diagnostics, mut manifests, mut problems) = plugins::load();
    plugins::withhold_builtins(&<crate::Cli as clap::CommandFactory>::command(), &mut manifests, &mut problems);
    report::diagnostics(
        &config_diagnostics,
        &|path| std::fs::read_to_string(path).ok(),
        &project.root,
    );
    plugins::report_problems(&project, &manifests, &problems);

    for (index, manifest) in manifests.iter().enumerate() {
        if index > 0 {
            println!();
        }
        let origin = if manifest.bundled {
            "bundled".to_string()
        } else {
            project::display(&manifest.declared_in, &project.root)
        };
        println!(
            "{} {}",
            paint(style::HEADING, &manifest.name),
            paint(style::DIM, format!("({origin})"))
        );
        if let Some(description) = &manifest.description {
            println!("  {description}");
        }
        if !manifest.commands.is_empty() {
            let names: Vec<String> = manifest
                .commands
                .iter()
                .map(|command| format!("piton {}", command.name))
                .collect();
            println!("  {} {}", paint(style::DIM, "commands:"), paint(style::NAME, names.join(", ")));
        }
        if let Some(settings) = &manifest.settings {
            let package = manifest
                .module_root
                .as_deref()
                .map(|root| format!(" from {root}"))
                .unwrap_or_default();
            println!("  {} {settings}{package}", paint(style::DIM, "settings:"));
        }
        let capabilities = &manifest.capabilities;
        let mut may = Vec::new();
        if !capabilities.process.is_empty() {
            may.push(format!("run {}", capabilities.process.join(", ")));
        }
        if capabilities.read_files {
            may.push("read files in the project".to_string());
        }
        if capabilities.temporary_files {
            may.push("write temporary files".to_string());
        }
        if may.is_empty() {
            may.push("nothing beyond reading the compiled project".to_string());
        }
        println!("  {} {}", paint(style::DIM, "may:"), may.join("; "));
    }
    report::done(format!(
        "{} {}",
        manifests.len(),
        report::plural(manifests.len(), "plugin", "plugins")
    ));
    EXIT_SUCCESS
}
