use console::style;
use std::fs;
use std::path::{Path, PathBuf};

const PROFILE_NAME: &str = "sensiblefox";

pub fn firefox_root() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join("Library/Application Support/Firefox"))
}

pub fn default_profile_path() -> PathBuf {
    firefox_root()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("Profiles")
        .join(PROFILE_NAME)
}

pub fn create(profile_path: &Path) {
    fs::create_dir_all(profile_path).expect("failed to create profile directory");
    fs::create_dir_all(profile_path.join("chrome")).expect("failed to create chrome directory");
    fs::create_dir_all(profile_path.join("extensions")).expect("failed to create extensions directory");

    println!("  {} Profile directory created", style("✓").green());
}

pub fn register_default(profile_path: &Path) {
    let Some(root) = firefox_root() else { return };
    let Ok(rel) = profile_path.strip_prefix(&root) else {
        return;
    };
    let rel_str = rel.to_string_lossy().replace('\\', "/");

    fs::create_dir_all(&root).ok();

    let profiles_ini = root.join("profiles.ini");
    let mut ini = read_ini(&profiles_ini);
    upsert_profile(&mut ini, &rel_str);
    point_installs_at(&mut ini, &rel_str);
    ensure_general(&mut ini);
    fs::write(&profiles_ini, write_ini(&ini)).ok();

    let installs_ini = root.join("installs.ini");
    if installs_ini.exists() {
        let mut ini = read_ini(&installs_ini);
        point_installs_at(&mut ini, &rel_str);
        fs::write(&installs_ini, write_ini(&ini)).ok();
    }

    println!("  {} Set as default profile in profiles.ini", style("✓").green());
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
            !(name.starts_with("Profile") && get(kv, "Path").map_or(false, |p| p == rel_str))
        });
        for (name, kv) in ini.sections.iter_mut() {
            if name.starts_with("Install") && get(kv, "Default").map_or(false, |p| p == rel_str) {
                remove(kv, "Default");
                remove(kv, "Locked");
            }
        }
        renumber_profiles(&mut ini);
        fs::write(&profiles_ini, write_ini(&ini)).ok();
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
        let is_ours = get(kv, "Path").map_or(false, |p| p == rel_path);
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
        .filter_map(|(n, _)| n.strip_prefix("Profile").and_then(|s| s.parse::<usize>().ok()))
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
