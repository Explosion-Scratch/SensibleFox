use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

const FIREFOX_APP: &str = env!("SENSIBLEFOX_FIREFOX_PATH");
const FIREFOX_BIN: &str = concat!(env!("SENSIBLEFOX_FIREFOX_PATH"), "/Contents/MacOS/firefox");
const FIREFOX_DMG_URL: &str =
    "https://download.mozilla.org/?product=firefox-latest-ssl&os=osx&lang=en-US";

pub fn detect_or_download() -> PathBuf {
    let bin = PathBuf::from(FIREFOX_BIN);
    if bin.exists() {
        println!("  {} Firefox found", style("✓").green());
        return bin;
    }

    if let Some(p) = which_firefox() {
        println!("  {} Firefox found at {}", style("✓").green(), p.display());
        return p;
    }

    println!(
        "  {} Firefox not found — downloading...",
        style("↓").cyan()
    );
    download_and_install();

    if bin.exists() {
        println!("  {} Firefox installed", style("✓").green());
        bin
    } else {
        eprintln!(
            "  {} Firefox installation failed. Install manually:",
            style("✗").red().bold()
        );
        eprintln!("    brew install --cask firefox");
        std::process::exit(1);
    }
}

fn download_and_install() {
    let tmp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let dmg_path = tmp_dir.path().join("Firefox.dmg");

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("  {spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message("Downloading Firefox...");
    pb.enable_steady_tick(Duration::from_millis(80));

    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .expect("failed to build HTTP client")
        .get(FIREFOX_DMG_URL)
        .send()
        .expect("failed to download Firefox");

    let bytes = response.bytes().expect("failed to read Firefox DMG");
    std::fs::write(&dmg_path, &bytes).expect("failed to write Firefox DMG");

    pb.set_message(format!(
        "Downloaded ({} MB) — mounting...",
        bytes.len() / (1024 * 1024)
    ));

    let mount_output = Command::new("hdiutil")
        .args(["attach", "-nobrowse", "-quiet"])
        .arg(&dmg_path)
        .output()
        .expect("failed to mount DMG");

    if !mount_output.status.success() {
        pb.finish_and_clear();
        eprintln!(
            "  {} Failed to mount Firefox DMG",
            style("✗").red().bold()
        );
        eprintln!(
            "    {}",
            String::from_utf8_lossy(&mount_output.stderr).trim()
        );
        std::process::exit(1);
    }

    let mount_point = find_firefox_mount();

    pb.set_message("Copying Firefox.app to /Applications...");

    let cp_status = Command::new("cp")
        .args(["-R"])
        .arg(format!("{}/Firefox.app", mount_point))
        .arg(FIREFOX_APP)
        .status()
        .expect("failed to copy Firefox.app");

    Command::new("hdiutil")
        .args(["detach", "-quiet"])
        .arg(&mount_point)
        .status()
        .ok();

    pb.finish_and_clear();

    if !cp_status.success() {
        eprintln!(
            "  {} Failed to copy Firefox.app to /Applications",
            style("✗").red().bold()
        );
        eprintln!("    You may need to run with sudo or install manually");
        std::process::exit(1);
    }
}

fn find_firefox_mount() -> String {
    let output = Command::new("bash")
        .args(["-c", "ls -d /Volumes/Firefox* 2>/dev/null | head -1"])
        .output()
        .expect("failed to find Firefox mount");
    let mount = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if mount.is_empty() {
        eprintln!(
            "  {} Could not find Firefox mount point",
            style("✗").red().bold()
        );
        std::process::exit(1);
    }
    mount
}

fn which_firefox() -> Option<PathBuf> {
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
