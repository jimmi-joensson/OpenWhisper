//! `openwhisper ask <subcommand>`: the headless surface of Ask
//! (TASK-97). Same core functions the desktop app calls, same
//! `settings.json`, same backend as Refine.
//!
//! Subcommands:
//! - `run [TEXT]`: ask a question (argument or stdin). The answer
//!   streams to stdout as it arrives; where the question was sent,
//!   the time to the first text, the total time and the tokens go to
//!   stderr. `--json` prints one record at the end instead. The run
//!   leaves the diagnostics row an ask in the app leaves (a row in
//!   `refine_runs`, when the app's database exists), and with
//!   `--history` the question and answer in the transcript history,
//!   when that is switched on.
//! - `status`: whether the backend can run, where questions are sent,
//!   and the settings an ask would use.
//! - `config`: print the `ask` settings block, or change it with
//!   flags.
//! - `eval`: run the built-in eval set against the real backend.
//!
//! `run` and `eval` work whether or not Ask is switched on in the
//! settings: invoking them is the opt-in. They send the question to
//! the configured backend, exactly like the ask key would, and say
//! so. An ask has no tools: it can answer and nothing else, plus a
//! web search with `--web-search`.

use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Subcommand};
use openwhisper_core::ask::{self, AskEvent, AskInput, AskPlan, CancelFlag, eval};
use openwhisper_core::refine;
use openwhisper_core::settings::{self, AskSettings, RefineBackendKind, RefineEffort, RefineSettings};
use openwhisper_core::store::Store;
use openwhisper_core::{dictionary, history, stats, transcript};

#[derive(Args, Debug)]
pub struct AskArgs {
    #[command(subcommand)]
    pub command: AskCommand,
}

#[derive(Subcommand, Debug)]
pub enum AskCommand {
    /// Ask a question (argument or stdin) and print the answer.
    Run(RunArgs),
    /// Show whether an ask can run, where it is sent and with what.
    Status(Overrides),
    /// Print the Ask settings, or change them with flags.
    Config(ConfigArgs),
    /// Run the built-in eval set against the real backend.
    Eval(EvalArgs),
}

/// Per-invocation overrides of the saved settings. Nothing here is
/// written back.
#[derive(Args, Debug, Default)]
pub struct Overrides {
    /// Backend to use: claude-cli or openai-compatible. Saved in the
    /// Refine settings; Ask uses the same one.
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
    /// Timeout in milliseconds (doubled with web search).
    #[arg(long)]
    timeout_ms: Option<u64>,
    /// Standing instructions for the answer (replaces the saved ones
    /// for this run).
    #[arg(long)]
    instructions: Option<String>,
    /// Path to the claude binary.
    #[arg(long)]
    claude_path: Option<String>,
    /// Base URL of the OpenAI-compatible endpoint.
    #[arg(long)]
    base_url: Option<String>,
}

#[derive(Args, Debug)]
pub struct RunArgs {
    /// The question. Read from stdin when omitted.
    text: Option<String>,
    /// Let the model search the web for this question (Claude backend
    /// only).
    #[arg(long)]
    web_search: bool,
    /// Print the answer once it is complete instead of as it arrives.
    #[arg(long)]
    no_stream: bool,
    /// Send the text as given, without the rule pass (fillers,
    /// dictionary, duplicates) that a spoken question gets first.
    #[arg(long)]
    skip_rules: bool,
    /// Start the backend, wait this long, then send the question.
    /// Mimics a recording of that length, to measure the warmed
    /// latency.
    #[arg(long, value_name = "MS")]
    warm_ms: Option<u64>,
    /// Also keep the question and the answer in the transcript
    /// history, like an ask in the app, when history is switched on.
    /// Off by default, so a script does not write into it.
    #[arg(long)]
    history: bool,
    #[command(flatten)]
    overrides: Overrides,
}

