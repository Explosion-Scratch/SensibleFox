use crate::paths::{
    self, SUPPORT_DIR, SYSTEM_FIREFOX_APP, SYSTEM_MANAGED_STORAGE, SYSTEM_POLICY_PLIST,
    USER_APP_REL, USER_MANAGED_REL, USER_POLICY_REL,
};
use crate::profile;
use console::style;
use dialoguer::{theme::ColorfulTheme, Confirm, MultiSelect};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Snapshot of every artifact SensibleFox (or Firefox itself) may have left
/// on disk. Only paths that currently exist are populated.
pub struct Artifacts {
    pub system_policy: Option<PathBuf>,
    pub user_policy: Option<PathBuf>,
    pub system_managed: Option<PathBuf>,
    pub user_managed: Option<PathBuf>,
    pub system_app: Option<PathBuf>,
    pub user_app: Option<PathBuf>,
    pub support_dir: Option<PathBuf>,
    pub profiles: Vec<PathBuf>,
}

impl Artifacts {
    pub fn discover() -> Self {
        let home = profile::user_home();
        let from_home = |rel: &str| home.as_ref().and_then(|h| exists(paths::join_home(h, rel)));
        Artifacts {
            system_policy: exists(PathBuf::from(SYSTEM_POLICY_PLIST)),
            user_policy: from_home(USER_POLICY_REL),
            system_managed: exists(PathBuf::from(SYSTEM_MANAGED_STORAGE)),
            user_managed: from_home(USER_MANAGED_REL),
            system_app: exists(PathBuf::from(SYSTEM_FIREFOX_APP)),
            user_app: from_home(USER_APP_REL),
            support_dir: exists(PathBuf::from(SUPPORT_DIR)),
            profiles: profile::discover_profiles(),
        }
    }
}

/// Remove the macOS Firefox enterprise policies SensibleFox installed.
/// Firefox.app, profile data, bookmarks, history, and extensions all stay.
pub fn revert_to_stock() {
    let a = Artifacts::discover();
    let targets: Vec<PathBuf> = [
        a.system_policy,
        a.user_policy,
        a.system_managed,
        a.user_managed,
        a.support_dir,
    ]
    .into_iter()
    .flatten()
    .collect();

    if targets.is_empty() {
        info("SensibleFox isn't installed — nothing to revert.");
        return;
    }

    header(
        "Reverting Firefox to stock",
        "Profile data, bookmarks, history, and Firefox.app are kept.",
    );
    run_deletion(&targets, "item");
}

/// Remove every Firefox.app SensibleFox knows about. Profiles stay intact so
/// reinstalling Firefox brings the user back exactly where they left off.
pub fn uninstall_firefox() {
    let a = Artifacts::discover();
    let targets: Vec<PathBuf> = [a.system_app, a.user_app].into_iter().flatten().collect();

    if targets.is_empty() {
        info("No Firefox.app found.");
        return;
    }

    header(
        "Uninstalling Firefox.app",
        "Your profiles, bookmarks, and SensibleFox policies are kept.",
    );
    kill_firefox();
    run_deletion(&targets, "Firefox.app");
}

/// Delete a SensibleFox profile by name, or interactively if `name` is None.
pub fn delete_profile(name: Option<&str>) {
    let profiles = profile::discover_profiles();
    if profiles.is_empty() {
        info("No SensibleFox profiles found.");
        return;
    }

    let chosen: Vec<PathBuf> = match name {
        Some(n) => profiles
            .into_iter()
            .filter(|p| p.file_name().and_then(|f| f.to_str()) == Some(n))
            .collect(),
        None => multiselect("Select profile(s) to delete", &profiles, false),
    };

    if chosen.is_empty() {
        if let Some(n) = name {
            warn(&format!("No SensibleFox profile named '{n}' found."));
        } else {
            info("Nothing selected.");
        }
        return;
    }

    header(
        "Deleting SensibleFox profile(s)",
        "Bookmarks, history, and extensions inside the profile will be lost.",
    );
    for p in &chosen {
        profile::unregister(p);
    }
    run_deletion(&chosen, "profile");
}

