//! `openwhisper transcribe <wav>` — offline batch transcribe over
//! the same recognizer the desktop uses.
//!
//! Format gate: WAV must already be 16 kHz mono (i16 or f32 PCM).
//! Anything else bails with an actionable ffmpeg-resample message.
//! Live capture in core re-samples cpal output internally, but the
//! sinc resampler isn't on the public surface — for v1 the CLI
//! pushes the format burden onto the caller.

use std::path::Path;
use std::time::Instant;

use anyhow::{anyhow, bail, Context, Result};
use clap::Args as ClapArgs;
use openwhisper_core::{recognizer, transcript};

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Path to a 16 kHz mono WAV file (i16 or f32 PCM).
    pub wav: std::path::PathBuf,

    /// Skip the transcript filter pipeline (filler stripping,
    /// substitutions, dedupe, whitespace normalize). Off by
    /// default — the CLI emits the same text the desktop shell
    /// would inject.
    #[arg(long)]
    pub raw: bool,
}

pub fn run(args: Args, json: bool) -> Result<()> {
    let samples = read_wav_16khz_mono(&args.wav)
        .with_context(|| format!("read {}", args.wav.display()))?;
    if samples.is_empty() {
        bail!("WAV decoded to zero samples — empty file?");
    }
    eprintln!(
        "[cli.transcribe] loaded {} samples (~{:.2}s)",
        samples.len(),
        samples.len() as f64 / 16_000.0,
    );

    let t_load = Instant::now();
    recognizer::recognizer_ensure_loaded()
        .map_err(|e| anyhow!(e))
        .context("recognizer init")?;
    eprintln!(
        "[cli.transcribe] recognizer ready in {} ms",
        t_load.elapsed().as_millis(),
    );

    let t_tx = Instant::now();
    let res = recognizer::recognizer_transcribe(&samples)
        .map_err(|e| anyhow!(e))
        .context("transcribe")?;
    let transcribe_ms = t_tx.elapsed().as_millis() as u64;
    eprintln!(
        "[cli.transcribe] decoded in {transcribe_ms} ms confidence={:.2}",
        res.confidence,
    );

    let text = if args.raw {
        res.text.trim().to_string()
    } else {
        // process() appends a trailing space so consecutive
        // injections don't fuse into one another. The CLI emits
        // a single result, so drop it for clean stdout.
        transcript::process(&res.text).trim().to_string()
    };

    if json {
        let value = serde_json::json!({
            "text": text,
            "confidence": res.confidence,
            "duration_ms": transcribe_ms,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("{text}");
    }
    Ok(())
}

fn read_wav_16khz_mono(path: &Path) -> Result<Vec<f32>> {
    let reader = hound::WavReader::open(path)
        .with_context(|| format!("open WAV: {}", path.display()))?;
    let spec = reader.spec();
    if spec.sample_rate != 16_000 {
        bail!(
            "WAV must be 16 kHz; got sample_rate={}. Resample with: \
             `ffmpeg -i <in> -ar 16000 -ac 1 -sample_fmt s16 out.wav`",
            spec.sample_rate,
        );
    }
    if spec.channels != 1 {
        bail!(
            "WAV must be mono; got channels={}. Downmix with: \
             `ffmpeg -i <in> -ar 16000 -ac 1 -sample_fmt s16 out.wav`",
            spec.channels,
        );
    }
    decode_samples(reader, spec)
}

fn decode_samples<R: std::io::Read>(
    mut reader: hound::WavReader<R>,
    spec: hound::WavSpec,
) -> Result<Vec<f32>> {
    use hound::SampleFormat::{Float, Int};
    match (spec.sample_format, spec.bits_per_sample) {
        (Int, 16) => {
            let max = i16::MAX as f32;
            reader
                .samples::<i16>()
                .map(|r| r.map(|v| v as f32 / max).map_err(|e| anyhow!(e)))
                .collect()
        }
        (Int, 32) => {
            // 32-bit int PCM is rare for speech but valid; normalize
            // by max-positive i32.
            let max = i32::MAX as f32;
            reader
                .samples::<i32>()
                .map(|r| r.map(|v| v as f32 / max).map_err(|e| anyhow!(e)))
                .collect()
        }
        (Float, 32) => reader
            .samples::<f32>()
            .map(|r| r.map_err(|e| anyhow!(e)))
            .collect(),
        (fmt, bits) => bail!(
            "unsupported WAV format: {fmt:?} {bits}-bit. Re-encode as 16 kHz mono i16 PCM.",
        ),
    }
}
