//! `piton tether` — clone a repository into the tethers directory.
//!
//! Tethering is how a dependency arrives. The repository is cloned, stripped of
//! every trace of git, and copied into `tethers/` under the name it publishes
//! itself as. From that point it is ordinary project source: committed, read by
//! the compiler from disk, and never fetched again until someone asks.

use piton_compile::packages::{Dependency, Lock, Pin};

use crate::packages::{Failure, Installer};
use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

pub fn run(source: &str, rename: Option<&str>) -> u8 {
    let (configured, _) = project::current();

    // A repository already named in `dependencies` brings its pin with it, so
    // tethering and building agree about which version the project wants.
    let pin = configured
        .dependencies
        .iter()
        .find(|dependency| dependency.source == source)
        .map(|dependency| dependency.pin.clone())
        .unwrap_or(Pin::Default);
    let dependency = Dependency {
        source: source.to_string(),
        pin,
    };

    let lock = Lock::load(&configured.root);
    let mut installer = Installer::new(&configured.root, lock);
    if let Err(failure) = installer.add(&dependency, rename) {
        return fail(failure);
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
        eprintln!("warning: {warning}");
    }
    for package in &placed {
        println!(
            "tethered {} to tethers/{} ({} {})",
            package.source,
            package.name,
            package.files,
            report::plural(package.files, "file", "files")
        );
        if let Some(commit) = &package.commit {
            println!("  at {} ({commit})", package.pin);
        }
    }

    // Tethering does not edit the configuration: `piton update` reads the
    // dependency list to decide what to refresh, so a package that is only in
    // the lock file is one that update will not see.
    if !configured
        .dependencies
        .iter()
        .any(|dependency| dependency.source == source)
    {
        println!();
        println!("declare it in piton.config.pi so `piton update` refreshes it:");
        println!("    dependencies:");
        println!("        - {source}");
    }

    EXIT_SUCCESS
}

pub fn fail(failure: Failure) -> u8 {
    report::fail(failure.message);
    if let Some(help) = failure.help {
        eprintln!("  help: {help}");
    }
    EXIT_ERRORS
}
