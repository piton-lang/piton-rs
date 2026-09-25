//! `piton update` — refresh installed packages from the configured dependencies.
//!
//! Update reads `piton.config.pi`, not the tethers directory: the dependency
//! list is what the project asked for, and an installed package that nothing
//! asks for any more is `piton remove`'s business, not this command's.
//!
//! Nothing is replaced without first checking that what is on disk is still
//! what was installed. An edited package is carrying work, and the only safe
//! answers are to leave it alone and say so.

use piton_compile::packages::Lock;

use anstream::println;

use crate::style::{self, paint};
use crate::commands::tether::{fail, report_config_errors};
use crate::packages::Installer;
use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

pub fn run(requested: &[String]) -> u8 {
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
    for dependency in &wanted {
        if let Err(failure) = installer.add(dependency, None) {
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
        } else {
            println!(
                "{} {}",
                paint(style::NAME, &package.name),
                paint(style::DIM, format!("is already at {}", package.pin))
            );
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
