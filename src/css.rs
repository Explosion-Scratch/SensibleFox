use console::style;
use std::fs;
use std::io::Write;
use std::path::Path;

const MACOS_TABBAR: &str = include_str!("../assets/macos-native-tabbar.css");
const UBLOCK_ICON: &str = include_str!("../assets/ublock_icon_change.css");
const CLEANER_EXTENSIONS: &str = include_str!("../assets/cleaner_extensions_menu.css");
const NO_SEARCH_ENGINES: &str = include_str!("../assets/no_search_engines_in_url_bar.css");
const PRIVACY_EMAIL: &str = include_str!("../assets/privacy_change_email_text.css");
const SEARCHBAR_DOTS: &str = include_str!("../assets/show_searchbar_dots_only_on_hover.css");
const CONTEXT_MENU_CLEANUP: &str = include_str!("../assets/context_menu_cleanup.css");

pub fn write(profile_path: &Path) -> Result<(), String> {
    let chrome_dir = profile_path.join("chrome");
    fs::create_dir_all(&chrome_dir)
        .map_err(|e| format!("failed to create chrome directory: {e}"))?;

    let css_dir = chrome_dir.join("css");
    fs::create_dir_all(&css_dir).map_err(|e| format!("failed to create css directory: {e}"))?;

    let css_modules: &[(&str, &str)] = &[
        ("macos-native-tabbar.css", MACOS_TABBAR),
        ("ublock-icon-change.css", UBLOCK_ICON),
        ("cleaner-extensions-menu.css", CLEANER_EXTENSIONS),
        ("no-search-engines-urlbar.css", NO_SEARCH_ENGINES),
        ("privacy-email-text.css", PRIVACY_EMAIL),
        ("searchbar-dots-hover.css", SEARCHBAR_DOTS),
        ("context-menu-cleanup.css", CONTEXT_MENU_CLEANUP),
    ];

    let mut written = 0usize;
    for (name, content) in css_modules {
        let path = css_dir.join(name);
        fs::write(&path, content).map_err(|e| format!("failed to write {name}: {e}"))?;
        match fs::read_to_string(&path) {
            Ok(actual) if actual == *content => {}
            Ok(_) => return Err(format!("{name} verification failed: content mismatch")),
            Err(e) => return Err(format!("{name} written but could not verify: {e}")),
        }
        written += 1;
    }

    let user_chrome_path = chrome_dir.join("userChrome.css");
    let mut user_chrome = fs::File::create(&user_chrome_path)
        .map_err(|e| format!("failed to create userChrome.css: {e}"))?;

    writeln!(user_chrome, "/* sensiblefox — userChrome.css */")
        .map_err(|e| format!("failed to write userChrome.css: {e}"))?;
    writeln!(user_chrome, "/* Auto-generated. Do not edit manually. */\n")
        .map_err(|e| format!("failed to write userChrome.css: {e}"))?;

    for (name, _) in css_modules {
        writeln!(user_chrome, "@import url(\"css/{name}\");")
            .map_err(|e| format!("failed to write userChrome.css: {e}"))?;
    }
    drop(user_chrome);

    let user_chrome_text = fs::read_to_string(&user_chrome_path)
        .map_err(|e| format!("userChrome.css written but could not verify: {e}"))?;
    for (name, _) in css_modules {
        let import = format!("@import url(\"css/{name}\");");
        if !user_chrome_text.contains(&import) {
            return Err(format!("userChrome.css is missing import for {name}"));
        }
    }

    println!(
        "  {} Wrote {} CSS modules + userChrome.css",
        style("✓").green(),
        written
    );
    Ok(())
}
