use crate::paths;
use crate::progress::Progress;
use console::style;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const MAX_DOWNLOAD_RETRIES: u32 = 3;

/// Where Firefox.app lives if the offline PKG pre-staged it.
const BUNDLED_FIREFOX_APP: &str = "/Library/Application Support/SensibleFox/bundles/Firefox.app";
/// Legacy bundled-DMG location — still respected for backwards compat.
const BUNDLED_FIREFOX_DMG: &str = "/Library/Application Support/SensibleFox/bundles/Firefox.dmg";

// ── Step table (the only place install steps are defined) ─────────────

pub struct InstallSteps {
    pub detect: usize,
    pub download: usize,
    pub copy: usize,
    pub policies: usize,
    pub profile: usize,
    pub prefs: usize,
    pub chrome: usize,
    pub ublock: usize,
    pub register: usize,
}

pub fn step_indexes() -> InstallSteps {
    InstallSteps {
        detect: 0,
        download: 1,
        copy: 2,
        policies: 3,
        profile: 4,
        prefs: 5,
        chrome: 6,
        ublock: 7,
        register: 8,
    }
}

pub fn install_step_list() -> Vec<crate::progress::Step> {
    use crate::progress::Step;
    vec![
        Step { title: "Detecting Firefox", weight: 2 },
        Step { title: "Downloading Firefox", weight: 35 },
        Step { title: "Installing Firefox", weight: 12 },
        Step { title: "Applying policies", weight: 5 },
        Step { title: "Creating profile", weight: 3 },
        Step { title: "Writing preferences", weight: 5 },
        Step { title: "Writing userChrome", weight: 3 },
        Step { title: "Installing uBlock Origin", weight: 25 },
        Step { title: "Registering default profile", weight: 4 },
    ]
}

// ── Install target ────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum InstallTarget {
    /// `~/Applications` — no elevation needed.
    User,
    /// `/Applications` — requires root.
    System,
    /// Custom path supplied via `--app-dir`.
    Custom(PathBuf),
}

impl InstallTarget {
    pub fn app_path(&self) -> PathBuf {
        match self {
            InstallTarget::User => crate::profile::user_home()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join(paths::USER_APP_REL),
            InstallTarget::System => {
                let configured = env!("SENSIBLEFOX_FIREFOX_PATH");
                if !configured.is_empty() && configured != paths::SYSTEM_FIREFOX_APP {
                    PathBuf::from(configured)
                } else {
                    PathBuf::from(paths::SYSTEM_FIREFOX_APP)
                }
            }
            InstallTarget::Custom(p) => p.clone(),
        }
    }

    pub fn bin_path(&self) -> PathBuf {
        self.app_path().join("Contents/MacOS/firefox")
    }

    fn display_name(&self) -> &str {
        match self {
            InstallTarget::User => "~/Applications",
            InstallTarget::System => "/Applications",
            InstallTarget::Custom(_) => "the chosen directory",
        }
    }
}

// ── Public API ────────────────────────────────────────────────────────

