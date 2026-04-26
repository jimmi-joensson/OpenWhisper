//! sherpa-onnx + Parakeet-TDT v3 (int8) backend.
//!
//! Provider string `"coreml"` engages the CoreML EP at runtime. CoreML
//! picks compute units across CPU/GPU/ANE per op
//! (`MLComputeUnits.all`) — see decision doc
//! `backlog/decisions/recognizer-bench-thresholds-2026-04-26.md`.

use std::path::Path;

use sherpa_onnx::{
    OfflineModelConfig, OfflineRecognizer, OfflineRecognizerConfig,
    OfflineTransducerModelConfig,
};

use super::{Recognizer, TranscribeResult, download};

/// Stringify a path for the sherpa-onnx C ABI. On Windows, sherpa expects
/// LPSTR (ANSI/UTF-8) and opens files via the ANSI Win32 API, so any
/// non-ASCII path component (e.g. the `ø` in `C:\Users\JimmiJønsson\...`)
/// fails to open. Converting to the 8.3 short name keeps the bytes in the
/// ANSI-safe range. No-op on non-Windows. See
/// `feedback_ansi_path_marshaling.md`.
fn path_for_sherpa(p: &Path) -> String {
    #[cfg(windows)]
    if let Some(short) = windows_short_path(p) {
        return short;
    }
    p.to_string_lossy().into_owned()
}

#[cfg(windows)]
fn windows_short_path(p: &Path) -> Option<String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetShortPathNameW(
            lpsz_long_path: *const u16,
            lpsz_short_path: *mut u16,
            cch_buffer: u32,
        ) -> u32;
    }

    let wide: Vec<u16> = p.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut buf = vec![0u16; 1024];
    let len = unsafe { GetShortPathNameW(wide.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
    if len == 0 || len as usize >= buf.len() {
        return None;
    }
    Some(OsString::from_wide(&buf[..len as usize]).to_string_lossy().into_owned())
}

pub struct SherpaParakeet {
    inner: Option<OfflineRecognizer>,
}

impl SherpaParakeet {
    pub fn new() -> Self {
        Self { inner: None }
    }
}

impl Default for SherpaParakeet {
    fn default() -> Self {
        Self::new()
    }
}

impl Recognizer for SherpaParakeet {
    fn ensure_loaded(&mut self) -> Result<(), String> {
        if self.inner.is_some() {
            return Ok(());
        }
        let paths = download::ensure_model()?;

        let mut config = OfflineRecognizerConfig::default();
        let mut model_config = OfflineModelConfig::default();
        model_config.transducer = OfflineTransducerModelConfig {
            encoder: Some(path_for_sherpa(&paths.encoder)),
            decoder: Some(path_for_sherpa(&paths.decoder)),
            joiner: Some(path_for_sherpa(&paths.joiner)),
        };
        model_config.tokens = Some(path_for_sherpa(&paths.tokens));
        model_config.model_type = Some("nemo_transducer".to_string());
        // ONNXRuntime CPU EP intra-op thread count. Defaults to
        // min(physical_cores, 8): sweeps across 4c4t Xeon (best at 2,
        // tied with 4) and 6c12t Ryzen (best at 8) showed perf cratered
        // once threads ≥ logical_cpus (SMT oversubscription cliff).
        // Capping at physical cores avoids that cliff; capping at 8
        // avoids ONNXRuntime's per-thread coordination overhead on
        // very-high-core-count workstations. Override via env when
        // tuning per host (TASK-39 results in scripts/bench/results/).
        model_config.num_threads = std::env::var("OPENWHISPER_NUM_THREADS")
            .ok()
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or_else(|| num_cpus::get_physical().min(8) as i32);
        // Runtime EP selection. Default `"coreml"` for Mac (which decides
        // CPU/GPU/ANE per op via MLComputeUnits.all). On non-Mac the C++
        // side falls back to CPU and logs a warning — fine for the
        // shared bench harness. Override via env to try `directml`,
        // `cuda`, etc. on Windows.
        model_config.provider = Some(
            std::env::var("OPENWHISPER_PROVIDER").unwrap_or_else(|_| "coreml".to_string()),
        );
        config.model_config = model_config;
        config.decoding_method = Some("greedy_search".to_string());

        let rec = OfflineRecognizer::create(&config)
            .ok_or_else(|| "OfflineRecognizer::create returned null".to_string())?;
        self.inner = Some(rec);
        Ok(())
    }

    fn transcribe(&mut self, samples: &[f32]) -> Result<TranscribeResult, String> {
        let rec = self
            .inner
            .as_ref()
            .ok_or_else(|| "recognizer not loaded".to_string())?;
        let stream = rec.create_stream();
        stream.accept_waveform(16_000, samples);
        rec.decode(&stream);
        let result = stream
            .get_result()
            .ok_or_else(|| "no decode result".to_string())?;
        Ok(TranscribeResult {
            text: result.text,
            confidence: 1.0,
            elapsed_ms: 0,
        })
    }
}
