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
//! | `--tools ""` | No built-in tool: no shell, no file access, no web fetch. With web search on for asks, exactly `WebSearch`, as in a plain ask. |
//! | `--allowedTools` | The ticked read and draft tools: they run without a question. |
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
//!   answer is theirs; a yes hands the input back exactly as it was
//!   shown. For any other tool the answer is no, at once, without
//!   bothering the person: the model was not supposed to have it.
//!
//! Measured with Claude Code 2.1.322, Sonnet at low effort and a
//! local test connection (TASK-99 notes): about 3.5 s for an ask with
//! one tool call, 4 to 4.5 s with two.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use super::policy::{self, ToolKind};
use super::{AgentRun, connections};
use crate::ask::claude_cli::{self, Line, Permission, Process, Reader, Verdict};
use crate::ask::steps::{self, AskConfirm, AskStep, ConfirmOutcome, StepState};
use crate::ask::{AskBackend, AskError, AskEvent, AskInput, AskReply, AskSetup, CancelFlag};
use crate::settings::{ClaudeCliSettings, RefineEffort};

/// What the model is told when it reaches for a tool this ask does
/// not have.
pub const NOT_AVAILABLE: &str = "This tool is not available in this ask, so it was not run. Do not try it again or \
in another way. Tell the person that it was not done.";

/// The Claude Code CLI as the [`AskBackend`] of an agent ask.
pub struct ClaudeAgent {
    /// Only [`ClaudeCliSettings::path`] is used.
    settings: ClaudeCliSettings,
    run: AgentRun,
    process: Option<Process>,
}

impl ClaudeAgent {
    pub fn new(settings: ClaudeCliSettings, run: AgentRun) -> Self {
        Self { settings, run, process: None }
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

/// The arguments of one agent ask. See the module docs for what each
/// is there for.
pub fn spawn_args(setup: &AskSetup, run: &AgentRun, config_file: &Path, system_prompt_file: &Path) -> Vec<String> {
    // The Claude CLI has no "minimal" level.
    let effort = match setup.effort {
        None | Some(RefineEffort::Minimal) => RefineEffort::Low,
        Some(effort) => effort,
    };
    let mut allowed = run.tools.allowed.clone();
    if setup.web_search {
        allowed.insert(0, claude_cli::WEB_SEARCH_TOOL.to_string());
    }
    let mut args: Vec<String> = [
        "-p",
        "--model",
        setup.model.as_str(),
        "--effort",
        effort.as_str(),
        "--setting-sources",
        "",
        "--tools",
        if setup.web_search { claude_cli::WEB_SEARCH_TOOL } else { "" },
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
    args
}

/// What the lines of an agent ask mean, and the steps so far.
struct AgentReader {
    web_search: bool,
    confirm_timeout_s: u64,
    max_turns: u32,
    /// The tools the model can call, by full name.
    tools: HashMap<String, policy::PlannedTool>,
    /// Connection names by the way the CLI spells them in a tool's
    /// full name.
    connections: HashMap<String, String>,
    /// Every step so far, by its id.
    steps: HashMap<String, AskStep>,
    /// The step a confirm is about, by the confirm's request id.
    confirms: HashMap<String, String>,
}

impl AgentReader {
    fn new(run: &AgentRun, web_search: bool) -> Self {
        Self {
            web_search,
            confirm_timeout_s: run.confirm_timeout_s,
            max_turns: run.max_turns,
            tools: run.planned.iter().map(|t| (t.full_name.clone(), t.clone())).collect(),
            connections: run.connections.iter().map(|c| (policy::server_key(&c.name), c.name.clone())).collect(),
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
            None => {
                let (server, tool) = full_name.strip_prefix("mcp__").and_then(|rest| rest.split_once("__")).unwrap_or(("", full_name));
                let connection = self.connections.get(server).cloned().unwrap_or_else(|| server.to_string());
                (connection, tool.to_string(), shown.unwrap_or(tool).to_string(), ToolKind::Act)
            }
        };
        AskStep {
            id: id.to_string(),
            connection,
            tool,
            title,
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
                events.push(self.record(step));
            } else if claude_cli::check_tool(block, self.web_search)? {
                events.push(AskEvent::Searching { query: block["input"]["query"].as_str().unwrap_or("").to_string() });
            }
        }
        Ok(if events.is_empty() { Line::Other } else { Line::Events(events) })
    }

    /// A tool's answer, fed back to the model.
    fn tool_results(&mut self, v: &Value) -> Line {
        let mut events = Vec::new();
        for block in v["message"]["content"].as_array().into_iter().flatten() {
            if block["type"].as_str() != Some("tool_result") {
                continue;
            }
            let id = block["tool_use_id"].as_str().unwrap_or("");
            if !self.steps.contains_key(id) {
                // Not one of ours: the web search coming back.
                events.push(AskEvent::SearchDone);
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
        if events.is_empty() { Line::Other } else { Line::Events(events) }
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

    /// The process is up: say so when a connection of this run is
    /// not.
    fn init(&self, v: &Value) -> Line {
        let down: Vec<&str> = v["mcp_servers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|server| matches!(server["status"].as_str(), Some("failed" | "needs-auth" | "disabled")))
            .filter_map(|server| server["name"].as_str())
            .collect();
        match down.as_slice() {
            [] => Line::Other,
            [one] => Line::Notice(format!("The connection \"{one}\" could not be reached, so this answer was made without it.")),
            many => Line::Notice(format!(
                "{} connections could not be reached ({}), so this answer was made without them.",
                many.len(),
                many.join(", ")
            )),
        }
    }

    /// The CLI asks whether a tool may run.
    fn permission(&mut self, v: &Value) -> Line {
        let Some(request_id) = v["request_id"].as_str().filter(|id| !id.is_empty()) else {
            // Nothing to answer to.
            return Line::Other;
        };
        let request = &v["request"];
        let verdict = if request["subtype"].as_str() != Some("can_use_tool") {
            Verdict::Unsupported
        } else {
            self.can_use_tool(request_id, request)
        };
        Line::Permission(Permission { request_id: request_id.to_string(), verdict })
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
        let asks_person = self.tools.get(name).is_some_and(|tool| tool.confirm);
        if !asks_person {
            // On neither list, or on the list of tools that run by
            // themselves and the CLI asked all the same: the person
            // is not asked about a call the plan did not foresee.
            let mut step = self.step(id, name, display, input, StepState::Denied);
            step.outcome = "Not available in this ask.".to_string();
            return Verdict::Deny { message: NOT_AVAILABLE.to_string(), events: vec![self.record(step)] };
        }
        let step = self.step(id, name, display, input, StepState::Waiting);
        let (shown, cut) = steps::confirm_input(input);
        let confirm = AskConfirm {
            request_id: request_id.to_string(),
            step_id: step.id.clone(),
            connection: step.connection.clone(),
            tool: step.tool.clone(),
            title: step.title.clone(),
            input: shown,
            input_cut: cut,
            timeout_s: self.confirm_timeout_s,
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
            Some("user") => Ok(self.tool_results(&v)),
            Some("system") => Ok(match v["subtype"].as_str() {
                Some("permission_denied") => self.denied_by_cli(&v),
                Some("init") => self.init(&v),
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
            _ => claude_cli::read_line(line, self.web_search),
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
        };
        step.state = state;
        step.outcome = said.to_string();
        vec![AskEvent::Step(step.clone())]
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
