mod css;
mod extensions;
mod firefox;
mod prefs;
mod profile;
mod upstream;

use clap::Parser;
use console::style;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Parser)]
#[command(name = "sensiblefox")]
#[command(about = "Opinionated, clean Firefox for macOS — zero config")]
#[command(version)]
struct Cli {
    /// Build profile without launching Firefox
    #[arg(long)]
    profile_only: bool,

    /// Custom profile output path
    #[arg(long)]
    profile_path: Option<PathBuf>,

    /// Re-fetch upstream prefs from Betterfox/arkenfox
    #[arg(long)]
    update_upstream: bool,

    /// Clean existing sensiblefox profiles
    #[arg(long)]
    clean: bool,
}

fn main() {
    let cli = Cli::parse();

    print_banner();

    if cli.clean {
        clean_profiles();
        return;
    }

    if cli.update_upstream {
        upstream::fetch_all();
        return;
    }

    let profile_path = cli.profile_path.unwrap_or_else(default_profile_path);

    if profile_path.exists() {
        println!(
            "{} Profile already exists at {}",
            style("→").blue().bold(),
            style(profile_path.display()).cyan()
        );
        println!("  Launching existing profile...\n");
        extensions::write_ublock_managed_storage();
        let firefox_path = firefox::detect_or_download();
        launch(&firefox_path, &profile_path);
        return;
    }

    step("Detecting Firefox");
    let firefox_path = firefox::detect_or_download();

    step("Creating profile");
    profile::create(&profile_path);

    step("Writing preferences");
    prefs::write(&profile_path);

    step("Writing userChrome CSS");
    css::write(&profile_path);

    step("Installing uBlock Origin");
    extensions::install_ublock(&profile_path);

    println!(
        "\n{} Profile built at {}",
        style("✓").green().bold(),
        style(profile_path.display()).cyan()
    );

    if !cli.profile_only {
        step("Launching Firefox");
        launch(&firefox_path, &profile_path);
    } else {
        println!(
            "\n  Launch manually:\n  {} --profile {}",
            firefox_path.display(),
            profile_path.display()
        );
    }
}

fn launch(firefox_path: &PathBuf, profile_path: &PathBuf) {
    Command::new(firefox_path)
        .arg("--profile")
        .arg(profile_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to launch Firefox");

    println!(
        "{} Firefox launched with sensiblefox profile",
        style("✓").green().bold()
    );
}

fn print_banner() {
    let fox = r#"
   ____                 _ _     _       __
  / ___|  ___ _ __  ___(_) |__ | | ___ / _| _____  __
  \___ \ / _ \ '_ \/ __| | '_ \| |/ _ \ |_ / _ \ \/ /
   ___) |  __/ | | \__ \ | |_) | |  __/  _| (_) >  <
  |____/ \___|_| |_|___/_|_.__/|_|\___|_|  \___/_/\_\
"#;
    println!("{}", style(fox).cyan().bold());
    println!(
        "  {}",
        style("opinionated firefox · zero config · mac only").dim()
    );
    println!();
}

fn step(msg: &str) {
    println!("{} {}", style("→").blue().bold(), style(msg).bold());
}

fn default_profile_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("sensiblefox")
        .join("profile")
}

fn clean_profiles() {
    use dialoguer::{theme::ColorfulTheme, MultiSelect};
    use std::fs;

    let base_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("sensiblefox");

    if !base_dir.exists() {
        println!("  {} No profiles found at {}", style("!").yellow(), base_dir.display());
        return;
    }

    let mut profiles = Vec::new();
    if let Ok(entries) = fs::read_dir(&base_dir) {
        for entry in entries.flatten() {
            if entry.file_type().map_or(false, |t| t.is_dir()) {
                profiles.push(entry.path());
            }
        }
    }

    if profiles.is_empty() {
        println!("  {} No profiles found in {}", style("!").yellow(), base_dir.display());
        return;
    }

    let profile_names: Vec<String> = profiles
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();

    let selections = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select profiles to delete (Space to select, Enter to confirm)")
        .items(&profile_names)
        .interact()
        .expect("Failed to render prompt");

    if selections.is_empty() {
        println!("  No profiles selected for deletion.");
        return;
    }

    for idx in selections {
        let path = &profiles[idx];
        print!("  {} Deleting {}... ", style("→").blue(), path.display());
        if let Err(e) = fs::remove_dir_all(path) {
            println!("{} ({})", style("failed").red(), e);
        } else {
            println!("{}", style("ok").green());
        }
    }
}
