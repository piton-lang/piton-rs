//! `piton --update` — replace this binary with the latest release.
//!
//! Uses the release source `install.sh` uses: the GitHub releases of
//! piton-lang/piton-rs, or `PITON_DOWNLOAD_BASE`. The latest version comes
//! from the redirect of `/releases/latest`, which needs no API and has no
//! rate limit. Downloads go through curl or wget, and archives are unpacked
//! with tar or unzip, as the install script does, so the binary carries no
//! HTTP or TLS stack of its own. The download is checked against its
//! `.sha256` file before anything is replaced.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::style::{self, paint};
use crate::{report, EXIT_ERRORS, EXIT_SUCCESS, VERSION};

const REPO: &str = "piton-lang/piton-rs";

pub fn run() -> u8 {
    match update() {
        Ok(()) => EXIT_SUCCESS,
        Err(message) => {
            report::stopped("piton was not updated");
            report::fail(message);
            EXIT_ERRORS
        }
    }
}

fn update() -> Result<(), String> {
    let base = std::env::var("PITON_DOWNLOAD_BASE")
        .ok()
        .filter(|base| !base.is_empty())
        .unwrap_or_else(|| format!("https://github.com/{REPO}/releases"));
    let (platform, arch, extension, binary) = platform()?;

    let latest = latest_version(&base)?;
    if latest == VERSION {
        report::done(format!(
            "piton {} is already the latest release",
            paint(style::NAME, VERSION)
        ));
        return Ok(());
    }
    if newer(VERSION, &latest) {
        report::done(format!(
            "piton {} is newer than the latest release, {}; left as it is",
            paint(style::NAME, VERSION),
            paint(style::NAME, &latest)
        ));
        return Ok(());
    }

    let current = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|error| format!("couldn't find the running piton: {error}"))?;

    let archive = format!("piton-edge-{platform}-{arch}-{latest}.{extension}");
    let url = format!("{base}/download/v{latest}/{archive}");

    let work = Work::new()?;
    eprintln!(
        "{}",
        paint(style::DIM, format!("downloading piton {latest} for {platform} {arch}"))
    );
    let archive_path = work.path.join(&archive);
    let checksum_path = work.path.join(format!("{archive}.sha256"));
    download(&url, &archive_path).map_err(|error| format!("couldn't download {url}: {error}"))?;
    download(&format!("{url}.sha256"), &checksum_path)
        .map_err(|error| format!("couldn't download the checksum for {archive}: {error}"))?;

    let expected = std::fs::read_to_string(&checksum_path)
        .map_err(|error| format!("couldn't read the checksum for {archive}: {error}"))?;
    let expected = expected.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
    let bytes = std::fs::read(&archive_path)
        .map_err(|error| format!("couldn't read {archive}: {error}"))?;
    if sha256_hex(&bytes) != expected {
        return Err("the download doesn't match its checksum; try again".to_string());
    }

    let unpacked = work.path.join("unpacked");
    std::fs::create_dir_all(&unpacked).map_err(|error| error.to_string())?;
    unpack(&archive_path, &unpacked, extension)?;
    let fresh = unpacked.join(binary);
    if !fresh.is_file() {
        return Err(format!("{archive} holds no {binary}"));
    }

    replace(&fresh, &current)?;

    report::done(format!(
        "updated piton from {} to {} at {}",
        paint(style::NAME, VERSION),
        paint(style::NAME, &latest),
        current.display()
    ));
    Ok(())
}

/// The build for this machine, named as the release archives name it.
fn platform() -> Result<(&'static str, &'static str, &'static str, &'static str), String> {
    let platform = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "macos",
        "windows" => "windows",
        other => return Err(format!("there's no build for {other} yet")),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => return Err(format!("there's no build for {other} yet")),
    };
    match (platform, arch) {
        ("linux", "x86_64") => Ok((platform, arch, "tar.gz", "piton")),
        ("macos", _) => Ok((platform, arch, "zip", "piton")),
        ("windows", "x86_64") => Ok((platform, arch, "zip", "piton.exe")),
        _ => Err(format!("there's no build for {platform} on {arch} yet")),
    }
}

