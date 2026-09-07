//! Development tasks for the Piton workspace.
//!
//! Run these with `cargo xtask <command>`; the alias lives in
//! `.cargo/config.toml`. The point of the pattern is that installing the
//! compiler is a checked-in, reproducible command rather than a copy-paste
//! recipe in the README.

use std::env;
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};

/// The binary this workspace produces.
const PACKAGE: &str = "piton-cli";
/// The name it is installed under.
const BINARY: &str = "piton";

#[derive(Parser)]
#[command(name = "xtask", about = "Development tasks for the Piton workspace")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build the piton CLI and print where the binary landed.
    Build(BuildArgs),
    /// Build the piton CLI and copy it onto your PATH.
    Install(InstallArgs),
    /// Remove a previously installed piton binary.
    Uninstall(Destination),
    /// Check that the Zed extension compiles to WebAssembly.
    Zed,
    /// Regenerate every editor integration, including the Tree-sitter parser.
    Grammar,
    /// Publish the Tree-sitter grammar subtree and pin the editors to it.
    PublishGrammar(PublishArgs),
}

#[derive(Args)]
struct PublishArgs {
    /// Git remote to push to, by name or URL. Defaults to the `grammar` remote.
    #[arg(long, value_name = "REMOTE", default_value = piton_grammar::GRAMMAR_REMOTE_NAME)]
    remote: String,
    /// Branch to publish onto.
    #[arg(long, default_value = "main")]
    branch: String,
    /// Show what would happen without committing or pushing.
    #[arg(long)]
    dry_run: bool,
    /// Fail instead of committing on your behalf.
    #[arg(long)]
    no_commit: bool,
    /// Overwrite the remote branch. Needed only after rewriting history here.
    ///
    /// `git subtree push` cannot force, so this splits and pushes by hand.
    #[arg(long)]
    force: bool,
}

#[derive(Args)]
struct BuildArgs {
    /// Build without optimisations, for a faster edit-test loop.
    #[arg(long)]
    debug: bool,
}

#[derive(Args)]
struct InstallArgs {
    #[command(flatten)]
    build: BuildArgs,
    #[command(flatten)]
    destination: Destination,
}

#[derive(Args)]
struct Destination {
    /// Directory to install into. Defaults to Cargo's binary directory.
    #[arg(long, value_name = "DIR")]
    dest: Option<PathBuf>,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Build(args) => {
            let binary = build(!args.debug)?;
            println!("{}", binary.display());
        }
        Command::Install(args) => install(&args)?,
        Command::Uninstall(destination) => uninstall(&destination)?,
        Command::Zed => zed()?,
        Command::Grammar => regenerate_editors(None, None, true)?,
        Command::PublishGrammar(args) => publish_grammar(&args)?,
    }
    Ok(())
}

// ---- building --------------------------------------------------------------

/// Build the CLI and return the path cargo says it wrote.
///
/// The path comes from cargo's own JSON output rather than being guessed from
/// `target/`, so a custom `CARGO_TARGET_DIR` or `--target` still works.
fn build(release: bool) -> Result<PathBuf> {
    let mut command = Process::new(cargo());
    command
        .current_dir(workspace_root())
        .arg("build")
        .args(["--package", PACKAGE])
        .args(["--message-format", "json-render-diagnostics"])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if release {
        command.arg("--release");
    }

    let mut child = command.spawn().with_context(|| format!("running {:?}", cargo()))?;
    let mut messages = String::new();
    child
        .stdout
        .take()
        .expect("stdout was piped")
        .read_to_string(&mut messages)
        .context("reading cargo's output")?;
    let status = child.wait().context("waiting for cargo")?;
    if !status.success() {
        bail!("cargo build failed");
    }

    executable(&messages)
        .context("cargo did not report a `piton` executable; did the binary target change?")
}

/// Find the `piton` binary among cargo's build messages.
fn executable(messages: &str) -> Option<PathBuf> {
    messages
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| message["reason"] == "compiler-artifact")
        .filter(|message| message["target"]["name"] == BINARY)
        .filter_map(|message| message["executable"].as_str().map(PathBuf::from))
        .next_back()
}

// ---- installing ------------------------------------------------------------

fn install(args: &InstallArgs) -> Result<()> {
    let built = build(!args.build.debug)?;
    let directory = args.destination.resolve()?;
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("creating {}", directory.display()))?;

    let target = directory.join(format!("{BINARY}{}", env::consts::EXE_SUFFIX));
    let existing = target.symlink_metadata().ok();
    if let Some(metadata) = &existing {
        if metadata.is_dir() {
            bail!("{} is a directory, not a binary", target.display());
        }
        if metadata.is_symlink() {
            let points_at = std::fs::read_link(&target).unwrap_or_default();
            println!("replacing symlink {} -> {}", target.display(), points_at.display());
        } else {
            println!("replacing {}", target.display());
        }
        // Unlinking first rather than writing over the file in place, so that
        // a copy running right now keeps working.
        std::fs::remove_file(&target)
            .with_context(|| format!("removing {}", target.display()))?;
    }

    std::fs::copy(&built, &target)
        .with_context(|| format!("copying {} to {}", built.display(), target.display()))?;

    println!("installed {} -> {}", built.display(), target.display());
    report_version(&target)?;
    warn_if_unreachable(&directory);
    Ok(())
}

