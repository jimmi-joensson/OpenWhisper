//! `openwhisper memory` — print the running process's RSS + peak,
//! optionally with per-model rows.
//!
//! Same primitive the in-app Diagnostics → Memory pane reads
//! (`core::telemetry::collect_memory_stats`). Surfaced headlessly so
//! external contributors can confirm the engine's footprint without
//! launching the desktop shell.
//!
//! With no flag: process-level only (RSS + peak). Useful for cheap
//! probes that don't want to load the recognizer.
//!
//! With `--models`: also calls `recognizer_ensure_loaded` so the
//! `ModelHandle` registry has the recognizer in it, then prints one
//! row per registered model (label / state / estimated RSS delta).
//! In a fresh process the recognizer is the only entry; future LLM
//! handles will appear here too.

use anyhow::{Context, Result};
use clap::Args;
use openwhisper_core::telemetry::{
    collect_memory_stats, query_process_memory, query_system_memory, MemoryStats, ModelMemoryRow,
    ProcessMemory, SystemMemory,
};

#[derive(Args, Debug)]
pub struct MemoryArgs {
    /// Also print per-model rows from the ModelHandle registry. Loads
    /// the recognizer first (so the registry has it) — adds Mac
    /// 200-500 ms / Win 100-300 ms cold-load latency on first run.
    #[arg(long)]
    pub models: bool,
}

pub fn run(args: MemoryArgs, json: bool) -> Result<()> {
    if args.models {
        // Force the recognizer into the registry so its row appears.
        // Cheap on warm cache; on the first cold run this downloads +
        // loads the model.
        openwhisper_core::recognizer::recognizer_ensure_loaded()
            .map_err(|e| anyhow::anyhow!(e))
            .context("recognizer ensure_loaded")?;
        let stats = collect_memory_stats();
        if json {
            println!("{}", serde_json::to_string_pretty(&stats)?);
        } else {
            print_full_text(&stats);
        }
        return Ok(());
    }
    let process = query_process_memory();
    let system = query_system_memory();
    if json {
        let payload = serde_json::json!({
            "system": system,
            "process": process,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        print_system(&system);
        println!();
        print_process(&process);
    }
    Ok(())
}

fn print_system(s: &SystemMemory) {
    println!("system total    {}", fmt_bytes(s.total_bytes));
    println!("system used     {}", fmt_bytes(s.used_bytes));
    println!("system avail    {}", fmt_bytes(s.available_bytes));
    println!("swap used       {}", fmt_bytes(s.swap_used_bytes));
    println!("swap total      {}", fmt_bytes(s.swap_total_bytes));
}

fn print_process(m: &ProcessMemory) {
    println!("rss             {}", fmt_bytes(m.rss_bytes));
    println!("peak rss        {}", fmt_bytes(m.peak_rss_bytes));
    println!("rss_bytes       {}", m.rss_bytes);
    println!("peak_rss_bytes  {}", m.peak_rss_bytes);
}

fn print_full_text(stats: &MemoryStats) {
    print_system(&stats.system);
    println!();
    print_process(&stats.process);
    println!();
    if stats.models.is_empty() {
        println!("models          (none registered)");
        return;
    }
    println!("models");
    for row in &stats.models {
        println!("  {}", fmt_model_row(row));
    }
}

fn fmt_model_row(r: &ModelMemoryRow) -> String {
    let est = if r.estimated_rss_bytes == 0 {
        "—".to_string()
    } else {
        let mb = r.estimated_rss_bytes as f64 / (1024.0 * 1024.0);
        format!("{:>6.1} MB", mb)
    };
    format!("{:<14} {:<10} est {}", r.label, format!("{:?}", r.state), est)
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
