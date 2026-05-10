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

pub fn bundle() -> String {
    let css_modules: &[(&str, &str)] = &[
        ("macos-native-tabbar.css", MACOS_TABBAR),
        ("ublock-icon-change.css", UBLOCK_ICON),
        ("cleaner-extensions-menu.css", CLEANER_EXTENSIONS),
        ("no-search-engines-urlbar.css", NO_SEARCH_ENGINES),
        ("privacy-email-text.css", PRIVACY_EMAIL),
        ("searchbar-dots-hover.css", SEARCHBAR_DOTS),
        ("context-menu-cleanup.css", CONTEXT_MENU_CLEANUP),
    ];

    let mut bundle = String::new();
    bundle.push_str("/* SensibleFox bundled userChrome */\n");
    for (name, content) in css_modules {
        bundle.push_str(&format!("\n/* ── {} ── */\n", name));
        bundle.push_str(content);
        bundle.push('\n');
    }
    bundle
}

pub fn write(profile_path: &Path) {
    let chrome_dir = profile_path.join("chrome");
    if let Err(e) = fs::create_dir_all(&chrome_dir) {
        eprintln!(
            "  {} Failed to create chrome directory: {e}",
            style("✗").red()
        );
        return;
    }

    let css_dir = chrome_dir.join("css");
    if let Err(e) = fs::create_dir_all(&css_dir) {
        eprintln!(
            "  {} Failed to create css directory: {e}",
            style("✗").red()
        );
        return;
    }

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
        if let Err(e) = fs::write(css_dir.join(name), content) {
            eprintln!(
                "  {} Failed to write {name}: {e}",
                style("!").yellow()
            );
        } else {
            written += 1;
        }
    }

    let user_chrome_path = chrome_dir.join("userChrome.css");
    let mut user_chrome = match fs::File::create(&user_chrome_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "  {} Failed to create userChrome.css: {e}",
                style("✗").red()
            );
            return;
        }
    };

    let _ = writeln!(user_chrome, "/* sensiblefox — userChrome.css */");
    let _ = writeln!(user_chrome, "/* Auto-generated. Do not edit manually. */\n");

    for (name, _) in css_modules {
        let _ = writeln!(user_chrome, "@import url(\"css/{name}\");");
    }

    println!(
        "  {} Wrote {} CSS modules + userChrome.css",
        style("✓").green(),
        written
    );
}