/// Find or install Firefox, returning the path to the `firefox` binary.
/// Skips signature verification — the binary's existence + a quick test
/// launch is enough; deep `codesign --verify --deep` was the single
/// slowest part of every CLI run (multi-second on every invocation).
///
/// Reuse fast path verifies ESR freshness: SensibleFox tracks latest ESR
/// (unsandboxed autoconfig only works on ESR-class channels), so a stale
/// or rapid-channel install falls through to replacement. Unknowns
/// (offline, unparseable) fail OPEN — reuse, never block launch.
pub fn detect_or_download(
    target: &InstallTarget,
    _unattended: bool,
    replace_existing: bool,
    progress: &Progress,
) -> Result<PathBuf, String> {
    let bin = target.bin_path();
    let app_path = target.app_path();
    // Short-timeout client for the ESR freshness check only (fail-open).
    let check_client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .ok();

    if app_path.exists() && bin.exists() && !replace_existing {
        match esr_status(&app_path, check_client.as_ref()) {
            EsrStatus::Stale { installed, latest } => {
                if !progress.is_quiet() {
                    println!(
                        "  {} Firefox {} found, but latest ESR is {} — updating…",
                        style("↻").cyan(),
                        installed,
                        latest
                    );
                }
                // fall through to replacement below
            }
            EsrStatus::Current(version) | EsrStatus::Unknown(version) => {
                if !progress.is_quiet() {
                    println!(
                        "  {} Firefox {} found at {}",
                        style("✓").green(),
                        version,
                        style(app_path.display()).cyan()
                    );
                }
                return Ok(bin);
            }
        }
    }

    if app_path.exists() {
        if !progress.is_quiet() {
            println!("  {} Replacing Firefox at {}", style("↻").cyan(), app_path.display());
        }
        kill_firefox();
        let _ = remove_existing_app(&app_path);
    }

    // A plain CLI run can reuse Firefox from a standard location instead of
    // installing into ~/Applications — but only if it's current ESR.
    if matches!(target, InstallTarget::User) {
        if let Some(p) = which_firefox() {
            let current = app_bundle_from_bin(&p)
                .map(|app| {
                    matches!(
                        esr_status(&app, check_client.as_ref()),
                        EsrStatus::Current(_) | EsrStatus::Unknown(_)
                    )
                })
                .unwrap_or(true);
            if current {
                if !progress.is_quiet() {
                    println!("  {} Firefox found at {}", style("✓").green(), p.display());
                }
                return Ok(p);
            } else if !progress.is_quiet() {
                println!(
                    "  {} Existing Firefox at {} is not current ESR — installing fresh…",
                    style("↻").cyan(),
                    p.display()
                );
            }
        }
    }

    install(target, progress)?;
    if !bin.exists() {
        return Err("Firefox binary not found after install".into());
    }
    Ok(bin)
}

// ── Install pipeline ──────────────────────────────────────────────────

fn install(target: &InstallTarget, progress: &Progress) -> Result<(), String> {
    let steps = step_indexes();

    // Fast path: PKG payload pre-staged Firefox.app directly. Just move it
    // into place (or it's already at /Applications — nothing to do).
    if Path::new(BUNDLED_FIREFOX_APP).exists() {
        progress.step(steps.download, "Using bundled Firefox.app");
        return copy_app(Path::new(BUNDLED_FIREFOX_APP), target, progress);
    }

    // Bundled DMG path (legacy): mount and copy.
    let tmp_dir = tempfile::tempdir().map_err(|e| format!("tempdir failed: {e}"))?;
    let dmg_path = if Path::new(BUNDLED_FIREFOX_DMG).exists() {
        progress.step(steps.download, "Using bundled Firefox disk image");
        PathBuf::from(BUNDLED_FIREFOX_DMG)
    } else {
        let dmg_path = tmp_dir.path().join("Firefox.dmg");
        progress.step(steps.download, "Starting download...");
        download_dmg_with_retry(&dmg_path, progress)?;
        dmg_path
    };

    progress.step(steps.copy, "Mounting Firefox disk image...");
    progress.indeterminate("Mounting Firefox disk image...");
    let mount_dir = tmp_dir.path().join("mount");
    std::fs::create_dir(&mount_dir).map_err(|e| format!("mkdir mount: {e}"))?;
    let mount = DmgMount::attach(&dmg_path, &mount_dir)?;
    copy_app(Path::new(&mount.src_app()), target, progress)
}

fn copy_app(src_app: &Path, target: &InstallTarget, progress: &Progress) -> Result<(), String> {
    let dest = target.app_path();

    if dest.exists() {
        kill_firefox();
        let _ = remove_existing_app(&dest);
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }

    progress.indeterminate(&format!("Copying Firefox.app to {}…", target.display_name()));

    // `--noqtn` strips com.apple.quarantine so Firefox launches without
    // the "downloaded app" prompt. We trust Mozilla's signed source.
    let status = Command::new("ditto")
        .args(["--noqtn"])
        .arg(src_app)
        .arg(&dest)
        .status()
        .map_err(|e| format!("failed to launch ditto: {e}"))?;

    if !status.success() || !dest.join("Contents/MacOS/firefox").exists() {
        progress.fail("SensibleFox", "Failed to copy Firefox.app");
        return Err(format!(
            "ditto failed to copy {} -> {}. Install manually: brew install --cask firefox",
            src_app.display(),
            dest.display()
        ));
    }

    progress.sub(1.0, "Firefox installed");
    Ok(())
}

