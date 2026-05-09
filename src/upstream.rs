use console::style;
use std::fs;
use std::path::PathBuf;

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

const ARKENFOX_URL: &str =
    "https://raw.githubusercontent.com/arkenfox/user.js/master/user.js";

pub fn fetch_all() {
    let out_dir = upstream_dir();
    fs::create_dir_all(&out_dir).expect("failed to create generated directory");

    println!(
        "{} Fetching upstream prefs...\n",
        style("→").blue().bold()
    );

    for (name, url) in BETTERFOX_URLS {
        print!("  {} Betterfox/{name}... ", style("↓").cyan());
        match reqwest::blocking::get(*url) {
            Ok(resp) if resp.status().is_success() => {
                let body = resp.text().unwrap_or_default();
                let path = out_dir.join(format!("betterfox-{}.js", name.to_lowercase()));
                fs::write(&path, &body).expect("failed to write upstream file");
                println!("{} ({} lines)", style("ok").green(), body.lines().count());
            }
            Ok(resp) => {
                println!("{} (HTTP {})", style("failed").red(), resp.status());
            }
            Err(e) => {
                println!("{} ({})", style("failed").red(), e);
            }
        }
    }

    print!("  {} arkenfox/user.js... ", style("↓").cyan());
    match reqwest::blocking::get(ARKENFOX_URL) {
        Ok(resp) if resp.status().is_success() => {
            let body = resp.text().unwrap_or_default();
            let path = out_dir.join("arkenfox-user.js");
            fs::write(&path, &body).expect("failed to write arkenfox file");
            println!("{} ({} lines)", style("ok").green(), body.lines().count());
        }
        Ok(resp) => {
            println!("{} (HTTP {})", style("failed").red(), resp.status());
        }
        Err(e) => {
            println!("{} ({})", style("failed").red(), e);
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

fn merge_upstream(dir: &PathBuf) {
    let mut merged = String::new();
    merged.push_str("// sensiblefox — upstream prefs (auto-generated)\n");
    merged.push_str("// Pulled from Betterfox + arkenfox. Do not edit.\n");
    merged.push_str(&format!(
        "// Generated: {}\n\n",
        chrono_like_now()
    ));

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
    println!(
        "  {} Merged into upstream-merged.js",
        style("✓").green()
    );
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
