use console::style;
use std::fs;
use std::path::Path;

const USER_JS: &str = include_str!("../generated/user.js");

pub fn write(profile_path: &Path) -> Result<(), String> {
    let user_js_path = profile_path.join("user.js");

    let pref_count = USER_JS
        .lines()
        .filter(|l| l.trim().starts_with("user_pref("))
        .count();

    fs::write(&user_js_path, USER_JS).map_err(|e| format!("failed to write user.js: {e}"))?;

    let required = [
        "user_pref(\"toolkit.legacyUserProfileCustomizations.stylesheets\", true);",
        "user_pref(\"browser.legacyUserProfileCustomizations.stylesheets\", true);",
    ];

    match fs::read_to_string(&user_js_path) {
        Ok(actual) if actual == USER_JS && required.iter().all(|pref| actual.contains(pref)) => {
            println!(
                "  {} Wrote {} preferences to user.js",
                style("✓").green(),
                pref_count
            );
            Ok(())
        }
        Ok(actual) if actual != USER_JS => Err(format!(
            "user.js verification failed (wrote {} bytes, expected {} bytes)",
            actual.len(),
            USER_JS.len()
        )),
        Ok(_) => Err("user.js is missing required userChrome enablement prefs".to_string()),
        Err(e) => Err(format!("user.js written but could not verify: {e}")),
    }
}
