//! Installing, moving, and removing vendored packages.
//!
//! A tethered package is plain files. Cloning brings a repository in, the clone
//! is stripped of everything that made it a repository, and what is left is
//! copied into `tethers/` and committed with the project. Nothing here runs at
//! compile time; these are the operations the four package commands are made
//! of.
//!
//! The rule that shapes all of them is that an installed package may have been
//! edited, and an edit must never be thrown away silently. So every operation
//! that would overwrite or delete files checks the lock file first and refuses
//! when what is on disk is not what was installed.

use std::path::{Path, PathBuf};
use std::process::Command;

use piton_compile::config;
use piton_compile::packages::{self, Dependency, Installed, Lock, Pin};
use piton_compile::{PackageDecl, Project};
use piton_syntax::ast::Item;
use piton_syntax::parser;

/// What went wrong, in terms the command can print without decoration.
#[derive(Debug)]
pub struct Failure {
    pub message: String,
    pub help: Option<String>,
}

impl Failure {
    pub fn new(message: impl Into<String>) -> Failure {
        Failure {
            message: message.into(),
            help: None,
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Failure {
        self.help = Some(help.into());
        self
    }
}

pub type Outcome<T> = Result<T, Failure>;

/// One package installed by an operation, for reporting.
pub struct Placed {
    pub name: String,
    pub source: String,
    pub pin: Pin,
    pub commit: Option<String>,
    pub files: usize,
    /// Every file that differs between what was installed before and what is
    /// installed now, when the operation was asked for a diff.
    pub changes: Vec<FileChange>,
    /// The local edits a forced replacement threw away, described for a
    /// warning, when there were any.
    pub discarded: Option<String>,
}

/// One file's contents before and after an install. None is a file that
/// wasn't there.
pub struct FileChange {
    /// The path inside the package, `/`-separated.
    pub path: String,
    pub before: Option<Vec<u8>>,
    pub after: Option<Vec<u8>>,
}

/// Every file under `directory`, by its `/`-separated path inside it.
fn read_tree(directory: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, directory: &Path, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else if let (Ok(relative), Ok(contents)) = (path.strip_prefix(root), std::fs::read(&path)) {
                let parts: Vec<String> = relative
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().to_string())
                    .collect();
                out.insert(parts.join("/"), contents);
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(directory, directory, &mut out);
    out
}

/// The files that differ between two snapshots of a package, by path.
fn changes(
    mut before: std::collections::BTreeMap<String, Vec<u8>>,
    after: std::collections::BTreeMap<String, Vec<u8>>,
) -> Vec<FileChange> {
    let mut out = Vec::new();
    for (path, contents) in after {
        let old = before.remove(&path);
        if old.as_ref() != Some(&contents) {
            out.push(FileChange {
                path,
                before: old,
                after: Some(contents),
            });
        }
    }
    for (path, contents) in before {
        out.push(FileChange {
            path,
            before: Some(contents),
            after: None,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// The local edits to an installed package, file by file: the files as they
/// were installed, fetched again from the commit the lock file records,
/// against what is on disk now.
fn local_edits(
    project_root: &Path,
    lock: &Lock,
    name: &str,
    fallback: &str,
    scratch: &Path,
) -> Outcome<Vec<FileChange>> {
    let Some(recorded) = lock.get(name) else {
        return Err(Failure::new(format!(
            "`{name}` was not recorded by `piton tether`, so there is nothing to compare it with"
        )));
    };
    let Some(commit) = recorded.commit.clone() else {
        return Err(Failure::new(format!(
            "the lock file records no commit for `{name}`, so what was installed can't be fetched again"
        )));
    };
    let checkout = clone(&recorded.source, &Pin::Commit(commit), scratch)?;
    let offered = published(&checkout.directory, fallback);
    // A package tethered under another name is the one the repository offers
    // when it offers only one.
    let package = offered
        .iter()
        .find(|package| package.name == name)
        .or_else(|| (offered.len() == 1).then(|| &offered[0]))
        .ok_or_else(|| {
            Failure::new(format!(
                "`{}` no longer offers `{name}` at the installed commit",
                recorded.source
            ))
        })?;
    // Copied the way installing copies it, so only the files an install
    // writes are compared.
    let installed = scratch.join(format!("installed-{name}"));
    let _ = std::fs::remove_dir_all(&installed);
    copy_ungitted(&package.root, &installed)?;
    let edits = changes(
        read_tree(&installed),
        read_tree(&packages::package_directory(project_root, name)),
    );
    let _ = std::fs::remove_dir_all(&installed);
    Ok(edits)
}

/// What has changed in an installed package since it was installed, for the
/// warning a forced replacement gives.
fn local_changes(project_root: &Path, lock: &Lock, name: &str) -> String {
    let directory = packages::package_directory(project_root, name);
    let Some(recorded) = lock.get(name) else {
        return "it was not recorded by `piton tether`".to_string();
    };
    let drift = recorded.compare(&directory);
    let paths = drift.paths();
    let mut listed: Vec<String> = paths.iter().take(5).cloned().collect();
    if paths.len() > 5 {
        listed.push(format!("and {} more", paths.len() - 5));
    }
    format!("{}: {}", drift.summary(), listed.join(", "))
}

/// Runs `git`, returning its trimmed standard output.
fn git(directory: &Path, args: &[&str]) -> Outcome<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(directory)
        .output()
        .map_err(|error| {
            Failure::new(format!("cannot run git: {error}"))
                .with_help("package sources are git repositories, so git has to be installed")
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Failure::new(format!(
            "git {} failed: {}",
            args.first().copied().unwrap_or(""),
            stderr.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// A checkout that deletes itself.
pub struct Checkout {
    pub directory: PathBuf,
    pub commit: Option<String>,
    /// When `commit` was committed, in seconds since the epoch. Version
    /// conflicts are settled by it: the newest commit wins.
    pub committed_at: Option<i64>,
}

impl Drop for Checkout {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// Clones `source` at `pin` into a temporary directory.
///
/// A branch or tag is fetched directly, which keeps the clone shallow. A commit
/// cannot be: a server need not serve an arbitrary revision to a shallow fetch,
/// so the repository is cloned in full and then checked out.
pub fn clone(source: &str, pin: &Pin, scratch: &Path) -> Outcome<Checkout> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    // Two checkouts can be alive at once -- comparing two versions of one
    // repository takes both -- so each gets its own directory.
    static NEXT: AtomicUsize = AtomicUsize::new(0);

    std::fs::create_dir_all(scratch)
        .map_err(|error| Failure::new(format!("cannot create a working directory: {error}")))?;
    let directory = scratch.join(format!("checkout-{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&directory);

    let target = directory.to_string_lossy().to_string();
    match pin {
        Pin::Default => {
            git(scratch, &["clone", "--depth", "1", "--quiet", source, &target])?;
        }
        Pin::Branch(name) | Pin::Tag(name) => {
            git(
                scratch,
                &[
                    "clone", "--depth", "1", "--quiet", "--branch", name, source, &target,
                ],
            )?;
        }
        Pin::Commit(commit) => {
            // Only that commit is fetched: cloning the whole history to check
            // one commit out is the slow part of a pinned install. A server
            // that won't hand out a commit by its hash, or a commit given
            // abbreviated, falls back to the full clone.
            std::fs::create_dir_all(&directory).map_err(|error| {
                Failure::new(format!("cannot create a working directory: {error}"))
            })?;
            let shallow = git(&directory, &["init", "--quiet", "."])
                .and_then(|_| git(&directory, &["fetch", "--quiet", "--depth", "1", source, commit]))
                .and_then(|_| git(&directory, &["checkout", "--quiet", "FETCH_HEAD"]));
            if shallow.is_err() {
                let _ = std::fs::remove_dir_all(&directory);
                git(scratch, &["clone", "--quiet", source, &target])?;
                git(&directory, &["checkout", "--quiet", commit])?;
            }
        }
    }

    let commit = git(&directory, &["rev-parse", "HEAD"]).ok();
    let committed_at = git(&directory, &["log", "-1", "--format=%ct", "HEAD"])
        .ok()
        .and_then(|text| text.trim().parse().ok());
    Ok(Checkout {
        directory,
        commit,
        committed_at,
    })
}

/// The packages a checkout publishes.
///
/// A repository that carries a `piton.config.pi` says which of its directories
/// are packages, what each is called, and what each needs. Only those
/// packages' own dependencies come along: the repository's project-level
/// dependencies are for working on that repository, not for using what it
/// publishes.
///
/// A repository that publishes no packages is installed whole, under the name
/// its source implies, which is what makes a plain repository of `.pi` files
/// usable without it having to know about Piton's packaging at all.
pub fn published(checkout: &Path, fallback: &str) -> Vec<PackageDecl> {
    let (project, _) = config::load(checkout, None);
    // A configuration found outside the checkout belongs to whatever directory
    // the clone happened to land in, not to the package.
    let owned = project
        .config_path
        .as_ref()
        .is_some_and(|path| path.starts_with(checkout));
    if !owned || project.packages.is_empty() {
        return vec![PackageDecl {
            name: fallback.to_string(),
            root: checkout.to_path_buf(),
            dependencies: Vec::new(),
        }];
    }
    project.packages
}

/// Copies a tree, leaving behind everything that made it a git repository.
///
/// "Un-gitting" is the whole point of tethering: what lands in `tethers/` is a
/// directory of files belonging to this project's history, not a nested
/// checkout of someone else's.
pub fn copy_ungitted(from: &Path, to: &Path) -> Outcome<usize> {
    const GIT_ARTEFACTS: [&str; 6] = [
        ".git",
        ".gitignore",
        ".gitattributes",
        ".gitmodules",
        ".github",
        ".gitkeep",
    ];

    let mut copied = 0usize;
    let mut stack = vec![(from.to_path_buf(), to.to_path_buf())];
    std::fs::create_dir_all(to)
        .map_err(|error| Failure::new(format!("cannot create `{}`: {error}", to.display())))?;

    while let Some((source, destination)) = stack.pop() {
        let entries = std::fs::read_dir(&source).map_err(|error| {
            Failure::new(format!("cannot read `{}`: {error}", source.display()))
        })?;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if GIT_ARTEFACTS.contains(&name.as_str()) {
                continue;
            }
            let path = entry.path();
            let into = destination.join(&name);
            if path.is_dir() {
                std::fs::create_dir_all(&into).map_err(|error| {
                    Failure::new(format!("cannot create `{}`: {error}", into.display()))
                })?;
                stack.push((path, into));
            } else {
                std::fs::copy(&path, &into).map_err(|error| {
                    Failure::new(format!("cannot write `{}`: {error}", into.display()))
                })?;
                copied += 1;
            }
        }
    }
    Ok(copied)
}

/// Refuses to touch a package whose files no longer match the lock file.
///
/// This is the conflict rule: an installed package that has been edited is
/// carrying work, and replacing it would destroy that work. Untethering is the
/// way out, because it makes the files the project's own.
pub fn require_unmodified(project_root: &Path, lock: &Lock, name: &str) -> Outcome<()> {
    let directory = packages::package_directory(project_root, name);
    if !directory.exists() {
        return Ok(());
    }
    let Some(recorded) = lock.get(name) else {
        return Err(Failure::new(format!(
            "`{name}` is installed but was not recorded by `piton tether`"
        ))
        .with_help(
            "nothing says what it should contain, so it cannot be replaced safely; \
remove it and tether it again, or untether it to keep it",
        ));
    };
    let drift = recorded.compare(&directory);
    if drift.is_clean() {
        return Ok(());
    }
    let listed = drift
        .paths()
        .iter()
        .take(5)
        .map(|path| format!("  {path}"))
        .collect::<Vec<_>>()
        .join("\n");
    let more = drift.paths().len().saturating_sub(5);
    let tail = if more > 0 {
        format!("\n  and {more} more")
    } else {
        String::new()
    };
    Err(Failure::new(format!(
        "`{name}` has been modified since it was installed ({}):\n{listed}{tail}",
        drift.summary()
    ))
    .with_help(format!(
        "run `piton untether {name}` to keep the changes, or `piton update --force` to discard them"
    )))
}

/// Installs one package's files, recording what was written.
pub fn install(
    project_root: &Path,
    lock: &mut Lock,
    package: &PackageDecl,
    source: &str,
    pin: &Pin,
    commit: Option<&str>,
) -> Outcome<Placed> {
    let directory = packages::package_directory(project_root, &package.name);
    if directory.exists() {
        std::fs::remove_dir_all(&directory).map_err(|error| {
            Failure::new(format!(
                "cannot replace `{}`: {error}",
                directory.display()
            ))
        })?;
    }
    if let Some(parent) = directory.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            Failure::new(format!("cannot create `{}`: {error}", parent.display()))
        })?;
    }
    let files = copy_ungitted(&package.root, &directory)?;

    let entry = Installed {
        name: package.name.clone(),
        source: source.to_string(),
        pin: pin.clone(),
        commit: commit.map(str::to_string),
        files: packages::digest_tree(&directory),
    };
    lock.insert(entry);

    Ok(Placed {
        name: package.name.clone(),
        source: source.to_string(),
        pin: pin.clone(),
        commit: commit.map(str::to_string),
        files,
        changes: Vec::new(),
        discarded: None,
    })
}

/// Installs a set of dependencies and everything they in turn require.
///
/// Dependencies are flat by design: one version of a repository is installed
/// for the whole project. A source reached twice is fetched once. When two
/// declarations ask for different versions, the one whose commit is newest is
/// installed and the disagreement is reported, because a dependency version
/// nobody chose is worse than one nobody has.
pub struct Installer<'a> {
    pub project_root: &'a Path,
    pub scratch: PathBuf,
    pub lock: Lock,
    pub placed: Vec<Placed>,
    pub warnings: Vec<String>,
    /// Replace an edited package anyway, discarding its local changes, rather
    /// than refusing.
    pub force: bool,
    /// Record what each install changed, for a diff.
    pub diff: bool,
    /// With `diff`, the local edits of each package that was edited since it
    /// was installed, so they can be shown even when the update is refused
    /// because of them.
    pub edited: Vec<(String, Outcome<Vec<FileChange>>)>,
    /// Sources already installed.
    seen: Vec<Seen>,
}

/// A source the installer has already placed, and the version it took.
struct Seen {
    source: String,
    pin: Pin,
    commit: Option<String>,
    committed_at: Option<i64>,
}

impl<'a> Installer<'a> {
    pub fn new(project_root: &'a Path, lock: Lock) -> Installer<'a> {
        let scratch = std::env::temp_dir().join(format!("piton-tether-{}", std::process::id()));
        Installer {
            project_root,
            scratch,
            lock,
            placed: Vec::new(),
            warnings: Vec::new(),
            force: false,
            diff: false,
            edited: Vec::new(),
            seen: Vec::new(),
        }
    }

    /// Installs `dependency` and, transitively, what the packages it publishes
    /// require.
    ///
    /// `rename` names the single package a repository publishes, which is what
    /// `piton tether --as` needs; a repository that publishes several names them
    /// itself.
    pub fn add(&mut self, dependency: &Dependency, rename: Option<&str>) -> Outcome<()> {
        let mut queue = vec![(dependency.clone(), rename.map(str::to_string))];

        while let Some((dependency, rename)) = queue.pop() {
            let checkout = match self
                .seen
                .iter()
                .position(|seen| seen.source == dependency.source)
            {
                Some(index) if self.seen[index].pin == dependency.pin => continue,
                Some(index) => {
                    // Asked for at a second version. Both are looked at, and
                    // the newer commit is the one the project gets.
                    let checkout = clone(&dependency.source, &dependency.pin, &self.scratch)?;
                    let taken = &self.seen[index];
                    if checkout.commit.is_some() && checkout.commit == taken.commit {
                        continue;
                    }
                    let newer = match (checkout.committed_at, taken.committed_at) {
                        (Some(candidate), Some(current)) => candidate > current,
                        (Some(_), None) => true,
                        _ => false,
                    };
                    let (winner, loser) = if newer {
                        (&dependency.pin, &taken.pin)
                    } else {
                        (&taken.pin, &dependency.pin)
                    };
                    self.warnings.push(format!(
                        "`{}` is required at {} and at {}; installing {winner}, which has the newest commit, instead of {loser}",
                        dependency.source, taken.pin, dependency.pin
                    ));
                    if !newer {
                        continue;
                    }
                    self.seen[index] = Seen {
                        source: dependency.source.clone(),
                        pin: dependency.pin.clone(),
                        commit: checkout.commit.clone(),
                        committed_at: checkout.committed_at,
                    };
                    checkout
                }
                None => {
                    let checkout = clone(&dependency.source, &dependency.pin, &self.scratch)?;
                    self.seen.push(Seen {
                        source: dependency.source.clone(),
                        pin: dependency.pin.clone(),
                        commit: checkout.commit.clone(),
                        committed_at: checkout.committed_at,
                    });
                    checkout
                }
            };

            let mut published = published(&checkout.directory, &dependency.default_name());
            // `packages:` under the dependency picks which of the repository's
            // packages to take.
            if let Some(wanted) = &dependency.packages {
                for name in wanted {
                    if !published.iter().any(|package| &package.name == name) {
                        let offered: Vec<&str> =
                            published.iter().map(|package| package.name.as_str()).collect();
                        return Err(Failure::new(format!(
                            "`{}` does not offer a package named `{name}`",
                            dependency.source
                        ))
                        .with_help(format!("it offers: {}", offered.join(", "))));
                    }
                }
                published.retain(|package| wanted.contains(&package.name));
            }

            let published = match (rename, published.len()) {
                (Some(name), 1) => {
                    if let Some(problem) = packages::validate_name(&name) {
                        return Err(Failure::new(problem));
                    }
                    vec![PackageDecl {
                        name,
                        ..published.into_iter().next().expect("one")
                    }]
                }
                (Some(name), count) => {
                    return Err(Failure::new(format!(
                        "`{}` publishes {count} packages, so `{name}` does not name one of them",
                        dependency.source
                    ))
                    .with_help(
                        "tether it without a name and each package installs under its own",
                    ))
                }
                (None, _) => published,
            };

            let mut discarded = std::collections::HashMap::new();
            for package in &published {
                let unmodified = require_unmodified(self.project_root, &self.lock, &package.name);
                if unmodified.is_err() && self.diff && !self.force {
                    let edits = local_edits(
                        self.project_root,
                        &self.lock,
                        &package.name,
                        &dependency.default_name(),
                        &self.scratch,
                    );
                    self.edited.push((package.name.clone(), edits));
                }
                match unmodified {
                    Ok(()) => {}
                    // Forced, the edits are thrown away, and said to be.
                    Err(_) if self.force => {
                        discarded.insert(
                            package.name.clone(),
                            local_changes(self.project_root, &self.lock, &package.name),
                        );
                    }
                    Err(failure) => return Err(failure),
                }
            }
            for package in &published {
                let directory = packages::package_directory(self.project_root, &package.name);
                let before = self.diff.then(|| read_tree(&directory));
                let mut placed = install(
                    self.project_root,
                    &mut self.lock,
                    package,
                    &dependency.source,
                    &dependency.pin,
                    checkout.commit.as_deref(),
                )?;
                if let Some(before) = before {
                    placed.changes = changes(before, read_tree(&directory));
                }
                placed.discarded = discarded.remove(&package.name);
                // A later, newer version replaces what an earlier one placed.
                self.placed.retain(|earlier| earlier.name != placed.name);
                self.placed.push(placed);
            }

            // What each package needs becomes this project's dependency too,
            // because there is nowhere nested for it to go. The repository's
            // own project dependencies are not followed: they are for working
            // on that repository, not for using what it publishes.
            for required in published.iter().flat_map(|package| &package.dependencies) {
                queue.push((required.clone(), None));
            }
        }
        Ok(())
    }

    pub fn finish(self) -> Outcome<(Lock, Vec<Placed>, Vec<String>)> {
        let _ = std::fs::remove_dir_all(&self.scratch);
        Ok((self.lock, self.placed, self.warnings))
    }
}

/// An import that names a package, and where its path sits in the source.
pub struct ImportSite {
    pub file: PathBuf,
    pub start: usize,
    pub end: usize,
    pub written: String,
    /// True when the import is itself inside an installed package.
    ///
    /// Those are not rewritten. Editing one would make that package differ from
    /// what the lock file recorded, which is precisely the state that stops it
    /// being updated -- so it is reported instead, and the person decides.
    pub managed: bool,
}

/// Every import in the project that reaches into `name`.
///
/// Found through the parser rather than by searching text, so a package name
/// that also appears in prose, in a comment, or as part of a longer name is not
/// mistaken for an import.
pub fn imports_of(project: &Project, name: &str) -> Vec<ImportSite> {
    let mut found = Vec::new();
    let mut files = piton_compile::module::sources(&project.source_root);
    if project.root != project.source_root {
        files.extend(piton_compile::module::sources(&project.root));
    }
    // `sources` leaves installed packages out, which is right for everything
    // that reports on the project's own code. Here they matter: one package
    // importing another by name breaks the same way anything else does.
    let tethers = project.root.join(packages::TETHERS);
    files.extend(piton_compile::module::sources(&tethers));
    files.sort();
    files.dedup();

    let inside = packages::package_directory(&project.root, name);
    for file in files {
        // An import written inside the package being moved is relative to the
        // package and moves with it.
        if file.starts_with(&inside) {
            continue;
        }
        let managed = file.starts_with(&tethers);
        let Ok(source) = std::fs::read_to_string(&file) else {
            continue;
        };
        let parse = parser::parse(&source, &file);
        for item in &parse.file.items {
            let path = match item {
                Item::Use(decl) => &decl.path,
                Item::From(decl) => &decl.path,
                _ => continue,
            };
            let written = path.text.trim();
            if written != name && !written.starts_with(&format!("{name}/")) {
                continue;
            }
            found.push(ImportSite {
                file: file.clone(),
                start: path.span.start,
                end: path.span.end,
                written: written.to_string(),
                managed,
            });
        }
    }
    found
}

/// Rewrites every import of `name` to point at `replacement`, which is written
/// as a path from the project's source root.
///
/// Returns how many files changed.
pub fn rewrite_imports(sites: &[ImportSite], name: &str, replacement: &str) -> Outcome<usize> {
    let mut by_file: std::collections::BTreeMap<PathBuf, Vec<&ImportSite>> = Default::default();
    for site in sites.iter().filter(|site| !site.managed) {
        by_file.entry(site.file.clone()).or_default().push(site);
    }

    let mut changed = 0usize;
    for (file, mut sites) in by_file {
        let mut source = std::fs::read_to_string(&file).map_err(|error| {
            Failure::new(format!("cannot read `{}`: {error}", file.display()))
        })?;
        // Back to front, so an earlier edit does not move a later span.
        sites.sort_by_key(|site| std::cmp::Reverse(site.start));
        for site in sites {
            let rest = site.written.strip_prefix(name).unwrap_or_default();
            source.replace_range(site.start..site.end, &format!("{replacement}{rest}"));
        }
        std::fs::write(&file, source).map_err(|error| {
            Failure::new(format!("cannot write `{}`: {error}", file.display()))
        })?;
        changed += 1;
    }
    Ok(changed)
}

/// Removes a package's directory, and `tethers/` itself when that leaves it
/// empty.
pub fn remove_directory(project_root: &Path, name: &str) -> Outcome<()> {
    let directory = packages::package_directory(project_root, name);
    std::fs::remove_dir_all(&directory).map_err(|error| {
        Failure::new(format!(
            "cannot remove `{}`: {error}",
            directory.display()
        ))
    })?;
    prune_empty_tethers(project_root);
    Ok(())
}

/// Removes `tethers/` when nothing is installed in it any more.
pub fn prune_empty_tethers(project_root: &Path) {
    let tethers = project_root.join(packages::TETHERS);
    if std::fs::read_dir(&tethers).is_ok_and(|mut entries| entries.next().is_none()) {
        let _ = std::fs::remove_dir(&tethers);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "piton-pkgops-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn copying_leaves_the_repository_behind() {
        let dir = sandbox("ungit");
        let from = dir.join("from");
        std::fs::create_dir_all(from.join(".git/objects")).expect("dirs");
        std::fs::create_dir_all(from.join("nested")).expect("dirs");
        std::fs::write(from.join(".git/config"), "[core]").expect("write");
        std::fs::write(from.join(".gitignore"), "target").expect("write");
        std::fs::write(from.join("index.pi"), "export a: 1\n").expect("write");
        std::fs::write(from.join("nested/Thing.pi"), "b: 2\n").expect("write");

        let to = dir.join("to");
        let copied = copy_ungitted(&from, &to).expect("copy");
        assert_eq!(copied, 2);
        assert!(to.join("index.pi").is_file());
        assert!(to.join("nested/Thing.pi").is_file());
        assert!(!to.join(".git").exists());
        assert!(!to.join(".gitignore").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }


    #[test]
    fn an_empty_tethers_directory_is_pruned_and_a_sibling_survives() {
        let dir = sandbox("prune");
        for name in ["one", "two"] {
            let package = packages::package_directory(&dir, name);
            std::fs::create_dir_all(&package).expect("dirs");
            std::fs::write(package.join("index.pi"), "a: 1\n").expect("write");
        }
        remove_directory(&dir, "one").expect("remove");
        assert!(dir.join("tethers/two/index.pi").is_file());
        remove_directory(&dir, "two").expect("remove");
        assert!(!dir.join("tethers").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
