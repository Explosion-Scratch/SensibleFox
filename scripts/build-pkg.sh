#!/usr/bin/env bash
set -euo pipefail

# SensibleFox — Build macOS .pkg installer (productbuild)
#
# Produces dist/SensibleFox.pkg. The .pkg ships:
#   • SensibleFox payload (policies.json, autoconfig.cfg, userChrome.css, uBO managed storage)
#   • A compiled AppleScript applet ("SensibleFox Installer.app") that shows
#     a native macOS progress window driven by /tmp/sensiblefox-install.status.
# At install time the postinstall script:
#   • launches the applet as the console user
#   • streams the latest Firefox download into /Applications/Firefox.app,
#     publishing live progress (MB / %) into the status file
#   • injects the SensibleFox payload into the freshly-installed Firefox.app
#
# A productbuild Welcome screen is rendered with the Firefox version (queried
# at build time) so the installer's first screen shows what is about to ship.
#
# Configurable via environment:
#   INSTALL_LOCATION  Where Firefox.app lands (default: /Applications)
#   FIREFOX_LANG      Firefox locale (default: en-US)
#   PKG_VERSION       Package version (default: 1.0.0)
#   PKG_IDENTIFIER    Package identifier (default: com.sensiblefox.firefox)

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
DIST_DIR="$ROOT_DIR/dist"
ASSETS_DIR="$ROOT_DIR/assets"
INSTALLER_DIR="$ASSETS_DIR/installer"
GEN_DIR="$ROOT_DIR/generated"
PKG_ROOT="$DIST_DIR/pkg-root"
SCRIPTS_DIR="$DIST_DIR/pkg-scripts"
RES_DIR="$DIST_DIR/pkg-resources"
COMPONENT_PKG="$DIST_DIR/component.pkg"

INSTALL_LOCATION="${INSTALL_LOCATION:-/Applications}"
FIREFOX_LANG="${FIREFOX_LANG:-en-US}"
PKG_VERSION="${PKG_VERSION:-1.0.0}"
PKG_IDENTIFIER="${PKG_IDENTIFIER:-com.sensiblefox.firefox}"
SUPPORT_DIR="/Library/Application Support/SensibleFox"
PAYLOAD_DIR="$SUPPORT_DIR/payload"
HELPER_APP_NAME="SensibleFox Installer.app"
HELPER_APP_REL="$SUPPORT_DIR/$HELPER_APP_NAME"

echo "SensibleFox: building installer .pkg"
echo "===================================="
echo "  install location : $INSTALL_LOCATION"
echo "  locale           : $FIREFOX_LANG"
echo "  version          : $PKG_VERSION"
echo ""

if [ ! -f "$GEN_DIR/sensiblefox-defaults.js" ]; then
    echo "  ✗ generated/sensiblefox-defaults.js missing."
    echo "    Run: ./scripts/generate-prefs.sh"
    exit 1
fi

for f in policies.json autoconfig.js sensiblefox.cfg.tail uBlock0@raymondhill.net.json; do
    if [ ! -f "$ASSETS_DIR/$f" ]; then
        echo "  ✗ assets/$f missing."
        exit 1
    fi
done
for f in installer.applescript welcome.html conclusion.html Distribution.xml; do
    if [ ! -f "$INSTALLER_DIR/$f" ]; then
        echo "  ✗ assets/installer/$f missing."
        exit 1
    fi
done

FIREFOX_DMG_URL="https://download.mozilla.org/?product=firefox-latest-ssl&os=osx&lang=$FIREFOX_LANG"
VERSION_URL="https://product-details.mozilla.org/1.0/firefox_versions.json"

echo "  → Querying latest Firefox version..."
FF_VERSION="$(curl -fsSL --max-time 15 "$VERSION_URL" 2>/dev/null \
    | /usr/bin/sed -n 's/.*"LATEST_FIREFOX_VERSION"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -1 || true)"
[ -z "$FF_VERSION" ] && FF_VERSION="latest"

echo "  → Querying Firefox DMG size..."
FF_SIZE_BYTES="$(curl -fsSLI --max-time 15 "$FIREFOX_DMG_URL" 2>/dev/null \
    | /usr/bin/awk 'BEGIN{IGNORECASE=1} /^content-length:/ {gsub(/\r/,""); v=$2} END{print v+0}' || true)"
if [ -z "$FF_SIZE_BYTES" ] || [ "$FF_SIZE_BYTES" -eq 0 ]; then
    FF_SIZE_BYTES=162529280
    FF_SIZE_MB="155"
    FF_INSTALLED_MB="570"
else
    FF_SIZE_MB="$((FF_SIZE_BYTES / 1048576))"
    FF_INSTALLED_MB="$((FF_SIZE_MB * 4))"
fi
FF_FALLBACK_BYTES=$FF_SIZE_BYTES
echo "    Firefox $FF_VERSION — ${FF_SIZE_MB} MB download, ~${FF_INSTALLED_MB} MB installed"

echo "  → Compiling sensiblefox CLI (release)..."
cargo build --release --quiet

echo "  → Cleaning previous build..."
rm -rf "$PKG_ROOT" "$SCRIPTS_DIR" "$RES_DIR" "$COMPONENT_PKG" \
    "$DIST_DIR/SensibleFox.pkg" "$DIST_DIR/sensiblefox.pkg"
mkdir -p "$PKG_ROOT" "$SCRIPTS_DIR" "$RES_DIR"

echo "  → Staging Rust binary to scripts..."
cp "$ROOT_DIR/target/release/sensiblefox" "$SCRIPTS_DIR/sensiblefox"
chmod 755 "$SCRIPTS_DIR/sensiblefox"

