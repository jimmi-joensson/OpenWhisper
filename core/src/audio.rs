//! Microphone capture for OpenWhisper.
//!
//! Owns a dedicated worker thread that holds the cpal stream. Swift drives
//! it via the pull-based FFI in [`crate::ffi`]: start, talk, stop + drain.
//! Output is always 16 kHz mono f32 so FluidAudio can consume it directly.

use std::sync::{Arc, Mutex, OnceLock, atomic::{AtomicU32, AtomicU64, Ordering}, mpsc};
use std::thread;
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{DeviceType, FromSample, InterfaceType, Sample, SizedSample};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

const TARGET_SAMPLE_RATE: u32 = 16_000;

// Peak amplitude of the most recent callback. Written by the CoreAudio callback,
// read from any thread via audio_current_level(). Stored as f32 bits so we can
// use a lock-free atomic. Peak reads cleaner on a UI meter than RMS because
// transients in speech (consonants, syllable onsets) show as real spikes.
static LEVEL_BITS: AtomicU32 = AtomicU32::new(0);

// Wall-clock time of the last LEVEL_BITS write, in nanoseconds since
// LEVEL_EPOCH. We use this to detect dead streams: virtual / aggregate
// devices that produce no callbacks at all (e.g. "Microsoft Teams Audio"
// while no call is in progress) would otherwise leave the meter frozen on
// the last sampled peak forever. audio_current_level() returns 0 once the
// most recent write is older than LEVEL_STALE_NS.
static LEVEL_LAST_WRITE_NS: AtomicU64 = AtomicU64::new(0);
const LEVEL_STALE_NS: u64 = 150_000_000; // 150 ms — a few callback periods

fn level_epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

// Selected input device by cpal ID. `None` = default. The ID is the stable
// identifier cpal exposes via `Device::id()` (Display-serialized) — survives
// reboots and reconnections, unlike the device name which changes when a
// driver is reinstalled. Looked up at begin_capture time; if no matching
// device is present we transparently fall back to the host default.
static SELECTED_DEVICE: Mutex<Option<String>> = Mutex::new(None);

#[derive(Clone, Debug)]
pub struct AudioDeviceInfo {
    /// Stable cpal device id (Display form: "host:device_id"). Used for
    /// persistence and lookup.
    pub id: String,
    /// Human-readable label shown in the picker. Mirrors what Discord and
    /// the Windows Sound control panel show — see `device_label`.
    pub label: String,
    pub is_default: bool,
}

/// Human-readable display label for a cpal device.
///
/// On Windows, cpal 0.17's WASAPI backend prefers the (often generic)
/// `PKEY_Device_DeviceDesc` for `DeviceDescription::name()` — both
/// "Microphone (SteelSeries Arctis 5 Chat)" and "Microphone (Steam
/// Streaming Microphone)" collapse to "Microphone", which makes the picker
/// useless when more than one capture endpoint is plugged in. The full
/// `PKEY_Device_FriendlyName` is preserved as the first extended line when
/// it differs, and that's what every other Windows audio app surfaces, so
/// we prefer it for the displayed label. On macOS / Linux the description
/// name is already unique and the extended-line check no-ops.
fn device_label(device: &cpal::Device) -> Option<String> {
    let desc = device.description().ok()?;
    let short = desc.name();
    if let Some(friendly) = desc.extended().first() {
        if friendly.len() > short.len() && friendly.contains(short) {
            return Some(friendly.clone());
        }
    }
    Some(short.to_string())
}

/// Stable cpal device id, serialized via `DeviceId::Display`. Round-trips
/// through `DeviceId::FromStr` (used by `find_input_device`).
fn device_id_string(device: &cpal::Device) -> Option<String> {
    device.id().ok().map(|id| id.to_string())
}

pub struct AudioEngine {
    ctrl_tx: mpsc::Sender<Ctrl>,
}

enum Ctrl {
    Start { preview: bool, reply: mpsc::SyncSender<Result<(), String>> },
    Stop(mpsc::SyncSender<()>),
    Drain(mpsc::SyncSender<Vec<f32>>),
    IsCapturing(mpsc::SyncSender<bool>),
    IsPreviewing(mpsc::SyncSender<bool>),
}

