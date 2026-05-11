mod css;
mod extensions;
mod firefox;
mod policy;
mod prefs;
mod profile;
mod upstream;

use clap::Parser;
use console::style;
use std::path::{Path, PathBuf};
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

    /// Install policy-managed Firefox using macOS policy locations
    #[arg(long)]
    policied: bool,

    /// Replace Firefox.app even when a valid copy is already installed
    #[arg(long)]
    replace_firefox: bool,

    /// Install Firefox and system policy files only; skip user profile setup
    #[arg(long)]
    system_only: bool,

    /// Do not prompt for confirmation (e.g. for installer use)
    #[arg(long)]
    unattended: bool,

    /// Write progress status to a file (for installer UI)
    #[arg(long)]
    status_file: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();

    // Set up SIGINT / SIGTERM handler
    let _ = ctrlc::set_handler(|| {
        eprintln!("\n…interrupted — cleaning up");
        std::process::exit(130);
    });

    if cli.status_file.is_none() {
        print_banner();
    }

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

    // Normal launches can reuse an existing profile. Installer/profile-only
    // runs intentionally refresh it so user.js, CSS, and extensions stay current.
    if !cli.policied && !cli.profile_only && profile_path.exists() {
        println!(
            "{} Profile already exists at {}",
            style("→").blue().bold(),
            style(profile_path.display()).cyan()
        );

        let firefox_path = match firefox::detect_or_download(
            &install_target,
            cli.unattended,
            cli.replace_firefox,
            cli.status_file.as_ref(),
        ) {
            Ok(path) => path,
            Err(e) => {
                eprintln!(
                    "  {} {}\n  Install Firefox manually and re-run, or pass --app-dir.",
                    style("✗").red().bold(),
                    e
                );
                std::process::exit(1);
            }
        };

        if using_default_path {
            profile::register_default(&profile_path, Some(firefox_path.as_path()));
        }
        extensions::write_ublock_managed_storage(None);

        println!("  Launching existing profile...\n");
        launch(&firefox_path, &profile_path);
        return;
    }

    step("Detecting Firefox");
    let firefox_path = match firefox::detect_or_download(
        &install_target,
        cli.unattended,
        cli.replace_firefox,
        cli.status_file.as_ref(),
    ) {
        Ok(p) => p,
        Err(e) => {
            if cli.status_file.is_none() {
                eprintln!(
                    "  {} {}\n  Install Firefox manually: brew install --cask firefox",
                    style("✗").red().bold(),
                    e
                );
            }
            std::process::exit(1);
        }
    };

    if cli.policied {
        step("Applying macOS Firefox policies");
        write_status(
            &cli.status_file,
            "configure",
            "Applying Firefox policies",
            "Step 1 of 7: writing macOS policy files...",
            15,
        );
        if let Err(e) = policy::apply_macos_policies() {
            eprintln!("  {} Failed to apply policies: {}", style("✗").red(), e);
            std::process::exit(1);
        }

        if cli.system {
            step("Configuring system-wide uBlock managed storage");
            write_status(
                &cli.status_file,
                "configure",
                "Applying Firefox policies",
                "Step 2 of 7: writing system uBlock managed storage...",
                25,
            );
            if let Err(e) = policy::apply_system_managed_storage() {
                eprintln!("  {} {}", style("!").yellow(), e);
            }
        }
    }

    if cli.system_only {
        write_status(
            &cli.status_file,
            "configure",
            "System files installed",
            "System files are ready. Preparing the user profile...",
            30,
        );
        return;
    }

    step("Creating profile");
    write_status(
        &cli.status_file,
        "configure",
        "Creating SensibleFox profile",
        "Step 3 of 7: creating profile directories...",
        35,
    );
    if let Err(e) = profile::create(&profile_path) {
        fail_install(&cli.status_file, "Failed to create profile", &e);
    }

    step("Writing preferences");
    write_status(
        &cli.status_file,
        "configure",
        "Applying SensibleFox preferences",
        "Step 4 of 7: writing and verifying user.js...",
        50,
    );
    if let Err(e) = prefs::write(&profile_path) {
        fail_install(&cli.status_file, "Failed to write preferences", &e);
    }

    step("Writing userChrome CSS");
    write_status(
        &cli.status_file,
        "configure",
        "Applying SensibleFox chrome",
        "Step 5 of 7: writing and verifying userChrome.css...",
        65,
    );
    if let Err(e) = css::write(&profile_path) {
        fail_install(&cli.status_file, "Failed to write userChrome CSS", &e);
    }

    step("Installing uBlock Origin");
    write_status(
        &cli.status_file,
        "configure",
        "Installing uBlock Origin",
        "Step 6 of 7: downloading extension and managed storage...",
        78,
    );
    extensions::install_ublock(&profile_path, cli.status_file.as_ref());

    if using_default_path {
        step("Registering default profile");
        write_status(
            &cli.status_file,
            "configure",
            "Registering Firefox profile",
            "Step 7 of 7: updating profiles.ini...",
            92,
        );
        profile::register_default(&profile_path, Some(firefox_path.as_path()));
    }

    // Ensure profile is owned by user if we just created it as root.
    ensure_correct_ownership(&profile_path);

    if cli.status_file.is_none() {
        println!(
            "\n{} Profile built at {}",
            style("✓").green().bold(),
            style(profile_path.display()).cyan()
        );
    } else {
        finish_status(&cli.status_file);
    }

    if !cli.profile_only {
        step("Launching Firefox");
        launch(&firefox_path, &profile_path);
    } else if cli.status_file.is_none() {
        println!(
            "\n  Launch manually:\n  {} --profile {}",
            firefox_path.display(),
            profile_path.display()
        );
    }
}

fn finish_status(status_file: &Option<PathBuf>) {
    if let Some(sf) = status_file {
        firefox::write_status(
            sf,
            "done",
            "SensibleFox installed",
            "Firefox is ready to launch.",
            100,
            100,
        );
    }
}

fn fail_install(status_file: &Option<PathBuf>, title: &str, detail: &str) -> ! {
    if let Some(sf) = status_file {
        firefox::write_status(sf, "error", title, detail, 0, 100);
    }
    eprintln!("  {} {}: {}", style("✗").red().bold(), title, detail);
    std::process::exit(1);
}

fn write_status(
    status_file: &Option<PathBuf>,
    step: &str,
    title: &str,
    detail: &str,
    progress: u64,
) {
    if let Some(sf) = status_file {
        firefox::write_status(sf, step, title, detail, progress, 100);
    }
}

fn ensure_correct_ownership(path: &Path) {
    // Only attempt if we are running as root (e.g. from the PKG installer).
    if !is_root() {
        return;
    }

    if let Some(user) = get_console_user() {
        let _ = Command::new("chown")
            .args(["-R", &user, &path.to_string_lossy()])
            .status();
    }
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

fn get_console_user() -> Option<String> {
    // Use stat -f%Su /dev/console to find the logged-in GUI user.
    Command::new("stat")
        .args(["-f%Su", "/dev/console"])
        .output()
        .ok()
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() || s == "root" {
                None
            } else {
                Some(s)
            }
        })
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
