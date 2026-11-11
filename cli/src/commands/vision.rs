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
//! - `probe [--text TEXT]`: a dry run. Says what a capture would do
//!   right now and takes none: the permission, the display that would
//!   be captured and its size, the app in front and whether it is on
//!   the never-capture list, which listed apps are open and how many
//!   windows they have on screen, and, with `--text`, what the trigger
//!   would decide for that question.
//! - `eval`: run the Vision eval sets against the real backend, with
//!   synthetic images. Never looks at the real screen.
//!
//! **No command here takes a screenshot.** A screenshot exists only
//! inside an ask, and the one way to an ask with the screen from the
//! command line is `openwhisper ask run --screen` (or `--vision`).
//! This file does not make a plan that may capture, and a test at the
//! bottom holds that line for the whole CLI.
//!
//! A command line program has no Screen Recording permission of its
//! own: macOS counts the terminal it runs in.
//!
//! The hidden `test-window` subcommand, the helper of the live pixel
//! test, exists only in a build with the cargo feature
//! `vision-live-test` (`cli/tests/vision_live.rs`).

use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Subcommand};
use openwhisper_core::refine;
use openwhisper_core::settings::{self, NeverCaptureApp, VisionSettings, VisionTrigger};
use openwhisper_core::vision::{self, Decision, eval};

use super::ask::{Overrides, STOPPED_EXIT, Stopped, effort_label, latency, load, one_line, parse_trigger};

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
    /// A dry run: say what a capture would do right now. Takes no screenshot.
    Probe(ProbeArgs),
    /// Run the Vision eval sets against the real backend.
    Eval(EvalArgs),
    /// The helper window of the live pixel test.
    #[cfg(feature = "vision-live-test")]
    #[command(hide = true)]
    TestWindow(test_window::TestWindowArgs),
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
    /// Also say what the trigger would decide for this question, as
    /// it would be heard.
    #[arg(long, value_name = "TEXT")]
    text: Option<String>,
    /// Decide with this trigger instead of the saved one.
    #[arg(long, value_name = "TRIGGER")]
    trigger: Option<String>,
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

