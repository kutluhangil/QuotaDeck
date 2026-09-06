fn main() {
    #[cfg(target_os = "macos")]
    build_widget_shim();
    tauri_build::build();
}

/// Compile the WidgetKit bridge into a static library and link it.
///
/// Swift rather than Rust because `WidgetCenter` has no Objective-C surface to bind to. A
/// missing toolchain stops the build loudly: this is not a production path, and a host binary
/// that silently cannot reload its own widget would be worse than one that refuses to build.
#[cfg(target_os = "macos")]
fn build_widget_shim() {
    use std::process::Command;

    let source = "src/shim/WidgetReload.swift";
    println!("cargo:rerun-if-changed={source}");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo");
    let library = format!("{out_dir}/libquotadeck_widget_shim.a");
    let status = Command::new("swiftc")
        .args([
            "-emit-library",
            "-static",
            "-parse-as-library",
            "-O",
            "-o",
            &library,
            source,
        ])
        .status()
        .expect("swiftc must be available to build the macOS widget shim");
    assert!(status.success(), "swiftc failed to build {source}");
    println!("cargo:rustc-link-search=native={out_dir}");
    println!("cargo:rustc-link-lib=static=quotadeck_widget_shim");
}
