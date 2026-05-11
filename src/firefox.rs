use crate::progress::Progress;
use console::style;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const FIREFOX_DMG_URL: &str =
    "https://download.mozilla.org/?product=firefox-latest-ssl&os=osx&lang=en-US";
const FIREFOX_VERSION_URL: &str = "https://product-details.mozilla.org/1.0/firefox_versions.json";

const MAX_DOWNLOAD_RETRIES: u32 = 3;

/// Optional path to a Firefox DMG bundled inside the PKG. When this file
/// exists the installer skips the network download and reuses the bundled
/// copy. Populated by build-pkg.sh's all-in-one variant.
pub const BUNDLED_FIREFOX_DMG: &str = "/Library/Application Support/SensibleFox/bundles/Firefox.dmg";

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

pub fn install_step_list() -> Vec<crate::progress::Step> {
    use crate::progress::Step;
    vec![
        Step { title: "Detecting Firefox", weight: 2 },
        Step { title: "Downloading Firefox", weight: 35 },
        Step { title: "Installing Firefox", weight: 18 },
        Step { title: "Applying policies", weight: 5 },
        Step { title: "Creating profile", weight: 3 },
        Step { title: "Writing preferences", weight: 5 },
        Step { title: "Writing userChrome", weight: 3 },
        Step { title: "Installing uBlock Origin", weight: 25 },
        Step { title: "Registering default profile", weight: 4 },
    ]
}

// ── DMG mount RAII guard ──────────────────────────────────────────────

struct DmgMount {
    mount_point: PathBuf,
}

impl DmgMount {
    fn attach(dmg_path: &Path, mount_dir: &Path) -> Result<Self, String> {
        if let Some(stale) = find_firefox_mount() {
            let _ = Command::new("hdiutil")
                .args(["detach", "-force", "-quiet"])
                .arg(&stale)
                .status();
            std::thread::sleep(Duration::from_millis(500));
        }

        let output = Command::new("hdiutil")
            .args(["attach", "-nobrowse", "-quiet", "-mountpoint"])
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

        let app = mount_dir.join("Firefox.app");
        if !app.exists() {
            let _ = Command::new("hdiutil")
                .args(["detach", "-force", "-quiet"])
                .arg(mount_dir)
                .status();
            return Err("Mounted image did not contain Firefox.app".to_string());
        }

        Ok(DmgMount {
            mount_point: mount_dir.to_path_buf(),
        })
    }

