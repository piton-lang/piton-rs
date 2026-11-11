//! `piton tether` — clone repositories into the tethers directory.
//!
//! A tether adds a dependency. The command clones the repository and removes
//! all git data. Then it copies the repository into `tethers/` with the name
//! that the repository publishes. After this, it is usual project source. The
//! user commits it. The compiler reads it from disk. No command fetches it
//! again until a person asks.
//!
//! Without a source, `piton tether` installs all items that `piton.config.pi`
//! lists under `dependencies`. With a source, it installs the source. Then it
//! adds the source to the list. Thus the configuration and `tethers/` agree
//! about the dependencies of the project. A source can fail to install. A
//! source can publish a package name that a different package already has.
//! In these two cases, the command changes nothing.
//!
//! A source is a URL or a directory on disk. A relative path starts at the
//! directory of `piton.config.pi`. The directory where piton runs has no
//! effect.

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

    // A repository that `dependencies` already names keeps its pin. Thus the
    // tether and the build agree about the version that the project needs.
    let declared = configured
        .dependencies
        .iter()
        .find(|dependency| dependency.source == source);
    let dependency = Dependency {
        source: source.to_string(),
        pin: declared.map(|dependency| dependency.pin.clone()).unwrap_or(Pin::Default),
        packages: declared.and_then(|dependency| dependency.packages.clone()),
    };

    // The command edits the configuration in memory first. Thus, if the
    // configuration cannot record a source, the command stops before a fetch.
    // The command writes the configuration only after a successful install.
    // Thus it does not add a source that fails to clone. It also does not add
    // a source that conflicts with a different package.
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

/// A source in the form that the configuration must record.
///
/// A relative path on the command line starts at the working directory. But
/// the configuration reads it from the directory of the configuration. When
/// the two directories are different, this function changes the path. Then
/// the path goes to the same location from the directory of the configuration.
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

/// Installs all dependencies of the configuration.
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

/// Installs all `dependencies`, or none of them. With `guard_owners`, it
/// refuses a package that a different source already tethered. It does not
/// replace that package. A new source needs this behaviour.
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

/// Prints configuration diagnostics. Returns true when one or more are errors.
///
/// A package command obeys the configuration. Thus a configuration with a
/// read problem must stop the command. An example is two pins on one
/// dependency. If not, the command can install a package that nobody asked
/// for.
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
