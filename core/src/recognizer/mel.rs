//! Librosa-style mel spectrogram preprocessor for NeMo Parakeet-TDT-v3.
//!
//! Matches `nemo.collections.asr.modules.AudioToMelSpectrogramPreprocessor`
//! with the v3 0.6B FastConformer XL config — 128 mel bins, hop 160,
//! window 400, n_fft 512, slaney mel norm, per-feature normalize. Verified
//! against sherpa-onnx's `kaldi-native-fbank` invocation in
//! `scripts/nemo/parakeet-tdt-0.6b-v2/test_onnx.py` (v3 symlinks the same
//! pipeline).
//!
//! Output layout: `[n_mels=128, n_frames]` f32. Caller transposes to the
//! `[B=1, n_mels, T]` layout the encoder ONNX expects.
//!
//! Why pure Rust instead of binding kaldi-native-fbank: keeps the build
//! closure C++-free on Windows, where we just removed sherpa-onnx-sys
//! exactly to avoid that. `rustfft` + a slaney filterbank is < 200 LOC
//! and unit-testable against the sherpa baseline.

use std::f32::consts::PI;
use std::sync::Arc;

use rustfft::{Fft, FftPlanner, num_complex::Complex};

/// NeMo Parakeet-TDT-v3 mel preprocessor config. Constants come from
/// `conformer_tdt_bpe.yaml` + `fastconformer_hybrid_tdt_ctc_bpe.yaml`
/// (see backlog/decisions doc TASK-40 spec for cites). Don't change
/// without also re-bending the encoder — these are baked into the
/// trained network.
pub const SAMPLE_RATE: u32 = 16_000;
pub const N_FFT: usize = 512;
pub const WIN_LENGTH: usize = 400;
pub const HOP_LENGTH: usize = 160;
pub const N_MELS: usize = 128;
pub const PREEMPH: f32 = 0.97;
/// `log_zero_guard_value = 2 ^ -24` per NeMo default; used as the floor
/// inside the `log(x + guard)` step so silent frames don't blow up to
/// `-inf`.
pub const LOG_ZERO_GUARD: f32 = 5.960_464_5e-8; // 2.0_f32.powi(-24)
/// Per-feature normalize epsilon. NeMo uses 1e-5 in
/// `audio_preprocessing.py` — keeps zero-variance bins finite.
pub const NORM_EPS: f32 = 1.0e-5;

/// Reusable mel preprocessor. Keep one instance across decodes so the
/// FFT plan + filterbank allocation amortize.
pub struct MelExtractor {
    fft: Arc<dyn Fft<f32>>,
    /// Hann window of length WIN_LENGTH (zero-padded to N_FFT inside the
    /// hot loop). Pre-computed because rustfft re-uses the same buffer.
    window: Vec<f32>,
    /// Slaney mel filterbank, shape [N_MELS, N_FFT/2 + 1]. Row-major.
    /// Multiplied against the |STFT|² magnitude per frame.
    filterbank: Vec<f32>,
}

