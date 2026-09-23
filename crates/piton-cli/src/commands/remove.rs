//! `piton remove` — delete an installed package.
//!
//! Removing deletes files, so it checks two things first: that the package is
//! still what was installed, and that nothing imports it. Either one being
//! false means removing it would lose something -- edits in the first case, a
//! working build in the second -- so both are reported rather than overridden.

use piton_compile::packages::{self, Lock};

use crate::commands::tether::fail;
use crate::config_edit;
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

    // And from the configuration, or the next `piton tether` or `piton update`
    // would install it again.
    let source = removed.map(|entry| entry.source);
    let declared = configured.dependencies.iter().find(|dependency| {
        source.as_deref() == Some(dependency.source.as_str()) || dependency.default_name() == name
    });
    if let (Some(dependency), Some(config_path)) = (declared, &configured.config_path) {
        // A repository can publish several packages. It stays declared while
        // any of the others is still installed from it.
        let siblings: Vec<String> = lock
            .packages
            .iter()
            .filter(|entry| entry.source == dependency.source)
            .map(|entry| entry.name.clone())
            .collect();
        if !siblings.is_empty() {
            println!(
                "{} stays in piton.config.pi, because {} still {} installed from it",
                dependency.source,
                siblings.join(", "),
                report::plural(siblings.len(), "is", "are")
            );
            return EXIT_SUCCESS;
        }
        let edited = std::fs::read_to_string(config_path)
            .map_err(|error| error.to_string())
            .and_then(|text| config_edit::remove_dependency(&text, config_path, &dependency.source));
        match edited {
            Ok(Some(updated)) => {
                if let Err(error) = std::fs::write(config_path, updated) {
                    report::fail(format!("cannot write `{}`: {error}", config_path.display()));
                    return EXIT_ERRORS;
                }
                println!("dropped {} from piton.config.pi", dependency.source);
            }
            Ok(None) => println!(
                "{} is declared in piton.config.pi in a way that could not be edited; \
drop it from dependencies by hand or `piton update` will install it again",
                dependency.source
            ),
            Err(message) => {
                report::fail(message);
                return EXIT_ERRORS;
            }
        }
    }

    EXIT_SUCCESS
}