fn uninstall(destination: &Destination) -> Result<()> {
    let directory = destination.resolve()?;
    let target = directory.join(format!("{BINARY}{}", env::consts::EXE_SUFFIX));
    match target.symlink_metadata() {
        Ok(metadata) if metadata.is_dir() => bail!("{} is a directory", target.display()),
        Ok(_) => {
            std::fs::remove_file(&target)
                .with_context(|| format!("removing {}", target.display()))?;
            println!("removed {}", target.display());
        }
        Err(_) => println!("nothing installed at {}", target.display()),
    }
    Ok(())
}

// ---- the grammar subtree ---------------------------------------------------

/// The directory published as its own repository.
const GRAMMAR_PREFIX: &str = "editors/tree-sitter-piton";

/// Regenerate `editors/`, then refresh the checked-in Tree-sitter parser.
///
/// `src/parser.c` is committed on purpose: Zed and Helix compile it, they do
/// not run the Tree-sitter CLI, so a publish without it produces a grammar the
/// editors cannot build.
fn regenerate_editors(
    repository: Option<&str>,
    rev: Option<&str>,
    parser_optional: bool,
) -> Result<()> {
    let root = workspace_root();
    let mut command = Process::new(cargo());
    command
        .current_dir(&root)
        .args(["run", "--quiet", "--package", PACKAGE, "--", "grammar", "editors"]);
    if let Some(repository) = repository {
        command.args(["--grammar-repo", repository]);
    }
    if let Some(rev) = rev {
        command.args(["--grammar-rev", rev]);
    }
    let status = command.status().context("regenerating editors")?;
    if !status.success() {
        bail!("`piton grammar` failed");
    }

    let grammar = root.join(GRAMMAR_PREFIX);
    if !have("tree-sitter") {
        let message = "the tree-sitter CLI is not installed; \
                       install it with `cargo install tree-sitter-cli`";
        if parser_optional {
            println!("note: {message}, so src/parser.c was left as it was");
            return Ok(());
        }
        bail!("{message}");
    }
    let status = Process::new("tree-sitter")
        .current_dir(&grammar)
        .arg("generate")
        .status()
        .context("running tree-sitter generate")?;
    if !status.success() {
        bail!("tree-sitter generate failed");
    }
    println!("regenerated {} and its parser", grammar.display());
    Ok(())
}

/// Push the grammar subtree to its own repository and record the commit.
fn publish_grammar(args: &PublishArgs) -> Result<()> {
    let root = workspace_root();
    if !root.join(".git").exists() {
        bail!("{} is not a git repository", root.display());
    }
    if git(&root, &["rev-parse", "--verify", "HEAD"]).is_err() {
        bail!(
            "this repository has no commits yet; `git subtree` needs history. \
             Make an initial commit first."
        );
    }
    let url = remote_url(&root, &args.remote)?;

    // The published README names the remote, so generate before splitting.
    regenerate_editors(Some(&url), None, false)?;

    let pending = git(&root, &["status", "--porcelain", "--", GRAMMAR_PREFIX])?;
    if !pending.trim().is_empty() {
        println!("{GRAMMAR_PREFIX} has changes:");
        for line in pending.lines() {
            println!("  {line}");
        }
        if args.no_commit {
            bail!("commit {GRAMMAR_PREFIX} first, or drop --no-commit");
        }
        if !args.dry_run {
            git(&root, &["add", "--", GRAMMAR_PREFIX])?;
            git(&root, &["commit", "-m", "grammar: regenerate", "--", GRAMMAR_PREFIX])?;
            println!("committed {GRAMMAR_PREFIX}");
        }
    }

    let prefix = format!("--prefix={GRAMMAR_PREFIX}");
    if args.dry_run {
        let sha = git(&root, &["subtree", "split", &prefix])?.trim().to_string();
        println!("dry run: would push {sha} to {url} ({})", args.branch);
        return Ok(());
    }

    if args.force {
        // `git subtree push` has no --force, so do what it does and force it.
        let sha = git(&root, &["subtree", "split", &prefix])?.trim().to_string();
        let refspec = format!("{sha}:refs/heads/{}", args.branch);
        git(&root, &["push", "--force", &args.remote, &refspec])?;
    } else {
        git(&root, &["subtree", "push", &prefix, &args.remote, &args.branch])?;
    }

    // Ask the remote what it now has, rather than trusting a local split.
    let sha = remote_head(&root, &args.remote, &args.branch)?;
    println!("published {sha} to {url} ({})", args.branch);

    // Now that the commit exists, the editors that fetch it can be pinned.
    regenerate_editors(Some(&url), Some(&sha), false)?;
    if !args.no_commit {
        let pending = git(&root, &["status", "--porcelain", "--", "editors"])?;
        if !pending.trim().is_empty() {
            let message = format!("editors: pin grammar to {}", &sha[..12.min(sha.len())]);
            git(&root, &["add", "--", "editors"])?;
            git(&root, &["commit", "-m", &message, "--", "editors"])?;
            println!("committed the grammar pin");
        }
    }
    Ok(())
}

