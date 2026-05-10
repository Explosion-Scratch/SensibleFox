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

    /// Install Firefox system-wide to /Applications (requires admin password prompt)
    #[arg(short = 's', long)]
    system: bool,

    /// Custom Firefox.app install directory (e.g. ~/Applications or /Applications)
    #[arg(long)]
    app_dir: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();

    // Set up SIGINT / SIGTERM handler so temp files and DMG mounts are
    // cleaned up on Ctrl-C.  ctrlc::set_handler sets a one-shot handler;
    // we just request a graceful exit — the Drop impls do the heavy lifting.
    let _ = ctrlc::set_handler(|| {
        eprintln!("\n…interrupted — cleaning up");
    });

    print_banner();

    if cli.clean {
        clean_profiles();
        return;
    }

    if cli.update_upstream {
        upstream::fetch_all();
        return;
    }

    // Determine install target.
    let install_target = if let Some(ref custom) = cli.app_dir {
        firefox::InstallTarget::Custom(custom.join("Firefox.app"))
    } else if cli.system {
        firefox::InstallTarget::System
    } else {
        firefox::InstallTarget::User
    };

    let using_default_path = cli.profile_path.is_none();
    let profile_path = cli
        .profile_path
        .unwrap_or_else(profile::default_profile_path);

    if profile_path.exists() {
        println!(
            "{} Profile already exists at {}",
            style("→").blue().bold(),
            style(profile_path.display()).cyan()
        );
        if using_default_path {
            profile::register_default(&profile_path);
        }
        println!("  Launching existing profile...\n");
        extensions::write_ublock_managed_storage();
        match firefox::detect_or_download(&install_target) {
            Ok(firefox_path) => launch(&firefox_path, &profile_path),
            Err(e) => {
                eprintln!(
                    "  {} {}\n  Install Firefox manually and re-run, or pass --app-dir.",
                    style("✗").red().bold(),
                    e
                );
                std::process::exit(1);
            }
        }
        return;
    }

    step("Detecting Firefox");
    let firefox_path = match firefox::detect_or_download(&install_target) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "  {} {}\n  Install Firefox manually: brew install --cask firefox",
                style("✗").red().bold(),
                e
            );
            std::process::exit(1);
        }
    };

    step("Creating profile");
    profile::create(&profile_path);

    step("Writing preferences");
    prefs::write(&profile_path);

    step("Writing userChrome CSS");
    css::write(&profile_path);

    step("Installing uBlock Origin");
    extensions::install_ublock(&profile_path);

    if using_default_path {
        step("Registering default profile");
        profile::register_default(&profile_path);
    }

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
    match Command::new(firefox_path)
        .arg("--profile")
        .arg(profile_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(_) => {
            println!(
                "{} Firefox launched with sensiblefox profile",
                style("✓").green().bold()
            );
        }
        Err(e) => {
            eprintln!(
                "  {} Failed to launch Firefox: {}\n  Path: {}\n  Profile: {}",
                style("✗").red().bold(),
                e,
                firefox_path.display(),
                profile_path.display()
            );
            std::process::exit(1);
        }
    }
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

fn clean_profiles() {
    use dialoguer::{theme::ColorfulTheme, Confirm};
    use std::fs;

    let path = profile::default_profile_path();
    if !path.exists() {
        println!("  {} No SensibleFox profile found", style("!").yellow());
        return;
    }

    let confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("Delete profile at {}?", path.display()))
        .default(false)
        .interact()
        .unwrap_or(false);

    if !confirm {
        return;
    }

    if let Err(e) = fs::remove_dir_all(&path) {
        println!("  {} Failed to delete: {}", style("✗").red(), e);
        return;
    }
    profile::unregister(&path);
    println!("  {} Profile deleted", style("✓").green());
}
