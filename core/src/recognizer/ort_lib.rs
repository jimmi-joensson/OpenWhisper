//! Resolve where to load `onnxruntime` from at runtime (`load-dynamic`).
//!
//! ort 2.0.0-rc.10 expects `ort::init_from(path).commit()` to be called
//! once per process before any session work. The path points at the
//! actual shared library: `onnxruntime.dll` on Windows,
//! `libonnxruntime.dylib` on Mac, `libonnxruntime.so` on Linux.
//!
//! Resolution order, first hit wins:
//!
//! 1. `OPENWHISPER_ORT_LIB` env var — explicit override, full path to the
//!    library file. Useful for benches pinning a specific build (CPU-only
//!    vs DirectML vs CUDA) without touching the on-disk cache.
//! 2. Next to the running executable (e.g. `target/debug/onnxruntime.dll`).
//!    Tauri release bundles ship the DLL alongside the exe, so this is
//!    the production path.
//! 3. `~/.cache/openwhisper/onnxruntime/<libname>` — populated by
//!    `apps/tauri/scripts/fetch-ort.cjs`, the dev-time setup script.
//!
//! On miss: actionable error citing the setup script.

use std::path::PathBuf;

/// Lib filename for the host OS. ort's `load-dynamic` wants the actual
/// shared object, not a .lib import library.
const LIB_NAME: &str = if cfg!(target_os = "windows") {
    "onnxruntime.dll"
} else if cfg!(target_os = "macos") {
    "libonnxruntime.dylib"
} else {
    "libonnxruntime.so"
};

pub fn resolve() -> Result<PathBuf, String> {
    if let Ok(v) = std::env::var("OPENWHISPER_ORT_LIB") {
        let p = PathBuf::from(&v);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!(
            "OPENWHISPER_ORT_LIB={v:?} does not point to a file"
        ));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(LIB_NAME);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    if let Some(home) = dirs::home_dir() {
        let candidate = home
            .join(".cache")
            .join("openwhisper")
            .join("onnxruntime")
            .join(LIB_NAME);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(format!(
        "could not find {LIB_NAME}. Run `pnpm setup:ort` from apps/tauri/ \
         (downloads ONNX Runtime 1.22.0 into ~/.cache/openwhisper/onnxruntime/) \
         or set OPENWHISPER_ORT_LIB to a full path."
    ))
}
