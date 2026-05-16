use console::style;
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
