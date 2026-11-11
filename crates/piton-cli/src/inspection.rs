//! Inspecting a project before a command runs.
//!
//! Plugins with an inspect hook, like the bundled migrate plugin, look at the
//! project before each command and say what is out of date, with the edits
//! that would fix it. Nothing changes without the user: on a terminal the
//! edits are shown and the user is asked once; anywhere else they are only
//! reported, with the command that applies them.

use std::io::IsTerminal;
use std::rc::Rc;

use piton_compile::Project;
use piton_plugin::inspection::{self, Finding};
use piton_plugin::Manifest;

use crate::packages::FileChange;
use crate::plugins::CliServices;
use crate::style::{self, paint};
use crate::{diff, report};

/// Set on a command run again after its project was updated, so the run
/// doesn't inspect and ask a second time.
const RERUN: &str = "PITON_INSPECTED";

/// Runs every inspection, and offers what they find. Returns the exit code
/// when the command was run again against an updated project; None when the
/// command should run as asked. `skip` is the plugin whose own command is
/// running, which deals with its findings itself.
pub fn offer(project: &Project, manifests: &[Manifest], skip: Option<&str>) -> Option<u8> {
    if project.config_path.is_none() || std::env::var_os(RERUN).is_some() {
        return None;
    }
    let mut found: Vec<(&Manifest, Finding)> = Vec::new();
    for manifest in manifests.iter().filter(|manifest| manifest.inspect && Some(manifest.name.as_str()) != skip) {
        let services = Rc::new(CliServices { project: project.clone() });
        match piton_plugin::run_inspect(manifests, &manifest.name, services) {
            Ok(result) => match inspection::read(manifest, &result, &project.root) {
                Ok(findings) => found.extend(findings.into_iter().map(|finding| (manifest, finding))),
                Err(message) => report::warn(format!("plugin `{}` found something it can't offer: {message}", manifest.name)),
            },
            Err(failure) => report::warn(format!("plugin `{}` could not inspect the project: {failure}", manifest.name)),
        }
    }
    if found.is_empty() {
        return None;
    }

    let fixable = found.iter().any(|(_, finding)| !finding.edits.is_empty());
    let interactive = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    if !interactive || !fixable {
        for (manifest, finding) in &found {
            report::warn(&finding.message);
            match manifest.commands.first() {
                Some(command) if !finding.edits.is_empty() => report::help(format!("run `piton {}` to update it", command.name)),
                _ if finding.edits.is_empty() => report::help("this one has to be changed by hand"),
                _ => {}
            }
        }
        return None;
    }

    anstream::eprintln!("{}", paint(style::HEADING, "This project was written for an older Piton:"));
    for (_, finding) in &found {
        anstream::eprintln!("  {} {}", paint(style::WARNING, "!"), finding.message);
    }
    let changes: Vec<FileChange> = found
        .iter()
        .flat_map(|(_, finding)| finding.edits.iter())
        .map(|edit| FileChange {
            path: edit.path.clone(),
            before: edit.before.clone(),
            after: Some(edit.after.clone()),
        })
        .collect();
    diff::eprint(".", &changes);
    anstream::eprintln!();
    let agreed = inquire::Confirm::new("Switch it over?")
        .with_default(true)
        .prompt()
        .unwrap_or(false);
    if !agreed {
        return None;
    }

    let root = &project.root;
    for (_, finding) in &found {
        match inspection::apply(&finding.edits, root) {
            Ok(paths) => {
                for path in paths {
                    anstream::eprintln!("{}", report::added(path.display()));
                }
            }
            Err(error) => {
                report::fail(format!("cannot write the update: {error}"));
                return Some(crate::EXIT_ERRORS);
            }
        }
    }
    report::done("updated the project; running the command again");
    report::flush();

    // The command runs again from the start, so everything it reads, the
    // configuration first, is the updated project.
    let program = std::env::current_exe().ok()?;
    let status = std::process::Command::new(program)
        .args(std::env::args_os().skip(1))
        .env(RERUN, "1")
        .status()
        .ok()?;
    Some(status.code().unwrap_or(1).clamp(0, 255) as u8)
}
