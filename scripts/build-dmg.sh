#!/usr/bin/env bash
set -euo pipefail

# SensibleFox — Build a standalone patched Firefox.app as a DMG
#
# Produces:
#   dist/SensibleFox.dmg — a SensibleFox.app (branded + re-signed Firefox)
#                          with all SensibleFox customizations baked directly
#                          into the app bundle via autoconfig + enterprise
#                          policies. Every new profile created from this app
#                          gets sensiblefox defaults automatically.
#                          Download → open DMG → drag to /Applications → open.
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
#   SENSIBLEFOX_APP_NAME   default: SensibleFox
#   SENSIBLEFOX_BUNDLE_ID  default: com.sensiblefox.browser
#   SENSIBLEFOX_ICON       default: assets/SensibleFox.icns — baked into the
#                          bundle (CFBundleIconFile) so the icon survives
#                          reboots, cache clears, and reinstalls.
#   DEVELOPER_ID_APPLICATION  Developer ID identity for re-signing the
#                             branded app (smooth Gatekeeper on other Macs).
#                             Without it the app is ad-hoc signed: works
#                             locally, needs right-click → Open elsewhere.
#   NOTARYTOOL_PROFILE      notarize + staple the finished DMG.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
DIST_DIR="$ROOT_DIR/dist"
ASSETS_DIR="$ROOT_DIR/assets"
GEN_DIR="$ROOT_DIR/generated"

FIREFOX_LANG="${FIREFOX_LANG:-en-US}"
DMG_VOLUME_NAME="${DMG_VOLUME_NAME:-SensibleFox}"
SENSIBLEFOX_APP_NAME="${SENSIBLEFOX_APP_NAME:-SensibleFox}"
SENSIBLEFOX_BUNDLE_ID="${SENSIBLEFOX_BUNDLE_ID:-com.sensiblefox.browser}"
SENSIBLEFOX_ICON="${SENSIBLEFOX_ICON:-$ASSETS_DIR/SensibleFox.icns}"
FIREFOX_DMG_URL="https://download.mozilla.org/?product=firefox-esr-latest-ssl&os=osx&lang=$FIREFOX_LANG"
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
# NOTE: the DMG ships policies-dmg.json (= policies.json + ManualAppUpdateOnly).
# Mozilla self-update would overwrite our branded Info.plist and silently
# un-brand the app (updater.cpp: complete updates remove every precomplete-
# listed file, and Info.plist ships in every MAR). So the branded app never
# auto-updates or prompts; updates only appear when the user opens
# Settings > General, and installing one from there still de-brands —
# ESR point releases should arrive via a rebuilt DMG / reinstall.
# The PKG/CLI path is manual-only too (same policy in policies.json).
[ -f "$ASSETS_DIR/policies-dmg.json" ]       || { echo "  ✗ assets/policies-dmg.json missing."; exit 1; }
[ -f "$ASSETS_DIR/autoconfig.js" ]           || { echo "  ✗ assets/autoconfig.js missing."; exit 1; }
[ -f "$ASSETS_DIR/sensiblefox.cfg.tail" ]    || { echo "  ✗ assets/sensiblefox.cfg.tail missing."; exit 1; }

mkdir -p "$DIST_DIR"

# ── Look up Firefox version ──────────────────────────────────────────────
echo "  → Querying Firefox version..."
FF_VERSION="$(curl -fsSL --max-time 15 "$VERSION_URL" 2>/dev/null \
    | /usr/bin/sed -n 's/.*"FIREFOX_ESR"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -1 || true)"
[ -z "$FF_VERSION" ] && FF_VERSION="latest"
echo "    Firefox $FF_VERSION"

# ── Download Firefox.dmg ─────────────────────────────────────────────────
echo "  → Downloading Firefox.dmg..."
BUILD_TMP="$(mktemp -d)"
cleanup() { rm -rf "$BUILD_TMP"; }
trap cleanup EXIT

curl -fL --retry 3 --retry-all-errors --connect-timeout 15 --max-time 1800 \
    --output "$BUILD_TMP/Firefox.dmg" "$FIREFOX_DMG_URL"

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
cp "$ASSETS_DIR/policies-dmg.json" "$DISTRIBUTION/policies.json"

# ── Patch: uBlock managed storage (distribution/) ────────────────────────
echo "  → Patching: uBlock managed storage..."
mkdir -p "$DISTRIBUTION"
cp "$ASSETS_DIR/uBlock0@raymondhill.net.json" "$DISTRIBUTION/uBlock0@raymondhill.net.json"

