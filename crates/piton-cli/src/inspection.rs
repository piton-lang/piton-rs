//! Inspects a project before a command runs.
//!
//! Some plugins have an inspect hook, for example the bundled migrate plugin.
//! They examine the project before each command. They tell which parts are
//! out of date, and give the edits that correct them. Only the user can apply
//! a change. On a terminal, the CLI shows the edits and asks the user one time.
//! This includes `piton check`. In other places, it only reports them with the
//! command that applies them.

use std::io::IsTerminal;
use std::rc::Rc;

use piton_compile::Project;
use piton_plugin::inspection::{self, Finding};
use piton_plugin::Manifest;

use crate::packages::FileChange;
use crate::plugins::CliServices;
use crate::style::{self, paint};
use crate::{diff, report};

/// The CLI sets this on a command that runs again after an update to its
/// project. Thus the second run does not inspect and ask again.
const RERUN: &str = "PITON_INSPECTED";

/// The result of an offer.
pub enum Offer {
    /// The command runs as the user asked.
    Run,
    /// The command ran again on the updated project, with this exit code.
    Ran(u8),
    /// The user agreed to the findings before a command that writes no file,
    /// for example `piton check`. The command runs on the project as it is.
    /// Then the CLI applies these findings with [`apply`].
    After(Vec<Finding>),
}

/// Runs all inspections, and offers their findings. `skip` is the plugin that
/// owns the command that runs now. That plugin uses its own findings. When
/// `read_only` is true, the command writes no file, for example `piton check`.
/// The CLI still asks the user. But if the user agrees, the command first runs
/// on the project as it is. The CLI applies the edits after the command ends.
pub fn offer(project: &Project, manifests: &[Manifest], skip: Option<&str>, read_only: bool) -> Offer {
    if project.config_path.is_none() || std::env::var_os(RERUN).is_some() {
        return Offer::Run;
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
        return Offer::Run;
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
        return Offer::Run;
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
        return Offer::Run;
    }
    let findings: Vec<Finding> = found.into_iter().map(|(_, finding)| finding).collect();
    if read_only {
        return Offer::After(findings);
    }
    if !apply(project, &findings) {
        return Offer::Ran(crate::EXIT_ERRORS);
    }
    report::done("updated the project; running the command again");
    report::flush();

    // The command runs again from the start. Thus it reads the updated project
    // for all data, and it reads the configuration first.
    let Ok(program) = std::env::current_exe() else {
        return Offer::Run;
    };
    match std::process::Command::new(program)
        .args(std::env::args_os().skip(1))
        .env(RERUN, "1")
        .status()
    {
        Ok(status) => Offer::Ran(status.code().unwrap_or(1).clamp(0, 255) as u8),
        Err(_) => Offer::Run,
    }
}

/// Writes the edits of each finding, and reports each changed path. It
/// returns false when a write failed.
pub fn apply(project: &Project, findings: &[Finding]) -> bool {
    for finding in findings {
        match inspection::apply(&finding.edits, &project.root) {
            Ok(paths) => {
                for path in paths {
                    anstream::eprintln!("{}", report::added(path.display()));
                }
            }
            Err(error) => {
                report::fail(format!("cannot write the update: {error}"));
                return false;
            }
        }
    }
    true
}