/// Interactive picker that lets the user delete any combination of artifacts.
pub fn clean_interactive() {
    let a = Artifacts::discover();
    let mut items: Vec<(String, PathBuf)> = Vec::new();
    for p in &a.profiles {
        items.push((format!("Profile: {}", p.display()), p.clone()));
    }
    let mut push = |label: &str, p: Option<PathBuf>| {
        if let Some(p) = p {
            items.push((format!("{}: {}", label, p.display()), p));
        }
    };
    push("System policy plist", a.system_policy);
    push("User policy plist", a.user_policy);
    push("System uBO managed storage", a.system_managed);
    push("User uBO managed storage", a.user_managed);
    push("System Firefox.app", a.system_app);
    push("User Firefox.app", a.user_app);
    push("Installer support files", a.support_dir);

    if items.is_empty() {
        warn("Nothing to clean — no SensibleFox artifacts found.");
        return;
    }

    let labels: Vec<&str> = items.iter().map(|(l, _)| l.as_str()).collect();
    let defaults: Vec<bool> = vec![true; items.len()];
    let picks = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select items to delete (space to toggle, enter to confirm)")
        .items(&labels)
        .defaults(&defaults)
        .interact()
        .unwrap_or_default();

    if picks.is_empty() {
        info("Nothing selected.");
        return;
    }
    let targets: Vec<PathBuf> = picks.into_iter().map(|i| items[i].1.clone()).collect();

    for p in &targets {
        if profile::is_sensiblefox_profile(p) {
            profile::unregister(p);
        }
    }
    run_deletion(&targets, "item");
}

// ── helpers ────────────────────────────────────────────────────────────────

fn run_deletion(targets: &[PathBuf], noun: &str) {
    for p in targets {
        println!("  {} {}", style("•").dim(), p.display());
    }
    if !confirm(&format!("Delete {} {noun}(s)?", targets.len())) {
        info("Aborted.");
        return;
    }
    delete_paths(targets);
}

fn delete_paths(paths: &[PathBuf]) {
    let (root_paths, user_paths): (Vec<&PathBuf>, Vec<&PathBuf>) = paths
        .iter()
        .partition(|p| needs_root(p) && !crate::is_root());

    for p in &user_paths {
        match remove_recursive(p) {
            Ok(()) => println!("  {} {}", style("✓").green(), p.display()),
            Err(e) => println!("  {} {}: {}", style("✗").red(), p.display(), e),
        }
    }

    if !root_paths.is_empty() {
        info("Removing system files requires sudo. You may be prompted.");
        let mut cmd = Command::new("sudo");
        cmd.arg("rm").arg("-rf");
        for p in &root_paths {
            cmd.arg(p);
        }
        let ok = cmd.status().map(|s| s.success()).unwrap_or(false);
        let mark = if ok {
            style("✓").green()
        } else {
            style("✗").red()
        };
        for p in &root_paths {
            println!("  {} {}", mark, p.display());
        }
    }
}

fn remove_recursive(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
    .map_err(|e| e.to_string())
}

fn needs_root(p: &Path) -> bool {
    p.starts_with("/Library") || p.starts_with("/Applications")
}

fn exists(p: PathBuf) -> Option<PathBuf> {
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

fn kill_firefox() {
    for name in &["firefox", "Firefox", "firefox-bin", "plugin-container"] {
        let _ = Command::new("pkill").args(["-x", name]).status();
    }
}

fn multiselect(prompt: &str, options: &[PathBuf], default_on: bool) -> Vec<PathBuf> {
    let labels: Vec<String> = options.iter().map(|p| p.display().to_string()).collect();
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let defaults = vec![default_on; options.len()];
    let picks = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(&label_refs)
        .defaults(&defaults)
        .interact()
        .unwrap_or_default();
    picks.into_iter().map(|i| options[i].clone()).collect()
}

fn confirm(prompt: &str) -> bool {
    Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .default(false)
        .interact()
        .unwrap_or(false)
}

fn header(title: &str, sub: &str) {
    println!("{} {}", style("→").blue().bold(), style(title).bold());
    println!("  {} {}", style("ℹ").dim(), style(sub).dim());
}

fn info(msg: &str) {
    println!("  {} {}", style("ℹ").cyan(), msg);
}

fn warn(msg: &str) {
    println!("  {} {}", style("!").yellow(), msg);
}
