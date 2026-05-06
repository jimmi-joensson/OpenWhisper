//! `openwhisper recognizer-info` — print the active engine, model
//! version, and execution provider after the recognizer has loaded.
//!
//! Calls `core::recognizer::recognizer_ensure_loaded` first so the
//! `ep` field is populated from the live probe outcome (Mac always
//! "ANE"; Windows depends on which EP succeeded).

use anyhow::{Context, Result};
use openwhisper_core::diagnostics::{self, RecognizerInfo};
use openwhisper_core::recognizer;

pub fn run(json: bool) -> Result<()> {
    recognizer::recognizer_ensure_loaded()
        .map_err(|e| anyhow::anyhow!(e))
        .context("recognizer init")?;
    let info = diagnostics::recognizer_info();
    if json {
        print_json(&info)?;
    } else {
        print_text(&info);
    }
    Ok(())
}

fn print_text(info: &RecognizerInfo) {
    println!("engine          {}", info.engine);
    println!("model version   {}", info.model_version);
    println!(
        "model path      {}",
        info.model_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<unknown>".into())
    );
    println!(
        "execution prov  {}",
        info.ep.as_deref().unwrap_or("<unknown>")
    );
}

fn print_json(info: &RecognizerInfo) -> Result<()> {
    let value = serde_json::json!({
        "engine": info.engine,
        "model_version": info.model_version,
        "model_path": info.model_path.as_ref().map(|p| p.display().to_string()),
        "ep": info.ep,
    });
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
