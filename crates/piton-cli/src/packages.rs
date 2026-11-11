//! Installs, moves, and removes vendored packages.
//!
//! A tethered package is plain files. A clone gives a repository. The code
//! removes all repository data from the clone. It copies the other files
//! into `tethers/`, and the user commits them with the project. A source can
//! also be a directory on disk. A git repository there clones like all other
//! repositories. A plain directory copies as it is. No code here runs at
//! compile time. These are the operations of the four package commands.
//!
//! One rule controls all of them. A person can edit an installed package, and
//! the code must never discard an edit without a message. Thus each operation that can
//! overwrite or delete files first checks the lock file. It refuses when the
//! files on disk are not the same as the installed files.

use std::path::{Path, PathBuf};
use std::process::Command;

use piton_compile::config;
use piton_compile::packages::{self, Dependency, Installed, Lock, Pin};
use piton_compile::{PackageDecl, Project};
use piton_syntax::ast::Item;
use piton_syntax::parser;

/// The problem, in words that the command can print without decoration.
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

/// One package that an operation installed, for the report.
pub struct Placed {
    pub name: String,
    pub source: String,
    pub pin: Pin,
    pub commit: Option<String>,
    pub files: usize,
    /// Each file that is different between the previous install and the
    /// current install. The operation fills it only when it must give a diff.
    pub changes: Vec<FileChange>,
    /// The local edits that a forced replacement discarded, as text for a
    /// `Severity::Warning` diagnostic. It has a value only when there were edits.
    pub discarded: Option<String>,
}

/// The contents of one file before and after an install. None is a file that
/// was not there.
pub struct FileChange {
    /// The path inside the package, `/`-separated.
    pub path: String,
    pub before: Option<Vec<u8>>,
    pub after: Option<Vec<u8>>,
}

/// All files under `directory`, by their `/`-separated path in it.
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

/// The files that are different between two snapshots of a package, by path.
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

/// The local edits to an installed package, file by file. The function fetches
/// the installed files again from the commit in the lock file. It compares
/// them with the current files on disk.
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
        // A plain directory keeps no history. Thus its installed contents are
        // gone. Only the digests in the lock file remain.
        let why = if Dependency::new(&recorded.source).is_local() {
            "came from a plain directory, which keeps no history"
        } else {
            "has no commit in the lock file"
        };
        return Err(Failure::new(format!(
            "`{name}` {why}, so what was installed can't be fetched again"
        )));
    };
    let checkout = fetch(
        &recorded.source,
        &Pin::Commit(commit),
        Some(project_root),
        scratch,
    )?;
    let fallback = checkout.fallback_name().unwrap_or_else(|| fallback.to_string());
    let offered = published(&checkout.directory, &fallback);
    // A package can have a different tether name. When the repository offers
    // only one package, that package is the tethered package.
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
    // The copy is the same as the install copy. Thus the comparison uses only
    // the files that an install writes.
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

/// The changes in an installed package after its install. A forced
/// replacement shows them in a `Severity::Warning` diagnostic.
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

/// Runs `git`. Returns its trimmed standard output.
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

/// A checkout that deletes itself when the program drops it.
pub struct Checkout {
    pub directory: PathBuf,
    pub commit: Option<String>,
    /// The time of `commit`, in seconds after the epoch. This time decides
    /// version conflicts. The newest commit wins.
    pub committed_at: Option<i64>,
    /// The directory on disk that the source named, if it named one. Relative
    /// paths in the dependencies of its packages start here.
    pub local: Option<PathBuf>,
    /// False when `directory` is a plain directory that the code reads in
    /// place. That directory is the work of a person, not a scratch copy. It
    /// stays after the checkout.
    owned: bool,
}

impl Checkout {
    /// The install name of a local source that declares no name. It is the
    /// name of its directory. The written `.` or `..` does not give that name.
    pub fn fallback_name(&self) -> Option<String> {
        let name = self.local.as_deref()?.file_name()?.to_string_lossy();
        Some(name.trim_end_matches(".git").to_string())
    }
}

