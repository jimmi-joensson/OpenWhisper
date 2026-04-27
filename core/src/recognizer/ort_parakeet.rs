//! ORT-backed Parakeet-TDT-v3 recognizer (Windows / Linux path).
//!
//! Replaces `sherpa-onnx` (TASK-40). Loads the three Parakeet ONNX
//! sessions through `pykeio/ort` 2.x and drives them with a Rust RNN-T
//! greedy decoder that handles TDT's (token, duration) joint output.
//!
//! Why we own the decode loop now: sherpa-onnx wrapped this for us in
//! C++, but its EP plumbing is hardcoded — adding DirectML to a sherpa
//! build means a per-vendor source build per Windows variant. `ort` lets
//! us register a prioritised EP list and let ONNXRuntime fall back; the
//! decoder logic moves into Rust in exchange. ~250 LOC vs an unbounded
//! per-vendor build matrix.
//!
//! See `backlog/decisions/recognizer-cuda-decision-2026-04-26.md` for
//! the GPU-EP context that motivates this swap, and TASK-40 spec for the
//! full I/O reference (encoder/decoder/joiner tensor names + shapes).

use std::env;
use std::path::Path;
use std::sync::OnceLock;

use ndarray::{Array1, Array2, Array3};
use ort::execution_providers::ExecutionProviderDispatch;
use ort::session::{Session, builder::GraphOptimizationLevel};
use ort::value::TensorRef;

use super::ep_probe::{EpChoice, resolve_ep};
use super::mel::{MelExtractor, N_MELS};
use super::{Recognizer, TranscribeResult, download, ort_lib};
use crate::dictation;

/// `<blk>` index in `tokens.txt`, last entry. RNN-T blank.
const BLANK_ID: i32 = 8192;
/// Vocab size in token logits (incl. blank).
const VOCAB_SIZE: usize = 8193;
/// TDT durations the v3 joiner emits, in joiner-output order.
const TDT_DURATIONS: [usize; 5] = [0, 1, 2, 3, 4];
/// Prevent infinite same-frame emission: if the joiner keeps emitting
/// non-blank tokens with duration=0 on a single encoder frame, force a
/// step after this many. Matches sherpa-onnx's heuristic.
const MAX_SYMBOLS_PER_STEP: usize = 5;
/// LSTM prediction net: 2 layers × 640 hidden (Parakeet 0.6B v3
/// metadata — NeMo `pred_rnn_layers=2`, `pred_hidden=640`). Verified at
/// runtime against the decoder ONNX `states.1` dim-0.
const PRED_LAYERS: usize = 2;
const PRED_HIDDEN: usize = 640;

pub struct OrtParakeet {
    sessions: Option<Sessions>,
    mel: MelExtractor,
    tokens: Vec<String>,
    /// EP that engaged on first session creation; logged once and cached
    /// for `OPENWHISPER_PROVIDER` tooling. Falls back to "cpu" if no
    /// other EP was tried.
    selected_ep: String,
}

struct Sessions {
    encoder: Session,
    decoder: Session,
    joiner: Session,
    /// Cached input/output tensor names for each session — `ort` returns
    /// these as `Cow<str>`, but we want owned `String`s so the decode
    /// loop can hand them back to `inputs!` without re-allocating.
    encoder_input_names: Vec<String>,
    encoder_output_names: Vec<String>,
    decoder_input_names: Vec<String>,
    decoder_output_names: Vec<String>,
    joiner_input_names: Vec<String>,
    joiner_output_names: Vec<String>,
}

impl OrtParakeet {
    pub fn new() -> Self {
        Self {
            sessions: None,
            mel: MelExtractor::new(),
            tokens: Vec::new(),
            selected_ep: "uninitialized".to_string(),
        }
    }

    /// EP name that won the probe. Useful for the bench harness JSON.
    pub fn selected_ep(&self) -> &str {
        &self.selected_ep
    }
}

impl Default for OrtParakeet {
    fn default() -> Self {
        Self::new()
    }
}