#[derive(Args, Debug)]
pub struct ConfigArgs {
    /// Switch Ask on or off.
    #[arg(long, value_parser = parse_bool)]
    enabled: Option<bool>,
    /// Let the model search the web (Claude backend only).
    #[arg(long, value_parser = parse_bool)]
    web_search: Option<bool>,
    /// Standing instructions for every answer. An empty string clears
    /// them.
    #[arg(long)]
    instructions: Option<String>,
    /// Claude model for asks.
    #[arg(long, conflicts_with = "clear_model")]
    model: Option<String>,
    /// Back to the default model (sonnet).
    #[arg(long)]
    clear_model: bool,
    /// Claude effort for asks: low, medium, high, xhigh or max.
    #[arg(long, conflicts_with = "clear_effort")]
    effort: Option<String>,
    /// Back to the default effort (low).
    #[arg(long)]
    clear_effort: bool,
    /// Timeout in milliseconds (doubled with web search).
    #[arg(long)]
    timeout_ms: Option<u64>,
}

#[derive(Args, Debug)]
pub struct EvalArgs {
    /// Run only cases whose name starts with this text.
    #[arg(long, value_name = "PREFIX")]
    only: Option<String>,
    #[command(flatten)]
    overrides: Overrides,
}

fn parse_bool(s: &str) -> Result<bool, String> {
    match s.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        other => Err(format!("expected true/false (or 1/0, yes/no, on/off); got {other}")),
    }
}

pub fn run(args: AskArgs, json: bool) -> Result<()> {
    match args.command {
        AskCommand::Run(a) => run_question(a, json),
        AskCommand::Status(o) => status(&o, json),
        AskCommand::Config(a) => config(a, json),
        AskCommand::Eval(a) => run_eval(a, json),
    }
}

fn settings_path() -> Result<PathBuf> {
    Ok(super::config_dir()?.join("settings.json"))
}

/// The saved Ask and Refine blocks with `overrides` applied (not
/// persisted).
fn load(overrides: &Overrides) -> Result<(AskSettings, RefineSettings)> {
    let path = settings_path()?;
    apply(settings::load_ask_settings(&path), settings::load_refine_settings(&path), overrides)
}

/// A model or effort override goes where the selected backend reads
/// it: the Ask block for Claude, the Refine block for an
/// OpenAI-compatible endpoint (see `AskSettings::model_for`).
fn apply(mut ask: AskSettings, mut refine: RefineSettings, o: &Overrides) -> Result<(AskSettings, RefineSettings)> {
    if let Some(backend) = &o.backend {
        refine.backend = RefineBackendKind::parse(backend)
            .ok_or_else(|| anyhow!("unknown backend \"{backend}\"; use claude-cli or openai-compatible"))?;
    }
    let openai = refine.backend == RefineBackendKind::OpenaiCompatible;
    if let Some(model) = &o.model {
        if openai {
            refine.openai.model = model.clone();
        } else {
            ask.model = Some(model.clone());
        }
    }
    if let Some(effort) = &o.effort {
        let parsed = RefineEffort::parse(effort);
        if openai {
            if parsed.is_none() && !effort.eq_ignore_ascii_case("none") {
                bail!("unknown effort \"{effort}\"; use minimal, low, medium, high, xhigh, max or none");
            }
            refine.openai.effort = parsed;
        } else {
            ask.effort = Some(claude_effort(effort)?);
        }
    }
    if let Some(ms) = o.timeout_ms {
        ask.timeout_ms = ms;
    }
    if let Some(instructions) = &o.instructions {
        ask.instructions = instructions.clone();
    }
    if let Some(path) = &o.claude_path {
        refine.claude_cli.path = Some(path.clone());
    }
    if let Some(url) = &o.base_url {
        refine.openai.base_url = url.clone();
    }
    Ok((ask.cleaned(), refine.cleaned()))
}

fn claude_effort(effort: &str) -> Result<RefineEffort> {
    RefineEffort::parse(effort).ok_or_else(|| anyhow!("unknown effort \"{effort}\"; use low, medium, high, xhigh or max"))
}

/// Point core at the user's dictionary so its terms reach the prompt
/// and the rule pass, like in the app.
fn use_dictionary() {
    if let Ok(dir) = super::config_dir() {
        let path = dir.join(dictionary::FILE_NAME);
        if path.is_file() {
            dictionary::set_path(&path);
        }
    }
}