# ── Patch: uBlock Origin XPI (distribution/extensions/) ──────────────────
echo "  → Downloading uBlock Origin XPI..."
mkdir -p "$DISTRIBUTION/extensions"
curl -fL --retry 3 --retry-all-errors --connect-timeout 15 --max-time 600 \
    --output "$DISTRIBUTION/extensions/uBlock0@raymondhill.net.xpi" \
    "$UBLOCK_XPI_URL"
echo "    uBlock Origin installed to distribution/extensions/"

# ── Brand: Firefox.app → SensibleFox.app ─────────────────────────────────
# Patching the bundle invalidates Mozilla's code signature, so the app must
# be re-signed under our own identity. Renaming (rather than shipping a
# second "Firefox.app") avoids clobbering a user's stock Firefox in
# /Applications and gives the branded app its own Dock identity.
BRANDED_APP="$STAGING/$SENSIBLEFOX_APP_NAME.app"
PLIST="$BRANDED_APP/Contents/Info.plist"
set_or_add() {
    /usr/libexec/PlistBuddy -c "Set :$1 '$3'" "$PLIST" 2>/dev/null \
        || /usr/libexec/PlistBuddy -c "Add :$1 $2 '$3'" "$PLIST"
}
if [ "$SENSIBLEFOX_APP_NAME" = "Firefox" ]; then
    echo "  (keeping upstream Firefox.app name + stock Info.plist)"
else
    echo "  → Branding: $SENSIBLEFOX_APP_NAME.app..."
    mv "$STAGING/Firefox.app" "$BRANDED_APP"
    set_or_add CFBundleName string "$SENSIBLEFOX_APP_NAME"
    set_or_add CFBundleDisplayName string "$SENSIBLEFOX_APP_NAME"
    set_or_add CFBundleIdentifier string "$SENSIBLEFOX_BUNDLE_ID"
fi

# ── Custom icon — baked into the bundle (not Finder Get-Info pasting,
# which lives in a fragile resource-fork xattr and resets). Must run before
# re-signing so the signature covers the new Resources.
# Hard gate (not warn-and-continue): the branded app must never ship with
# the stock Firefox icon. Pass SENSIBLEFOX_ICON=none to opt out explicitly.
# ──────────────────────────────────────────────────────────────────────────
if [ "${SENSIBLEFOX_ICON:-}" = "none" ]; then
    echo "  (SENSIBLEFOX_ICON=none — keeping upstream icon)"
elif [ -f "${SENSIBLEFOX_ICON:-}" ]; then
    echo "  → Applying custom icon..."
    # CFBundleIconFile takes the icon name WITHOUT extension (Apple
    # convention — matches Mozilla's own "firefox" value). Finder ignores
    # the key when it carries the ".icns" suffix.
    APP_ICON_NAME="$(basename "$SENSIBLEFOX_ICON" .icns)"
    cp "$SENSIBLEFOX_ICON" "$BRANDED_APP/Contents/Resources/$APP_ICON_NAME.icns"
    rm -f "$BRANDED_APP/Contents/Resources/firefox.icns"
    set_or_add CFBundleIconFile string "$APP_ICON_NAME"
    echo "    icon: $APP_ICON_NAME.icns"
else
    echo "  ✗ SENSIBLEFOX_ICON=$SENSIBLEFOX_ICON missing." >&2
    echo "    The icon asset must be committed (assets/SensibleFox.icns)," >&2
    echo "    or pass SENSIBLEFOX_ICON=<path|none>." >&2
    exit 1
fi

# ── Strip quarantine attributes ──────────────────────────────────────────
echo "  → Stripping quarantine attributes..."
xattr -cr "$BRANDED_APP" 2>/dev/null || true

# ── Re-sign (required: patching broke Mozilla's signature) ───────────────
echo "  → Re-signing $SENSIBLEFOX_APP_NAME.app..."
if [ -n "${DEVELOPER_ID_APPLICATION:-}" ]; then
    /usr/bin/codesign --force --deep --timestamp --options runtime \
        --sign "$DEVELOPER_ID_APPLICATION" "$BRANDED_APP"
    echo "    signed with: $DEVELOPER_ID_APPLICATION"
else
    # Ad-hoc: launches on this Mac, right-click → Open on others.
    /usr/bin/codesign --force --deep --sign - "$BRANDED_APP"
    echo "    ad-hoc signed (no DEVELOPER_ID_APPLICATION — Gatekeeper will"
    echo "    prompt on other Macs; right-click → Open, or sign + notarize)"