/// GitHub redirects `/releases/latest` to `/releases/tag/vX.Y.Z`.
fn latest_version(base: &str) -> Result<String, String> {
    let latest = format!("{base}/latest");
    let url = if on_path("curl") {
        let output = Command::new("curl")
            .args(["-fsSLI", "-o", null_device(), "-w", "%{url_effective}", &latest])
            .stdin(Stdio::null())
            .output()
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err(format!("couldn't reach {latest}"));
        }
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else if on_path("wget") {
        let output = Command::new("wget")
            .args(["-q", "--max-redirect=5", "-S", "-O", null_device(), &latest])
            .stdin(Stdio::null())
            .output()
            .map_err(|error| error.to_string())?;
        String::from_utf8_lossy(&output.stderr)
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                line.strip_prefix("Location:")
                    .or_else(|| line.strip_prefix("location:"))
                    .map(|value| value.trim().to_string())
            })
            .last()
            .unwrap_or_default()
    } else {
        return Err("updating needs curl or wget".to_string());
    };
    let version = url.rsplit_once("/v").map(|(_, version)| version).unwrap_or("");
    if version.starts_with(|c: char| c.is_ascii_digit()) {
        Ok(version.to_string())
    } else {
        Err(format!("couldn't find the latest release at {latest}"))
    }
}

fn download(url: &str, to: &Path) -> Result<(), String> {
    let mut command = if on_path("curl") {
        let mut command = Command::new("curl");
        command.args(["-fsSL", url, "-o"]).arg(to);
        command
    } else if on_path("wget") {
        let mut command = Command::new("wget");
        command.args(["-q", url, "-O"]).arg(to);
        command
    } else {
        return Err("updating needs curl or wget".to_string());
    };
    let status = command
        .stdin(Stdio::null())
        .status()
        .map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("the download exited with {status}"))
    }
}

fn unpack(archive: &Path, into: &Path, extension: &str) -> Result<(), String> {
    let mut command = if extension == "tar.gz" {
        let mut command = Command::new("tar");
        command.arg("-xzf").arg(archive).arg("-C").arg(into);
        command
    } else if on_path("unzip") {
        let mut command = Command::new("unzip");
        command.arg("-q").arg(archive).arg("-d").arg(into);
        command
    } else {
        // bsdtar, on macOS and Windows 10 and later, reads zip archives too.
        let mut command = Command::new("tar");
        command.arg("-xf").arg(archive).arg("-C").arg(into);
        command
    };
    let status = command
        .stdin(Stdio::null())
        .status()
        .map_err(|error| format!("couldn't unpack {}: {error}", archive.display()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("couldn't unpack {}", archive.display()))
    }
}

/// Put `fresh` where `current` is. The new binary is copied beside the old
/// one and renamed over it, so a running piton is never half written. Windows
/// won't replace a running executable, but it will rename one, so there the
/// old binary is moved aside first.
fn replace(fresh: &Path, current: &Path) -> Result<(), String> {
    let staged = sibling(current, ".new");
    std::fs::copy(fresh, &staged)
        .map_err(|error| format!("couldn't write {}: {error}", staged.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
    }
    if cfg!(windows) {
        let old = sibling(current, ".old");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(current, &old)
            .map_err(|error| format!("couldn't move {} aside: {error}", current.display()))?;
    }
    if let Err(error) = std::fs::rename(&staged, current) {
        let _ = std::fs::remove_file(&staged);
        return Err(format!("couldn't replace {}: {error}", current.display()));
    }
    if cfg!(target_os = "macos") {
        // The macOS builds aren't signed yet; drop the quarantine flag so
        // Gatekeeper doesn't refuse to run the new binary.
        let _ = Command::new("xattr")
            .args(["-d", "com.apple.quarantine"])
            .arg(current)
            .stderr(Stdio::null())
            .status();
    }
    Ok(())
}

/// Whether version `a` is later than `b`, comparing dotted numbers. A part
/// that isn't a number compares as 0.
fn newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    parts(a) > parts(b)
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

fn on_path(program: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| {
        dir.join(program).is_file() || dir.join(format!("{program}.exe")).is_file()
    })
}

fn null_device() -> &'static str {
    if cfg!(windows) { "NUL" } else { "/dev/null" }
}

/// A temporary directory removed when dropped.
struct Work {
    path: PathBuf,
}

impl Work {
    fn new() -> Result<Self, String> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|time| time.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!("piton-update-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path)
            .map_err(|error| format!("couldn't make a temporary directory: {error}"))?;
        Ok(Self { path })
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// SHA-256 of `data`, as lowercase hex.
fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let mut message = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bits.to_be_bytes());
    for block in message.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(value);
        }
    }
    h.iter().map(|word| format!("{word:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::sha256_hex;

    #[test]
    fn sha256_matches_known_digests() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // Two blocks of padding.
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }
}