struct Capture {
    // `None` means the stream has been stopped but the captured samples
    // are still waiting in `buffer` for the next `drain`. This lets Swift
    // call stop → drain in that order without losing data.
    stream: Option<cpal::Stream>,
    buffer: Arc<Mutex<Vec<f32>>>,
    native_rate: u32,
    channels: u16,
    // Preview captures don't accumulate samples — the input callback only
    // updates LEVEL_BITS for the meter and skips the buffer push entirely.
    // Drain returns empty without resampling.
    preview: bool,
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
        self.rpc(|reply| Ctrl::Start { preview: false, reply })
    }

    pub fn start_preview(&self) -> Result<(), String> {
        self.rpc(|reply| Ctrl::Start { preview: true, reply })
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

    pub fn is_previewing(&self) -> bool {
        self.rpc(Ctrl::IsPreviewing)
    }
}

fn run_worker(rx: mpsc::Receiver<Ctrl>) {
    let mut capture: Option<Capture> = None;
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Ctrl::Start { preview, reply } => {
                // Re-using an existing capture only makes sense when the
                // mode matches. A mismatch (caller wants preview but a
                // recording is live, or vice-versa) surfaces as Err so the
                // UI can serialize them.
                if let Some(c) = capture.as_ref() {
                    if c.preview == preview {
                        let _ = reply.send(Ok(()));
                    } else if preview {
                        let _ = reply.send(Err("recording active".into()));
                    } else {
                        let _ = reply.send(Err("preview active".into()));
                    }
                    continue;
                }
                match begin_capture(preview) {
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
                let is_live = capture
                    .as_ref()
                    .is_some_and(|c| c.stream.is_some() && !c.preview);
                let _ = reply.send(is_live);
            }
            Ctrl::IsPreviewing(reply) => {
                let is_live = capture
                    .as_ref()
                    .is_some_and(|c| c.stream.is_some() && c.preview);
                let _ = reply.send(is_live);
            }
        }
    }
}

fn begin_capture(preview: bool) -> Result<Capture, String> {
    let host = cpal::default_host();
    let selected = SELECTED_DEVICE.lock().ok().and_then(|g| g.clone());
    let device = match selected.as_deref() {
        Some(id) => find_input_device(&host, id)
            .or_else(|| host.default_input_device())
            .ok_or_else(|| format!("no input device matched {id:?} and no default available"))?,
        None => host
            .default_input_device()
            .ok_or_else(|| "no default input device".to_string())?,
    };

    let device_label = device_label(&device).unwrap_or_else(|| "unknown".to_string());
    let supported = device
        .default_input_config()
        .map_err(|e| format!("default input config: {e}"))?;

    let sample_format = supported.sample_format();
    let native_rate = supported.sample_rate();
    let channels = supported.channels();
    eprintln!(
        "[openwhisper-core] mic: device={device_label:?} rate={native_rate} ch={channels} fmt={sample_format:?} preview={preview}"
    );
    let config: cpal::StreamConfig = supported.into();

    let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));

    let stream = match sample_format {
        cpal::SampleFormat::F32 => build_input_stream::<f32>(&device, &config, buffer.clone(), preview)?,
        cpal::SampleFormat::I16 => build_input_stream::<i16>(&device, &config, buffer.clone(), preview)?,
        cpal::SampleFormat::U16 => build_input_stream::<u16>(&device, &config, buffer.clone(), preview)?,
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
        preview,
    })
}

fn find_input_device(host: &cpal::Host, id_str: &str) -> Option<cpal::Device> {
    let target: cpal::DeviceId = id_str.parse().ok()?;
    host.input_devices()
        .ok()?
        .find(|d| d.id().ok().as_ref() == Some(&target))
}

fn build_input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    buffer: Arc<Mutex<Vec<f32>>>,
    preview: bool,
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
                if preview {
                    // Meter-only path: skip the buffer push entirely so the
                    // settings pane can hold the stream open indefinitely
                    // without unbounded memory growth.
                    for &s in data {
                        let f = f32::from_sample(s);
                        let abs = f.abs();
                        if abs > peak {
                            peak = abs;
                        }
                    }
                } else if let Ok(mut buf) = buffer.lock() {
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
                    let elapsed = level_epoch().elapsed().as_nanos() as u64;
                    LEVEL_LAST_WRITE_NS.store(elapsed, Ordering::Relaxed);
                }
            },
            err_fn,
            None,
        )
        .map_err(|e| format!("build_input_stream: {e}"))
}

