fn main() {
    // `native_surface` marks targets with the native terminal surface
    // (docs/renderer.md). One list here; every gate in the crate
    // references the cfg, so adding a platform is a one-line change.
    println!("cargo::rustc-check-cfg=cfg(native_surface)");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" {
        println!("cargo::rustc-cfg=native_surface");
    }
    // Tauri puts its Windows manifest, which asks for Common Controls 6,
    // only into the app binary. The unit test binary links the mock
    // runtime too, and without that manifest Windows refuses to start it
    // (STATUS_ENTRYPOINT_NOT_FOUND). So with the MSVC linker the same
    // manifest goes into every binary through the linker instead.
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
        let manifest = std::path::Path::new(&dir).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
        tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
            .expect("failed to run the tauri build script");
    } else {
        tauri_build::build();
    }
}
