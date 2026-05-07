//! `openwhisper memory` — print the running process's RSS + peak.
//!
//! Same primitive the in-app Diagnostics → Memory pane reads
//! (`core::telemetry::query_process_memory`). Surfaced headlessly so
//! external contributors can confirm the engine's footprint without
//! launching the desktop shell.
//!
//! Today: process-level only. Per-model rows land once
//! `model_lifecycle` exposes a registry (TASK-62.4 / TASK-62.7) and a
//! recognizer is wrapped in a `ModelHandle` (TASK-62.5 / TASK-62.6).

use anyhow::Result;
use openwhisper_core::telemetry::{query_process_memory, ProcessMemory};

pub fn run(json: bool) -> Result<()> {
    let m = query_process_memory();
    if json {
        print_json(&m)?;
    } else {
        print_text(&m);
    }
    Ok(())
}

fn print_text(m: &ProcessMemory) {
    println!("rss             {}", fmt_bytes(m.rss_bytes));
    println!("peak rss        {}", fmt_bytes(m.peak_rss_bytes));
    println!("rss_bytes       {}", m.rss_bytes);
    println!("peak_rss_bytes  {}", m.peak_rss_bytes);
}

fn print_json(m: &ProcessMemory) -> Result<()> {
    let unix_ms = m
        .timestamp
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i128)
        .unwrap_or(-1);
    let value = serde_json::json!({
        "rss_bytes": m.rss_bytes,
        "peak_rss_bytes": m.peak_rss_bytes,
        "timestamp_unix_ms": unix_ms,
    });
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

/// Human-friendly byte formatter — MB up to 1 GB, GB beyond. Tabular
/// numerics so a wide-terminal user can eyeball drift across calls.
fn fmt_bytes(n: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = MB * 1024.0;
    let f = n as f64;
    if f >= GB {
        format!("{:>8.2} GB", f / GB)
    } else {
        format!("{:>8.2} MB", f / MB)
    }
}
