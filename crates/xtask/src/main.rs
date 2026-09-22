//! Repository automation, run as `cargo xtask <task>`.
//!
//! Tasks live here rather than in a shell script so they work the same on every
//! machine cargo runs on, and so they can be read and changed like the rest of
//! the code.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The binary the workspace produces.
const BINARY: &str = "piton";
/// The package that produces it.
const PACKAGE: &str = "piton-cli";

const USAGE: &str = "\
Repository automation for the Piton workspace.

Usage:
    cargo xtask <task> [options]

Tasks:
    install             Build the release binary and install it locally
    publish-grammar     Push the tree-sitter grammar to its own repository

Options for `install`:
    --root <dir>    Install into <dir>/bin instead of the cargo home
    --dry-run       Report what would happen without copying anything

Options for `publish-grammar`:
    --remote <url>  Publish somewhere other than the grammar's own repository
    --branch <name> Branch to publish (default: main)
    --tag <name>    Also create and push this tag
    -m, --message   Subject line for the publish commit
    --dry-run       Prepare the commit but do not push
    --allow-dirty   Publish although the grammar has uncommitted changes

Options:
    -h, --help      Print this message
";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let task = args.next();
    let rest: Vec<String> = args.collect();

    let result = match task.as_deref() {
        Some("install") => install(&rest),
        Some("publish-grammar") => publish_grammar(&rest),
        Some("-h") | Some("--help") | Some("help") | None => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Some(other) => Err(Error::Usage(format!("unknown task `{other}`"))),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            if matches!(error, Error::Usage(_)) {
                eprintln!();
                eprint!("{USAGE}");
            }
            ExitCode::FAILURE
        }
    }
}

// ---------------------------------------------------------------------------
// install
// ---------------------------------------------------------------------------

/// Builds the release binary and installs it into the local cargo bin
/// directory.
///
/// This builds in the workspace's own target directory and copies the result,
/// rather than shelling out to `cargo install --path`, which would rebuild
/// every dependency from scratch in a separate directory.
fn install(args: &[String]) -> Result<(), Error> {
    let options = InstallOptions::parse(args)?;

    let workspace = workspace_root()?;
    build_release(&workspace, options.dry_run)?;

    let built = target_directory(&workspace).join("release").join(file_name());
    if !options.dry_run && !built.is_file() {
        return Err(Error::Message(format!(
            "`{}` was not produced by the build",
            built.display()
        )));
    }

    let bin_directory = options.root.clone().unwrap_or(cargo_home()?).join("bin");
    let destination = bin_directory.join(file_name());

    if options.dry_run {
        println!("would install {} -> {}", built.display(), destination.display());
        return Ok(());
    }

    fs::create_dir_all(&bin_directory).map_err(|error| {
        Error::Message(format!(
            "cannot create `{}`: {error}",
            bin_directory.display()
        ))
    })?;

    replace(&built, &destination).map_err(|error| {
        Error::Message(format!(
            "cannot install to `{}`: {error}",
            destination.display()
        ))
    })?;

    println!("installed {} to {}", BINARY, destination.display());
    if !on_path(&bin_directory) {
        // Installing something the shell cannot find is a confusing outcome, so
        // say so rather than reporting success and leaving it at that.
        println!(
            "note: `{}` is not on PATH; add it with\n      export PATH=\"{}:$PATH\"",
            bin_directory.display(),
            bin_directory.display()
        );
    }
    Ok(())
}

#[derive(Debug, Default)]
struct InstallOptions {
    root: Option<PathBuf>,
    dry_run: bool,
}

impl InstallOptions {
    fn parse(args: &[String]) -> Result<InstallOptions, Error> {
        let mut options = InstallOptions::default();
        let mut iter = args.iter();
        while let Some(argument) = iter.next() {
            match argument.as_str() {
                "--root" => {
                    let value = iter.next().ok_or_else(|| {
                        Error::Usage("`--root` needs a directory".to_string())
                    })?;
                    options.root = Some(PathBuf::from(value));
                }
                "--dry-run" => options.dry_run = true,
                "-h" | "--help" => {
                    print!("{USAGE}");
                    std::process::exit(0);
                }
                other => {
                    return Err(Error::Usage(format!("unexpected argument `{other}`")));
                }
            }
        }
        Ok(options)
    }
}

