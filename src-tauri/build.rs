fn main() {
    // `native_surface` marks targets with a Tier 3 native terminal surface
    // (docs/native-renderer.md). One list here; every gate in the crate
    // references the cfg, so adding a platform is a one-line change.
    println!("cargo::rustc-check-cfg=cfg(native_surface)");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" || target_os == "windows" || target_os == "linux" {
        println!("cargo::rustc-cfg=native_surface");
    }
    tauri_build::build();
}
