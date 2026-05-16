//! Single source of truth for every path, URL, and bundle id SensibleFox
//! touches. Other modules MUST import from here.

use std::path::{Path, PathBuf};

// ── System (root-owned) paths ─────────────────────────────────────────────
pub const SYSTEM_FIREFOX_APP: &str = "/Applications/Firefox.app";
pub const SYSTEM_POLICY_PLIST: &str = "/Library/Preferences/org.mozilla.firefox.plist";
pub const SYSTEM_MANAGED_STORAGE: &str =
    "/Library/Application Support/Mozilla/ManagedStorage/uBlock0@raymondhill.net.json";
pub const SUPPORT_DIR: &str = "/Library/Application Support/SensibleFox";

/// uBlock Origin XPI optionally pre-staged by the offline PKG.
pub const BUNDLED_UBLOCK_XPI: &str =
    "/Library/Application Support/SensibleFox/bundles/uBlock0@raymondhill.net.xpi";

// ── Home-relative paths ──────────────────────────────────────────────────
pub const USER_APP_REL: &str = "Applications/Firefox.app";
pub const USER_POLICY_REL: &str = "Library/Preferences/org.mozilla.firefox.plist";
pub const USER_MANAGED_REL: &str =
    "Library/Application Support/Mozilla/ManagedStorage/uBlock0@raymondhill.net.json";
pub const FIREFOX_ROOT_REL: &str = "Library/Application Support/Firefox";
pub const MANAGED_STORAGE_DIR_REL: &str = "Library/Application Support/Mozilla/ManagedStorage";

// ── Network endpoints ────────────────────────────────────────────────────
pub const FIREFOX_DMG_URL: &str =
    "https://download.mozilla.org/?product=firefox-latest-ssl&os=osx&lang=en-US";
pub const FIREFOX_VERSION_URL: &str =
    "https://product-details.mozilla.org/1.0/firefox_versions.json";
pub const UBLOCK_XPI_URL: &str = concat!(
    "https://addons.mozilla.org/firefox/downloads/latest/",
    "ublock-origin/platform:3/ublock-origin.xpi"
);

// ── IDs ──────────────────────────────────────────────────────────────────
pub const UBLOCK_ID: &str = "uBlock0@raymondhill.net";
pub const PROFILE_NAME: &str = "sensiblefox";

// ── Helpers ──────────────────────────────────────────────────────────────
pub fn join_home(home: &Path, rel: &str) -> PathBuf {
    home.join(rel)
}