fn build_release(workspace: &Path, dry_run: bool) -> Result<(), Error> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    command
        .current_dir(workspace)
        .args(["build", "--release", "--package", PACKAGE]);

    if dry_run {
        println!("would run: cargo build --release --package {PACKAGE}");
        return Ok(());
    }

    let status = command
        .status()
        .map_err(|error| Error::Message(format!("cannot run cargo: {error}")))?;
    if !status.success() {
        return Err(Error::Message("the release build failed".to_string()));
    }
    Ok(())
}

/// Copies `source` over `destination` through a staging file.
///
/// Writing directly over a binary that is currently running fails with "text
/// file busy" on Linux. Renaming onto the destination replaces the directory
/// entry instead, which works whether or not the old binary is in use.
fn replace(source: &Path, destination: &Path) -> io::Result<()> {
    let staging = destination.with_file_name(format!(".{}.new", file_name()));
    let _ = fs::remove_file(&staging);
    fs::copy(source, &staging)?;
    copy_permissions(source, &staging)?;
    match fs::rename(&staging, destination) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&staging);
            Err(error)
        }
    }
}

#[cfg(unix)]
fn copy_permissions(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(source)?.permissions().mode();
    fs::set_permissions(destination, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn copy_permissions(_source: &Path, _destination: &Path) -> io::Result<()> {
    Ok(())
}

// ---------------------------------------------------------------------------
// Locations
// ---------------------------------------------------------------------------

fn file_name() -> String {
    format!("{BINARY}{}", std::env::consts::EXE_SUFFIX)
}

/// The workspace root, found from this crate's manifest directory.
fn workspace_root() -> Result<PathBuf, Error> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .ok_or_else(|| Error::Message("cannot locate the workspace root".to_string()))
}

/// Where cargo puts build output, honouring `CARGO_TARGET_DIR`.
fn target_directory(workspace: &Path) -> PathBuf {
    match std::env::var_os("CARGO_TARGET_DIR") {
        Some(value) if !value.is_empty() => {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                path
            } else {
                workspace.join(path)
            }
        }
        _ => workspace.join("target"),
    }
}

/// The directory `cargo install` treats as its root, without a `--root` of our
/// own: `CARGO_INSTALL_ROOT`, then `CARGO_HOME`, then `~/.cargo`.
fn cargo_home() -> Result<PathBuf, Error> {
    for variable in ["CARGO_INSTALL_ROOT", "CARGO_HOME"] {
        if let Some(value) = std::env::var_os(variable) {
            if !value.is_empty() {
                return Ok(PathBuf::from(value));
            }
        }
    }
    home_directory()
        .map(|home| home.join(".cargo"))
        .ok_or_else(|| {
            Error::Message(
                "cannot find a cargo home; set CARGO_HOME or pass --root".to_string(),
            )
        })
}

fn home_directory() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// True when `directory` is one of the entries in `PATH`.
fn on_path(directory: &Path) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|entry| entry == directory)
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum Error {
    /// The command line was wrong; usage is worth printing alongside it.
    Usage(String),
    Message(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Usage(message) | Error::Message(message) => f.write_str(message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_options_parse() {
        let options =
            InstallOptions::parse(&["--root".into(), "/tmp/here".into(), "--dry-run".into()])
                .expect("parse");
        assert_eq!(options.root.as_deref(), Some(Path::new("/tmp/here")));
        assert!(options.dry_run);
    }

    #[test]
    fn a_root_without_a_value_is_a_usage_error() {
        let error = InstallOptions::parse(&["--root".into()]).expect_err("error");
        assert!(matches!(error, Error::Usage(_)));
    }

    #[test]
    fn unexpected_arguments_are_rejected() {
        let error = InstallOptions::parse(&["--nope".into()]).expect_err("error");
        assert!(matches!(error, Error::Usage(_)));
    }

    #[test]
    fn the_target_directory_follows_cargo_target_dir() {
        let workspace = Path::new("/w");
        // The variable is process-wide, so only the default is asserted here;
        // the override path is exercised by the integration test.
        if std::env::var_os("CARGO_TARGET_DIR").is_none() {
            assert_eq!(target_directory(workspace), Path::new("/w/target"));
        }
    }

    #[test]
    fn the_binary_name_carries_the_platform_suffix() {
        assert!(file_name().starts_with(BINARY));
        assert!(file_name().ends_with(std::env::consts::EXE_SUFFIX));
    }

    #[test]
    fn replacing_a_file_preserves_the_executable_bit() {
        let dir = std::env::temp_dir().join(format!("xtask-replace-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        let source = dir.join("source");
        let destination = dir.join("destination");
        fs::write(&source, b"binary").expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&source, fs::Permissions::from_mode(0o755)).expect("chmod");
        }

        replace(&source, &destination).expect("replace");
        assert_eq!(fs::read(&destination).expect("read"), b"binary");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&destination).expect("stat").permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "the copy must stay executable");
        }
        // No staging file is left behind.
        assert!(!dir.join(format!(".{}.new", file_name())).exists());
        let _ = fs::remove_dir_all(&dir);
    }
}