/// Register the app's database with core, when the app has made
/// one, so the run leaves its diagnostics row there (the place
/// `openwhisper refine history` reads). Without one, nothing is
/// recorded: the CLI does not create the app's database.
fn use_store() -> bool {
    let Ok(path) = super::database_path() else {
        return false;
    };
    if !path.is_file() {
        return false;
    }
    match Store::open_or_init(&path) {
        Ok(store) => {
            stats::set_store(Arc::new(store));
            true
        }
        Err(e) => {
            eprintln!("note: the run is not recorded ({}: {e})", path.display());
            false
        }
    }
}

/// Point core at the transcript history of this config: its settings
/// and the folder the app writes to. Returns whether history is
/// switched on.
fn use_history() -> Result<bool> {
    let dir = super::config_dir()?;
    history::set_default_dir(dir.join(history::DEFAULT_FOLDER_NAME));
    Ok(settings::load_history_settings(&dir.join("settings.json")).enabled)
}

fn effort_label(effort: &str) -> String {
    if effort.is_empty() { String::new() } else { format!(", {effort} effort") }
}

fn run_question(args: RunArgs, json: bool) -> Result<()> {
    let (mut ask, refine) = load(&args.overrides)?;
    if args.web_search {
        ask.web_search = true;
    }
    let text = match args.text {
        Some(t) => t,
        None => {
            if std::io::stdin().is_terminal() {
                bail!("no question given: pass it as an argument or pipe it on stdin");
            }
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf).context("read stdin")?;
            buf
        }
    };
    use_dictionary();
    let question = if args.skip_rules { text } else { transcript::process(&text) };
    let question = question.trim().to_string();
    if question.is_empty() {
        bail!("no question given: pass it as an argument or pipe it on stdin");
    }

    let plan = AskPlan::new(&ask, &refine);
    let destination = plan.destination();
    let recorded = use_store();
    let kept = args.history && use_history()?;
    if !json {
        // Before anything leaves the machine.
        eprintln!("question goes to: {destination}");
        if !plan.notice.is_empty() {
            eprintln!("note: {}", plan.notice);
        }
        if args.history && !kept {
            eprintln!("note: transcript history is switched off, so this ask is not kept in it");
        }
    }

    let stream = !json && !args.no_stream;
    let mut out = std::io::stdout().lock();
    let mut last_char = '\n';
    let mut sink = |event: AskEvent| match event {
        AskEvent::Text(piece) if stream => {
            if let Some(c) = piece.chars().last() {
                last_char = c;
            }
            let _ = out.write_all(piece.as_bytes());
            let _ = out.flush();
        }
        AskEvent::Searching { query } if !json => eprintln!("searching the web: {query}"),
        _ => {}
    };
    let mut backend = plan.backend();
    if let Some(ms) = args.warm_ms {
        backend.warm(&plan.setup);
        thread::sleep(Duration::from_millis(ms));
    }
    // The same path as an ask in the app: the answer, then its
    // diagnostics row and, when asked for, its history entry.
    let result = plan.send_and_record(
        backend.as_mut(),
        &AskInput::text(question.clone()),
        &mut sink,
        &CancelFlag::new(),
        kept,
    );
    drop(backend);
    drop(out);
    if stream && last_char != '\n' {
        println!();
    }

    let effort = plan.effort_id();
    if json {
        let reply = result.as_ref().ok();
        let record = serde_json::json!({
            "question": question,
            "answer": reply.map(|r| r.text.as_str()),
            "error": result.as_ref().err().map(ToString::to_string),
            "first_text_ms": reply.and_then(|r| r.first_text_ms),
            "elapsed_ms": reply.map(|r| r.elapsed_ms),
            "warm_lead_ms": reply.and_then(|r| r.warm_lead_ms),
            "usage": reply.and_then(|r| r.usage.as_ref()),
            "notice": reply.map_or(plan.notice.as_str(), |r| r.notice.as_str()),
            "backend": refine.backend.as_str(),
            "model": plan.setup.model,
            "effort": effort,
            "web_search": plan.setup.web_search,
            "timeout_ms": plan.timeout.as_millis() as u64,
            "destination": destination,
            "recorded": recorded,
            "kept_in_history": kept && result.is_ok(),
        });
        println!("{}", serde_json::to_string_pretty(&record)?);
    }
    match result {
        Ok(reply) => {
            if !json {
                if !stream {
                    println!("{}", reply.text);
                }
                eprintln!(
                    "first text after {}, done in {} ms ({}, {}{}{})",
                    reply.first_text_ms.map_or("-".to_string(), |ms| format!("{ms} ms")),
                    reply.elapsed_ms,
                    refine.backend.as_str(),
                    plan.setup.model,
                    effort_label(effort),
                    reply.warm_lead_ms.map_or(String::new(), |ms| format!(", started {ms} ms ahead"))
                );
                if let Some(usage) = &reply.usage {
                    eprintln!(
                        "tokens: {} in ({} cached), {} out{}",
                        usage.tokens_in,
                        usage.tokens_cached,
                        usage.tokens_out,
                        usage.cost_usd.map_or(String::new(), |c| format!("; API list price ${c:.4}"))
                    );
                }
            }
            Ok(())
        }
        Err(e) => {
            if !json {
                eprintln!("no answer: {e}");
            }
            // Non-zero exit so scripts can branch on it; the reason
            // is already printed.
            std::process::exit(1);
        }
    }
}

