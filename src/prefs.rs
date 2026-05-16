use console::style;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

const USER_JS: &str = include_str!("../generated/user.js");

pub fn write(profile_path: &Path) -> Result<(), String> {
    let user_js_path = profile_path.join("user.js");
    fs::write(&user_js_path, USER_JS).map_err(|e| format!("failed to write user.js: {e}"))?;

    let pref_count = USER_JS
        .lines()
        .filter(|l| l.trim().starts_with("user_pref("))
        .count();
    println!(
        "  {} Wrote {} preferences to user.js",
        style("✓").green(),
        pref_count
    );
    Ok(())
}

fn user_pref_key_from_line(line: &str) -> Option<&str> {
    let t = line.trim_start();
    if t.starts_with("//") {
        return None;
    }
    let start = t.find("user_pref(")?;
    let mut i = start + "user_pref(".len();
    let bytes = t.as_bytes();
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if bytes.get(i) != Some(&b'"') {
        return None;
    }
    i += 1;
    let key_start = i;
    while i < bytes.len() && bytes[i] != b'"' {
        i += 1;
    }
    if i <= key_start {
        return None;
    }
    t.get(key_start..i)
}

pub fn collect_user_pref_keys_from_user_js(src: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    for line in src.lines() {
        if let Some(k) = user_pref_key_from_line(line) {
            out.insert(k.to_string());
        }
    }
    out
}

pub fn strip_prefs_js_for_user_js_keys(profile_path: &Path) -> Result<usize, String> {
    let keys = collect_user_pref_keys_from_user_js(USER_JS);
    if keys.is_empty() {
        return Ok(0);
    }
    let prefs_js = profile_path.join("prefs.js");
    if !prefs_js.is_file() {
        return Ok(0);
    }
    let text =
        fs::read_to_string(&prefs_js).map_err(|e| format!("failed to read prefs.js: {e}"))?;
    let mut removed = 0usize;
    let mut kept: Vec<&str> = Vec::new();
    for line in text.lines() {
        if let Some(k) = user_pref_key_from_line(line) {
            if keys.contains(k) {
                removed += 1;
                continue;
            }
        }
        kept.push(line);
    }
    if removed == 0 {
        return Ok(0);
    }
    let body = if kept.is_empty() {
        String::new()
    } else {
        format!("{}\n", kept.join("\n"))
    };
    let tmp = prefs_js.with_extension("js.sensiblefox-tmp");
    fs::write(&tmp, body.as_bytes())
        .map_err(|e| format!("failed to write prefs.js temp: {e}"))?;
    fs::rename(&tmp, &prefs_js).map_err(|e| format!("failed to replace prefs.js: {e}"))?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_user_pref_key_skips_comments() {
        assert_eq!(
            user_pref_key_from_line(r#"user_pref("browser.urlbar.suggest.searches", false);"#),
            Some("browser.urlbar.suggest.searches")
        );
        assert_eq!(
            user_pref_key_from_line(r#"//user_pref("foo.bar", true);"#),
            None
        );
    }
}
