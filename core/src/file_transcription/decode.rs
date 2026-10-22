//! Container demux + codec decode to 16 kHz mono f32.
//!
//! Symphonia handles every container and most codecs. Opus is the one
//! gap that matters (WhatsApp voice notes, WebM audio): symphonia
//! demuxes it fine but ships no decoder, so Opus tracks are routed to
//! the pure-Rust `opus-decoder` crate, which decodes straight to
//! 16 kHz mono and skips the resampler entirely.
//!
//! Everything else is downmixed per packet and streamed through a
//! rubato sinc resampler in fixed blocks, so a long 48 kHz recording
//! never sits in memory at its native rate.

use std::fs::File;
use std::path::Path;

use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use symphonia::core::audio::GenericAudioBufferRef;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use super::TARGET_SAMPLE_RATE;

/// Frames fed to the resampler per call. Small enough that the
/// native-rate staging buffer stays tiny, large enough that per-call
/// overhead is noise.
const RESAMPLE_BLOCK: usize = 8192;

/// Opus always runs its internal clock at 48 kHz; `pre_skip` in the
/// OpusHead is expressed in 48 kHz samples regardless of the rate we
/// ask the decoder to output.
const OPUS_INTERNAL_RATE: u32 = 48_000;

/// Decoded audio plus a little provenance for diagnostics.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    /// 16 kHz mono f32, ready for `recognizer_transcribe`.
    pub samples: Vec<f32>,
    /// Sample rate of the source track before resampling.
    pub source_rate: u32,
    /// Channel count of the source track before downmixing.
    pub source_channels: usize,
}

impl DecodedAudio {
    /// Duration of the decoded audio in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        self.samples.len() as u64 * 1000 / TARGET_SAMPLE_RATE as u64
    }
}

/// Decode the first audio track of `path` to 16 kHz mono f32.
///
/// Works for audio files and for video files (the video track is
/// ignored). Fails with a readable message when the container or
/// codec isn't supported or the file has no audio track.
pub fn decode_media_file(path: &Path) -> Result<DecodedAudio, String> {
    let file = File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| format!("unsupported or unreadable file format ({e})"))?;

    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| "file has no audio track".to_string())?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| "audio track is missing codec parameters".to_string())?
        .clone();
    let delay = track.delay.unwrap_or(0);

    let source_channels = params.channels.as_ref().map(|c| c.count()).unwrap_or(1).max(1);

    if params.codec == CODEC_ID_OPUS {
        let samples = decode_opus(format.as_mut(), track_id, source_channels, delay)?;
        return Ok(DecodedAudio {
            samples,
            source_rate: OPUS_INTERNAL_RATE,
            source_channels,
        });
    }

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|e| format!("unsupported audio codec ({e})"))?;

    let mut sink: Option<MonoResampler> = None;
    let mut source_rate = params.sample_rate.unwrap_or(0);
    let mut interleaved: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            // A truncated tail (interrupted download, live recording
            // still being written) still yields everything before it.
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(format!("read packet: {e}")),
        };
        if packet.track_id != track_id {
            continue;
        }
        let buf = match decoder.decode(&packet) {
            Ok(buf) => buf,
            // Corrupt packet: skip it rather than failing the whole file.
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => return Err(format!("decode: {e}")),
        };
        if buf.frames() == 0 {
            continue;
        }
        let rate = buf.spec().rate();
        let channels = buf.spec().channels().count().max(1);
        let sink = sink.get_or_insert_with(|| {
            source_rate = rate;
            MonoResampler::new(rate)
        });
        downmix_packet(&buf, channels, &mut interleaved, sink)?;
    }

    let samples = match sink {
        Some(sink) => sink.finish()?,
        None => Vec::new(),
    };
    Ok(DecodedAudio {
        samples,
        source_rate,
        source_channels,
    })
}

fn downmix_packet(
    buf: &GenericAudioBufferRef<'_>,
    channels: usize,
    scratch: &mut Vec<f32>,
    sink: &mut MonoResampler,
) -> Result<(), String> {
    scratch.resize(buf.samples_interleaved(), 0.0);
    buf.copy_to_slice_interleaved(&mut scratch[..]);
    if channels == 1 {
        return sink.push(scratch);
    }
    let inv = 1.0 / channels as f32;
    let mono: Vec<f32> = scratch
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() * inv)
        .collect();
    sink.push(&mono)
}

