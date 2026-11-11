//! `piton untether` — make an installed package part of the project source.
//!
//! Piton manages an installed package. `piton update` replaces it. After a
//! person edits the files, `piton update` refuses to replace it. An untether
//! stops this management. The package moves from `tethers/` into the source
//! root. There it is usual project source, and no command overwrites it. The
//! command changes each import of the package to the new location.

use std::path::Path;

use piton_compile::module;
use piton_compile::packages::{self, Lock};

use anstream::println;

use crate::style::{self, paint};
use crate::commands::tether::fail;
use crate::packages::{self as ops, Failure};
use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

pub fn run(name: &str, rename: Option<&str>, no_rewrite: bool) -> u8 {
    let (configured, _) = project::current();
    let from = packages::package_directory(&configured.root, name);
    if !from.is_dir() {
        report::fail(format!("`{name}` is not installed"));
        let installed = packages::installed_names(&configured.root);
        if installed.is_empty() {
            report::help("nothing is installed in tethers/");
        } else {
            report::help(format_args!("installed packages are {}", installed.join(", ")));
        }
        return EXIT_ERRORS;
    }

    let adopted = rename.unwrap_or(name);
    let mut to = configured.source_root.join(packages::UNTETHERED);
    for segment in adopted.split('/').filter(|segment| !segment.is_empty()) {
        to.push(segment);
    }
    if to.exists() {
        report::fail(format!(
            "`{}` already exists",
            project::display(&to, &configured.root)
        ));
        report::help("pass `--as <name>` to untether it under a different name");
        return EXIT_ERRORS;
    }

    // The command finds the imports before the move. After the move, the
    // package is not in the location where the resolver looks.
    let sites = ops::imports_of(&configured, name);

    if let Some(parent) = to.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            report::fail(format!("cannot create `{}`: {error}", parent.display()));
            return EXIT_ERRORS;
        }
    }
    if let Err(failure) = move_tree(&configured.root, name, &from, &to) {
        return fail(failure);
    }

    let mut lock = Lock::load(&configured.root);
    if lock.remove(name).is_some() {
        if let Err(error) = lock.save(&configured.root) {
            report::fail(format!("cannot write the package lock file: {error}"));
            return EXIT_ERRORS;
        }
    }

    println!(
        "{} {} to {}",
        paint(style::SUCCESS, "untethered"),
        paint(style::NAME, name),
        paint(style::NAME, project::display(&to, &configured.root))
    );

    if no_rewrite {
        if !sites.is_empty() {
            println!(
                "{} {} still {} `{name}`",
                sites.len(),
                report::plural(sites.len(), "import", "imports"),
                report::plural(sites.len(), "names", "name")
            );
            for site in &sites {
                println!(
                    "  {} {}",
                    paint(style::DIM, format!("{}:", project::display(&site.file, &configured.root))),
                    site.written
                );
            }
        }
        return EXIT_SUCCESS;
    }


    // The command writes a root-absolute path. It resolves from the source
    // root in all files. A relative path must change for each importer. Also,
    // it breaks again when a file moves.
    let replacement = format!(
        "/{}",
        module::relative_path(&configured.source_root, &to)
            .to_string_lossy()
            .replace('\\', "/")
    );
    let rewritable = sites.iter().filter(|site| !site.managed).count();
    match ops::rewrite_imports(&sites, name, &replacement) {
        Ok(0) => println!("no imports named it"),
        Ok(files) => println!(
            "rewrote {rewritable} {} in {files} {}",
            report::plural(rewritable, "import", "imports"),
            report::plural(files, "file", "files")
        ),
        Err(failure) => return fail(failure),
    }
    report_managed(&configured, &sites, name);

    EXIT_SUCCESS
}

/// Reports imports in a different installed package.
///
/// A change to such an import makes that package different from the lock file
/// record. Then no update is possible for that package. Thus the command does
/// not change the import, but names it. The user must choose between an
/// edited package and a broken import.
fn report_managed(project: &piton_compile::Project, sites: &[ops::ImportSite], name: &str) {
    let managed: Vec<_> = sites.iter().filter(|site| site.managed).collect();
    if managed.is_empty() {
        return;
    }
    report::warn(format_args!(
        "{} {} inside other installed packages still {} `{name}`, and {} not rewritten",
        managed.len(),
        report::plural(managed.len(), "import", "imports"),
        report::plural(managed.len(), "names", "name"),
        report::plural(managed.len(), "was", "were")
    ));
    for site in managed {
        report::note(format_args!(
            "{}: {}",
            project::display(&site.file, &project.root),
            site.written
        ));
    }
    report::help("untether those packages too, or the imports will not resolve");
}

/// Moves a package out of `tethers/`. Between filesystems, it copies and then
/// deletes.
///
/// Usually, `tethers/` and the source root are on the same device. There, a
/// rename is atomic. But they can be on different devices. A temporary
/// directory on a different device must not stop an untether.
///
/// In the two cases, the function removes `tethers/` when it contains no other
/// package.
fn move_tree(project_root: &Path, name: &str, from: &Path, to: &Path) -> Result<(), Failure> {
    if std::fs::rename(from, to).is_ok() {
        ops::prune_empty_tethers(project_root);
        return Ok(());
    }
    ops::copy_ungitted(from, to)?;
    ops::remove_directory(project_root, name)
}