fn or_dash(s: &str) -> &str {
    if s.is_empty() { "-" } else { s }
}

fn status(overrides: &Overrides, json: bool) -> Result<()> {
    let (ask, refine) = load(overrides)?;
    let status = ask::status(&ask, &refine);
    if json {
        let mut value = serde_json::to_value(&status)?;
        value["destination"] = serde_json::json!(status.backend.destination);
        value["settings_path"] = serde_json::json!(settings_path()?.display().to_string());
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    let b = &status.backend;
    println!("enabled           {}", status.settings.enabled);
    println!("backend           {} (the one Refine is set up with)", b.backend);
    println!("state             {}", if b.ready { "ready" } else { "not ready" });
    println!("message           {}", b.message);
    println!("question goes to  {}", b.destination);
    println!("model             {}", or_dash(&status.model));
    println!("effort            {}", or_dash(&status.effort));
    println!("web search        {}", if status.web_search { "on" } else { "off" });
    println!("tools             none{}", if status.web_search { " but the web search" } else { "" });
    println!("timeout_ms        {}", status.timeout_ms);
    println!(
        "instructions      {}",
        if status.settings.instructions.is_empty() { "(none)" } else { &status.settings.instructions }
    );
    if !status.notice.is_empty() {
        println!("note              {}", status.notice);
    }
    match refine.backend {
        RefineBackendKind::OpenaiCompatible => {
            println!("endpoint          {}", refine.openai.base_url);
            println!("api key           {}", if b.has_api_key { "stored" } else { "none" });
        }
        _ => {
            println!("binary            {}", b.path.as_deref().unwrap_or("-"));
            println!("version           {}", b.version.as_deref().unwrap_or("-"));
            println!("signed in via     {}", b.auth_method.as_deref().unwrap_or("-"));
            println!("plan              {}", b.plan.as_deref().unwrap_or("-"));
        }
    }
    println!("settings_path     {}", settings_path()?.display());
    Ok(())
}

fn config(args: ConfigArgs, json: bool) -> Result<()> {
    let path = settings_path()?;
    let mut next = settings::load_ask_settings(&path);
    let changes = args.enabled.is_some()
        || args.web_search.is_some()
        || args.instructions.is_some()
        || args.model.is_some()
        || args.clear_model
        || args.effort.is_some()
        || args.clear_effort
        || args.timeout_ms.is_some();

    if let Some(v) = args.enabled {
        next.enabled = v;
    }
    if let Some(v) = args.web_search {
        next.web_search = v;
    }
    if let Some(v) = args.instructions {
        next.instructions = v;
    }
    if let Some(v) = args.model {
        next.model = Some(v);
    }
    if args.clear_model {
        next.model = None;
    }
    if let Some(v) = &args.effort {
        next.effort = Some(claude_effort(v)?);
    }
    if args.clear_effort {
        next.effort = None;
    }
    if let Some(v) = args.timeout_ms {
        next.timeout_ms = v;
    }
    let current = if changes {
        settings::save_ask_settings(&path, next)
            .map_err(|e| anyhow!(e))
            .context("save ask settings")?
    } else {
        next
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&current)?);
        return Ok(());
    }
    println!("enabled        {}", current.enabled);
    println!("web_search     {}", current.web_search);
    println!(
        "instructions   {}",
        if current.instructions.is_empty() { "(none)" } else { &current.instructions }
    );
    println!("model          {}", current.model.as_deref().unwrap_or("(default: sonnet)"));
    println!("effort         {}", current.effort.map_or("(default: low)", RefineEffort::as_str));
    println!("timeout_ms     {}", current.timeout_ms);
    println!("{}  {}", if changes { "written      " } else { "settings_path" }, path.display());
    if changes {
        eprintln!("A running OpenWhisper picks this up when its Ask settings are opened, or on restart.");
    }
    Ok(())
}