// ---------------------------------------------------------------------------
// publish-grammar
// ---------------------------------------------------------------------------

/// Where the grammar lives in this repository.
const GRAMMAR_DIRECTORY: &str = "editors/tree-sitter-piton";
/// Where it is published to.
const GRAMMAR_REMOTE: &str = "git@github.com:piton-lang/tree-sitter-piton.git";

/// Copies the tree-sitter grammar into its own repository and pushes it.
///
/// The grammar is developed here, next to the compiler, because that is the
/// only place it can be checked against the language: `piton-syntax`'s keyword
/// list is the source of truth, and a test reads the grammar back to make sure
/// the two agree. Editors that consume tree-sitter grammars expect one
/// repository per grammar, so it is published as a copy rather than developed
/// as one.
///
/// The published history is kept: the remote is cloned, its contents replaced,
/// and the result committed. Each publish is one commit naming the source
/// revision it came from, so a reader of either repository can line them up.
fn publish_grammar(args: &[String]) -> Result<(), Error> {
    let options = PublishOptions::parse(args)?;
    let workspace = workspace_root()?;
    let source = workspace.join(GRAMMAR_DIRECTORY);
    if !source.is_dir() {
        return Err(Error::Message(format!(
            "`{}` does not exist",
            source.display()
        )));
    }

    // What is published has to be something a reader can find again, so the
    // working tree must match what is committed.
    let dirty = git_output(
        &workspace,
        &["status", "--porcelain", "--", GRAMMAR_DIRECTORY],
    )?;
    if !dirty.trim().is_empty() && !options.allow_dirty {
        return Err(Error::Message(format!(
            "`{GRAMMAR_DIRECTORY}` has uncommitted changes, so the published \
             commit would name a revision that does not contain them:\n{}\n\
             Commit them, or pass --allow-dirty to publish anyway.",
            dirty.trim_end()
        )));
    }
    let revision = git_output(&workspace, &["rev-parse", "HEAD"])?
        .trim()
        .to_string();
    let short = revision.chars().take(12).collect::<String>();

    identity(&workspace)?;

    let staging = staging_directory()?;
    let checkout = staging.join("tree-sitter-piton");
    let _cleanup = Cleanup(staging.clone());

    // Clone what is there, so the published history continues rather than
    // restarting. An empty remote has nothing to clone, which is not an error.
    let cloned = clone(&options.remote, &options.branch, &checkout)?;
    if !cloned {
        println!("the remote has no `{}` branch yet; starting one", options.branch);
        git(&staging, &["init", "--quiet", "--initial-branch", &options.branch, "tree-sitter-piton"])?;
        git(&checkout, &["remote", "add", "origin", &options.remote])?;
    }

    clear(&checkout)?;
    copy_tree(&source, &checkout)?;

    generate_parser(&checkout, options.dry_run);

    git(&checkout, &["add", "--all"])?;
    let staged = git_output(&checkout, &["status", "--porcelain"])?;
    if staged.trim().is_empty() {
        println!("the published grammar already matches {short}; nothing to do");
        return Ok(());
    }

    println!("publishing {} to {}", GRAMMAR_DIRECTORY, options.remote);
    for line in staged.lines() {
        println!("  {line}");
    }

    let message = options
        .message
        .clone()
        .unwrap_or_else(|| format!("Update grammar from piton@{short}"));
    let body = format!(
        "{message}\n\nGenerated by `cargo xtask publish-grammar` from\n\
         https://github.com/piton-lang/piton at {revision}.\n"
    );
    git(&checkout, &["commit", "--quiet", "--message", &body])?;

    if let Some(tag) = &options.tag {
        git(&checkout, &["tag", "--force", tag])?;
    }

    if options.dry_run {
        println!(
            "dry run: not pushing. The commit is prepared in {}",
            checkout.display()
        );
        // Kept so it can be inspected, since that is the point of a dry run.
        std::mem::forget(_cleanup);
        return Ok(());
    }

    git(&checkout, &["push", "origin", &options.branch])?;
    let pushed = git_output(&checkout, &["rev-parse", "HEAD"])?.trim().to_string();
    pin_zed_grammar(&workspace, &pushed)?;
    if let Some(tag) = &options.tag {
        git(&checkout, &["push", "--force", "origin", tag])?;
        println!("pushed {} and tag {tag}", options.branch);
    } else {
        println!("pushed {}", options.branch);
    }
    Ok(())
}

