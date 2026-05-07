//! `openwhisper settings <subcommand>` — read and write the same
//! `settings.json` the Tauri shell uses.
//!
//! Subcommands:
//! - `get-performance` — print the Performance block (currently
//!   `keep_models_warm`).
//! - `set-keep-models-warm <true|false>` — flip the keep-warm flag,
//!   persist, and apply to any registered `ModelHandle` in this
//!   process. CLI invocations are short-lived so the in-process
//!   "apply" only matters for any handles spun up later in the same
//!   command — but the persistence is the load-bearing effect: the
//!   GUI shell reads the same file on next launch.
//!
//! Path resolution mirrors Tauri's `app.path().app_config_dir()`
//! using the `dirs` crate + the `com.openwhisper.app` bundle id.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use openwhisper_core::settings::{self, PerformanceSettings};

const BUNDLE_ID: &str = "com.openwhisper.app";

#[derive(Args, Debug)]
pub struct SettingsArgs {
    #[command(subcommand)]
    pub command: SettingsCommand,
}

#[derive(Subcommand, Debug)]
pub enum SettingsCommand {
    /// Print the persisted Performance block.
    GetPerformance,
    /// Set the `keep_models_warm` flag. Accepts `true` or `false`.
    SetKeepModelsWarm {
        #[arg(value_parser = parse_bool)]
        value: bool,
    },
}

fn parse_bool(s: &str) -> Result<bool, String> {
    match s.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        other => Err(format!(
            "expected true/false (or 1/0, yes/no, on/off); got {other}"
        )),
    }
}

pub fn run(args: SettingsArgs, json: bool) -> Result<()> {
    match args.command {
        SettingsCommand::GetPerformance => get_performance(json),
        SettingsCommand::SetKeepModelsWarm { value } => set_keep_models_warm(value, json),
    }
}

fn settings_path() -> Result<PathBuf> {
    let dir = dirs::config_dir().context("could not resolve OS config directory")?;
    Ok(dir.join(BUNDLE_ID).join("settings.json"))
}

fn get_performance(json: bool) -> Result<()> {
    let path = settings_path()?;
    let perf = settings::load_performance_settings(&path);
    if json {
        let value = serde_json::json!({
            "keep_models_warm": perf.keep_models_warm,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("keep_models_warm  {}", perf.keep_models_warm);
        println!("settings_path     {}", path.display());
    }
    Ok(())
}

fn set_keep_models_warm(value: bool, json: bool) -> Result<()> {
    let path = settings_path()?;
    settings::save_performance_settings(
        &path,
        PerformanceSettings {
            keep_models_warm: value,
        },
    )
    .map_err(|e| anyhow::anyhow!(e))
    .context("save performance settings")?;
    // Apply to any ModelHandle registered earlier in this same
    // process. CLI invocations don't keep a long-lived handle
    // around, but the call is cheap and keeps the headless
    // surface symmetrical with `settings_set_keep_models_warm` in
    // the Tauri shell.
    openwhisper_core::model_lifecycle::apply_keep_warm(value);
    if json {
        let v = serde_json::json!({ "keep_models_warm": value });
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else {
        println!("keep_models_warm  {}", value);
        println!("written           {}", path.display());
    }
    Ok(())
}
