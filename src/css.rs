use console::style;
use std::fs;
use std::path::Path;

const MODULES: &[(&str, &str)] = &[
    (
        "macos-native-tabbar.css",
        include_str!("../assets/macos-native-tabbar.css"),
    ),
    (
        "ublock-icon-change.css",
        include_str!("../assets/ublock_icon_change.css"),
    ),
    (
        "cleaner-extensions-menu.css",
        include_str!("../assets/cleaner_extensions_menu.css"),
    ),
    (
        "no-search-engines-urlbar.css",
        include_str!("../assets/no_search_engines_in_url_bar.css"),
    ),
    (
        "privacy-email-text.css",
        include_str!("../assets/privacy_change_email_text.css"),
    ),
    (
        "searchbar-dots-hover.css",
        include_str!("../assets/show_searchbar_dots_only_on_hover.css"),
    ),
    (
        "context-menu-cleanup.css",
        include_str!("../assets/context_menu_cleanup.css"),
    ),
];

pub fn write(profile_path: &Path) -> Result<(), String> {
    let chrome_dir = profile_path.join("chrome");
    let css_dir = chrome_dir.join("css");
    fs::create_dir_all(&css_dir).map_err(|e| format!("failed to create css dir: {e}"))?;

    for (name, content) in MODULES {
        fs::write(css_dir.join(name), content)
            .map_err(|e| format!("failed to write {name}: {e}"))?;
    }

    let mut user_chrome = String::from("/* sensiblefox — userChrome.css (auto-generated) */\n\n");
    for (name, _) in MODULES {
        user_chrome.push_str(&format!("@import url(\"css/{name}\");\n"));
    }
    fs::write(chrome_dir.join("userChrome.css"), user_chrome)
        .map_err(|e| format!("failed to write userChrome.css: {e}"))?;

    println!(
        "  {} Wrote {} CSS modules + userChrome.css",
        style("✓").green(),
        MODULES.len()
    );
    Ok(())
}