/// Tracks whether `ort::init_from(...).commit()` has succeeded for this
/// process. ort's load-dynamic global state must be set exactly once
/// before any session creation; subsequent attempts are silent no-ops
/// inside ort itself, but we still want to surface the resolver/init
/// error on the first call.
static ORT_INIT: OnceLock<Result<(), String>> = OnceLock::new();

fn init_ort_runtime() -> Result<(), String> {
    let result = ORT_INIT.get_or_init(|| {
        let lib_path = ort_lib::resolve()?;
        eprintln!("[recognizer/ort] loading ONNXRuntime from {}", lib_path.display());
        ort::init_from(lib_path.to_string_lossy().into_owned())
            .commit()
            .map_err(|e| format!("ort::init_from commit: {e}"))?;
        Ok(())
    });
    result.clone()
}

impl Recognizer for OrtParakeet {
    fn ensure_loaded(&mut self) -> Result<(), String> {
        if self.sessions.is_some() {
            return Ok(());
        }
        init_ort_runtime()?;
        let paths = download::ensure_model()?;
        self.tokens = load_tokens(&paths.tokens)?;
        // Surface the session-build phase to the UI for both cached and
        // post-download boots: ~2.5 s on Windows CPU, longer on cold cache.
        // Without this the user would still see "loading model…" or the
        // tail of "downloading 100%" while ORT actually grinds.
        dictation::dictation_mark_loading_session();

        // Build all three sessions with the *same* EP list so a partial
        // fallback (e.g. encoder on DML + decoder on CPU) isn't accidental.
        // The EP probe runs once on the encoder ONNX (largest of the
        // three — most representative for "did the EP load + handle
        // these ops"). Once it picks a winner we register that EP on
        // every subsequent session builder.
        let EpChoice { eps, label: ep_label } = resolve_ep(&paths.encoder)?;
        self.selected_ep = ep_label.clone();
        eprintln!("[recognizer/ort] EP={ep_label}");

        let encoder = build_session(&paths.encoder, &eps)
            .map_err(|e| format!("encoder session: {e}"))?;
        let decoder = build_session(&paths.decoder, &eps)
            .map_err(|e| format!("decoder session: {e}"))?;
        let joiner = build_session(&paths.joiner, &eps)
            .map_err(|e| format!("joiner session: {e}"))?;

        let encoder_input_names = io_names(encoder.inputs.iter().map(|i| i.name.as_str()));
        let encoder_output_names = io_names(encoder.outputs.iter().map(|o| o.name.as_str()));
        let decoder_input_names = io_names(decoder.inputs.iter().map(|i| i.name.as_str()));
        let decoder_output_names = io_names(decoder.outputs.iter().map(|o| o.name.as_str()));
        let joiner_input_names = io_names(joiner.inputs.iter().map(|i| i.name.as_str()));
        let joiner_output_names = io_names(joiner.outputs.iter().map(|o| o.name.as_str()));

        self.sessions = Some(Sessions {
            encoder,
            decoder,
            joiner,
            encoder_input_names,
            encoder_output_names,
            decoder_input_names,
            decoder_output_names,
            joiner_input_names,
            joiner_output_names,
        });
        Ok(())
    }

