//! Automation for the repository. Run it as `cargo xtask <task>`.
//!
//! The tasks are here and not in a shell script. Thus they work the same on
//! each machine that runs cargo. Also, you can read and change them as you do
//! the rest of the code.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The binary that the workspace makes.
const BINARY: &str = "piton";
/// The package that makes the binary.
const PACKAGE: &str = "piton-cli";

const USAGE: &str = "\
Repository automation for the Piton workspace.

Usage:
    cargo xtask <task> [options]

Tasks:
    install             Build the release binary and install it locally
    publish-grammar     Push the tree-sitter grammar to its own repository
    sync-fluency        Regenerate FLUENCY_PROMPT.md from the spec with Claude, then
                        copy it into the compiler's built-in copy

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

Options for `sync-fluency`:
    --check         Generate and copy nothing; exit 1 if the copies differ
    --copy-only     Don't regenerate; copy FLUENCY_PROMPT.md as it is
    --model <name>  The model Claude generates with, like opus; without one,
                    Claude's default is used and its models are listed

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
        Some("sync-fluency") => sync_fluency(&rest),
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
/// This task builds in the target directory of the workspace and copies the
/// result. It does not call `cargo install --path`. That command builds each
/// dependency again from the start in a different directory.
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
        // The user can become confused when the shell cannot find the new
        // binary. Thus the task tells the user, and does not only report
        // success.
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
/// On Linux, a direct write over a binary that runs fails with "text file
/// busy".
/// A rename to the destination replaces the directory entry. This works when
/// the old binary is in use, and when it is not in use.
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

/// The workspace root. This function finds it from the manifest directory of
/// this crate.
fn workspace_root() -> Result<PathBuf, Error> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .ok_or_else(|| Error::Message("cannot locate the workspace root".to_string()))
}

/// The directory for the build output of cargo. It obeys `CARGO_TARGET_DIR`.
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

/// The root directory of `cargo install` when we give no `--root`. The order
/// is `CARGO_INSTALL_ROOT`, then `CARGO_HOME`, then `~/.cargo`.
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
// sync-fluency
// ---------------------------------------------------------------------------

/// The fluency prompt that the GenerateFluencyPrompt skill writes at the root.
const FLUENCY_PROMPT: &str = "FLUENCY_PROMPT.md";
/// The copy in the binary. `piton agent` uses it when there is no other
/// prompt.
const EMBEDDED_FLUENCY: &str = "plugins/belay/fluency.md";

/// Generates the fluency prompt again. Then it copies the prompt over the
/// copy in the compiler.
///
/// An agent writes the prompt, not code. The GenerateFluencyPrompt skill reads
/// the full specification and makes it shorter. Thus this task runs Claude
/// Code headless on that skill. Then it copies the result to the copy that
/// `piton agent` uses when there is no other prompt. If not, that copy
/// becomes old.
///
/// `--copy-only` does not generate the prompt. Use it for a prompt that a
/// person wrote, or for a prompt from an interactive run of the skill.
/// `--check` changes nothing. It fails when the two copies are different. CI
/// uses it.
fn sync_fluency(args: &[String]) -> Result<(), Error> {
    let mut check = false;
    let mut copy_only = false;
    let mut model: Option<String> = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--check" => check = true,
            "--copy-only" => copy_only = true,
            "--model" => {
                model = Some(
                    args.next()
                        .ok_or_else(|| Error::Usage("--model needs a model name".to_string()))?
                        .clone(),
                )
            }
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            other => return Err(Error::Usage(format!("unknown option `{other}` for sync-fluency"))),
        }
    }
    if check && (copy_only || model.is_some()) {
        return Err(Error::Usage("--check generates nothing, so it takes no other option".to_string()));
    }

    let workspace = workspace_root()?;
    if !check && !copy_only {
        generate_fluency(&workspace, model.as_deref())?;
    }
    let source = workspace.join(FLUENCY_PROMPT);
    let destination = workspace.join(EMBEDDED_FLUENCY);
    let prompt = fs::read_to_string(&source).map_err(|error| {
        Error::Message(format!(
            "cannot read {FLUENCY_PROMPT}: {error}; generate it with `cargo xtask sync-fluency`"
        ))
    })?;
    if prompt.trim().is_empty() {
        return Err(Error::Message(format!("{FLUENCY_PROMPT} is empty; regenerate it first")));
    }
    let current = fs::read_to_string(&destination).unwrap_or_default();

    if current == prompt {
        println!("{EMBEDDED_FLUENCY} is up to date with {FLUENCY_PROMPT}");
        return Ok(());
    }
    if check {
        return Err(Error::Message(format!(
            "{EMBEDDED_FLUENCY} differs from {FLUENCY_PROMPT}; run `cargo xtask sync-fluency`"
        )));
    }
    fs::write(&destination, &prompt)
        .map_err(|error| Error::Message(format!("cannot write {EMBEDDED_FLUENCY}: {error}")))?;
    println!(
        "copied {FLUENCY_PROMPT} to {EMBEDDED_FLUENCY} ({} lines); rebuild piton to embed it",
        prompt.lines().count()
    );
    Ok(())
}

