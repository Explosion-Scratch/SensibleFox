fn main() {
    let path = std::env::var("SENSIBLEFOX_FIREFOX_PATH")
        .unwrap_or_else(|_| "/Applications/Firefox.app".to_string());
    println!("cargo:rustc-env=SENSIBLEFOX_FIREFOX_PATH={}", path);
    println!("cargo:rerun-if-env-changed=SENSIBLEFOX_FIREFOX_PATH");
}
