//! `openwhisper transcribe <file>`: offline batch transcribe over
//! the same recognizer the desktop uses.
//!
//! Accepts any audio or video file `core::file_transcription` can
//! decode (wav, mp3, m4a, mp4/mov, ogg/opus, webm/mkv, flac, …) at any
//! sample rate or channel count. Long recordings are recognized in
//! silence-aligned chunks; progress goes to stderr.

use std::sync::atomic::AtomicBool;

use anyhow::{anyhow, Result};
use clap::Args as ClapArgs;
use openwhisper_core::file_transcription::{
    transcribe_file, FileProgress, FileTranscribeOptions,
};

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Path to an audio or video file (wav, mp3, m4a, mp4, mov, ogg,
    /// opus, webm, mkv, flac, aiff, caf).
    pub file: std::path::PathBuf,

    /// Skip the transcript filter pipeline (filler stripping,
    /// substitutions, dedupe, whitespace normalize). Off by
    /// default; the CLI emits the same text the desktop shell
    /// would show.
    #[arg(long)]
    pub raw: bool,
}

pub fn run(args: Args, json: bool) -> Result<()> {
    let opts = FileTranscribeOptions::default().raw(args.raw);
    let never_cancel = AtomicBool::new(false);
    let mut on_progress = |p: FileProgress| match p {
        FileProgress::Decoding => eprintln!("[cli.transcribe] decoding {}", args.file.display()),
        FileProgress::Decoded { audio_ms } => {
            eprintln!("[cli.transcribe] decoded ~{:.2}s of audio", audio_ms as f64 / 1000.0)
        }
        FileProgress::LoadingModel => eprintln!("[cli.transcribe] loading recognizer"),
        FileProgress::Transcribing { done, total } if done > 0 => {
            eprintln!("[cli.transcribe] chunk {done}/{total}")
        }
        _ => {}
    };

    let res = transcribe_file(&args.file, &opts, &never_cancel, &mut on_progress)
        .map_err(|e| anyhow!("transcribe {}: {e}", args.file.display()))?;
    eprintln!(
        "[cli.transcribe] decoded in {} ms, recognized in {} ms, confidence={:.2}",
        res.decode_ms, res.transcribe_ms, res.confidence,
    );

    if json {
        let value = serde_json::json!({
            "text": res.text,
            "confidence": res.confidence,
            "duration_ms": res.transcribe_ms,
            "audio_ms": res.audio_ms,
            "decode_ms": res.decode_ms,
            "chunks": res.chunks,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("{}", res.text);
    }
    Ok(())
}
