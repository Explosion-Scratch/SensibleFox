#!/usr/bin/env bash
set -euo pipefail

# SensibleFox — Build macOS .pkg installer(s)
#
# Produces:
#   dist/SensibleFox.pkg           — small online installer (downloads Firefox)
#   dist/SensibleFox-Offline.pkg   — when BUNDLE_FIREFOX=1; pre-stages
#                                    Firefox.app directly into /Applications
#                                    so pkgbuild extracts it in one shot —
#                                    NO postinstall download / mount / copy.
#
# Both PKGs share a single postinstall template that just runs the Rust CLI
# in two phases (root: install + policies, user: profile + extensions).
#
# Env knobs:
#   FIREFOX_LANG, PKG_VERSION, PKG_IDENTIFIER
#   DEVELOPER_ID_APPLICATION, DEVELOPER_ID_INSTALLER, NOTARYTOOL_PROFILE
#   BUNDLE_FIREFOX=1               also build the offline pkg
#   SENSIBLEFOX_NATIVE_ONLY=1      skip cross-arch; ship host arch only (not for distribution)

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
DIST_DIR="$ROOT_DIR/dist"
ASSETS_DIR="$ROOT_DIR/assets"
INSTALLER_DIR="$ASSETS_DIR/installer"
GEN_DIR="$ROOT_DIR/generated"

FIREFOX_LANG="${FIREFOX_LANG:-en-US}"
PKG_VERSION="${PKG_VERSION:-1.0.0}"
PKG_IDENTIFIER="${PKG_IDENTIFIER:-com.sensiblefox.firefox}"
SUPPORT_DIR="/Library/Application Support/SensibleFox"
HELPER_APP_NAME="SensibleFox Installer.app"
HELPER_APP_REL="$SUPPORT_DIR/$HELPER_APP_NAME"

FIREFOX_DMG_URL="https://download.mozilla.org/?product=firefox-latest-ssl&os=osx&lang=$FIREFOX_LANG"
VERSION_URL="https://product-details.mozilla.org/1.0/firefox_versions.json"
UBLOCK_XPI_URL="https://addons.mozilla.org/firefox/downloads/latest/ublock-origin/platform:3/ublock-origin.xpi"

echo "SensibleFox: building installer .pkg"
echo "===================================="
echo "  locale  : $FIREFOX_LANG"
echo "  version : $PKG_VERSION"
echo ""

# ── Sanity ────────────────────────────────────────────────────────────────
[ -f "$GEN_DIR/user.js" ] || { echo "  ✗ generated/user.js missing. Run ./scripts/generate-prefs.sh"; exit 1; }
for f in policies.json uBlock0@raymondhill.net.json; do
    [ -f "$ASSETS_DIR/$f" ] || { echo "  ✗ assets/$f missing."; exit 1; }
done
for f in installer.applescript welcome.html conclusion.html Distribution.xml; do
    [ -f "$INSTALLER_DIR/$f" ] || { echo "  ✗ assets/installer/$f missing."; exit 1; }
done

# ── Look up Firefox version + DMG size for the welcome screen ────────────
echo "  → Querying Firefox version + DMG size..."
FF_VERSION="$(curl -fsSL --max-time 15 "$VERSION_URL" 2>/dev/null \
    | /usr/bin/sed -n 's/.*"LATEST_FIREFOX_VERSION"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -1 || true)"
[ -z "$FF_VERSION" ] && FF_VERSION="latest"

FF_SIZE_BYTES="$(curl -fsSLI --max-time 15 "$FIREFOX_DMG_URL" 2>/dev/null \
    | /usr/bin/awk 'BEGIN{IGNORECASE=1} /^content-length:/ {gsub(/\r/,""); v=$2} END{print v+0}' || true)"
if [ -z "$FF_SIZE_BYTES" ] || [ "$FF_SIZE_BYTES" -eq 0 ]; then
    FF_SIZE_MB="155"; FF_INSTALLED_MB="570"
else
    FF_SIZE_MB="$((FF_SIZE_BYTES / 1048576))"
    FF_INSTALLED_MB="$((FF_SIZE_MB * 4))"
fi
echo "    Firefox $FF_VERSION — ${FF_SIZE_MB} MB download, ~${FF_INSTALLED_MB} MB installed"

