//! The backend of an agent ask: the person's Claude Code CLI, started
//! with their enabled connections and nothing else.
//!
//! The process, the question on stdin, the streaming, cancel and the
//! timeout are the plain ask's ([`crate::ask::claude_cli`]): one
//! machinery, shared. This file brings the two things that differ.
//!
//! **The arguments** ([`spawn_args`]), every one of them load-bearing
//! and pinned by a test:
//!
//! | flag | why |
//! |------|-----|
//! | `--permission-mode default` | The CLI's own default is a per-machine setting; on a machine where it is `auto`, a tool that is on no list would run without asking. |
//! | `--setting-sources ""` | None of the person's Claude Code settings, hooks, plugins or permission rules. Not `--safe-mode`: that drops every connection too. |
//! | `--strict-mcp-config`, `--mcp-config FILE` | Only the connections in the run's private file ([`super::connections::write_run_config`]) exist. |
//! | `--tools ""` | No built-in tool: no shell, no file access, no web fetch. With web search on for asks, exactly `WebSearch,WebFetch`, as in a plain ask. |
//! | `--allowedTools` | The ticked read and draft tools: they run without a question. With web search on, also the search and the fetch tool for the two weather hosts ([`crate::ask::weather`]), as in a plain ask. |
//! | `--disallowedTools` | Every other tool the connections are known to have: taken out of what the model sees. |
//! | `--permission-prompt-tool stdio` | A tool on neither list makes the CLI ask this process and wait. That is the confirm. |
//! | `--no-session-persistence` | Nothing of the conversation is written to disk, a screenshot included. |
//! | `--disable-slash-commands` | No skills, no commands. |
//! | `--max-turns N` | The model cannot go round its tools forever. |
//!
//! The lists are handed over as one comma-separated argument each,
//! and every flag that takes a list is followed by another flag, so
//! nothing after it can be read as one more tool. The question never
//! rides on the command line: it goes in on stdin.
//!
//! **The names on the lists are checked twice.** A tool's name comes
//! from its connection, and a name with a comma in it would be read
//! as two entries. Such a tool never gets into the plan
//! ([`super::policy::unusable_reason`]); and should a name that is
//! not made of plain characters reach [`spawn_args`] all the same,
//! there are no arguments: the ask ends with an error before a
//! process exists. The entries for the web are not names from a
//! connection: they are fixed at compile time
//! ([`claude_cli::web_allow_rules`]), the fetch tool's with a host in
//! brackets, and are put on the list after that check, so the check
//! stays as strict as it is for every name that comes from outside.
//!
//! **A fetch of another address is never asked about.** With web
//! search on the model has the fetch tool, allowed for the weather
//! hosts only. The CLI checks with this process before every fetch
//! ([`claude_cli::WebTools::hello`]), and for any other address the
//! answer is no ([`AgentReader::before_fetch`]). Such an address is
//! on no list either, so a CLI that did not check would ask whether
//! the tool may run, and that answer is no too, at once, without a
//! confirm: the person is never asked to let an ask read an address
//! ([`AgentReader::can_use_tool`]). Either way it shows as a denied
//! step. Should the result of such a fetch come back as anything but
//! a refusal, the ask ends there, as a plain ask does
//! ([`claude_cli::WebTools`]).
//!
//! **The plan is checked against what the CLI really started with**
//! ([`AgentReader::init`]). The first thing the CLI prints for a
//! question is the list of tools it has. For every connection that
//! is up, each tool the plan lets the model call must be on it under
//! the name this app worked out, and no tool the plan took away may
//! be on it. When that does not hold, this app and the CLI disagree
//! about a name, which means the two lists did not do what they were
//! made for: the ask ends there with an error that names the
//! connection, before the model can call anything. (A tool the
//! connection has grown since it was last listed is a different
//! thing: it is on no list, so the CLI asks about it and the answer
//! is no.)
//!
//! **What the lines mean** ([`AgentReader`]). On top of the answer's
//! text:
//!
//! - an `assistant` line with a tool call: a step starts (connection,
//!   tool, a line of its input);
//! - a `user` line with a tool result: the step ends, done or failed,
//!   with a line of the result;
//! - a `system` line `permission_denied`: the CLI refused the call by
//!   itself; the step is denied;
//! - a `control_request` `can_use_tool`: the CLI asks whether a tool
//!   may run, and waits. For a tool that is on the confirm list the
//!   person is asked ([`crate::ask::AskEvent::Confirm`]) and the
//!   answer is theirs; a yes hands the input back exactly as it
//!   came. The confirm shows the whole input; an input too long for
//!   that is refused without asking. For any other tool the answer
//!   is no, at once, without bothering the person: the model was not
//!   supposed to have it.
//!
//! Everything a step or a confirm shows (names, the input, what came
//! back) is made safe to show first ([`steps::safe_to_show`]).
//!
//! Measured with Claude Code 2.1.322, Sonnet at low effort and a
//! local test connection (TASK-99 notes): about 3.5 s for an ask with
//! one tool call, 4 to 4.5 s with two.