impl Drop for Checkout {
    fn drop(&mut self) {
        if self.owned {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
}

/// Fetches `source` at `pin`.
///
/// The function clones a URL. A path is a directory on disk. A relative path
/// starts at `base`. That is the directory of the configuration that named
/// the path. For a repository from a URL, `base` is `None`. There, a relative
/// path has no sense. The function clones a git directory like a URL. Thus
/// pins work, and the lock records the commit. It reads a different directory
/// as it is. That directory has no versions. Thus it cannot have a pin.
pub fn fetch(source: &str, pin: &Pin, base: Option<&Path>, scratch: &Path) -> Outcome<Checkout> {
    let dependency = Dependency::new(source);
    if !dependency.is_local() {
        return clone(source, pin, scratch);
    }
    let relative = Path::new(source.trim()).is_relative() && !source.trim().starts_with("~/");
    let path = match (base, relative) {
        (None, true) => {
            return Err(Failure::new(format!(
                "`{source}` is a relative path in a repository fetched from a URL, so there is no directory to read it from"
            ))
            .with_help("give the dependency as a URL or an absolute path"))
        }
        (base, _) => dependency
            .local_path(base.unwrap_or(Path::new("/")))
            .ok_or_else(|| Failure::new(format!("`{source}` is not a path")))?,
    };
    let path = std::fs::canonicalize(&path)
        .ok()
        .filter(|path| path.is_dir())
        .ok_or_else(|| {
            Failure::new(format!(
                "`{source}` is not a directory (looked for {})",
                path.display()
            ))
        })?;

    if path.join(".git").exists() {
        let mut checkout = clone(&path.to_string_lossy(), pin, scratch)?;
        checkout.local = Some(path);
        return Ok(checkout);
    }
    if *pin != Pin::Default {
        return Err(Failure::new(format!(
            "`{source}` is pinned to {pin}, but it is a plain directory, not a git repository, so it has no versions to pin"
        ))
        .with_help("remove the pin, or make the directory a git repository"));
    }
    Ok(Checkout {
        directory: path.clone(),
        commit: None,
        committed_at: None,
        local: Some(path),
        owned: false,
    })
}

/// True when the git repository at `directory` has changes without a commit.
/// A clone does not copy those changes.
fn uncommitted(directory: &Path) -> bool {
    git(directory, &["status", "--porcelain"]).is_ok_and(|status| !status.is_empty())
}

/// Clones `source` at `pin` into a temporary directory.
///
/// The function fetches a branch or tag directly. Thus the clone stays
/// shallow. A commit is different. A server does not always give a random
/// revision to a shallow fetch. Thus the function clones the full repository
/// and then does a `git checkout` of the commit.
pub fn clone(source: &str, pin: &Pin, scratch: &Path) -> Outcome<Checkout> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    // Two checkouts can exist at the same time. A comparison of two versions
    // of one repository needs both. Thus each checkout has its own directory.
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
            // The function fetches only that commit. A clone of the full
            // history for one commit is the slow part of a pinned install.
            // Some servers do not give a commit by its hash. Some commits have
            // an abbreviated hash. In these cases, the function does a full
            // clone.
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
        local: None,
        owned: true,
    })
}

