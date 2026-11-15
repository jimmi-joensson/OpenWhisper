//! `openwhisper scenarios <subcommand>`: end-to-end checks of normal
//! use (TASK-103). Where the eval sets (`refine eval`, `ask eval`,
//! `vision eval`, `agent eval`) each judge one thing a model decides,
//! a scenario is a person's situation: settings as a person would
//! have them, a question as they would say it, and a check on what
//! they would see (the answer, a notice, the web search hint, a
//! step, where the ask looked, an error message), plus how long it
//! took.
//!
//! Subcommands:
//! - `list`: every scenario, by group, with the situation and what
//!   the person should see.
//! - `run [--only GROUP_OR_PREFIX] [--offline]`: run them and print
//!   one line per scenario (pass, fail, skipped with the reason, the
//!   time), then the totals. `--json` prints one record instead.
//!   Takes the backend overrides of the eval commands (`--model`,
//!   `--effort`, `--claude-path`, ...).
//!
//! The groups: `everyday` (the weather with and without web search,
//! a web address an ask may not read, asked for and written on the
//! screen, what day it is, which app, which computer, the same with
//! those facts switched off, both as what the app sends and as what
//! is answered, Danish, no long dashes), `spoken` (the system
//! voice speaks the question, the recognizer transcribes it), `app-
//! context` (the connection of the app in front before the screen),
//! `modes` and `failures`.
//!
//! What a run may touch:
//!
//! - **A scratch config folder, and only that.** Every run makes a
//!   folder of its own in the temp folder, writes the settings of
//!   each scenario there and removes it at the end. It refuses to
//!   start when the folder is, is inside or holds a real OpenWhisper
//!   folder ([`env::refuse_real`]). No dictionary and no history is
//!   pointed at, and nothing is recorded in the app's database.
//! - **The model the person has set up**, through the same functions
//!   `ask run`, `agent run` and `refine run --mode` call. A scenario
//!   that could not run because the account is at a limit is
//!   "stopped", not failed, and nothing more is sent.
//! - **One connection: the test fixture** (`agent fake-server`),
//!   started by the Claude CLI with `--strict-mcp-config`. No real
//!   connection is listed or called anywhere in here.
//! - **No real screen.** Every ask of a scenario has a stand-in in
//!   place of the screen: a synthetic image, or a made-up platform
//!   that has no permission, has a protected app in front, or has no
//!   display.
//!
//! `run --offline` asks no model: for every scenario it proves that
//! the check accepts an ideal outcome and rejects each mistake it is
//! there for, like `agent eval --offline`.

use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use openwhisper_core::ask::eval::limit_reached;
use openwhisper_core::refine;
use serde_json::{Value, json};

use super::ask::{self as ask_cmd, Overrides};

mod app_context;
mod asking;
mod checks;
mod env;
mod everyday;
mod failures;
mod modes;
mod report;
mod spoken;

use env::Env;

#[derive(Args, Debug)]
pub struct ScenariosArgs {
    #[command(subcommand)]
    pub command: ScenariosCommand,
}

#[derive(Subcommand, Debug)]
pub enum ScenariosCommand {
    /// List every scenario: the situation and what should be seen.
    List,
    /// Run the scenarios against the real backend, in a scratch
    /// config folder.
    Run(RunArgs),
}

#[derive(Args, Debug)]
pub struct RunArgs {
    /// Run only one group (everyday, spoken, app-context, modes,
    /// failures) or the scenarios whose name starts with this text.
    #[arg(long, value_name = "GROUP_OR_PREFIX")]
    only: Option<String>,
    /// Do not ask a model: only prove that every check accepts a
    /// right outcome and rejects the mistakes it is there for.
    #[arg(long)]
    offline: bool,
    /// The scratch config folder to use, in place of a new one in
    /// the temp folder. It must be empty or not exist yet, and it is
    /// refused when it is a real OpenWhisper folder. It is left in
    /// place after the run.
    #[arg(long, value_name = "DIR")]
    config_dir: Option<std::path::PathBuf>,
    #[command(flatten)]
    overrides: Overrides,
}

