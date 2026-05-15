use console::style;
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

const POLICIES_JSON: &str = include_str!("../assets/policies.json");
const UBLOCK_MANAGED_STORAGE: &str = include_str!("../assets/uBlock0@raymondhill.net.json");

pub fn apply_macos_policies() -> Result<(), String> {
    let policies = parse_policy_map()?;
    let plist = firefox_plist(&policies)?;
    
    let path = if crate::is_root() {
        PathBuf::from("/Library/Preferences/org.mozilla.firefox.plist")
    } else {
        crate::profile::user_home().unwrap_or_default().join("Library/Preferences/org.mozilla.firefox.plist")
    };

    write_root_file(&path, plist.as_bytes(), 0o644)?;
    validate_plist(&path)?;

    println!(
        "  {} Firefox policies configured in {}",
        style("✓").green(),
        style(path.display()).cyan()
    );

    Ok(())
}

fn validate_plist(path: &Path) -> Result<(), String> {
    let output = std::process::Command::new("plutil")
        .args(["-lint", "-s"])
        .arg(path)
        .output()
        .map_err(|e| format!("failed to run plutil: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!(
            "installed policy plist failed validation: {}",
            if stderr.is_empty() {
                "plutil reported invalid plist"
            } else {
                &stderr
            }
        ))
    }
}

fn parse_policy_map() -> Result<Map<String, Value>, String> {
    let root: Value = serde_json::from_str(POLICIES_JSON)
        .map_err(|e| format!("assets/policies.json is not valid JSON: {e}"))?;

    root.get("policies")
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| "assets/policies.json must contain a top-level policies object".to_string())
}

fn firefox_plist(policies: &Map<String, Value>) -> Result<String, String> {
    let mut out = String::new();
    out.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    out.push('\n');
    out.push_str(r#"<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">"#);
    out.push('\n');
    out.push_str(r#"<plist version="1.0">"#);
    out.push('\n');
    out.push_str("<dict>\n");
    out.push_str("  <key>EnterprisePoliciesEnabled</key>\n");
    out.push_str("  <true/>\n");

    for (key, value) in policies {
        write_key_value(&mut out, key, value, 1)?;
    }

    out.push_str("</dict>\n</plist>\n");
    Ok(out)
}

fn write_key_value(out: &mut String, key: &str, value: &Value, depth: usize) -> Result<(), String> {
    let indent = "  ".repeat(depth);
    out.push_str(&format!("{indent}<key>{}</key>\n", escape_xml(key)));
    write_value(out, value, depth)
}

fn write_value(out: &mut String, value: &Value, depth: usize) -> Result<(), String> {
    let indent = "  ".repeat(depth);
    match value {
        Value::Bool(true) => out.push_str(&format!("{indent}<true/>\n")),
        Value::Bool(false) => out.push_str(&format!("{indent}<false/>\n")),
        Value::String(s) => out.push_str(&format!("{indent}<string>{}</string>\n", escape_xml(s))),
        Value::Number(n) if n.is_i64() => {
            out.push_str(&format!("{indent}<integer>{n}</integer>\n"))
        }
        Value::Number(n) if n.is_u64() => {
            out.push_str(&format!("{indent}<integer>{n}</integer>\n"))
        }
        Value::Number(n) if n.is_f64() => out.push_str(&format!("{indent}<real>{n}</real>\n")),
        Value::Array(values) => {
            out.push_str(&format!("{indent}<array>\n"));
            for item in values {
                write_value(out, item, depth + 1)?;
            }
            out.push_str(&format!("{indent}</array>\n"));
        }
        Value::Object(map) => {
            out.push_str(&format!("{indent}<dict>\n"));
            for (key, item) in map {
                write_key_value(out, key, item, depth + 1)?;
            }
            out.push_str(&format!("{indent}</dict>\n"));
        }
        Value::Null => {
            return Err("Firefox macOS policy plist cannot contain null values".to_string());
        }
        _ => return Err(format!("unsupported policy value: {value}")),
    }
    Ok(())
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn apply_system_managed_storage() -> Result<(), String> {
    let dir = if crate::is_root() {
        PathBuf::from("/Library/Application Support/Mozilla/ManagedStorage")
    } else {
        crate::profile::user_home().unwrap_or_default().join("Library/Application Support/Mozilla/ManagedStorage")
    };
    let path = dir.join("uBlock0@raymondhill.net.json");

    write_root_file(&path, UBLOCK_MANAGED_STORAGE.as_bytes(), 0o644)?;

    println!(
        "  {} System-wide uBlock managed storage configured",
        style("✓").green()
    );

    Ok(())
}

fn write_root_file(path: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("invalid output path: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;

    let tmp = temp_path(path);
    fs::write(&tmp, bytes).map_err(|e| format!("failed to write {}: {e}", tmp.display()))?;

    set_mode(&tmp, mode)?;

    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("failed to install {}: {e}", path.display()));
    }

    set_mode(path, mode)?;
    Ok(())
}

fn temp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("sensiblefox.tmp");
    path.with_file_name(format!(".{file_name}.tmp"))
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .map_err(|e| format!("failed to stat {}: {e}", path.display()))?
        .permissions();
    permissions.set_mode(mode);
    fs::set_permissions(path, permissions)
        .map_err(|e| format!("failed to chmod {}: {e}", path.display()))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_policy_plist_contains_supported_root_keys() {
        let policies = parse_policy_map().expect("policies should parse");
        let plist = firefox_plist(&policies).expect("plist should render");

        assert!(plist.contains("<key>EnterprisePoliciesEnabled</key>"));
        assert!(plist.contains("<key>FirefoxHome</key>"));
        assert!(plist.contains("<key>ExtensionSettings</key>"));
        assert!(!plist.contains("<key>policies</key>"));
    }
}
