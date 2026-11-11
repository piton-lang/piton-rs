//! Finds the version that `piton --version` shows.

use std::path::PathBuf;

fn main() {
    version();
}

/// Sets `PITON_BUILD_VERSION` to the version this build reports.
///
/// The edge workflow in `.github/workflows/edge.yml` publishes the commit
/// with the same version. The major and minor numbers come from `Cargo.toml`.
/// The patch is the number of commits up to `HEAD`. Thus a local build of a
/// commit shows the same version as the release of that commit. A release
/// build sets `PITON_VERSION` to that version, and this script uses it as
/// given. Without git, or outside a checkout, the crate version stays.
fn version() {
    println!("cargo:rerun-if-env-changed=PITON_VERSION");
    let crate_version = std::env::var("CARGO_PKG_VERSION").expect("crate version");
    let version = match std::env::var("PITON_VERSION") {
        Ok(version) if !version.trim().is_empty() => version.trim().to_string(),
        _ => match git(&["rev-list", "--count", "HEAD"]) {
            Some(count) => {
                let major_minor = crate_version
                    .rsplit_once('.')
                    .map(|(major_minor, _)| major_minor)
                    .unwrap_or(&crate_version);
                // A new commit changes the count. Thus the script calculates
                // the version again when HEAD, or the branch of HEAD, moves.
                if let Some(git_dir) = git(&["rev-parse", "--absolute-git-dir"]) {
                    let git_dir = PathBuf::from(git_dir);
                    println!("cargo:rerun-if-changed={}", git_dir.join("HEAD").display());
                    println!("cargo:rerun-if-changed={}", git_dir.join("packed-refs").display());
                    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
                        println!("cargo:rerun-if-changed={}", git_dir.join(branch).display());
                    }
                }
                format!("{major_minor}.{count}")
            }
            None => crate_version,
        },
    };
    println!("cargo:rustc-env=PITON_BUILD_VERSION={version}");
}

/// Runs git in the directory of this crate. When git succeeds, it returns the
/// trimmed output.
fn git(args: &[&str]) -> Option<String> {
    let directory = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(directory)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}
