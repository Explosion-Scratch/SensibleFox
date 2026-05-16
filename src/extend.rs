use console::style;
use std::fs;
use std::path::{Path, PathBuf};

/// Categories of data that can be extended from an existing Firefox profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExtendSelection {
    History,
    Passwords,
    Bookmarks,
    Extensions,
    Cookies,
}

impl ExtendSelection {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "history" => Some(Self::History),
            "passwords" => Some(Self::Passwords),
            "bookmarks" => Some(Self::Bookmarks),
            "extensions" => Some(Self::Extensions),
            "cookies" => Some(Self::Cookies),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::History => "history",
            Self::Passwords => "passwords",
            Self::Bookmarks => "bookmarks",
            Self::Extensions => "extensions",
            Self::Cookies => "cookies",
        }
    }
}

#[allow(dead_code)]
pub const DEFAULT_SELECTIONS: &[ExtendSelection] = &[
    ExtendSelection::Bookmarks,
    ExtendSelection::History,
    ExtendSelection::Extensions,
    ExtendSelection::Passwords,
];

#[allow(dead_code)]
pub const ALL_SELECTIONS: &[ExtendSelection] = &[
    ExtendSelection::Bookmarks,
    ExtendSelection::History,
    ExtendSelection::Extensions,
    ExtendSelection::Passwords,
    ExtendSelection::Cookies,
];

/// Parse a comma-separated list of extend-selection values.
pub fn parse_selections(input: &str) -> Result<Vec<ExtendSelection>, String> {
    let mut out = Vec::new();
    for part in input.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        match ExtendSelection::from_str(trimmed) {
            Some(sel) => {
                if !out.contains(&sel) {
                    out.push(sel);
                }
            }
            None => {
                return Err(format!(
                    "unknown extend-selection '{}'. Valid: bookmarks,history,extensions,passwords,cookies",
                    trimmed
                ));
            }
        }
    }
    if out.is_empty() {
        return Err("extend-selections must not be empty".into());
    }
    Ok(out)
}

/// Discover all non-SensibleFox Firefox profiles on the system.
/// Returns (profile_name, profile_path) pairs.
pub fn discover_firefox_profiles() -> Vec<(String, PathBuf)> {
    let Some(root) = crate::profile::firefox_root() else {
        return Vec::new();
    };

    let profiles_ini = root.join("profiles.ini");
    if !profiles_ini.exists() {
        return discover_profiles_by_scan(&root);
    }

    let text = fs::read_to_string(&profiles_ini).unwrap_or_default();
    let mut results = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_path: Option<String> = None;
    let mut is_relative: Option<bool> = None;

    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            if let (Some(name), Some(path)) = (current_name.take(), current_path.take()) {
                let resolved = resolve_profile_path(&root, &path, is_relative.unwrap_or(true));
                if resolved.exists() {
                    results.push((name, resolved));
                }
            }
            current_name = None;
            current_path = None;
            is_relative = None;
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "Name" => current_name = Some(value.trim().to_string()),
                "Path" => current_path = Some(value.trim().to_string()),
                "IsRelative" => is_relative = Some(value.trim() == "1"),
                _ => {}
            }
        }
    }
    if let (Some(name), Some(path)) = (current_name, current_path) {
        let resolved = resolve_profile_path(&root, &path, is_relative.unwrap_or(true));
        if resolved.exists() {
            results.push((name, resolved));
        }
    }

    if results.is_empty() {
        return discover_profiles_by_scan(&root);
    }

    results
}

fn discover_profiles_by_scan(root: &Path) -> Vec<(String, PathBuf)> {
    let profiles_dir = root.join("Profiles");
    let mut results = Vec::new();
    if let Ok(rd) = fs::read_dir(&profiles_dir) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let name = p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                results.push((name.clone(), p));
            }
        }
    }
    results
}

fn resolve_profile_path(root: &Path, path: &str, is_relative: bool) -> PathBuf {
    if is_relative {
        root.join(path)
    } else {
        PathBuf::from(path)
    }
}