fn decode_opus(
    format: &mut dyn FormatReader,
    track_id: u32,
    channels: usize,
    pre_skip_48k: u32,
) -> Result<Vec<f32>, String> {
    if channels > 2 {
        return Err(format!(
            "Opus with {channels} channels (surround) isn't supported; export as mono or stereo"
        ));
    }
    // Ask for 16 kHz mono directly; the decoder downmixes stereo.
    let mut decoder = opus_decoder::OpusDecoder::new(TARGET_SAMPLE_RATE, 1)
        .map_err(|e| format!("opus decoder init: {e:?}"))?;
    let mut frame = vec![0f32; decoder.max_frame_size_per_channel()];
    let mut out = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(format!("read packet: {e}")),
        };
        if packet.track_id != track_id {
            continue;
        }
        match decoder.decode_float(&packet.data, &mut frame, false) {
            Ok(n) => out.extend_from_slice(&frame[..n]),
            // Same policy as the symphonia path: drop a bad packet,
            // keep the rest of the recording.
            Err(_) => continue,
        }
    }

    let skip = (pre_skip_48k as usize * TARGET_SAMPLE_RATE as usize)
        / OPUS_INTERNAL_RATE as usize;
    out.drain(..skip.min(out.len()));
    Ok(out)
}

/// Streams mono samples at the source rate into 16 kHz output.
struct MonoResampler {
    inner: Option<SincFixedIn<f32>>,
    pending: Vec<f32>,
    out: Vec<f32>,
    /// Source frames pushed in total, used to trim the flushed tail to
    /// the exact expected output length.
    frames_in: usize,
    source_rate: u32,
}

impl MonoResampler {
    fn new(source_rate: u32) -> Self {
        let inner = (source_rate != TARGET_SAMPLE_RATE).then(|| {
            let params = SincInterpolationParameters {
                sinc_len: 128,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Cubic,
                oversampling_factor: 64,
                window: WindowFunction::BlackmanHarris2,
            };
            let ratio = TARGET_SAMPLE_RATE as f64 / source_rate as f64;
            SincFixedIn::<f32>::new(ratio, 1.0, params, RESAMPLE_BLOCK, 1)
                .expect("sinc resampler params are static and valid")
        });
        Self {
            inner,
            pending: Vec::new(),
            out: Vec::new(),
            frames_in: 0,
            source_rate,
        }
    }

    fn push(&mut self, mono: &[f32]) -> Result<(), String> {
        self.frames_in += mono.len();
        let Some(rs) = self.inner.as_mut() else {
            self.out.extend_from_slice(mono);
            return Ok(());
        };
        self.pending.extend_from_slice(mono);
        let mut offset = 0;
        while self.pending.len() - offset >= RESAMPLE_BLOCK {
            let block = &self.pending[offset..offset + RESAMPLE_BLOCK];
            let res = rs
                .process(&[block], None)
                .map_err(|e| format!("resample: {e}"))?;
            self.out.extend_from_slice(&res[0]);
            offset += RESAMPLE_BLOCK;
        }
        self.pending.drain(..offset);
        Ok(())
    }

    fn finish(mut self) -> Result<Vec<f32>, String> {
        let Some(mut rs) = self.inner.take() else {
            return Ok(self.out);
        };
        let delay = rs.output_delay();
        let expected = (self.frames_in as u64 * TARGET_SAMPLE_RATE as u64
            / self.source_rate as u64) as usize;
        if !self.pending.is_empty() {
            let res = rs
                .process_partial(Some(&[&self.pending[..]]), None)
                .map_err(|e| format!("resample tail: {e}"))?;
            self.out.extend_from_slice(&res[0]);
        }
        // Flush the filter's group delay so the last ~ms of speech
        // isn't clipped.
        while self.out.len() < expected + delay {
            let res = rs
                .process_partial(None::<&[&[f32]]>, None)
                .map_err(|e| format!("resample flush: {e}"))?;
            if res[0].is_empty() {
                break;
            }
            self.out.extend_from_slice(&res[0]);
        }
        self.out.drain(..delay.min(self.out.len()));
        self.out.truncate(expected);
        Ok(self.out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampler_passthrough_at_target_rate() {
        let mut rs = MonoResampler::new(TARGET_SAMPLE_RATE);
        rs.push(&[0.1, 0.2, 0.3]).unwrap();
        assert_eq!(rs.finish().unwrap(), vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn resampler_48k_to_16k_keeps_length_and_tone() {
        // 1.3 s of 440 Hz at 48 kHz, pushed in odd-sized packets so the
        // block staging + tail flush both run.
        let n = 62_400;
        let tone: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48_000.0).sin() * 0.5)
            .collect();
        let mut rs = MonoResampler::new(48_000);
        for chunk in tone.chunks(1023) {
            rs.push(chunk).unwrap();
        }
        let out = rs.finish().unwrap();
        assert_eq!(out.len(), n / 3);
        // Energy should survive resampling (RMS of a 0.5 sine ≈ 0.354).
        let mid = &out[2000..out.len() - 2000];
        let rms = (mid.iter().map(|s| s * s).sum::<f32>() / mid.len() as f32).sqrt();
        assert!((rms - 0.354).abs() < 0.02, "rms={rms}");
    }

    #[test]
    fn missing_file_is_a_readable_error() {
        let err = decode_media_file(Path::new("/definitely/not/here.opus")).unwrap_err();
        assert!(err.starts_with("open "), "{err}");
    }
}
