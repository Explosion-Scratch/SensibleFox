mod css;
mod extensions;
mod firefox;
mod paths;
mod policy;
mod prefs;
mod profile;
mod progress;
mod uninstall;
mod upstream;

use crate::progress::Progress;
use clap::{Parser, Subcommand};
use console::style;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Parser)]
#[command(name = "sensiblefox", version)]
#[command(about = "Opinionated, clean Firefox for macOS — zero config")]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,

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

#[derive(Subcommand)]
enum Cmd {
    /// Re-fetch upstream prefs from Betterfox/arkenfox into generated/
    Update,

    /// Interactive picker to delete any SensibleFox / Firefox artifact
    Clean,

    /// Uninstall things SensibleFox installed
    #[command(subcommand)]
    Uninstall(UninstallCmd),

    /// Delete a SensibleFox profile (interactive picker if NAME is omitted)
    DeleteProfile {
        /// Profile directory name, e.g. `sensiblefox` or `sensiblefox-1`
        name: Option<String>,
    },
}

#[derive(Subcommand)]
enum UninstallCmd {
    /// Remove Firefox.app from /Applications and ~/Applications (profiles kept)
    Firefox,

    /// Remove SensibleFox policies + managed storage; reverts Firefox to
    /// stock behaviour. Profile data, bookmarks, history, extensions are kept.
    Sensiblefox,
}

fn main() {
    let cli = Cli::parse();

    #[cfg(unix)]
    {
        if let Ok(mut signals) = signal_hook::iterator::Signals::new(&[
            signal_hook::consts::SIGINT,
            signal_hook::consts::SIGTERM,
        ]) {
            std::thread::spawn(move || {
                for _ in signals.forever() {
                    std::process::exit(130);
                }
            });
        }
    }
    #[cfg(not(unix))]
    {
        let _ = ctrlc::set_handler(|| std::process::exit(130));
    }

    if cli.status_file.is_none() && !cli.system_only {
        print_banner();
    }

    match cli.command {
        Some(Cmd::Update) => {
            upstream::fetch_all();
            return;
        }
        Some(Cmd::Clean) => {
            uninstall::clean_interactive();
            return;
        }
        Some(Cmd::Uninstall(UninstallCmd::Firefox)) => {
            uninstall::uninstall_firefox();
            return;
        }
        Some(Cmd::Uninstall(UninstallCmd::Sensiblefox)) => {
            uninstall::revert_to_stock();
            return;
        }
        Some(Cmd::DeleteProfile { ref name }) => {
            uninstall::delete_profile(name.as_deref());
            return;
        }
        None => {}
    }

    run_install(cli);
}

