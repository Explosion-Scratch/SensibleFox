use crate::paths::{self, PROFILE_NAME};
use naive_cityhash::cityhash64;
use console::style;
use std::fs;
use std::path::{Path, PathBuf};

fn mozilla_install_hash_hex(install_parent_path: &str) -> String {
    let trimmed = install_parent_path.trim_end_matches('/');
    let utf16: Vec<u16> = trimmed.encode_utf16().collect();
    let mut bytes = Vec::with_capacity(utf16.len() * 2);
    for u in utf16 {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    let h: u64 = cityhash64(&bytes);
    format!("{:X}", h)
}

fn install_parent_directory_for_hash(firefox_binary: &Path) -> Option<String> {
    let resolved = fs::canonicalize(firefox_binary).unwrap_or_else(|_| firefox_binary.to_path_buf());
    let parent = resolved.parent()?;
    let mut s = parent.to_string_lossy().replace('\\', "/");
    while s.ends_with('/') && s.len() > 1 {
        s.pop();
    }
    if s.is_empty() {
        return None;
    }
    Some(s)
}

#[cfg(unix)]
fn effective_uid() -> u32 {
    extern "C" {
        fn geteuid() -> u32;
    }
    unsafe { geteuid() }
}

pub fn user_home() -> Option<PathBuf> {
    #[cfg(unix)]
    if effective_uid() != 0 {
        return dirs::home_dir();
    }
    #[cfg(not(unix))]
    {
        return dirs::home_dir();
    }

    if let Ok(sudo_user) = std::env::var("SUDO_USER") {
        if !sudo_user.is_empty() && sudo_user != "root" {
            let candidate = PathBuf::from(format!("/Users/{sudo_user}"));
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }
    dirs::home_dir()
}

pub fn firefox_root() -> Option<PathBuf> {
    user_home().map(|h| h.join(paths::FIREFOX_ROOT_REL))
}

pub fn default_profile_path() -> PathBuf {
    firefox_root()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("Profiles")
        .join(PROFILE_NAME)
}

pub fn is_sensiblefox_profile(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else { return false };
    name == PROFILE_NAME || name.starts_with(PROFILE_NAME) || name.ends_with(".sensiblefox")
}

pub fn discover_profiles() -> Vec<PathBuf> {
    let Some(root) = firefox_root() else { return Vec::new() };
    let profiles_dir = root.join("Profiles");
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(&profiles_dir) {
        for entry in rd.flatten() {
            let p = entry.path();
            if is_sensiblefox_profile(&p) {
                out.push(p);
            }
        }
    }
    out
}

pub fn create(profile_path: &Path) -> Result<(), String> {
    // Basic sanity check — refuse to create profiles in obviously wrong places.
    let path_str = profile_path.to_string_lossy();
    if path_str == "/" || path_str == "/System" || path_str.starts_with("/System/") {
        return Err(format!(
            "refusing to create profile at system path: {}",
            profile_path.display()
        ));
    }

    fs::create_dir_all(profile_path)
        .map_err(|e| format!("failed to create profile directory: {e}"))?;

    let chrome_dir = profile_path.join("chrome");
    fs::create_dir_all(&chrome_dir)
        .map_err(|e| format!("failed to create chrome directory: {e}"))?;

    let ext_dir = profile_path.join("extensions");
    fs::create_dir_all(&ext_dir)
        .map_err(|e| format!("failed to create extensions directory: {e}"))?;

    println!("  {} Profile directory created", style("✓").green());
    Ok(())
}

pub fn register_default(profile_path: &Path, firefox_binary: Option<&Path>) {
    let Some(root) = firefox_root() else { return };
    let Ok(rel) = profile_path.strip_prefix(&root) else {
        eprintln!(
            "  {} Profile path is outside Firefox root — cannot register as default",
            style("!").yellow()
        );
        return;
    };
    let rel_str = rel.to_string_lossy().replace('\\', "/");

    if let Err(e) = fs::create_dir_all(&root) {
        eprintln!(
            "  {} Cannot create Firefox root directory: {e}",
            style("!").yellow()
        );
        return;
    }

    let profiles_ini = root.join("profiles.ini");
    let mut ini = read_ini(&profiles_ini);
    upsert_profile(&mut ini, &rel_str);
    if let Some(bin) = firefox_binary {
        if let Some(parent) = install_parent_directory_for_hash(bin) {
            let hash_hex = mozilla_install_hash_hex(&parent);
            ensure_install_section(&mut ini, &hash_hex);
        } else {
            eprintln!(
                "  {} Could not resolve Firefox install directory for profile hash",
                style("!").yellow()
            );
        }
    }
    point_installs_at(&mut ini, &rel_str);
    ensure_general(&mut ini);
    if let Err(e) = fs::write(&profiles_ini, write_ini(&ini)) {
        eprintln!(
            "  {} Failed to write profiles.ini: {e}",
            style("!").yellow()
        );
        return;
    }

    if firefox_binary.is_some() {
        write_install_sections_mirror(&root, &ini);
    }

    println!(
        "  {} Set as default profile in profiles.ini",
        style("✓").green()
    );
}

pub fn unregister(profile_path: &Path) {
    let Some(root) = firefox_root() else { return };
    let Ok(rel) = profile_path.strip_prefix(&root) else {
        return;
    };
    let rel_str = rel.to_string_lossy().replace('\\', "/");

    let profiles_ini = root.join("profiles.ini");
    if profiles_ini.exists() {
        let mut ini = read_ini(&profiles_ini);
        ini.sections.retain(|(name, kv)| {
            !(name.starts_with("Profile") && get(kv, "Path").is_some_and(|p| p == rel_str))
        });
        for (name, kv) in ini.sections.iter_mut() {
            if name.starts_with("Install") && get(kv, "Default").is_some_and(|p| p == rel_str) {
                remove(kv, "Default");
                remove(kv, "Locked");
            }
        }
        renumber_profiles(&mut ini);
        if let Err(e) = fs::write(&profiles_ini, write_ini(&ini)) {
            eprintln!(
                "  {} Failed to update profiles.ini: {e}",
                style("!").yellow()
            );
        }
    }
}

type Section = (String, Vec<(String, String)>);

struct Ini {
    sections: Vec<Section>,
}

fn read_ini(path: &Path) -> Ini {
    let text = fs::read_to_string(path).unwrap_or_default();
    let mut sections: Vec<Section> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            sections.push((line[1..line.len() - 1].to_string(), Vec::new()));
        } else if let Some((k, v)) = line.split_once('=') {
            if let Some(last) = sections.last_mut() {
                last.1.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
    }
    Ini { sections }
}

fn write_ini(ini: &Ini) -> String {
    let mut out = String::new();
    for (i, (name, kv)) in ini.sections.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&format!("[{}]\n", name));
        for (k, v) in kv {
            out.push_str(&format!("{}={}\n", k, v));
        }
    }
    out
}