    fn src_app(&self) -> String {
        self.mount_point
            .join("Firefox.app")
            .to_string_lossy()
            .into_owned()
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

// ── Public API ────────────────────────────────────────────────────────

/// Where to install Firefox.app.
#[derive(Clone, Debug, PartialEq)]
pub enum InstallTarget {
    /// ~/Applications — no elevation needed.
    User,
    /// /Applications — requires admin privileges.
    System,
    /// Custom path supplied by the user via --app-dir.
    Custom(PathBuf),
}

impl InstallTarget {
    pub fn app_path(&self) -> PathBuf {
        match self {
            InstallTarget::User => {
                let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
                home.join("Applications/Firefox.app")
            }
            InstallTarget::System => {
                // Honour the build-time override if set to a non-default value.
                let configured = env!("SENSIBLEFOX_FIREFOX_PATH");
                if configured != "/Applications/Firefox.app" && !configured.is_empty() {
                    PathBuf::from(configured)
                } else {
                    PathBuf::from("/Applications/Firefox.app")
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

/// Find or install Firefox, returning the path to the `firefox` binary.
pub fn detect_or_download(
    target: &InstallTarget,
    unattended: bool,
    replace_existing: bool,
    progress: &Progress,
) -> Result<PathBuf, String> {
    let app_path = target.app_path();
    let bin = target.bin_path();

    if app_path.exists() {
        let version = read_firefox_version(&app_path);
        let version_display = version.as_deref().unwrap_or("unknown version");

        if !progress.is_quiet() {
            println!(
                "  {} Firefox {} found at {}",
                style("✓").green(),
                version_display,
                style(app_path.display()).cyan()
            );
        }

        if !bin.exists() {
            if !progress.is_quiet() {
                println!(
                    "  {} Firefox.app is incomplete — reinstalling...",
                    style("!").yellow()
                );
            }
            replace_app(target, progress)?;
            return installed_bin(&bin);
        }

        match verify_app_signature(&app_path) {
            SignatureState::Valid => {}
            SignatureState::Invalid(reason) => {
                if !progress.is_quiet() {
                    eprintln!(
                        "  {} Firefox.app signature is invalid.",
                        style("!").yellow()
                    );
                    eprintln!(
                        "    {}",
                        reason
                            .lines()
                            .next()
                            .unwrap_or("codesign verification failed")
                    );
                }
                let repair = replace_existing
                    || unattended
                    || dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
                        .with_prompt("Repair Firefox with a fresh Mozilla-signed copy?")
                        .default(true)
                        .interact()
                        .unwrap_or(false);
                if repair {
                    replace_app(target, progress)?;
                    return installed_bin(&bin);
                }
            }
            SignatureState::Unknown(reason) => {
                if !progress.is_quiet() {
                    eprintln!(
                        "  {} Could not verify Firefox.app signature: {}",
                        style("!").yellow(),
                        reason
                    );
                }
            }
        }

        if replace_existing {
            replace_app(target, progress)?;
            return installed_bin(&bin);
        }

        return Ok(bin);
    }

    // A plain CLI run can reuse Firefox from a standard location. Explicit
    // installer targets should install exactly where the target says.
    if matches!(target, InstallTarget::User) {
        if let Some(p) = which_firefox() {
            if !progress.is_quiet() {
                println!("  {} Firefox found at {}", style("✓").green(), p.display());
            }
            return Ok(p);
        }
    }

    if !progress.is_quiet() {
        println!("  {} Firefox not found — downloading...", style("↓").cyan());
    }
    download_and_install(target, progress)?;
    installed_bin(&bin)
}

fn replace_app(target: &InstallTarget, progress: &Progress) -> Result<(), String> {
    let app_path = target.app_path();

    if !progress.is_quiet() {
        println!("  {} Installing fresh Firefox.app...", style("↻").cyan());
    }

    kill_firefox();
    remove_existing_app(&app_path)?;
    download_and_install(target, progress)
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
    .map_err(|e| format!("Failed to remove existing {}: {e}", app_path.display()))
}

fn installed_bin(bin: &Path) -> Result<PathBuf, String> {
    if bin.exists() {
        Ok(bin.to_path_buf())
    } else {
        Err("Firefox binary not found after install".to_string())
    }
}

// ── Download + install ────────────────────────────────────────────────

fn download_and_install(target: &InstallTarget, progress: &Progress) -> Result<(), String> {
    let steps = step_indexes();
    let tmp_dir = tempfile::tempdir().map_err(|e| format!("failed to create temp dir: {e}"))?;

    let bundled = Path::new(BUNDLED_FIREFOX_DMG);
    let dmg_path = if bundled.exists() {
        progress.step(steps.download, "Using bundled Firefox disk image");
        bundled.to_path_buf()
    } else {
        let dmg_path = tmp_dir.path().join("Firefox.dmg");
        progress.step(steps.download, "Starting download...");
        download_dmg_with_retry(&dmg_path, progress)?;
        dmg_path
    };

    progress.step(steps.copy, "Mounting the downloaded disk image...");
    progress.indeterminate("Mounting Firefox disk image...");

    let mount_dir = tmp_dir.path().join("firefox-mount");
    std::fs::create_dir(&mount_dir)
        .map_err(|e| format!("failed to create DMG mount directory: {e}"))?;

    let mount = DmgMount::attach(&dmg_path, &mount_dir)?;
    let src_app = mount.src_app();

    copy_app(&src_app, target, progress)?;
    Ok(())
}

fn download_dmg_with_retry(dmg_path: &Path, progress: &Progress) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let firefox_version = fetch_latest_firefox_version(&client).unwrap_or_else(|| "latest".into());
    let mut last_err = String::new();

    for attempt in 1..=MAX_DOWNLOAD_RETRIES {
        if attempt > 1 {
            let backoff = Duration::from_secs(2u64.pow(attempt - 1));
            if !progress.is_quiet() {
                eprintln!(
                    "  {} Retrying download (attempt {}/{})…",
                    style("!").yellow(),
                    attempt,
                    MAX_DOWNLOAD_RETRIES
                );
            }
            std::thread::sleep(backoff);
        }

        match download_dmg(&client, dmg_path, progress, &firefox_version) {
            Ok(()) => return Ok(()),
            Err(e) => last_err = e,
        }
    }

    progress.fail("SensibleFox", "Failed to download Firefox. Check connection.");
    Err(format!(
        "Download failed after {MAX_DOWNLOAD_RETRIES} attempts: {last_err}"
    ))
}

fn download_dmg(
    client: &reqwest::blocking::Client,
    dmg_path: &Path,
    progress: &Progress,
    firefox_version: &str,
) -> Result<(), String> {
    let response = client
        .get(FIREFOX_DMG_URL)
        .send()
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }

    let total_bytes = response.content_length().unwrap_or(0);
    let label = if firefox_version == "latest" {
        "Downloading Firefox".to_string()
    } else {
        format!("Downloading Firefox {firefox_version}")
    };

    let mut reader = response;
    let mut file =
        std::fs::File::create(dmg_path).map_err(|e| format!("failed to create DMG file: {e}"))?;
    let mut downloaded: u64 = 0;
    let mut buf = [0u8; 65536];

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("download stream read error: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("failed to write DMG chunk: {e}"))?;
        downloaded += n as u64;

        if total_bytes > 0 {
            let detail = format!(
                "{} — {} MB of {} MB",
                label,
                bytes_to_mb(downloaded),
                bytes_to_mb(total_bytes)
            );
            let frac = downloaded as f64 / total_bytes as f64;
            progress.sub(frac, &detail);
        } else {
            progress.indeterminate(&format!("{} — {} MB", label, bytes_to_mb(downloaded)));
        }
    }
    drop(file);
    progress.sub(1.0, "Firefox download complete");
    Ok(())
}

fn copy_app(src_app: &str, target: &InstallTarget, progress: &Progress) -> Result<(), String> {
    let dest = target.app_path();

    if dest.exists() {
        remove_existing_app(&dest).ok();
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }

    let expected_bytes = du_bytes(&PathBuf::from(src_app));

    let needs_elevation = matches!(target, InstallTarget::System) && !is_root();
    if needs_elevation && !gui_session_available() {
        return Err("Cannot install to /Applications in a non-GUI session.\n\
             Run with --app-dir ~/Applications, or run: sudo sensiblefox"
            .into());
    }

    progress.sub(0.0, &format!("Copying Firefox.app to {}…", target.display_name()));

    let mut child = if needs_elevation {
        let script = format!(
            "do shell script \"ditto '{0}' '{1}'\" with administrator privileges",
            src_app.replace('\'', "'\\''"),
            dest.to_string_lossy().replace('\'', "'\\''")
        );
        Command::new("osascript")
            .args(["-e", &script])
            .spawn()
            .map_err(|e| format!("failed to launch osascript: {e}"))?
    } else {
        Command::new("ditto")
            .arg(src_app)
            .arg(&dest)
            .spawn()
            .map_err(|e| format!("failed to launch ditto: {e}"))?
    };

    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cp_result = std::thread::scope(|scope| {
        let poll_dest = dest.clone();
        let poll_done = done.clone();
        scope.spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            while !poll_done.load(std::sync::atomic::Ordering::Relaxed) {
                if poll_dest.exists() {
                    let copied = du_bytes(&poll_dest);
                    if expected_bytes > 0 {
                        let frac = (copied as f64 / expected_bytes as f64).min(0.99);
                        progress.sub(
                            frac,
                            &format!(
                                "Copying Firefox — {} / {} MB to {}",
                                bytes_to_mb(copied),
                                bytes_to_mb(expected_bytes),
                                destination_label(&poll_dest)
                            ),
                        );
                    }
                }
                std::thread::sleep(Duration::from_millis(300));
            }
        });
        let r = child.wait();
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        r
    });

    let success = cp_result.map(|s| s.success()).unwrap_or(false);
    if !success || !dest.join("Contents/MacOS/firefox").exists() {
        progress.fail("SensibleFox", "Failed to copy Firefox to /Applications.");
        if needs_elevation {
            return Err("Failed to copy Firefox.app to /Applications.\n\
                 The admin authorization may have been cancelled, or the copy failed.\n\
                 Try installing to ~/Applications instead, or install manually:\n\
                 brew install --cask firefox"
                .to_string());
        }
        return Err(
            "Failed to copy Firefox.app. Install manually: brew install --cask firefox".to_string(),
        );
    }

    progress.sub(1.0, "Verifying Firefox signature...");

    match verify_app_signature(&dest) {
        SignatureState::Valid => {}
        SignatureState::Invalid(reason) => {
            return Err(format!(
                "Firefox.app copied, but its code signature is invalid: {}",
                reason
                    .lines()
                    .next()
                    .unwrap_or("codesign verification failed")
            ));
        }
        SignatureState::Unknown(reason) => {
            if !progress.is_quiet() {
                eprintln!(
                    "  {} Could not verify copied Firefox.app signature: {}",
                    style("!").yellow(),
                    reason
                );
            }
        }
    }

    progress.sub(1.0, "Running Gatekeeper assessment...");

    match assess_gatekeeper(&dest) {
        SignatureState::Valid => {}
        SignatureState::Invalid(reason) => {
            return Err(format!(
                "Firefox.app copied, but Gatekeeper rejected it: {}",
                reason.lines().next().unwrap_or("spctl assessment failed")
            ));
        }
        SignatureState::Unknown(reason) => {
            if !progress.is_quiet() {
                eprintln!(
                    "  {} Could not run Gatekeeper assessment for Firefox.app: {}",
                    style("!").yellow(),
                    reason
                );
            }
        }
    }

    Ok(())
}

fn is_root() -> bool {
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<u32>()
                .ok()
        })
        .map(|uid| uid == 0)
        .unwrap_or(false)
}

// ── Helpers ────────────────────────────────────────────────────────────

enum SignatureState {
    Valid,
    Invalid(String),
    Unknown(String),
}

fn verify_app_signature(app_path: &Path) -> SignatureState {
    let output = Command::new("codesign")
        .args(["--verify", "--deep", "--strict", "--verbose=2"])
        .arg(app_path)
        .output();

    match output {
        Ok(result) if result.status.success() => SignatureState::Valid,
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&result.stdout).trim().to_string();
            let reason = if stderr.is_empty() { stdout } else { stderr };
            SignatureState::Invalid(if reason.is_empty() {
                "codesign verification failed".to_string()
            } else {
                reason
            })
        }
        Err(e) => SignatureState::Unknown(e.to_string()),
    }
}

