fn main() {
    // ScreenCaptureKit's Swift bridge links libswift_Concurrency through
    // @rpath. On supported macOS versions, the matching runtime is supplied
    // by the OS at /usr/lib/swift; make that location available to both the
    // development executable and the bundled app.
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
        println!("cargo:rustc-link-lib=framework=Foundation");
        cc::Build::new()
            .file("src/now_playing_macos.m")
            .flag("-fblocks")
            .compile("sonora_now_playing");
    }

    tauri_build::build()
}
