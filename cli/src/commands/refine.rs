//! `openwhisper refine <subcommand>`: the headless surface of the
//! Refine step (TASK-93). Same core functions the desktop app calls,
//! same `settings.json`, same keychain entry.
//!
//! Subcommands:
//! - `run [TEXT]`: refine an argument or stdin, print the result.
//!   `--mode <id>` runs it in one of the modes (TASK-96), `--app
//!   <app-id>` in the mode a dictation into that app would get, and
//!   `--current-mode` in the mode a dictation would run in right now
//!   (the selected mode). Without any of them: the built-in mode.
//! - `status`: settings + whether the selected backend can run.
//! - `test`: the connection test from the settings pane.
//! - `config`: print the settings block, or change it with flags.
//! - `set-key` / `clear-key`: the API key in the OS keychain.
//! - `eval`: run the built-in eval set against the real backend.
//! - `prompt`: print the exact prompt that is sent, for the built-in
//!   mode or with `--mode <id>` for another.
//! - `history`: what Refine did on recent dictations in the desktop
//!   app (status, response time, warm or cold, reason for a fallback).
//!   The same data as the app's Diagnostics pane; no dictated text.
//!   Asks (`openwhisper ask`, TASK-97) are rows of the same table,
//!   listed with the source `ask` and counted on a line of their own.
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
use openwhisper_core::modes::{self, ModeChoice, ModeReason, ModeView};
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
    Prompt(PromptArgs),
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
    /// Run the text in this mode (an id from `openwhisper modes
    /// list`). Without --mode, --app and --current-mode: the built-in
    /// Dictation mode.
    #[arg(long, value_name = "ID", conflicts_with_all = ["app", "current_mode"])]
    mode: Option<String>,
    /// Run the text the way a dictation would run right now: in the
    /// selected mode, or with --app in that app's default mode when
    /// it has one.
    #[arg(long)]
    current_mode: bool,
    /// Run the text in the mode a dictation into this app would get:
    /// the app's default mode, else the selected mode. Takes an app
    /// id as `openwhisper focus` prints it.
    #[arg(long, value_name = "APP_ID")]
    app: Option<String>,
    #[command(flatten)]
    overrides: Overrides,
}