use std::collections::{HashMap, HashSet};
use std::io::IsTerminal;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use super::policy::{self, ToolKind};
use super::{AgentRun, connections};
use crate::ask::claude_cli::{self, Line, Permission, Process, Reader, Verdict, WebTools};
use crate::ask::steps::{self, AskConfirm, AskStep, ConfirmOutcome, StepState};
use crate::ask::{AskBackend, AskError, AskEvent, AskInput, AskReply, AskSetup, CancelFlag, weather};
use crate::settings::{ClaudeCliSettings, RefineEffort};

/// What the model is told when it reaches for a tool this ask does
/// not have.
pub const NOT_AVAILABLE: &str = "This tool is not available in this ask, so it was not run. Do not try it again or \
in another way. Tell the person that it was not done.";

/// What the model is told when the input of a call is too long to
/// be shown in a confirm.
pub const TOO_LARGE_TO_CONFIRM: &str = "The input of this call is too large to be shown to the person for \
confirmation, so it was not run. Do not try it again as it is. Tell the person that it was not done, and why.";

/// The Claude Code CLI as the [`AskBackend`] of an agent ask.
pub struct ClaudeAgent {
    /// Only [`ClaudeCliSettings::path`] is used.
    settings: ClaudeCliSettings,
    run: AgentRun,
    process: Option<Process>,
    /// Whether the CLI gets a process group of its own. `None`: it
    /// does unless this process sits at a terminal
    /// ([`at_a_terminal`]).
    own_group: Option<bool>,
}

/// This process has a terminal on its input, its output or its error
/// output: it was started by a person at a shell (`openwhisper agent
/// run`). Ctrl-C there goes to every process of the terminal's
/// group, and that is how the CLI under this one is stopped; in a
/// group of its own it would not hear it. The desktop app has no
/// terminal.
fn at_a_terminal() -> bool {
    std::io::stdin().is_terminal() || std::io::stdout().is_terminal() || std::io::stderr().is_terminal()
}

impl ClaudeAgent {
    pub fn new(settings: ClaudeCliSettings, run: AgentRun) -> Self {
        Self { settings, run, process: None, own_group: None }
    }

    /// Say whether the CLI gets a process group of its own, instead
    /// of leaving it to whether this process sits at a terminal.
    #[cfg(test)]
    pub(crate) fn in_own_group(mut self, yes: bool) -> Self {
        self.own_group = Some(yes);
        self
    }

    fn spawn(&self, setup: &AskSetup) -> Result<Process, AskError> {
        let bin = claude_cli::find_binary(&self.settings)?;
        // First: without its connections the run does not start.
        let config = connections::write_run_config(&self.run.connections)?;
        let reader = Box::new(AgentReader::new(&self.run, setup.web_search));
        let config_path = config.path().to_path_buf();
        Process::spawn(
            &bin,
            setup,
            |prompt_file| spawn_args(setup, &self.run, &config_path, prompt_file),
            reader,
            // The file lives as long as the process and goes with it.
            Some(Box::new(config)),
            // On cancel or a timeout, the local servers of the
            // connections go with the CLI (unix).
            self.own_group.unwrap_or_else(|| !at_a_terminal()),
        )
    }
}

