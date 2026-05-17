#!/usr/bin/env bash
set -euo pipefail

# SensibleFox — Build a standalone patched Firefox.app as a DMG
#
# Produces:
#   dist/SensibleFox.dmg — a Firefox.app with all SensibleFox customizations
#                          baked directly into the app bundle via autoconfig +
#                          enterprise policies. Every new profile created from
#                          this app gets sensiblefox defaults automatically.
#
# The patched app contains:
#   Resources/defaults/pref/autoconfig.js     — bootstraps sensiblefox.cfg
#   Resources/sensiblefox.cfg                 — defaultPref() prefs + CSS injection
#   Resources/sensiblefox/userChrome.css      — aggregated UI stylesheets
#   Resources/sensiblefox/userContent.css     — page-level stylesheets (if any)
#   Resources/distribution/policies.json      — enterprise policies
#   Resources/distribution/extensions/*.xpi   — pre-installed extensions
#
# Env knobs:
#   FIREFOX_LANG     default: en-US
#   DMG_VOLUME_NAME  default: SensibleFox

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
DIST_DIR="$ROOT_DIR/dist"
ASSETS_DIR="$ROOT_DIR/assets"
GEN_DIR="$ROOT_DIR/generated"

FIREFOX_LANG="${FIREFOX_LANG:-en-US}"
DMG_VOLUME_NAME="${DMG_VOLUME_NAME:-SensibleFox}"
FIREFOX_DMG_URL="https://download.mozilla.org/?product=firefox-latest-ssl&os=osx&lang=$FIREFOX_LANG"
VERSION_URL="https://product-details.mozilla.org/1.0/firefox_versions.json"
UBLOCK_XPI_URL="https://addons.mozilla.org/firefox/downloads/latest/ublock-origin/platform:3/ublock-origin.xpi"

echo "SensibleFox: building standalone patched Firefox DMG"
echo "===================================================="
echo "  locale       : $FIREFOX_LANG"
echo ""

# ── Sanity ────────────────────────────────────────────────────────────────
[ -f "$GEN_DIR/user.js" ]                    || { echo "  ✗ generated/user.js missing. Run ./scripts/generate-prefs.sh"; exit 1; }
[ -f "$GEN_DIR/sensiblefox-defaults.js" ]    || { echo "  ✗ generated/sensiblefox-defaults.js missing. Run ./scripts/generate-prefs.sh"; exit 1; }
[ -f "$ASSETS_DIR/policies.json" ]           || { echo "  ✗ assets/policies.json missing."; exit 1; }
[ -f "$ASSETS_DIR/autoconfig.js" ]           || { echo "  ✗ assets/autoconfig.js missing."; exit 1; }
[ -f "$ASSETS_DIR/sensiblefox.cfg.tail" ]    || { echo "  ✗ assets/sensiblefox.cfg.tail missing."; exit 1; }

mkdir -p "$DIST_DIR"

# ── Look up Firefox version ──────────────────────────────────────────────
echo "  → Querying Firefox version..."
FF_VERSION="$(curl -fsSL --max-time 15 "$VERSION_URL" 2>/dev/null \
    | /usr/bin/sed -n 's/.*"LATEST_FIREFOX_VERSION"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -1 || true)"
[ -z "$FF_VERSION" ] && FF_VERSION="latest"
echo "    Firefox $FF_VERSION"

# ── Download Firefox.dmg ─────────────────────────────────────────────────
echo "  → Downloading Firefox.dmg..."
BUILD_TMP="$(mktemp -d)"
cleanup() { rm -rf "$BUILD_TMP"; }
trap cleanup EXIT

curl -fL --retry 3 --output "$BUILD_TMP/Firefox.dmg" "$FIREFOX_DMG_URL"

# ── Mount + extract Firefox.app ──────────────────────────────────────────
echo "  → Extracting Firefox.app..."
MOUNT_DIR="$BUILD_TMP/mount"
mkdir -p "$MOUNT_DIR"
/usr/bin/hdiutil attach -nobrowse -noverify -quiet -mountpoint "$MOUNT_DIR" "$BUILD_TMP/Firefox.dmg"

STAGING="$BUILD_TMP/staging"
mkdir -p "$STAGING"
/usr/bin/ditto --noqtn "$MOUNT_DIR/Firefox.app" "$STAGING/Firefox.app"
/usr/bin/hdiutil detach -quiet "$MOUNT_DIR" || true

RESOURCES="$STAGING/Firefox.app/Contents/Resources"

# ── Patch: autoconfig bootstrap ──────────────────────────────────────────
echo "  → Patching: autoconfig bootstrap..."
mkdir -p "$RESOURCES/defaults/pref"
cp "$ASSETS_DIR/autoconfig.js" "$RESOURCES/defaults/pref/autoconfig.js"

