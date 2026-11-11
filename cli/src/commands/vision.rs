//! `openwhisper vision <subcommand>`: the headless surface of Vision
//! (TASK-98), the screenshot an ask can carry. Same core functions the
//! desktop app calls, same `settings.json`.
//!
//! Subcommands:
//! - `status`: the trigger, the phrases, the never-capture list and
//!   whether this process may record the screen.
//! - `config [--trigger off|phrases|smart|always]`: print or set when
//!   an ask includes the screen.
//! - `phrases list | add TEXT | remove TEXT | reset`: the spoken
//!   phrases that ask for the screen.
//! - `never-capture list | add ID [--name NAME] | remove ID | reset`:
//!   the apps that never end up in a screenshot. Ids are what
//!   `openwhisper focus` prints.
//! - `check TEXT`: what the trigger decides for this question, and the
//!   phrase that matched. Decided on this machine: nothing is captured
//!   and nothing is sent.
//! - `request-permission`: ask macOS for the Screen Recording
//!   permission.
//! - `probe`: take one screenshot the way an ask would (same
//!   permission check, same never-capture list, same paint and scale)
//!   and print its size, the timings and how many apps were left out.
//!   The image stays in memory and is dropped; it is never printed,
//!   written or sent.
//! - `eval`: run the Vision eval sets against the real backend, with
//!   synthetic images. Never looks at the real screen.
//!
//! To ask a question with the screen: `openwhisper ask run --screen`.
//!
//! No command here writes a screenshot anywhere or prints image data.
//! A command line program has no Screen Recording permission of its
//! own: macOS counts the terminal it runs in.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Subcommand};
use openwhisper_core::ask::{
    AskBackend, AskError, AskEvent, AskInput, AskPlan, AskReply, AskSetup, CancelFlag, ScreenState, VisionChoice,
};
use openwhisper_core::refine;
use openwhisper_core::settings::{self, AskSettings, NeverCaptureApp, RefineSettings, VisionSettings, VisionTrigger};
use openwhisper_core::vision::{self, Decision, eval};

use super::ask::{Overrides, effort_label, latency, load, one_line, parse_trigger};

#[derive(Args, Debug)]
pub struct VisionArgs {
    #[command(subcommand)]
    command: VisionCommand,
}

#[derive(Subcommand, Debug)]
enum VisionCommand {
    /// Show the trigger, the lists and the screen recording permission.
    Status,
    /// Print when an ask includes the screen, or set it.
    Config(ConfigArgs),
    /// The phrases that ask for the screen.
    Phrases(PhrasesArgs),
    /// The apps that are never in a screenshot.
    NeverCapture(NeverCaptureArgs),
    /// Show what the trigger decides for a question. Captures nothing.
    Check(CheckArgs),
    /// Ask macOS for the Screen Recording permission.
    RequestPermission,
    /// Take one screenshot like an ask would; print sizes and timings only.
    Probe(ProbeArgs),
    /// Run the Vision eval sets against the real backend.
    Eval(EvalArgs),
    /// The helper window of the live pixel test.
    #[command(hide = true)]
    TestWindow(TestWindowArgs),
}

#[derive(Args, Debug)]
struct ConfigArgs {
    /// When an ask includes the screen: off, phrases (you say so),
    /// smart (you say so, or the model asks for it) or always.
    #[arg(long, value_name = "TRIGGER")]
    trigger: Option<String>,
}

#[derive(Args, Debug)]
struct PhrasesArgs {
    #[command(subcommand)]
    command: PhrasesCommand,
}

#[derive(Subcommand, Debug)]
enum PhrasesCommand {
    /// Print the phrases in force.
    List,
    /// Add a phrase.
    Add(TextArg),
    /// Remove a phrase (case is ignored).
    Remove(TextArg),
    /// Back to the built-in phrases.
    Reset,
}

#[derive(Args, Debug)]
struct TextArg {
    /// The phrase, e.g. "look at this window".
    text: String,
}

#[derive(Args, Debug)]
struct NeverCaptureArgs {
    #[command(subcommand)]
    command: NeverCaptureCommand,
}

#[derive(Subcommand, Debug)]
enum NeverCaptureCommand {
    /// Print the apps in force.
    List,
    /// Add an app by its id.
    Add(AppArgs),
    /// Remove an app by its id (case is ignored).
    Remove(IdArg),
    /// Back to the built-in list (the common password managers).
    Reset,
}