/// The packages that a checkout publishes.
///
/// A repository with a `piton.config.pi` tells which of its directories are
/// packages. It gives the name and the dependencies of each package. Only the
/// dependencies of those packages come with them. The project dependencies
/// of the repository are for work on that repository. They are not for the
/// use of its packages.
///
/// When a repository publishes no packages, the function installs the full
/// repository. The name comes from its source. Thus a plain repository of
/// `.pi` files is usable without knowledge of the packages of Piton.
pub fn published(checkout: &Path, fallback: &str) -> Vec<PackageDecl> {
    let (project, _) = config::load(checkout, None);
    // A configuration outside the checkout belongs to the directory that
    // contains the clone. It does not belong to the package.
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

/// Copies a tree. It does not copy the git repository data.
///
/// The main purpose of a tether is to remove the git data. The result in `tethers/` is
/// a directory of files in the history of this project. It is not a nested
/// checkout of a different project.
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

/// Refuses to change a package when its files do not match the lock file.
///
/// This is the conflict rule. An edited installed package contains work. A
/// replacement destroys that work. An untether is the solution, because
/// it makes the files part of the project.
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

/// Installs the files of one package. It records the written files.
pub fn install(
    project_root: &Path,
    lock: &mut Lock,
    package: &PackageDecl,
    source: &str,
    pin: &Pin,
    commit: Option<&str>,
) -> Outcome<Placed> {
    let directory = packages::package_directory(project_root, &package.name);
    // A tether of a directory that contains the project copies the project
    // into itself. That copy also contains `tethers/`.
    if let (Ok(project), Ok(root)) = (
        std::fs::canonicalize(project_root),
        std::fs::canonicalize(&package.root),
    ) {
        if project.starts_with(&root) {
            return Err(Failure::new(format!(
                "`{source}` contains this project, so tethering it would copy the project into itself"
            )));
        }
    }
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

/// Installs a set of dependencies, and all dependencies that they need.
///
/// Dependencies are flat by design. The whole project has one version of a
/// repository. When the installer reaches a source two times, it fetches it
/// one time. Two declarations can ask for different versions. Then the installer
/// installs the version with the newest commit, and reports the conflict. A
/// dependency version that nobody chose is worse than no version.
pub struct Installer<'a> {
    pub project_root: &'a Path,
    pub scratch: PathBuf,
    pub lock: Lock,
    pub placed: Vec<Placed>,
    pub warnings: Vec<String>,
    /// Replace an edited package, and discard its local changes. Do not refuse.
    pub force: bool,
    /// Record the changes of each install, for a diff.
    pub diff: bool,
    /// With `diff`, the local edits of each package with edits after its
    /// install. Thus the command can show them, also when they stop the
    /// update.
    pub edited: Vec<(String, Outcome<Vec<FileChange>>)>,
    /// Refuse a package when the lock file records its name for a different
    /// source. Do not replace that package with this package.
    pub guard_owners: bool,
    /// The names of the packages that this project offers. An installed
    /// package must not use these names.
    pub own: Vec<String>,
    /// Sources that the installer resolved before.
    seen: Vec<Seen>,
    /// The packages that `finish` will put in place, after all dependencies
    /// resolve.
    planned: Vec<Planned>,
}

/// A package that the installer resolved and checked. It is not in `tethers/` at this time.
struct Planned {
    package: PackageDecl,
    source: String,
    pin: Pin,
    commit: Option<String>,
    discarded: Option<String>,
    /// Keeps the files of `package.root` until the copy ends.
    _checkout: std::rc::Rc<Checkout>,
}

/// A source that the installer put in place before, and its version.
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
            guard_owners: false,
            own: Vec::new(),
            seen: Vec::new(),
            planned: Vec::new(),
        }
    }

    /// Resolves `dependency`. It also resolves the dependencies of its
    /// packages transitively. It checks each package against all other
    /// packages. It writes no files until `finish`. Thus a failure does not
    /// change the project.
    ///
    /// `rename` names the single package that a repository publishes.
    /// `piton tether --as` needs this. A repository that publishes many
    /// packages names them.
    pub fn add(&mut self, dependency: &Dependency, rename: Option<&str>) -> Outcome<()> {
        // Each dependency has the start directory for its relative path. For
        // the project dependencies, it is the project directory. For the
        // packages of a local repository, it is that repository. For a
        // repository from a URL, there is no directory.
        let mut queue = vec![(
            dependency.clone(),
            rename.map(str::to_string),
            Some(self.project_root.to_path_buf()),
        )];

        while let Some((dependency, rename, base)) = queue.pop() {
            let checkout = match self
                .seen
                .iter()
                .position(|seen| seen.source == dependency.source)
            {
                Some(index) if self.seen[index].pin == dependency.pin => continue,
                Some(index) => {
                    // A different declaration asks for a second version. The
                    // installer examines the two. The project uses the newer
                    // commit.
                    let checkout = fetch(&dependency.source, &dependency.pin, base.as_deref(), &self.scratch)?;
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
                    let checkout = fetch(&dependency.source, &dependency.pin, base.as_deref(), &self.scratch)?;
                    self.seen.push(Seen {
                        source: dependency.source.clone(),
                        pin: dependency.pin.clone(),
                        commit: checkout.commit.clone(),
                        committed_at: checkout.committed_at,
                    });
                    checkout
                }
            };

            if checkout.commit.is_some()
                && checkout.local.as_deref().is_some_and(uncommitted)
            {
                self.warnings.push(format!(
                    "`{}` has uncommitted changes, which are not tethered; tethering takes what is committed",
                    dependency.source
                ));
            }
            let fallback = checkout
                .fallback_name()
                .unwrap_or_else(|| dependency.default_name());
            let mut published = published(&checkout.directory, &fallback);
            // `packages:` under the dependency selects the packages of the
            // repository to install.
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

            for package in &published {
                self.check_name(&package.name, &dependency.source)?;
            }

            let mut discarded = std::collections::HashMap::new();
            for package in &published {
                let unmodified = require_unmodified(self.project_root, &self.lock, &package.name);
                if unmodified.is_err() && self.diff && !self.force {
                    let edits = local_edits(
                        self.project_root,
                        &self.lock,
                        &package.name,
                        &fallback,
                        &self.scratch,
                    );
                    self.edited.push((package.name.clone(), edits));
                }
                match unmodified {
                    Ok(()) => {}
                    // With force, the installer discards the edits and reports
                    // it.
                    Err(_) if self.force => {
                        discarded.insert(
                            package.name.clone(),
                            local_changes(self.project_root, &self.lock, &package.name),
                        );
                    }
                    Err(failure) => return Err(failure),
                }
            }
            let checkout = std::rc::Rc::new(checkout);
            for package in &published {
                // A later, newer version replaces the plan of an earlier one.
                self.planned.retain(|earlier| earlier.package.name != package.name);
                self.planned.push(Planned {
                    package: package.clone(),
                    source: dependency.source.clone(),
                    pin: dependency.pin.clone(),
                    commit: checkout.commit.clone(),
                    discarded: discarded.remove(&package.name),
                    _checkout: checkout.clone(),
                });
            }

            // The dependencies of each package also become dependencies of
            // this project. There is no nested location for them. The
            // installer does not follow the project dependencies of the
            // repository. They are for work on that repository. They are not
            // for the use of its packages.
            //
            // A relative path there starts at the directory of the repository.
            // Thus the installer makes it absolute before it sends it.
            for required in published.iter().flat_map(|package| &package.dependencies) {
                let mut required = required.clone();
                if let Some(path) = checkout
                    .local
                    .as_deref()
                    .and_then(|local| required.local_path(local))
                {
                    required.source = std::fs::canonicalize(&path)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .to_string();
                }
                queue.push((required, None, checkout.local.clone()));
            }
        }
        Ok(())
    }

    /// Refuses a package name that a different package has.
    ///
    /// Two packages cannot use the same `tethers/<name>`. An install of one
    /// over the other changes the sense of each import of that name.
    fn check_name(&self, name: &str, source: &str) -> Outcome<()> {
        if self.own.iter().any(|own| own == name) {
            return Err(Failure::new(format!(
                "`{source}` publishes a package named `{name}`, which this project offers itself"
            ))
            .with_help("tether it under another name with `--as <name>`"));
        }
        if let Some(other) = self
            .planned
            .iter()
            .find(|planned| planned.package.name == name && planned.source != source)
        {
            return Err(Failure::new(format!(
                "`{}` and `{source}` both publish a package named `{name}`",
                other.source
            ))
            .with_help("only one package can be installed under a name; pick which one with `packages:` under the dependency"));
        }
        if self.guard_owners {
            if let Some(owner) = self.lock.get(name).filter(|entry| entry.source != source) {
                return Err(Failure::new(format!(
                    "`{source}` publishes a package named `{name}`, which is already tethered from `{}`",
                    owner.source
                ))
                .with_help(format!(
                    "tether it under another name with `--as <name>`, or `piton remove {name}` first"
                )));
            }
        }
        Ok(())
    }

    /// Writes each planned package to `tethers/`, and records it in the lock.
    pub fn finish(mut self) -> Outcome<(Lock, Vec<Placed>, Vec<String>)> {
        let planned = std::mem::take(&mut self.planned);
        let mut outcome = Ok(());
        for planned in &planned {
            let directory = packages::package_directory(self.project_root, &planned.package.name);
            let before = self.diff.then(|| read_tree(&directory));
            match install(
                self.project_root,
                &mut self.lock,
                &planned.package,
                &planned.source,
                &planned.pin,
                planned.commit.as_deref(),
            ) {
                Ok(mut placed) => {
                    if let Some(before) = before {
                        placed.changes = changes(before, read_tree(&directory));
                    }
                    placed.discarded = planned.discarded.clone();
                    self.placed.push(placed);
                }
                Err(failure) => {
                    outcome = Err(failure);
                    break;
                }
            }
        }
        // The checkouts drop before the scratch directory that contains them.
        drop(planned);
        let _ = std::fs::remove_dir_all(&self.scratch);
        outcome.map(|()| (self.lock, self.placed, self.warnings))
    }
}