/// An answer on one line, cut to fit a terminal row.
fn one_line(answer: &str) -> String {
    let flat = answer.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = flat.chars().take(110).collect();
    if flat.chars().count() > 110 {
        out.push_str("...");
    }
    out
}

fn run_eval(args: EvalArgs, json: bool) -> Result<()> {
    let (ask, refine) = load(&args.overrides)?;
    let backend_status = refine::backend_status(&refine);
    if !backend_status.ready {
        bail!("backend {} is not ready: {}", backend_status.backend, backend_status.message);
    }
    let plan = eval::plan(&ask, &refine);
    if !json {
        eprintln!("questions go to: {}", plan.destination());
    }
    let mut results = Vec::new();
    for case in eval::cases() {
        if args.only.as_deref().is_some_and(|prefix| !case.name.starts_with(prefix)) {
            continue;
        }
        // A fresh conversation per case, like a fresh ask.
        let mut backend = plan.backend();
        let result = eval::run_case(&case, &plan, backend.as_mut());
        if !json {
            println!(
                "{}  {:<24} {:>6} {:>6} ms  {}",
                if result.passed { "PASS" } else { "FAIL" },
                result.name,
                result.first_text_ms.map_or("-".to_string(), |ms| ms.to_string()),
                result.elapsed_ms,
                one_line(&result.answer)
            );
            if !result.passed {
                println!("      asked:   {}", result.question);
                println!("      risk:    {}", result.risk);
                println!("      problem: {}", result.problem);
                if !result.answer.is_empty() {
                    println!("      answer:  {}", result.answer.replace('\n', "\n               "));
                }
            }
        }
        results.push(result);
    }
    if results.is_empty() {
        bail!(
            "no eval case starts with \"{}\"; the cases are: {}",
            args.only.unwrap_or_default(),
            eval::cases().iter().map(|c| c.name).collect::<Vec<_>>().join(", ")
        );
    }

    let passed = results.iter().filter(|r| r.passed).count();
    let first_text = latency(results.iter().filter_map(|r| r.first_text_ms));
    let total = latency(results.iter().filter(|r| r.error.is_empty()).map(|r| r.elapsed_ms));
    if json {
        let value = serde_json::json!({
            "backend": refine.backend.as_str(),
            "model": plan.setup.model,
            "effort": plan.effort_id(),
            "passed": passed,
            "total": results.len(),
            "latency_ms": { "first_text": first_text.to_json(), "total": total.to_json() },
            "results": results,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!();
        println!("{passed}/{} passed", results.len());
        println!(
            "latency ({}, {}{}, cold): first text min {} ms, median {} ms, max {} ms; whole answer min {} ms, \
median {} ms, max {} ms",
            refine.backend.as_str(),
            plan.setup.model,
            effort_label(plan.effort_id()),
            first_text.min,
            first_text.median,
            first_text.max,
            total.min,
            total.median,
            total.max
        );
    }
    if passed == results.len() {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

/// Response times over the cases that got an answer.
#[derive(Debug, PartialEq, Eq)]
struct Latency {
    answers: usize,
    min: u64,
    median: u64,
    max: u64,
}

impl Latency {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({ "answers": self.answers, "min": self.min, "median": self.median, "max": self.max })
    }
}

fn latency(times: impl Iterator<Item = u64>) -> Latency {
    let mut times: Vec<u64> = times.collect();
    times.sort_unstable();
    Latency {
        answers: times.len(),
        min: times.first().copied().unwrap_or(0),
        median: times.get(times.len() / 2).copied().unwrap_or(0),
        max: times.last().copied().unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_override_goes_where_the_backend_reads_it() {
        let o = Overrides { model: Some("opus".into()), effort: Some("high".into()), ..Overrides::default() };
        let (ask, refine) = apply(AskSettings::default(), RefineSettings::default(), &o).unwrap();
        assert_eq!(ask.model.as_deref(), Some("opus"));
        assert_eq!(ask.effort, Some(RefineEffort::High));
        // Refine's own model is not touched by an ask override.
        assert_eq!(refine, RefineSettings::default());
        assert_eq!(AskPlan::with_terms(&ask, &refine, &[]).setup.model, "opus");

        let o = Overrides {
            backend: Some("openai-compatible".into()),
            model: Some("local-model".into()),
            effort: Some("none".into()),
            base_url: Some("http://localhost:11434/v1/".into()),
            ..Overrides::default()
        };
        let (ask, refine) = apply(AskSettings::default(), RefineSettings::default(), &o).unwrap();
        assert_eq!(ask.model, None);
        assert_eq!(refine.openai.model, "local-model");
        assert_eq!(refine.openai.effort, None);
        assert_eq!(refine.openai.base_url, "http://localhost:11434/v1");
        let plan = AskPlan::with_terms(&ask, &refine, &[]);
        assert_eq!((plan.setup.model.as_str(), plan.effort_id()), ("local-model", ""));
    }

    #[test]
    fn overrides_are_checked_and_cleaned() {
        let bad = Overrides { effort: Some("turbo".into()), ..Overrides::default() };
        assert!(apply(AskSettings::default(), RefineSettings::default(), &bad).is_err());
        let bad = Overrides { backend: Some("telepathy".into()), ..Overrides::default() };
        assert!(apply(AskSettings::default(), RefineSettings::default(), &bad).is_err());
        // "none" is an OpenAI-compatible thing.
        let bad = Overrides { effort: Some("none".into()), ..Overrides::default() };
        assert!(apply(AskSettings::default(), RefineSettings::default(), &bad).is_err());

        let o = Overrides {
            timeout_ms: Some(1),
            instructions: Some("  Answer in Danish. ".into()),
            claude_path: Some("/opt/claude".into()),
            ..Overrides::default()
        };
        let (ask, refine) = apply(AskSettings::default(), RefineSettings::default(), &o).unwrap();
        assert_eq!(ask.timeout_ms, settings::ASK_TIMEOUT_MIN_MS);
        assert_eq!(ask.instructions, "Answer in Danish.");
        assert_eq!(refine.claude_cli.path.as_deref(), Some("/opt/claude"));
        // Nothing here switches Ask on: running is the opt-in.
        assert!(!ask.enabled);
    }

    #[test]
    fn latency_is_over_the_answers_there_were() {
        assert_eq!(latency([900u64, 1500, 1200].into_iter()), Latency { answers: 3, min: 900, median: 1200, max: 1500 });
        assert_eq!(latency(std::iter::empty()), Latency { answers: 0, min: 0, median: 0, max: 0 });
    }

    #[test]
    fn a_long_answer_fits_one_row() {
        assert_eq!(one_line("Oslo.\n\nIt is  the capital."), "Oslo. It is the capital.");
        let long = one_line(&"word ".repeat(100));
        assert!(long.ends_with("...") && long.chars().count() == 113, "{long}");
    }
}
