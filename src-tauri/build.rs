fn main() {
    tauri_build::build();

    // Windows only: rfd (file/message dialogs) imports TaskDialogIndirect,
    // which exists solely in comctl32 v6. Without an SxS manifest the
    // loader binds v5 and every exe fails at startup with
    // STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139). Embed the manifest in
    // both app binaries and the test harness.
    #[cfg(windows)]
    {
        let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("app.manifest");
        println!("cargo::rerun-if-changed=app.manifest");
        println!(
            "cargo::rustc-link-arg-bins=/MANIFESTINPUT:{}",
            manifest.display()
        );
        println!(
            "cargo::rustc-link-arg-tests=/MANIFESTINPUT:{}",
            manifest.display()
        );
    }
}
