//! `openwhisper refine <subcommand>`: the headless surface of the
//! Refine step (TASK-93). Same core functions the desktop app calls,
//! same `settings.json`, same keychain entry.
//!
//! Subcommands:
//! - `run [TEXT]`: refine an argument or stdin, print the result.
//! - `status`: settings + whether the selected backend can run.
//! - `test`: the connection test from the settings pane.
//! - `config`: print the settings block, or change it with flags.
//! - `set-key` / `clear-key`: the API key in the OS keychain.
//! - `eval`: run the built-in eval set against the real backend.
//! - `prompt`: print the exact prompt that is sent.
//! - `history`: what Refine did on recent dictations in the desktop
//!   app (status, response time, warm or cold, reason for a fallback).
//!   The same data as the app's Diagnostics pane; no dictated text.
//!
//! `run`, `test` and `eval` work whether or not Refine is enabled:
//! invoking them is the opt-in. They send the text to the configured
//! backend, exactly like a dictation would.

use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Subcommand};
use openwhisper_core::refine::{self, RefineStatus, eval, history, secrets};
use openwhisper_core::settings::{self, RefineBackendKind, RefineEffort, RefineSettings};
use openwhisper_core::store::Store;
use openwhisper_core::{dictionary, transcript};

#[derive(Args, Debug)]
pub struct RefineArgs {
    #[command(subcommand)]
    pub command: RefineCommand,
}

#[derive(Subcommand, Debug)]
pub enum RefineCommand {
    /// Refine text (argument or stdin) and print it.
    Run(RunArgs),
    /// Show the Refine settings and whether the backend is ready.
    Status(Overrides),
    /// Send a test sentence through the backend.
    Test(Overrides),
    /// Print the Refine settings, or change them with flags.
    Config(ConfigArgs),
    /// Store the API key for the OpenAI-compatible backend in the OS
    /// keychain. Reads the key from stdin.
    SetKey,
    /// Remove the stored API key.
    ClearKey,
    /// Run the built-in eval set against the real backend.
    Eval(EvalArgs),
    /// Show what Refine did on recent dictations in the desktop app.
    History(HistoryArgs),
    /// Print the exact prompt Refine sends (rules, your instructions,
    /// your dictionary words). No model call.
    Prompt(Overrides),
}

/// Per-invocation overrides of the saved settings. Nothing here is
/// written back.
#[derive(Args, Debug, Default)]
pub struct Overrides {
    /// Backend to use: claude-cli or openai-compatible.
    #[arg(long)]
    backend: Option<String>,
    /// Model for the selected backend.
    #[arg(long)]
    model: Option<String>,
    /// Effort for the selected backend: low, medium, high, xhigh or
    /// max (OpenAI-compatible also: minimal, and "none" to send no
    /// effort at all).
    #[arg(long)]
    effort: Option<String>,
    /// Base URL of the OpenAI-compatible endpoint.
    #[arg(long)]
    base_url: Option<String>,
    /// Path to the claude binary.
    #[arg(long)]
    claude_path: Option<String>,
    /// Cleanup instructions (replaces the saved ones for this run).
    #[arg(long)]
    instructions: Option<String>,
    /// Timeout in milliseconds.
    #[arg(long)]
    timeout_ms: Option<u64>,
}

#[derive(Args, Debug)]
pub struct RunArgs {
    /// The text to refine. Read from stdin when omitted.
    text: Option<String>,
    /// Send the text as given, without the rule pass (fillers,
    /// dictionary, duplicates) that a dictation gets first.
    #[arg(long)]
    skip_rules: bool,
    /// Start the backend, wait this long, then send the text. Mimics
    /// a recording of that length, to measure the warmed latency.
    #[arg(long, value_name = "MS")]
    warm_ms: Option<u64>,
    #[command(flatten)]
    overrides: Overrides,
}

#[derive(Args, Debug)]
pub struct ConfigArgs {
    /// Switch Refine on or off.
    #[arg(long, value_parser = parse_bool)]
    enabled: Option<bool>,
    /// Reject output that drifts from what was said (on by default).
    #[arg(long, value_parser = parse_bool)]
    drift_guard: Option<bool>,
    /// Utterances with fewer words are not sent.
    #[arg(long)]
    min_words: Option<u32>,
    #[command(flatten)]
    overrides: Overrides,
}

#[derive(Args, Debug)]
pub struct HistoryArgs {
    /// How many runs to show and summarize.
    #[arg(long, default_value_t = 50)]
    limit: u32,
    /// Delete the stored history instead of showing it.
    #[arg(long)]
    clear: bool,
}

