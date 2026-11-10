//! `openwhisper` — headless CLI over the same `openwhisper-core`
//! library the Tauri desktop shell consumes.
//!
//! By construction: every feature available in the GUI is available
//! via CLI is available in the public library. No CLI-private logic
//! lives in `core/`; no GUI-private logic lives in `apps/tauri`.
//!
//! Subcommands (v1):
//!
//! - `transcribe <file>`: offline batch transcribe of any audio or
//!   video file; prints the text to stdout (or a JSON record with
//!   `--json`).
//! - `enumerate-devices` — list input devices the engine sees, one
//!   per line (or a JSON array with `--json`). Same filter rules as
//!   the desktop's Audio settings pane.
//! - `recognizer-info` — print the active engine, model version, and
//!   execution provider after the recognizer loads.
//! - `memory` — print the running process's RSS + peak. Same readout
//!   the Diagnostics → Memory pane consumes.
//! - `settings` — read/write the persisted settings.json (currently
//!   the Performance block; more knobs added as needed). Same file
//!   the Tauri shell consumes.
//! - `crash-dump` — list / read crash files (placeholder until
//!   TASK-78 lands a concrete reader).
//! - `refine`: the optional language-model cleanup step (TASK-93).
//!   Run text through it, check and test the backend, edit its
//!   settings, store the API key, run the live eval set.
//! - `history`: the opt-in transcript history (TASK-95), a folder of
//!   Markdown day files. List, search, show, delete and clear stored
//!   dictations; switch it on or off, set the retention and the
//!   folder.
//! - `focus`: print which app is frontmost and its window title
//!   (TASK-94).
//! - `modes`: the modes of the Refine step (TASK-96). List, add, edit
//!   and delete your own modes, select one or cycle to the next, give
//!   an app a default mode, and ask which mode a dictation would run
//!   in. `refine run --mode <id>` runs text through one.
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
    /// Transcribe an audio or video file and print the text to stdout.
    Transcribe(commands::transcribe::Args),
    /// List the input devices the recognizer would consider.
    EnumerateDevices,
    /// Print active engine, model version, and execution provider.
    RecognizerInfo,
    /// Print the running process's RSS + peak.
    Memory(commands::memory::MemoryArgs),
    /// Read/write the persisted settings.json (Performance block).
    Settings(commands::settings::SettingsArgs),
    /// Inspect on-disk crash dumps written by the panic hook.
    CrashDump(commands::crash_dump::Args),
    /// Refine: clean up a transcript with a language model.
    Refine(commands::refine::RefineArgs),
    /// Transcript history: list, search, delete, switch on or off.
    History(commands::history::HistoryArgs),
    /// Print which app is frontmost right now.
    Focus(commands::focus::FocusArgs),
    /// Modes: what Refine does with a dictation, and in which app.
    Modes(commands::modes::ModesArgs),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Transcribe(args) => commands::transcribe::run(args, cli.json),
        Command::EnumerateDevices => commands::enumerate_devices::run(cli.json),
        Command::RecognizerInfo => commands::recognizer_info::run(cli.json),
        Command::Memory(args) => commands::memory::run(args, cli.json),
        Command::Settings(args) => commands::settings::run(args, cli.json),
        Command::CrashDump(args) => commands::crash_dump::run(args, cli.json),
        Command::Refine(args) => commands::refine::run(args, cli.json),
        Command::History(args) => commands::history::run(args, cli.json),
        Command::Focus(args) => commands::focus::run(args, cli.json),
        Command::Modes(args) => commands::modes::run(args, cli.json),
    }
}