fn ensure_install_section(ini: &mut Ini, hash_hex: &str) {
    let name = format!("Install{}", hash_hex);
    if ini.sections.iter().any(|(n, _)| n == &name) {
        return;
    }
    ini.sections.push((name, Vec::new()));
}

fn write_install_sections_mirror(root: &Path, ini: &Ini) {
    let filtered = Ini {
        sections: ini
            .sections
            .iter()
            .filter(|(name, _)| name.starts_with("Install"))
            .cloned()
            .collect(),
    };
    if filtered.sections.is_empty() {
        return;
    }
    let installs_ini = root.join("installs.ini");
    if let Err(e) = fs::write(&installs_ini, write_ini(&filtered)) {
        eprintln!(
            "  {} Failed to write installs.ini: {e}",
            style("!").yellow()
        );
    }
}

fn get<'a>(kv: &'a [(String, String)], key: &str) -> Option<&'a str> {
    kv.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn set(kv: &mut Vec<(String, String)>, key: &str, value: &str) {
    if let Some(entry) = kv.iter_mut().find(|(k, _)| k == key) {
        entry.1 = value.to_string();
    } else {
        kv.push((key.to_string(), value.to_string()));
    }
}

fn remove(kv: &mut Vec<(String, String)>, key: &str) {
    kv.retain(|(k, _)| k != key);
}

fn upsert_profile(ini: &mut Ini, rel_path: &str) {
    let mut found = false;
    for (name, kv) in ini.sections.iter_mut() {
        if !name.starts_with("Profile") {
            continue;
        }
        let is_ours = get(kv, "Path") == Some(rel_path);
        if is_ours {
            set(kv, "Name", PROFILE_NAME);
            set(kv, "IsRelative", "1");
            set(kv, "Path", rel_path);
            set(kv, "Default", "1");
            found = true;
        } else {
            remove(kv, "Default");
        }
    }
    if !found {
        let idx = next_profile_index(ini);
        ini.sections.push((
            format!("Profile{}", idx),
            vec![
                ("Name".into(), PROFILE_NAME.into()),
                ("IsRelative".into(), "1".into()),
                ("Path".into(), rel_path.into()),
                ("Default".into(), "1".into()),
            ],
        ));
    }
}

fn next_profile_index(ini: &Ini) -> usize {
    ini.sections
        .iter()
        .filter_map(|(n, _)| {
            n.strip_prefix("Profile")
                .and_then(|s| s.parse::<usize>().ok())
        })
        .max()
        .map(|m| m + 1)
        .unwrap_or(0)
}

fn renumber_profiles(ini: &mut Ini) {
    let mut idx = 0;
    for (name, _) in ini.sections.iter_mut() {
        if name.starts_with("Profile") {
            *name = format!("Profile{}", idx);
            idx += 1;
        }
    }
}

fn ensure_general(ini: &mut Ini) {
    let general = ini.sections.iter_mut().find(|(n, _)| n == "General");
    if let Some((_, kv)) = general {
        if get(kv, "StartWithLastProfile").is_none() {
            set(kv, "StartWithLastProfile", "1");
        }
        if get(kv, "Version").is_none() {
            set(kv, "Version", "2");
        }
    } else {
        ini.sections.insert(
            0,
            (
                "General".into(),
                vec![
                    ("StartWithLastProfile".into(), "1".into()),
                    ("Version".into(), "2".into()),
                ],
            ),
        );
    }
}

fn point_installs_at(ini: &mut Ini, rel_path: &str) {
    for (name, kv) in ini.sections.iter_mut() {
        if !name.starts_with("Install") {
            continue;
        }
        set(kv, "Default", rel_path);
        set(kv, "Locked", "1");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mozilla_install_hash_matches_public_vectors() {
        assert_eq!(
            mozilla_install_hash_hex("/Applications/Firefox.app/Contents/MacOS"),
            "2656FF1E876E9973"
        );
        assert_eq!(
            mozilla_install_hash_hex("/usr/lib/firefox"),
            "4F96D1932A9F858E"
        );
        assert_eq!(
            mozilla_install_hash_hex("/opt/firefox"),
            "6AFDA46A1A8AD48"
        );
    }
}