#[derive(Args, Debug)]
struct AppArgs {
    /// The app's id as `openwhisper focus` prints it: the bundle id on
    /// macOS (com.example.vault), the exe name on Windows (vault.exe).
    id: String,
    /// A name to show for it.
    #[arg(long)]
    name: Option<String>,
}

#[derive(Args, Debug)]
struct IdArg {
    /// The app's id.
    id: String,
}

#[derive(Args, Debug)]
struct CheckArgs {
    /// The question, as it would be heard.
    text: String,
    /// Decide with this trigger instead of the saved one.
    #[arg(long, value_name = "TRIGGER")]
    trigger: Option<String>,
}

#[derive(Args, Debug)]
struct ProbeArgs {
    /// Do the capture in this process and print the raw result. What
    /// `probe` runs itself with, to read the capture's log line.
    #[arg(long, hide = true)]
    inner: bool,
}

#[derive(Args, Debug)]
struct EvalArgs {
    /// Run only cases whose name starts with this text: smart,
    /// phrases, read, inject, or a whole case name.
    #[arg(long, value_name = "PREFIX")]
    only: Option<String>,
    /// The trigger whose prompt the cases with a screenshot run
    /// under: phrases (the default setting), always or smart.
    #[arg(long, value_name = "TRIGGER", default_value = "phrases")]
    trigger: String,
    #[command(flatten)]
    overrides: Overrides,
}

#[derive(Args, Debug)]
struct TestWindowArgs {
    /// The window's colour, as RRGGBB.
    #[arg(long)]
    color: String,
    /// Left edge, in global display points.
    #[arg(long, allow_negative_numbers = true)]
    x: f64,
    /// Top edge, in global display points (y grows downward).
    #[arg(long, allow_negative_numbers = true)]
    y: f64,
    #[arg(long)]
    width: f64,
    #[arg(long)]
    height: f64,
    /// Window level; a higher one is in front.
    #[arg(long, default_value_t = 1000)]
    level: i64,
    /// Close by itself after this many seconds, so a helper whose
    /// test died does not stay on the screen.
    #[arg(long, default_value_t = 120)]
    timeout_s: u64,
}

pub fn run(args: VisionArgs, json: bool) -> Result<()> {
    match args.command {
        VisionCommand::Status => status(json),
        VisionCommand::Config(a) => config(a, json),
        VisionCommand::Phrases(a) => phrases(a.command, json),
        VisionCommand::NeverCapture(a) => never_capture(a.command, json),
        VisionCommand::Check(a) => check(a, json),
        VisionCommand::RequestPermission => request_permission(json),
        VisionCommand::Probe(a) if a.inner => probe_inner(),
        VisionCommand::Probe(_) => probe(json),
        VisionCommand::Eval(a) => run_eval(a, json),
        VisionCommand::TestWindow(a) => test_window(a),
    }
}

fn settings_path() -> Result<PathBuf> {
    Ok(super::config_dir()?.join("settings.json"))
}

fn load_settings() -> Result<(PathBuf, VisionSettings)> {
    let path = settings_path()?;
    let settings = settings::load_vision_settings(&path);
    Ok((path, settings))
}

fn save(path: &std::path::Path, next: VisionSettings) -> Result<VisionSettings> {
    let saved = settings::save_vision_settings(path, next).map_err(|e| anyhow!(e)).context("save vision settings")?;
    eprintln!("A running OpenWhisper picks this up when its Ask settings are opened, or on restart.");
    Ok(saved)
}

/// What each trigger does, in one line.
fn trigger_line(trigger: VisionTrigger) -> &'static str {
    match trigger {
        VisionTrigger::Off => "an ask never includes the screen",
        VisionTrigger::Phrases => "an ask includes the screen when the question has one of the phrases",
        VisionTrigger::Smart => "a phrase, or the model says it needs the screen (the question goes as text first)",
        VisionTrigger::Always => "every ask includes the screen",
    }
}

const TERMINAL_GRANT: &str =
    "a command line program uses the Screen Recording permission of the terminal it runs in; the desktop app has its own";

fn app_json(app: &NeverCaptureApp) -> serde_json::Value {
    serde_json::json!({ "id": app.id, "name": app.name })
}

