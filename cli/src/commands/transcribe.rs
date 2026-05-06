//! `openwhisper transcribe <wav>` — placeholder until Task 5 lands
//! the Mac (FluidAudio) and Windows (ort+sherpa-onnx) handlers.

use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::Args as ClapArgs;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Path to a 16 kHz mono WAV file.
    pub wav: PathBuf,
}

pub fn run(_args: Args, _json: bool) -> Result<()> {
    bail!("`transcribe` lands in TASK-81.2 / Task 5 — see backlog/tasks/task-81.5*")
}
