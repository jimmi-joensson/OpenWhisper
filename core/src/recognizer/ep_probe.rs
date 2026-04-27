//! Runtime execution-provider probe + per-host cache.
//!
//! At first launch, try the priority list (TensorRT → CUDA → DirectML →
//! CPU) in order: build an encoder session with the EP, and if that
//! succeeds keep it. CPU is the floor — it always succeeds. Cache the
//! winning choice to `~/.cache/openwhisper/ep-pref.json` keyed by
//! ort-sys version + EP-feature build flags so subsequent launches skip
//! the probe.
//!
//! `OPENWHISPER_PROVIDER` env var short-circuits the probe — useful for
//! the bench harness where we want to force a specific EP regardless of
//! cache state.
//!
//! Why no async timeout: the probe only runs once per host (cached after).
//! `Session::builder().commit_from_file()` returning `Err` IS the
//! fail-fast guard the spec asks for — a misconfigured GPU EP fails
//! cleanly during DLL load + provider registration before we ever ship
//! tensors. Adding `thread::spawn + recv_timeout` to wrap a 100 ms
//! synchronous failure mode is complexity without value.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use ort::execution_providers::ExecutionProviderDispatch;
use ort::session::{Session, builder::GraphOptimizationLevel};

const ORT_VERSION: &str = "2.0.0-rc.10";
const PROBE_CACHE_FILE: &str = "ep-pref.json";

/// Outcome of `resolve_ep` — both the EP list to register on each
/// session builder and the human-readable label for logs/JSON.
pub struct EpChoice {
    pub eps: Vec<ExecutionProviderDispatch>,
    pub label: String,
}

/// Resolve which EP to use, in this priority order:
///   1. `OPENWHISPER_PROVIDER` env var (always honoured, never cached).
///   2. Cached choice in `~/.cache/openwhisper/ep-pref.json` if the
///      cache key still matches.
///   3. Live probe over the EPs compiled into this build, in priority
///      order TRT → CUDA → DML → CPU. The first one whose encoder
///      session builds successfully wins. Result is cached.
pub fn resolve_ep(encoder_path: &Path) -> Result<EpChoice, String> {
    if let Ok(raw) = env::var("OPENWHISPER_PROVIDER") {
        let raw = raw.trim().to_lowercase();
        if !raw.is_empty() {
            return ep_for_label(&raw)
                .ok_or_else(|| {
                    format!(
                        "OPENWHISPER_PROVIDER={raw:?} not supported in this build. \
                         Available: {}",
                        compiled_eps().join(", ")
                    )
                });
        }
    }

    if let Some(cached) = load_cache_if_valid() {
        if let Some(choice) = ep_for_label(&cached) {
            eprintln!("[recognizer/ort] using cached EP={cached}");
            return Ok(choice);
        }
        // Cached label is no longer compiled in (e.g. user rebuilt
        // without the directml feature). Fall through to re-probe.
        eprintln!("[recognizer/ort] cached EP={cached} not available in this build, re-probing");
    }

    // Live probe — ascend the priority list, keep the first success.
    let candidates = ["tensorrt", "cuda", "directml", "cpu"];
    let mut probe_errs: Vec<String> = Vec::new();
    for label in candidates {
        let Some(choice) = ep_for_label(label) else {
            continue; // EP not compiled into this build, try next.
        };
        eprintln!("[recognizer/ort] probing EP={label}");
        match try_build_session(encoder_path, &choice.eps) {
            Ok(()) => {
                save_cache(label);
                return Ok(choice);
            }
            Err(e) => {
                eprintln!("[recognizer/ort] EP={label} probe failed: {e}");
                probe_errs.push(format!("{label}: {e}"));
            }
        }
    }

    // All probes failed. The probe error chain is the only signal we have
    // — `eprintln!` lines above are invisible in installed Tauri builds, so
    // the returned Err must carry enough to diagnose without re-running.
    // Encoder size matters because a truncated/corrupt model is a common
    // cause (the ~650 MB int8 archive survives reinstall via ~/.cache).
    let enc_meta = match fs::metadata(encoder_path) {
        Ok(m) => format!("{} bytes", m.len()),
        Err(e) => format!("metadata error: {e}"),
    };
    Err(format!(
        "no execution provider succeeded. Encoder: {} ({}). Compiled EPs: {}. Probe errors: {}",
        encoder_path.display(),
        enc_meta,
        compiled_eps().join("+"),
        if probe_errs.is_empty() {
            "none attempted (no EPs compiled into this build)".to_string()
        } else {
            probe_errs.join(" | ")
        },
    ))
}

