#!/usr/bin/env bash
set -euo pipefail

# Upload all SensibleFox dist/ artifacts to a GitHub release.
# Requires: gh (authenticated) + a completed build.
#
# Usage: ./scripts/release-to-github.sh <version>
#   version: e.g. 1.2.3 or v1.2.3 (release tag will be v1.2.3)

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
DIST="$ROOT_DIR/dist"

die() {
    printf '%s\n' "$*" >&2
    exit 1
}

VERSION_RAW="${1:-}"
[[ -n "$VERSION_RAW" ]] || die "usage: $0 <version>   (example: $0 1.2.3)"

VER="${VERSION_RAW#v}"
TAG="v${VER}"

[[ -f "$DIST/SensibleFox-universal.pkg" ]] || die "missing: dist/SensibleFox-universal.pkg (run build.sh first)"
[[ -f "$DIST/sensiblefox-universal" ]]     || die "missing: dist/sensiblefox-universal (run build.sh first)"

ASSETS=()

for f in \
    "$DIST/SensibleFox-universal.pkg" \
    "$DIST/SensibleFox-aarch64.pkg" \
    "$DIST/SensibleFox-x86_64.pkg" \
    "$DIST/SensibleFox-Offline.pkg" \
    "$DIST/SensibleFox.dmg" \
    "$DIST/sensiblefox-universal"
do
    [[ -f "$f" ]] && ASSETS+=("$f")
done

BIN_AARCH64="$ROOT_DIR/target/aarch64-apple-darwin/release/sensiblefox"
BIN_X86_64="$ROOT_DIR/target/x86_64-apple-darwin/release/sensiblefox"

if [[ -f "$BIN_AARCH64" ]]; then
    cp "$BIN_AARCH64" "$DIST/sensiblefox-aarch64"
    ASSETS+=("$DIST/sensiblefox-aarch64")
fi
if [[ -f "$BIN_X86_64" ]]; then
    cp "$BIN_X86_64" "$DIST/sensiblefox-x86_64"
    ASSETS+=("$DIST/sensiblefox-x86_64")
fi

printf 'Release %s — %d assets:\n' "$TAG" "${#ASSETS[@]}"
for f in "${ASSETS[@]}"; do
    size=$(du -h "$f" | cut -f1 | tr -d ' ')
    printf '  %s (%s)\n' "$(basename "$f")" "$size"
done

command -v gh >/dev/null 2>&1 || die "gh not found — install GitHub CLI and run: gh auth login"

if gh release view "$TAG" --json tagName >/dev/null 2>&1; then
    gh release upload "$TAG" "${ASSETS[@]}" --clobber
    printf '%s\n' "Uploaded assets to existing release $TAG"
else
    gh release create "$TAG" "${ASSETS[@]}" \
        --title "$TAG" \
        --generate-notes
    printf '%s\n' "Created release $TAG with ${#ASSETS[@]} assets"
fi