/// The groups, in the order they run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Everyday,
    Spoken,
    AppContext,
    Modes,
    Failures,
}

impl Group {
    pub const ALL: [Group; 5] = [Group::Everyday, Group::Spoken, Group::AppContext, Group::Modes, Group::Failures];

    /// Stable id: what `--only` takes, and what every name in the
    /// group starts with.
    pub fn id(self) -> &'static str {
        match self {
            Self::Everyday => "everyday",
            Self::Spoken => "spoken",
            Self::AppContext => "app-context",
            Self::Modes => "modes",
            Self::Failures => "failures",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Everyday => "Everyday questions",
            Self::Spoken => "Spoken questions",
            Self::AppContext => "The app in front: its connection before the screen",
            Self::Modes => "Modes",
            Self::Failures => "When something is missing",
        }
    }
}

/// What the machine the scenarios run on has, as far as a scenario
/// may need to know before it starts.
#[derive(Clone, Copy, Debug, Default)]
pub struct Host {
    /// The backend of this run is an OpenAI-compatible endpoint.
    pub openai: bool,
}

/// One scenario.
pub struct Scenario {
    pub name: &'static str,
    pub group: Group,
    /// The person's situation, in one line.
    pub situation: &'static str,
    /// What they should see.
    pub expect: &'static str,
    /// How long it may take, from the question to the end. Longer is
    /// a failure.
    pub limit_ms: u64,
    /// How long the scenario may spend before the person's clock
    /// starts (the voice speaks, the recognizer loads). Not part of
    /// what is judged; only how long the run waits before it calls
    /// the scenario hanging.
    pub setup_ms: u64,
    /// The scenario asks the real model, so a limit of the account
    /// can stop it.
    pub live_model: bool,
    /// Why it cannot run here; `None` when it can.
    pub skip: fn(&Host) -> Option<String>,
    pub run: fn(&Env) -> Seen,
    pub check: fn(&Seen) -> Result<(), String>,
    /// For the offline self-test.
    pub samples: fn() -> Samples,
}

/// An outcome the check must accept, and outcomes it must reject,
/// each with what went wrong in it.
pub struct Samples {
    pub ideal: Seen,
    pub mistakes: Vec<(&'static str, Seen)>,
}

/// What a person would see of one scenario, and the few things the
/// runner sees for them (the fixture's log, the count of captures,
/// whether the backend's process is gone).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seen {
    /// The question as it went to the model (after the rule pass), or
    /// the dictated text of a mode scenario.
    pub question: String,
    /// What the recognizer heard, in a spoken scenario.
    pub transcript: String,
    /// The answer, or the text a mode made of the dictation. Empty
    /// when there is none.
    pub answer: String,
    /// Why there is no answer. Empty when there is one.
    pub error: String,
    /// What the answer says about how it was made (`AskReply::notice`).
    pub notice: String,
    /// `web-search` when the answer came with the hint that points
    /// to the web search setting. Empty otherwise.
    pub suggestion: String,
    /// The model searched the web, or read live weather.
    pub searched: bool,
    /// The hosts of web addresses the model tried to read and was
    /// refused: nothing was fetched from them.
    pub refused: Vec<String>,
    /// What OpenWhisper itself sent, where a stand-in for the Claude
    /// CLI wrote it down: the system prompt, then the user message.
    /// Empty when the scenario asks the real model.
    pub sent: String,
    /// What became of the screen: `none`, `included`, `blocked`,
    /// `unavailable`.
    pub screen: String,
    /// Why there is no screenshot, as the person is told.
    pub screen_notice: String,
    /// Every tool call as the panel shows it: `tool on connection:
    /// state`.
    pub steps: Vec<String>,
    /// Where the ask looked: `none`, `screen`, `connection:NAME`.
    pub context_source: String,
    /// The tools that reached the fixture, in order (its log).
    pub calls: Vec<String>,
    /// How many times a screenshot left the stand-in for the screen.
    pub captures: usize,
    /// The mode a dictation ran in, and why (`selected`,
    /// `app-default`).
    pub mode: String,
    pub mode_reason: String,
    /// How the Refine run of a mode scenario ended.
    pub status: String,
    /// After a cancel or a timeout: the backend's process is gone.
    /// `None` when the scenario does not look.
    pub process_gone: Option<bool>,
    /// From the cancel to the end of the ask.
    pub cancel_to_end_ms: Option<u64>,
    /// What the check compares the answer with, where that is only
    /// known at run time: today's weekday, the name of the app in
    /// front, what this computer is.
    pub facts: Vec<String>,
    /// What must not show at all, where that is only known at run
    /// time: what only OpenWhisper could have told the model about
    /// this computer, in a scenario where it was told not to.
    pub never: Vec<String>,
    /// From the question to the end.
    pub elapsed_ms: u64,
}

