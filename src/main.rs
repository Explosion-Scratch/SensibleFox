mod css;
mod extensions;
mod firefox;
mod policy;
mod prefs;
mod profile;
mod progress;
mod upstream;

use crate::progress::Progress;
use clap::Parser;
use console::style;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Parser)]
#[command(name = "sensiblefox")]
#[command(about = "Opinionated, clean Firefox for macOS — zero config")]
#[command(version)]
struct Cli {
    /// Configure the profile without launching Firefox
    #[arg(long)]
    profile_only: bool,

    /// Install Firefox to ~/Applications (no admin prompt)
    #[arg(short = 'u', long)]
    user: bool,

    /// Skip writing the macOS Firefox policy files
    #[arg(long)]
    no_policies: bool,

    /// Reinstall Firefox even when a valid copy is already present
    #[arg(long)]
    replace_firefox: bool,

    /// Pick which SensibleFox profiles, policies, and managed storage to delete
    #[arg(long)]
    clean: bool,

    /// Re-fetch upstream prefs from Betterfox/arkenfox into generated/
    #[arg(long)]
    update_upstream: bool,

    /// Custom Firefox.app install directory
    #[arg(long, hide = true)]
    app_dir: Option<PathBuf>,

    /// Custom profile output path
    #[arg(long, hide = true)]
    profile_path: Option<PathBuf>,

    /// Install Firefox + policies only; skip user profile (installer use)
    #[arg(long, hide = true)]
    system_only: bool,

    /// Never prompt; fail fast (installer use)
    #[arg(long, hide = true)]
    unattended: bool,

    /// Write progress status to a file (installer use)
    #[arg(long, hide = true)]
    status_file: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();

    let _ = ctrlc::set_handler(|| {
        eprintln!("\n…interrupted — cleaning up");
        std::process::exit(130);
    });

    if cli.status_file.is_none() && !cli.system_only {
        print_banner();
    }

    if cli.clean {
        clean::run();
        return;
    }

    if cli.update_upstream {
        upstream::fetch_all();
        return;
    }

    // CLI default: install Firefox to /Applications, apply policies, configure
    // user profile — same as the PKG. Pass --user to install to ~/Applications,
    // --no-policies to skip the macOS policy plist.
    let install_target = if let Some(ref custom) = cli.app_dir {
        firefox::InstallTarget::Custom(custom.join("Firefox.app"))
    } else if cli.user {
        firefox::InstallTarget::User
    } else {
        firefox::InstallTarget::System
    };

    if !is_root() && matches!(install_target, firefox::InstallTarget::System) && !cli.profile_only {
        println!("  {} Elevating to root for system-wide installation...", style("ℹ").cyan());
        let current_exe = std::env::current_exe().expect("Failed to get current executable path");
        let args: Vec<String> = std::env::args().skip(1).collect();
        let status = std::process::Command::new("sudo")
            .arg(current_exe)
            .args(args)
            .status()
            .expect("Failed to execute sudo");
        std::process::exit(status.code().unwrap_or(1));
    }

    let apply_policies = !cli.no_policies && !cli.profile_only && (!cli.user || is_root());

    let using_default_path = cli.profile_path.is_none();
    let profile_path = cli
        .profile_path
        .clone()
        .unwrap_or_else(profile::default_profile_path);
    let mut final_profile_path = profile_path.clone();
    let mut just_launch = false;

    if cli.profile_path.is_none() && !cli.system_only {
        let existing = profile::discover_profiles();
        if !existing.is_empty() {
            if cli.unattended {
                // Non-interactive: create a new profile if the default one exists
                if profile_path.exists() {
                    let mut i = 1;
                    loop {
                        let p = profile_path.with_file_name(format!("sensiblefox-{}", i));
                        if !p.exists() {
                            final_profile_path = p;
                            break;
                        }
                        i += 1;
                    }
                }
            } else if cli.status_file.is_none() {
                // Interactive: prompt
                use dialoguer::{theme::ColorfulTheme, Select};
                let mut items = vec!["Launch existing profile".to_string(), "Create new profile".to_string()];
                for p in &existing {
                    items.push(format!("Update profile: {}", p.file_name().unwrap_or_default().to_string_lossy()));
                }

                let selection = Select::with_theme(&ColorfulTheme::default())
                    .with_prompt("SensibleFox profile(s) already exist. What would you like to do?")
                    .default(0)
                    .items(&items)
                    .interact()
                    .unwrap_or(0);

                if selection == 0 {
                    final_profile_path = existing[0].clone();
                    just_launch = true;
                } else if selection == 1 {
                    let mut i = 1;
                    loop {
                        let p = profile_path.with_file_name(format!("sensiblefox-{}", i));
                        if !p.exists() {
                            final_profile_path = p;
                            break;
                        }
                        i += 1;
                    }
                } else {
                    final_profile_path = existing[selection - 2].clone();
                }
            }
        }
    }

