#!/bin/sh
# Installs piton.
#
#   curl -fsSL https://github.com/piton-lang/piton-rs/releases/latest/download/install.sh | sh
#
# Works on macOS, Linux, and Windows under Git Bash or MSYS. On Windows in
# PowerShell, use install.ps1 instead.
#
# Settings, all optional:
#   PITON_VERSION      a version to install, like 0.1.41 (default: the latest)
#   PITON_INSTALL_DIR  where to put piton (default: ~/.local/bin)

set -eu

REPO="piton-lang/piton-rs"
BASE="${PITON_DOWNLOAD_BASE:-https://github.com/${REPO}/releases}"
INSTALL_DIR="${PITON_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf 'piton: %s\n' "$1"; }
fail() { printf 'piton: %s\n' "$1" >&2; exit 1; }

need() {
    command -v "$1" >/dev/null 2>&1 || fail "this needs \`$1\`, which isn't installed"
}

download() {
    # download <url> <file>
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$1" -o "$2"
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$1" -O "$2"
    else
        fail "this needs curl or wget"
    fi
}

latest_version() {
    # GitHub redirects /releases/latest to /releases/tag/vX.Y.Z. Following the
    # redirect avoids the API and its rate limit.
    if command -v curl >/dev/null 2>&1; then
        url=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "${BASE}/latest")
    else
        url=$(wget -q --max-redirect=5 -S -O /dev/null "${BASE}/latest" 2>&1 \
            | sed -n 's/^ *[Ll]ocation: *//p' | tail -n 1)
    fi
    version=${url##*/v}
    case "$version" in
        [0-9]*) printf '%s' "$version" ;;
        *) fail "couldn't find the latest release at ${BASE}/latest" ;;
    esac
}

# -- Which build ------------------------------------------------------------

os=$(uname -s)
arch=$(uname -m)

case "$os" in
    Linux) platform="linux" ;;
    Darwin) platform="macos" ;;
    MINGW* | MSYS* | CYGWIN*) platform="windows" ;;
    *) fail "there's no build for $os yet" ;;
esac

case "$arch" in
    x86_64 | amd64) arch="x86_64" ;;
    arm64 | aarch64) arch="aarch64" ;;
    *) fail "there's no build for $arch yet" ;;
esac

# A terminal running under Rosetta reports x86_64 on an Apple Silicon Mac;
# the native build is the better one there.
if [ "$platform" = "macos" ] && [ "$arch" = "x86_64" ] \
    && [ "$(sysctl -n hw.optional.arm64 2>/dev/null || echo 0)" = "1" ]; then
    arch="aarch64"
fi

case "$platform-$arch" in
    linux-x86_64 | macos-aarch64 | macos-x86_64 | windows-x86_64) ;;
    *) fail "there's no build for $platform on $arch yet" ;;
esac

if [ "$platform" = "linux" ]; then
    extension="tar.gz"
    binary="piton"
else
    extension="zip"
    binary="piton"
    [ "$platform" = "windows" ] && binary="piton.exe"
fi

version="${PITON_VERSION:-}"
version="${version#v}"
[ -n "$version" ] || version=$(latest_version)

archive="piton-edge-${platform}-${arch}-${version}.${extension}"
url="${BASE}/download/v${version}/${archive}"

# -- Download and check -----------------------------------------------------

work=$(mktemp -d 2>/dev/null || mktemp -d -t piton)
trap 'rm -rf "$work"' EXIT INT TERM

say "downloading piton ${version} for ${platform} ${arch}"
download "$url" "$work/$archive" || fail "couldn't download $url"
download "$url.sha256" "$work/$archive.sha256" || fail "couldn't download the checksum for $archive"

expected=$(cut -d ' ' -f 1 < "$work/$archive.sha256")
if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$work/$archive" | cut -d ' ' -f 1)
elif command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "$work/$archive" | cut -d ' ' -f 1)
else
    fail "this needs sha256sum or shasum to check the download"
fi
[ "$expected" = "$actual" ] || fail "the download doesn't match its checksum; try again"

# -- Unpack and install -----------------------------------------------------

mkdir -p "$work/unpacked"
if [ "$extension" = "tar.gz" ]; then
    need tar
    tar -xzf "$work/$archive" -C "$work/unpacked"
else
    need unzip
    unzip -q "$work/$archive" -d "$work/unpacked"
fi

mkdir -p "$INSTALL_DIR"
# Replace rather than overwrite in place, so a running piton isn't disturbed.
cp "$work/unpacked/$binary" "$INSTALL_DIR/$binary.new"
chmod +x "$INSTALL_DIR/$binary.new"
mv -f "$INSTALL_DIR/$binary.new" "$INSTALL_DIR/$binary"

# The macOS builds aren't signed yet. Drop the quarantine flag so Gatekeeper
# doesn't refuse to run it.
if [ "$platform" = "macos" ]; then
    xattr -d com.apple.quarantine "$INSTALL_DIR/$binary" 2>/dev/null || true
fi

say "installed $("$INSTALL_DIR/$binary" --version 2>/dev/null || echo "piton $version") to $INSTALL_DIR/$binary"

case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        say "$INSTALL_DIR isn't on your PATH. Add this to your shell's profile:"
        printf '\n    export PATH="%s:$PATH"\n\n' "$INSTALL_DIR"
        ;;
esac