#[derive(Args, Debug)]
pub struct PromptArgs {
    /// Print the prompt of this mode (an id from `openwhisper modes
    /// list`) instead of the built-in Dictation mode.
    #[arg(long, value_name = "ID")]
    mode: Option<String>,
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
    /// Never write an em dash or an en dash: Refine and Ask use a
    /// hyphen, a comma or a new sentence instead (on by default).
    #[arg(long, value_parser = parse_bool)]
    no_long_dashes: Option<bool>,
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
        RefineCommand::Prompt(a) => show_prompt(&a, json),
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

/// The mode a run is made in. `--mode` names one; `--app` asks for
/// what a dictation into that app would get (its default mode, else
/// the selected one); `--current-mode` (`current`) for what a
/// dictation would get right now, which without an app is the
/// selected mode. None of them means the built-in Dictation mode, so
/// plain `refine run` does what it always did.
fn pick_mode(
    settings: &RefineSettings,
    mode: Option<&str>,
    app: Option<&str>,
    current: bool,
) -> Result<ModeChoice> {
    let saved = settings::load_modes_settings(&settings_path()?);
    choose_mode(&saved, settings, mode, app, current)
}

fn choose_mode(
    saved: &settings::ModesSettings,
    settings: &RefineSettings,
    mode: Option<&str>,
    app: Option<&str>,
    current: bool,
) -> Result<ModeChoice> {
    let selected = |mode: ModeView| ModeChoice {
        mode,
        reason: ModeReason::Selected,
        app_id: String::new(),
        app_name: String::new(),
    };
    match (mode, app) {
        (Some(id), _) => modes::get(saved, settings, id).map(selected).ok_or_else(|| {
            anyhow!("There is no mode with the id \"{id}\"; see `openwhisper modes list`.")
        }),
        (None, Some(app)) => Ok(modes::resolve(saved, settings, Some((app, "")))),
        // The same call the dictation pipeline makes, with no app known.
        (None, None) if current => Ok(modes::resolve(saved, settings, None)),
        (None, None) => Ok(selected(ModeView::builtin(settings))),
    }
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

    let choice = pick_mode(&settings, args.mode.as_deref(), args.app.as_deref(), args.current_mode)?;
    // The same two doors the dictation pipeline uses: a backend
    // warmed ahead of the text, or a cold one.
    let outcome = match args.warm_ms {
        Some(ms) => {
            let run = refine::apply_mode(&settings, &choice);
            let mut backend = refine::backend_for(&run.settings);
            backend.warm(&run.prompt);
            thread::sleep(Duration::from_millis(ms));
            run.refine(backend.as_mut(), &input)
        }
        None => refine::refine_text_in_mode(&settings, &choice, &input),
    };

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
        if !outcome.mode.is_builtin() || outcome.mode.reason != ModeReason::Selected.as_str() {
            eprintln!("mode: {}", mode_label(&outcome.mode));
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
    println!("no_long_dashes {}", settings.no_long_dashes);
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
        || args.no_long_dashes.is_some()
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
    if let Some(v) = args.no_long_dashes {
        next.no_long_dashes = v;
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
        println!("no_long_dashes {}", current.no_long_dashes);
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

/// "Formal (selected)" or "Formal (default for Mail)".
fn mode_label(mode: &refine::ModeUse) -> String {
    if mode.reason == ModeReason::AppDefault.as_str() {
        format!("{} (default for {})", mode.name, mode.app_name)
    } else {
        format!("{} (selected)", mode.name)
    }
}

fn show_prompt(args: &PromptArgs, json: bool) -> Result<()> {
    let settings = load(&args.overrides)?;
    use_dictionary();
    let choice = pick_mode(&settings, args.mode.as_deref(), None, false)?;
    let run = refine::apply_mode(&settings, &choice);
    let prompt = &run.prompt;
    let example = prompt.user_message("what was said goes here");
    if json {
        let value = serde_json::json!({
            "system": prompt.system(),
            "user_example": example,
            "mode": run.mode,
            "rewrite": prompt.is_rewrite(),
            "model": run.settings.active_model(),
            "effort": run.settings.active_effort(),
        });
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
    if summary.asks > 0 {
        // Asks share the table (and the plan); the numbers above are
        // about Refine only.
        println!(
            "asks           {} ({} answered; median first text {}, whole answer {})",
            summary.asks,
            summary.asks_answered,
            ms(summary.ask_median_first_text_ms),
            ms(summary.ask_median_ms)
        );
        // Every ask a screenshot was taken for is counted, also one
        // that failed or was cancelled: the image is kept nowhere,
        // this is the record that there was one.
        println!("with a screen  {} of them had a screenshot taken (the image is kept nowhere)", summary.asks_with_screen);
        for run in runs.iter().filter(|r| r.is_ask() && r.screen == Some(true)) {
            println!("               {}  {}", super::history::local_time(run.at_ms), run.status);
        }
    }
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
    let mut results = Vec::new();
    for case in eval::cases() {
        if args.only.as_deref().is_some_and(|f| !case.name.contains(f)) {
            continue;
        }
        // Each case in its own mode, put together like a dictation in
        // that mode, and a fresh backend per case, like a fresh
        // dictation.
        let run = eval::mode_run(&case, &settings);
        let mut backend = refine::backend_for(&run.settings);
        if let Some(ms) = args.warm_ms {
            let sent = run.prompt.is_rewrite()
                || openwhisper_core::refine::guard::word_count(case.input) >= settings.min_words as usize;
            if sent {
                backend.warm(&run.prompt);
                thread::sleep(Duration::from_millis(ms));
            }
        }
        let outcome = run.refine(backend.as_mut(), case.input);
        let result = eval::judge(&case, outcome);
        if !json {
            println!(
                "{}  {:<26} {:>5} ms  {}",
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
                if !result.mode.is_empty() {
                    println!("      mode:    {}", result.mode);
                }
                println!("      problem: {}", result.problem);
            }
        }
        results.push(result);
    }

    let passed = results.iter().filter(|r| r.passed).count();
    let safe = results.iter().filter(|r| r.safe).count();
    let all = latency(results.iter());
    // A mode prompt is another prompt and another kind of answer, so
    // its response time is reported next to the built-in mode's.
    let builtin = latency(results.iter().filter(|r| r.mode.is_empty()));
    let custom = latency(results.iter().filter(|r| !r.mode.is_empty()));
    let custom_passed = results.iter().filter(|r| !r.mode.is_empty() && r.passed).count();
    let custom_total = results.iter().filter(|r| !r.mode.is_empty()).count();

    if json {
        let value = serde_json::json!({
            "backend": settings.backend.as_str(),
            "model": settings.active_model(),
            "effort": settings.active_effort(),
            "warm_ms": args.warm_ms,
            "passed": passed,
            "safe": safe,
            "total": results.len(),
            "custom_modes": { "passed": custom_passed, "total": custom_total },
            "latency_ms": {
                "min": all.min,
                "median": all.median,
                "max": all.max,
                "builtin": builtin.to_json(),
                "custom_modes": custom.to_json(),
            },
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
        if custom_total > 0 {
            println!("custom modes: {custom_passed}/{custom_total} passed");
        }
        println!(
            "latency ({}, {}, {}): min {} ms, median {} ms, max {} ms",
            settings.backend.as_str(),
            settings.active_model(),
            match args.warm_ms {
                Some(ms) => format!("warmed {ms} ms before sending"),
                None => "cold".to_string(),
            },
            all.min,
            all.median,
            all.max
        );
        for (label, l) in [("built-in mode", &builtin), ("custom modes", &custom)] {
            if l.calls > 0 {
                println!(
                    "  {label:<14} {} calls: min {} ms, median {} ms, max {} ms",
                    l.calls, l.min, l.median, l.max
                );
            }
        }
    }
    if safe == results.len() {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

/// Response times over the cases that made a model call.
#[derive(Debug, PartialEq, Eq)]
struct Latency {
    calls: usize,
    min: u64,
    median: u64,
    max: u64,
}

impl Latency {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({ "calls": self.calls, "min": self.min, "median": self.median, "max": self.max })
    }
}

fn latency<'a>(results: impl Iterator<Item = &'a eval::EvalResult>) -> Latency {
    let mut times: Vec<u64> = results.filter(|r| r.elapsed_ms > 0).map(|r| r.elapsed_ms).collect();
    times.sort_unstable();
    Latency {
        calls: times.len(),
        min: times.first().copied().unwrap_or(0),
        median: times.get(times.len() / 2).copied().unwrap_or(0),
        max: times.last().copied().unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openwhisper_core::settings::ModesSettings;

    fn saved() -> ModesSettings {
        let m = modes::add(&ModesSettings::default(), "Formal", "Rewrite it in a formal tone.", None, None, None)
            .unwrap();
        let m = modes::add(&m, "Write in English", "Write it in English.", None, None, None).unwrap();
        let m = modes::select(&m, "formal").unwrap();
        modes::set_app_default(&m, "com.example.mail", "Mail", "write-in-english").unwrap()
    }

    #[test]
    fn plain_run_stays_in_the_built_in_mode_whatever_is_selected() {
        let settings = RefineSettings::default();
        let choice = choose_mode(&saved(), &settings, None, None, false).unwrap();
        assert!(choice.mode.builtin);
        assert_eq!(choice.reason, ModeReason::Selected);
    }

    #[test]
    fn current_mode_runs_what_a_dictation_would_run_right_now() {
        let settings = RefineSettings::default();
        // No app known: the selected mode.
        let choice = choose_mode(&saved(), &settings, None, None, true).unwrap();
        assert_eq!((choice.mode.id.as_str(), choice.reason), ("formal", ModeReason::Selected));
        // With an app: its default when it has one, else the selected mode.
        let choice = choose_mode(&saved(), &settings, None, Some("com.example.Mail"), true).unwrap();
        assert_eq!((choice.mode.id.as_str(), choice.reason), ("write-in-english", ModeReason::AppDefault));
        let choice = choose_mode(&saved(), &settings, None, Some("com.example.notes"), true).unwrap();
        assert_eq!((choice.mode.id.as_str(), choice.reason), ("formal", ModeReason::Selected));
        // Nothing selected yet: Dictation, like a fresh install.
        let choice = choose_mode(&ModesSettings::default(), &settings, None, None, true).unwrap();
        assert!(choice.mode.builtin);
    }

    #[test]
    fn mode_flag_names_a_mode_and_an_unknown_id_is_an_error() {
        let settings = RefineSettings::default();
        let choice = choose_mode(&saved(), &settings, Some("write-in-english"), None, false).unwrap();
        assert_eq!(choice.mode.name, "Write in English");
        assert_eq!(choice.reason, ModeReason::Selected);
        let choice = choose_mode(&saved(), &settings, Some("dictation"), None, false).unwrap();
        assert!(choice.mode.builtin);
        let err = choose_mode(&saved(), &settings, Some("nope"), None, false).unwrap_err().to_string();
        assert!(err.contains("nope") && err.contains("modes list"), "{err}");
    }

    #[test]
    fn app_flag_resolves_like_a_dictation_into_that_app() {
        let settings = RefineSettings::default();
        let choice = choose_mode(&saved(), &settings, None, Some("com.example.mail"), false).unwrap();
        assert_eq!(choice.mode.id, "write-in-english");
        assert_eq!(choice.reason, ModeReason::AppDefault);
        assert_eq!(choice.app_name, "Mail");
        // An app without a default gets the selected mode.
        let choice = choose_mode(&saved(), &settings, None, Some("com.example.notes"), false).unwrap();
        assert_eq!((choice.mode.id.as_str(), choice.reason), ("formal", ModeReason::Selected));
    }

    #[test]
    fn mode_labels_say_why_the_mode_ran() {
        let mut mode = refine::ModeUse::default();
        assert_eq!(mode_label(&mode), "Dictation (selected)");
        mode.name = "Formal".into();
        mode.reason = "app-default".into();
        mode.app_name = "Mail".into();
        assert_eq!(mode_label(&mode), "Formal (default for Mail)");
    }

    #[test]
    fn overrides_change_the_selected_backend_only() {
        let o = Overrides { model: Some("haiku".into()), effort: Some("medium".into()), ..Overrides::default() };
        let s = apply(RefineSettings::default(), &o).unwrap();
        assert_eq!(s.claude_cli.model, "haiku");
        assert_eq!(s.claude_cli.effort, RefineEffort::Medium);
        assert_eq!(s.openai, RefineSettings::default().openai);
        let bad = Overrides { effort: Some("turbo".into()), ..Overrides::default() };
        assert!(apply(RefineSettings::default(), &bad).is_err());
    }

    #[test]
    fn latency_counts_only_cases_that_made_a_call() {
        let results: Vec<eval::EvalResult> = [0u64, 900, 1500, 1200]
            .iter()
            .map(|ms| {
                let case = &eval::cases()[0];
                let mut outcome = refine::RefineOutcome::disabled(case.input);
                outcome.elapsed_ms = *ms;
                eval::judge(case, outcome)
            })
            .collect();
        assert_eq!(latency(results.iter()), Latency { calls: 3, min: 900, median: 1200, max: 1500 });
        assert_eq!(latency([].iter()), Latency { calls: 0, min: 0, median: 0, max: 0 });
    }
}
