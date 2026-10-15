fn main() {
    // Require Administrator privileges on Windows: a UAC prompt appears on
    // every launch. Diagnostics and repairs (powercfg, HKLM registry,
    // DISM/SFC) need elevation to work for standard users. The manifest goes
    // through tauri-build so it lands in the single resource object it
    // already embeds (separate winres compilation would duplicate resources).
    println!("cargo:rerun-if-changed=app.manifest");
    let windows =
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest"));
    let attrs = tauri_build::Attributes::new().windows_attributes(windows);
    tauri_build::try_build(attrs).expect("failed to run tauri build script");
}