fi
/usr/bin/codesign --verify --deep --strict "$BRANDED_APP"

# ── Create DMG ───────────────────────────────────────────────────────────
# Pretty drag-to-install layout, cloned from Mozilla's own Firefox.dmg:
# .background/background.png (1440x880, generic — no Firefox branding),
# /Applications symlink, .VolumeIcon.icns (SensibleFox fox, not Firefox),
# icon size 128, positions SensibleFox.app (190,472) / Applications (510,472),
# window 680x425. Applied live via Finder (volume-name independent).
OUT_DMG="$DIST_DIR/SensibleFox.dmg"
rm -f "$OUT_DMG"

echo "  → Styling DMG (drag-to-install layout)..."
ln -sfn /Applications "$STAGING/Applications"
mkdir -p "$STAGING/.background"
cp "$ASSETS_DIR/dmg-background.png" "$STAGING/.background/background.png"
/usr/bin/SetFile -a V "$STAGING/.background/background.png" 2>/dev/null || true
if [ -f "${SENSIBLEFOX_ICON:-}" ]; then
    cp "$SENSIBLEFOX_ICON" "$STAGING/.VolumeIcon.icns"
    /usr/bin/SetFile -a C "$STAGING" 2>/dev/null || true
else
    echo "  (no volume icon — SENSIBLEFOX_ICON=none)"
fi

echo "  → Creating SensibleFox.dmg..."
RW_DMG="$DIST_DIR/.SensibleFox-rw.dmg"
rm -f "$RW_DMG"
/usr/bin/hdiutil create \
    -volname "$DMG_VOLUME_NAME" \
    -srcfolder "$STAGING" \
    -ov \
    -format UDRW \
    "$RW_DMG" > /dev/null
RW_MOUNT="$(mktemp -d)"
if /usr/bin/hdiutil attach -nobrowse -noverify -quiet -mountpoint "$RW_MOUNT" "$RW_DMG" \
    && /usr/bin/osascript <<OSA_EOF 2>/dev/null
tell application "Finder"
    set theDisk to disk "$DMG_VOLUME_NAME"
    open theDisk
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set the bounds of container window to {200, 120, 880, 545}
    set theViewOptions to the icon view options of container window
    set arrangement of theViewOptions to not arranged
    set icon size of theViewOptions to 128
    set background picture of theViewOptions to file ".background:background.png" of theDisk
    set position of item "$SENSIBLEFOX_APP_NAME.app" of container window to {190, 472}
    set position of item "Applications" of container window to {510, 472}
    close container window
    open theDisk
    update container window without registering applications
    delay 2
    close container window
end tell
OSA_EOF
then
    echo "    Finder layout applied (icon view, background, positions)"
else
    echo "    (Finder styling skipped — headless session? plain DMG layout)"
fi
# Finder writes the .DS_Store on touch — sync it back so the layout sticks.
/usr/bin/hdiutil detach -quiet "$RW_MOUNT" || true
rmdir "$RW_MOUNT"
/usr/bin/hdiutil convert "$RW_DMG" -format UDZO -o "$OUT_DMG" > /dev/null
rm -f "$RW_DMG"

if [ -n "${NOTARYTOOL_PROFILE:-}" ]; then
    echo "  → Notarizing SensibleFox.dmg..."
    xcrun notarytool submit "$OUT_DMG" --keychain-profile "$NOTARYTOOL_PROFILE" --wait
    xcrun stapler staple "$OUT_DMG"
fi

DMG_SIZE=$(du -h "$OUT_DMG" | cut -f1 | tr -d ' ')
echo ""
echo "  ✓ Built dist/SensibleFox.dmg ($DMG_SIZE) — Firefox $FF_VERSION as $SENSIBLEFOX_APP_NAME.app"
echo ""
echo "    Install: open dist/SensibleFox.dmg"
echo "    Then drag $SENSIBLEFOX_APP_NAME.app to /Applications and open it."
echo ""
echo "    The patched app applies SensibleFox defaults to every new profile via:"
echo "      • Enterprise policies (distribution/policies.json)"
echo "      • Autoconfig prefs (sensiblefox.cfg → defaultPref)"
echo "      • CSS injection (autoconfig → Resources/sensiblefox/)"
echo "      • Ctrl/Cmd+Shift+A → tab search (autoconfig → sensiblefox.cfg)"
if [ "${SENSIBLEFOX_ICON:-}" != "none" ]; then
    echo "      • Custom SensibleFox icon (baked into bundle)"
fi
echo "      • uBlock Origin (distribution/extensions/)"
