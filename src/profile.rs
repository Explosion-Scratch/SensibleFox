use console::style;
use std::fs;
use std::path::Path;

pub fn create(profile_path: &Path) {
    fs::create_dir_all(profile_path).expect("failed to create profile directory");
    fs::create_dir_all(profile_path.join("chrome")).expect("failed to create chrome directory");
    fs::create_dir_all(profile_path.join("extensions")).expect("failed to create extensions directory");

    println!(
        "  {} Profile directory created",
        style("✓").green()
    );
}