fn assess_gatekeeper(app_path: &Path) -> SignatureState {
    let output = Command::new("spctl")
        .args(["--assess", "--type", "execute", "--verbose=2"])
        .arg(app_path)
        .output();

    match output {
        Ok(result) if result.status.success() => SignatureState::Valid,
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&result.stdout).trim().to_string();
            let reason = if stderr.is_empty() { stdout } else { stderr };
            SignatureState::Invalid(if reason.is_empty() {
                "spctl assessment failed".to_string()
            } else {
                reason
            })
        }
        Err(e) => SignatureState::Unknown(e.to_string()),
    }
}

fn fetch_latest_firefox_version(client: &reqwest::blocking::Client) -> Option<String> {
    let text = client.get(FIREFOX_VERSION_URL).send().ok()?.text().ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;

    value
        .get("LATEST_FIREFOX_VERSION")
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .map(|v| v.to_string())
}

fn read_firefox_version(app_path: &Path) -> Option<String> {
    let plist = app_path.join("Contents/Info.plist");
    let output = Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", "Print :CFBundleShortVersionString"])
        .arg(&plist)
        .output()
        .ok()?;
    if output.status.success() {
        let v = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if v.is_empty() {
            None
        } else {
            Some(v)
        }
    } else {
        None
    }
}

fn bytes_to_mb(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

fn destination_label(dest: &Path) -> String {
    dest.parent()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| dest.display().to_string())
}