impl AskBackend for ClaudeAgent {
    fn id(&self) -> &'static str {
        claude_cli::ID
    }

    fn warm(&mut self, setup: &AskSetup) {
        // A failed spawn is reported by `send`, which tries again.
        self.process = self.spawn(setup).ok();
    }

    fn send(
        &mut self,
        setup: &AskSetup,
        input: &AskInput,
        sink: &mut dyn FnMut(AskEvent),
        cancel: &CancelFlag,
        timeout: Duration,
    ) -> Result<AskReply, AskError> {
        let mut slot = self.process.take();
        let reply = claude_cli::send_through(&mut slot, setup, || self.spawn(setup), input, sink, cancel, timeout);
        self.process = slot;
        reply
    }
}

/// Every entry of a list of tool names is made of plain characters
/// only (ASCII letters, digits, `_`, `-`): nothing in it can end the
/// entry, start another, or mean something to the CLI's rules (a
/// comma, a space, `*`, a bracket). The plan never holds anything
/// else; this is the last look before the names leave the process.
fn check_list(flag: &str, entries: &[String]) -> Result<(), AskError> {
    let plain = |entry: &&String| !entry.is_empty() && entry.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    match entries.iter().find(|entry| !plain(entry)) {
        None => Ok(()),
        Some(entry) => Err(AskError::Backend(format!(
            "A tool name that cannot be handed to Claude Code safely was about to be ({flag}: \"{}\"), so the ask was \
stopped before it started. Refresh the connections in the Agent settings and ask again.",
            steps::one_line(entry, 80)
        ))),
    }
}

