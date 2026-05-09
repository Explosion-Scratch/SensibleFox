use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::io::Write;
use std::path::Path;

const UBLOCK_ID: &str = "uBlock0@raymondhill.net";
const UBLOCK_SLUG: &str = "ublock-origin";
const UBLOCK_UUID: &str = "14d4bd8f-4d00-422a-92b2-ba06bd9deaa7";

const UBLOCK_MANAGED_STORAGE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/uBlock0@raymondhill.net.json"
));

const DEFAULT_EXTENSIONS: &[(&str, &str)] = &[
    ("formautofill@mozilla.org", "647ab118-a5e4-43c4-90e3-411d4e5155fe"),
    ("pictureinpicture@mozilla.org", "882abc45-e743-4032-9766-4571e5dae35d"),
    ("screenshots@mozilla.org", "11098328-de4a-4b76-a17e-b26039cae0cc"),
    ("webcompat-reporter@mozilla.org", "6ec2ebcc-1aa1-4fa5-a339-68fec0201ea9"),
    ("webcompat@mozilla.org", "0b52c378-083f-4fbf-98ee-1c1166674cc6"),
    ("default-theme@mozilla.org", "35c104e7-5b2b-4d06-a440-5cfede7cc8dd"),
    ("addons-search-detection@mozilla.com", "b138d06f-2e3c-4a31-8329-f03604fd5430"),
];

pub fn install_ublock(profile_path: &Path) {
    let url = format!(
        "https://addons.mozilla.org/firefox/downloads/latest/{UBLOCK_SLUG}/platform:3/{UBLOCK_SLUG}.xpi"
    );

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("  {spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message("Downloading uBlock Origin...");
    pb.enable_steady_tick(std::time::Duration::from_millis(80));

    let response = reqwest::blocking::get(&url).expect("failed to download uBlock Origin");
    let bytes = response.bytes().expect("failed to read uBlock Origin XPI");

    let ext_dir = profile_path.join("extensions");
    fs::create_dir_all(&ext_dir).expect("failed to create extensions directory");

    let xpi_path = ext_dir.join(format!("{UBLOCK_ID}.xpi"));
    fs::write(&xpi_path, &bytes).expect("failed to write uBlock Origin XPI");

    pb.finish_and_clear();

    write_extension_prefs(profile_path);
    write_ublock_managed_storage();

    println!(
        "  {} uBlock Origin installed ({} KB)",
        style("✓").green(),
        bytes.len() / 1024
    );
}

pub fn write_ublock_managed_storage() {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    let dir = home.join("Library/Application Support/Mozilla/ManagedStorage");
    let path = dir.join(format!("{UBLOCK_ID}.json"));
    fs::create_dir_all(&dir).expect("failed to create ManagedStorage directory");
    fs::write(&path, UBLOCK_MANAGED_STORAGE.as_bytes()).expect("failed to write uBO managed storage");
}

fn write_extension_prefs(profile_path: &Path) {
    let user_js_path = profile_path.join("user.js");
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&user_js_path)
        .expect("failed to open user.js for extension prefs");

    writeln!(file, "\n// ═══════════════════════════════════════════").unwrap();
    writeln!(file, "// EXTENSIONS").unwrap();
    writeln!(file, "// ═══════════════════════════════════════════").unwrap();

    let mut uuids: Vec<String> = DEFAULT_EXTENSIONS
        .iter()
        .map(|(id, uuid)| format!("\"{}\":\"{}\"", id, uuid))
        .collect();
    uuids.push(format!("\"{}\":\"{}\"", UBLOCK_ID, UBLOCK_UUID));

    writeln!(
        file,
        "user_pref(\"extensions.webextensions.uuids\", \"{{{}}}\");",
        uuids.join(",")
    )
    .unwrap();
    writeln!(
        file,
        "user_pref(\"extensions.webextensions.ExtensionStorageIDB.migrated.{UBLOCK_ID}\", true);"
    )
    .unwrap();
}