    let profile_path = final_profile_path;

    let progress = Progress::new(cli.status_file.clone(), firefox::install_step_list());
    let steps = firefox::step_indexes();

    // Fast path: reuse an existing profile, just relaunch.
    if just_launch {
        println!(
            "{} Launching existing profile at {}",
            style("→").blue().bold(),
            style(profile_path.display()).cyan()
        );
        progress.step(steps.detect, "Detecting Firefox");
        let firefox_path =
            match firefox::detect_or_download(&install_target, cli.unattended, cli.replace_firefox, &progress) {
                Ok(p) => p,
                Err(e) => fail(&progress, "Failed to find Firefox", &e),
            };
        println!("  Launching existing profile...\n");
        launch(&firefox_path, &profile_path);
        return;
    }

    let firefox_path = if cli.profile_only {
        // Second-phase PKG invocation: Firefox is already in place. Don't
        // rewind the progress bar — start at the profile step.
        let bin = install_target.bin_path();
        if bin.exists() {
            bin
        } else {
            progress.step(steps.detect, "Detecting Firefox");
            match firefox::detect_or_download(&install_target, cli.unattended, cli.replace_firefox, &progress) {
                Ok(p) => p,
                Err(e) => fail(&progress, "Could not install Firefox", &e),
            }
        }
    } else {
        progress.step(steps.detect, "Detecting Firefox");
        match firefox::detect_or_download(&install_target, cli.unattended, cli.replace_firefox, &progress) {
            Ok(p) => p,
            Err(e) => fail(&progress, "Could not install Firefox", &e),
        }
    };

    if apply_policies {
        progress.step(steps.policies, "Writing macOS Firefox policy plist...");
        if let Err(e) = policy::apply_macos_policies() {
            fail(&progress, "Failed to apply policies", &e);
        }
        if matches!(install_target, firefox::InstallTarget::System) {
            progress.sub(0.5, "Writing system uBlock managed storage...");
            if let Err(e) = policy::apply_system_managed_storage() {
                if !progress.is_quiet() {
                    eprintln!("  {} {}", style("!").yellow(), e);
                }
            }
        }
    }

    if cli.system_only {
        // PKG splits root vs user work — advance the bar so the applet shows
        // "Preparing user profile" rather than the last completed substep
        // while the user-side phase spins up.
        progress.step(steps.profile, "Preparing user profile...");
        return;
    }

    progress.step(steps.profile, "Creating profile directory");
    if let Err(e) = profile::create(&profile_path) {
        fail(&progress, "Failed to create profile", &e);
    }

    progress.step(steps.prefs, "Writing user.js");
    if let Err(e) = prefs::write(&profile_path) {
        fail(&progress, "Failed to write preferences", &e);
    }

    progress.step(steps.chrome, "Writing userChrome.css");
    if let Err(e) = css::write(&profile_path) {
        fail(&progress, "Failed to write userChrome CSS", &e);
    }

    progress.step(steps.ublock, "Installing uBlock Origin");
    extensions::install_ublock(&profile_path, &progress);

    if using_default_path {
        progress.step(steps.register, "Registering default profile");
        profile::register_default(&profile_path, Some(firefox_path.as_path()));
    }

    ensure_correct_ownership(&profile_path);

    progress.finish();

    if cli.status_file.is_none() {
        println!(
            "\n{} Profile built at {}",
            style("✓").green().bold(),
            style(profile_path.display()).cyan()
        );
    }

    if !cli.profile_only && cli.status_file.is_none() {
        launch(&firefox_path, &profile_path);
    } else if cli.profile_only && cli.status_file.is_none() {
        println!(
            "\n  Launch manually:\n  {} --profile {}",
            firefox_path.display(),
            profile_path.display()
        );
    }
}

fn fail(progress: &Progress, title: &str, detail: &str) -> ! {
    progress.fail(title, detail);
    eprintln!("  {} {}: {}", style("✗").red().bold(), title, detail);
    std::process::exit(1);
}

fn ensure_correct_ownership(path: &Path) {
    if !is_root() {
        return;
    }
    if let Some(user) = get_console_user() {
        let _ = Command::new("chown")
            .args(["-R", &user, &path.to_string_lossy()])
            .status();
    }
}