/// The models that the --model option of Claude accepts. Each is the alias
/// of the latest model of a family. Claude has no command that lists them.
/// `piton agent` from Belay gives the same list.
const CLAUDE_MODELS: &[&str] = &["fable", "opus", "sonnet", "haiku"];

/// The request to Claude. The skill tells what to write. This request tells
/// where to write it. It also tells that no person can answer questions
/// during this run.
const GENERATE_REQUEST: &str = "\
Use the generate-fluency-prompt skill to regenerate the Piton fluency prompt from the current \
specification. Write the complete result to FLUENCY_PROMPT.md at the repository root, which is \
the working directory, replacing the file that is there. Change no other file. Nobody can answer \
questions during this run, so make every decision yourself.";

/// Runs Claude Code on the GenerateFluencyPrompt skill. The skill writes
/// FLUENCY_PROMPT.md again.
///
/// Claude can read the repository and edit files. Claude cannot run commands.
/// The task accepts edits without a question, because no person can answer.
/// The person who runs this task reads the diff. That person decides if the
/// prompt is correct.
fn generate_fluency(workspace: &Path, model: Option<&str>) -> Result<(), Error> {
    let path = workspace.join(FLUENCY_PROMPT);
    let before = fs::read_to_string(&path).ok();
    eprintln!("generating {FLUENCY_PROMPT} with Claude Code; reading the whole spec takes a few minutes");

    let mut command = Command::new("claude");
    command
        .current_dir(workspace)
        .args(["-p", GENERATE_REQUEST])
        .args(["--allowedTools", "Read,Grep,Glob,Write,Edit,Skill"])
        .args(["--disallowedTools", "Bash"])
        .args(["--permission-mode", "acceptEdits"])
        .stdin(std::process::Stdio::null());
    match model {
        Some(model) => {
            command.args(["--model", model]);
        }
        None => eprintln!(
            "Claude uses its default model; its models are {}; choose one with --model",
            CLAUDE_MODELS.join(", ")
        ),
    }
    let status = command.status().map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            Error::Message(
                "`claude` is not on PATH; install Claude Code, or write FLUENCY_PROMPT.md yourself and pass --copy-only"
                    .to_string(),
            )
        } else {
            Error::Message(format!("cannot run `claude`: {error}"))
        }
    })?;
    if !status.success() {
        return Err(Error::Message(format!(
            "Claude Code failed ({status}); {FLUENCY_PROMPT} was not synced"
        )));
    }

    let after = fs::read_to_string(&path).unwrap_or_default();
    if after.trim().is_empty() {
        return Err(Error::Message(format!(
            "Claude Code finished without writing {FLUENCY_PROMPT}; nothing was synced"
        )));
    }
    if before.as_deref() == Some(after.as_str()) {
        eprintln!("{FLUENCY_PROMPT} is unchanged; the spec may say nothing new");
    } else {
        eprintln!("regenerated {FLUENCY_PROMPT} ({} lines)", after.lines().count());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum Error {
    /// The command line was wrong. Print the usage with the error.
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
        // The variable applies to the full process. Thus this test asserts
        // only the default. The integration test examines the override path.
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
        // The function leaves no staging file.
        assert!(!dir.join(format!(".{}.new", file_name())).exists());
        let _ = fs::remove_dir_all(&dir);
    }
}

// ---------------------------------------------------------------------------
// publish-grammar
// ---------------------------------------------------------------------------

/// The location of the grammar in this repository.
const GRAMMAR_DIRECTORY: &str = "editors/tree-sitter-piton";
/// The location where the task publishes the grammar.
const GRAMMAR_REMOTE: &str = "git@github.com:piton-lang/tree-sitter-piton.git";