fn remove_existing_app(app_path: &Path) -> Result<(), String> {
    if !app_path.exists() {
        return Ok(());
    }
    if app_path.is_dir() {
        std::fs::remove_dir_all(app_path)
    } else {
        std::fs::remove_file(app_path)
    }
    .map_err(|e| format!("Failed to remove {}: {e}", app_path.display()))
}

// ── DMG mount RAII guard ──────────────────────────────────────────────

struct DmgMount {
    mount_point: PathBuf,
}

impl DmgMount {
    fn attach(dmg_path: &Path, mount_dir: &Path) -> Result<Self, String> {
        // `-noverify` skips checksumming the DMG (we either just downloaded
        // it and the HTTPS layer already authenticated, or it's a trusted
        // PKG-bundled copy). Saves several seconds on a 180 MB DMG.
        let output = Command::new("hdiutil")
            .args(["attach", "-nobrowse", "-noverify", "-quiet", "-mountpoint"])
            .arg(mount_dir)
            .arg(dmg_path)
            .output()
            .map_err(|e| format!("failed to run hdiutil: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "hdiutil attach failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        if !mount_dir.join("Firefox.app").exists() {
            let _ = Command::new("hdiutil")
                .args(["detach", "-force", "-quiet"])
                .arg(mount_dir)
                .status();
            return Err("Mounted image did not contain Firefox.app".into());
        }

        Ok(DmgMount { mount_point: mount_dir.to_path_buf() })
    }

    fn src_app(&self) -> String {
        self.mount_point.join("Firefox.app").to_string_lossy().into_owned()
    }
}

impl Drop for DmgMount {
    fn drop(&mut self) {
        let _ = Command::new("hdiutil")
            .args(["detach", "-quiet"])
            .arg(&self.mount_point)
            .status();
    }
}

// ── Download ──────────────────────────────────────────────────────────

fn download_dmg_with_retry(dmg_path: &Path, progress: &Progress) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| format!("HTTP client: {e}"))?;

    let version = fetch_latest_firefox_version(&client).unwrap_or_else(|| "latest".into());
    let mut last_err = String::new();

    for attempt in 1..=MAX_DOWNLOAD_RETRIES {
        if attempt > 1 {
            std::thread::sleep(Duration::from_secs(2u64.pow(attempt - 1)));
            if !progress.is_quiet() {
                eprintln!(
                    "  {} Retrying download ({attempt}/{MAX_DOWNLOAD_RETRIES})…",
                    style("!").yellow()
                );
            }
        }
        match download_dmg(&client, dmg_path, progress, &version) {
            Ok(()) => return Ok(()),
            Err(e) => last_err = e,
        }
    }

    progress.fail("SensibleFox", "Failed to download Firefox. Check connection.");
    Err(format!("Download failed after {MAX_DOWNLOAD_RETRIES} attempts: {last_err}"))
}

fn download_dmg(
    client: &reqwest::blocking::Client,
    dmg_path: &Path,
    progress: &Progress,
    version: &str,
) -> Result<(), String> {
    let response = client
        .get(paths::FIREFOX_DMG_URL)
        .send()
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }

    let total = response.content_length().unwrap_or(0);
    let label = if version == "latest" {
        "Downloading Firefox".to_string()
    } else {
        format!("Downloading Firefox {version}")
    };

    let mut file = std::fs::File::create(dmg_path)
        .map_err(|e| format!("create DMG file: {e}"))?;
    let mut reader = response;
    let mut buf = [0u8; 256 * 1024];
    let mut downloaded: u64 = 0;
    let mut last_report = std::time::Instant::now();

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("download read: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("write DMG chunk: {e}"))?;
        downloaded += n as u64;

        // Throttle progress updates to ~10/s; far less stdio + file-write
        // churn than a report per 64 KB chunk.
        if last_report.elapsed() >= Duration::from_millis(100) || downloaded == total {
            last_report = std::time::Instant::now();
            if total > 0 {
                progress.sub(
                    downloaded as f64 / total as f64,
                    &format!("{label} — {} / {} MB", mb(downloaded), mb(total)),
                );
            } else {
                progress.indeterminate(&format!("{label} — {} MB", mb(downloaded)));
            }
        }
    }
    progress.sub(1.0, "Firefox download complete");
    // A truncated stream (connection dropped exactly at a chunk boundary)
    // reads as clean EOF. Without a length check we'd hand a corrupt DMG
    // to hdiutil and fail there with a confusing error — or worse, retry
    // logic never fires. Fail here so download_dmg_with_retry retries.
    if total > 0 && downloaded != total {
        return Err(format!(
            "incomplete download: got {downloaded} of {total} bytes"
        ));
    }
    Ok(())
}