/// Resolve a remote name to its URL, so nothing has to hard-code one.
///
/// A `--remote` that already looks like a URL is passed through, which is what
/// makes a one-off publish to somewhere else possible.
fn remote_url(root: &Path, remote: &str) -> Result<String> {
    if let Ok(url) = git(root, &["remote", "get-url", remote]) {
        return Ok(url.trim().to_string());
    }
    if remote.contains("://") || remote.contains('@') || remote.starts_with('/') {
        return Ok(remote.to_string());
    }
    bail!(
        "there is no git remote called `{remote}`. Point it at the grammar's \
         repository once:\n    git remote add {remote} <url>"
    )
}

/// The commit a branch points at on the remote.
fn remote_head(root: &Path, remote: &str, branch: &str) -> Result<String> {
    let listing = git(root, &["ls-remote", remote, &format!("refs/heads/{branch}")])?;
    listing
        .split_whitespace()
        .next()
        .map(str::to_string)
        .filter(|sha| sha.len() == 40)
        .with_context(|| format!("{remote} has no branch `{branch}` after the push"))
}

/// Run git in the workspace, returning its stdout.
fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Process::new("git")
        .current_dir(root)
        .args(args)
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!("git {} failed", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn have(program: &str) -> bool {
    Process::new(program)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

// ---- editors ---------------------------------------------------------------

/// The WebAssembly target Zed builds extensions for.
const ZED_TARGET: &str = "wasm32-wasip1";

/// Build the Zed extension the way Zed will.
///
/// Zed compiles the extension itself when you install it, so this only exists
/// to find a broken extension here rather than inside the editor.
fn zed() -> Result<()> {
    let directory = workspace_root().join("editors").join("zed");
    if !directory.join("Cargo.toml").is_file() {
        bail!("{} is missing; run `piton grammar editors` first", directory.display());
    }
    if !target_installed(ZED_TARGET)? {
        bail!("the {ZED_TARGET} target is not installed; run `rustup target add {ZED_TARGET}`");
    }

    let status = Process::new(cargo())
        .current_dir(&directory)
        .args(["build", "--release", "--target", ZED_TARGET])
        .status()
        .with_context(|| format!("building {}", directory.display()))?;
    if !status.success() {
        bail!("the Zed extension failed to build");
    }

    let artifact = directory
        .join("target")
        .join(ZED_TARGET)
        .join("release")
        .join("zed_piton.wasm");
    println!("{}", artifact.display());
    println!(
        "note: install it with `zed: install dev extension` and pick {}",
        directory.display()
    );
    Ok(())
}

/// Ask rustc whether a target's standard library is available.
fn target_installed(target: &str) -> Result<bool> {
    let output = Process::new("rustup")
        .args(["target", "list", "--installed"])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8_lossy(&output.stdout).lines().any(|line| line.trim() == target))
        }
        // No rustup: assume the toolchain was installed some other way.
        _ => Ok(true),
    }
}

/// Run the freshly installed binary so the command fails loudly if it cannot.
fn report_version(binary: &Path) -> Result<()> {
    let output = Process::new(binary)
        .arg("--version")
        .output()
        .with_context(|| format!("running {}", binary.display()))?;
    if !output.status.success() {
        bail!("{} did not run", binary.display());
    }
    print!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

impl Destination {
    /// Where to install: `--dest`, else Cargo's binary directory.
    fn resolve(&self) -> Result<PathBuf> {
        if let Some(directory) = &self.dest {
            return Ok(directory.clone());
        }
        if let Some(root) = env::var_os("CARGO_INSTALL_ROOT") {
            return Ok(PathBuf::from(root).join("bin"));
        }
        if let Some(home) = env::var_os("CARGO_HOME") {
            return Ok(PathBuf::from(home).join("bin"));
        }
        if let Some(home) = home() {
            return Ok(home.join(".cargo").join("bin"));
        }
        bail!("cannot find Cargo's binary directory; pass --dest")
    }
}

fn home() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Say something if the install directory is not on `PATH`.
fn warn_if_unreachable(directory: &Path) {
    let wanted = directory.canonicalize().unwrap_or_else(|_| directory.to_path_buf());
    let on_path = env::var_os("PATH")
        .map(|path| {
            env::split_paths(&path).any(|entry| {
                entry.canonicalize().unwrap_or(entry) == wanted
            })
        })
        .unwrap_or(false);
    if !on_path {
        println!(
            "note: {} is not on PATH; add it, or run the binary by its full path",
            directory.display()
        );
    }
}

// ---- workspace plumbing -----------------------------------------------------

/// The cargo that invoked this task, so the toolchain stays consistent.
fn cargo() -> OsString {
    env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

/// The workspace root: this crate's directory is `<root>/xtask`.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives inside the workspace")
        .to_path_buf()
}