fn drain_and_resample(capture: &Capture) -> Vec<f32> {
    // Preview captures never push samples into the buffer in the first
    // place, so there is nothing to drain or resample.
    if capture.preview {
        return Vec::new();
    }
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
    // A live preview held the cpal stream open for the meter; tear it down
    // before opening the recording stream so the worker doesn't reject the
    // start as "preview active". Mirrors the do_toggle BEGIN path: callers
    // shouldn't have to know about preview at all.
    audio_preview_stop();
    LEVEL_BITS.store(0, Ordering::Relaxed);
    LEVEL_LAST_WRITE_NS.store(0, Ordering::Relaxed);
    engine().start()
}

pub fn audio_stop_capture() {
    if let Some(e) = ENGINE.get() {
        e.stop();
    }
    LEVEL_BITS.store(0, Ordering::Relaxed);
    LEVEL_LAST_WRITE_NS.store(0, Ordering::Relaxed);
}

pub fn audio_drain_samples() -> Vec<f32> {
    ENGINE.get().map(|e| e.drain()).unwrap_or_default()
}

/// Estimate active-speech milliseconds in a sample buffer using an
/// energy-threshold VAD with hangover. Walks the buffer in 20 ms RMS
/// frames; a frame above `VAD_THRESHOLD_DBFS` (= -40 dBFS) opens the
/// active window AND arms a `HANGOVER_MS` (= 600 ms) hold. Subsequent
/// silent frames still count as active until the hold expires.
/// Hangover matches the user's mental model of "time spent speaking":
/// a 100–400 ms breath between words is part of the speech window,
/// not a silence to be stripped — but a true tail silence (≥600 ms
/// of continuous low-energy frames) closes the window cleanly.
///
/// Trailing partial frames (under 20 ms of leftover samples) are
/// ignored — they can't push the active count meaningfully and avoid
/// biasing very short clips.
///
/// Used by the stats writer to record "real speech time" in the
/// `dictations.duration_ms` column instead of wall-clock recording
/// duration. Wall-clock would credit silence at the tail of a
/// recording against the user's Time Saved metric — the user could
/// leave the mic on after speaking and watch their saved time
/// shrink. Energy-only (no spectral VAD, no neural model) so it's
/// model-agnostic — works regardless of which recognizer transcribes.
pub fn estimate_voiced_ms(samples: &[f32], sample_rate: u32) -> i64 {
    const FRAME_MS: u32 = 20;
    const VAD_THRESHOLD_DBFS: f32 = -40.0;
    const HANGOVER_MS: u32 = 600;
    if samples.is_empty() || sample_rate == 0 {
        return 0;
    }
    let frame_size = ((sample_rate as u64 * FRAME_MS as u64) / 1000) as usize;
    if frame_size == 0 {
        return 0;
    }
    let threshold_amp = 10f32.powf(VAD_THRESHOLD_DBFS / 20.0);
    let hangover_frames = (HANGOVER_MS / FRAME_MS) as i32;

    let mut active_frames: u64 = 0;
    let mut hangover_remaining: i32 = 0;
    for chunk in samples.chunks(frame_size) {
        if chunk.len() < frame_size {
            break;
        }
        let sum_sq: f32 = chunk.iter().map(|x| x * x).sum();
        let rms = (sum_sq / chunk.len() as f32).sqrt();
        if rms >= threshold_amp {
            hangover_remaining = hangover_frames;
            active_frames += 1;
        } else if hangover_remaining > 0 {
            hangover_remaining -= 1;
            active_frames += 1;
        }
    }
    (active_frames * FRAME_MS as u64) as i64
}

#[cfg(test)]
mod vad_tests {
    use super::estimate_voiced_ms;

    const SR: u32 = 16_000;
    const FRAME_MS: usize = 20;
    const SAMPLES_PER_FRAME: usize = (SR as usize * FRAME_MS) / 1000; // 320