// ── ESR freshness ─────────────────────────────────────────────────────
// SensibleFox tracks latest ESR: download.mozilla.org esr-latest-ssl always
// resolves to the newest ESR, and FIREFOX_ESR in product-details names it.
// Channel comes from Contents/Resources/application.ini SourceRepository
// (mozilla-esr* on ESR, mozilla-release on rapid) — the old channel-prefs.js
// no longer ships; the channel is baked into omni.ja instead.

/// Numeric components of a Firefox version; "153.5.0esr" -> [153, 5, 0].
fn normalize_version(v: &str) -> Vec<u64> {
    let t = v.trim();
    let t = t
        .strip_suffix("esr")
        .or_else(|| t.strip_suffix("ESR"))
        .unwrap_or(t);
    t.split('.')
        .map(|c| {
            c.chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>()
                .parse::<u64>()
                .unwrap_or(0)
        })
        .collect()
}

fn versions_equal(a: &str, b: &str) -> bool {
    let mut na = normalize_version(a);
    let mut nb = normalize_version(b);
    let len = na.len().max(nb.len());
    na.resize(len, 0);
    nb.resize(len, 0);
    na == nb
}

/// (version, source_repo) from the [App] section of application.ini.
fn read_application_ini(app_path: &Path) -> Option<(String, String)> {
    let text =
        std::fs::read_to_string(app_path.join("Contents/Resources/application.ini")).ok()?;
    let mut in_app = false;
    let mut version = None;
    let mut repo = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_app = line.eq_ignore_ascii_case("[App]");
            continue;
        }
        if !in_app {
            continue;
        }
        if let Some(v) = line.strip_prefix("Version=") {
            version = Some(v.trim().to_string());
        } else if let Some(r) = line.strip_prefix("SourceRepository=") {
            repo = Some(r.trim().to_string());
        }
    }
    Some((version?, repo?))
}

/// Pure decision: must this install be replaced to track latest ESR?
/// Wrong channel (even numerically newer rapid) always replaces; on ESR,
/// any version mismatch replaces.
fn needs_esr_replace(installed_version: &str, source_repo: &str, latest_esr: &str) -> bool {
    if !source_repo.to_lowercase().contains("esr") {
        return true;
    }
    !versions_equal(installed_version, latest_esr)
}

enum EsrStatus {
    /// Right channel, version matches latest ESR — reuse.
    Current(String),
    /// Wrong channel or older version — reinstall. Holds (installed, latest).
    Stale { installed: String, latest: String },
    /// Couldn't determine (offline, unparseable) — reuse, never block launch.
    Unknown(String),
}

fn esr_status(app_path: &Path, client: Option<&reqwest::blocking::Client>) -> EsrStatus {
    let installed = read_firefox_version(app_path).unwrap_or_else(|| "unknown version".into());
    let Some((ini_version, repo)) = read_application_ini(app_path) else {
        return EsrStatus::Unknown(installed);
    };
    let Some(c) = client else {
        return EsrStatus::Unknown(installed);
    };
    let Some(latest) = fetch_latest_firefox_version(c) else {
        return EsrStatus::Unknown(installed);
    };
    if needs_esr_replace(&ini_version, &repo, &latest) {
        EsrStatus::Stale { installed, latest }
    } else {
        EsrStatus::Current(installed)
    }
}

/// Map `…/Firefox.app/Contents/MacOS/firefox` back to `…/Firefox.app`.
fn app_bundle_from_bin(bin: &Path) -> Option<PathBuf> {
    let mut p = bin.to_path_buf();
    for tail in ["firefox", "MacOS", "Contents"] {
        if p.ends_with(tail) {
            p.pop();
        } else {
            return None;
        }
    }
    Some(p)
}

// ── Helpers ────────────────────────────────────────────────────────────

