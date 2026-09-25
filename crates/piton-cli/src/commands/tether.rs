//! `piton tether` — clone repositories into the tethers directory.
//!
//! Tethering is how a dependency arrives. The repository is cloned, stripped of
//! every trace of git, and copied into `tethers/` under the name it publishes
//! itself as. From that point it is ordinary project source: committed, read by
//! the compiler from disk, and never fetched again until someone asks.
//!
//! On its own, `piton tether` installs everything `piton.config.pi` lists under
//! `dependencies`. Given a source, it adds that source to the list first, so
//! the configuration and `tethers/` agree about what the project depends on.

use piton_compile::packages::{Dependency, Lock, Pin};
use piton_compile::Project;

use anstream::println;

use crate::style::{self, paint};
use crate::config_edit;
use crate::packages::{Failure, Installer};
use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

pub fn run(source: Option<&str>, rename: Option<&str>) -> u8 {
    let (mut configured, diagnostics) = project::current();
    if report_config_errors(&configured, &diagnostics) {
        return EXIT_ERRORS;
    }

    let Some(source) = source else {
        if rename.is_some() {
            report::fail("`--as` names one package, so it needs a source");
            return EXIT_ERRORS;
        }
        return install_all(&configured);
    };

    // The source is recorded before anything is installed, so a failed clone
    // leaves the intent written down rather than a project that forgot it.
    match &configured.config_path {
        Some(config_path) => {
            let text = match std::fs::read_to_string(config_path) {
                Ok(text) => text,
                Err(error) => {
                    report::fail(format!("cannot read `{}`: {error}", config_path.display()));
                    return EXIT_ERRORS;
                }
            };
            match config_edit::add_dependency(&text, config_path, source) {
                Ok(Some(updated)) => {
                    if let Err(error) = std::fs::write(config_path, updated) {
                        report::fail(format!(
                            "cannot write `{}`: {error}",
                            config_path.display()
                        ));
                        return EXIT_ERRORS;
                    }
                    println!(
                        "{} {} to piton.config.pi",
                        paint(style::SUCCESS, "added"),
                        paint(style::NAME, &source)
                    );
                    configured = project::current().0;
                }
                Ok(None) => {}
                Err(message) => {
                    report::fail(message);
                    return EXIT_ERRORS;
                }
            }
        }
        None => {
            report::warn(format_args!("there is no piton.config.pi to record {source} in; \
`piton update` will not know about it"));
        }
    }

    // A repository already named in `dependencies` brings its pin with it, so
    // tethering and building agree about which version the project wants.
    let pin = configured
        .dependencies
        .iter()
        .find(|dependency| dependency.source == source)
        .map(|dependency| dependency.pin.clone())
        .unwrap_or(Pin::Default);
    let packages = configured
        .dependencies
        .iter()
        .find(|dependency| dependency.source == source)
        .and_then(|dependency| dependency.packages.clone());
    let dependency = Dependency {
        source: source.to_string(),
        pin,
        packages,
    };
    install(&configured, &[dependency], rename)
}

/// Installs everything the configuration depends on.
pub fn install_all(configured: &Project) -> u8 {
    if configured.config_path.is_none() {
        report::fail("no piton.config.pi, so there is no dependency list to install from");
        report::help("pass a repository to tether it: `piton tether <url>`");
        return EXIT_ERRORS;
    }
    if configured.dependencies.is_empty() {
        println!("no dependencies are declared in piton.config.pi");
        return EXIT_SUCCESS;
    }
    install(configured, &configured.dependencies, None)
}

fn install(configured: &Project, dependencies: &[Dependency], rename: Option<&str>) -> u8 {
    let lock = Lock::load(&configured.root);
    let mut installer = Installer::new(&configured.root, lock);
    for dependency in dependencies {
        if let Err(failure) = installer.add(dependency, rename) {
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
    for package in &placed {
        println!(
            "{} {} to {} {}",
            paint(style::SUCCESS, "tethered"),
            package.source,
            paint(style::NAME, format!("tethers/{}", package.name)),
            paint(
                style::DIM,
                format!("({} {})", package.files, report::plural(package.files, "file", "files"))
            )
        );
        if let Some(commit) = &package.commit {
            println!("  {}", paint(style::DIM, format!("at {} ({commit})", package.pin)));
        }
    }
    EXIT_SUCCESS
}

/// Prints configuration diagnostics, returning true when any was an error.
///
/// A package command acts on what the configuration says, so a configuration
/// that could not be read correctly -- two pins on one dependency, say -- has
/// to stop it rather than install something nobody asked for.
pub fn report_config_errors(
    configured: &Project,
    diagnostics: &piton_core::DiagnosticSink,
) -> bool {
    if diagnostics.is_empty() {
        return false;
    }
    report::diagnostics(
        diagnostics,
        &|path| std::fs::read_to_string(path).ok(),
        &configured.root,
    )
}

pub fn fail(failure: Failure) -> u8 {
    report::fail(failure.message);
    if let Some(help) = failure.help {
        report::help(format_args!("{help}"));
    }
    EXIT_ERRORS
}
