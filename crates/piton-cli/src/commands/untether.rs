//! `piton untether` — adopt an installed package as the project's own source.
//!
//! An installed package is managed: `piton update` replaces it, and it refuses
//! to do so once the files have been edited. Untethering is the way out of that
//! arrangement. The package moves out of `tethers/` and into the source root,
//! where it is ordinary project source that nothing will overwrite, and every
//! import that named the package is rewritten to point at where it went.

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

    // The imports are found before the move, because afterwards the package is
    // no longer where the resolver would look for it.
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


    // Written as a root-absolute path, which resolves against the source root
    // from any file. A relative one would have to be recomputed per importer,
    // and would break again the moment a file moved.
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

/// Reports imports that live inside another installed package.
///
/// Rewriting one would make that package differ from what the lock file
/// recorded, and a package in that state cannot be updated. So it is left as it
/// is and named, because the choice between an edited package and a broken
/// import belongs to whoever has to live with it.
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

/// Moves a package out of `tethers/`, falling back to copy-and-delete across
/// filesystems.
///
/// `tethers/` and the source root are usually on the same device, where a
/// rename is atomic; they need not be, and a package that cannot be untethered
/// because a temporary directory lives elsewhere would be a strange limit.
///
/// Either way `tethers/` is removed when the package was the last thing in
/// it.
fn move_tree(project_root: &Path, name: &str, from: &Path, to: &Path) -> Result<(), Failure> {
    if std::fs::rename(from, to).is_ok() {
        ops::prune_empty_tethers(project_root);
        return Ok(());
    }
    ops::copy_ungitted(from, to)?;
    ops::remove_directory(project_root, name)
}