pub fn is_root() -> bool {
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
    let mut app_path = firefox_path.clone();
    if app_path.ends_with("firefox") {
        app_path.pop();
    }
    if app_path.ends_with("MacOS") {
        app_path.pop();
    }
    if app_path.ends_with("Contents") {
        app_path.pop();
    }

    match Command::new("open")
        .arg("-n")
        .arg("-a")
        .arg(&app_path)
        .arg("--args")
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
                "  {} Failed to launch Firefox: {}\n  App: {}\n  Profile: {}",
                style("✗").red().bold(),
                e,
                app_path.display(),
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

mod clean {
    use crate::profile;
    use console::style;
    use dialoguer::{theme::ColorfulTheme, Confirm, MultiSelect};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    const SYSTEM_POLICY_PLIST: &str = "/Library/Preferences/org.mozilla.firefox.plist";
    const SYSTEM_MANAGED_STORAGE: &str =
        "/Library/Application Support/Mozilla/ManagedStorage/uBlock0@raymondhill.net.json";
    const SUPPORT_DIR: &str = "/Library/Application Support/SensibleFox";
    const SYSTEM_APP: &str = "/Applications/Firefox.app";

    pub fn run() {
        let profiles = profile::discover_profiles();
        let user_home = profile::user_home().unwrap_or_default();
        let user_policy = user_home.join("Library/Preferences/org.mozilla.firefox.plist");
        let user_managed = user_home.join("Library/Application Support/Mozilla/ManagedStorage/uBlock0@raymondhill.net.json");
        let user_app = user_home.join("Applications/Firefox.app");

        let system_policy = Path::new(SYSTEM_POLICY_PLIST);
        let system_managed = Path::new(SYSTEM_MANAGED_STORAGE);
        let support_dir = Path::new(SUPPORT_DIR);
        let system_app = Path::new(SYSTEM_APP);

        let mut items: Vec<(String, PathBuf)> = Vec::new();
        for p in &profiles {
            items.push((format!("Profile: {}", p.display()), p.clone()));
        }
        if system_policy.exists() {
            items.push((format!("System policy plist: {}", system_policy.display()), system_policy.into()));
        }
        if user_policy.exists() {
            items.push((format!("User policy plist: {}", user_policy.display()), user_policy.into()));
        }
        if system_managed.exists() {
            items.push((format!("System uBO managed storage: {}", system_managed.display()), system_managed.into()));
        }
        if user_managed.exists() {
            items.push((format!("User uBO managed storage: {}", user_managed.display()), user_managed.into()));
        }
        if system_app.exists() {
            items.push((format!("System Firefox app: {}", system_app.display()), system_app.into()));
        }
        if user_app.exists() {
            items.push((format!("User Firefox app: {}", user_app.display()), user_app.into()));
        }
        if support_dir.exists() {
            items.push((format!("Installer support files: {}", support_dir.display()), support_dir.into()));
        }

        if items.is_empty() {
            println!("  {} Nothing to clean — no SensibleFox artifacts found", style("!").yellow());
            return;
        }

        let labels: Vec<&str> = items.iter().map(|(l, _)| l.as_str()).collect();
        let defaults: Vec<bool> = items.iter().map(|_| true).collect();
        let picks = MultiSelect::with_theme(&ColorfulTheme::default())
            .with_prompt("Select items to delete (space to toggle, enter to confirm)")
            .items(&labels)
            .defaults(&defaults)
            .interact()
            .unwrap_or_default();

        if picks.is_empty() {
            println!("  Nothing selected.");
            return;
        }

        let confirm = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("Delete {} item(s)?", picks.len()))
            .default(false)
            .interact()
            .unwrap_or(false);
        if !confirm {
            return;
        }

        let mut root_paths = Vec::new();
        let mut user_paths = Vec::new();

        for &i in &picks {
            let path = &items[i].1;
            let needs_root = path.starts_with("/Library") || path.starts_with("/Applications");
            if profiles.contains(path) {
                profile::unregister(path);
            }
            if needs_root && !super::is_root() {
                root_paths.push(path);
            } else {
                user_paths.push(path);
            }
        }

        for path in user_paths {
            match remove_local_path(path) {
                Ok(()) => println!("  {} {}", style("✓").green(), path.display()),
                Err(e) => println!("  {} {}: {}", style("✗").red(), path.display(), e),
            }
        }

        if !root_paths.is_empty() {
            println!("  {} Deleting system files requires sudo. Please enter your password if prompted.", style("ℹ").cyan());
            let mut cmd = Command::new("sudo");
            cmd.arg("rm").arg("-rf");
            for path in &root_paths {
                cmd.arg(path);
            }
            match cmd.status() {
                Ok(status) if status.success() => {
                    for path in root_paths {
                        println!("  {} {}", style("✓").green(), path.display());
                    }
                }
                _ => {
                    for path in root_paths {
                        println!("  {} Failed to delete system file: {}", style("✗").red(), path.display());
                    }
                }
            }
        }
    }

    fn remove_local_path(path: &Path) -> Result<(), String> {
        if !path.exists() {
            return Ok(());
        }
        if path.is_dir() {
            fs::remove_dir_all(path).map_err(|e| e.to_string())
        } else {
            fs::remove_file(path).map_err(|e| e.to_string())
        }
    }


}