/// The arguments of one agent ask. See the module docs for what each
/// is there for. An error when a name on one of the two lists is not
/// made of plain characters ([`check_list`]): then nothing is
/// started.
pub fn spawn_args(
    setup: &AskSetup,
    run: &AgentRun,
    config_file: &Path,
    system_prompt_file: &Path,
) -> Result<Vec<String>, AskError> {
    // The Claude CLI has no "minimal" level.
    let effort = match setup.effort {
        None | Some(RefineEffort::Minimal) => RefineEffort::Low,
        Some(effort) => effort,
    };
    // Every name that comes from a connection is looked at first.
    check_list("--allowedTools", &run.tools.allowed)?;
    check_list("--disallowedTools", &run.tools.disallowed)?;
    // Then the web, when it is on: entries of this app's own, fixed
    // at compile time. The fetch tool's carry a host in brackets,
    // which the check above would refuse, and rightly so for a name
    // from outside; they never pass through it and nothing else is
    // ever added here.
    let mut allowed = if setup.web_search { claude_cli::web_allow_rules() } else { Vec::new() };
    allowed.extend(run.tools.allowed.iter().cloned());
    let web_tools = if setup.web_search { claude_cli::web_tools() } else { String::new() };
    let mut args: Vec<String> = [
        "-p",
        "--model",
        setup.model.as_str(),
        "--effort",
        effort.as_str(),
        "--setting-sources",
        "",
        "--tools",
        web_tools.as_str(),
        "--strict-mcp-config",
        "--mcp-config",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    args.push(config_file.display().to_string());
    // A list is left out when it is empty: a flag that takes a list
    // must not swallow the flag after it.
    if !allowed.is_empty() {
        args.push("--allowedTools".into());
        args.push(allowed.join(","));
    }
    if !run.tools.disallowed.is_empty() {
        args.push("--disallowedTools".into());
        args.push(run.tools.disallowed.join(","));
    }
    args.extend(
        [
            "--permission-mode",
            "default",
            "--permission-prompt-tool",
            "stdio",
            "--disable-slash-commands",
            "--no-session-persistence",
            "--max-turns",
            run.max_turns.to_string().as_str(),
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            "--system-prompt-file",
            system_prompt_file.display().to_string().as_str(),
        ]
        .iter()
        .map(|s| (*s).to_string()),
    );
    Ok(args)
}

/// What the lines of an agent ask mean, and the steps so far.
struct AgentReader {
    /// The built-in web tools, read as in a plain ask.
    web: WebTools,
    confirm_timeout_s: u64,
    max_turns: u32,
    /// The tools the model can call, by full name.
    tools: HashMap<String, policy::PlannedTool>,
    /// Connection names by the way the CLI spells them in a tool's
    /// full name.
    connections: HashMap<String, String>,
    /// The connections of the run, by name, in order.
    names: Vec<String>,
    /// Every tool the plan lets the model call (the allowed ones and
    /// the ones that ask first), and every tool it took away, with
    /// the connection each belongs to.
    visible: Vec<(String, String)>,
    hidden: Vec<(String, String)>,
    /// Connections that were still coming up when the CLI said what
    /// it started with, and have not been heard of since.
    pending: Vec<String>,
    /// Every step so far, by its id.
    steps: HashMap<String, AskStep>,
    /// The step a confirm is about, by the confirm's request id.
    confirms: HashMap<String, String>,
}

/// Names for a sentence: `"a"`, or `"a", "b"`. Safe to show.
fn quoted(names: &[String]) -> String {
    names.iter().map(|name| format!("\"{}\"", steps::one_line(name, 80))).collect::<Vec<_>>().join(", ")
}

impl AgentReader {
    fn new(run: &AgentRun, web_search: bool) -> Self {
        let allowed: HashSet<&String> = run.tools.allowed.iter().chain(&run.tools.confirm).collect();
        let disallowed: HashSet<&String> = run.tools.disallowed.iter().collect();
        let (mut visible, mut hidden) = (Vec::new(), Vec::new());
        for connection in &run.connections {
            for tool in &connection.tools {
                let full = policy::full_tool_name(&connection.name, &tool.name);
                if allowed.contains(&full) {
                    visible.push((full, connection.name.clone()));
                } else if disallowed.contains(&full) {
                    hidden.push((full, connection.name.clone()));
                }
            }
        }
        Self {
            web: WebTools::new(web_search),
            confirm_timeout_s: run.confirm_timeout_s,
            max_turns: run.max_turns,
            tools: run.planned.iter().map(|t| (t.full_name.clone(), t.clone())).collect(),
            connections: run.connections.iter().map(|c| (policy::server_key(&c.name), c.name.clone())).collect(),
            names: run.connections.iter().map(|c| c.name.clone()).collect(),
            visible,
            hidden,
            pending: Vec::new(),
            steps: HashMap::new(),
            confirms: HashMap::new(),
        }
    }

    /// A new step for a call of `full_name`. A tool the plan knows
    /// brings its connection, title and kind; any other is taken
    /// apart by its name and counts as an act.
    fn step(&self, id: &str, full_name: &str, display_name: Option<&str>, input: &Value, state: StepState) -> AskStep {
        let shown = display_name.map(str::trim).filter(|d| !d.is_empty());
        let (connection, tool, title, kind) = match self.tools.get(full_name) {
            Some(planned) => {
                // The plan's title is the tool's own name when the
                // connection gave none; the CLI's is nicer then.
                let title = if planned.title == planned.tool { shown.unwrap_or(&planned.title) } else { &planned.title };
                (planned.connection.clone(), planned.tool.clone(), title.to_string(), planned.kind)
            }
            // The fetch tool is the CLI's own and belongs to no
            // connection: it reads the web.
            None if full_name == weather::FETCH_TOOL => {
                ("web".to_string(), full_name.to_string(), shown.unwrap_or(full_name).to_string(), ToolKind::Act)
            }
            None => {
                let (server, tool) = full_name.strip_prefix("mcp__").and_then(|rest| rest.split_once("__")).unwrap_or(("", full_name));
                let connection = self.connections.get(server).cloned().unwrap_or_else(|| server.to_string());
                (connection, tool.to_string(), shown.unwrap_or(tool).to_string(), ToolKind::Act)
            }
        };
        // None of the three names is ours: each is made safe to
        // show, on one line.
        let shown = |text: &str| steps::one_line(text, steps::STEP_LINE_MAX_CHARS);
        AskStep {
            id: id.to_string(),
            connection: shown(&connection),
            tool: shown(&tool),
            title: shown(&title),
            kind,
            input_summary: steps::input_summary(input),
            state,
            outcome: String::new(),
        }
    }

    /// Keep `step` and make the event for it.
    fn record(&mut self, step: AskStep) -> AskEvent {
        self.steps.insert(step.id.clone(), step.clone());
        AskEvent::Step(step)
    }

    /// The step `id` ends in `state` with `outcome`. `None` when
    /// there is no such step, or it already ended as denied: what
    /// the CLI reports after a no changes nothing about it.
    fn end(&mut self, id: &str, state: StepState, outcome: &str) -> Option<AskEvent> {
        let step = self.steps.get_mut(id)?;
        if step.state == StepState::Denied {
            return None;
        }
        step.state = state;
        step.outcome = steps::one_line(outcome, steps::STEP_LINE_MAX_CHARS);
        Some(AskEvent::Step(step.clone()))
    }

    /// The finished message: tool calls are complete here, with
    /// their input.
    fn assistant(&mut self, v: &Value) -> Result<Line, AskError> {
        let mut events = Vec::new();
        for block in v["message"]["content"].as_array().into_iter().flatten() {
            let name = block["name"].as_str().unwrap_or("");
            if block["type"].as_str() == Some("tool_use") && name.starts_with("mcp__") {
                let id = block["id"].as_str().unwrap_or(name);
                // A confirm for this call may have come first.
                if self.steps.contains_key(id) {
                    continue;
                }
                let display = v["tool_use_meta"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|meta| meta["id"].as_str() == Some(id))
                    .and_then(|meta| meta["display_name"].as_str());
                let step = self.step(id, name, display, &block["input"], StepState::Running);
                // A connection whose tool is called is up.
                if let Some(planned) = self.tools.get(name) {
                    self.pending.retain(|pending| *pending != planned.connection);
                }
                events.push(self.record(step));
            } else {
                // The web search, or a read of the weather: shown as
                // in a plain ask. Any other built-in tool ends the
                // ask here.
                events.extend(self.web.called(block)?);
            }
        }
        Ok(if events.is_empty() { Line::Other } else { Line::Events(events) })
    }

    /// A tool's answer, fed back to the model. An error when a fetch
    /// of an address this ask may not reach came back with content
    /// ([`WebTools::came_back`]).
    fn tool_results(&mut self, v: &Value) -> Result<Line, AskError> {
        let mut events = Vec::new();
        for block in v["message"]["content"].as_array().into_iter().flatten() {
            if block["type"].as_str() != Some("tool_result") {
                continue;
            }
            let id = block["tool_use_id"].as_str().unwrap_or("");
            if !self.steps.contains_key(id) {
                // Not a step: the web search or the weather coming
                // back.
                events.push(self.web.came_back(block)?);
                continue;
            }
            let failed = block["is_error"].as_bool().unwrap_or(false);
            // The CLI says when a call never ran (it was refused by
            // a rule): that is a denied step, not a failed tool.
            let not_run = v["tool_result_meta"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|meta| meta["id"].as_str() == Some(id) && !meta["non_execution_kind"].is_null());
            let state = match (failed, not_run) {
                (true, true) => StepState::Denied,
                (true, false) => StepState::Failed,
                (false, _) => StepState::Done,
            };
            events.extend(self.end(id, state, &result_text(&block["content"])));
        }
        Ok(if events.is_empty() { Line::Other } else { Line::Events(events) })
    }

    /// The CLI refused a call by itself.
    fn denied_by_cli(&mut self, v: &Value) -> Line {
        let name = v["tool_name"].as_str().unwrap_or("");
        let id = v["tool_use_id"].as_str().unwrap_or(name);
        if !self.steps.contains_key(id) {
            let step = self.step(id, name, None, &Value::Null, StepState::Running);
            self.steps.insert(id.to_string(), step);
        }
        match self.end(id, StepState::Denied, "Not available in this ask.") {
            Some(event) => Line::Events(vec![event]),
            None => Line::Other,
        }
    }

    /// The process is up and says what it started with: its
    /// connections with their state, and its tools.
    ///
    /// This is where the plan is held against the CLI (see the
    /// module docs). For every connection of the run that is up:
    ///
    /// - a tool the plan lets the model call is not among the CLI's
    ///   tools under the name this app worked out: the names do not
    ///   agree, so the list that allows it allowed nothing, or
    ///   something else;
    /// - a tool the plan took away is among them: the list that
    ///   takes tools away did not take it.
    ///
    /// Either one ends the ask here, with the connection named,
    /// before the model has called anything. So does a line that
    /// does not list the tools at all, and a tool of a connection
    /// that is not one of this run's.
    ///
    /// Not an error, and said in a notice: a connection that is down
    /// (the answer is made without it), and a tool that is on none
    /// of the plan's lists because the connection has it only since
    /// it was last listed. Such a tool is not allowed anywhere, so
    /// the CLI asks before it runs, and that is answered with a no
    /// ([`AgentReader::can_use_tool`]). A connection that is still
    /// coming up is remembered and said at the end, should nothing
    /// have been heard of it by then ([`Reader::final_notice`]).
    fn init(&mut self, v: &Value) -> Result<Line, AskError> {
        let stopped = |why: String| {
            Err(AskError::Backend(format!(
                "{why}, so the ask was stopped before anything ran. Refresh the connections in the Agent settings and ask again."
            )))
        };
        let Some(tools) = v["tools"].as_array() else {
            return stopped("Claude Code did not say which tools it started with".to_string());
        };
        let started: HashSet<&str> = tools.iter().filter_map(Value::as_str).filter(|name| name.starts_with("mcp__")).collect();
        let state_of = |name: &str| {
            v["mcp_servers"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|server| server["name"].as_str() == Some(name))
                .and_then(|server| server["status"].as_str())
        };
        let (mut down, mut pending) = (Vec::new(), Vec::new());
        for name in &self.names {
            match state_of(name) {
                Some("connected") => {
                    let of_this = |list: &[(String, String)], want_started: bool| -> Vec<String> {
                        list.iter()
                            .filter(|(full, connection)| connection == name && started.contains(full.as_str()) == want_started)
                            .map(|(full, _)| full.clone())
                            .collect()
                    };
                    let missing = of_this(&self.visible, false);
                    let offered = of_this(&self.hidden, true);
                    if let Some(tool) = missing.first() {
                        return stopped(format!(
                            "The connection {} does not have a tool under the name this app expected ({})",
                            quoted(std::slice::from_ref(name)),
                            steps::one_line(tool, 100)
                        ));
                    }
                    if let Some(tool) = offered.first() {
                        return stopped(format!(
                            "The connection {} still offered a tool that was taken out of this ask ({})",
                            quoted(std::slice::from_ref(name)),
                            steps::one_line(tool, 100)
                        ));
                    }
                }
                Some("pending") => pending.push(name.clone()),
                // Failed, needs a sign-in, switched off, or not
                // started at all.
                _ => down.push(name.clone()),
            }
        }
        // What the CLI has that the plan has never heard of.
        let known: HashSet<&str> = self.visible.iter().chain(&self.hidden).map(|(full, _)| full.as_str()).collect();
        let mut grown: Vec<String> = Vec::new();
        for tool in started.iter().filter(|tool| !known.contains(*tool)) {
            let owner = self.connections.iter().find(|(key, _)| tool.starts_with(&format!("mcp__{key}__")));
            match owner {
                Some((_, name)) if !grown.contains(name) => grown.push(name.clone()),
                Some(_) => {}
                None => {
                    return stopped(format!(
                        "Claude Code started with a tool of a connection this ask did not ask for ({})",
                        steps::one_line(tool, 100)
                    ));
                }
            }
        }
        grown.sort();
        self.pending = pending;
        let notice = match (down.as_slice(), grown.as_slice()) {
            ([], []) => return Ok(Line::Other),
            ([one], _) => {
                format!("The connection {} could not be reached, so this answer was made without it.", quoted(std::slice::from_ref(one)))
            }
            ([], grown) => format!(
                "{} {} tools this app has not seen yet. They cannot be used until you refresh the connections in the Agent settings.",
                quoted(grown),
                if grown.len() == 1 { "has" } else { "have" }
            ),
            (many, _) => format!(
                "{} connections could not be reached ({}), so this answer was made without them.",
                many.len(),
                quoted(many)
            ),
        };
        Ok(Line::Notice(notice))
    }

    /// The CLI asks whether a tool may run.
    fn permission(&mut self, v: &Value) -> Line {
        let Some(request_id) = v["request_id"].as_str().filter(|id| !id.is_empty()) else {
            // Nothing to answer to.
            return Line::Other;
        };
        let request = &v["request"];
        let verdict = match request["subtype"].as_str() {
            Some("can_use_tool") => self.can_use_tool(request_id, request),
            Some("hook_callback") => self.before_fetch(request_id, request),
            _ => Verdict::Unsupported,
        };
        Line::Permission(Permission { request_id: request_id.to_string(), verdict })
    }

    /// The CLI checks before a fetch, as this app asked it to
    /// ([`WebTools::hello`]). A weather host is left to the allow
    /// list; any other address is a no, and shows as a denied step.
    /// A check this app never asked for is not answered with a yes
    /// or a no at all.
    fn before_fetch(&mut self, request_id: &str, request: &Value) -> Verdict {
        match self.web.before_fetch(request) {
            None => Verdict::Unsupported,
            Some(Ok(())) => Verdict::Fetch { deny: None, events: Vec::new() },
            Some(Err(_)) => {
                let input = &request["input"];
                let id = request["tool_use_id"]
                    .as_str()
                    .or_else(|| input["tool_use_id"].as_str())
                    .filter(|id| !id.is_empty())
                    .unwrap_or(request_id);
                let name = input["tool_name"].as_str().unwrap_or(weather::FETCH_TOOL);
                let events = vec![self.refused_fetch(id, name, None, &input["tool_input"])];
                Verdict::Fetch { deny: Some(claude_cli::FETCH_ELSEWHERE_DENIED.to_string()), events }
            }
        }
    }

    /// The step of a fetch that was refused.
    fn refused_fetch(&mut self, id: &str, name: &str, display: Option<&str>, input: &Value) -> AskEvent {
        let mut step = self.step(id, name, display, input, StepState::Denied);
        step.outcome = format!("An ask can only read weather data from {}.", weather::SOURCE);
        self.record(step)
    }

    fn can_use_tool(&mut self, request_id: &str, request: &Value) -> Verdict {
        let name = request["tool_name"].as_str().unwrap_or("").trim();
        let input = &request["input"];
        if name.is_empty() || !input.is_object() {
            // A question that does not say what it is about gets a
            // no, and there is nothing to show as a step.
            return Verdict::Deny { message: NOT_AVAILABLE.to_string(), events: Vec::new() };
        }
        let id = request["tool_use_id"].as_str().filter(|id| !id.is_empty()).unwrap_or(request_id);
        let display = request["display_name"].as_str();
        if name == weather::FETCH_TOOL {
            // A fetch of an address that is on no allow list, so not
            // one of the weather hosts. The person is never asked to
            // let an ask read an address: the answer is no.
            let events = vec![self.refused_fetch(id, name, display, input)];
            return Verdict::Deny { message: claude_cli::FETCH_ELSEWHERE_DENIED.to_string(), events };
        }
        let asks_person = self.tools.get(name).is_some_and(|tool| tool.confirm);
        if !asks_person {
            // On neither list, or on the list of tools that run by
            // themselves and the CLI asked all the same: the person
            // is not asked about a call the plan did not foresee.
            let mut step = self.step(id, name, display, input, StepState::Denied);
            step.outcome = "Not available in this ask.".to_string();
            return Verdict::Deny { message: NOT_AVAILABLE.to_string(), events: vec![self.record(step)] };
        }
        // The person says yes to what they can see, and a confirm
        // shows all of it or is not raised.
        let shown = match steps::confirm_input(input) {
            Ok(shown) => shown,
            Err(total) => {
                let mut step = self.step(id, name, display, input, StepState::Denied);
                step.outcome = format!("Its input is too large to show you in full ({total} characters), so it was not run.");
                return Verdict::Deny { message: TOO_LARGE_TO_CONFIRM.to_string(), events: vec![self.record(step)] };
            }
        };
        let step = self.step(id, name, display, input, StepState::Waiting);
        let confirm = AskConfirm {
            request_id: request_id.to_string(),
            step_id: step.id.clone(),
            connection: step.connection.clone(),
            tool: step.tool.clone(),
            title: step.title.clone(),
            input: shown,
            timeout_s: self.confirm_timeout_s,
            armed_in_ms: steps::CONFIRM_ARM_MS,
        };
        self.confirms.insert(request_id.to_string(), step.id.clone());
        Verdict::Confirm { confirm, input: input.clone(), events: vec![self.record(step)] }
    }
}

impl Reader for AgentReader {
    fn read(&mut self, line: &str) -> Result<Line, AskError> {
        let Ok(v) = serde_json::from_str::<Value>(line.trim()) else {
            return Ok(Line::Other);
        };
        match v["type"].as_str() {
            Some("control_request") => Ok(self.permission(&v)),
            Some("assistant") => self.assistant(&v),
            Some("user") => self.tool_results(&v),
            Some("system") => Ok(match v["subtype"].as_str() {
                Some("permission_denied") => self.denied_by_cli(&v),
                Some("init") => self.init(&v)?,
                _ => Line::Other,
            }),
            // A tool call that starts: one of the connections' tools
            // is a step once it is complete (the `assistant` line);
            // anything else is checked now, as in a plain ask.
            Some("stream_event")
                if v["event"]["type"].as_str() == Some("content_block_start")
                    && v["event"]["content_block"]["name"].as_str().is_some_and(|name| name.starts_with("mcp__")) =>
            {
                Ok(Line::Other)
            }
            Some("result") if v["subtype"].as_str() == Some("error_max_turns") => Err(AskError::Backend(format!(
                "The ask stopped after {} steps without finishing. Ask for less at a time, or raise the limit in the Agent settings.",
                self.max_turns
            ))),
            // Text, the plan's windows, the result: as in a plain ask.
            _ => claude_cli::read_line(line, &mut self.web),
        }
    }

    fn decided(&mut self, request_id: &str, outcome: ConfirmOutcome) -> Vec<AskEvent> {
        let Some(id) = self.confirms.remove(request_id) else {
            return Vec::new();
        };
        let Some(step) = self.steps.get_mut(&id) else {
            return Vec::new();
        };
        let (state, said) = match outcome {
            ConfirmOutcome::Allowed => (StepState::Running, ""),
            ConfirmOutcome::Denied => (StepState::Denied, "You said no."),
            ConfirmOutcome::TimedOut => (StepState::Denied, "No answer in time, so it was not done."),
            ConfirmOutcome::Cancelled => (StepState::Denied, "Cancelled before it ran."),
            ConfirmOutcome::Gone => (StepState::Denied, "Claude Code stopped before you answered, so it was not run."),
        };
        step.state = state;
        step.outcome = said.to_string();
        vec![AskEvent::Step(step.clone())]
    }

    fn final_notice(&mut self) -> Option<String> {
        match self.pending.as_slice() {
            [] => None,
            [one] => Some(format!(
                "The connection {} was still starting when this was answered, so the answer may have been made without it. Ask again in a moment.",
                quoted(std::slice::from_ref(one))
            )),
            many => Some(format!(
                "{} connections were still starting when this was answered ({}), so the answer may have been made without them. Ask again in a moment.",
                many.len(),
                quoted(many)
            )),
        }
    }
}

/// The text of a tool result: a string, or the text blocks of a
/// list.
fn result_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

#[cfg(all(test, unix))]
mod tests;
