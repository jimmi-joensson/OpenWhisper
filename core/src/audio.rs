//! Microphone capture for OpenWhisper.
//!
//! Owns a dedicated worker thread that holds the cpal stream. Swift drives
//! it via the pull-based FFI in [`crate::ffi`]: start, talk, stop + drain.
//! Output is always 16 kHz mono f32 so FluidAudio can consume it directly.

use std::sync::{Arc, Mutex, OnceLock, atomic::{AtomicU32, Ordering}, mpsc};
use std::thread;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

const TARGET_SAMPLE_RATE: u32 = 16_000;

// Peak amplitude of the most recent callback. Written by the CoreAudio callback,
// read from any thread via audio_current_level(). Stored as f32 bits so we can
// use a lock-free atomic. Peak reads cleaner on a UI meter than RMS because
// transients in speech (consonants, syllable onsets) show as real spikes.
static LEVEL_BITS: AtomicU32 = AtomicU32::new(0);

pub struct AudioEngine {
    ctrl_tx: mpsc::Sender<Ctrl>,
}

enum Ctrl {
    Start(mpsc::SyncSender<Result<(), String>>),
    Stop(mpsc::SyncSender<()>),
    Drain(mpsc::SyncSender<Vec<f32>>),
    IsCapturing(mpsc::SyncSender<bool>),
}

struct Capture {
    // `None` means the stream has been stopped but the captured samples
    // are still waiting in `buffer` for the next `drain`. This lets Swift
    // call stop → drain in that order without losing data.
    stream: Option<cpal::Stream>,
    buffer: Arc<Mutex<Vec<f32>>>,
    native_rate: u32,
    channels: u16,
}

impl AudioEngine {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<Ctrl>();
        thread::Builder::new()
            .name("openwhisper-audio".into())
            .spawn(move || run_worker(rx))
            .expect("spawn audio worker");
        Self { ctrl_tx: tx }
    }

    fn rpc<R>(&self, make: impl FnOnce(mpsc::SyncSender<R>) -> Ctrl) -> R {
        let (tx, rx) = mpsc::sync_channel(0);
        let _ = self.ctrl_tx.send(make(tx));
        rx.recv().expect("audio worker died")
    }

    pub fn start(&self) -> Result<(), String> {
        self.rpc(Ctrl::Start)
    }

    pub fn stop(&self) {
        self.rpc(Ctrl::Stop);
    }

    pub fn drain(&self) -> Vec<f32> {
        self.rpc(Ctrl::Drain)
    }

    pub fn is_capturing(&self) -> bool {
        self.rpc(Ctrl::IsCapturing)
    }
}

fn run_worker(rx: mpsc::Receiver<Ctrl>) {
    let mut capture: Option<Capture> = None;
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Ctrl::Start(reply) => {
                if capture.is_some() {
                    let _ = reply.send(Ok(()));
                    continue;
                }
                match begin_capture() {
                    Ok(c) => {
                        capture = Some(c);
                        let _ = reply.send(Ok(()));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(e));
                    }
                }
            }
            Ctrl::Stop(reply) => {
                if let Some(c) = capture.as_mut() {
                    c.stream = None;
                }
                let _ = reply.send(());
            }
            Ctrl::Drain(reply) => {
                let samples = capture
                    .as_ref()
                    .map(drain_and_resample)
                    .unwrap_or_default();
                // Once stopped + drained, fully release.
                if capture.as_ref().is_some_and(|c| c.stream.is_none()) {
                    capture = None;
                }
                let _ = reply.send(samples);
            }
            Ctrl::IsCapturing(reply) => {
                let is_live = capture.as_ref().is_some_and(|c| c.stream.is_some());
                let _ = reply.send(is_live);
            }
        }
    }
}

fn begin_capture() -> Result<Capture, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "no default input device".to_string())?;

    let device_name = device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    let supported = device
        .default_input_config()
        .map_err(|e| format!("default input config: {e}"))?;

    let sample_format = supported.sample_format();
    let native_rate = supported.sample_rate();
    let channels = supported.channels();
    eprintln!(
        "[openwhisper-core] mic: device={device_name:?} rate={native_rate} ch={channels} fmt={sample_format:?}"
    );
    let config: cpal::StreamConfig = supported.into();

    let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));

    let stream = match sample_format {
        cpal::SampleFormat::F32 => build_input_stream::<f32>(&device, &config, buffer.clone())?,
        cpal::SampleFormat::I16 => build_input_stream::<i16>(&device, &config, buffer.clone())?,
        cpal::SampleFormat::U16 => build_input_stream::<u16>(&device, &config, buffer.clone())?,
        other => return Err(format!("unsupported sample format: {other:?}")),
    };

    stream
        .play()
        .map_err(|e| format!("stream.play failed: {e}"))?;

    Ok(Capture {
        stream: Some(stream),
        buffer,
        native_rate,
        channels,
    })
}

