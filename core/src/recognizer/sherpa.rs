//! sherpa-onnx + Parakeet-TDT v3 (int8) backend.
//!
//! Provider string `"coreml"` engages the CoreML EP at runtime. CoreML
//! picks compute units across CPU/GPU/ANE per op
//! (`MLComputeUnits.all`) — see decision doc
//! `backlog/decisions/recognizer-bench-thresholds-2026-04-26.md`.

use sherpa_onnx::{
    OfflineModelConfig, OfflineRecognizer, OfflineRecognizerConfig,
    OfflineTransducerModelConfig,
};

use super::{Recognizer, TranscribeResult, download};

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
            encoder: Some(paths.encoder.to_string_lossy().into_owned()),
            decoder: Some(paths.decoder.to_string_lossy().into_owned()),
            joiner: Some(paths.joiner.to_string_lossy().into_owned()),
        };
        model_config.tokens = Some(paths.tokens.to_string_lossy().into_owned());
        model_config.model_type = Some("nemo_transducer".to_string());
        model_config.num_threads = 1;
        // Runtime EP selection. `"coreml"` → CoreML EP, which decides
        // CPU/GPU/ANE per op (MLComputeUnits.all). On non-Mac builds the
        // C++ side falls back to CPU and logs a warning — fine for
        // bench-tooling that compiles on Linux/Win.
        model_config.provider = Some("coreml".to_string());
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