#[derive(Args, Debug)]
pub struct EvalArgs {
    /// Start the backend, wait this long, then send each case. Mimics
    /// a recording of that length, to measure the warmed latency.
    #[arg(long, value_name = "MS")]
    warm_ms: Option<u64>,
    /// Run only cases whose name contains this text.
    #[arg(long)]
    only: Option<String>,
    #[command(flatten)]
    overrides: Overrides,
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

pub fn run(args: RefineArgs, json: bool) -> Result<()> {
    match args.command {
        RefineCommand::Run(a) => run_text(a, json),
        RefineCommand::Status(o) => status(&o, json),
        RefineCommand::Test(o) => test(&o, json),
        RefineCommand::Config(a) => config(a, json),
        RefineCommand::SetKey => set_key(json),
        RefineCommand::ClearKey => clear_key(json),
        RefineCommand::Eval(a) => run_eval(a, json),
        RefineCommand::History(a) => show_history(a, json),
        RefineCommand::Prompt(o) => show_prompt(&o, json),
    }
}

fn settings_path() -> Result<PathBuf> {
    Ok(super::config_dir()?.join("settings.json"))
}

/// Saved settings with `overrides` applied (not persisted).
fn load(overrides: &Overrides) -> Result<RefineSettings> {
    let saved = settings::load_refine_settings(&settings_path()?);
    apply(saved, overrides)
}

fn apply(mut s: RefineSettings, o: &Overrides) -> Result<RefineSettings> {
    if let Some(backend) = &o.backend {
        s.backend = RefineBackendKind::parse(backend).ok_or_else(|| {
            anyhow!("unknown backend \"{backend}\"; use claude-cli or openai-compatible")
        })?;
    }
    if let Some(model) = &o.model {
        match s.backend {
            RefineBackendKind::OpenaiCompatible => s.openai.model = model.clone(),
            _ => s.claude_cli.model = model.clone(),
        }
    }
    if let Some(effort) = &o.effort {
        let parsed = RefineEffort::parse(effort);
        match s.backend {
            RefineBackendKind::OpenaiCompatible => {
                if parsed.is_none() && !effort.eq_ignore_ascii_case("none") {
                    bail!(
                        "unknown effort \"{effort}\"; use minimal, low, medium, high, xhigh, max or none"
                    );
                }
                s.openai.effort = parsed;
            }
            _ => {
                s.claude_cli.effort = parsed.ok_or_else(|| {
                    anyhow!("unknown effort \"{effort}\"; use low, medium, high, xhigh or max")
                })?;
            }
        }
    }
    if let Some(url) = &o.base_url {
        s.openai.base_url = url.clone();
    }
    if let Some(path) = &o.claude_path {
        s.claude_cli.path = Some(path.clone());
    }
    if let Some(instructions) = &o.instructions {
        s.instructions = instructions.clone();
    }
    if let Some(ms) = o.timeout_ms {
        s.timeout_ms = ms;
    }
    Ok(s.cleaned())
}

/// Point core at the user's dictionary so its terms reach the prompt,
/// like in the app.
fn use_dictionary() {
    if let Ok(dir) = super::config_dir() {
        let path = dir.join(dictionary::FILE_NAME);
        if path.is_file() {
            dictionary::set_path(&path);
        }
    }
}

fn run_text(args: RunArgs, json: bool) -> Result<()> {
    let settings = load(&args.overrides)?;
    let text = match args.text {
        Some(t) => t,
        None => {
            if std::io::stdin().is_terminal() {
                bail!("no text given: pass it as an argument or pipe it on stdin");
            }
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf).context("read stdin")?;
            buf
        }
    };
    use_dictionary();
    let input = if args.skip_rules {
        text
    } else {
        transcript::process(&text)
    };

    let prompt = refine::compose_prompt(&settings);
    let mut backend = refine::backend_for(&settings);
    if let Some(ms) = args.warm_ms {
        backend.warm(&prompt);
        thread::sleep(Duration::from_millis(ms));
    }
    let outcome = refine::refine_with(backend.as_mut(), &prompt, &settings, &input);

