//! `piton remove` — delete an installed package.
//!
//! The removal deletes files. Thus it does two checks first. The package must
//! be the same as the installed package. No file must import it. If a check
//! fails, the removal causes a loss. In the first case, the user loses edits. In the
//! second case, the build stops. Thus the command reports these problems, and
//! does not ignore them.

use piton_compile::packages::{self, Lock};

use anstream::println;

use crate::style::{self, paint};
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
            report::help("nothing is installed in tethers/");
        } else {
            report::help(format_args!("installed packages are {}", installed.join(", ")));
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
        report::help(format_args!("remove those imports first, or run `piton untether {name}` to keep the files"));
        return EXIT_ERRORS;
    }

    if let Err(failure) = ops::remove_directory(&configured.root, name) {
        return fail(failure);
    }
    // The source repository of the package. The name does not give it. A
    // package installs with the name that it publishes, not the name in its
    // URL.
    let removed = lock.remove(name);
    if removed.is_some() {
        if let Err(error) = lock.save(&configured.root) {
            report::fail(format!("cannot write the package lock file: {error}"));
            return EXIT_ERRORS;
        }
    }

    println!("{} {}", paint(style::SUCCESS, "removed"), paint(style::NAME, name));

    // Also remove it from the configuration. If not, the next `piton tether`
    // or `piton update` installs it again.
    let source = removed.map(|entry| entry.source);
    let declared = configured.dependencies.iter().find(|dependency| {
        source.as_deref() == Some(dependency.source.as_str()) || dependency.default_name() == name
    });
    if let (Some(dependency), Some(config_path)) = (declared, &configured.config_path) {
        // A repository can publish many packages. The declaration stays while
        // one of the other packages from it is still installed.
        let siblings: Vec<String> = lock
            .packages
            .iter()
            .filter(|entry| entry.source == dependency.source)
            .map(|entry| entry.name.clone())
            .collect();
        if !siblings.is_empty() {
            println!(
                "{} stays in piton.config.pi, because {} still {} installed from it",
                paint(style::NAME, &dependency.source),
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
                println!(
                    "{} {} from piton.config.pi",
                    paint(style::SUCCESS, "dropped"),
                    paint(style::NAME, &dependency.source)
                );
            }
            Ok(None) => {
                report::warn(format_args!(
                    "{} is declared in piton.config.pi in a way that could not be edited",
                    dependency.source
                ));
                report::help("drop it from dependencies by hand, or `piton update` will install it again");
            }
            Err(message) => {
                report::fail(message);
                return EXIT_ERRORS;
            }
        }
    }

    EXIT_SUCCESS
}