TARGET_AARCH64="aarch64-apple-darwin"
TARGET_X86_64="x86_64-apple-darwin"
BIN_ARM="$ROOT_DIR/target/$TARGET_AARCH64/release/sensiblefox"
BIN_X64="$ROOT_DIR/target/$TARGET_X86_64/release/sensiblefox"
SENSIBLEFOX_CLI="$DIST_DIR/sensiblefox-universal"

rustup_has_target() {
    rustup target list --installed 2>/dev/null | /usr/bin/grep -qx "$1"
}

echo "  → Compiling sensiblefox CLI (release)..."
mkdir -p "$DIST_DIR"
if [ "${SENSIBLEFOX_NATIVE_ONLY:-0}" = "1" ]; then
    echo "    (SENSIBLEFOX_NATIVE_ONLY=1 — single-arch host build)"
    cargo build --release --quiet
    SENSIBLEFOX_CLI="$ROOT_DIR/target/release/sensiblefox"
else
    need=()
    rustup_has_target "$TARGET_AARCH64" || need+=("$TARGET_AARCH64")
    rustup_has_target "$TARGET_X86_64" || need+=("$TARGET_X86_64")
    if [ "${#need[@]}" -gt 0 ]; then
        echo "  ✗ Missing Rust std targets: ${need[*]}"
        echo "    Install with: rustup target add ${need[*]}"
        exit 1
    fi
    echo "    targets: $TARGET_AARCH64 + $TARGET_X86_64 → universal (lipo)"
    cargo build --release --target "$TARGET_AARCH64" --quiet
    cargo build --release --target "$TARGET_X86_64" --quiet
    /usr/bin/lipo -create "$BIN_ARM" "$BIN_X64" -output "$SENSIBLEFOX_CLI"
    chmod 755 "$SENSIBLEFOX_CLI"
fi

# ── Helper functions ─────────────────────────────────────────────────────

stage_helper_app() {
    # $1 = absolute path where the SensibleFox Installer.app should live.
    local app_path="$1"
    rm -rf "$app_path"
    osacompile -o "$app_path" "$INSTALLER_DIR/installer.applescript"
    local plist="$app_path/Contents/Info.plist"
    /usr/libexec/PlistBuddy -c "Set :CFBundleName 'SensibleFox Installer'" "$plist" 2>/dev/null \
        || /usr/libexec/PlistBuddy -c "Add :CFBundleName string 'SensibleFox Installer'" "$plist"
    /usr/libexec/PlistBuddy -c "Set :CFBundleDisplayName 'SensibleFox Installer'" "$plist" 2>/dev/null \
        || /usr/libexec/PlistBuddy -c "Add :CFBundleDisplayName string 'SensibleFox Installer'" "$plist"
    /usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier com.sensiblefox.installer" "$plist" 2>/dev/null \
        || /usr/libexec/PlistBuddy -c "Add :CFBundleIdentifier string com.sensiblefox.installer" "$plist"
    /usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $PKG_VERSION" "$plist" 2>/dev/null \
        || /usr/libexec/PlistBuddy -c "Add :CFBundleShortVersionString string $PKG_VERSION" "$plist"
    /usr/libexec/PlistBuddy -c "Set :NSHighResolutionCapable true" "$plist" 2>/dev/null \
        || /usr/libexec/PlistBuddy -c "Add :NSHighResolutionCapable bool true" "$plist"
    if [ -n "${DEVELOPER_ID_APPLICATION:-}" ]; then
        /usr/bin/codesign --force --timestamp --options runtime --sign "$DEVELOPER_ID_APPLICATION" "$app_path"
    fi
}

