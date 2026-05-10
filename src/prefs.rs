use console::style;
use std::fs;
use std::path::Path;

const USER_JS: &str = include_str!("../generated/user.js");

pub fn write(profile_path: &Path) {
    let user_js_path = profile_path.join("user.js");

    let expected_len = USER_JS.len();
    let pref_count = USER_JS
        .lines()
        .filter(|l| l.trim().starts_with("user_pref("))
        .count();

    match fs::write(&user_js_path, USER_JS) {
        Ok(()) => {
            // Verify the write was complete.
            match fs::read_to_string(&user_js_path) {
                Ok(actual) if actual.len() == expected_len => {
                    println!(
                        "  {} Wrote {} preferences to user.js",
                        style("✓").green(),
                        pref_count
                    );
                }
                Ok(actual) => {
                    eprintln!(
                        "  {} user.js may be truncated (wrote {} of {} bytes)",
                        style("!").yellow(),
                        actual.len(),
                        expected_len
                    );
                }
                Err(e) => {
                    eprintln!(
                        "  {} user.js written but could not verify: {e}",
                        style("!").yellow()
                    );
                }
            }
        }
        Err(e) => {
            eprintln!(
                "  {} Failed to write user.js: {e}",
                style("✗").red()
            );
            eprintln!(
                "    The profile at {} may be incomplete.",
                profile_path.display()
            );
        }
    }
}