/// Copies the tree-sitter grammar into its own repository and pushes it.
///
/// We develop the grammar here, next to the compiler. Only here can a test
/// compare the grammar with the language. The keyword list of `piton-syntax`
/// is the source of truth. A test reads the grammar to make sure that the two
/// agree. Editors that use tree-sitter grammars expect one repository for each
/// grammar. Thus the task publishes a copy. We do not develop the grammar in
/// that repository.
///
/// The task keeps the published history. It clones the remote, replaces its
/// contents, and commits the result. Each publish is one commit. The commit
/// names its source revision. Thus a reader can align the two repositories.
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

    // A reader must be able to find the published source again. Thus the
    // working tree must agree with the last commit.
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

    // Clone the remote. Thus the published history continues and does not
    // start again. An empty remote has nothing to clone. This is not an
    // error.
    let cloned = clone(&options.remote, &options.branch, &checkout)?;
    if !cloned {
        println!("the remote has no `{}` branch yet; starting one", options.branch);
        git(&staging, &["init", "--quiet", "--initial-branch", &options.branch, "tree-sitter-piton"])?;
        git(&checkout, &["remote", "add", "origin", &options.remote])?;
    }

    clear(&checkout)?;
    copy_tree(&source, &checkout)?;

    let generated = generate_parser(&checkout, options.dry_run);
    if !generated && !options.allow_dirty {
        return Err(Error::Message(
            "the grammar has no `src/parser.c`, so Zed would report it as a \
             grammar that will not compile.\n\
             Install the tree-sitter CLI (`cargo install tree-sitter-cli`) and \
             publish again, or pass --allow-dirty to publish without it."
                .to_string(),
        ));
    }

    stage(&checkout)?;
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
        // The task keeps the directory. The purpose of a dry run is to let you
        // examine it.
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

/// Runs git in a directory. This function fails if git fails.
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

/// Runs git and returns its printed output.
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

/// Checks that git knows the identity of the committer.
///
/// The git error for this problem is many paragraphs long. It also comes
/// after the work ends. That is a bad time to learn about the problem.
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

/// Makes a shallow clone of a branch of a remote. False when the branch does
/// not exist.
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
    // A remote can exist with no such branch, or with no commits. This is the
    // first publish. It is not a failure.
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

/// Removes all contents of a checkout except its `.git` directory.
///
/// When this function deletes a file, the published repository also loses
/// it. Thus the publish is a copy. It does not collect old files.
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
/// A published grammar must have `src/parser.c`. Zed compiles only that file.
/// Thus Zed reports a grammar without it as a grammar that does not compile.
/// Returns true when the parser is available to publish.
fn generate_parser(checkout: &Path, dry_run: bool) -> bool {
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
        return false;
    }
    if dry_run {
        println!("would run `tree-sitter generate`");
        return checkout.join("src/parser.c").is_file();
    }
    let status = Command::new("tree-sitter")
        .arg("generate")
        .current_dir(checkout)
        .status();
    match status {
        Ok(status) if status.success() => println!("generated the parser"),
        _ => println!("note: `tree-sitter generate` failed; publishing the grammar alone"),
    }
    checkout.join("src/parser.c").is_file()
}

/// All the files from `tree-sitter generate` that a consumer needs.
///
/// The `.gitignore` of the grammar excludes these files. In this repository,
/// they are build output that the build makes again from `grammar.js`. The
/// published copy is a distribution and not a source tree. Thus the task
/// stages these files by name. `git add --all` does not add them.
const GENERATED: [&str; 2] = ["src", "bindings"];

/// Stages the published tree, with the generated parser.
fn stage(checkout: &Path) -> Result<(), Error> {
    git(checkout, &["add", "--all"])?;
    for path in GENERATED {
        if checkout.join(path).exists() {
            git(checkout, &["add", "--force", "--", path])?;
        }
    }
    Ok(())
}

/// A temporary directory for the work.
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