write_postinstall() {
    # $1 = scripts dir to write into.
    local scripts_dir="$1"
    cat > "$scripts_dir/postinstall" <<POSTINSTALL
#!/usr/bin/env bash
set -euo pipefail

# This script runs during the PKG installation.
#   Root phase: install Firefox (or use the one the payload extracted) and
#               write /Library policy files.
#   User phase: build the SensibleFox profile + extensions as the console user.

SCRIPTS_DIR="\$(dirname "\$0")"
STATUS=/tmp/sensiblefox-install.status
HELPER_APP="$HELPER_APP_REL"

CONSOLE_USER="\$(/usr/bin/stat -f%Su /dev/console 2>/dev/null || true)"
CONSOLE_UID=""
if [ -n "\$CONSOLE_USER" ] && [ "\$CONSOLE_USER" != "root" ]; then
    CONSOLE_UID="\$(/usr/bin/id -u "\$CONSOLE_USER" 2>/dev/null || true)"
fi

# Status file must be writable by the user-phase, but /tmp's sticky bit
# blocks a user-owned rename over a root-owned file. Create it user-owned.
printf 'step=init\ntitle=SensibleFox\ndetail=Preparing installation...\nprogress=0\ntotal=100\n' > "\$STATUS"
chmod 644 "\$STATUS" 2>/dev/null || true
if [ -n "\$CONSOLE_USER" ] && [ "\$CONSOLE_USER" != "root" ]; then
    /usr/sbin/chown "\$CONSOLE_USER" "\$STATUS" 2>/dev/null || true
fi

mark_failed() {
    rc=\$?
    if [ "\$rc" -ne 0 ]; then
        printf 'step=error\ntitle=SensibleFox install failed\ndetail=Installer exited with code %s. See /var/log/install.log.\nprogress=0\ntotal=100\n' "\$rc" > "\$STATUS"
    fi
    exit "\$rc"
}
trap mark_failed EXIT

SF_PID=
abort_install() {
    trap - EXIT INT TERM HUP
    printf 'step=error\ntitle=SensibleFox\ndetail=Installation was stopped.\nprogress=0\ntotal=100\n' > "\$STATUS" 2>/dev/null || true
    if [ -n "\${SF_PID:-}" ]; then
        /bin/kill -TERM "\$SF_PID" 2>/dev/null || true
        /bin/sleep 0.4
        /bin/kill -KILL "\$SF_PID" 2>/dev/null || true
    fi
    /usr/bin/pkill -TERM -f "\$SCRIPTS_DIR/sensiblefox" 2>/dev/null || true
    /bin/sleep 0.2
    /usr/bin/pkill -KILL -f "\$SCRIPTS_DIR/sensiblefox" 2>/dev/null || true
    if [ -n "\${CONSOLE_UID:-}" ] && [ -n "\${CONSOLE_USER:-}" ]; then
        /bin/launchctl asuser "\$CONSOLE_UID" /usr/bin/sudo -u "\$CONSOLE_USER" \\
            /usr/bin/osascript -e 'tell application id "com.sensiblefox.installer" to quit' >/dev/null 2>&1 || true
    fi
    exit 130
}
trap abort_install INT TERM HUP

# Kill any running Firefox so the install replaces it cleanly and we don't
# end up holding a stale profile lock when we launch.
/usr/bin/pkill -x firefox 2>/dev/null || true
/usr/bin/pkill -x Firefox 2>/dev/null || true

# Show the AppleScript progress applet to the console user.
if [ -n "\$CONSOLE_UID" ]; then
    /bin/launchctl asuser "\$CONSOLE_UID" /usr/bin/sudo -u "\$CONSOLE_USER" /usr/bin/open "\$HELPER_APP" >/dev/null 2>&1 || true
fi

# Phase 1 (root): install Firefox + write /Library policies. Run in the
# background so we keep a PID for Stop (SIGTERM) to terminate mid-download.
"\$SCRIPTS_DIR/sensiblefox" --system-only --unattended --status-file "\$STATUS" &
SF_PID=\$!
wait \$SF_PID
SF_PID=

if [ -z "\$CONSOLE_USER" ] || [ -z "\$CONSOLE_UID" ]; then
    printf 'step=error\ntitle=SensibleFox\ndetail=No logged-in user was found to configure.\nprogress=0\ntotal=100\n' > "\$STATUS"
    exit 1
fi

# Phase 2 (user): build the SensibleFox profile + extensions.
/bin/launchctl asuser "\$CONSOLE_UID" \\
    /usr/bin/sudo -u "\$CONSOLE_USER" -H \\
    /usr/bin/env -u SUDO_USER -u SUDO_COMMAND -u SUDO_GID -u SUDO_UID \\
    "\$SCRIPTS_DIR/sensiblefox" \\
        --unattended --profile-only --no-policies --status-file "\$STATUS" &
SF_PID=\$!
wait \$SF_PID
SF_PID=

printf 'step=done\ntitle=SensibleFox installed\ndetail=Firefox is ready to launch.\nprogress=100\ntotal=100\n' > "\$STATUS"

for _ in 1 2 3 4 5 6 7 8 9 10; do
    /usr/bin/pgrep -f "SensibleFox Installer" >/dev/null 2>&1 || break
    /bin/sleep 0.2
done
/bin/launchctl asuser "\$CONSOLE_UID" /usr/bin/sudo -u "\$CONSOLE_USER" \\
    /usr/bin/osascript -e 'tell application id "com.sensiblefox.installer" to quit' >/dev/null 2>&1 || true

exit 0
POSTINSTALL
    chmod 755 "$scripts_dir/postinstall"
}

