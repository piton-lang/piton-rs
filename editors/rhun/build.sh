#!/bin/sh
# Builds the rhun extension for Piton into bin/.
#
# The program is rhun-editor/lsp at the commit below, with lsp.patch. The patch
# makes it serve Piton only and start `piton lsp`. The build needs git and Go.
# usage: ./build.sh
set -e
cd "$(dirname "$0")"
REPOSITORY=https://github.com/rhun-editor/lsp
COMMIT=587afa7597b30e9371448e66170a48a0c6efd81e
case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) target=linux-x86_64; exe= ;;
    Darwin-arm64) target=macos-arm64; exe= ;;
    MINGW*-x86_64|MSYS*-x86_64) target=windows-x64; exe=.exe ;;
    *) echo "rhun has no build for $(uname -s)-$(uname -m)"; exit 1 ;;
esac
here=$(pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
git clone --quiet "$REPOSITORY" "$work/lsp"
git -C "$work/lsp" checkout --quiet "$COMMIT"
git -C "$work/lsp" apply "$here/lsp.patch"
mkdir -p bin
(cd "$work/lsp" && CGO_ENABLED=0 go build -o "$here/bin/piton-lsp-$target$exe" .)
echo "built bin/piton-lsp-$target$exe"
