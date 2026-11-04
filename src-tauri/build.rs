fn main() {
    tauri_build::build();

    // Windows only: rfd (file dialogs) imports TaskDialogIndirect, which
    // exists only in comctl32 v6. App binaries bind v6 via tauri-build's
    // SxS manifest, but the manifest-less unit-test harness binds v5 and
    // dies at startup (0xc0000139). Delay-loading comctl32 defers import
    // resolution to first call — tests never call it, prod resolves
    // against the manifested v6 at first dialog use.
    #[cfg(windows)]
    {
        println!("cargo::rustc-link-arg=/DELAYLOAD:comctl32.dll");
        println!("cargo::rustc-link-arg=delayimp.lib");
    }
}
