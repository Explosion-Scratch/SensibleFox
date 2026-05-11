#!/usr/bin/env bash
set -euo pipefail

# SensibleFox — Build macOS .pkg installer (productbuild)
#
# Produces dist/SensibleFox.pkg. The .pkg ships:
#   • SensibleFox CLI plus a small progress applet
#   • A compiled AppleScript applet ("SensibleFox Installer.app") that shows
#     a native macOS progress window driven by /tmp/sensiblefox-install.status.
# At install time the postinstall script:
#   • launches the applet as the console user
#   • installs or repairs Firefox.app only when needed
#   • writes Firefox policies to /Library/Preferences/org.mozilla.firefox.plist
#   • configures the logged-in user's SensibleFox profile
#
# A productbuild Welcome screen is rendered with the Firefox version (queried
# at build time) so the installer's first screen shows what is about to ship.
#
# Configurable via environment:
#   INSTALL_LOCATION  Where Firefox.app lands (default: /Applications)
#   FIREFOX_LANG      Firefox locale (default: en-US)
#   PKG_VERSION       Package version (default: 1.0.0)
#   PKG_IDENTIFIER    Package identifier (default: com.sensiblefox.firefox)
#   DEVELOPER_ID_APPLICATION  Optional Developer ID Application identity
#   DEVELOPER_ID_INSTALLER    Optional Developer ID Installer identity
#   NOTARYTOOL_PROFILE        Optional notarytool keychain profile for notarization

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
HELPER_APP_NAME="SensibleFox Installer.app"
HELPER_APP_REL="$SUPPORT_DIR/$HELPER_APP_NAME"
SIGNED_PKG="$DIST_DIR/SensibleFox-signed.pkg"

echo "SensibleFox: building installer .pkg"
echo "===================================="
echo "  install location : $INSTALL_LOCATION"
echo "  locale           : $FIREFOX_LANG"
echo "  version          : $PKG_VERSION"
echo ""

if [ ! -f "$GEN_DIR/user.js" ]; then
    echo "  ✗ generated/user.js missing."
    echo "    Run: ./scripts/generate-prefs.sh"
    exit 1
fi

for f in policies.json uBlock0@raymondhill.net.json; do
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
echo "    Firefox $FF_VERSION — ${FF_SIZE_MB} MB download, ~${FF_INSTALLED_MB} MB installed"

echo "  → Compiling sensiblefox CLI (release)..."
cargo build --release --quiet

echo "  → Cleaning previous build..."
rm -rf "$PKG_ROOT" "$SCRIPTS_DIR" "$RES_DIR" "$COMPONENT_PKG" \
    "$DIST_DIR/SensibleFox.pkg" "$SIGNED_PKG" "$DIST_DIR/sensiblefox.pkg"
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

if [ -n "${DEVELOPER_ID_APPLICATION:-}" ]; then
    echo "  → Signing helper app and CLI..."
    /usr/bin/codesign --force --timestamp --options runtime --sign "$DEVELOPER_ID_APPLICATION" "$HELPER_APP_PATH"
    /usr/bin/codesign --force --timestamp --options runtime --sign "$DEVELOPER_ID_APPLICATION" "$SCRIPTS_DIR/sensiblefox"
fi

echo "  → Writing postinstall (invokes Rust)..."
cat > "$SCRIPTS_DIR/postinstall" <<POSTINSTALL
#!/usr/bin/env bash
set -euo pipefail

# This script runs during the PKG installation.
# Root work installs Firefox and system policy files. User work creates the
# profile as the logged-in person, never as /var/root.

SCRIPTS_DIR="\$(dirname "\$0")"
STATUS=/tmp/sensiblefox-install.status
HELPER_APP="$HELPER_APP_REL"

printf 'step=init\ntitle=SensibleFox\ndetail=Preparing installation...\nprogress=0\ntotal=100\n' > "\$STATUS"
chmod 644 "\$STATUS" 2>/dev/null || true

mark_failed() {
    rc=\$?
    if [ "\$rc" -ne 0 ]; then
        printf 'step=error\ntitle=SensibleFox install failed\ndetail=The package installer exited with code %s. Check /var/log/install.log for details.\nprogress=0\ntotal=100\n' "\$rc" > "\$STATUS"
    fi
    exit "\$rc"
}
trap mark_failed EXIT

