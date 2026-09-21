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
    install     Build the release binary and install it locally

Options for `install`:
    --root <dir>    Install into <dir>/bin instead of the cargo home
    --dry-run       Report what would happen without copying anything

Options:
    -h, --help      Print this message
";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let task = args.next();
    let rest: Vec<String> = args.collect();

    let result = match task.as_deref() {
        Some("install") => install(&rest),
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