    fn transcribe(&mut self, samples: &[f32]) -> Result<TranscribeResult, String> {
        let s = self
            .sessions
            .as_mut()
            .ok_or_else(|| "recognizer not loaded".to_string())?;

        // ---------------------------------------------------------------
        // 1. Mel extraction → encoder input. NeMo expects [B, n_mels, T].
        let mel = self.mel.extract(samples);
        if mel.is_empty() {
            return Ok(TranscribeResult { text: String::new(), confidence: 1.0, elapsed_ms: 0 });
        }
        let n_frames = mel.len() / N_MELS;
        // Ndarray view of mel — already in [n_mels, n_frames] layout.
        let audio_signal = Array3::<f32>::from_shape_vec((1, N_MELS, n_frames), mel)
            .map_err(|e| format!("mel→ndarray: {e}"))?;
        let length = Array1::<i64>::from_vec(vec![n_frames as i64]);

        // ---------------------------------------------------------------
        // 2. Run encoder. Outputs are positional: [B, enc_dim=1024, T'],
        //    [B] encoded_lengths.
        let enc_outputs = s
            .encoder
            .run(ort::inputs![
                s.encoder_input_names[0].as_str() => TensorRef::from_array_view(&audio_signal)
                    .map_err(|e| format!("audio_signal tensor: {e}"))?,
                s.encoder_input_names[1].as_str() => TensorRef::from_array_view(&length)
                    .map_err(|e| format!("length tensor: {e}"))?,
            ])
            .map_err(|e| format!("encoder run: {e}"))?;

        let encoder_features_arr = enc_outputs[s.encoder_output_names[0].as_str()]
            .try_extract_array::<f32>()
            .map_err(|e| format!("encoder outputs extract: {e}"))?;
        let enc_shape = encoder_features_arr.shape().to_vec();
        if enc_shape.len() != 3 || enc_shape[0] != 1 {
            return Err(format!("unexpected encoder output shape: {enc_shape:?}"));
        }
        let enc_dim = enc_shape[1];
        let enc_t = enc_shape[2];
        let enc_owned: Vec<f32> = encoder_features_arr.iter().copied().collect();
        // Drop the borrow on `enc_outputs` before the decode loop —
        // we'll reach into `enc_owned` slice-by-slice and only need the
        // numbers, not the SessionOutputs container.
        drop(enc_outputs);

        // ---------------------------------------------------------------
        // 3. Greedy TDT decode loop.
        //    Decoder state init: zero-filled LSTM (h, c). Initial
        //    "previous label" is the blank token.
        let mut h = Array3::<f32>::zeros((PRED_LAYERS, 1, PRED_HIDDEN));
        let mut c = Array3::<f32>::zeros((PRED_LAYERS, 1, PRED_HIDDEN));
        let mut prev_token: i32 = BLANK_ID;
        let decoder_out_arr = run_decoder(s, prev_token, &h, &c)?;
        // run_decoder returns (decoder_out, h_next, c_next) — but for the
        // bootstrap we keep the original (zero) state until a real token
        // is emitted, mirroring sherpa-onnx's behavior at line 138 of
        // offline-transducer-greedy-search-nemo-decoder.cc (initial step
        // is BLANK so state stays at zero; we still call decoder once
        // to get an `outputs` tensor for the joiner).
        let mut decoder_out = decoder_out_arr.0;
        // Drop init h/c-next: they correspond to a blank-fed step we
        // didn't emit. State stays zero.
        let _ = (decoder_out_arr.1, decoder_out_arr.2);

        let mut tokens_out: Vec<i32> = Vec::with_capacity(enc_t * 2);
        let mut t = 0usize;
        let mut symbols_this_frame = 0usize;
        while t < enc_t {
            // Single encoder frame slice: [B=1, enc_dim, 1].
            let mut frame = Array3::<f32>::zeros((1, enc_dim, 1));
            for d in 0..enc_dim {
                frame[[0, d, 0]] = enc_owned[d * enc_t + t];
            }

            let joiner_outputs = s
                .joiner
                .run(ort::inputs![
                    s.joiner_input_names[0].as_str() => TensorRef::from_array_view(&frame)
                        .map_err(|e| format!("joiner enc tensor: {e}"))?,
                    s.joiner_input_names[1].as_str() => TensorRef::from_array_view(&decoder_out)
                        .map_err(|e| format!("joiner dec tensor: {e}"))?,
                ])
                .map_err(|e| format!("joiner run: {e}"))?;
            let logits_arr = joiner_outputs[s.joiner_output_names[0].as_str()]
                .try_extract_array::<f32>()
                .map_err(|e| format!("joiner extract: {e}"))?;
            // Joiner output shape: [B=1, 1, 1, vocab_size + n_durations]
            // = [1, 1, 1, 8198]. Flatten to a plain &[f32] over the last
            // axis — argmax works the same regardless of leading dims.
            let logits_flat: Vec<f32> = logits_arr.iter().copied().collect();
            drop(joiner_outputs);
            if logits_flat.len() != VOCAB_SIZE + TDT_DURATIONS.len() {
                return Err(format!(
                    "joiner output len {} != {} (vocab+durations)",
                    logits_flat.len(),
                    VOCAB_SIZE + TDT_DURATIONS.len()
                ));
            }

            let token_id = argmax(&logits_flat[..VOCAB_SIZE]) as i32;
            let duration_idx = argmax(&logits_flat[VOCAB_SIZE..]);
            let mut skip = TDT_DURATIONS[duration_idx];

            if token_id != BLANK_ID {
                tokens_out.push(token_id);
                prev_token = token_id;
                let (out, h_next, c_next) = run_decoder(s, prev_token, &h, &c)?;
                decoder_out = out;
                h = h_next;
                c = c_next;
                symbols_this_frame += 1;
            }

            if skip > 0 {
                symbols_this_frame = 0;
            }
            // Anti-stall: if we keep emitting tokens with skip=0 on the
            // same frame, force progress after MAX_SYMBOLS_PER_STEP. Same
            // policy as sherpa.
            if symbols_this_frame >= MAX_SYMBOLS_PER_STEP {
                skip = 1;
                symbols_this_frame = 0;
            }
            // Anti-stall: if joiner emits blank with skip=0, advance one
            // frame so we don't loop forever on a silent step.
            if token_id == BLANK_ID && skip == 0 {
                skip = 1;
            }

            t += skip;
        }

        let text = detokenize(&tokens_out, &self.tokens);
        Ok(TranscribeResult { text, confidence: 1.0, elapsed_ms: 0 })
    }
}

