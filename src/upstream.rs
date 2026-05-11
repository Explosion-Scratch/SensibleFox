use console::style;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const BETTERFOX_URLS: &[(&str, &str)] = &[
    (
        "Fastfox",
        "https://raw.githubusercontent.com/yokoffing/Betterfox/main/Fastfox.js",
    ),
    (
        "Peskyfox",
        "https://raw.githubusercontent.com/yokoffing/Betterfox/main/Peskyfox.js",
    ),
    (
        "Securefox",
        "https://raw.githubusercontent.com/yokoffing/Betterfox/main/Securefox.js",
    ),
    (
        "Smoothfox",
        "https://raw.githubusercontent.com/yokoffing/Betterfox/main/Smoothfox.js",
    ),
];

const ARKENFOX_URL: &str = "https://raw.githubusercontent.com/arkenfox/user.js/master/user.js";

const MAX_FETCH_RETRIES: u32 = 2;

pub fn fetch_all() {
    let out_dir = upstream_dir();
    if let Err(e) = fs::create_dir_all(&out_dir) {
        eprintln!("  {} Failed to create generated dir: {e}", style("✗").red());
        return;
    }

    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("  {} Failed to build HTTP client: {e}", style("✗").red());
            return;
        }
    };

    println!("{} Fetching upstream prefs...\n", style("→").blue().bold());

    for (name, url) in BETTERFOX_URLS {
        print!("  {} Betterfox/{name}... ", style("↓").cyan());
        match fetch_with_retry(&client, url) {
            Ok(body) => {
                let path = out_dir.join(format!("betterfox-{}.js", name.to_lowercase()));
                if let Err(e) = fs::write(&path, &body) {
                    println!("{} (write error: {e})", style("failed").red());
                } else {
                    println!("{} ({} lines)", style("ok").green(), body.lines().count());
                }
            }
            Err(msg) => {
                println!("{} ({})", style("failed").red(), msg);
            }
        }
    }

    print!("  {} arkenfox/user.js... ", style("↓").cyan());
    match fetch_with_retry(&client, ARKENFOX_URL) {
        Ok(body) => {
            let path = out_dir.join("arkenfox-user.js");
            if let Err(e) = fs::write(&path, &body) {
                println!("{} (write error: {e})", style("failed").red());
            } else {
                println!("{} ({} lines)", style("ok").green(), body.lines().count());
            }
        }
        Err(msg) => {
            println!("{} ({})", style("failed").red(), msg);
        }
    }

    merge_upstream(&out_dir);

    println!(
        "\n{} Upstream prefs saved to {}",
        style("✓").green().bold(),
        out_dir.display()
    );
    println!(
        "  Run {} to rebuild with updated prefs",
        style("cargo build --release").cyan()
    );
}

fn fetch_with_retry(client: &reqwest::blocking::Client, url: &str) -> Result<String, String> {
    let mut last_err = String::new();

    for attempt in 1..=MAX_FETCH_RETRIES {
        if attempt > 1 {
            let backoff = Duration::from_secs(2u64.pow(attempt - 1));
            std::thread::sleep(backoff);
        }

        match client.get(url).send() {
            Ok(resp) if resp.status().is_success() => {
                return resp.text().map_err(|e| format!("read error: {e}"));
            }
            Ok(resp) => {
                last_err = format!("HTTP {}", resp.status());
            }
            Err(e) => {
                last_err = e.to_string();
            }
        }
    }

    Err(last_err)
}

fn merge_upstream(dir: &Path) {
    let mut merged = String::new();
    merged.push_str("// sensiblefox — upstream prefs (auto-generated)\n");
    merged.push_str("// Pulled from Betterfox + arkenfox. Do not edit.\n");
    merged.push_str(&format!("// Generated: {}\n\n", chrono_like_now()));

    let files = [
        "betterfox-fastfox.js",
        "betterfox-peskyfox.js",
        "betterfox-securefox.js",
        "betterfox-smoothfox.js",
    ];

    for name in &files {
        let path = dir.join(name);
        if path.exists() {
            let content = fs::read_to_string(&path).unwrap_or_default();
            merged.push_str(&format!("\n// === {name} ===\n"));
            merged.push_str(&content);
            merged.push('\n');
        }
    }

    let out = dir.join("upstream-merged.js");
    fs::write(&out, &merged).expect("failed to write merged upstream prefs");
    println!("  {} Merged into upstream-merged.js", style("✓").green());
}

fn upstream_dir() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().expect("cannot determine cwd"));
    manifest_dir.join("generated")
}

fn chrono_like_now() -> String {
    let output = std::process::Command::new("date")
        .arg("+%Y-%m-%d %H:%M:%S")
        .output();
    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(_) => "unknown".to_string(),
    }
}