echo "  → Compiling progress applet..."
mkdir -p "$PKG_ROOT$SUPPORT_DIR"
HELPER_APP_PATH="$PKG_ROOT$HELPER_APP_REL"
rm -rf "$HELPER_APP_PATH"
osacompile -o "$HELPER_APP_PATH" "$INSTALLER_DIR/installer.applescript"

PLIST="$HELPER_APP_PATH/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleName 'SensibleFox Installer'" "$PLIST" 2>/dev/null \
    || /usr/libexec/PlistBuddy -c "Add :CFBundleName string 'SensibleFox Installer'" "$PLIST"
/usr/libexec/PlistBuddy -c "Set :CFBundleDisplayName 'SensibleFox Installer'" "$PLIST" 2>/dev/null \
    || /usr/libexec/PlistBuddy -c "Add :CFBundleDisplayName string 'SensibleFox Installer'" "$PLIST"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier com.sensiblefox.installer" "$PLIST" 2>/dev/null \
    || /usr/libexec/PlistBuddy -c "Add :CFBundleIdentifier string com.sensiblefox.installer" "$PLIST"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $PKG_VERSION" "$PLIST" 2>/dev/null \
    || /usr/libexec/PlistBuddy -c "Add :CFBundleShortVersionString string $PKG_VERSION" "$PLIST"
/usr/libexec/PlistBuddy -c "Set :NSHighResolutionCapable true" "$PLIST" 2>/dev/null \
    || /usr/libexec/PlistBuddy -c "Add :NSHighResolutionCapable bool true" "$PLIST"

echo "  → Writing postinstall (invokes Rust)..."
cat > "$SCRIPTS_DIR/postinstall" <<POSTINSTALL
#!/usr/bin/env bash
set -e

# This script runs during the PKG installation.
# It invokes the bundled 'sensiblefox' Rust binary to perform the actual
# Firefox download, installation, and policy injection.

SCRIPTS_DIR="\$(dirname "\$0")"
STATUS=/tmp/sensiblefox-install.status
HELPER_APP="$HELPER_APP_REL"

cleanup() {
    rm -f "\$STATUS"
}
trap cleanup EXIT

CONSOLE_USER="\$(/usr/bin/stat -f%Su /dev/console 2>/dev/null || true)"
CONSOLE_UID=""
if [ -n "\$CONSOLE_USER" ] && [ "\$CONSOLE_USER" != "root" ]; then
    CONSOLE_UID="\$(/usr/bin/id -u "\$CONSOLE_USER" 2>/dev/null || true)"
fi

# Launch the progress UI as the console user.
if [ -n "\$CONSOLE_UID" ]; then
    /bin/launchctl asuser "\$CONSOLE_UID" /usr/bin/sudo -u "\$CONSOLE_USER" /usr/bin/open -a "\$HELPER_APP" >/dev/null 2>&1 || true
fi

# Invoke Rust binary to do the heavy lifting.
"\$SCRIPTS_DIR/sensiblefox" \\
    --policied \\
    --system \\
    --unattended \\
    --profile-only \\
    --status-file "\$STATUS"

exit 0
POSTINSTALL
chmod 755 "$SCRIPTS_DIR/postinstall"

echo "  → Building component pkg..."
pkgbuild \
    --root "$PKG_ROOT" \
    --scripts "$SCRIPTS_DIR" \
    --identifier "$PKG_IDENTIFIER" \
    --version "$PKG_VERSION" \
    --install-location "/" \
    "$COMPONENT_PKG" \
    > /dev/null

echo "  → Rendering installer Resources..."
/usr/bin/sed \
    -e "s/{{FF_VERSION}}/$FF_VERSION/g" \
    -e "s/{{FF_SIZE_MB}}/$FF_SIZE_MB/g" \
    -e "s/{{FF_INSTALLED_MB}}/$FF_INSTALLED_MB/g" \
    "$INSTALLER_DIR/welcome.html" > "$RES_DIR/welcome.html"
cp "$INSTALLER_DIR/conclusion.html" "$RES_DIR/conclusion.html"

DIST_XML="$DIST_DIR/Distribution.xml"
/usr/bin/sed \
    -e "s/{{PKG_IDENTIFIER}}/$PKG_IDENTIFIER/g" \
    -e "s/{{PKG_VERSION}}/$PKG_VERSION/g" \
    "$INSTALLER_DIR/Distribution.xml" > "$DIST_XML"

echo "  → Building product .pkg..."
productbuild \
    --distribution "$DIST_XML" \
    --resources "$RES_DIR" \
    --package-path "$DIST_DIR" \
    "$DIST_DIR/SensibleFox.pkg" \
    > /dev/null

rm -rf "$PKG_ROOT" "$SCRIPTS_DIR" "$RES_DIR" "$COMPONENT_PKG" "$DIST_XML"

SIZE=$(du -h "$DIST_DIR/SensibleFox.pkg" | cut -f1 | tr -d ' ')
echo ""
echo "  ✓ Built dist/SensibleFox.pkg ($SIZE) — Firefox $FF_VERSION (download ~${FF_SIZE_MB} MB, installs ~${FF_INSTALLED_MB} MB)"
echo "    Install: open dist/SensibleFox.pkg"
echo "    Or:      sudo installer -pkg dist/SensibleFox.pkg -target /"
echo "    Note:    To bypass macOS Gatekeeper for this unsigned package, run:"
echo "             xattr -d com.apple.quarantine dist/SensibleFox.pkg"
