//! Embed Swift runtime rpaths into the CLI binary so dyld can find
//! libswift_Concurrency.dylib at runtime — must be emitted here,
//! not relied on from `core/build.rs`. `cargo:rustc-link-arg` does
//! not propagate across crate boundaries; only `link-lib` and
//! `link-search` do. Mirror of `apps/tauri/src-tauri/build.rs` and
//! `core/build.rs::swift_runtime_search_paths`.

fn main() {
    #[cfg(target_os = "macos")]
    emit_swift_rpaths();
}

#[cfg(target_os = "macos")]
fn emit_swift_rpaths() {
    use std::path::Path;
    let mut paths: Vec<String> = vec![
        "/usr/lib/swift".to_string(),
        "/Library/Developer/CommandLineTools/usr/lib/swift-5.5/macosx".to_string(),
    ];
    if let Some(toolchain) = swift_toolchain_dir() {
        paths.push(format!("{toolchain}/usr/lib/swift/macosx"));
        paths.push(format!("{toolchain}/usr/lib/swift-5.5/macosx"));
    }
    for p in &paths {
        if Path::new(p).is_dir() {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{p}");
        }
    }
}

#[cfg(target_os = "macos")]
fn swift_toolchain_dir() -> Option<String> {
    std::process::Command::new("xcrun")
        .args(["--find", "swift"])
        .output()
        .ok()
        .and_then(|o| {
            if !o.status.success() {
                return None;
            }
            let path = String::from_utf8_lossy(&o.stdout).trim().to_string();
            std::path::Path::new(&path)
                .parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
                .map(|p| p.display().to_string())
        })
}
