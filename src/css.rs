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

pub fn write(profile_path: &Path) {
    let chrome_dir = profile_path.join("chrome");
    fs::create_dir_all(&chrome_dir).expect("failed to create chrome directory");

    let css_dir = chrome_dir.join("css");
    fs::create_dir_all(&css_dir).expect("failed to create css directory");

    let css_modules: &[(&str, &str)] = &[
        ("macos-native-tabbar.css", MACOS_TABBAR),
        ("ublock-icon-change.css", UBLOCK_ICON),
        ("cleaner-extensions-menu.css", CLEANER_EXTENSIONS),
        ("no-search-engines-urlbar.css", NO_SEARCH_ENGINES),
        ("privacy-email-text.css", PRIVACY_EMAIL),
        ("searchbar-dots-hover.css", SEARCHBAR_DOTS),
        ("context-menu-cleanup.css", CONTEXT_MENU_CLEANUP),
    ];

    for (name, content) in css_modules {
        fs::write(css_dir.join(name), content).expect("failed to write CSS module");
    }

    let mut user_chrome = fs::File::create(chrome_dir.join("userChrome.css"))
        .expect("failed to create userChrome.css");

    writeln!(user_chrome, "/* sensiblefox — userChrome.css */").unwrap();
    writeln!(user_chrome, "/* Auto-generated. Do not edit manually. */\n").unwrap();

    for (name, _) in css_modules {
        writeln!(user_chrome, "@import url(\"css/{name}\");").unwrap();
    }

    println!(
        "  {} Wrote {} CSS modules + userChrome.css",
        style("✓").green(),
        css_modules.len()
    );
}
