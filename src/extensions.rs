use crate::paths::{self, BUNDLED_UBLOCK_XPI, UBLOCK_ID, UBLOCK_XPI_URL};
use crate::progress::Progress;
use console::style;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

const MAX_FETCH_RETRIES: u32 = 3;

const UBLOCK_MANAGED_STORAGE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/uBlock0@raymondhill.net.json"
));

pub fn install_ublock(profile_path: &Path, progress: &Progress) {
    match install_ublock_xpi(profile_path, progress) {
        Ok(bytes) => {
            write_ublock_managed_storage();
            if !progress.is_quiet() {
                println!(
                    "  {} uBlock Origin installed ({} KB)",
                    style("✓").green(),
                    bytes / 1024
                );
            }
        }
        Err(e) => {
            if !progress.is_quiet() {
                eprintln!(
                    "  {} Failed to install uBlock Origin: {}",
                    style("✗").red(),
                    e
                );
                eprintln!("    Firefox will still work; you can install uBlock manually.");
            }
        }
    }
}

fn install_ublock_xpi(profile_path: &Path, progress: &Progress) -> Result<usize, String> {
    let ext_dir = profile_path.join("extensions");
    fs::create_dir_all(&ext_dir).map_err(|e| format!("failed to create extensions dir: {e}"))?;
    let xpi_path = ext_dir.join(format!("{UBLOCK_ID}.xpi"));

    let bundled = Path::new(BUNDLED_UBLOCK_XPI);
    if bundled.exists() {
        progress.sub(0.0, "Copying bundled uBlock Origin...");
        fs::copy(bundled, &xpi_path)
            .map_err(|e| format!("failed to copy bundled uBlock XPI: {e}"))?;
        let size = fs::metadata(&xpi_path).map(|m| m.len()).unwrap_or(0) as usize;
        progress.sub(1.0, "uBlock Origin installed");
        return Ok(size);
    }

    download_ublock_with_retry(&xpi_path, progress)
}

fn download_ublock_with_retry(xpi_path: &Path, progress: &Progress) -> Result<usize, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let mut last_err = String::new();

    for attempt in 1..=MAX_FETCH_RETRIES {
        if attempt > 1 {
            let backoff = Duration::from_secs(2u64.pow(attempt - 1));
            if !progress.is_quiet() {
                eprintln!(
                    "  {} Retrying uBlock download (attempt {}/{})…",
                    style("!").yellow(),
                    attempt,
                    MAX_FETCH_RETRIES
                );
            }
            std::thread::sleep(backoff);
        }

        match download_ublock(&client, xpi_path, progress) {
            Ok(n) => return Ok(n),
            Err(e) => last_err = e,
        }
    }

    Err(format!(
        "download failed after {MAX_FETCH_RETRIES} attempts: {last_err}"
    ))
}

fn mb(n: u64) -> u64 {
    n / (1024 * 1024)
}

fn download_ublock(
    client: &reqwest::blocking::Client,
    xpi_path: &Path,
    progress: &Progress,
) -> Result<usize, String> {
    let response = client
        .get(UBLOCK_XPI_URL)
        .send()
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }

    let total_bytes = response.content_length().unwrap_or(0);

    let mut file =
        fs::File::create(xpi_path).map_err(|e| format!("failed to create uBlock XPI: {e}"))?;

    let mut reader = response;
    let mut downloaded: u64 = 0;
    let mut buf = [0u8; 65536];

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("uBlock download read error: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("failed to write uBlock chunk: {e}"))?;
        downloaded += n as u64;

        if total_bytes > 0 {
            let frac = downloaded as f64 / total_bytes as f64;
            progress.sub(
                frac,
                &format!("Downloading uBlock — {} MB of {} MB", mb(downloaded), mb(total_bytes).max(1)),
            );
        } else {
            progress.indeterminate(&format!("Downloading uBlock — {} MB", mb(downloaded)));
        }
    }
    drop(file);
    Ok(downloaded as usize)
}

pub fn write_ublock_managed_storage() {
    let Some(home) = crate::profile::user_home() else {
        eprintln!(
            "  {} Could not determine home directory — skipping uBO managed storage",
            style("!").yellow()
        );
        return;
    };
    let dir = home.join(paths::MANAGED_STORAGE_DIR_REL);
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
    }
}