impl Seen {
    /// A scenario that could not even be set up.
    pub fn broken(why: impl Into<String>) -> Self {
        Self { error: format!("the scenario could not be set up: {}", why.into()), ..Self::default() }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "question": self.question,
            "transcript": self.transcript,
            "answer": self.answer,
            "error": self.error,
            "notice": self.notice,
            "suggestion": self.suggestion,
            "searched": self.searched,
            "refused_fetches": self.refused,
            "sent_chars": self.sent.chars().count(),
            "screen": self.screen,
            "screen_notice": self.screen_notice,
            "steps": self.steps,
            "context_source": self.context_source,
            "calls": self.calls,
            "captures": self.captures,
            "mode": self.mode,
            "mode_reason": self.mode_reason,
            "status": self.status,
            "process_gone": self.process_gone,
            "cancel_to_end_ms": self.cancel_to_end_ms,
            "facts": self.facts,
            "never": self.never,
            "elapsed_ms": self.elapsed_ms,
        })
    }
}

/// How one scenario ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Passed,
    Failed,
    /// It cannot run on this machine; the reason says why.
    Skipped,
    /// The account behind the backend is at a limit. Not a failure.
    Stopped,
    /// Not run, because the run stopped before it.
    NotRun,
}

impl Status {
    /// Stable id, for the JSON record.
    pub fn id(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::Stopped => "stopped",
            Self::NotRun => "not-run",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Passed => "PASS",
            Self::Failed => "FAIL",
            Self::Skipped => "SKIP",
            Self::Stopped => "STOP",
            Self::NotRun => "----",
        }
    }
}

/// One row of the report.
#[derive(Clone, Debug)]
pub struct Row {
    pub name: String,
    pub group: Group,
    pub status: Status,
    pub elapsed_ms: u64,
    pub limit_ms: u64,
    pub situation: String,
    pub expect: String,
    /// Why it failed; empty when it passed.
    pub problem: String,
    /// Why it was skipped, stopped or not run.
    pub reason: String,
    /// What was seen. `None` for a scenario that did not run.
    pub seen: Option<Seen>,
}

impl Row {
    pub fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "group": self.group.id(),
            "status": self.status.id(),
            "elapsed_ms": self.elapsed_ms,
            "limit_ms": self.limit_ms,
            "situation": self.situation,
            "expect": self.expect,
            "problem": self.problem,
            "reason": self.reason,
            "seen": self.seen.as_ref().map(Seen::to_json),
        })
    }

    fn without_a_run(scenario: &Scenario, status: Status, reason: String) -> Self {
        Self {
            name: scenario.name.to_string(),
            group: scenario.group,
            status,
            elapsed_ms: 0,
            limit_ms: scenario.limit_ms,
            situation: scenario.situation.to_string(),
            expect: scenario.expect.to_string(),
            problem: String::new(),
            reason,
            seen: None,
        }
    }
}

/// Every scenario, in the order they run.
pub fn all() -> Vec<Scenario> {
    let mut list = everyday::scenarios();
    list.extend(spoken::scenarios());
    list.extend(app_context::scenarios());
    list.extend(modes::scenarios());
    list.extend(failures::scenarios());
    list
}