/// Resolve an `--extend` argument to a source profile path.
///
/// - If `extend_arg` is `Some(value)`:
///   - If `value` is an existing directory path, use it directly.
///   - Otherwise, treat it as a profile name and look it up.
/// - If `extend_arg` is `None`:
///   - If exactly one non-sensiblefox profile exists, use it.
///   - Otherwise, prompt interactively (or fail in unattended mode).
pub fn resolve_extend_source(
    extend_arg: Option<&str>,
    unattended: bool,
) -> Result<PathBuf, String> {
    let profiles = discover_firefox_profiles();

    match extend_arg {
        Some(value) if !value.is_empty() => {
            let as_path = PathBuf::from(value);
            if as_path.is_dir() {
                return Ok(as_path);
            }

            for (name, path) in &profiles {
                if name == value {
                    return Ok(path.clone());
                }
            }

            let profiles_dir = crate::profile::firefox_root()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join("Profiles");
            let candidate = profiles_dir.join(value);
            if candidate.is_dir() {
                return Ok(candidate);
            }

            Err(format!(
                "could not find Firefox profile '{}'. Available: {}",
                value,
                if profiles.is_empty() {
                    "(none found)".to_string()
                } else {
                    profiles
                        .iter()
                        .map(|(n, _)| n.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            ))
        }
        _ => {
            if profiles.is_empty() {
                return Err(
                    "no existing Firefox profiles found to extend from".into(),
                );
            }
            if profiles.len() == 1 {
                return Ok(profiles[0].1.clone());
            }
            if unattended {
                return Ok(profiles[0].1.clone());
            }

            prompt_extend_source(&profiles)
        }
    }
}

fn prompt_extend_source(profiles: &[(String, PathBuf)]) -> Result<PathBuf, String> {
    use dialoguer::{theme::ColorfulTheme, Select};

    let items: Vec<String> = profiles
        .iter()
        .map(|(name, path)| {
            format!(
                "{} ({})",
                name,
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            )
        })
        .collect();

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Which Firefox profile should SensibleFox extend?")
        .default(0)
        .items(&items)
        .interact()
        .map_err(|e| format!("selection failed: {e}"))?;

    Ok(profiles[selection].1.clone())
}

/// Prompt interactively for what data to extend.
pub fn prompt_selections() -> Result<Vec<ExtendSelection>, String> {
    use dialoguer::{theme::ColorfulTheme, MultiSelect};

    let items = vec![
        ("Bookmarks", ExtendSelection::Bookmarks, true),
        ("History", ExtendSelection::History, true),
        ("Passwords", ExtendSelection::Passwords, true),
        ("Extensions", ExtendSelection::Extensions, true),
        ("Cookies", ExtendSelection::Cookies, false),
    ];

    let labels: Vec<&str> = items.iter().map(|(l, _, _)| *l).collect();
    let defaults: Vec<bool> = items.iter().map(|(_, _, d)| *d).collect();

    let selections = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select data to import (Space to toggle, Enter to confirm)")
        .items(&labels)
        .defaults(&defaults)
        .interact()
        .map_err(|e| format!("selection failed: {e}"))?;

    let mut out = Vec::new();
    for idx in selections {
        out.push(items[idx].1);
    }
    
    if out.is_empty() {
        return Err("no data selected to extend".into());
    }

    Ok(out)
}

/// Copy selected data from `source_profile` into `dest_profile`.
pub fn apply_extend(
    source: &Path,
    dest: &Path,
    selections: &[ExtendSelection],
) -> Result<(), String> {
    println!(
        "  {} Extending from {}",
        style("→").blue().bold(),
        style(source.display()).cyan()
    );

    for sel in selections {
        match sel {
            ExtendSelection::Bookmarks => copy_bookmarks(source, dest)?,
            ExtendSelection::History => copy_history(source, dest)?,
            ExtendSelection::Passwords => copy_passwords(source, dest)?,
            ExtendSelection::Extensions => copy_extensions(source, dest)?,
            ExtendSelection::Cookies => copy_cookies(source, dest)?,
        }
    }

    let labels: Vec<&str> = selections.iter().map(|s| s.label()).collect();
    println!(
        "  {} Extended profile with: {}",
        style("✓").green(),
        labels.join(", ")
    );
    Ok(())
}

/// Bookmarks & history live together in `places.sqlite`.
/// We also grab the `bookmarkbackups/` folder.
fn copy_bookmarks(source: &Path, dest: &Path) -> Result<(), String> {
    copy_if_exists(source, dest, "places.sqlite")?;
    copy_dir_if_exists(source, dest, "bookmarkbackups")?;
    copy_if_exists(source, dest, "favicons.sqlite")?;
    Ok(())
}

/// History is in `places.sqlite` — same file as bookmarks. If bookmarks
/// already copied it, this is a no-op (the file is identical).
fn copy_history(source: &Path, dest: &Path) -> Result<(), String> {
    copy_if_exists(source, dest, "places.sqlite")?;
    copy_if_exists(source, dest, "favicons.sqlite")?;
    Ok(())
}

/// Passwords require both `logins.json` (encrypted credentials) and
/// `key4.db` (the NSS key database that decrypts them).
fn copy_passwords(source: &Path, dest: &Path) -> Result<(), String> {
    let logins = source.join("logins.json");
    let key_db = source.join("key4.db");
    if logins.exists() && key_db.exists() {
        copy_file(&logins, &dest.join("logins.json"))?;
        copy_file(&key_db, &dest.join("key4.db"))?;
        copy_if_exists(source, dest, "cert9.db")?;
    } else if logins.exists() || key_db.exists() {
        eprintln!(
            "  {} Skipping passwords: both logins.json and key4.db are required",
            style("!").yellow()
        );
    }
    Ok(())
}

/// Extensions: copy the `extensions/` directory (XPI files) and let
/// Firefox rebuild its own `extensions.json` registry on first launch.
/// Also copy `extension-preferences.json` for per-extension settings.
///
/// We deliberately do NOT copy `extensions.json` because it contains
/// hardcoded absolute paths to the source profile. Deleting it forces
/// Firefox to re-scan the extensions/ directory and regenerate correct
/// paths for the destination profile.
fn copy_extensions(source: &Path, dest: &Path) -> Result<(), String> {
    let src_ext = source.join("extensions");
    if !src_ext.is_dir() {
        return Ok(());
    }

    let dest_ext = dest.join("extensions");
    fs::create_dir_all(&dest_ext)
        .map_err(|e| format!("failed to create extensions dir: {e}"))?;

    if let Ok(entries) = fs::read_dir(&src_ext) {
        for entry in entries.flatten() {
            let src_path = entry.path();
            let file_name = entry.file_name();
            let dest_path = dest_ext.join(&file_name);

            if crate::paths::UBLOCK_ID == file_name.to_string_lossy().trim_end_matches(".xpi") {
                continue;
            }

            if src_path.is_file() {
                copy_file(&src_path, &dest_path)?;
            } else if src_path.is_dir() {
                copy_dir_recursive(&src_path, &dest_path)?;
            }
        }
    }

    copy_if_exists(source, dest, "extension-preferences.json")?;
    copy_if_exists(source, dest, "extension-settings.json")?;
    copy_if_exists(source, dest, "addons.json")?;

    let dest_ext_json = dest.join("extensions.json");
    if dest_ext_json.exists() {
        let _ = fs::remove_file(&dest_ext_json);
    }

    copy_extension_storage(source, dest)?;

    Ok(())
}

/// Copy per-extension local storage from `storage/default/moz-extension+++*`.
fn copy_extension_storage(source: &Path, dest: &Path) -> Result<(), String> {
    let src_storage = source.join("storage").join("default");
    if !src_storage.is_dir() {
        return Ok(());
    }

    let dest_storage = dest.join("storage").join("default");

    if let Ok(entries) = fs::read_dir(&src_storage) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.starts_with("moz-extension") {
                continue;
            }
            let src_path = entry.path();
            if !src_path.is_dir() {
                continue;
            }
            let dest_path = dest_storage.join(&name);
            fs::create_dir_all(dest_path.parent().unwrap_or(&dest_storage))
                .map_err(|e| format!("failed to create storage dir: {e}"))?;
            copy_dir_recursive(&src_path, &dest_path)?;
        }
    }

    Ok(())
}

