#!/usr/bin/env bash
set -euo pipefail

# SensibleFox — Master Build Script
#
# Builds everything in the correct order:
#   1. Generate prefs (user.js + sensiblefox-defaults.js)
#   2. Compile Rust CLI (universal + per-arch binaries)
#   3. Build .pkg installers (online: universal + aarch64 + x86_64, offline)
#   4. Build patched Firefox .dmg
#
# Produces:
#   dist/SensibleFox-universal.pkg     online installer (universal binary)
#   dist/SensibleFox-aarch64.pkg       online installer (ARM only)
#   dist/SensibleFox-x86_64.pkg        online installer (Intel only)
#   dist/SensibleFox-Offline.pkg       offline installer (Firefox bundled)
#   dist/SensibleFox.dmg               standalone patched Firefox app
#   dist/sensiblefox-universal          CLI binary (universal)
#   dist/sensiblefox-aarch64            CLI binary (ARM)
#   dist/sensiblefox-x86_64             CLI binary (Intel)
#
# Env knobs (forwarded to sub-scripts):
#   FIREFOX_LANG                       default: en-US
#   PKG_VERSION                        default: 1.0.0
#   PKG_IDENTIFIER                     default: com.sensiblefox.firefox
#   DEVELOPER_ID_APPLICATION           code signing identity (app)
#   DEVELOPER_ID_INSTALLER             code signing identity (pkg)
#   NOTARYTOOL_PROFILE                 notarization keychain profile
#   SENSIBLEFOX_NATIVE_ONLY=1          skip cross-arch (not for distribution)
#   SKIP_PKG=1                         skip .pkg builds
#   SKIP_DMG=1                         skip .dmg build
#   SKIP_OFFLINE=1                     skip offline .pkg (saves ~200MB download)

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

echo "╔══════════════════════════════════════════════════════════════════╗"
echo "║  SensibleFox — Full Build                                      ║"
echo "╚══════════════════════════════════════════════════════════════════╝"
echo ""

# ── Step 1: Generate prefs ────────────────────────────────────────────────
echo "┌─ Step 1/4: Generate prefs ──────────────────────────────────────┐"
"$SCRIPT_DIR/generate-prefs.sh"
echo "└────────────────────────────────────────────────────────────────────┘"
echo ""

# ── Step 2: Compile Rust CLI ──────────────────────────────────────────────
echo "┌─ Step 2/4: Compile CLI ─────────────────────────────────────────┐"
echo "  → Building sensiblefox CLI (release)..."
mkdir -p "$ROOT_DIR/dist"

TARGET_AARCH64="aarch64-apple-darwin"
TARGET_X86_64="x86_64-apple-darwin"

if [ "${SENSIBLEFOX_NATIVE_ONLY:-0}" = "1" ]; then
    echo "    (SENSIBLEFOX_NATIVE_ONLY=1 — single-arch host build)"
    cargo build --release --quiet --manifest-path "$ROOT_DIR/Cargo.toml"
else
    rustup_has_target() {
        rustup target list --installed 2>/dev/null | /usr/bin/grep -qx "$1"
    }
    need=()
    rustup_has_target "$TARGET_AARCH64" || need+=("$TARGET_AARCH64")
    rustup_has_target "$TARGET_X86_64" || need+=("$TARGET_X86_64")
    if [ "${#need[@]}" -gt 0 ]; then
        echo "  ✗ Missing Rust std targets: ${need[*]}"
        echo "    Install with: rustup target add ${need[*]}"
        exit 1
    fi

    echo "    targets: $TARGET_AARCH64 + $TARGET_X86_64 → universal (lipo)"
    cargo build --release --target "$TARGET_AARCH64" --quiet --manifest-path "$ROOT_DIR/Cargo.toml"
    cargo build --release --target "$TARGET_X86_64" --quiet --manifest-path "$ROOT_DIR/Cargo.toml"
    /usr/bin/lipo -create \
        "$ROOT_DIR/target/$TARGET_AARCH64/release/sensiblefox" \
        "$ROOT_DIR/target/$TARGET_X86_64/release/sensiblefox" \
        -output "$ROOT_DIR/dist/sensiblefox-universal"
    chmod 755 "$ROOT_DIR/dist/sensiblefox-universal"

    cp "$ROOT_DIR/target/$TARGET_AARCH64/release/sensiblefox" "$ROOT_DIR/dist/sensiblefox-aarch64"
    cp "$ROOT_DIR/target/$TARGET_X86_64/release/sensiblefox" "$ROOT_DIR/dist/sensiblefox-x86_64"
    echo "  ✓ CLI binaries: universal + aarch64 + x86_64"
fi
echo "└────────────────────────────────────────────────────────────────────┘"
echo ""

# ── Step 3: Build .pkg installers ─────────────────────────────────────────
if [ "${SKIP_PKG:-0}" != "1" ]; then
    echo "┌─ Step 3/4: Build PKG installers ─────────────────────────────────┐"
    if [ "${SKIP_OFFLINE:-0}" != "1" ]; then
        BUNDLE_FIREFOX=1 "$SCRIPT_DIR/build-pkg.sh"
    else
        "$SCRIPT_DIR/build-pkg.sh"
    fi
    echo "└────────────────────────────────────────────────────────────────────┘"
    echo ""
else
    echo "┌─ Step 3/4: Build PKG installers (SKIPPED) ───────────────────────┐"
    echo "  SKIP_PKG=1 — skipping .pkg builds"
    echo "└────────────────────────────────────────────────────────────────────┘"
    echo ""
fi

# ── Step 4: Build patched Firefox .dmg ────────────────────────────────────
if [ "${SKIP_DMG:-0}" != "1" ]; then
    echo "┌─ Step 4/4: Build DMG ────────────────────────────────────────────┐"
    "$SCRIPT_DIR/build-dmg.sh"
    echo "└────────────────────────────────────────────────────────────────────┘"
    echo ""
else
    echo "┌─ Step 4/4: Build DMG (SKIPPED) ──────────────────────────────────┐"
    echo "  SKIP_DMG=1 — skipping .dmg build"
    echo "└────────────────────────────────────────────────────────────────────┘"
    echo ""
fi

# ── Summary ───────────────────────────────────────────────────────────────
echo "╔══════════════════════════════════════════════════════════════════╗"
echo "║  Build complete                                                ║"
echo "╚══════════════════════════════════════════════════════════════════╝"
echo ""
echo "  Artifacts in dist/:"
for f in "$ROOT_DIR/dist/"*.pkg "$ROOT_DIR/dist/"*.dmg "$ROOT_DIR/dist/sensiblefox-"*; do
    [ -f "$f" ] || continue
    size=$(du -h "$f" | cut -f1 | tr -d ' ')
    echo "    $(basename "$f")  ($size)"
done
echo ""