/// Removes the staging directory when the task ends, in all cases.
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
        // Each publish replaces all other contents of the checkout. If the
        // task removes `.git`, the published history goes away. Then each
        // publish makes a new repository.
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
    fn staging_includes_the_generated_parser() {
        // The `.gitignore` of the grammar excludes `src/`. Thus a plain `git
        // add --all` publishes a repository with no `src/parser.c`. Zed
        // compiles only that file. When the file is not there, Zed reports a
        // grammar that does not compile.
        let checkout = scratch("stage");
        git(&checkout, &["init", "--quiet", "--initial-branch", "main"]).expect("init");
        fs::write(&checkout.join(".gitignore"), "src/\nnode_modules/\n").unwrap();
        fs::write(&checkout.join("grammar.js"), "module.exports = {};").unwrap();
        fs::create_dir_all(checkout.join("src/tree_sitter")).unwrap();
        fs::write(checkout.join("src/parser.c"), "/* generated */").unwrap();
        fs::write(checkout.join("src/tree_sitter/parser.h"), "/* generated */").unwrap();
        fs::create_dir_all(checkout.join("node_modules/tree-sitter-cli")).unwrap();
        fs::write(checkout.join("node_modules/tree-sitter-cli/index.js"), "").unwrap();

        stage(&checkout).expect("stage");

        let staged = git_output(&checkout, &["diff", "--cached", "--name-only"]).expect("staged");
        let staged: Vec<&str> = staged.lines().collect();
        assert!(staged.contains(&"src/parser.c"), "{staged:?}");
        assert!(staged.contains(&"src/tree_sitter/parser.h"), "{staged:?}");
        assert!(staged.contains(&"grammar.js"), "{staged:?}");
        // When the task forces the generated parser in, it must not also add
        // the toolchain.
        assert!(
            !staged.iter().any(|path| path.starts_with("node_modules/")),
            "{staged:?}"
        );
    }

    #[test]
    fn a_publish_defaults_to_the_grammars_own_repository() {
        let options = PublishOptions::parse(&[]).expect("defaults");
        assert_eq!(options.remote, GRAMMAR_REMOTE);
        assert_eq!(options.branch, "main");
        // The purpose of the task is to publish. Thus the default is not a dry
        // run. But the task refuses to publish work that has no commit.
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
        // The task copies a path from a constant. If someone moves the
        // grammar, the task publishes an empty repository. This test prevents
        // that.
        let source = workspace_root().expect("workspace").join(GRAMMAR_DIRECTORY);
        assert!(source.join("grammar.js").is_file(), "{}", source.display());
        assert!(source.join("queries").is_dir(), "{}", source.display());
    }
}

/// The file where the Zed extension pins the grammar that it fetches.
const ZED_EXTENSION: &str = "editors/zed/extension.toml";

/// Other editors that fetch the same grammar at the same commit as Zed.
const OTHER_GRAMMAR_PINS: [&str; 2] = ["editors/helix/languages.toml", "editors/neovim/piton.lua"];

/// Changes the pin of the Zed extension to the commit that the task
/// published.
///
/// Zed fetches one revision of the grammar repository and builds it. An old
/// pin keeps each Zed user on an older grammar than the new one. A pin that
/// is not a real commit fetches nothing. Zed then reports a grammar that does
/// not compile. The publish makes the new commit. Thus the publish is also
/// the time to record it.
fn pin_zed_grammar(workspace: &Path, commit: &str) -> Result<(), Error> {
    let path = workspace.join(ZED_EXTENSION);
    let Ok(text) = fs::read_to_string(&path) else {
        // The extension is optional. The publish of the grammar does not need
        // it.
        return Ok(());
    };

    let mut out = String::with_capacity(text.len());
    let mut in_grammar = false;
    let mut pinned = false;
    let mut previous: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_grammar = trimmed == "[grammars.piton]";
        }
        if in_grammar && trimmed.starts_with("commit") && !pinned {
            previous = trimmed.split('"').nth(1).map(str::to_string);
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

    // Helix and Neovim fetch the same grammar at the same commit as Zed. Thus
    // their pins change with the pin of Zed.
    if let Some(previous) = previous.filter(|p| !p.is_empty() && p != commit) {
        for relative in OTHER_GRAMMAR_PINS {
            let path = workspace.join(relative);
            let Ok(text) = fs::read_to_string(&path) else { continue };
            if !text.contains(&previous) {
                println!("note: {relative} does not pin {previous}; left alone");
                continue;
            }
            fs::write(&path, text.replace(&previous, commit)).map_err(|error| {
                Error::Message(format!("cannot write `{}`: {error}", path.display()))
            })?;
            println!("pinned {relative} to {}", &commit[..commit.len().min(12)]);
        }
    }
    println!("note: that is a change to this repository; commit it so editor users get it");
    Ok(())
}