    if json {
        println!("{}", serde_json::to_string_pretty(&outcome)?);
    } else {
        println!("{}", outcome.text.trim_end());
        match outcome.status {
            RefineStatus::Refined => {
                eprintln!(
                    "refined in {} ms ({}, {}{})",
                    outcome.elapsed_ms,
                    outcome.backend,
                    outcome.model,
                    if outcome.effort.is_empty() {
                        String::new()
                    } else {
                        format!(", {} effort", outcome.effort)
                    }
                );
                if !outcome.instruction.is_empty() {
                    eprintln!("followed spoken instruction: {}", outcome.instruction);
                }
            }
            status => eprintln!(
                "not refined ({}): {}",
                status.as_str(),
                if outcome.detail.is_empty() { "-" } else { &outcome.detail }
            ),
        }
    }
    Ok(())
}

fn status(overrides: &Overrides, json: bool) -> Result<()> {
    let settings = load(overrides)?;
    let backend = refine::backend_status(&settings);
    if json {
        let value = serde_json::json!({
            "settings": settings,
            "backend_status": backend,
            "settings_path": settings_path()?.display().to_string(),
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    println!("enabled        {}", settings.enabled);
    println!("backend        {}", settings.backend.as_str());
    println!("state          {}", if backend.ready { "ready" } else { "not ready" });
    println!("message        {}", backend.message);
    println!("text goes to   {}", backend.destination);
    match settings.backend {
        RefineBackendKind::OpenaiCompatible => {
            println!("endpoint       {}", settings.openai.base_url);
            println!("model          {}", or_dash(&settings.openai.model));
            println!("effort         {}", or_dash(settings.active_effort()));
            println!("api key        {}", if backend.has_api_key { "stored" } else { "none" });
        }
        _ => {
            println!("model          {}", settings.claude_cli.model);
            println!("effort         {}", settings.claude_cli.effort.as_str());
            println!("binary         {}", backend.path.as_deref().unwrap_or("-"));
            println!("version        {}", backend.version.as_deref().unwrap_or("-"));
            println!("signed in via  {}", backend.auth_method.as_deref().unwrap_or("-"));
            println!("plan           {}", backend.plan.as_deref().unwrap_or("-"));
        }
    }
    println!("timeout_ms     {}", settings.timeout_ms);
    println!("min_words      {}", settings.min_words);
    println!("drift_guard    {}", settings.drift_guard);
    println!("settings_path  {}", settings_path()?.display());
    Ok(())
}

fn or_dash(s: &str) -> &str {
    if s.is_empty() { "-" } else { s }
}

fn test(overrides: &Overrides, json: bool) -> Result<()> {
    let settings = load(overrides)?;
    let result = refine::test_connection(&settings);
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else if result.ok {
        println!("ok in {} ms ({})", result.elapsed_ms, result.backend);
        println!("sent      {}", refine::TEST_SENTENCE);
        println!("received  {}", result.output);
    } else {
        println!("failed ({}): {}", result.backend, result.message);
    }
    if result.ok {
        Ok(())
    } else {
        // Non-zero exit so scripts can branch on it; the reason is
        // already on stdout.
        std::process::exit(1);
    }
}

fn config(args: ConfigArgs, json: bool) -> Result<()> {
    let path = settings_path()?;
    let saved = settings::load_refine_settings(&path);
    let o = &args.overrides;
    let changes = args.enabled.is_some()
        || args.drift_guard.is_some()
        || args.min_words.is_some()
        || o.backend.is_some()
        || o.model.is_some()
        || o.effort.is_some()
        || o.base_url.is_some()
        || o.claude_path.is_some()
        || o.instructions.is_some()
        || o.timeout_ms.is_some();

    let mut next = apply(saved, o)?;
    if let Some(v) = args.enabled {
        next.enabled = v;
    }
    if let Some(v) = args.drift_guard {
        next.drift_guard = v;
    }
    if let Some(v) = args.min_words {
        next.min_words = v;
    }
    let current = if changes {
        settings::save_refine_settings(&path, next)
            .map_err(|e| anyhow!(e))
            .context("save refine settings")?
    } else {
        next
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&current)?);
    } else {
        println!("enabled        {}", current.enabled);
        println!("backend        {}", current.backend.as_str());
        println!(
            "instructions   {}",
            if current.instructions.is_empty() {
                "(default)"
            } else {
                &current.instructions
            }
        );
        println!("timeout_ms     {}", current.timeout_ms);
        println!("min_words      {}", current.min_words);
        println!("drift_guard    {}", current.drift_guard);
        println!("claude model   {}", current.claude_cli.model);
        println!("claude effort  {}", current.claude_cli.effort.as_str());
        println!("claude path    {}", current.claude_cli.path.as_deref().unwrap_or("(auto)"));
        println!("openai url     {}", current.openai.base_url);
        println!("openai model   {}", or_dash(&current.openai.model));
        println!(
            "openai effort  {}",
            current.openai.effort.map_or("(not sent)", RefineEffort::as_str)
        );
        println!(
            "{}  {}",
            if changes { "written      " } else { "settings_path" },
            path.display()
        );
        if changes {
            eprintln!("A running OpenWhisper picks this up when its Refine settings are opened, or on restart.");
        }
    }
    Ok(())
}

fn set_key(json: bool) -> Result<()> {
    if std::io::stdin().is_terminal() {
        eprintln!("Paste the API key and press Enter (it is not echoed back):");
    }
    let mut key = String::new();
    std::io::stdin().read_line(&mut key).context("read key from stdin")?;
    let key = key.trim();
    if key.is_empty() {
        bail!("no key given");
    }
    secrets::set_api_key(key).map_err(|e| anyhow!(e))?;
    if json {
        println!("{}", serde_json::json!({ "has_api_key": true }));
    } else {
        println!("API key stored in the OS keychain.");
    }
    Ok(())
}

fn clear_key(json: bool) -> Result<()> {
    secrets::clear_api_key().map_err(|e| anyhow!(e))?;
    if json {
        println!("{}", serde_json::json!({ "has_api_key": secrets::has_api_key() }));
    } else {
        println!("API key removed from the OS keychain.");
    }
    Ok(())
}

fn show_prompt(overrides: &Overrides, json: bool) -> Result<()> {
    let settings = load(overrides)?;
    use_dictionary();
    let prompt = refine::compose_prompt(&settings);
    let example = prompt.user_message("what was said goes here");
    if json {
        let value = serde_json::json!({ "system": prompt.system(), "user_example": example });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("{}", prompt.system());
        eprintln!("\n--- the transcript is sent separately, as:\n{example}");
    }
    Ok(())
}

fn show_history(args: HistoryArgs, json: bool) -> Result<()> {
    let path = super::database_path()?;
    if !path.is_file() {
        bail!(
            "no database at {}: the desktop app has not run yet",
            path.display()
        );
    }
    let store = Store::open_or_init(&path).map_err(|e| anyhow!("open {}: {e}", path.display()))?;
    if args.clear {
        history::clear(&store).map_err(|e| anyhow!("clear history: {e}"))?;
        if json {
            println!("{}", serde_json::json!({ "cleared": true }));
        } else {
            println!("Refine history cleared.");
        }
        return Ok(());
    }
    let runs = history::recent(&store, args.limit).map_err(|e| anyhow!("read history: {e}"))?;
    let summary = history::summarize(&runs);
    if json {
        let value = serde_json::json!({ "summary": summary, "runs": runs });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    if runs.is_empty() {
        println!("No Refine runs recorded yet.");
        return Ok(());
    }
    let ms = |v: Option<i64>| v.map_or("-".to_string(), |v| format!("{v} ms"));
    println!("last {} runs", summary.total);
    println!("refined        {}", summary.refined);
    println!(
        "fell back      {} (timeout {}, unavailable {}, failed {}, rejected {})",
        summary.fallbacks, summary.timeouts, summary.unavailable, summary.failed, summary.rejected
    );
    println!("too short      {}", summary.skipped);
    println!("instructions   {}", summary.instructions);
    println!(
        "response time  median {}, p90 {}, slowest {}",
        ms(summary.median_ms),
        ms(summary.p90_ms),
        ms(summary.max_ms)
    );
    println!(
        "warm / cold    {} runs median {} / {} runs median {}",
        summary.warm_runs,
        ms(summary.median_warm_ms),
        summary.cold_runs,
        ms(summary.median_cold_ms)
    );
    if summary.usage_runs > 0 {
        println!(
            "per run        {} tokens in, {} out{}",
            summary.avg_tokens_in.unwrap_or(0),
            summary.avg_tokens_out.unwrap_or(0),
            match (summary.avg_cost_usd, summary.total_cost_usd) {
                (Some(avg), Some(total)) => format!(
                    "; API list price ${avg:.4} each, ${total:.4} for these {} (a yardstick: a subscription is not billed per call)",
                    summary.usage_runs
                ),
                _ => String::new(),
            }
        );
    }
    if let Some(plan) = &summary.plan {
        println!(
            "5-hour window  {:.0}% used{}",
            plan.five_hour_used * 100.0,
            plan.five_hour_resets_at
                .map_or(String::new(), |t| format!(", resets at epoch {t}"))
        );
        if let Some(week) = plan.seven_day_used {
            println!("7-day window   {:.0}% used", week * 100.0);
        }
        match plan.max_share_per_run {
            Some(share) => println!(
                "per dictation  at most {:.2}% of the 5-hour window ({} runs while it moved {:.0} points; includes all other use of the plan)",
                share * 100.0,
                plan.window_runs,
                plan.window_moved.unwrap_or(0.0) * 100.0
            ),
            None => println!(
                "per dictation  too small to see: {} runs in this window without it moving 1 point",
                plan.window_runs
            ),
        }
        if plan.pace_known {
            match plan.full_at_ms {
                Some(at) => println!("at this pace   the 5-hour window is full at epoch {}", at / 1000),
                None => println!("at this pace   the 5-hour window resets before it fills"),
            }
        }
    }
    println!();
    println!(
        "{:<14} {:<10} {:<14} {:>8} {:>7}  {:<16} {:>13} {:>9}  {}",
        "when (epoch s)", "source", "status", "ms", "start", "model/effort", "tokens in>out", "list $", "reason"
    );
    for r in &runs {
        println!(
            "{:<14} {:<10} {:<14} {:>8} {:>7}  {:<16} {:>13} {:>9}  {}",
            r.at_ms / 1000,
            r.source,
            r.status,
            r.elapsed_ms,
            if r.warm_lead_ms.is_some() { "warm" } else { "cold" },
            format!("{}/{}", r.model, if r.effort.is_empty() { "-" } else { &r.effort }),
            match (r.tokens_in, r.tokens_out) {
                (Some(i), Some(o)) => format!("{i}>{o}"),
                _ => "-".to_string(),
            },
            r.cost_usd.map_or("-".to_string(), |c| format!("{c:.4}")),
            r.detail
        );
    }
    Ok(())
}

fn run_eval(args: EvalArgs, json: bool) -> Result<()> {
    let settings = load(&args.overrides)?;
    let status = refine::backend_status(&settings);
    if !status.ready {
        bail!("backend {} is not ready: {}", status.backend, status.message);
    }
    let prompt = eval::prompt(&settings);
    let mut results = Vec::new();
    for case in eval::cases() {
        if args.only.as_deref().is_some_and(|f| !case.name.contains(f)) {
            continue;
        }
        // A fresh backend per case, like a fresh dictation.
        let mut backend = refine::backend_for(&settings);
        if let Some(ms) = args.warm_ms {
            if openwhisper_core::refine::guard::word_count(case.input) >= settings.min_words as usize {
                backend.warm(&prompt);
                thread::sleep(Duration::from_millis(ms));
            }
        }
        let outcome = refine::refine_with(backend.as_mut(), &prompt, &settings, case.input);
        let result = eval::judge(&case, outcome);
        if !json {
            println!(
                "{}  {:<24} {:>5} ms  {}",
                if result.passed {
                    "PASS"
                } else if result.safe {
                    "SAFE"
                } else {
                    "FAIL"
                },
                result.name,
                result.elapsed_ms,
                result.output
            );
            if !result.passed {
                println!("      said:    {}", result.input);
                println!("      problem: {}", result.problem);
            }
        }
        results.push(result);
    }

    let passed = results.iter().filter(|r| r.passed).count();
    let safe = results.iter().filter(|r| r.safe).count();
    let mut times: Vec<u64> = results
        .iter()
        .filter(|r| r.elapsed_ms > 0)
        .map(|r| r.elapsed_ms)
        .collect();
    times.sort_unstable();
    let median = times.get(times.len() / 2).copied().unwrap_or(0);
    let (min, max) = (
        times.first().copied().unwrap_or(0),
        times.last().copied().unwrap_or(0),
    );

    if json {
        let value = serde_json::json!({
            "backend": settings.backend.as_str(),
            "warm_ms": args.warm_ms,
            "passed": passed,
            "safe": safe,
            "total": results.len(),
            "latency_ms": { "min": min, "median": median, "max": max },
            "results": results,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!();
        println!(
            "{passed}/{} passed, {safe}/{} safe (PASS = model behaved; SAFE = model misbehaved \
but the unrefined text was inserted; FAIL = wrong text would have been inserted)",
            results.len(),
            results.len()
        );
        println!(
            "latency ({}, {}): min {min} ms, median {median} ms, max {max} ms",
            settings.backend.as_str(),
            match args.warm_ms {
                Some(ms) => format!("warmed {ms} ms before sending"),
                None => "cold".to_string(),
            }
        );
    }
    if safe == results.len() {
        Ok(())
    } else {
        std::process::exit(1);
    }
}
