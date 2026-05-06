//! `openwhisper` — headless CLI over the same `openwhisper-core`
//! library the Tauri desktop shell consumes.
//!
//! By construction: every feature available in the GUI is available
//! via CLI is available in the public library. No CLI-private logic
//! lives in `core/`; no GUI-private logic lives in `apps/tauri`.
//!
//! Subcommands (v1):
//!
//! - `transcribe <wav>` — offline batch transcribe; prints the text
//!   to stdout (or a JSON record with `--json`).
//! - `enumerate-devices` — list input devices the engine sees, one
//!   per line (or a JSON array with `--json`). Same filter rules as
//!   the desktop's Audio settings pane.
//! - `recognizer-info` — print the active engine, model version, and
//!   execution provider after the recognizer loads.
//! - `crash-dump` — list / read crash files (placeholder until
//!   TASK-78 lands a concrete reader).
//!
//! Convention: stdout = data (the answer), stderr = log (verbose
//! traces, errors). Pipelines like `openwhisper transcribe --json
//! sample.wav | jq .text` work without a special flag.

use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser, Debug)]
#[command(
    name = "openwhisper",
    version,
    about = "Headless OpenWhisper — record, transcribe, inspect."
)]
struct Cli {
    /// Emit machine-readable JSON to stdout instead of human text.
    /// Stderr stays plain regardless.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Decode a WAV file and print the transcript to stdout.
    Transcribe(commands::transcribe::Args),
    /// List the input devices the recognizer would consider.
    EnumerateDevices,
    /// Print active engine, model version, and execution provider.
    RecognizerInfo,
    /// Inspect on-disk crash dumps (placeholder until TASK-78).
    CrashDump(commands::crash_dump::Args),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Transcribe(args) => commands::transcribe::run(args, cli.json),
        Command::EnumerateDevices => commands::enumerate_devices::run(cli.json),
        Command::RecognizerInfo => commands::recognizer_info::run(cli.json),
        Command::CrashDump(args) => commands::crash_dump::run(args, cli.json),
    }
}
