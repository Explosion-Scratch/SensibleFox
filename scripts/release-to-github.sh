#!/usr/bin/env bash
set -euo pipefail

# Upload dist/SensibleFox.pkg and the release binary to a GitHub release for a
# given semantic version. Requires: gh (authenticated), built .pkg, and
# cargo build --release.
#
# Usage: ./scripts/release-to-github.sh <version>
#   version: e.g. 1.2.3 or v1.2.3 (release tag will be v1.2.3)
#
# Optional env:
#   PKG_PATH    default: ROOT/dist/SensibleFox.pkg
#   BIN_PATH    default: ROOT/target/release/sensiblefox

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

die() {
    printf '%s\n' "$*" >&2
    exit 1
}

VERSION_RAW="${1:-}"
[[ -n "$VERSION_RAW" ]] || die "usage: $0 <version>   (example: $0 1.2.3)"

VER="${VERSION_RAW#v}"
TAG="v${VER}"

PKG_UNIVERSAL="$ROOT_DIR/dist/SensibleFox-universal.pkg"
PKG_AARCH64="$ROOT_DIR/dist/SensibleFox-aarch64.pkg"
PKG_X86_64="$ROOT_DIR/dist/SensibleFox-x86_64.pkg"

BIN_UNIVERSAL="$ROOT_DIR/dist/sensiblefox-universal"
BIN_AARCH64="$ROOT_DIR/target/aarch64-apple-darwin/release/sensiblefox"
BIN_X86_64="$ROOT_DIR/target/x86_64-apple-darwin/release/sensiblefox"

[[ -f "$PKG_UNIVERSAL" ]] || die "missing pkg: $PKG_UNIVERSAL (run build-pkg.sh first)"
[[ -f "$BIN_UNIVERSAL" ]] || die "missing binary: $BIN_UNIVERSAL (run build-pkg.sh first)"

ASSETS=("$PKG_UNIVERSAL" "$BIN_UNIVERSAL")

if [[ -f "$PKG_AARCH64" ]]; then ASSETS+=("$PKG_AARCH64"); fi
if [[ -f "$PKG_X86_64" ]]; then ASSETS+=("$PKG_X86_64"); fi
if [[ -f "$BIN_AARCH64" ]]; then
    cp "$BIN_AARCH64" "$ROOT_DIR/dist/sensiblefox-aarch64"
    ASSETS+=("$ROOT_DIR/dist/sensiblefox-aarch64")
fi
if [[ -f "$BIN_X86_64" ]]; then
    cp "$BIN_X86_64" "$ROOT_DIR/dist/sensiblefox-x86_64"
    ASSETS+=("$ROOT_DIR/dist/sensiblefox-x86_64")
fi

command -v gh >/dev/null 2>&1 || die "gh not found — install GitHub CLI and run: gh auth login"

if gh release view "$TAG" --json tagName >/dev/null 2>&1; then
    gh release upload "$TAG" "${ASSETS[@]}" --clobber
    printf '%s\n' "Uploaded assets to existing release $TAG"
else
    gh release create "$TAG" "${ASSETS[@]}" \
        --title "$TAG" \
        --generate-notes
    printf '%s\n' "Created release $TAG with pkgs + binaries"
fi