/// `--only`, tidied: lower case, with spaces and underscores as
/// hyphens, so "app context" and "app_context" name the group too.
fn tidy(only: &str) -> String {
    only.trim().to_lowercase().replace([' ', '_'], "-")
}

/// The scenarios `only` names: a whole group, or every scenario whose
/// name starts with it. Every name starts with the id of its group.
fn chosen(only: Option<&str>) -> Result<Vec<Scenario>> {
    let Some(only) = only.map(tidy).filter(|o| !o.is_empty()) else {
        return Ok(all());
    };
    let list: Vec<Scenario> =
        all().into_iter().filter(|s| s.group.id() == only || s.name.starts_with(&only)).collect();
    if list.is_empty() {
        bail!(
            "no scenario starts with \"{only}\"; the groups are: {}; the scenarios are: {}",
            Group::ALL.map(Group::id).join(", "),
            all().iter().map(|s| s.name).collect::<Vec<_>>().join(", ")
        );
    }
    Ok(list)
}

/// The verdict on what was seen: the time limit first, then the
/// scenario's own check.
pub fn judge(scenario: &Scenario, seen: &Seen) -> Result<(), String> {
    if seen.elapsed_ms > scenario.limit_ms {
        return Err(format!("it took {} ms, and the limit is {} ms", seen.elapsed_ms, scenario.limit_ms));
    }
    (scenario.check)(seen)
}

pub fn run(args: ScenariosArgs, json: bool) -> Result<()> {
    match args.command {
        ScenariosCommand::List => report::list(&all(), json),
        ScenariosCommand::Run(a) => run_them(a, json),
    }
}

/// How much longer than its limit a scenario is waited for before it
/// counts as hanging.
const HANG_GRACE: Duration = Duration::from_secs(20);