    #[test]
    fn silence_only_returns_zero() {
        let samples = vec![0.0f32; SAMPLES_PER_FRAME * 50]; // 1 s of silence
        assert_eq!(estimate_voiced_ms(&samples, SR), 0);
    }

    #[test]
    fn pure_tone_above_threshold_counts_full_duration() {
        // 1 s of 440 Hz at 0.5 amplitude → RMS ~= 0.354 → ~-9 dBFS,
        // well above the -40 dBFS threshold.
        let total = SR as usize; // 1 s
        let samples: Vec<f32> = (0..total)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SR as f32).sin())
            .collect();
        let ms = estimate_voiced_ms(&samples, SR);
        assert!(
            (ms - 1000).abs() <= FRAME_MS as i64,
            "expected ~1000 ms voiced, got {ms}",
        );
    }

    #[test]
    fn voiced_then_long_silence_includes_hangover_then_closes() {
        // 1 s voiced + 2 s silence. After the last voiced frame the
        // hangover holds the active window open for 600 ms before the
        // continuous silence closes it. Result = 1000 ms voiced + 600
        // ms hangover = 1600 ms active.
        let voiced: Vec<f32> = (0..SR as usize)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SR as f32).sin())
            .collect();
        let silence = vec![0.0f32; SR as usize * 2];
        let mut combined = voiced;
        combined.extend(silence);
        let ms = estimate_voiced_ms(&combined, SR);
        assert!(
            (ms - 1600).abs() <= FRAME_MS as i64,
            "expected ~1600 ms (1000 ms voiced + 600 ms hangover) from 1 s speech + 2 s tail silence, got {ms}",
        );
    }

    #[test]
    fn short_inter_word_silence_is_bridged_by_hangover() {
        // Simulate a "speech-pause-speech" pattern: 200 ms voiced,
        // 300 ms silence (typical inter-word gap, well below 600 ms
        // hangover), 200 ms voiced. The hangover should hold the
        // active window through the whole gap, so all three segments
        // count as one continuous speech window of 700 ms.
        let make_voiced = |len: usize| -> Vec<f32> {
            (0..len)
                .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SR as f32).sin())
                .collect()
        };
        let voiced_a = make_voiced((SR as usize * 200) / 1000);
        let gap = vec![0.0f32; (SR as usize * 300) / 1000];
        let voiced_b = make_voiced((SR as usize * 200) / 1000);
        let mut combined = voiced_a;
        combined.extend(gap);
        combined.extend(voiced_b);
        let ms = estimate_voiced_ms(&combined, SR);
        assert!(
            (ms - 700).abs() <= FRAME_MS as i64,
            "expected ~700 ms (200 + 300 bridged + 200) from a normal between-word gap, got {ms}",
        );
    }

    #[test]
    fn very_quiet_signal_below_threshold_returns_zero() {
        // -60 dBFS sine = amplitude 0.001 — well below -40 dB threshold.
        let total = SR as usize;
        let samples: Vec<f32> = (0..total)
            .map(|i| 0.001 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SR as f32).sin())
            .collect();
        assert_eq!(estimate_voiced_ms(&samples, SR), 0);
    }

    #[test]
    fn empty_input_is_zero() {
        assert_eq!(estimate_voiced_ms(&[], SR), 0);
        assert_eq!(estimate_voiced_ms(&[0.0; 100], 0), 0);
    }
}

pub fn audio_is_capturing() -> bool {
    ENGINE.get().map(|e| e.is_capturing()).unwrap_or(false)
}

/// Begin a meter-only capture for the Settings → Audio live preview. The
/// input callback updates the global level but skips the sample buffer, so
/// holding the stream open while the pane is mounted is bounded in memory.
/// Errors if a recording is already in flight.
pub fn audio_preview_start() -> Result<(), String> {
    LEVEL_BITS.store(0, Ordering::Relaxed);
    LEVEL_LAST_WRITE_NS.store(0, Ordering::Relaxed);
    engine().start_preview()
}