fn try_build_session(path: &Path, eps: &[ExecutionProviderDispatch]) -> Result<(), String> {
    let mut builder = Session::builder()
        .map_err(|e| format!("Session::builder: {e}"))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| format!("with_optimization_level: {e}"))?;
    if !eps.is_empty() {
        builder = builder
            .with_execution_providers(eps.to_vec())
            .map_err(|e| format!("with_execution_providers: {e}"))?;
    }
    let _ = builder
        .commit_from_file(path)
        .map_err(|e| format!("commit_from_file: {e}"))?;
    Ok(())
}

/// Map a human label to `(EP list, canonical label)`. Returns `None` if
/// the EP wasn't compiled into this build — caller should pick a
/// different EP. CPU is always returned (no feature flag needed).
fn ep_for_label(label: &str) -> Option<EpChoice> {
    match label {
        "" | "cpu" | "default" => Some(EpChoice { eps: Vec::new(), label: "cpu".to_string() }),
        #[cfg(feature = "recognizer-directml")]
        "dml" | "directml" => Some(EpChoice {
            eps: vec![ort::execution_providers::DirectMLExecutionProvider::default().build()],
            label: "directml".to_string(),
        }),
        #[cfg(feature = "recognizer-cuda")]
        "cuda" => Some(EpChoice {
            eps: vec![ort::execution_providers::CUDAExecutionProvider::default().build()],
            label: "cuda".to_string(),
        }),
        #[cfg(feature = "recognizer-tensorrt")]
        "tensorrt" | "trt" => Some(EpChoice {
            eps: vec![
                ort::execution_providers::TensorRTExecutionProvider::default().build(),
                ort::execution_providers::CUDAExecutionProvider::default().build(),
            ],
            label: "tensorrt".to_string(),
        }),
        _ => None,
    }
}

fn compiled_eps() -> Vec<&'static str> {
    let mut v = vec!["cpu"];
    if cfg!(feature = "recognizer-directml") {
        v.push("directml");
    }
    if cfg!(feature = "recognizer-cuda") {
        v.push("cuda");
    }
    if cfg!(feature = "recognizer-tensorrt") {
        v.push("tensorrt");
    }
    v
}

// ---------- cache --------------------------------------------------------

fn cache_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".cache").join("openwhisper").join(PROBE_CACHE_FILE))
}

/// Tiny hand-rolled JSON to avoid pulling serde just for two keys. The
/// file looks like: `{"ort":"2.0.0-rc.10","eps":"cpu+directml","ep":"cpu"}`.
/// `eps` is a stable hash of the compiled EP feature flags so flipping
/// `recognizer-directml` on/off invalidates the cache.
fn load_cache_if_valid() -> Option<String> {
    let path = cache_path()?;
    let raw = fs::read_to_string(&path).ok()?;
    let ort = field(&raw, "ort")?;
    let eps = field(&raw, "eps")?;
    let ep = field(&raw, "ep")?;
    if ort != ORT_VERSION || eps != compiled_ep_key() {
        return None;
    }
    Some(ep)
}

fn save_cache(ep: &str) {
    let Some(path) = cache_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let body = format!(
        "{{\"ort\":\"{}\",\"eps\":\"{}\",\"ep\":\"{}\"}}\n",
        ORT_VERSION,
        compiled_ep_key(),
        ep,
    );
    let _ = fs::write(&path, body);
}

fn compiled_ep_key() -> String {
    compiled_eps().join("+")
}

/// Minimal "find `\"<key>\":\"<value>\"`" parse — just enough to read
/// the cache file we wrote ourselves. Doesn't handle escapes, nesting,
/// or anything else; cache file is invalidated on parse failure.
fn field(raw: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = raw.find(&needle)? + needle.len();
    let rest = &raw[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}
