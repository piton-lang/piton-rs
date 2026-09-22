//! `piton remove` — delete an installed package.
//!
//! Removing deletes files, so it checks two things first: that the package is
//! still what was installed, and that nothing imports it. Either one being
//! false means removing it would lose something -- edits in the first case, a
//! working build in the second -- so both are reported rather than overridden.

use piton_compile::packages::{self, Lock};

use crate::commands::tether::fail;
use crate::packages as ops;
use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

pub fn run(name: &str) -> u8 {
    let (configured, _) = project::current();
    let directory = packages::package_directory(&configured.root, name);
    if !directory.is_dir() {
        report::fail(format!("`{name}` is not installed"));
        let installed = packages::installed_names(&configured.root);
        if installed.is_empty() {
            eprintln!("  help: nothing is installed in tethers/");
        } else {
            eprintln!("  help: installed packages are {}", installed.join(", "));
        }
        return EXIT_ERRORS;
    }

    let mut lock = Lock::load(&configured.root);
    if let Err(failure) = ops::require_unmodified(&configured.root, &lock, name) {
        return fail(failure);
    }

    let sites = ops::imports_of(&configured, name);
    if !sites.is_empty() {
        report::fail(format!(
            "`{name}` is still imported by {} {}",
            sites.len(),
            report::plural(sites.len(), "file", "files")
        ));
        for site in &sites {
            eprintln!(
                "  {}: {}",
                project::display(&site.file, &configured.root),
                site.written
            );
        }
        eprintln!("  help: remove those imports first, or run `piton untether {name}` to keep the files");
        return EXIT_ERRORS;
    }

    if let Err(failure) = ops::remove_directory(&configured.root, name) {
        return fail(failure);
    }
    // The repository it came from, which is not derivable from the name: a
    // package installs under the name it publishes, not the one its URL
    // suggests.
    let removed = lock.remove(name);
    if removed.is_some() {
        if let Err(error) = lock.save(&configured.root) {
            report::fail(format!("cannot write the package lock file: {error}"));
            return EXIT_ERRORS;
        }
    }

    println!("removed {name}");
    let source = removed.map(|entry| entry.source);
    if let Some(dependency) = configured.dependencies.iter().find(|dependency| {
        source.as_deref() == Some(dependency.source.as_str())
            || dependency.default_name() == name
    }) {
        println!(
            "it is still declared in piton.config.pi; drop `{}` from dependencies \
or `piton update` will install it again",
            dependency.source
        );
    }

    EXIT_SUCCESS
}