pub fn run(args: VisionArgs, json: bool) -> Result<()> {
    match args.command {
        VisionCommand::Status => status(json),
        VisionCommand::Config(a) => config(a, json),
        VisionCommand::Phrases(a) => phrases(a.command, json),
        VisionCommand::NeverCapture(a) => never_capture(a.command, json),
        VisionCommand::Check(a) => check(a, json),
        VisionCommand::RequestPermission => request_permission(json),
        VisionCommand::Probe(a) => probe(a, json),
        VisionCommand::Eval(a) => run_eval(a, json),
        #[cfg(feature = "vision-live-test")]
        VisionCommand::TestWindow(a) => test_window::run(a),
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
    // An ask of the running app reads this block from the file when
    // it starts (`settings::fresh_vision_settings`).
    eprintln!("A running OpenWhisper uses this from its next ask on.");
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

/// What the trigger of `s` decides for `text`: an id, a line for a
/// person, and the phrase that is in the question (reported for every
/// trigger: it is what `phrases` and `smart` would capture on).
fn decision_for(s: &VisionSettings, text: &str) -> (&'static str, &'static str, Option<String>) {
    let phrase = vision::matched_phrase(&s.effective_phrases(), text);
    let (id, line) = match vision::decide(s, text) {
        Decision::NoCapture => ("no-capture", "no screenshot for this question"),
        Decision::Capture { .. } => ("capture", "a screenshot would go with this question"),
        Decision::AskModel => {
            ("ask-model", "the question would go as text; a screenshot follows only if the model asks for it")
        }
    };
    (id, line, phrase)
}

fn check(args: CheckArgs, json: bool) -> Result<()> {
    let (_, mut s) = load_settings()?;
    if let Some(id) = &args.trigger {
        s.trigger = parse_trigger(id, s.trigger)?;
    }
    let (id, line, phrase) = decision_for(&s, &args.text);
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

/// `probe`: a dry run. Everything a capture for an ask would look at
/// is looked at and reported (`vision::dry_run`), and no screenshot is
/// taken: there is no ask here, and a screenshot exists only inside
/// one. To see a real capture work, ask a question with the screen:
/// `openwhisper ask run --screen`.
fn probe(args: ProbeArgs, json: bool) -> Result<()> {
    let (_, mut s) = load_settings()?;
    if let Some(id) = &args.trigger {
        s.trigger = parse_trigger(id, s.trigger)?;
    }
    let seen = vision::dry_run(&s);
    let decision = args.text.as_deref().map(|text| (text, decision_for(&s, text)));
    let open: Vec<&vision::ListedApp> = seen.listed.iter().filter(|app| app.running).collect();

    if json {
        let mut value = serde_json::to_value(&seen)?;
        let map = value.as_object_mut().ok_or_else(|| anyhow!("the dry run did not serialize as an object"))?;
        map.insert("captured".into(), false.into());
        map.insert("dry_run".into(), true.into());
        map.insert("would_capture".into(), seen.refusal.is_none().into());
        map.insert("refusal_message".into(), seen.refusal.as_ref().map(|no| no.message()).into());
        map.insert("never_capture_is_default".into(), s.never_capture.is_none().into());
        map.insert("trigger".into(), s.trigger.as_str().into());
        map.insert(
            "decision".into(),
            match &decision {
                Some((text, (id, _, phrase))) => serde_json::json!({ "text": text, "decision": id, "phrase": phrase }),
                None => serde_json::Value::Null,
            },
        );
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }

    println!("captured       nothing: this is a dry run, it takes no screenshot");
    println!("permission     {} for this process", seen.permission.as_str());
    println!("               {TERMINAL_GRANT}");
    match &seen.display {
        Some(d) => println!(
            "display        {} x {} px would be captured ({} x {} points at {}, {})",
            d.width, d.height, d.point_width, d.point_height, d.x, d.y
        ),
        None => println!("display        none: {}", seen.display_problem),
    }
    if seen.front.is_empty() {
        println!("in front       not known: {}", seen.front_problem);
    }
    for (n, app) in seen.front.iter().enumerate() {
        let name = if app.name.is_empty() { "(no name)" } else { &app.name };
        let id = if app.id.is_empty() { "no id" } else { &app.id };
        println!(
            "{}{name} ({id}): {}",
            if n == 0 { "in front       " } else { "               " },
            if app.listed { "ON the never-capture list" } else { "not on the never-capture list" }
        );
    }
    println!(
        "never capture  {} on the list ({}), {} open",
        seen.listed.len(),
        if s.never_capture.is_none() { "the built-in list" } else { "your list" },
        open.len()
    );
    if seen.listed.is_empty() {
        println!("               the list is empty: no app is kept out of a screenshot");
    }
    for app in &open {
        let windows = match app.windows {
            Some(1) => "1 window on screen".to_string(),
            Some(n) => format!("{n} windows on screen"),
            None => "its windows could not be read".to_string(),
        };
        println!("               {:<40} {:<16} open, {windows}", app.id, app.name);
    }
    if !seen.windows_problem.is_empty() {
        println!("               the windows could not be read: {}", seen.windows_problem);
    }
    match &seen.refusal {
        None => println!("would capture  yes: nothing stands in the way right now"),
        Some(no) => println!("would capture  no. {}", no.message()),
    }
    println!("trigger        {} ({})", s.trigger.as_str(), trigger_line(s.trigger));
    if let Some((text, (id, line, phrase))) = &decision {
        println!("for \"{}\"", one_line(text));
        println!("               {id}: {line}");
        println!("               phrase: {}", phrase.as_deref().unwrap_or("(none of the phrases is in the question)"));
    }
    eprintln!("nothing was captured and nothing was sent; to ask with the screen: openwhisper ask run --screen");
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
    let mut stopped: Option<Stopped> = None;
    for (at, case) in cases.iter().enumerate() {
        // A fresh conversation per case, like a fresh ask.
        let mut backend = plan.backend();
        let result = runner.run_case(case, backend.as_mut());
        // A spend or rate limit: not a wrong answer, and every
        // further question would be refused the same way.
        if openwhisper_core::ask::eval::limit_reached(&result.error) {
            let stop = Stopped { case: result.name, reason: result.error, not_run: cases.len() - at };
            if !json {
                println!("{}", stop.line());
            }
            stopped = Some(stop);
            break;
        }
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
            "stopped": stopped.as_ref().map(Stopped::to_json),
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
        if let Some(stop) = &stopped {
            println!("{}", stop.summary());
        }
    }
    if stopped.is_some() {
        std::process::exit(STOPPED_EXIT);
    }
    if passed == results.len() {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

// --- the test window ----------------------------------------------

/// The helper window of the live pixel test. Only in a build with the
/// feature `vision-live-test`: see `cli/tests/vision_live.rs`.
#[cfg(feature = "vision-live-test")]
mod test_window;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_trigger_has_a_line_and_the_lists_have_defaults() {
        for trigger in VisionTrigger::ALL {
            assert!(!trigger_line(trigger).is_empty());
        }
        assert!(!settings::default_phrases().is_empty() && !settings::default_never_capture().is_empty());
    }

    #[test]
    fn a_decision_names_what_would_happen_and_the_phrase_that_is_there() {
        let with = |trigger| VisionSettings { trigger, ..VisionSettings::default() };
        let (id, line, phrase) = decision_for(&with(VisionTrigger::Phrases), "what is on my screen");
        assert_eq!((id, phrase.as_deref()), ("capture", Some("on my screen")));
        assert!(line.contains("would go with this question"));
        assert_eq!(decision_for(&with(VisionTrigger::Phrases), "two plus two").0, "no-capture");
        assert_eq!(decision_for(&with(VisionTrigger::Smart), "two plus two").0, "ask-model");
        assert_eq!(decision_for(&with(VisionTrigger::Always), "two plus two").0, "capture");
        // Off: nothing is captured, and the phrase is still reported.
        let (id, _, phrase) = decision_for(&with(VisionTrigger::Off), "what is on my screen");
        assert_eq!((id, phrase.as_deref()), ("no-capture", Some("on my screen")));
    }

    /// Every `.rs` file under `cli/src`, with its path from there.
    fn sources() -> Vec<(String, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        let mut dirs = vec![root.clone()];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let name = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
                    files.push((name, std::fs::read_to_string(&path).unwrap()));
                }
            }
        }
        assert!(files.len() > 10, "{}", files.len());
        files
    }

    /// A screenshot exists only inside an ask, and from the command
    /// line the one ask that may take one is `ask run`.
    ///
    /// Core's capture needs a token only `core::ask` can make, so the
    /// CLI cannot call it at all. What the CLI can do is make a plan
    /// that may capture (`with_vision` with a choice other than
    /// `None`) and send a question through it. This test holds that
    /// to one place: the words that make such a plan are in
    /// `commands/ask.rs`, inside the code of `ask run`, and in no
    /// other file of the CLI; and no file names core's capture entry
    /// point or the pixel test's hook.
    ///
    /// (The words are put together here so this test does not find
    /// itself.)
    #[test]
    fn the_only_way_to_a_capture_from_the_cli_is_ask_run() {
        let join = |a: &str, b: &str| format!("{a}{b}");
        let makes_a_capturing_plan =
            [join("with_", "vision("), join("VisionChoice::", "Force"), join("VisionChoice::", "Settings")];
        let never_named = [join("capture_", "for_ask"), join("probe_", "for_test"), join("AskCapture", "Token")];
        let mut plan_makers = Vec::new();
        for (name, text) in sources() {
            for word in &never_named {
                assert!(!text.contains(word.as_str()), "{name} names {word}");
            }
            if makes_a_capturing_plan.iter().any(|word| text.contains(word.as_str())) {
                plan_makers.push(name);
            }
        }
        assert_eq!(plan_makers, ["commands/ask.rs"], "only `ask run` may make a plan that can capture");

        // In that file: one call, and it is in the function that runs
        // `ask run`.
        let ask = sources().into_iter().find(|(name, _)| name == "commands/ask.rs").unwrap().1;
        let shipped = &ask[..ask.find("#[cfg(test)]\nmod tests").expect("the tests of ask.rs")];
        let call = join("with_", "vision(");
        assert_eq!(shipped.matches(call.as_str()).count(), 1, "one place makes a plan that can capture");
        let at = shipped.find(call.as_str()).unwrap();
        let function = shipped[..at].rfind("\nfn ").map(|i| &shipped[i + 1..at]).expect("a function around the call");
        assert!(
            function.starts_with("fn run_question("),
            "the call is in: {}",
            function.lines().next().unwrap_or_default()
        );

        // And this file, the Vision commands, sends no question at
        // all through a plan: its eval hands its cases to a backend
        // itself, with images it drew.
        let vision = include_str!("vision.rs");
        let shipped = &vision[..vision.find("#[cfg(test)]\nmod tests").expect("the tests of vision.rs")];
        for word in [".send(", "send_and_record(", "AskInput"] {
            assert!(!shipped.contains(word), "vision.rs has `{word}`");
        }
    }

    /// The helper window of the pixel test is not in a build without
    /// its feature: the subcommand, its arguments and its code are all
    /// behind the gate, and no default feature switches it on.
    #[test]
    fn the_test_window_is_behind_its_feature() {
        let vision = include_str!("vision.rs");
        let gate = "#[cfg(feature = \"vision-live-test\")]";
        for item in ["    #[command(hide = true)]\n    TestWindow(", "mod test_window;", "        VisionCommand::TestWindow("] {
            let at = vision.find(item).unwrap_or_else(|| panic!("{item} is gone"));
            assert!(vision[..at].trim_end().ends_with(gate), "`{}` is not behind the feature", item.trim());
        }
        let manifest = include_str!("../../Cargo.toml");
        assert!(manifest.contains("\nvision-live-test = ["), "the feature is declared");
        assert!(!manifest.contains("default = ["), "the CLI has no default features to hide it in");
        // The platform bindings the helper needs come with the
        // feature and not without it.
        for dep in ["objc2 = { version = \"0.6\", optional = true }", "windows = { version = \"0.61\", optional = true"] {
            assert!(manifest.contains(dep), "{dep}");
        }
        assert_eq!(cfg!(feature = "vision-live-test"), false, "this test run has the helper compiled in");
    }
}