/// Run the prediction net once with `prev_token`. Returns
/// (decoder_outputs[B, hidden, 1], h_next, c_next).
fn run_decoder(
    s: &mut Sessions,
    prev_token: i32,
    h: &Array3<f32>,
    c: &Array3<f32>,
) -> Result<(Array3<f32>, Array3<f32>, Array3<f32>), String> {
    let targets = Array2::<i32>::from_shape_vec((1, 1), vec![prev_token])
        .map_err(|e| format!("decoder targets: {e}"))?;
    let target_length = Array1::<i32>::from_vec(vec![1]);

    let outputs = s
        .decoder
        .run(ort::inputs![
            s.decoder_input_names[0].as_str() => TensorRef::from_array_view(&targets)
                .map_err(|e| format!("decoder targets tensor: {e}"))?,
            s.decoder_input_names[1].as_str() => TensorRef::from_array_view(&target_length)
                .map_err(|e| format!("decoder length tensor: {e}"))?,
            s.decoder_input_names[2].as_str() => TensorRef::from_array_view(h)
                .map_err(|e| format!("decoder h tensor: {e}"))?,
            s.decoder_input_names[3].as_str() => TensorRef::from_array_view(c)
                .map_err(|e| format!("decoder c tensor: {e}"))?,
        ])
        .map_err(|e| format!("decoder run: {e}"))?;

    // Decoder positional outputs: [outputs, prednet_lengths, h_next, c_next].
    let decoder_out = extract_owned3(&outputs, &s.decoder_output_names[0])?;
    let h_next = extract_owned3(&outputs, &s.decoder_output_names[2])?;
    let c_next = extract_owned3(&outputs, &s.decoder_output_names[3])?;
    Ok((decoder_out, h_next, c_next))
}

fn extract_owned3(
    outputs: &ort::session::SessionOutputs,
    name: &str,
) -> Result<Array3<f32>, String> {
    let arr = outputs[name]
        .try_extract_array::<f32>()
        .map_err(|e| format!("extract {name}: {e}"))?;
    let shape = arr.shape();
    if shape.len() != 3 {
        return Err(format!("expected rank-3 tensor for {name}, got {shape:?}"));
    }
    let (d0, d1, d2) = (shape[0], shape[1], shape[2]);
    let v: Vec<f32> = arr.iter().copied().collect();
    Array3::<f32>::from_shape_vec((d0, d1, d2), v).map_err(|e| format!("from_shape_vec {name}: {e}"))
}