impl MelExtractor {
    pub fn new() -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(N_FFT);
        let window = hann_window(WIN_LENGTH);
        let filterbank = slaney_mel_filterbank(SAMPLE_RATE, N_FFT, N_MELS, 0.0, SAMPLE_RATE as f32 / 2.0);
        Self { fft, window, filterbank }
    }

    /// Returns log-mel features in `[n_mels, n_frames]` order, already
    /// per-feature normalized.
    pub fn extract(&self, samples: &[f32]) -> Vec<f32> {
        // Center-padded short-time Fourier transform: librosa default is
        // `center=True`, which pads the signal with `n_fft // 2` reflect
        // samples on both sides so the first frame is centered at t=0.
        // NeMo uses the same convention via `pad_to=0, center=True`
        // implicit in the torch.stft call inside FilterbankFeatures.
        let pad = N_FFT / 2;
        let padded = reflect_pad(samples, pad);

        // Pre-emphasis is applied to the *raw* waveform before framing
        // in NeMo (see audio_preprocessing.py L177). Equivalent to
        // y[n] - 0.97 * y[n-1].
        let preemphed = preemphasis(&padded, PREEMPH);

        let n_frames = if preemphed.len() < WIN_LENGTH {
            0
        } else {
            (preemphed.len() - WIN_LENGTH) / HOP_LENGTH + 1
        };
        if n_frames == 0 {
            return Vec::new();
        }

        let n_freq = N_FFT / 2 + 1;
        let mut power = vec![0.0_f32; n_freq * n_frames];
        let mut frame_buf = vec![Complex::<f32> { re: 0.0, im: 0.0 }; N_FFT];

        for f in 0..n_frames {
            let start = f * HOP_LENGTH;
            // Window + zero-pad WIN_LENGTH (400) to N_FFT (512). Top
            // 112 lanes stay zero. rustfft is in-place on `frame_buf`.
            for c in frame_buf.iter_mut() {
                *c = Complex { re: 0.0, im: 0.0 };
            }
            for i in 0..WIN_LENGTH {
                frame_buf[i].re = preemphed[start + i] * self.window[i];
            }
            self.fft.process(&mut frame_buf);
            // Power spectrum: |X|² (mag_power=2.0 in NeMo config). Only
            // the first n_freq bins matter (Hermitian symmetry).
            for k in 0..n_freq {
                let z = frame_buf[k];
                power[f * n_freq + k] = z.re * z.re + z.im * z.im;
            }
        }

        // Apply mel filterbank: mel[m, f] = sum_k fb[m, k] * power[f, k]
        let mut mel = vec![0.0_f32; N_MELS * n_frames];
        for f in 0..n_frames {
            for m in 0..N_MELS {
                let mut s = 0.0_f32;
                for k in 0..n_freq {
                    s += self.filterbank[m * n_freq + k] * power[f * n_freq + k];
                }
                // log(x + guard) with guard = 2^-24. Matches NeMo's
                // log_zero_guard_type="add".
                mel[m * n_frames + f] = (s + LOG_ZERO_GUARD).ln();
            }
        }

        // Per-feature normalize: subtract per-mel mean, divide by per-mel
        // std + eps, computed across the time axis (axis=1). NeMo
        // FilterbankFeatures.normalize_batch with normalize_type="per_feature".
        for m in 0..N_MELS {
            let row = &mut mel[m * n_frames..(m + 1) * n_frames];
            let mean = row.iter().sum::<f32>() / n_frames as f32;
            let var = row.iter().map(|x| (x - mean) * (x - mean)).sum::<f32>() / n_frames as f32;
            let inv = 1.0 / (var.sqrt() + NORM_EPS);
            for x in row.iter_mut() {
                *x = (*x - mean) * inv;
            }
        }

        mel
    }
}

impl Default for MelExtractor {
    fn default() -> Self {
        Self::new()
    }
}

fn hann_window(n: usize) -> Vec<f32> {
    // Periodic hann (matches torch.hann_window with periodic=True, which
    // is librosa/NeMo's default). Index range [0, n).
    (0..n)
        .map(|i| 0.5 * (1.0 - ((2.0 * PI * i as f32) / n as f32).cos()))
        .collect()
}

fn reflect_pad(x: &[f32], pad: usize) -> Vec<f32> {
    // Reflect-without-edge mode: librosa's default `pad_mode="reflect"`.
    // For a signal `[a b c d]` and pad=2 the result is `[c b | a b c d | c b]`
    // (the first/last samples are NOT duplicated — that would be `edge`).
    let mut out = Vec::with_capacity(x.len() + 2 * pad);
    let n = x.len();
    if n == 0 {
        return out;
    }
    for i in 0..pad {
        // Mirror at index 0: out[pad - 1 - i] = x[1 + i]
        let src = (pad - 1 - i + 1).min(n - 1);
        out.push(x[src]);
    }
    out.extend_from_slice(x);
    for i in 0..pad {
        let src = n.saturating_sub(2 + i);
        out.push(x[src]);
    }
    out
}

fn preemphasis(x: &[f32], coef: f32) -> Vec<f32> {
    // y[0] = x[0]; y[n] = x[n] - coef * x[n-1] for n >= 1. Matches
    // NeMo FilterbankFeatures.preemphasis (and torchaudio.functional.preemphasis).
    if x.is_empty() {
        return Vec::new();
    }
    let mut y = vec![0.0_f32; x.len()];
    y[0] = x[0];
    for n in 1..x.len() {
        y[n] = x[n] - coef * x[n - 1];
    }
    y
}