fn build_input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    buffer: Arc<Mutex<Vec<f32>>>,
) -> Result<cpal::Stream, String>
where
    T: Sample + SizedSample,
    f32: FromSample<T>,
{
    let err_fn = |e| eprintln!("openwhisper audio stream error: {e}");
    device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                let mut peak: f32 = 0.0;
                if let Ok(mut buf) = buffer.lock() {
                    buf.reserve(data.len());
                    for &s in data {
                        let f = f32::from_sample(s);
                        let abs = f.abs();
                        if abs > peak {
                            peak = abs;
                        }
                        buf.push(f);
                    }
                }
                if !data.is_empty() {
                    LEVEL_BITS.store(peak.to_bits(), Ordering::Relaxed);
                }
            },
            err_fn,
            None,
        )
        .map_err(|e| format!("build_input_stream: {e}"))
}

fn drain_and_resample(capture: &Capture) -> Vec<f32> {
    let t0 = std::time::Instant::now();
    let raw = match capture.buffer.lock() {
        Ok(mut g) => std::mem::take(&mut *g),
        Err(_) => return Vec::new(),
    };

    if raw.is_empty() {
        return Vec::new();
    }

    let mono = downmix_to_mono(&raw, capture.channels);

    let out = if capture.native_rate == TARGET_SAMPLE_RATE {
        mono
    } else {
        resample_to_target(mono, capture.native_rate).unwrap_or_default()
    };

    let dt = t0.elapsed();
    crate::verbose_log!(
        "[ow.audio] drain raw={} mono_out={} native={}Hz ms={:.1}",
        raw.len(),
        out.len(),
        capture.native_rate,
        dt.as_secs_f64() * 1000.0,
    );

    out
}

fn downmix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    let ch = channels as usize;
    let frames = interleaved.len() / ch;
    let inv = 1.0 / ch as f32;
    let mut out = Vec::with_capacity(frames);
    for frame in 0..frames {
        let base = frame * ch;
        let mut acc = 0.0;
        for c in 0..ch {
            acc += interleaved[base + c];
        }
        out.push(acc * inv);
    }
    out
}

fn resample_to_target(mono: Vec<f32>, native_rate: u32) -> Result<Vec<f32>, String> {
    let params = SincInterpolationParameters {
        sinc_len: 128,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Cubic,
        oversampling_factor: 64,
        window: WindowFunction::BlackmanHarris2,
    };
    let ratio = TARGET_SAMPLE_RATE as f64 / native_rate as f64;
    let chunk = mono.len().max(params.sinc_len * 4);
    let mut resampler = SincFixedIn::<f32>::new(ratio, 2.0, params, chunk, 1)
        .map_err(|e| format!("resampler init: {e}"))?;

    // Pad input up to chunk size so the fixed-chunk resampler accepts it.
    let mut padded = mono;
    padded.resize(chunk, 0.0);

    let out = resampler
        .process(&[padded], None)
        .map_err(|e| format!("resampler process: {e}"))?;

    Ok(out.into_iter().next().unwrap_or_default())
}

static ENGINE: OnceLock<AudioEngine> = OnceLock::new();

fn engine() -> &'static AudioEngine {
    ENGINE.get_or_init(AudioEngine::new)
}

pub fn audio_start_capture() -> Result<(), String> {
    LEVEL_BITS.store(0, Ordering::Relaxed);
    engine().start()
}

pub fn audio_stop_capture() {
    if let Some(e) = ENGINE.get() {
        e.stop();
    }
    LEVEL_BITS.store(0, Ordering::Relaxed);
}

pub fn audio_drain_samples() -> Vec<f32> {
    ENGINE.get().map(|e| e.drain()).unwrap_or_default()
}

pub fn audio_is_capturing() -> bool {
    ENGINE.get().map(|e| e.is_capturing()).unwrap_or(false)
}

/// Returns the RMS level of the most recent audio callback, in [0, 1].
/// Lock-free, safe to poll from a UI timer.
pub fn audio_current_level() -> f32 {
    f32::from_bits(LEVEL_BITS.load(Ordering::Relaxed))
}