fn fetch_latest_firefox_version(client: &reqwest::blocking::Client) -> Option<String> {
    let text = client.get(paths::FIREFOX_VERSION_URL).send().ok()?.text().ok()?;
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()?
        .get("FIREFOX_ESR")?
        .as_str()
        .filter(|v| !v.trim().is_empty())
        .map(str::to_string)
}

fn read_firefox_version(app_path: &Path) -> Option<String> {
    let plist = app_path.join("Contents/Info.plist");
    let out = Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", "Print :CFBundleShortVersionString"])
        .arg(&plist)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

fn mb(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

fn kill_firefox() {
    // SIGTERM then SIGKILL after a short grace period. Don't bother polling
    // for clean exit — Firefox's profile lock releases as soon as the
    // process group dies, and ditto can overwrite a running app anyway.
    for sig in &["-TERM", "-KILL"] {
        for name in &["firefox", "Firefox", "firefox-bin", "plugin-container"] {
            let _ = Command::new("pkill").args([sig, "-x", name]).status();
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

fn which_firefox() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from(paths::SYSTEM_FIREFOX_APP).join("Contents/MacOS/firefox"),
        crate::profile::user_home()
            .map(|h| h.join(paths::USER_APP_REL).join("Contents/MacOS/firefox"))
            .unwrap_or_default(),
    ];
    candidates.into_iter().find(|p| p.exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_esr_versions() {
        assert_eq!(normalize_version("153.5.0esr"), vec![153, 5, 0]);
        assert_eq!(normalize_version("115.43.0esr"), vec![115, 43, 0]);
        assert_eq!(normalize_version("157.0.1"), vec![157, 0, 1]);
    }

    #[test]
    fn version_equality_ignores_trailing_zeros_and_esr_suffix() {
        assert!(versions_equal("153.5", "153.5.0esr"));
        assert!(versions_equal("153.5.0esr", "153.5.0"));
        assert!(!versions_equal("153.4.0", "153.5.0esr"));
        assert!(!versions_equal("157.0.1", "153.5.0esr"));
    }

    #[test]
    fn flags_wrong_channel_and_stale_versions() {
        // rapid channel, even numerically newer, must be replaced
        assert!(needs_esr_replace(
            "157.0.1",
            "https://hg.mozilla.org/releases/mozilla-release",
            "153.5.0esr"
        ));
        // stale ESR
        assert!(needs_esr_replace(
            "153.4.0",
            "https://hg.mozilla.org/releases/mozilla-esr153",
            "153.5.0esr"
        ));
        // current ESR, in 2- and 3-component forms
        assert!(!needs_esr_replace(
            "153.5",
            "https://hg.mozilla.org/releases/mozilla-esr153",
            "153.5.0esr"
        ));
        assert!(!needs_esr_replace(
            "153.5.0",
            "https://hg.mozilla.org/releases/mozilla-esr153",
            "153.5.0esr"
        ));
    }

    fn write_test_app(dir: &Path, version: &str, repo: &str) -> PathBuf {
        let app = dir.join("Firefox.app");
        std::fs::create_dir_all(app.join("Contents/Resources")).unwrap();
        std::fs::write(
            app.join("Contents/Resources/application.ini"),
            format!(
                "; fixture\n[App]\nVendor=Mozilla\nName=Firefox\nVersion={version}\n\
                 SourceRepository={repo}\n\n[Gecko]\nMinVersion={version}\nMaxVersion={version}\n"
            ),
        )
        .unwrap();
        app
    }

    #[test]
    fn parses_application_ini_app_section() {
        let dir = tempfile::tempdir().unwrap();
        let app = write_test_app(
            dir.path(),
            "153.5.0",
            "https://hg.mozilla.org/releases/mozilla-esr153",
        );
        assert_eq!(
            read_application_ini(&app),
            Some((
                "153.5.0".to_string(),
                "https://hg.mozilla.org/releases/mozilla-esr153".to_string()
            ))
        );
    }

    #[test]
    fn maps_bin_path_back_to_bundle() {
        assert_eq!(
            app_bundle_from_bin(Path::new("/Applications/Firefox.app/Contents/MacOS/firefox")),
            Some(PathBuf::from("/Applications/Firefox.app"))
        );
        assert_eq!(
            app_bundle_from_bin(Path::new("/usr/local/bin/firefox")),
            None
        );
    }
}