fn build_session(path: &Path, eps: &[ExecutionProviderDispatch]) -> Result<Session, String> {
    // CPU EP intra-op thread count carries over from TASK-39: physical
    // cores capped at 8, overridable by `OPENWHISPER_NUM_THREADS`. ORT's
    // `with_intra_threads` only affects the CPU EP; GPU EPs ignore it.
    let num_threads: i16 = env::var("OPENWHISPER_NUM_THREADS")
        .ok()
        .and_then(|s| s.parse::<i16>().ok())
        .unwrap_or_else(|| (num_cpus::get_physical().min(8)) as i16);

    let mut builder = Session::builder()
        .map_err(|e| format!("session builder: {e}"))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| format!("with_optimization_level: {e}"))?
        .with_intra_threads(num_threads as usize)
        .map_err(|e| format!("with_intra_threads: {e}"))?;
    if !eps.is_empty() {
        builder = builder
            .with_execution_providers(eps.to_vec())
            .map_err(|e| format!("with_execution_providers: {e}"))?;
    }
    builder
        .commit_from_file(path)
        .map_err(|e| format!("commit_from_file({}): {e}", path.display()))
}


fn argmax(slice: &[f32]) -> usize {
    let mut best_i = 0usize;
    let mut best_v = f32::NEG_INFINITY;
    for (i, &v) in slice.iter().enumerate() {
        if v > best_v {
            best_v = v;
            best_i = i;
        }
    }
    best_i
}

fn io_names<'a, I: Iterator<Item = &'a str>>(it: I) -> Vec<String> {
    it.map(|s| s.to_string()).collect()
}

/// `tokens.txt` format: "<piece> <id>" per line. <id> is decimal, in
/// ascending order from 0 to vocab_size-1. Some pieces contain spaces
/// inside `<|...|>` control tokens but those don't have a literal space
/// before the id — the id is the *last* whitespace-delimited token.
fn load_tokens(path: &Path) -> Result<Vec<String>, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("read tokens.txt: {e}"))?;
    let mut tokens = vec![String::new(); VOCAB_SIZE];
    for (lineno, line) in raw.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let mut parts = line.rsplitn(2, ' ');
        let id_str = parts.next().ok_or_else(|| format!("tokens.txt:{lineno} missing id"))?;
        let piece = parts
            .next()
            .ok_or_else(|| format!("tokens.txt:{lineno} missing piece"))?;
        let id: usize = id_str
            .parse()
            .map_err(|e| format!("tokens.txt:{lineno} bad id {id_str:?}: {e}"))?;
        if id >= tokens.len() {
            return Err(format!("tokens.txt:{lineno} id {id} >= vocab_size {}", tokens.len()));
        }
        tokens[id] = piece.to_string();
    }
    Ok(tokens)
}

/// SentencePiece BPE detokenize. Each piece either starts with U+2581
/// (▁) — meaning "preceded by a space in the original text" — or
/// continues the previous word. We concat, swap ▁ for ' ', strip the
/// leading space.
fn detokenize(token_ids: &[i32], tokens: &[String]) -> String {
    let mut out = String::with_capacity(token_ids.len() * 2);
    for &id in token_ids {
        if id == BLANK_ID || (id as usize) >= tokens.len() {
            continue;
        }
        let piece = &tokens[id as usize];
        // Skip the special control tokens (their textual form starts with
        // '<' and ends with '>'). They mark language/PNC/timestamp etc.
        // and shouldn't appear in transcribed text.
        if piece.starts_with('<') && piece.ends_with('>') {
            continue;
        }
        for ch in piece.chars() {
            if ch == '\u{2581}' {
                out.push(' ');
            } else {
                out.push(ch);
            }
        }
    }
    out.trim().to_string()
}