pub fn audio_preview_stop() {
    if let Some(e) = ENGINE.get() {
        if e.is_previewing() {
            e.stop();
            // Drain releases the capture slot in the worker. Preview
            // buffers are always empty, so the returned Vec is discarded.
            let _ = e.drain();
        }
    }
    LEVEL_BITS.store(0, Ordering::Relaxed);
    LEVEL_LAST_WRITE_NS.store(0, Ordering::Relaxed);
}

/// List input devices visible to cpal, filtered to ones a user is likely
/// to actually want to dictate into. `is_default` flags the host's current
/// default. Returned in host enumeration order.
///
/// Filters applied:
///   * cpal `DeviceType::Virtual` — software-only routes (e.g. Teams /
///     Zoom virtual mics) that produce no audio outside their host app.
///   * cpal `InterfaceType::Virtual` — same idea on the connection axis.
///   * Devices whose `default_input_config()` errors — they can't be
///     opened, so listing them is a footgun.
///
/// We deliberately keep `InterfaceType::Aggregate` (legitimate multi-mic
/// combinations on macOS) and any `DeviceType::Unknown` entries (most
/// real USB mics report `Unknown` because cpal can't always classify
/// them) — overfiltering would hide working hardware.
pub fn audio_list_input_devices() -> Vec<AudioDeviceInfo> {
    // Refresh the macOS virtual-device cache once per enumeration so a
    // newly-plugged device is reflected on the next list call.
    #[cfg(target_os = "macos")]
    mac_virtual::refresh();

    let host = cpal::default_host();
    let default_id = host
        .default_input_device()
        .and_then(|d| device_id_string(&d));
    let Ok(devices) = host.input_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|d| {
            let id = device_id_string(&d)?;
            let label = device_label(&d)?;
            if is_virtual_device(&d) {
                return None;
            }
            // Probe-open: if cpal can't even resolve a default input
            // config, opening a stream will fail too. Drop the entry
            // rather than letting the user pick a dead end.
            if d.default_input_config().is_err() {
                return None;
            }
            let is_default = default_id.as_deref() == Some(id.as_str());
            Some(AudioDeviceInfo { id, label, is_default })
        })
        .collect()
}

fn is_virtual_device(device: &cpal::Device) -> bool {
    if let Ok(desc) = device.description() {
        if matches!(desc.device_type(), DeviceType::Virtual)
            || matches!(desc.interface_type(), InterfaceType::Virtual)
        {
            return true;
        }
    }
    #[cfg(target_os = "macos")]
    {
        // The CoreAudio name lookup keys mac_virtual's cache, so use the
        // raw description name (matches `get_device_name(id)` from
        // coreaudio-rs) rather than our display label.
        if let Some(name) = device.description().ok().map(|d| d.name().to_string()) {
            return mac_virtual::is_virtual_named(&name);
        }
    }
    false
}

// macOS-specific virtual-mic detection. cpal's macOS backend doesn't set
// `DeviceType` or set `InterfaceType` to anything other than Aggregate,
// so devices like "Microsoft Teams Audio" / "ZoomAudioDevice" / "Krisp"
// slip through the cpal-level filter. CoreAudio's
// `kAudioDevicePropertyTransportType` distinguishes them — we query it
// directly via coreaudio-rs and cache the names of devices that report
// `kAudioDeviceTransportTypeVirtual`. Cached for the lifetime of the
// list call (cheap; only invalidates on a fresh enumeration).
#[cfg(target_os = "macos")]
mod mac_virtual {
    use coreaudio::audio_unit::macos_helpers::{
        get_audio_device_ids, get_device_name, get_device_transport_type,
    };
    use std::collections::HashSet;
    use std::sync::Mutex;

    // CoreAudio four-char code 'virt' (0x76697274). Matches the constant in
    // `objc2-core-audio::kAudioDeviceTransportTypeVirtual` — duplicated
    // here so we don't pull in another transitive dep just for one u32.
    const VIRTUAL_TRANSPORT: u32 = 0x76697274;

    static CACHE: Mutex<Option<HashSet<String>>> = Mutex::new(None);

    fn collect_virtual_names() -> HashSet<String> {
        let mut set = HashSet::new();
        let Ok(ids) = get_audio_device_ids() else {
            return set;
        };
        for id in ids {
            match get_device_transport_type(id) {
                Ok(t) if t == VIRTUAL_TRANSPORT => {
                    if let Ok(name) = get_device_name(id) {
                        set.insert(name);
                    }
                }
                _ => {}
            }
        }
        set
    }

