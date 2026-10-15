//! `piton tether` — clone repositories into the tethers directory.
//!
//! Tethering is how a dependency arrives. The repository is cloned, stripped of
//! every trace of git, and copied into `tethers/` under the name it publishes
//! itself as. From that point it is ordinary project source: committed, read by
//! the compiler from disk, and never fetched again until someone asks.
//!
//! On its own, `piton tether` installs everything `piton.config.pi` lists under
//! `dependencies`. Given a source, it installs it and then adds it to the list,
//! so the configuration and `tethers/` agree about what the project depends
//! on. A source that fails to install, or that publishes a package name
//! another package already has, changes nothing.
//!
//! A source is a URL or a directory on disk. A relative path is read from the
//! directory holding `piton.config.pi`, wherever piton happens to run.

use std::path::Path;

use piton_compile::packages::{Dependency, Lock, Pin};
use piton_compile::{module, Project};

use anstream::println;

use crate::style::{self, paint};
use crate::config_edit;
use crate::packages::{Failure, Installer};
use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

pub fn run(source: Option<&str>, rename: Option<&str>) -> u8 {
    let (configured, diagnostics) = project::current();
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
    let source = as_configured(&configured, source);
    let source = source.as_str();

    // A repository already named in `dependencies` brings its pin with it, so
    // tethering and building agree about which version the project wants.
    let declared = configured
        .dependencies
        .iter()
        .find(|dependency| dependency.source == source);
    let dependency = Dependency {
        source: source.to_string(),
        pin: declared.map(|dependency| dependency.pin.clone()).unwrap_or(Pin::Default),
        packages: declared.and_then(|dependency| dependency.packages.clone()),
    };

    // The configuration is edited in memory first, so a source it can't
    // record stops before anything is fetched, and written only once the
    // install has succeeded: a source that fails to clone, or that conflicts
    // with another package, is not added.
    let edit = match &configured.config_path {
        Some(config_path) => {
            let text = match std::fs::read_to_string(config_path) {
                Ok(text) => text,
                Err(error) => {
                    report::fail(format!("cannot read `{}`: {error}", config_path.display()));
                    return EXIT_ERRORS;
                }
            };
            match config_edit::add_dependency(&text, config_path, source) {
                Ok(updated) => updated.map(|updated| (config_path.clone(), updated)),
                Err(message) => {
                    report::fail(message);
                    return EXIT_ERRORS;
                }
            }
        }
        None => {
            report::warn(format_args!("there is no piton.config.pi to record {source} in; \
`piton update` will not know about it"));
            None
        }
    };

    let code = install(&configured, &[dependency], rename, true);
    if code != EXIT_SUCCESS {
        if edit.is_some() {
            report::stopped(format!("nothing was tethered, and {source} was not added to piton.config.pi"));
        }
        return code;
    }
    if let Some((config_path, updated)) = edit {
        if let Err(error) = std::fs::write(&config_path, updated) {
            report::fail(format!("cannot write `{}`: {error}", config_path.display()));
            report::help(format_args!("add {source} to `dependencies` by hand so `piton update` knows about it"));
            return EXIT_ERRORS;
        }
        println!(
            "{} {} to piton.config.pi",
            paint(style::SUCCESS, "added"),
            paint(style::NAME, source)
        );
    }
    EXIT_SUCCESS
}

/// A source as the configuration should record it.
///
/// A relative path typed on the command line is written from the working
/// directory, but the configuration reads it from its own, so when the two
/// differ it is rewritten to lead to the same place from there.
fn as_configured(configured: &Project, source: &str) -> String {
    let relative = Dependency::new(source).is_local()
        && Path::new(source).is_relative()
        && !source.starts_with("~/");
    if !relative {
        return source.to_string();
    }
    let (Ok(cwd), Ok(root)) = (
        std::env::current_dir().and_then(std::fs::canonicalize),
        std::fs::canonicalize(&configured.root),
    ) else {
        return source.to_string();
    };
    let Ok(target) = std::fs::canonicalize(cwd.join(source)) else {
        return source.to_string();
    };
    if cwd == root {
        return source.to_string();
    }
    let path = module::relative_path(&root, &target)
        .to_string_lossy()
        .replace('\\', "/");
    if path.starts_with("..") {
        path
    } else {
        format!("./{path}")
    }
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
    install(configured, &configured.dependencies, None, false)
}

/// Installs `dependencies`, all or nothing. `guard_owners` refuses a package
/// already tethered from another source instead of replacing it, which is what
/// adding a new source needs.
fn install(
    configured: &Project,
    dependencies: &[Dependency],
    rename: Option<&str>,
    guard_owners: bool,
) -> u8 {
    let lock = Lock::load(&configured.root);
    let mut installer = Installer::new(&configured.root, lock);
    installer.guard_owners = guard_owners;
    installer.own = configured.packages.iter().map(|package| package.name.clone()).collect();
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
        match &package.commit {
            Some(commit) => {
                println!("  {}", paint(style::DIM, format!("at {} ({commit})", package.pin)));
            }
            None => println!("  {}", paint(style::DIM, "copied as it stands, from a plain directory")),
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