build_pkg() {
    # $1 = pkg root dir, $2 = scripts dir, $3 = resources dir,
    # $4 = identifier, $5 = output .pkg, $6 = welcome-version label
    local pkg_root="$1" scripts_dir="$2" res_dir="$3"
    local identifier="$4" out_pkg="$5" version_label="$6"
    local component dist_xml
    component="$DIST_DIR/component-$(basename "$out_pkg" .pkg).pkg"
    dist_xml="$DIST_DIR/Distribution-$(basename "$out_pkg" .pkg).xml"

    cp "$SENSIBLEFOX_CLI" "$scripts_dir/sensiblefox"
    chmod 755 "$scripts_dir/sensiblefox"
    if [ -n "${DEVELOPER_ID_APPLICATION:-}" ]; then
        /usr/bin/codesign --force --timestamp --options runtime \
            --sign "$DEVELOPER_ID_APPLICATION" "$scripts_dir/sensiblefox"
    fi

    mkdir -p "$pkg_root$SUPPORT_DIR"
    stage_helper_app "$pkg_root$HELPER_APP_REL"
    write_postinstall "$scripts_dir"

    pkgbuild \
        --root "$pkg_root" \
        --scripts "$scripts_dir" \
        --identifier "$identifier" \
        --version "$PKG_VERSION" \
        --install-location "/" \
        "$component" > /dev/null

    /usr/bin/sed \
        -e "s/{{FF_VERSION}}/$version_label/g" \
        -e "s/{{FF_SIZE_MB}}/$FF_SIZE_MB/g" \
        -e "s/{{FF_INSTALLED_MB}}/$FF_INSTALLED_MB/g" \
        "$INSTALLER_DIR/welcome.html" > "$res_dir/welcome.html"
    cp "$INSTALLER_DIR/conclusion.html" "$res_dir/conclusion.html"

    /usr/bin/sed \
        -e "s/{{PKG_IDENTIFIER}}/$identifier/g" \
        -e "s/{{PKG_VERSION}}/$PKG_VERSION/g" \
        "$INSTALLER_DIR/Distribution.xml" \
        | /usr/bin/sed "s|component.pkg|$(basename "$component")|g" \
        > "$dist_xml"

    productbuild \
        --distribution "$dist_xml" \
        --resources "$res_dir" \
        --package-path "$DIST_DIR" \
        "$out_pkg" > /dev/null

    if [ -n "${DEVELOPER_ID_INSTALLER:-}" ]; then
        productsign --sign "$DEVELOPER_ID_INSTALLER" "$out_pkg" "$out_pkg.signed" > /dev/null
        mv "$out_pkg.signed" "$out_pkg"
    fi

    rm -f "$component" "$dist_xml"
}

# ── Build the online pkg ─────────────────────────────────────────────────
echo "  → Building online .pkg..."
ONLINE_ROOT="$DIST_DIR/pkg-root"
ONLINE_SCRIPTS="$DIST_DIR/pkg-scripts"
ONLINE_RES="$DIST_DIR/pkg-resources"
rm -rf "$ONLINE_ROOT" "$ONLINE_SCRIPTS" "$ONLINE_RES" "$DIST_DIR/SensibleFox.pkg"
mkdir -p "$ONLINE_ROOT" "$ONLINE_SCRIPTS" "$ONLINE_RES"
build_pkg "$ONLINE_ROOT" "$ONLINE_SCRIPTS" "$ONLINE_RES" \
    "$PKG_IDENTIFIER" "$DIST_DIR/SensibleFox.pkg" "$FF_VERSION"
rm -rf "$ONLINE_ROOT" "$ONLINE_SCRIPTS" "$ONLINE_RES"

