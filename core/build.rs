use std::path::{Path, PathBuf};

fn main() {
    let crate_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    #[cfg(feature = "macos-shell")]
    run_swift_bridge_codegen(&crate_dir);

    #[cfg(feature = "recognizer")]
    {
        let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
        if target_os == "macos" {
            build_fluidaudio_bridge(&crate_dir);
        }
    }
}

#[cfg(feature = "macos-shell")]
fn run_swift_bridge_codegen(crate_dir: &Path) {
    let repo_root = crate_dir
        .parent()
        .expect("core crate must live under the repo root");
    let out_dir = repo_root.join("apps").join("macos").join("Generated");

    let bridges = vec!["src/lib.rs"];
    for path in &bridges {
        println!("cargo:rerun-if-changed={}", path);
    }

    swift_bridge_build::parse_bridges(bridges)
        .write_all_concatenated(&out_dir, "openwhisper_core");
}

#[cfg(feature = "recognizer")]
fn build_fluidaudio_bridge(crate_dir: &Path) {
    let pkg_dir = crate_dir.join("swift").join("FluidAudioBridge");

    println!("cargo:rerun-if-changed={}", pkg_dir.join("Package.swift").display());
    println!(
        "cargo:rerun-if-changed={}",
        pkg_dir.join("Sources/FluidAudioBridge/Bridge.swift").display()
    );

    let status = std::process::Command::new("swift")
        .args(["build", "-c", "release"])
        .current_dir(&pkg_dir)
        .status()
        .expect("invoke `swift build` for FluidAudioBridge — is Xcode installed?");
    if !status.success() {
        panic!("swift build failed for FluidAudioBridge (exit {status})");
    }

    // SwiftPM emits one fat .a per static-library product (FluidAudio +
    // wrappers + Bridge code all bundled into libFluidAudioBridge.a — see
    // `Objects.LinkFileList` in .build/<arch>/release/).
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let triple_dir = match arch.as_str() {
        "aarch64" => "arm64-apple-macosx",
        "x86_64" => "x86_64-apple-macosx",
        other => panic!("unsupported macOS arch for FluidAudioBridge: {other}"),
    };
    let lib_dir = pkg_dir.join(".build").join(triple_dir).join("release");
    if !lib_dir.join("libFluidAudioBridge.a").exists() {
        panic!(
            "expected static lib not found: {}",
            lib_dir.join("libFluidAudioBridge.a").display()
        );
    }

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    // Use `link-lib=static=` (not `-Wl,-force_load`) because `link-arg`
    // does NOT propagate across crate boundaries — it only applies to
    // bins/examples/tests in the build script's own crate. `link-lib`
    // does propagate, so downstream crates (the Tauri shell, etc.)
    // also get the linkage. Symbol resolution starts from the Rust
    // extern decls in `recognizer/fluidaudio.rs` and cascades through
    // Bridge.swift into FluidAudio's internals.
    println!("cargo:rustc-link-lib=static=FluidAudioBridge");

    // Apple frameworks FluidAudio touches at runtime.
    for fw in &[
        "CoreML",
        "Foundation",
        "AVFoundation",
        "Accelerate",
        "CoreAudio",
        "AudioToolbox",
        "Metal",
        "MetalPerformanceShaders",
    ] {
        println!("cargo:rustc-link-lib=framework={fw}");
    }

    // C++ runtime (FastClusterWrapper.cpp inside FluidAudio).
    println!("cargo:rustc-link-lib=dylib=c++");

    // Swift runtime — Swift 5.5+ back-deploy libs (libswift_Concurrency.dylib
    // etc.) live alongside the toolchain. macOS 12+ has them in the dyld
    // cache, but the linker still emits @rpath references by default.
    // Embed the standard locations so dyld can find them.
    for candidate in swift_runtime_search_paths() {
        if Path::new(&candidate).is_dir() {
            println!("cargo:rustc-link-search=native={}", candidate);
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", candidate);
        }
    }
}

#[cfg(feature = "recognizer")]
fn swift_runtime_search_paths() -> Vec<String> {
    let mut out = vec![
        // OS-shipped Swift runtime location (macOS 12+).
        "/usr/lib/swift".to_string(),
        // Command Line Tools back-deploy.
        "/Library/Developer/CommandLineTools/usr/lib/swift-5.5/macosx".to_string(),
    ];
    if let Some(toolchain) = swift_toolchain_dir() {
        out.push(format!("{toolchain}/usr/lib/swift/macosx"));
        out.push(format!("{toolchain}/usr/lib/swift-5.5/macosx"));
    }
    out
}

#[cfg(feature = "recognizer")]
fn swift_toolchain_dir() -> Option<String> {
    std::process::Command::new("xcrun")
        .args(["--find", "swift"])
        .output()
        .ok()
        .and_then(|o| {
            if !o.status.success() {
                return None;
            }
            // xcrun --find swift → /Applications/Xcode.app/.../usr/bin/swift
            // Toolchain root = three parents up.
            let path = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let p = std::path::Path::new(&path);
            p.parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
                .map(|p| p.display().to_string())
        })
}