/// An import that names a package, and the location of its path in the source.
pub struct ImportSite {
    pub file: PathBuf,
    pub start: usize,
    pub end: usize,
    pub written: String,
    /// True when the import is in an installed package.
    ///
    /// The code does not change these imports. An edit makes that package
    /// different from the lock file record. That state stops all updates of
    /// the package. Thus the code reports the import, and the person decides.
    pub managed: bool,
}

/// All imports in the project that go into `name`.
///
/// The parser finds them. A text search does not. Thus a package name in
/// prose, in a comment, or in a longer name does not count as an import.
pub fn imports_of(project: &Project, name: &str) -> Vec<ImportSite> {
    let mut found = Vec::new();
    let mut files = piton_compile::module::sources(&project.source_root);
    if project.root != project.source_root {
        files.extend(piton_compile::module::sources(&project.root));
    }
    // `sources` does not include installed packages. That is correct for all
    // reports on the code of the project. Here, they are important. A package
    // can import a different package by name. That import breaks in the same
    // way as all other imports.
    let tethers = project.root.join(packages::TETHERS);
    files.extend(piton_compile::module::sources(&tethers));
    files.sort();
    files.dedup();

    let inside = packages::package_directory(&project.root, name);
    for file in files {
        // An import in the package that moves is relative to the package. It
        // moves with the package.
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

/// Changes each import of `name` to `replacement`. `replacement` is a path
/// from the source root of the project.
///
/// Returns the number of changed files.
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
        // The edits go from the end to the start. Thus an earlier edit does not move a
        // later span.
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

/// Removes the directory of a package. When `tethers/` becomes empty, it
/// also removes `tethers/`.
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

/// Removes `tethers/` when it contains no installed packages.
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