if [ -n "${NOTARYTOOL_PROFILE:-}" ]; then
    [ -n "${DEVELOPER_ID_INSTALLER:-}" ] || { echo "  ✗ NOTARYTOOL_PROFILE needs DEVELOPER_ID_INSTALLER"; exit 1; }
    echo "  → Notarizing online .pkg..."
    xcrun notarytool submit "$DIST_DIR/SensibleFox.pkg" --keychain-profile "$NOTARYTOOL_PROFILE" --wait
    xcrun stapler staple "$DIST_DIR/SensibleFox.pkg"
fi

SIZE=$(du -h "$DIST_DIR/SensibleFox.pkg" | cut -f1 | tr -d ' ')
echo ""
echo "  ✓ Built dist/SensibleFox.pkg ($SIZE) — Firefox $FF_VERSION (~${FF_SIZE_MB} MB download)"

# ── Build the offline pkg (Firefox.app pre-staged in the payload) ────────
if [ "${BUNDLE_FIREFOX:-0}" = "1" ]; then
    echo ""
    echo "  → Building offline .pkg (Firefox pre-staged in /Applications)..."
    OFF_ROOT="$DIST_DIR/pkg-root-offline"
    OFF_SCRIPTS="$DIST_DIR/pkg-scripts-offline"
    OFF_RES="$DIST_DIR/pkg-resources-offline"
    OFF_PKG="$DIST_DIR/SensibleFox-Offline.pkg"
    BUNDLE_REL="/Library/Application Support/SensibleFox/bundles"
    rm -rf "$OFF_ROOT" "$OFF_SCRIPTS" "$OFF_RES" "$OFF_PKG"
    mkdir -p "$OFF_ROOT/Applications" "$OFF_ROOT$BUNDLE_REL" "$OFF_SCRIPTS" "$OFF_RES"

    # Download Firefox.dmg once at build time, extract Firefox.app, drop it
    # straight into the payload's /Applications. At install time, pkgbuild
    # copies it into /Applications in a single pass — no postinstall mount,
    # no second copy, no DMG sitting on the user's disk afterwards.
    echo "    ↓ Downloading Firefox.dmg..."
    BUILD_TMP="$(mktemp -d)"
    trap 'rm -rf "$BUILD_TMP"' EXIT
    curl -fL --retry 3 --output "$BUILD_TMP/Firefox.dmg" "$FIREFOX_DMG_URL"
    echo "    ↪ Extracting Firefox.app into payload..."
    MOUNT="$BUILD_TMP/mount"
    mkdir -p "$MOUNT"
    /usr/bin/hdiutil attach -nobrowse -noverify -quiet -mountpoint "$MOUNT" "$BUILD_TMP/Firefox.dmg"
    /usr/bin/ditto --noqtn "$MOUNT/Firefox.app" "$OFF_ROOT/Applications/Firefox.app"
    /usr/bin/hdiutil detach -quiet "$MOUNT" || true

    echo "    ↓ Downloading uBlock Origin XPI to bundle..."
    curl -fL --retry 3 \
        --output "$OFF_ROOT$BUNDLE_REL/uBlock0@raymondhill.net.xpi" \
        "$UBLOCK_XPI_URL"

    build_pkg "$OFF_ROOT" "$OFF_SCRIPTS" "$OFF_RES" \
        "${PKG_IDENTIFIER}.offline" "$OFF_PKG" "$FF_VERSION (bundled)"

    if [ -n "${NOTARYTOOL_PROFILE:-}" ] && [ -n "${DEVELOPER_ID_INSTALLER:-}" ]; then
        echo "    → Notarizing offline .pkg..."
        xcrun notarytool submit "$OFF_PKG" --keychain-profile "$NOTARYTOOL_PROFILE" --wait
        xcrun stapler staple "$OFF_PKG"
    fi

    rm -rf "$OFF_ROOT" "$OFF_SCRIPTS" "$OFF_RES"
    OFF_SIZE=$(du -h "$OFF_PKG" | cut -f1 | tr -d ' ')
    echo "  ✓ Built dist/SensibleFox-Offline.pkg ($OFF_SIZE) — Firefox $FF_VERSION pre-staged"
fi

echo ""
echo "    Install: open dist/SensibleFox.pkg"
echo "    Or:      sudo installer -pkg dist/SensibleFox.pkg -target /"
if [ -z "${DEVELOPER_ID_INSTALLER:-}" ]; then
    echo "    Note:    Unsigned; right-click → Open on first launch."
fi