fn copy_cookies(source: &Path, dest: &Path) -> Result<(), String> {
    copy_if_exists(source, dest, "cookies.sqlite")?;
    Ok(())
}

fn copy_if_exists(source_dir: &Path, dest_dir: &Path, filename: &str) -> Result<(), String> {
    let src = source_dir.join(filename);
    if src.exists() {
        let dest = dest_dir.join(filename);
        copy_file(&src, &dest)?;
    }
    Ok(())
}

fn copy_file(src: &Path, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create parent dir: {e}"))?;
    }
    fs::copy(src, dest)
        .map_err(|e| format!("failed to copy {} → {}: {e}", src.display(), dest.display()))?;
    Ok(())
}

fn copy_dir_if_exists(source_dir: &Path, dest_dir: &Path, dirname: &str) -> Result<(), String> {
    let src = source_dir.join(dirname);
    if src.is_dir() {
        let dest = dest_dir.join(dirname);
        copy_dir_recursive(&src, &dest)?;
    }
    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest)
        .map_err(|e| format!("failed to create {}: {e}", dest.display()))?;

    let entries =
        fs::read_dir(src).map_err(|e| format!("failed to read {}: {e}", src.display()))?;

    for entry in entries.flatten() {
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            copy_file(&src_path, &dest_path)?;
        }
    }
    Ok(())
}
