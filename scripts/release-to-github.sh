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

PKG_PATH="${PKG_PATH:-$ROOT_DIR/dist/SensibleFox.pkg}"
BIN_PATH="${BIN_PATH:-$ROOT_DIR/target/release/sensiblefox}"

[[ -f "$PKG_PATH" ]] || die "missing pkg: $PKG_PATH (run build-pkg.sh first)"
[[ -f "$BIN_PATH" ]] || die "missing binary: $BIN_PATH (run: cargo build --release)"
command -v gh >/dev/null 2>&1 || die "gh not found — install GitHub CLI and run: gh auth login"

if gh release view "$TAG" --json tagName >/dev/null 2>&1; then
    gh release upload "$TAG" "$PKG_PATH" "$BIN_PATH" --clobber
    printf '%s\n' "Uploaded assets to existing release $TAG"
else
    gh release create "$TAG" "$PKG_PATH" "$BIN_PATH" \
        --title "$TAG" \
        --generate-notes
    printf '%s\n' "Created release $TAG with pkg + binary"
fi