#[derive(Debug)]
struct PublishOptions {
    remote: String,
    branch: String,
    tag: Option<String>,
    message: Option<String>,
    dry_run: bool,
    allow_dirty: bool,
}

impl Default for PublishOptions {
    fn default() -> PublishOptions {
        PublishOptions {
            remote: GRAMMAR_REMOTE.to_string(),
            branch: "main".to_string(),
            tag: None,
            message: None,
            dry_run: false,
            allow_dirty: false,
        }
    }
}

impl PublishOptions {
    fn parse(args: &[String]) -> Result<PublishOptions, Error> {
        let mut options = PublishOptions::default();
        let mut iter = args.iter();
        while let Some(argument) = iter.next() {
            let mut value = |name: &str| -> Result<String, Error> {
                iter.next()
                    .cloned()
                    .ok_or_else(|| Error::Usage(format!("`{name}` needs a value")))
            };
            match argument.as_str() {
                "--remote" => options.remote = value("--remote")?,
                "--branch" => options.branch = value("--branch")?,
                "--tag" => options.tag = Some(value("--tag")?),
                "--message" | "-m" => options.message = Some(value("--message")?),
                "--dry-run" => options.dry_run = true,
                "--allow-dirty" => options.allow_dirty = true,
                "-h" | "--help" => {
                    print!("{USAGE}");
                    std::process::exit(0);
                }
                other => {
                    return Err(Error::Usage(format!("unknown option `{other}`")));
                }
            }
        }
        Ok(options)
    }
}

/// Runs git in a directory, failing if it does.
fn git(directory: &Path, args: &[&str]) -> Result<(), Error> {
    let status = Command::new("git")
        .args(args)
        .current_dir(directory)
        .status()
        .map_err(|error| Error::Message(format!("cannot run git: {error}")))?;
    if !status.success() {
        return Err(Error::Message(format!(
            "`git {}` failed in {}",
            args.join(" "),
            directory.display()
        )));
    }
    Ok(())
}