fn run_them(args: RunArgs, json: bool) -> Result<()> {
    // Before anything else, also for a run that asks no model: a
    // folder that is a real one is never accepted.
    if let Some(dir) = &args.config_dir {
        env::refuse_real(dir, &env::real_config_dirs()).map_err(anyhow::Error::msg)?;
    }
    let scenarios = chosen(args.only.as_deref())?;
    if args.offline {
        return report::self_test(&self_test(&scenarios), json);
    }

    let env = Arc::new(Env::make(args.config_dir.as_deref(), args.overrides.clone()).map_err(anyhow::Error::msg)?);
    let (ask, refine_settings) = env.ask_and_refine().map_err(anyhow::Error::msg)?;
    let host = Host { openai: refine_settings.backend == openwhisper_core::settings::RefineBackendKind::OpenaiCompatible };
    let runnable = |s: &Scenario| (s.skip)(&host).is_none();
    if scenarios.iter().any(|s| s.live_model && runnable(s)) {
        let status = refine::backend_status(&refine_settings);
        if !status.ready {
            env.remove();
            bail!("backend {} is not ready: {}", status.backend, status.message);
        }
    }
    let plan = openwhisper_core::ask::AskPlan::with_terms(&ask, &refine_settings, &[]);
    if !json {
        if scenarios.iter().any(|s| s.live_model && runnable(s)) {
            eprintln!("questions go to: {}", plan.destination());
        } else {
            eprintln!("no scenario of this run asks a model");
        }
        eprintln!("settings, and nothing else of OpenWhisper, live in: {}", env.dir.display());
        eprintln!("the only connection is a test fixture, and the screen is a stand-in: no real one is used");
    }

    let started = Instant::now();
    let mut rows: Vec<Row> = Vec::new();
    let mut stopped: Option<report::Stopped> = None;
    for (at, scenario) in scenarios.iter().enumerate() {
        if let Some(stop) = &stopped {
            let row = Row::without_a_run(scenario, Status::NotRun, format!("not run: the run stopped at {}", stop.scenario));
            if !json {
                println!("{}", report::line(&row));
            }
            rows.push(row);
            continue;
        }
        if let Some(reason) = (scenario.skip)(&host) {
            let row = Row::without_a_run(scenario, Status::Skipped, reason);
            if !json {
                println!("{}", report::line(&row));
            }
            rows.push(row);
            continue;
        }
        let seen = run_one(scenario, &env);
        let mut row = Row {
            elapsed_ms: seen.as_ref().map_or(scenario.limit_ms, |s| s.elapsed_ms),
            ..Row::without_a_run(scenario, Status::Passed, String::new())
        };
        match seen {
            // An account limit: not a wrong outcome, and every
            // further question would be refused the same way.
            Some(seen) if scenario.live_model && limit_reached(&seen.error) => {
                row.status = Status::Stopped;
                row.reason = seen.error.clone();
                stopped = Some(report::Stopped {
                    scenario: scenario.name.to_string(),
                    reason: seen.error.clone(),
                    not_run: scenarios.len() - at,
                });
                row.seen = Some(seen);
            }
            Some(seen) => {
                if let Err(problem) = judge(scenario, &seen) {
                    row.status = Status::Failed;
                    row.problem = problem;
                }
                row.seen = Some(seen);
            }
            None => {
                row.status = Status::Failed;
                row.problem = format!(
                    "it hangs: nothing came back within {} ms (the limit is {} ms)",
                    waited_for(scenario).as_millis(),
                    scenario.limit_ms
                );
            }
        }
        if !json {
            println!("{}", report::line(&row));
            for detail in report::details(&row) {
                println!("{detail}");
            }
        }
        rows.push(row);
    }
    env.remove();

    let summary = report::Summary::of(&rows, started.elapsed().as_millis() as u64);
    if json {
        let value = json!({
            "mode": "live",
            "backend": refine_settings.backend.as_str(),
            "model": plan.setup.model,
            "effort": plan.effort_id(),
            "passed": summary.passed,
            "failed": summary.failed,
            "skipped": summary.skipped,
            "not_run": summary.not_run,
            "total": rows.len(),
            "elapsed_ms": summary.elapsed_ms,
            "stopped": stopped.as_ref().map(report::Stopped::to_json),
            "results": rows.iter().map(Row::to_json).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!();
        println!("{}", summary.line());
        if let Some(stop) = &stopped {
            println!("{}", stop.summary());
        }
    }
    if stopped.is_some() {
        std::process::exit(ask_cmd::STOPPED_EXIT);
    }
    if summary.failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}

/// Run one scenario on a thread of its own and wait for it no longer
/// than its limit and a grace on top. `None` = it hangs. The thread
/// is then left behind: the run goes on without it.
fn run_one(scenario: &Scenario, env: &Arc<Env>) -> Option<Seen> {
    let run = scenario.run;
    let env = env.clone();
    within(waited_for(scenario), move || run(&env))
}

/// How long a scenario is waited for before it counts as hanging.
fn waited_for(scenario: &Scenario) -> Duration {
    Duration::from_millis(scenario.setup_ms + scenario.limit_ms) + HANG_GRACE
}

/// What `work` comes to, when it comes to anything within `limit`.
fn within<T: Send + 'static>(limit: Duration, work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    rx.recv_timeout(limit).ok()
}

/// One line of the offline self-test.
#[derive(Clone, Debug)]
pub struct SelfCheck {
    pub scenario: String,
    /// What was tried: "accepts an ideal outcome", or "rejects: " and
    /// the mistake.
    pub what: String,
    pub passed: bool,
    /// Why the check itself is wrong; empty when it behaves.
    pub problem: String,
}

/// For every scenario: its check accepts an ideal outcome, rejects
/// each mistake it is there for, and rejects an ideal outcome that
/// took longer than the limit. No model is asked and nothing runs.
pub fn self_test(scenarios: &[Scenario]) -> Vec<SelfCheck> {
    let mut checks = Vec::new();
    for scenario in scenarios {
        let samples = (scenario.samples)();
        let mut add = |what: String, verdict: Result<(), String>, must_pass: bool| {
            let problem = match (&verdict, must_pass) {
                (Ok(()), true) | (Err(_), false) => String::new(),
                (Err(why), true) => format!("a right outcome was rejected: {why}"),
                (Ok(()), false) => "the mistake was accepted".to_string(),
            };
            checks.push(SelfCheck { scenario: scenario.name.to_string(), what, passed: problem.is_empty(), problem });
        };
        add("accepts an ideal outcome".to_string(), judge(scenario, &samples.ideal), true);
        let slow = Seen { elapsed_ms: scenario.limit_ms + 1, ..samples.ideal.clone() };
        add("rejects: over the time limit".to_string(), judge(scenario, &slow), false);
        for (mistake, seen) in &samples.mistakes {
            add(format!("rejects: {mistake}"), judge(scenario, seen), false);
        }
    }
    checks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_starts_with_its_group_and_is_there_once() {
        let list = all();
        let mut names: Vec<&str> = list.iter().map(|s| s.name).collect();
        for scenario in &list {
            assert!(
                scenario.name.starts_with(&format!("{}-", scenario.group.id())),
                "{} is in the group {}",
                scenario.name,
                scenario.group.id()
            );
            assert!(!scenario.situation.is_empty() && !scenario.expect.is_empty(), "{}", scenario.name);
            assert!(scenario.limit_ms >= 1_000, "{}", scenario.name);
        }
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), list.len(), "a name is there twice");
        // Every group of the plan has scenarios.
        for group in Group::ALL {
            assert!(list.iter().any(|s| s.group == group), "{} is empty", group.id());
        }
    }

    #[test]
    fn only_takes_a_group_or_a_prefix() {
        let names = |only: &str| chosen(Some(only)).unwrap().iter().map(|s| s.name).collect::<Vec<_>>();
        assert!(names("modes").iter().all(|n| n.starts_with("modes-")));
        assert_eq!(names("app context"), names("app-context"));
        assert_eq!(names("App_Context"), names("app-context"));
        assert_eq!(names("everyday-weather").len(), 2);
        assert_eq!(chosen(None).unwrap().len(), all().len());
        assert_eq!(chosen(Some("  ")).unwrap().len(), all().len());
        let none = chosen(Some("no-such")).err().unwrap().to_string();
        assert!(none.contains("no scenario starts with \"no-such\""), "{none}");
        assert!(none.contains("everyday, spoken, app-context, modes, failures"), "{none}");
    }

    #[test]
    fn every_check_accepts_its_ideal_and_rejects_its_mistakes() {
        let checks = self_test(&all());
        let wrong: Vec<String> =
            checks.iter().filter(|c| !c.passed).map(|c| format!("{} {}: {}", c.scenario, c.what, c.problem)).collect();
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
        // Every scenario has mistakes of its own on top of the time
        // limit.
        for scenario in all() {
            let own = checks.iter().filter(|c| c.scenario == scenario.name).count();
            assert!(own >= 4, "{} has only {own} checks", scenario.name);
        }
    }

    #[test]
    fn the_time_limit_is_judged_before_the_check() {
        let scenario = &all()[0];
        let ideal = (scenario.samples)().ideal;
        assert_eq!(judge(scenario, &ideal), Ok(()));
        let slow = Seen { elapsed_ms: scenario.limit_ms + 1, ..ideal };
        let problem = judge(scenario, &slow).unwrap_err();
        assert!(problem.contains("and the limit is"), "{problem}");
    }

    #[test]
    fn work_that_hangs_is_given_up_on() {
        assert_eq!(within(Duration::from_secs(5), || 7), Some(7));
        let started = Instant::now();
        let hung = within(Duration::from_millis(150), || {
            std::thread::sleep(Duration::from_secs(3));
            7
        });
        assert_eq!(hung, None);
        assert!(started.elapsed() < Duration::from_secs(2), "it waited for the work to end");
    }

    #[test]
    fn nothing_in_here_has_a_long_dash() {
        for scenario in all() {
            for text in [scenario.name, scenario.situation, scenario.expect] {
                assert!(!text.contains(['\u{2014}', '\u{2013}']), "{text}");
            }
        }
    }
}
