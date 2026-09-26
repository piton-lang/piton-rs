//! `piton update` — refresh installed packages from the configured dependencies.
//!
//! Update reads `piton.config.pi`, not the tethers directory: the dependency
//! list is what the project asked for, and an installed package that nothing
//! asks for any more is `piton remove`'s business, not this command's.
//!
//! Nothing is replaced without first checking that what is on disk is still
//! what was installed. An edited package is carrying work, and the only safe
//! answers are to leave it alone and say so -- unless `--force` says to throw
//! the work away, and then what was thrown away is still said.
//!
//! `--diff` shows what each update changed, file by file.

use piton_compile::packages::Lock;

use anstream::println;

use crate::style::{self, paint};
use crate::commands::tether::{fail, report_config_errors};
use crate::packages::Installer;
use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

pub fn run(requested: &[String], force: bool, diff: bool) -> u8 {
    let (configured, diagnostics) = project::current();
    if report_config_errors(&configured, &diagnostics) {
        return EXIT_ERRORS;
    }

    if configured.config_path.is_none() {
        report::fail("no piton.config.pi, so there is no dependency list to update from");
        return EXIT_ERRORS;
    }

    let lock = Lock::load(&configured.root);

    // A name on the command line selects a package, and a package is selected
    // by what it installed as, not by the repository it came from -- that is
    // the name the user sees in `tethers/`.
    let wanted: Vec<_> = configured
        .dependencies
        .iter()
        .filter(|dependency| {
            if requested.is_empty() {
                return true;
            }
            requested.iter().any(|name| {
                *name == dependency.source
                    || *name == dependency.default_name()
                    || lock
                        .packages
                        .iter()
                        .any(|entry| entry.name == *name && entry.source == dependency.source)
            })
        })
        .cloned()
        .collect();

    if wanted.is_empty() {
        if configured.dependencies.is_empty() {
            println!("no dependencies are declared in piton.config.pi");
            return EXIT_SUCCESS;
        }
        report::fail(format!(
            "no declared dependency matches {}",
            requested.join(", ")
        ));
        report::help(format_args!("declared dependencies are {}", configured
                .dependencies
                .iter()
                .map(|dependency| dependency.source.clone())
                .collect::<Vec<_>>()
                .join(", ")));
        return EXIT_ERRORS;
    }

    let before = lock.clone();
    let mut installer = Installer::new(&configured.root, lock);
    installer.force = force;
    installer.diff = diff;
    for dependency in &wanted {
        if let Err(failure) = installer.add(dependency, None) {
            // An update refused because of local edits still shows them.
            for (name, edits) in &installer.edited {
                match edits {
                    Ok(changes) => {
                        let files = changes.len();
                        println!(
                            "{} {} {}",
                            paint(style::WARNING, "edited"),
                            paint(style::NAME, name),
                            paint(
                                style::DIM,
                                format!(
                                    "({files} {} changed since it was installed)",
                                    report::plural(files, "file", "files")
                                )
                            )
                        );
                        crate::diff::print(&format!("tethers/{name}"), changes);
                    }
                    Err(problem) => report::warn(format_args!(
                        "cannot show the local changes to `{name}`: {}",
                        problem.message
                    )),
                }
            }
            let _ = std::fs::remove_dir_all(&installer.scratch);
            if !installer.edited.is_empty() {
                report::stopped("nothing was updated");
            }
            return fail(failure);
        }
    }
    let (lock, placed, warnings) = match installer.finish() {
        Ok(result) => result,
        Err(failure) => return fail(failure),
    };

    if let Err(error) = lock.save(&configured.root) {
        report::fail(format!("cannot write the package lock file: {error}"));
        return EXIT_ERRORS;
    }

    for warning in &warnings {
        report::warn(format_args!("{warning}"));
    }

    let mut changed = 0usize;
    for package in &placed {
        let was = before.get(&package.name);
        let moved = was.is_none_or(|entry| entry.commit != package.commit);
        if moved {
            changed += 1;
            let from = was
                .and_then(|entry| entry.commit.as_deref())
                .map(short)
                .unwrap_or_else(|| "nothing".to_string());
            println!(
                "{} {} from {from} to {}",
                paint(style::SUCCESS, "updated"),
                paint(style::NAME, &package.name),
                package.commit.as_deref().map(short).unwrap_or_else(|| "?".to_string())
            );
        } else if package.discarded.is_some() {
            changed += 1;
            println!(
                "{} {} to {}",
                paint(style::SUCCESS, "restored"),
                paint(style::NAME, &package.name),
                package.pin
            );
        } else {
            println!(
                "{} {}",
                paint(style::NAME, &package.name),
                paint(style::DIM, format!("is already at {}", package.pin))
            );
        }
        if diff {
            // Said even when nothing changed, so an empty diff reads as an
            // answer rather than as a flag that did nothing.
            let files = package.changes.len();
            println!(
                "  {}",
                paint(
                    style::DIM,
                    match files {
                        0 => "no files changed".to_string(),
                        _ => format!("{files} {} changed:", report::plural(files, "file", "files")),
                    }
                )
            );
            crate::diff::print(&format!("tethers/{}", package.name), &package.changes);
        }
        if let Some(discarded) = &package.discarded {
            report::warn(format_args!(
                "discarded the local changes to `{}` ({discarded})",
                package.name
            ));
        }
    }
    let outcome = format!(
        "{} of {} {} changed",
        changed,
        placed.len(),
        report::plural(placed.len(), "package", "packages")
    );
    println!("{}", paint(style::HEADING, outcome));

    EXIT_SUCCESS
}

/// A commit, shortened the way git shortens one.
fn short(commit: &str) -> String {
    commit.chars().take(8).collect()
}