/// Runs git and returns what it printed.
fn git_output(directory: &Path, args: &[&str]) -> Result<String, Error> {
    let output = Command::new("git")
        .args(args)
        .current_dir(directory)
        .output()
        .map_err(|error| Error::Message(format!("cannot run git: {error}")))?;
    if !output.status.success() {
        return Err(Error::Message(format!(
            "`git {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Checks that git knows who is committing.
///
/// Git's own error for this is several paragraphs long and arrives after the
/// work is done, which is a poor moment to learn it.
fn identity(workspace: &Path) -> Result<(), Error> {
    for key in ["user.name", "user.email"] {
        let set = Command::new("git")
            .args(["config", "--get", key])
            .current_dir(workspace)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        if !set {
            return Err(Error::Message(format!(
                "git has no `{key}`, so the publish commit would have no author.\n\
                 Set it with `git config --global {key} \"...\"`."
            )));
        }
    }
    Ok(())
}

/// Clones a branch of a remote, shallowly. False when the branch is not there.
fn clone(remote: &str, branch: &str, into: &Path) -> Result<bool, Error> {
    let parent = into
        .parent()
        .ok_or_else(|| Error::Message("no staging directory".to_string()))?;
    let output = Command::new("git")
        .args([
            "clone",
            "--quiet",
            "--depth",
            "1",
            "--branch",
            branch,
            remote,
            "tree-sitter-piton",
        ])
        .current_dir(parent)
        .output()
        .map_err(|error| Error::Message(format!("cannot run git: {error}")))?;
    if output.status.success() {
        return Ok(true);
    }
    let message = String::from_utf8_lossy(&output.stderr).to_string();
    // A remote that exists but has no such branch, or no commits at all, is
    // the first publish rather than a failure.
    let empty = message.contains("not found in upstream origin")
        || message.contains("Remote branch")
        || message.contains("empty repository")
        || message.contains("does not appear to be a git repository");
    if empty {
        let _ = fs::remove_dir_all(into);
        return Ok(false);
    }
    Err(Error::Message(format!(
        "cannot clone {remote}: {}",
        message.trim()
    )))
}

/// Empties a checkout of everything but its `.git` directory.
///
/// A file deleted here is deleted in the published repository, which is what
/// makes the publish a copy rather than an accumulation.
fn clear(checkout: &Path) -> Result<(), Error> {
    let entries = fs::read_dir(checkout).map_err(|error| {
        Error::Message(format!("cannot read `{}`: {error}", checkout.display()))
    })?;
    for entry in entries.flatten() {
        if entry.file_name() == ".git" {
            continue;
        }
        let path = entry.path();
        let result = if path.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        result.map_err(|error| {
            Error::Message(format!("cannot remove `{}`: {error}", path.display()))
        })?;
    }
    Ok(())
}

/// Copies a directory tree.
fn copy_tree(from: &Path, to: &Path) -> Result<(), Error> {
    fs::create_dir_all(to)
        .map_err(|error| Error::Message(format!("cannot create `{}`: {error}", to.display())))?;
    let entries = fs::read_dir(from)
        .map_err(|error| Error::Message(format!("cannot read `{}`: {error}", from.display())))?;
    for entry in entries.flatten() {
        let source = entry.path();
        let destination = to.join(entry.file_name());
        if source.is_dir() {
            copy_tree(&source, &destination)?;
        } else {
            fs::copy(&source, &destination).map_err(|error| {
                Error::Message(format!("cannot copy `{}`: {error}", source.display()))
            })?;
        }
    }
    Ok(())
}

/// Generates the parser, when the tree-sitter CLI is available.
///
/// A published grammar is more useful with `src/parser.c` in it, since then a
/// consumer needs no toolchain. It is not required, so a missing CLI is a note
/// rather than a failure.
fn generate_parser(checkout: &Path, dry_run: bool) {
    let available = Command::new("tree-sitter")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !available {
        println!(
            "note: `tree-sitter` is not installed, so the published grammar will \
             carry no generated parser.\n      Consumers will need to run \
             `tree-sitter generate` themselves."
        );
        return;
    }
    if dry_run {
        println!("would run `tree-sitter generate`");
        return;
    }
    let status = Command::new("tree-sitter")
        .arg("generate")
        .current_dir(checkout)
        .status();
    match status {
        Ok(status) if status.success() => println!("generated the parser"),
        _ => println!("note: `tree-sitter generate` failed; publishing the grammar alone"),
    }
}

/// A temporary directory to work in.
fn staging_directory() -> Result<PathBuf, Error> {
    let base = std::env::temp_dir().join(format!(
        "piton-publish-grammar-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&base)
        .map_err(|error| Error::Message(format!("cannot create `{}`: {error}", base.display())))?;
    Ok(base)
}

/// Removes the staging directory when the task ends, however it ends.
struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod publish_tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "piton-xtask-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("scratch directory");
        path
    }

    #[test]
    fn clearing_a_checkout_keeps_its_git_directory() {
        // Everything else in the checkout is replaced on each publish. Losing
        // `.git` would lose the published history and turn every publish into
        // a new repository.
        let checkout = scratch("clear");
        fs::create_dir_all(checkout.join(".git/objects")).unwrap();
        fs::write(checkout.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::create_dir_all(checkout.join("queries")).unwrap();
        fs::write(checkout.join("queries/highlights.scm"), "old").unwrap();
        fs::write(checkout.join("grammar.js"), "old").unwrap();

        clear(&checkout).expect("clear");

        assert!(checkout.join(".git/HEAD").is_file(), "`.git` must survive");
        assert!(!checkout.join("grammar.js").exists());
        assert!(!checkout.join("queries").exists());
    }

    #[test]
    fn copying_the_grammar_takes_the_whole_tree() {
        let base = scratch("copy");
        let source = base.join("source");
        let destination = base.join("destination");
        fs::create_dir_all(source.join("queries")).unwrap();
        fs::write(source.join("grammar.js"), "module.exports = {};").unwrap();
        fs::write(source.join("queries/highlights.scm"), "(anchor) @type").unwrap();

        copy_tree(&source, &destination).expect("copy");

        assert_eq!(
            fs::read_to_string(destination.join("grammar.js")).unwrap(),
            "module.exports = {};"
        );
        assert_eq!(
            fs::read_to_string(destination.join("queries/highlights.scm")).unwrap(),
            "(anchor) @type"
        );
    }

    #[test]
    fn a_publish_defaults_to_the_grammars_own_repository() {
        let options = PublishOptions::parse(&[]).expect("defaults");
        assert_eq!(options.remote, GRAMMAR_REMOTE);
        assert_eq!(options.branch, "main");
        // Publishing is the point of the task, so it is not a dry run by
        // default -- but it does refuse to publish uncommitted work.
        assert!(!options.dry_run);
        assert!(!options.allow_dirty);
    }

    #[test]
    fn an_unknown_option_is_a_usage_error() {
        let args = vec!["--push-it".to_string()];
        assert!(matches!(
            PublishOptions::parse(&args),
            Err(Error::Usage(_))
        ));
    }

    #[test]
    fn the_grammar_directory_is_where_the_grammar_is() {
        // The task copies a path spelled out as a constant; a moved grammar
        // would otherwise publish an empty repository.
        let source = workspace_root().expect("workspace").join(GRAMMAR_DIRECTORY);
        assert!(source.join("grammar.js").is_file(), "{}", source.display());
        assert!(source.join("queries").is_dir(), "{}", source.display());
    }
}

/// Where the Zed extension pins the grammar it fetches.
const ZED_EXTENSION: &str = "editors/zed/extension.toml";

/// Points the Zed extension at the commit that was just published.
///
/// Zed fetches one revision of the grammar repository and builds it. A pin
/// left behind keeps every Zed user on an older grammar than the one that was
/// just pushed, and a pin that is not a real commit fetches nothing at all,
/// which Zed reports as a grammar that will not compile. Since publishing is
/// the moment a new commit exists, it is also the moment to record it.
fn pin_zed_grammar(workspace: &Path, commit: &str) -> Result<(), Error> {
    let path = workspace.join(ZED_EXTENSION);
    let Ok(text) = fs::read_to_string(&path) else {
        // The extension is optional; publishing the grammar does not depend on
        // it being there.
        return Ok(());
    };

    let mut out = String::with_capacity(text.len());
    let mut in_grammar = false;
    let mut pinned = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_grammar = trimmed == "[grammars.piton]";
        }
        if in_grammar && trimmed.starts_with("commit") && !pinned {
            out.push_str(&format!("commit = \"{commit}\"\n"));
            pinned = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }

    if !pinned {
        println!("note: no grammar commit to pin in {ZED_EXTENSION}");
        return Ok(());
    }
    if out == text {
        return Ok(());
    }
    fs::write(&path, out)
        .map_err(|error| Error::Message(format!("cannot write `{}`: {error}", path.display())))?;
    println!("pinned {ZED_EXTENSION} to {}", &commit[..commit.len().min(12)]);
    println!("note: that is a change to this repository; commit it so Zed users get it");
    Ok(())
}