# ── Patch: sensiblefox.cfg (defaultPref prefs + CSS injection) ───────────
echo "  → Patching: sensiblefox.cfg..."
{
    echo "//"
    cat "$GEN_DIR/sensiblefox-defaults.js"
    echo ""
    cat "$ASSETS_DIR/sensiblefox.cfg.tail"
} > "$RESOURCES/sensiblefox.cfg"
cfg_prefs=$(grep -c 'defaultPref(' "$RESOURCES/sensiblefox.cfg" || true)
echo "    $cfg_prefs defaultPref() entries + CSS injection"

# ── Patch: CSS stylesheets (loaded by autoconfig CSS injection) ──────────
echo "  → Patching: CSS stylesheets..."
SF_CSS_DIR="$RESOURCES/sensiblefox"
mkdir -p "$SF_CSS_DIR"

CSS_MODULES=(
    "macos-native-tabbar.css:macos-native-tabbar.css"
    "ublock_icon_change.css:ublock-icon-change.css"
    "cleaner_extensions_menu.css:cleaner-extensions-menu.css"
    "no_search_engines_in_url_bar.css:no-search-engines-urlbar.css"
    "privacy_change_email_text.css:privacy-email-text.css"
    "show_searchbar_dots_only_on_hover.css:searchbar-dots-hover.css"
    "context_menu_cleanup.css:context-menu-cleanup.css"
)

UCHROME_IMPORTS=""
for entry in "${CSS_MODULES[@]}"; do
    src="${entry%%:*}"
    dest="${entry##*:}"
    cp "$ASSETS_DIR/$src" "$SF_CSS_DIR/$dest"
    UCHROME_IMPORTS="${UCHROME_IMPORTS}@import url(\"css/${dest}\");\n"
done

cat > "$SF_CSS_DIR/userChrome.css" <<USERCHROME
/* sensiblefox — userChrome.css (auto-generated for DMG build) */

$(for entry in "${CSS_MODULES[@]}"; do
    dest="${entry##*:}"
    echo "@import url(\"$dest\");"
done)
USERCHROME

touch "$SF_CSS_DIR/userContent.css"
echo "    ${#CSS_MODULES[@]} CSS modules + userChrome.css"

# ── Patch: enterprise policies (distribution/) ───────────────────────────
echo "  → Patching: enterprise policies..."
DISTRIBUTION="$RESOURCES/distribution"
mkdir -p "$DISTRIBUTION"
cp "$ASSETS_DIR/policies.json" "$DISTRIBUTION/policies.json"

# ── Patch: uBlock managed storage (distribution/) ────────────────────────
echo "  → Patching: uBlock managed storage..."
mkdir -p "$DISTRIBUTION"
cp "$ASSETS_DIR/uBlock0@raymondhill.net.json" "$DISTRIBUTION/uBlock0@raymondhill.net.json"

# ── Patch: uBlock Origin XPI (distribution/extensions/) ──────────────────
echo "  → Downloading uBlock Origin XPI..."
mkdir -p "$DISTRIBUTION/extensions"
curl -fL --retry 3 \
    --output "$DISTRIBUTION/extensions/uBlock0@raymondhill.net.xpi" \
    "$UBLOCK_XPI_URL"
echo "    uBlock Origin installed to distribution/extensions/"

# ── Strip quarantine attributes ──────────────────────────────────────────
echo "  → Stripping quarantine attributes..."
xattr -cr "$STAGING/Firefox.app" 2>/dev/null || true

# ── Create DMG ───────────────────────────────────────────────────────────
OUT_DMG="$DIST_DIR/SensibleFox.dmg"
rm -f "$OUT_DMG"

echo "  → Creating SensibleFox.dmg..."
/usr/bin/hdiutil create \
    -volname "$DMG_VOLUME_NAME" \
    -srcfolder "$STAGING" \
    -ov \
    -format UDZO \
    "$OUT_DMG" > /dev/null

DMG_SIZE=$(du -h "$OUT_DMG" | cut -f1 | tr -d ' ')
echo ""
echo "  ✓ Built dist/SensibleFox.dmg ($DMG_SIZE) — Firefox $FF_VERSION"
echo ""
echo "    Install: open dist/SensibleFox.dmg"
echo "    Then drag Firefox.app to /Applications"
echo ""
echo "    The patched app applies SensibleFox defaults to every new profile via:"
echo "      • Enterprise policies (distribution/policies.json)"
echo "      • Autoconfig prefs (sensiblefox.cfg → defaultPref)"
echo "      • CSS injection (autoconfig → Resources/sensiblefox/)"
echo "      • uBlock Origin (distribution/extensions/)"
