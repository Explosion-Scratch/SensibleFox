use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

const UBLOCK_ID: &str = "uBlock0@raymondhill.net";
const UBLOCK_UUID: &str = "14d4bd8f-4d00-422a-92b2-ba06bd9deaa7";

const UBLOCK_XPI_URL: &str = concat!(
    "https://addons.mozilla.org/firefox/downloads/latest/",
    "ublock-origin/platform:3/ublock-origin.xpi"
);

const MAX_FETCH_RETRIES: u32 = 3;

const UBLOCK_MANAGED_STORAGE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/uBlock0@raymondhill.net.json"
));

const DEFAULT_EXTENSIONS: &[(&str, &str)] = &[
    (
        "formautofill@mozilla.org",
        "647ab118-a5e4-43c4-90e3-411d4e5155fe",
    ),
    (
        "pictureinpicture@mozilla.org",
        "882abc45-e743-4032-9766-4571e5dae35d",
    ),
    (
        "screenshots@mozilla.org",
        "11098328-de4a-4b76-a17e-b26039cae0cc",
    ),
    (
        "webcompat-reporter@mozilla.org",
        "6ec2ebcc-1aa1-4fa5-a339-68fec0201ea9",
    ),
    (
        "webcompat@mozilla.org",
        "0b52c378-083f-4fbf-98ee-1c1166674cc6",
    ),
    (
        "default-theme@mozilla.org",
        "35c104e7-5b2b-4d06-a440-5cfede7cc8dd",
    ),
    (
        "addons-search-detection@mozilla.com",
        "b138d06f-2e3c-4a31-8329-f03604fd5430",
    ),
];

pub fn install_ublock(profile_path: &Path, status_file: Option<&PathBuf>) {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("  {spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message("Downloading uBlock Origin...");
    pb.enable_steady_tick(std::time::Duration::from_millis(80));

    match download_ublock_with_retry(profile_path) {
        Ok(bytes) => {
            pb.finish_and_clear();
            write_extension_prefs(profile_path);
            write_ublock_managed_storage(status_file);
            if let Some(sf) = status_file {
                crate::firefox::write_status(
                    sf,
                    "configure",
                    "Installing uBlock Origin",
                    "uBlock Origin extension and managed storage are ready.",
                    88,
                    100,
                );
            }
            println!(
                "  {} uBlock Origin installed ({} KB)",
                style("✓").green(),
                bytes / 1024
            );
        }
        Err(e) => {
            pb.finish_and_clear();
            eprintln!(
                "  {} Failed to install uBlock Origin: {}",
                style("✗").red(),
                e
            );
            eprintln!("    Firefox will still work; you can install uBlock manually.");
        }
    }
}

fn download_ublock_with_retry(profile_path: &Path) -> Result<usize, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let mut last_err = String::new();

    for attempt in 1..=MAX_FETCH_RETRIES {
        if attempt > 1 {
            let backoff = Duration::from_secs(2u64.pow(attempt - 1));
            eprintln!(
                "  {} Retrying uBlock download (attempt {}/{})…",
                style("!").yellow(),
                attempt,
                MAX_FETCH_RETRIES
            );
            std::thread::sleep(backoff);
        }

        match download_ublock(&client, profile_path) {
            Ok(n) => return Ok(n),
            Err(e) => last_err = e,
        }
    }

    Err(format!(
        "download failed after {MAX_FETCH_RETRIES} attempts: {last_err}"
    ))
}

fn download_ublock(
    client: &reqwest::blocking::Client,
    profile_path: &Path,
) -> Result<usize, String> {
    let response = client
        .get(UBLOCK_XPI_URL)
        .send()
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }

    let bytes = response
        .bytes()
        .map_err(|e| format!("failed to read XPI body: {e}"))?;

    let ext_dir = profile_path.join("extensions");
    fs::create_dir_all(&ext_dir).map_err(|e| format!("failed to create extensions dir: {e}"))?;

    let size = bytes.len();
    let xpi_path = ext_dir.join(format!("{UBLOCK_ID}.xpi"));
    fs::write(&xpi_path, &bytes[..]).map_err(|e| format!("failed to write uBlock XPI: {e}"))?;

    Ok(size)
}

pub fn write_ublock_managed_storage(status_file: Option<&PathBuf>) {
    let Some(home) = dirs::home_dir() else {
        eprintln!(
            "  {} Could not determine home directory — skipping uBO managed storage",
            style("!").yellow()
        );
        return;
    };
    let dir = home.join("Library/Application Support/Mozilla/ManagedStorage");
    let path = dir.join(format!("{UBLOCK_ID}.json"));

    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!(
            "  {} Failed to create ManagedStorage dir: {e}",
            style("!").yellow()
        );
        return;
    }

    if let Err(e) = fs::write(&path, UBLOCK_MANAGED_STORAGE.as_bytes()) {
        eprintln!(
            "  {} Failed to write uBO managed storage: {e}",
            style("!").yellow()
        );
        return;
    }

    match fs::read_to_string(&path) {
        Ok(actual) if actual == UBLOCK_MANAGED_STORAGE => {}
        Ok(_) => {
            eprintln!(
                "  {} uBO managed storage verification failed: content mismatch",
                style("!").yellow()
            );
            return;
        }
        Err(e) => {
            eprintln!(
                "  {} uBO managed storage verification failed: {e}",
                style("!").yellow()
            );
            return;
        }
    }

    if let Some(sf) = status_file {
        crate::firefox::write_status(
            sf,
            "configure",
            "Installing uBlock Origin",
            "uBlock managed storage has been verified.",
            86,
            100,
        );
    }
}

fn write_extension_prefs(profile_path: &Path) {
    let user_js_path = profile_path.join("user.js");

    // Read existing user.js to check whether extension prefs are already present.
    let existing = fs::read_to_string(&user_js_path).unwrap_or_default();
    if existing.contains("\"extensions.webextensions.uuids\"") {
        // Already written — idempotent.
        return;
    }

    let mut file = match fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&user_js_path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "  {} Could not append extension prefs to user.js: {e}",
                style("!").yellow()
            );
            return;
        }
    };

    // Best-effort writes below — if any fail we just warn.
    let _ = writeln!(file, "\n// ═══════════════════════════════════════════");
    let _ = writeln!(file, "// EXTENSIONS");
    let _ = writeln!(file, "// ═══════════════════════════════════════════");

    let mut uuids: Vec<String> = DEFAULT_EXTENSIONS
        .iter()
        .map(|(id, uuid)| format!("\"{}\":\"{}\"", id, uuid))
        .collect();
    uuids.push(format!("\"{}\":\"{}\"", UBLOCK_ID, UBLOCK_UUID));
    let uuid_json = format!("{{{}}}", uuids.join(","));
    let uuid_pref_value =
        serde_json::to_string(&uuid_json).unwrap_or_else(|_| "\"{}\"".to_string());

    let _ = writeln!(
        file,
        "user_pref(\"extensions.webextensions.uuids\", {});",
        uuid_pref_value
    );
    let _ = writeln!(
        file,
        "user_pref(\"extensions.webextensions.ExtensionStorageIDB.migrated.{UBLOCK_ID}\", true);"
    );
}
