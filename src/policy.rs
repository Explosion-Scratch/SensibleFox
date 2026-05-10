use crate::css;
use console::style;
use std::fs;
use std::path::Path;

const POLICIES_JSON: &str = include_str!("../assets/policies.json");
const AUTOCONFIG_JS: &str = include_str!("../assets/autoconfig.js");
const SENSIBLEFOX_CFG_TAIL: &str = include_str!("../assets/sensiblefox.cfg.tail");
const SENSIBLEFOX_DEFAULTS: &str = include_str!("../generated/sensiblefox-defaults.js");
const UBLOCK_MANAGED_STORAGE: &str = include_str!("../assets/uBlock0@raymondhill.net.json");

pub fn apply_to_app(app_path: &Path) -> Result<(), String> {
    let res_dir = app_path.join("Contents/Resources");
    if !res_dir.exists() {
        return Err(format!(
            "Resources directory not found: {}",
            res_dir.display()
        ));
    }

    println!(
        "  {} Injecting SensibleFox configuration into {}",
        style("↻").cyan(),
        style("Firefox.app").bold()
    );

    // 1 — distribution/policies.json
    let dist_dir = res_dir.join("distribution");
    fs::create_dir_all(&dist_dir).map_err(|e| format!("failed to create distribution dir: {e}"))?;
    fs::write(dist_dir.join("policies.json"), POLICIES_JSON)
        .map_err(|e| format!("failed to write policies.json: {e}"))?;

    // 2 — defaults/pref/autoconfig.js
    let pref_dir = res_dir.join("defaults/pref");
    fs::create_dir_all(&pref_dir).map_err(|e| format!("failed to create defaults/pref dir: {e}"))?;
    fs::write(pref_dir.join("autoconfig.js"), AUTOCONFIG_JS)
        .map_err(|e| format!("failed to write autoconfig.js: {e}"))?;

    // 3 — sensiblefox.cfg (concatenated defaults + tail)
    let mut cfg = String::new();
    cfg.push_str(SENSIBLEFOX_DEFAULTS);
    cfg.push_str("\n\n");
    cfg.push_str(SENSIBLEFOX_CFG_TAIL);
    fs::write(res_dir.join("sensiblefox.cfg"), cfg)
        .map_err(|e| format!("failed to write sensiblefox.cfg: {e}"))?;

    // 4 — sensiblefox/userChrome.css (bundled)
    let sf_res_dir = res_dir.join("sensiblefox");
    fs::create_dir_all(&sf_res_dir).map_err(|e| format!("failed to create sensiblefox res dir: {e}"))?;
    fs::write(sf_res_dir.join("userChrome.css"), css::bundle())
        .map_err(|e| format!("failed to write bundled userChrome.css: {e}"))?;

    println!(
        "  {} App-wide policies and configuration applied",
        style("✓").green()
    );

    Ok(())
}

pub fn apply_system_managed_storage() -> Result<(), String> {
    let dir = Path::new("/Library/Application Support/Mozilla/ManagedStorage");
    let path = dir.join("uBlock0@raymondhill.net.json");

    if let Err(e) = fs::create_dir_all(dir) {
        return Err(format!("failed to create system ManagedStorage dir: {e}"));
    }

    fs::write(&path, UBLOCK_MANAGED_STORAGE.as_bytes())
        .map_err(|e| format!("failed to write system uBO managed storage: {e}"))?;

    println!(
        "  {} System-wide uBlock managed storage configured",
        style("✓").green()
    );

    Ok(())
}