fn kill_firefox() {
    // Send SIGTERM to all Firefox-family processes.
    for name in &["firefox", "Firefox", "firefox-bin", "plugin-container"] {
        Command::new("pkill").args(["-x", name]).status().ok();
    }

    // Wait up to 10 s for processes to actually die.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        let any_alive = ["firefox", "Firefox"].iter().any(|name| {
            Command::new("pgrep")
                .args(["-x", name])
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        });
        if !any_alive {
            return;
        }
        std::thread::sleep(Duration::from_millis(250));
    }

    // Fallback: send SIGKILL if it's stubbornly refusing to quit
    for name in &["firefox", "Firefox", "firefox-bin", "plugin-container"] {
        Command::new("pkill").args(["-9", "-x", name]).status().ok();
    }
}

fn du_bytes(path: &PathBuf) -> u64 {
    let output = Command::new("du").args(["-sk"]).arg(path).output().ok();
    match output {
        Some(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            // `du -sk` output is "<kb>\t<path>". Take the first whitespace-delimited token.
            s.split_whitespace()
                .next()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0)
                * 1024
        }
        _ => 0,
    }
}

fn find_firefox_mount() -> Option<String> {
    // Use hdiutil info -plist for reliable parsing (instead of globbing /Volumes).
    let output = Command::new("hdiutil")
        .args(["info", "-plist"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // hdiutil info -plist emits an XML plist. Extract the first mount-point
    // whose image-path contains "Firefox".  Simple string scan is fine here;
    // we only need to find a single known volume.
    let mut in_firefox_block = false;
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.contains("Firefox") && trimmed.contains("image-path") {
            in_firefox_block = true;
        }
        if in_firefox_block && trimmed.starts_with("<key>mount-point</key>") {
            // The next <string>…</string> line holds the path.
            continue;
        }
        if in_firefox_block && trimmed.starts_with("<string>") && trimmed.ends_with("</string>") {
            let path = &trimmed["<string>".len()..trimmed.len() - "</string>".len()];
            let path = path.trim();
            if path.starts_with("/Volumes/") {
                return Some(path.to_string());
            }
        }
        if in_firefox_block && trimmed == "</dict>" {
            in_firefox_block = false;
        }
    }

    // Fallback: glob /Volumes/Firefox*
    let fallback = Command::new("bash")
        .args(["-c", "ls -d /Volumes/Firefox* 2>/dev/null | head -1"])
        .output()
        .ok()?;
    let mount = String::from_utf8_lossy(&fallback.stdout).trim().to_string();
    if mount.is_empty() {
        None
    } else {
        Some(mount)
    }
}

fn which_firefox() -> Option<PathBuf> {
    // 1 - Common macOS install locations
    let common_paths = [
        "/Applications/Firefox.app/Contents/MacOS/firefox",
        "~/Applications/Firefox.app/Contents/MacOS/firefox",
    ];

    for path in common_paths {
        let expanded = if path.starts_with("~/") {
            dirs::home_dir().map(|h| h.join(&path[2..]))
        } else {
            Some(PathBuf::from(path))
        };

        if let Some(p) = expanded {
            if p.exists() {
                return Some(p);
            }
        }
    }

    // 2 - Fallback to PATH
    Command::new("which")
        .arg("firefox")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if !s.is_empty() {
                    Some(PathBuf::from(s))
                } else {
                    None
                }
            } else {
                None
            }
        })
}

fn gui_session_available() -> bool {
    // If there's a window server connection, we can show the auth dialog.
    // SECURITYSESSIONID is set by the OS for GUI (Aqua) sessions.
    std::env::var("SECURITYSESSIONID").is_ok()
        && std::env::var("SSH_TTY").is_err()
        && std::env::var("TERM").unwrap_or_default() != "dumb"
}