fn run_install(cli: Cli) {
    let install_target = if let Some(ref custom) = cli.app_dir {
        firefox::InstallTarget::Custom(custom.join("Firefox.app"))
    } else if cli.user {
        firefox::InstallTarget::User
    } else {
        firefox::InstallTarget::System
    };

    // Elevation strategy: do ONLY the system-side work (install Firefox to
    // /Applications + write /Library policies) under sudo, then continue as
    // the original user so profile files end up user-owned. Mirrors the PKG
    // postinstall split.
    let mut system_done_via_sudo = false;
    if !is_root()
        && matches!(install_target, firefox::InstallTarget::System)
        && !cli.profile_only
        && !cli.system_only
    {
        println!(
            "  {} Elevating to root to install Firefox and write system policies...",
            style("ℹ").cyan()
        );
        let current_exe = std::env::current_exe().expect("Failed to get current executable path");
        let elevated_status = std::env::temp_dir().join("sensiblefox-cli-elevated.status");
        let mut sudo = std::process::Command::new("sudo");
        sudo.arg(current_exe)
            .arg("--system-only")
            .arg("--unattended")
            .arg("--status-file")
            .arg(&elevated_status);
        if cli.no_policies {
            sudo.arg("--no-policies");
        }
        if cli.replace_firefox {
            sudo.arg("--replace-firefox");
        }
        let status = sudo.status().expect("Failed to execute sudo");
        let _ = std::fs::remove_file(&elevated_status);
        if !status.success() {
            std::process::exit(status.code().unwrap_or(1));
        }
        system_done_via_sudo = true;
    }

    let apply_policies = !cli.no_policies
        && !cli.profile_only
        && !system_done_via_sudo
        && (!cli.user || is_root());

    let using_default_path = cli.profile_path.is_none();
    let default_path = profile::default_profile_path();
    let mut final_profile_path = cli.profile_path.clone().unwrap_or_else(|| default_path.clone());
    let mut just_launch = false;

    if cli.profile_path.is_none() && !cli.system_only {
        let existing = profile::discover_profiles();
        if !existing.is_empty() {
            if cli.unattended {
                if default_path.exists() {
                    final_profile_path = next_unused_profile(&default_path);
                }
            } else if cli.status_file.is_none() {
                let (chosen, launch_only) = prompt_existing_profile(&existing, &default_path);
                final_profile_path = chosen;
                just_launch = launch_only;
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
        let firefox_path = match firefox::detect_or_download(
            &install_target,
            cli.unattended,
            cli.replace_firefox,
            &progress,
        ) {
            Ok(p) => p,
            Err(e) => fail(&progress, "Failed to find Firefox", &e),
        };
        launch(&firefox_path, &profile_path);
        return;
    }

    let firefox_path = if cli.profile_only && install_target.bin_path().exists() {
        // PKG user-phase fast path: Firefox is already in place from the
        // system-phase. Don't rewind the progress bar.
        install_target.bin_path()
    } else {
        progress.step(steps.detect, "Detecting Firefox");
        match firefox::detect_or_download(
            &install_target,
            cli.unattended,
            cli.replace_firefox,
            &progress,
        ) {
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

    fix_user_ownership(&profile_path);

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

fn next_unused_profile(default: &Path) -> PathBuf {
    let mut i = 1;
    loop {
        let p = default.with_file_name(format!("sensiblefox-{}", i));
        if !p.exists() {
            return p;
        }
        i += 1;
    }
}

fn prompt_existing_profile(existing: &[PathBuf], default: &Path) -> (PathBuf, bool) {
    use dialoguer::{theme::ColorfulTheme, Select};
    let mut items = vec![
        "Launch existing profile".to_string(),
        "Create new profile".to_string(),
    ];
    for p in existing {
        items.push(format!(
            "Update profile: {}",
            p.file_name().unwrap_or_default().to_string_lossy()
        ));
    }
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("SensibleFox profile(s) already exist. What would you like to do?")
        .default(0)
        .items(&items)
        .interact()
        .unwrap_or(0);
    match selection {
        0 => (existing[0].clone(), true),
        1 => (next_unused_profile(default), false),
        i => (existing[i - 2].clone(), false),
    }
}

fn fail(progress: &Progress, title: &str, detail: &str) -> ! {
    progress.fail(title, detail);
    eprintln!("  {} {}: {}", style("✗").red().bold(), title, detail);
    std::process::exit(1);
}

/// Restore user ownership over anything we may have written as root into the
/// user's home. Only runs when the current process is root — normally the
/// user-phase isn't elevated and this is a no-op.
fn fix_user_ownership(profile_path: &Path) {
    if !is_root() {
        return;
    }
    let Some(user) = get_console_user() else { return };
    let Some(home) = profile::user_home() else { return };

    let targets = [
        profile_path.to_path_buf(),
        home.join(paths::FIREFOX_ROOT_REL),
        home.join(paths::MANAGED_STORAGE_DIR_REL),
        home.join(paths::USER_POLICY_REL),
    ];

    for path in targets.iter().filter(|p| p.exists()) {
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

fn launch(firefox_path: &Path, profile_path: &Path) {
    let app_path = firefox_app_bundle(firefox_path);
    let already_running = firefox_is_running();

    let mut cmd = launch_command_as_user();
    cmd.arg("open").arg("-a").arg(&app_path);
    if !already_running {
        cmd.arg("--args").arg("--profile").arg(profile_path);
    }
    cmd.stdout(Stdio::null()).stderr(Stdio::null());

    match cmd.status() {
        Ok(s) if s.success() && already_running => println!(
            "{} Firefox is already running — brought to the front",
            style("✓").green().bold()
        ),
        Ok(s) if s.success() => println!(
            "{} Firefox launched with sensiblefox profile",
            style("✓").green().bold()
        ),
        Ok(s) => {
            eprintln!(
                "  {} `open` exited with status {} while launching Firefox",
                style("✗").red().bold(),
                s.code().unwrap_or(-1)
            );
            std::process::exit(1);
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

fn firefox_app_bundle(firefox_path: &Path) -> PathBuf {
    let mut p = firefox_path.to_path_buf();
    for tail in ["firefox", "MacOS", "Contents"] {
        if p.ends_with(tail) {
            p.pop();
        }
    }
    p
}

/// Build an `open` invocation that runs in the console user's launchd
/// context. Required when the CLI was elevated via sudo — otherwise Firefox
/// would be spawned by /var/root and lock its profile against the real user.
fn launch_command_as_user() -> Command {
    if is_root() {
        if let Some(user) = get_console_user() {
            if let Some(uid) = uid_for(&user) {
                let mut cmd = Command::new("launchctl");
                cmd.args(["asuser", &uid.to_string(), "sudo", "-u", &user]);
                return cmd;
            }
        }
    }
    Command::new("env")
}

fn uid_for(user: &str) -> Option<u32> {
    Command::new("id")
        .args(["-u", user])
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
}

fn firefox_is_running() -> bool {
    ["firefox", "firefox-bin"].iter().any(|name| {
        Command::new("pgrep")
            .args(["-x", name])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    })
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
