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

# Retrieve auth token for curl uploads
token=$(gh auth token 2>/dev/null || true)
[[ -n "$token" ]] || die "gh CLI not authenticated — please run: gh auth login"

# Ensure the release exists (create it without files if it doesn't)
if ! gh release view "$TAG" --json tagName >/dev/null 2>&1; then
    printf 'Creating release %s...\n' "$TAG"
    gh release create "$TAG" --title "$TAG" --generate-notes
fi

# Append the unsigned-install guide so every release explains Gatekeeper.
# Single source: .github/release-notes-footer.md (also used by CI).
FOOTER="$ROOT_DIR/.github/release-notes-footer.md"
if [[ -f "$FOOTER" ]]; then
    body=$(gh release view "$TAG" --json body --jq '.body')
    if [[ "$body" != *"Installing unsigned builds"* ]]; then
        printf '%s\n\n%s\n' "$body" "$(cat "$FOOTER")" > "$DIST/.release-notes.md"
        gh release edit "$TAG" --notes-file "$DIST/.release-notes.md"
        rm -f "$DIST/.release-notes.md"
    fi
fi

# Retrieve release upload URL template
upload_url_template=$(gh release view "$TAG" --json uploadUrl --jq '.uploadUrl' 2>/dev/null || true)
[[ -n "$upload_url_template" ]] || die "Could not retrieve upload URL for release $TAG"

# Strip trailing {?name,label} template
upload_url="${upload_url_template%\{*\}}"

printf 'Uploading %d assets to release %s...\n\n' "${#ASSETS[@]}" "$TAG"

for asset in "${ASSETS[@]}"; do
    asset_name="$(basename "$asset")"
    size=$(du -h "$asset" | cut -f1 | tr -d ' ')
    
    printf 'Uploading %s (%s)...\n' "$asset_name" "$size"
    
    # Delete existing asset if it exists (clobber)
    gh release delete-asset "$TAG" "$asset_name" -y >/dev/null 2>&1 || true
    
    # Stream the upload with a progress bar
    if ! curl -f -L \
        -X POST \
        -H "Authorization: Bearer $token" \
        -H "Content-Type: application/octet-stream" \
        --progress-bar \
        -T "$asset" \
        "${upload_url}?name=${asset_name}" \
        -o /dev/null; then
        die "Failed to upload $asset_name"
    fi
    
    printf 'Successfully uploaded %s\n\n' "$asset_name"
done

printf 'Created/updated release %s with %d assets!\n' "$TAG" "${#ASSETS[@]}"