CONSOLE_USER="\$(/usr/bin/stat -f%Su /dev/console 2>/dev/null || true)"
CONSOLE_UID=""
if [ -n "\$CONSOLE_USER" ] && [ "\$CONSOLE_USER" != "root" ]; then
    CONSOLE_UID="\$(/usr/bin/id -u "\$CONSOLE_USER" 2>/dev/null || true)"
fi

# Launch the progress UI as the console user.
if [ -n "\$CONSOLE_UID" ]; then
    /bin/launchctl asuser "\$CONSOLE_UID" /usr/bin/sudo -u "\$CONSOLE_USER" /usr/bin/open "\$HELPER_APP" >/dev/null 2>&1 || true
fi

# Install Firefox if needed and write system-level policy files. This never
# modifies Firefox.app's Contents directory.
"\$SCRIPTS_DIR/sensiblefox" \\
    --policied \\
    --system \\
    --system-only \\
    --unattended \\
    --status-file "\$STATUS"

if [ -z "\$CONSOLE_USER" ] || [ -z "\$CONSOLE_UID" ]; then
    printf 'step=error\ntitle=SensibleFox\ndetail=No logged-in user was found to configure.\nprogress=0\ntotal=100\n' > "\$STATUS"
    exit 1
fi

# Build the actual Firefox profile as the console user so HOME, ownership, and
# profiles.ini all point at the person who installed the package.
/bin/launchctl asuser "\$CONSOLE_UID" \\
    /usr/bin/sudo -u "\$CONSOLE_USER" -H \\
    "\$SCRIPTS_DIR/sensiblefox" \\
        --app-dir /Applications \\
        --unattended \\
        --profile-only \\
        --status-file "\$STATUS"

printf 'step=done\ntitle=SensibleFox installed\ndetail=Firefox is ready to launch.\nprogress=100\ntotal=100\n' > "\$STATUS"

if [ -n "\$CONSOLE_UID" ]; then
    for _ in 1 2 3 4 5 6 7 8 9 10; do
        /usr/bin/pgrep -f "SensibleFox Installer" >/dev/null 2>&1 || break
        /bin/sleep 0.2
    done
    /bin/launchctl asuser "\$CONSOLE_UID" /usr/bin/sudo -u "\$CONSOLE_USER" \\
        /usr/bin/osascript -e 'tell application id "com.sensiblefox.installer" to quit' >/dev/null 2>&1 || true
fi

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

if [ -n "${DEVELOPER_ID_INSTALLER:-}" ]; then
    echo "  → Signing product .pkg..."
    productsign --sign "$DEVELOPER_ID_INSTALLER" "$DIST_DIR/SensibleFox.pkg" "$SIGNED_PKG" > /dev/null
    mv "$SIGNED_PKG" "$DIST_DIR/SensibleFox.pkg"
fi

if [ -n "${NOTARYTOOL_PROFILE:-}" ]; then
    if [ -z "${DEVELOPER_ID_INSTALLER:-}" ]; then
        echo "  ✗ NOTARYTOOL_PROFILE requires DEVELOPER_ID_INSTALLER so the pkg can be notarized."
        exit 1
    fi
    echo "  → Notarizing product .pkg..."
    xcrun notarytool submit "$DIST_DIR/SensibleFox.pkg" \
        --keychain-profile "$NOTARYTOOL_PROFILE" \
        --wait
    echo "  → Stapling notarization ticket..."
    xcrun stapler staple "$DIST_DIR/SensibleFox.pkg"
fi

rm -rf "$PKG_ROOT" "$SCRIPTS_DIR" "$RES_DIR" "$COMPONENT_PKG" "$DIST_XML"

SIZE=$(du -h "$DIST_DIR/SensibleFox.pkg" | cut -f1 | tr -d ' ')
echo ""
echo "  ✓ Built dist/SensibleFox.pkg ($SIZE) — Firefox $FF_VERSION (download ~${FF_SIZE_MB} MB, installs ~${FF_INSTALLED_MB} MB)"
echo "    Install: open dist/SensibleFox.pkg"
echo "    Or:      sudo installer -pkg dist/SensibleFox.pkg -target /"
if [ -z "${DEVELOPER_ID_INSTALLER:-}" ]; then
    echo "    Note:    This pkg is unsigned because DEVELOPER_ID_INSTALLER is not set."
    echo "             macOS may require right-click → Open for downloaded unsigned packages."
else
    echo "    Signed:  yes"
fi