fn status(json: bool) -> Result<()> {
    let (path, s) = load_settings()?;
    let phrases = s.effective_phrases();
    let apps = s.effective_never_capture();
    let permission = vision::permission();
    if json {
        let value = serde_json::json!({
            "trigger": s.trigger.as_str(),
            "phrase_count": phrases.len(),
            "phrases": phrases,
            "phrases_are_default": s.phrases.is_none(),
            "never_capture": apps.iter().map(app_json).collect::<Vec<_>>(),
            "never_capture_is_default": s.never_capture.is_none(),
            "permission": permission.as_str(),
            "permission_note": TERMINAL_GRANT,
            "settings_path": path.display().to_string(),
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    let which = |default: bool| if default { "the built-in list" } else { "your list" };
    println!("trigger        {} ({})", s.trigger.as_str(), trigger_line(s.trigger));
    println!("phrases        {} ({})", phrases.len(), which(s.phrases.is_none()));
    println!("never capture  {} ({})", apps.len(), which(s.never_capture.is_none()));
    for app in &apps {
        println!("               {:<40} {}", app.id, app.name);
    }
    println!("permission     {} for this process", permission.as_str());
    println!("               {TERMINAL_GRANT}");
    println!("settings_path  {}", path.display());
    Ok(())
}

fn config(args: ConfigArgs, json: bool) -> Result<()> {
    let (path, mut s) = load_settings()?;
    let changes = args.trigger.is_some();
    if let Some(id) = &args.trigger {
        s.trigger = VisionTrigger::parse(id)
            .ok_or_else(|| anyhow!("unknown trigger \"{id}\"; use off, phrases, smart or always"))?;
        s = save(&path, s)?;
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&s)?);
        return Ok(());
    }
    println!("trigger        {} ({})", s.trigger.as_str(), trigger_line(s.trigger));
    println!("phrases        {}", s.effective_phrases().len());
    println!("never capture  {}", s.effective_never_capture().len());
    println!("{}  {}", if changes { "written      " } else { "settings_path" }, path.display());
    Ok(())
}

fn print_phrases(s: &VisionSettings, json: bool) -> Result<()> {
    let list = s.effective_phrases();
    if json {
        let value = serde_json::json!({ "phrases": list, "default": s.phrases.is_none() });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    for phrase in &list {
        println!("{phrase}");
    }
    if list.is_empty() {
        eprintln!("no phrases: with the trigger on phrases, no ask includes the screen");
    }
    Ok(())
}

fn phrases(command: PhrasesCommand, json: bool) -> Result<()> {
    let (path, mut s) = load_settings()?;
    match command {
        PhrasesCommand::List => {}
        PhrasesCommand::Add(arg) => {
            let phrase = arg.text.split_whitespace().collect::<Vec<_>>().join(" ");
            if phrase.is_empty() {
                bail!("a phrase needs at least one word");
            }
            let mut list = s.effective_phrases();
            if list.iter().any(|p| p.to_lowercase() == phrase.to_lowercase()) {
                // Nothing to write: the list stays what it was (the
                // built-in one stays the built-in one).
                eprintln!("\"{phrase}\" is already a phrase");
            } else {
                list.push(phrase);
                s.phrases = Some(list);
                s = save(&path, s)?;
            }
        }
        PhrasesCommand::Remove(arg) => {
            let wanted = arg.text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
            let mut list = s.effective_phrases();
            let before = list.len();
            list.retain(|p| p.to_lowercase() != wanted);
            if list.len() == before {
                bail!("\"{}\" is not one of the phrases; see `openwhisper vision phrases list`", arg.text.trim());
            }
            s.phrases = Some(list);
            s = save(&path, s)?;
        }
        PhrasesCommand::Reset => {
            s.phrases = None;
            s = save(&path, s)?;
        }
    }
    print_phrases(&s, json)
}

fn print_apps(s: &VisionSettings, json: bool) -> Result<()> {
    let list = s.effective_never_capture();
    if json {
        let value = serde_json::json!({
            "never_capture": list.iter().map(app_json).collect::<Vec<_>>(),
            "default": s.never_capture.is_none(),
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    for app in &list {
        println!("{:<40} {}", app.id, app.name);
    }
    if list.is_empty() {
        eprintln!("the list is empty: no app is kept out of a screenshot");
    }
    Ok(())
}

fn never_capture(command: NeverCaptureCommand, json: bool) -> Result<()> {
    let (path, mut s) = load_settings()?;
    match command {
        NeverCaptureCommand::List => {}
        NeverCaptureCommand::Add(arg) => {
            let id = arg.id.trim().to_lowercase();
            if id.is_empty() {
                bail!("an app needs an id; `openwhisper focus --running` lists the open apps with theirs");
            }
            let mut list = s.effective_never_capture();
            let name = arg.name.unwrap_or_default().trim().to_string();
            let changed = match list.iter_mut().find(|app| app.id == id) {
                // Already listed: a new name replaces the old one.
                Some(app) if !name.is_empty() && app.name != name => {
                    app.name = name;
                    true
                }
                Some(_) => {
                    eprintln!("{id} is already on the list");
                    false
                }
                None => {
                    list.push(NeverCaptureApp::new(&id, &name));
                    true
                }
            };
            if changed {
                s.never_capture = Some(list);
                s = save(&path, s)?;
            }
        }
        NeverCaptureCommand::Remove(arg) => {
            let id = arg.id.trim().to_lowercase();
            let mut list = s.effective_never_capture();
            let before = list.len();
            list.retain(|app| app.id != id);
            if list.len() == before {
                bail!("{id} is not on the list; see `openwhisper vision never-capture list`");
            }
            s.never_capture = Some(list);
            s = save(&path, s)?;
        }
        NeverCaptureCommand::Reset => {
            s.never_capture = None;
            s = save(&path, s)?;
        }
    }
    print_apps(&s, json)
}

fn check(args: CheckArgs, json: bool) -> Result<()> {
    let (_, mut s) = load_settings()?;
    if let Some(id) = &args.trigger {
        s.trigger = parse_trigger(id, s.trigger)?;
    }
    // The phrase is reported for every trigger: it is what `phrases`
    // and `smart` would capture on.
    let phrase = vision::matched_phrase(&s.effective_phrases(), &args.text);
    let (id, line) = match vision::decide(&s, &args.text) {
        Decision::NoCapture => ("no-capture", "no screenshot for this question"),
        Decision::Capture { .. } => ("capture", "a screenshot would go with this question"),
        Decision::AskModel => {
            ("ask-model", "the question would go as text; a screenshot follows only if the model asks for it")
        }
    };
    if json {
        let value = serde_json::json!({
            "text": args.text,
            "trigger": s.trigger.as_str(),
            "decision": id,
            "phrase": phrase,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    println!("trigger   {}", s.trigger.as_str());
    println!("decision  {id}: {line}");
    println!("phrase    {}", phrase.as_deref().unwrap_or("(none of the phrases is in the question)"));
    eprintln!("decided on this machine: nothing was captured and nothing was sent");
    Ok(())
}

fn request_permission(json: bool) -> Result<()> {
    let granted = vision::request_permission();
    let what_now = if granted {
        "Screen recording is allowed for this process."
    } else if cfg!(target_os = "macos") {
        "Screen recording is not allowed yet. macOS has been asked: allow it in System Settings under Privacy & \
Security, Screen & System Audio Recording (a command line program shows up there as the terminal it runs in, the \
desktop app as OpenWhisper), then quit and reopen that app. The permission only counts after a restart."
    } else {
        "Screen capture is not allowed for this process."
    };
    if json {
        let value = serde_json::json!({
            "granted": granted,
            "permission": vision::permission().as_str(),
            "message": what_now,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("{what_now}");
    }
    Ok(())
}

// --- probe ---------------------------------------------------------

/// A backend that sends nothing anywhere. `probe` runs a real ask
/// through it, because only an ask can take a screenshot: the capture
/// is then exactly the one an ask makes. It keeps the size of the
/// image it was handed and nothing else.
#[derive(Default)]
struct NoModel {
    /// Width, height and file size of the PNG it was handed.
    image: Option<(u32, u32, usize)>,
}

/// Width and height from a PNG file's header.
fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let number = |at: usize| u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]);
    Some((number(16), number(20)))
}

impl AskBackend for NoModel {
    fn id(&self) -> &'static str {
        "probe"
    }

    fn warm(&mut self, _setup: &AskSetup) {}

    fn send(
        &mut self,
        _setup: &AskSetup,
        input: &AskInput,
        _sink: &mut dyn FnMut(AskEvent),
        _cancel: &CancelFlag,
        _timeout: Duration,
    ) -> Result<AskReply, AskError> {
        self.image = input.images.first().and_then(|image| {
            let (width, height) = png_size(&image.data)?;
            Some((width, height, image.data.len()))
        });
        Ok(AskReply { text: "probe".to_string(), ..AskReply::default() })
    }
}

/// The capture itself: one ask with a forced screenshot and no model.
/// Prints one JSON record of sizes and timings.
fn probe_inner() -> Result<()> {
    let (_, s) = load_settings()?;
    let plan = AskPlan::with_terms(&AskSettings::default(), &RefineSettings::default(), &[])
        .with_vision(&s, VisionChoice::Force);
    let mut backend = NoModel::default();
    let mut began: Option<Instant> = None;
    let mut took_ms: Option<u64> = None;
    let mut sink = |event: AskEvent| {
        if let AskEvent::Screen { state, .. } = event {
            match (state, began) {
                (ScreenState::Capturing, _) => began = Some(Instant::now()),
                (_, Some(at)) => took_ms = Some(at.elapsed().as_millis() as u64),
                _ => {}
            }
        }
    };
    let reply = plan
        .send(&mut backend, &AskInput::text("probe"), &mut sink, &CancelFlag::new())
        .map_err(|e| anyhow!("{e}"))?;
    let image = backend.image;
    let record = serde_json::json!({
        "captured": reply.screen == ScreenState::Included && image.is_some(),
        "state": reply.screen.as_str(),
        "reason": reply.screen_notice,
        "width": image.map(|i| i.0),
        "height": image.map(|i| i.1),
        "png_bytes": image.map(|i| i.2),
        "total_ms": took_ms,
    });
    println!("{}", serde_json::to_string(&record)?);
    Ok(())
}

/// What the capture's line in the verbose log says: the display's
/// size in pixels, how many never-capture apps were left out of the
/// capture, how many of their window rectangles were painted, and how
/// long the capture itself took.
#[derive(Debug, PartialEq, Eq, Default, Clone, Copy)]
struct CaptureLog {
    native: Option<(u32, u32)>,
    excluded_apps: Option<u64>,
    painted_rects: Option<u64>,
    capture_ms: Option<u64>,
}

/// Read the `[ow.vision] capture ...` line out of a verbose log.
/// `None` when there is no such line.
fn capture_log(log: &str) -> Option<CaptureLog> {
    let line = log.lines().find_map(|l| l.trim().strip_prefix("[ow.vision] capture "))?;
    let number = |key: &str| {
        line.split_whitespace().find_map(|word| word.strip_prefix(key)?.strip_prefix('=')?.parse::<u64>().ok())
    };
    let native = line.split_whitespace().next().and_then(|size| {
        let (w, h) = size.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    });
    Some(CaptureLog {
        native,
        excluded_apps: number("excluded_apps"),
        painted_rects: number("painted_rects"),
        capture_ms: number("capture_ms"),
    })
}

/// `probe`: run the capture in a copy of this program with the
/// verbose log on, and put its record and the capture's log line
/// together. The counts of apps left out and rectangles painted are
/// only in that log line: an ask does not carry them.
fn probe(json: bool) -> Result<()> {
    let exe = std::env::current_exe().context("find this program")?;
    let out = Command::new(exe)
        .args(["vision", "probe", "--inner"])
        .env("OPENWHISPER_VERBOSE", "1")
        .stdin(Stdio::null())
        .output()
        .context("run the capture")?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() {
        let said: Vec<&str> = stderr.lines().filter(|l| !l.trim_start().starts_with("[ow.")).collect();
        bail!("the probe failed: {}", said.join(" ").trim());
    }
    let record: serde_json::Value = serde_json::from_slice(&out.stdout).context("read the probe's record")?;
    let log = capture_log(&stderr).unwrap_or_default();
    let captured = record["captured"].as_bool().unwrap_or(false);
    let permission = vision::permission();
    let listed = settings::load_vision_settings(&settings_path()?).effective_never_capture().len();

    if json {
        let value = serde_json::json!({
            "captured": captured,
            "state": record["state"],
            "reason": record["reason"],
            "width": record["width"],
            "height": record["height"],
            "png_bytes": record["png_bytes"],
            "display_width": log.native.map(|n| n.0),
            "display_height": log.native.map(|n| n.1),
            "capture_ms": log.capture_ms,
            "total_ms": record["total_ms"],
            "excluded_apps": log.excluded_apps,
            "painted_rects": log.painted_rects,
            "never_capture_listed": listed,
            "permission": permission.as_str(),
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    if !captured {
        println!("captured       no");
        println!("reason         {}", record["reason"].as_str().filter(|r| !r.is_empty()).unwrap_or("not known"));
        println!("permission     {} for this process", permission.as_str());
        println!("               {TERMINAL_GRANT}");
        return Ok(());
    }
    let number = |v: &serde_json::Value| v.as_u64().map_or("-".to_string(), |n| n.to_string());
    let count = |n: Option<u64>| n.map_or("not reported".to_string(), |n| n.to_string());
    println!("captured       yes, in memory only; it is gone again, and nothing was written or sent");
    println!(
        "size           {} x {} px{}",
        number(&record["width"]),
        number(&record["height"]),
        log.native.map_or(String::new(), |(w, h)| format!(" (the display is {w} x {h} px)"))
    );
    println!("png            {} bytes", number(&record["png_bytes"]));
    println!(
        "time           {} ms for the capture, {} ms with paint, scaling and PNG",
        count(log.capture_ms),
        number(&record["total_ms"])
    );
    println!("excluded apps  {} (of {listed} on the never-capture list; an app that is not open counts as 0)", count(log.excluded_apps));
    println!("painted rects  {}", count(log.painted_rects));
    println!("permission     {} for this process", permission.as_str());
    Ok(())
}

// --- eval ----------------------------------------------------------

fn run_eval(args: EvalArgs, json: bool) -> Result<()> {
    let (ask, refine) = load(&args.overrides)?;
    let trigger = VisionTrigger::parse(&args.trigger)
        .ok_or_else(|| anyhow!("unknown trigger \"{}\"; use phrases, always or smart", args.trigger))?;
    let runner = eval::Runner::new(&ask, &refine, trigger).map_err(|e| anyhow!(e))?;
    let cases: Vec<eval::EvalCase> = eval::cases()
        .into_iter()
        .filter(|case| args.only.as_deref().is_none_or(|prefix| case.name.starts_with(prefix)))
        .collect();
    if cases.is_empty() {
        bail!(
            "no eval case starts with \"{}\"; the sets are smart, phrases, read and inject, and the cases are: {}",
            args.only.unwrap_or_default(),
            eval::cases().iter().map(|c| c.name).collect::<Vec<_>>().join(", ")
        );
    }
    let backend_status = refine::backend_status(&refine);
    if !backend_status.ready {
        bail!("backend {} is not ready: {}", backend_status.backend, backend_status.message);
    }
    let plan = runner.plan();
    if !json {
        eprintln!("questions go to: {}", plan.destination());
        eprintln!("the images are drawn from made-up text; the real screen is never captured");
        eprintln!("with the facts of a made-up computer, the same for whoever runs this");
    }
    let mut results = Vec::new();
    for case in &cases {
        // A fresh conversation per case, like a fresh ask.
        let mut backend = plan.backend();
        let result = runner.run_case(case, backend.as_mut());
        if !json {
            println!(
                "{}  {:<28} {:>6} {:>6} ms  {}",
                if result.passed { "PASS" } else { "FAIL" },
                result.name,
                result.first_text_ms.map_or("-".to_string(), |ms| ms.to_string()),
                result.elapsed_ms,
                one_line(&result.answer)
            );
            if !result.passed {
                println!("      asked:   {}{}", result.question, if result.image { " (with a synthetic screenshot)" } else { "" });
                println!("      risk:    {}", result.risk);
                println!("      problem: {}", result.problem);
                if !result.answer.is_empty() {
                    println!("      answer:  {}", result.answer.replace('\n', "\n               "));
                }
            }
        }
        results.push(result);
    }

    let passed = results.iter().filter(|r| r.passed).count();
    let per_set: Vec<(eval::Set, usize, usize)> = eval::Set::ALL
        .into_iter()
        .map(|set| {
            let of: Vec<&eval::EvalResult> = results.iter().filter(|r| r.set == set).collect();
            (set, of.iter().filter(|r| r.passed).count(), of.len())
        })
        .filter(|(_, _, total)| *total > 0)
        .collect();
    let counts = eval::confusion(&results);
    let answered = |image: bool| results.iter().filter(move |r| r.error.is_empty() && r.image == image);
    let first_text = |image: bool| latency(answered(image).filter_map(|r| r.first_text_ms));
    let whole = |image: bool| latency(answered(image).map(|r| r.elapsed_ms));
    let has_smart = results.iter().any(|r| r.set == eval::Set::Smart);

    if json {
        let sets: serde_json::Map<String, serde_json::Value> = per_set
            .iter()
            .map(|(set, ok, total)| (set.as_str().to_string(), serde_json::json!({ "passed": ok, "total": total })))
            .collect();
        let value = serde_json::json!({
            "backend": refine.backend.as_str(),
            "model": plan.setup.model,
            "effort": plan.effort_id(),
            "image_trigger": trigger.as_str(),
            "passed": passed,
            "total": results.len(),
            "sets": sets,
            "smart": if has_smart { serde_json::to_value(counts)? } else { serde_json::Value::Null },
            "latency_ms": {
                "text": { "first_text": first_text(false).to_json(), "total": whole(false).to_json() },
                "image": { "first_text": first_text(true).to_json(), "total": whole(true).to_json() },
            },
            "results": results,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!();
        println!("{passed}/{} passed", results.len());
        for (set, ok, total) in &per_set {
            println!("  {:<8} {ok}/{total}", set.as_str());
        }
        if has_smart {
            println!(
                "smart: needs the screen: {} asked for it, {} answered without; does not need it: {} answered, {} \
asked for it",
                counts.needed_and_asked, counts.needed_and_answered, counts.plain_and_answered, counts.plain_and_asked
            );
        }
        for (label, image) in [("text only", false), ("with an image", true)] {
            let (first, total) = (first_text(image), whole(image));
            if total.answers == 0 {
                continue;
            }
            println!(
                "latency {label} ({} {}; {}, {}{}, cold): first text min {} ms, median {} ms, max {} ms; \
whole answer min {} ms, median {} ms, max {} ms",
                total.answers,
                if total.answers == 1 { "answer" } else { "answers" },
                refine.backend.as_str(),
                plan.setup.model,
                effort_label(plan.effort_id()),
                first.min,
                first.median,
                first.max,
                total.min,
                total.median,
                total.max
            );
        }
        println!("cases with a screenshot ran under the prompt of the trigger \"{}\"", trigger.as_str());
    }
    if passed == results.len() {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

// --- the test window ----------------------------------------------

/// `RRGGBB` as three parts between 0 and 1.
fn parse_colour(hex: &str) -> Result<(f64, f64, f64)> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("a colour is six hex digits, RRGGBB; got \"{hex}\"");
    }
    let part = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).map(|v| f64::from(v) / 255.0);
    Ok((part(0)?, part(2)?, part(4)?))
}

/// Show a borderless window of one colour above everything else and
/// stay until killed (or until the timeout). The live pixel test
/// starts two of these and checks what a capture shows where they
/// are; see `cli/tests/vision_live.rs`.
#[cfg(target_os = "macos")]
fn test_window(args: TestWindowArgs) -> Result<()> {
    use std::io::Write;

    use objc2::MainThreadMarker;
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSScreen, NSWindow,
        NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    let (red, green, blue) = parse_colour(&args.color)?;
    if !(args.width >= 1.0 && args.height >= 1.0) {
        bail!("the window needs a width and a height of at least 1");
    }
    let mtm = MainThreadMarker::new().ok_or_else(|| anyhow!("the test window must be made on the main thread"))?;
    let app = NSApplication::sharedApplication(mtm);
    // No Dock icon, no menu bar, never the active app.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    // The arguments are in the coordinates Vision uses: origin at the
    // top left of the primary display, y downward. AppKit counts from
    // the bottom left of that display, upward.
    let screens = NSScreen::screens(mtm);
    let primary = screens.firstObject().ok_or_else(|| anyhow!("no display"))?;
    let primary_height = primary.frame().size.height;
    let rect = NSRect::new(
        NSPoint::new(args.x, primary_height - args.y - args.height),
        NSSize::new(args.width, args.height),
    );
    // SAFETY: a plain NSWindow made on the main thread, kept alive by
    // this function for the life of the process.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            mtm.alloc(),
            rect,
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: the window is owned by `window` here, not by AppKit's
    // close handling.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setBackgroundColor(Some(&NSColor::colorWithSRGBRed_green_blue_alpha(red, green, blue, 1.0)));
    window.setOpaque(true);
    window.setHasShadow(false);
    window.setIgnoresMouseEvents(true);
    window.setLevel(args.level as isize);
    window.orderFrontRegardless();

    // A helper whose test was killed must not stay on the screen.
    let timeout = Duration::from_secs(args.timeout_s.max(1));
    std::thread::spawn(move || {
        std::thread::sleep(timeout);
        std::process::exit(0);
    });
    println!("ready");
    std::io::stdout().flush().ok();
    app.run();
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn test_window(args: TestWindowArgs) -> Result<()> {
    parse_colour(&args.color)?;
    let _ = (args.x, args.y, args.width, args.height, args.level, args.timeout_s);
    bail!("the test window is only available on macOS")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_capture_line_of_the_verbose_log_is_read() {
        let log = "[ow.ask] something else\n[ow.vision] capture 3024x1964 -> 1568x1018 excluded_apps=1 painted_rects=2 \
capture_ms=47 total_ms=112\n[ow.vision] png_bytes=412345\n";
        assert_eq!(
            capture_log(log),
            Some(CaptureLog { native: Some((3024, 1964)), excluded_apps: Some(1), painted_rects: Some(2), capture_ms: Some(47) })
        );
        // No capture, no line.
        assert_eq!(capture_log("[ow.vision] png_bytes=1\nsomething"), None);
        assert_eq!(capture_log(""), None);
        // A line that changed shape gives what can still be read.
        assert_eq!(
            capture_log("[ow.vision] capture done painted_rects=3"),
            Some(CaptureLog { painted_rects: Some(3), ..CaptureLog::default() })
        );
    }

    /// `probe` reads the counts from a log line that core writes. If
    /// that line changes, this fails here instead of the counts going
    /// missing quietly.
    #[test]
    fn core_still_writes_the_capture_line_probe_reads() {
        let core = include_str!("../../../core/src/vision/capture.rs");
        let format = "\"[ow.vision] capture {}x{} -> {}x{} excluded_apps={} painted_rects={} capture_ms={} total_ms={}\"";
        assert!(core.contains(format), "the capture's verbose log line changed; update `capture_log`");
    }

    #[test]
    fn the_size_of_a_png_is_read_from_its_header() {
        let scene = vision::testimg::Scene { title: "Test", lines: Vec::new() };
        let png = vision::testimg::png(&scene).unwrap();
        assert_eq!(png_size(&png), Some((vision::testimg::WIDTH, vision::testimg::HEIGHT)));
        assert_eq!(png_size(b"not a png, but long enough to have a header"), None);
        assert_eq!(png_size(&png[..10]), None);
    }

    #[test]
    fn the_stand_in_backend_keeps_sizes_only() {
        let scene = vision::testimg::Scene { title: "Test", lines: Vec::new() };
        let data = vision::testimg::png(&scene).unwrap();
        let bytes = data.len();
        let image = openwhisper_core::ask::AskImage { media_type: "image/png".to_string(), data };
        let mut backend = NoModel::default();
        let setup = AskPlan::with_terms(&AskSettings::default(), &RefineSettings::default(), &[]).setup;
        let reply = backend
            .send(
                &setup,
                &AskInput { text: "probe".to_string(), images: vec![image] },
                &mut |_| {},
                &CancelFlag::new(),
                Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(reply.text, "probe");
        assert_eq!(backend.image, Some((vision::testimg::WIDTH, vision::testimg::HEIGHT, bytes)));
        // Without an image there is nothing to report.
        backend.send(&setup, &AskInput::text("probe"), &mut |_| {}, &CancelFlag::new(), Duration::from_secs(1)).unwrap();
        assert_eq!(backend.image, None);
    }

    #[test]
    fn colours_are_six_hex_digits() {
        assert_eq!(parse_colour("FF00ff").unwrap(), (1.0, 0.0, 1.0));
        assert_eq!(parse_colour("#000000").unwrap(), (0.0, 0.0, 0.0));
        for bad in ["", "fff", "gggggg", "ff00ff00"] {
            assert!(parse_colour(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn every_trigger_has_a_line_and_the_lists_have_defaults() {
        for trigger in VisionTrigger::ALL {
            assert!(!trigger_line(trigger).is_empty());
        }
        assert!(!settings::default_phrases().is_empty() && !settings::default_never_capture().is_empty());
    }
}