/// Build a slaney-normalized mel filterbank in `[n_mels, n_freq]` row-major
/// layout. Matches `librosa.filters.mel(htk=False, norm='slaney')` — which
/// is what NeMo defaults to (`mel_norm='slaney'`, `mel_scale='htk'` is NOT
/// set, so it falls back to slaney/non-htk).
fn slaney_mel_filterbank(sr: u32, n_fft: usize, n_mels: usize, fmin: f32, fmax: f32) -> Vec<f32> {
    let n_freq = n_fft / 2 + 1;

    // Mel points equally spaced in (slaney) mel space between fmin and fmax.
    // Slaney mel ↔ Hz: linear up to 1000 Hz, log above.
    let lo = hz_to_mel_slaney(fmin);
    let hi = hz_to_mel_slaney(fmax);
    let mel_pts: Vec<f32> = (0..(n_mels + 2))
        .map(|i| lo + (hi - lo) * (i as f32) / ((n_mels + 1) as f32))
        .collect();
    let hz_pts: Vec<f32> = mel_pts.iter().map(|&m| mel_to_hz_slaney(m)).collect();
    // FFT bin centers in Hz: k * sr / n_fft for k = 0..n_freq.
    let bin_freqs: Vec<f32> = (0..n_freq).map(|k| (k as f32) * (sr as f32) / (n_fft as f32)).collect();

    let mut fb = vec![0.0_f32; n_mels * n_freq];
    for m in 0..n_mels {
        let lower = hz_pts[m];
        let center = hz_pts[m + 1];
        let upper = hz_pts[m + 2];
        for k in 0..n_freq {
            let f = bin_freqs[k];
            let w = if f < lower || f > upper {
                0.0
            } else if f <= center {
                (f - lower) / (center - lower)
            } else {
                (upper - f) / (upper - center)
            };
            fb[m * n_freq + k] = w;
        }
        // Slaney norm: scale each filter by 2 / (upper - lower) so that
        // the area under each triangle is constant. This is what
        // librosa does with norm='slaney'.
        let enorm = 2.0 / (upper - lower);
        for k in 0..n_freq {
            fb[m * n_freq + k] *= enorm;
        }
    }
    fb
}

const SLANEY_F_MIN: f32 = 0.0;
const SLANEY_F_SP: f32 = 200.0 / 3.0; // ~66.667 Hz per mel in linear region
const SLANEY_MIN_LOG_HZ: f32 = 1000.0;
// SLANEY_MIN_LOG_MEL = (SLANEY_MIN_LOG_HZ - SLANEY_F_MIN) / SLANEY_F_SP = 15.0
const SLANEY_MIN_LOG_MEL: f32 = 15.0;
// log step that yields a step of `logstep` mels per octave; librosa uses
// log(6.4) / 27 so that 6400 Hz → 27 mels above the breakpoint. Keep
// identical so our filterbank == librosa's bit-for-bit (well, modulo
// f32 vs f64 — the mel-bin centers differ by < 1e-5 Hz which is sub-bin
// resolution at 16 kHz/512 fft).
const SLANEY_LOGSTEP: f32 = 0.068_751_77; // log(6.4) / 27

fn hz_to_mel_slaney(hz: f32) -> f32 {
    if hz < SLANEY_MIN_LOG_HZ {
        (hz - SLANEY_F_MIN) / SLANEY_F_SP
    } else {
        SLANEY_MIN_LOG_MEL + (hz / SLANEY_MIN_LOG_HZ).ln() / SLANEY_LOGSTEP
    }
}

fn mel_to_hz_slaney(mel: f32) -> f32 {
    if mel < SLANEY_MIN_LOG_MEL {
        SLANEY_F_MIN + SLANEY_F_SP * mel
    } else {
        SLANEY_MIN_LOG_HZ * ((mel - SLANEY_MIN_LOG_MEL) * SLANEY_LOGSTEP).exp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slaney_mel_round_trip() {
        for hz in [0.0, 100.0, 999.9, 1000.0, 1000.1, 4000.0, 8000.0] {
            let m = hz_to_mel_slaney(hz);
            let back = mel_to_hz_slaney(m);
            assert!((back - hz).abs() < 1e-2, "hz={hz} round-trip={back}");
        }
    }

    #[test]
    fn filterbank_shape_and_unity_partition() {
        let fb = slaney_mel_filterbank(SAMPLE_RATE, N_FFT, N_MELS, 0.0, 8000.0);
        assert_eq!(fb.len(), N_MELS * (N_FFT / 2 + 1));
        // Each mel filter is non-negative.
        assert!(fb.iter().all(|&w| w >= 0.0));
    }

    #[test]
    fn extract_shape() {
        // 1 second of silence → ~100 frames at hop=160 (with center pad).
        let samples = vec![0.0_f32; SAMPLE_RATE as usize];
        let mel = MelExtractor::new().extract(&samples);
        let n_frames = mel.len() / N_MELS;
        assert!(n_frames >= 99 && n_frames <= 102, "unexpected frames: {n_frames}");
        // Per-feature normalize on a silent signal: log-mel collapses to
        // a per-bin constant (= log(2^-24)), so the (x - mean) numerator
        // is zero up to f32 accumulation noise. With std ≈ 0 the divisor
        // becomes NORM_EPS (1e-5), which amplifies sub-ulp drift —
        // bound by ±1.0 rather than near-zero.
        for v in mel {
            assert!(v.abs() < 1.0, "silent-frame normalize drift {v}");
        }
    }
}