    pub fn is_virtual_named(name: &str) -> bool {
        // Build the cache lazily on first lookup, then re-use for the rest
        // of this enumeration pass. We deliberately don't expose a
        // refresh hook — `audio_list_input_devices` calls `refresh` at
        // the start of each list call so a hot-plugged device is picked
        // up next time the user opens the Audio pane.
        let mut guard = CACHE.lock().ok();
        if let Some(g) = guard.as_mut() {
            let set = g.get_or_insert_with(collect_virtual_names);
            return set.contains(name);
        }
        false
    }

    pub fn refresh() {
        if let Ok(mut g) = CACHE.lock() {
            *g = Some(collect_virtual_names());
        }
    }
}

/// Persisted device picker — `None` means "use the host default at
/// begin_capture time". The argument is the cpal device id string (Display
/// form). Effective on the next stream open; the live stream (if any) is
/// unaffected. Callers that want the change to take effect immediately
/// should stop+start preview themselves.
pub fn audio_set_selected_device_id(id: Option<String>) {
    if let Ok(mut g) = SELECTED_DEVICE.lock() {
        *g = id;
    }
}

pub fn audio_get_selected_device_id() -> Option<String> {
    SELECTED_DEVICE.lock().ok().and_then(|g| g.clone())
}

/// Display label of the host's current default input device, or `None` if
/// no default is reported. Used by the Settings UI to render the
/// "System default (<device label>)" option à la Discord. The default can
/// change while the app is running (user toggles a Bluetooth headset,
/// AirPods auto-route on connect), so callers that surface this in the UI
/// should poll and refresh rather than caching the boot-time value.
pub fn audio_default_input_label() -> Option<String> {
    let host = cpal::default_host();
    host.default_input_device().and_then(|d| device_label(&d))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectedDeviceStatus {
    /// Saved id matches a device in the current input list.
    Present,
    /// Saved id doesn't match any present input device (unplugged,
    /// virtual mic gone). The next `begin_capture` will silently fall
    /// back to host default; the saved id is preserved so a re-plug
    /// auto-resumes intent.
    MissingFallbackToDefault,
    /// No persisted selection — capture uses host default by design.
    NoneSelectedUsingDefault,
}

/// Snapshot of whether the persisted device pick is currently usable.
/// Cheap enough to call from a 0.5 Hz watcher (one cpal enumerate + a
/// linear scan over the device list). Does NOT mutate `SELECTED_DEVICE`
/// on a miss — preserving the saved name lets a re-plugged mic
/// auto-rebind without the user re-picking.
pub fn audio_selected_device_status() -> SelectedDeviceStatus {
    let Some(id_str) = audio_get_selected_device_id() else {
        return SelectedDeviceStatus::NoneSelectedUsingDefault;
    };
    let Ok(target) = id_str.parse::<cpal::DeviceId>() else {
        return SelectedDeviceStatus::MissingFallbackToDefault;
    };
    let host = cpal::default_host();
    let Ok(devices) = host.input_devices() else {
        return SelectedDeviceStatus::MissingFallbackToDefault;
    };
    for d in devices {
        if d.id().ok().as_ref() == Some(&target) {
            return SelectedDeviceStatus::Present;
        }
    }
    SelectedDeviceStatus::MissingFallbackToDefault
}

/// Returns the peak amplitude of the most recent audio callback, in [0, 1].
/// Lock-free, safe to poll from a UI timer. Returns 0 if the most recent
/// callback is older than `LEVEL_STALE_NS`, so a stream that suddenly
/// stops delivering data (virtual mic, USB unplug) drains the meter to
/// baseline instead of holding the last peak.
pub fn audio_current_level() -> f32 {
    let last = LEVEL_LAST_WRITE_NS.load(Ordering::Relaxed);
    if last == 0 {
        return 0.0;
    }
    let now = level_epoch().elapsed().as_nanos() as u64;
    if now.saturating_sub(last) > LEVEL_STALE_NS {
        return 0.0;
    }
    f32::from_bits(LEVEL_BITS.load(Ordering::Relaxed))
}
