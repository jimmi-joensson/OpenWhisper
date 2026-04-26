use std::path::Path;

fn main() {
    // Embed Swift runtime rpaths into the Tauri binary so dyld can find
    // libswift_Concurrency.dylib (and friends) at runtime. Must be emitted
    // here, not relied on from `core/build.rs` — `cargo:rustc-link-arg`
    // does not propagate across crate boundaries, only `link-lib` and
    // `link-search` do. Mirror of `core/build.rs::swift_runtime_search_paths`.
    #[cfg(target_os = "macos")]
    emit_swift_rpaths();

    #[cfg(target_os = "windows")]
    {
        run_tauri_build_windows();
        return;
    }

    #[cfg(not(target_os = "windows"))]
    tauri_build::build();
}

/// Windows GNU `windres` opens files via the ANSI Win32 API, so any
/// non-ASCII char in `CARGO_MANIFEST_DIR` (e.g. the `ø` in `JimmiJønsson`)
/// makes the icon embed step fail with "can't open icon file". Even when
/// we hand `tauri-build` a 1-line workaround (a short-name path), it runs
/// `dunce::canonicalize` internally and undoes the substitution.
///
/// Workaround: copy `icons/icon.ico` to a guaranteed-ASCII location and
/// point `WindowsAttributes::window_icon_path` at the copy. `dunce` then
/// canonicalizes to a path that's still ASCII, and windres can open it.
#[cfg(target_os = "windows")]
fn run_tauri_build_windows() {
    use tauri_build::{Attributes, WindowsAttributes};

    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .expect("CARGO_MANIFEST_DIR");
    let src_icon = manifest_dir.join("icons").join("icon.ico");

    let attrs = if path_is_ascii(&src_icon) {
        Attributes::new()
    } else {
        let dst_icon = ascii_icon_path();
        if let Some(parent) = dst_icon.parent() {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|e| panic!("create icon staging dir {}: {e}", parent.display()));
        }
        std::fs::copy(&src_icon, &dst_icon).unwrap_or_else(|e| {
            panic!(
                "copy {} -> {}: {e}",
                src_icon.display(),
                dst_icon.display()
            )
        });
        // Re-run if either the source or our copy moves. The copy only
        // changes when src_icon does.
        println!("cargo:rerun-if-changed={}", src_icon.display());
        Attributes::new()
            .windows_attributes(WindowsAttributes::new().window_icon_path(&dst_icon))
    };
    tauri_build::try_build(attrs).expect("tauri-build (windows)");
}

/// True if every byte of the path's UTF-16 representation fits in ASCII
/// (0–127). Anything outside trips the windres ANSI bug.
#[cfg(target_os = "windows")]
fn path_is_ascii(p: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    p.as_os_str().encode_wide().all(|c| c < 0x80)
}

/// Stable ASCII-only location to stage the windres-readable icon. Lives
/// next to the rust toolchain (also forced to ASCII, see
/// `feedback_windows_no_admin.md`). Per-package suffix avoids collisions
/// if other crates adopt the same workaround.
#[cfg(target_os = "windows")]
fn ascii_icon_path() -> std::path::PathBuf {
    let pkg = std::env::var("CARGO_PKG_NAME").unwrap_or_else(|_| "tauri-app".into());
    std::path::PathBuf::from(format!("C:\\rust\\openwhisper-build\\{pkg}-icon.ico"))
}

#[cfg(target_os = "macos")]
fn emit_swift_rpaths() {
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
