//! Ask backend: Claude, through the Claude Code CLI the user installed
//! and signed in themselves.
//!
//! The same binary, lookup, `PATH` handling and lean flags as Refine's
//! backend ([`crate::refine::claude_cli`], which also explains why the
//! flags are what they are and what differs on Windows). This file
//! only adds what an ask needs on top:
//!
//! - **Streaming.** `--include-partial-messages` makes the CLI print
//!   the answer as it is written (`stream_event` lines carrying
//!   `text_delta`s) instead of one `result` line at the end. Thinking
//!   deltas can come first and are skipped.
//! - **No tools, pinned.** `--tools ""` as in Refine, and always
//!   `--permission-mode default`: the CLI's own default mode is a
//!   per-machine setting and can be a looser one. With web search on,
//!   the tool list is exactly `WebSearch,WebFetch`, and the fetch
//!   tool is allowed for the two weather hosts and nothing else
//!   ([`super::weather`], [`web_allow_rules`]). If the model is seen
//!   starting any other tool, the turn fails and the process is
//!   killed; with these flags that cannot happen, so if it does, the
//!   flags no longer mean what this file assumes. A fetch of another
//!   address is not such a tool call: the CLI refuses it and the
//!   model answers without it ([`WebTools`]). Should one come back
//!   with content all the same, the turn fails there.
//! - **A conversation.** stdin stays open after the first question,
//!   so a later [`AskBackend::send`] continues in the same process
//!   with the context kept (TASK-98 sends the screen that way). One
//!   user line per turn; the turn ends at the CLI's `result` line.
//! - **Cancel.** The reader thread hands lines over a channel and the
//!   turn polls it, so a cancel or the timeout kills the process
//!   within a few milliseconds even while the model is silent (a web
//!   search prints nothing for 15 seconds or more). The question is
//!   written to stdin from a thread of its own for the same reason: a
//!   large one (a screenshot) can fill the pipe before the CLI reads,
//!   and a write that waits must not keep the turn from being
//!   cancelled or timing out. On Windows the kill takes the whole
//!   process tree, see [`refine_cli::end_process`].
//!
//! **Shared with Agent.** An ask with tools (`crate::agent::claude`,
//! TASK-99) is the same kind of process: the same spawn, the same
//! question on stdin, the same polling loop, cancel and timeout. What
//! differs is the argument list and what the lines mean, so those two
//! are handed in ([`Process::spawn`] takes the arguments and a
//! [`Reader`]), and the loop here also knows the one thing only an
//! ask with tools needs: a question from the CLI that waits for an
//! answer ([`Line::Permission`]). The plain ask below never gets
//! one; its arguments and its reading of the lines are exactly what
//! they were before Agent existed, and a test pins that.
//!
//! Measured with Claude Code 2.1.322, Sonnet at low effort
//! (2026-11-10): cold, the first text is back after 1.3 to 2.3 s;
//! with the process spawned at record start, after 0.5 to 0.7 s.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde_json::{Value, json};

use super::steps::{self, AskConfirm, ConfirmDecision, ConfirmOutcome, Waiting};
use super::{AskBackend, AskError, AskEvent, AskInput, AskReply, AskSetup, CancelFlag, POLL, Turn, weather};
use crate::refine::claude_cli as refine_cli;
use crate::refine::{CallUsage, PlanWindows};
use crate::settings::{ClaudeCliSettings, RefineEffort};

/// Backend id: the same backend as Refine's.
pub const ID: &str = refine_cli::ID;

/// The tool an ask has with web search on. The other one is the
/// fetch tool, for the weather hosts only ([`super::weather`]).
pub(crate) const WEB_SEARCH_TOOL: &str = "WebSearch";

/// The value of `--tools` with web search on: the two built-in web
/// tools and no other.
pub(crate) fn web_tools() -> String {
    format!("{WEB_SEARCH_TOOL},{}", weather::FETCH_TOOL)
}

/// What the allow list holds for the web with web search on: the
/// search, and the fetch tool once per weather host
/// ([`weather::allow_rules`]). Every entry is fixed at compile time.
/// The bare name of the fetch tool is never one of them: that would
/// allow every address.
pub(crate) fn web_allow_rules() -> Vec<String> {
    let mut rules = vec![WEB_SEARCH_TOOL.to_string()];
    rules.extend(weather::allow_rules());
    rules
}

/// The Claude Code CLI as an [`AskBackend`].
pub struct ClaudeCliAsk {
    /// Only [`ClaudeCliSettings::path`] is used: the model and effort
    /// of an ask come with the [`AskSetup`].
    settings: ClaudeCliSettings,
    process: Option<Process>,
}

impl ClaudeCliAsk {
    pub fn new(settings: ClaudeCliSettings) -> Self {
        Self { settings, process: None }
    }

    fn spawn(&self, setup: &AskSetup) -> Result<Process, AskError> {
        let bin = find_binary(&self.settings)?;
        let reader = Box::new(PlainReader { web: WebTools::new(setup.web_search) });
        Process::spawn(&bin, setup, |prompt_file| Ok(spawn_args(setup, prompt_file)), reader, None, false)
    }
}

/// The `claude` binary these settings point at, or the reason an ask
/// cannot run. Shared with the agent backend.
pub(crate) fn find_binary(settings: &ClaudeCliSettings) -> Result<PathBuf, AskError> {
    refine_cli::find_binary(settings).ok_or_else(|| AskError::Unavailable(refine_cli::NOT_INSTALLED.to_string()))
}

/// One turn through the process in `slot`: the process that is there
/// when it was started with this very setup (a warmed one, or the
/// one of the turn before), a new one from `spawn` otherwise. The
/// process is kept for a further turn after an answer and dropped,
/// which kills it, after an error. The one path for the plain ask
/// and the ask with tools.
pub(crate) fn send_through(
    slot: &mut Option<Process>,
    setup: &AskSetup,
    spawn: impl FnOnce() -> Result<Process, AskError>,
    input: &AskInput,
    sink: &mut dyn FnMut(AskEvent),
    cancel: &CancelFlag,
    timeout: Duration,
) -> Result<AskReply, AskError> {
    let started = Instant::now();
    let mut warm_lead = None;
    let mut process = match slot.take() {
        // The setup is fixed at spawn. If it changed since
        // (dictionary edited mid-recording), start over.
        Some(p) if p.setup == *setup => {
            if p.turns == 0 {
                warm_lead = Some(p.spawned_at.elapsed());
            }
            p
        }
        _ => spawn()?,
    };
    let reply = process.turn(input, sink, cancel, started, timeout, warm_lead)?;
    // Kept for a further turn. On any error above the process was
    // dropped instead, which kills it.
    *slot = Some(process);
    Ok(reply)
}

impl AskBackend for ClaudeCliAsk {
    fn id(&self) -> &'static str {
        ID
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
        let reply = send_through(&mut slot, setup, || self.spawn(setup), input, sink, cancel, timeout);
        self.process = slot;
        reply
    }
}

/// Arguments for one headless conversation: Refine's lean flags
/// ([`refine_cli::spawn_args`]) with streaming, the permission mode
/// pinned and, with web search on, the two web tools: the search, and
/// the fetch tool for the weather hosts only ([`web_allow_rules`]).
/// With web search off there is no tool and no allow list, as before.
///
/// `--tools` and `--allowedTools` take a list, handed over as one
/// comma-separated argument each, and each is followed by another
/// `--flag`, never by a bare value that could be read as one more
/// tool.
pub fn spawn_args(setup: &AskSetup, system_prompt_file: &Path) -> Vec<String> {
    // The Claude CLI has no "minimal" level.
    let effort = match setup.effort {
        None | Some(RefineEffort::Minimal) => RefineEffort::Low,
        Some(effort) => effort,
    };
    let mut args = refine_cli::spawn_args(&setup.model, effort, system_prompt_file);
    if setup.web_search
        && let Some(tools) = args.iter().position(|a| a == "--tools")
        && tools + 1 < args.len()
    {
        args[tools + 1] = web_tools();
        args.splice(tools + 2..tools + 2, ["--allowedTools".to_string(), web_allow_rules().join(",")]);
    }
    args.extend(
        ["--include-partial-messages", "--permission-mode", "default"]
            .iter()
            .map(|s| (*s).to_string()),
    );
    args
}

/// The user line of one turn, in the CLI's stream-json input format:
/// images first (as base64 blocks), then the text.
///
/// The line holds every image of the turn as base64 text. The JSON
/// value it is made from is wiped before this returns; the line
/// itself is the caller's to wipe once it is written
/// ([`super::wipe::Text`], as [`Process::turn`] does).
pub fn user_line(input: &AskInput) -> String {
    let mut content: Vec<Value> = input
        .images
        .iter()
        .map(|image| {
            json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": image.media_type,
                    "data": base64::engine::general_purpose::STANDARD.encode(&image.data),
                },
            })
        })
        .collect();
    content.push(json!({ "type": "text", "text": input.text }));
    let mut message = json!({ "type": "user", "message": { "role": "user", "content": content } });
    let line = message.to_string();
    if !input.images.is_empty() {
        super::wipe::json(&mut message);
    }
    line
}

/// What the lines of one kind of conversation mean. The plain ask
/// has [`PlainReader`]; an ask with tools brings its own
/// (`crate::agent::claude`), which also keeps track of its steps.
pub(crate) trait Reader: Send {
    /// Read one stdout line. An error ends the turn.
    fn read(&mut self, line: &str) -> Result<Line, AskError>;

    /// A confirm this reader asked for ([`Verdict::Confirm`]) has
    /// ended. Returns what to tell the sink about it.
    fn decided(&mut self, _request_id: &str, _outcome: ConfirmOutcome) -> Vec<AskEvent> {
        Vec::new()
    }

    /// The turn has its answer: something the person should know
    /// about how it was made that only shows at the end. Used when
    /// no line of the turn brought a notice ([`Line::Notice`]).
    fn final_notice(&mut self) -> Option<String> {
        None
    }
}

/// The reader of an ask without tools: [`read_line`].
struct PlainReader {
    web: WebTools,
}

impl Reader for PlainReader {
    fn read(&mut self, line: &str) -> Result<Line, AskError> {
        // The one question the CLI can have for an ask without
        // tools: may this fetch run?
        if let Some(permission) = self.web.asked(line) {
            return Ok(Line::Permission(permission));
        }
        read_line(line, &mut self.web)
    }
}

/// The built-in web tools of one conversation, and what its lines
/// say about them.
///
/// With web search off there are none: any tool call is an error.
/// With it on there are two, and they are read differently:
///
/// - the **search** is shown while it runs
///   ([`AskEvent::Searching`], [`AskEvent::SearchDone`]);
/// - a **fetch of a weather host** ([`weather::is_allowed`]) is shown
///   the same way, with [`weather::SHOWN`] in place of a query;
/// - a **fetch of any other address** is not shown, and it does not
///   run. It is stopped twice over: the CLI checks with this process
///   before every fetch ([`WebTools::hello`],
///   [`WebTools::before_fetch`]) and is told no, and the address is
///   on no allow list either. The model is told so and answers
///   without it; the sink hears that it was refused
///   ([`AskEvent::FetchRefused`]). Should the result of such a fetch
///   come back as anything but a refusal, the CLI has read from an
///   address this ask may not reach: the turn fails there, which
///   ends the process, and the person gets the error instead of an
///   answer made from it.
pub(crate) struct WebTools {
    on: bool,
    /// Fetches of an address that is not a weather host, by the id
    /// of the tool call, with the host as it may be shown.
    elsewhere: HashMap<String, String>,
}

/// One of the two built-in web tools.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WebTool {
    Search,
    Fetch,
}

/// What stands for the host of an address that has none this app
/// can read.
const UNREADABLE_ADDRESS: &str = "an address that could not be read";

/// The name this app gives its check before a fetch
/// ([`WebTools::hello`]); the CLI names it when it asks.
pub(crate) const FETCH_CHECK: &str = "openwhisper-fetch";

/// What the model is told when it tries to fetch an address that is
/// not a weather host.
pub const FETCH_ELSEWHERE_DENIED: &str = "Nothing was fetched: from here only weather data can be read, from api.open-meteo.com and geocoding-api.open-meteo.com. Do not try this or any other address again. Answer from what you know, and say in one sentence that you could not read the page.";

impl WebTools {
    pub(crate) fn new(web_search: bool) -> Self {
        Self { on: web_search, elsewhere: HashMap::new() }
    }

    /// What a process is told before its first question when the
    /// fetch tool is in play: check with this process before every
    /// fetch ([`FETCH_CHECK`]). `None` with web search off: then
    /// there is no fetch tool and nothing is said, so the CLI is
    /// started and spoken to exactly as before.
    ///
    /// Why the allow list is not enough. Claude Code lets its fetch
    /// tool read a list of its own without any rule (about a hundred
    /// documentation sites, found in Claude Code 2.1.325; nothing
    /// documented switches that off). The allow list of this app
    /// cannot take those away. A check before the tool runs can: it
    /// comes first, and a no from it is final. `onFailure: block`
    /// makes a check that is not answered, or answered wrongly,
    /// count as a no.
    pub(crate) fn hello(web_search: bool) -> Option<String> {
        web_search.then(|| {
            json!({
                "type": "control_request",
                "request_id": "openwhisper-hello",
                "request": {
                    "subtype": "initialize",
                    "hooks": {
                        "PreToolUse": [{
                            "matcher": weather::FETCH_TOOL,
                            "hookCallbackIds": [FETCH_CHECK],
                            "onFailure": "block",
                        }],
                    },
                },
            })
            .to_string()
        })
    }

    /// `line` is a question from the CLI to an ask without tools: the
    /// answer to give. The one question such an ask knows is the
    /// check before a fetch ([`WebTools::hello`]); any other gets
    /// "not supported", and nothing runs because of it. `None` for a
    /// line that is not a question.
    pub(crate) fn asked(&mut self, line: &str) -> Option<Permission> {
        // Cheap for the lines of an answer, which are never this.
        if !line.contains("\"control_request\"") {
            return None;
        }
        let v = serde_json::from_str::<Value>(line.trim()).ok()?;
        if v["type"].as_str() != Some("control_request") {
            return None;
        }
        let request_id = v["request_id"].as_str().filter(|id| !id.is_empty())?;
        let verdict = match self.before_fetch(&v["request"]) {
            None => Verdict::Unsupported,
            Some(Ok(())) => Verdict::Fetch { deny: None, events: Vec::new() },
            Some(Err(_)) => Verdict::Fetch { deny: Some(FETCH_ELSEWHERE_DENIED.to_string()), events: Vec::new() },
        };
        Some(Permission { request_id: request_id.to_string(), verdict })
    }

    /// What to say to one check before a fetch: `Ok` leaves the
    /// fetch to the CLI's allow list (the address is a weather
    /// host); `Err` is a no, with the host that was asked for, safe
    /// to show. `None` when `request` is not the check this app
    /// asked for.
    ///
    /// A no is the answer to everything that is not exactly a fetch
    /// of an https address on a weather host: another address, an
    /// address that cannot be read, another tool.
    pub(crate) fn before_fetch(&mut self, request: &Value) -> Option<Result<(), String>> {
        if !self.on
            || request["subtype"].as_str() != Some("hook_callback")
            || request["callback_id"].as_str() != Some(FETCH_CHECK)
        {
            return None;
        }
        let input = &request["input"];
        let url = input["tool_input"]["url"].as_str().unwrap_or("");
        if input["tool_name"].as_str() == Some(weather::FETCH_TOOL) && weather::is_allowed(url) {
            return Some(Ok(()));
        }
        let host = weather::host_of(url).unwrap_or_else(|| UNREADABLE_ADDRESS.to_string());
        // Remembered, so the result of this call is read as a
        // refusal also when the call itself was not seen first.
        let id = request["tool_use_id"].as_str().or_else(|| input["tool_use_id"].as_str()).unwrap_or("");
        self.elsewhere.insert(id.to_string(), host.clone());
        Some(Err(host))
    }

    /// Which web tool `block` calls: `Ok(None)` when it is not a tool
    /// call at all. Any other tool is an error, and so is a web tool
    /// while web search is off: an ask may not use tools.
    pub(crate) fn tool(&self, block: &Value) -> Result<Option<WebTool>, AskError> {
        let kind = block["type"].as_str().unwrap_or("");
        if !kind.ends_with("tool_use") {
            return Ok(None);
        }
        let name = block["name"].as_str().unwrap_or("unknown");
        match name {
            WEB_SEARCH_TOOL if self.on => Ok(Some(WebTool::Search)),
            weather::FETCH_TOOL if self.on => Ok(Some(WebTool::Fetch)),
            _ => Err(AskError::Backend(format!(
                "The model tried to use a tool ({}). An ask may not use tools, so it was stopped.",
                steps::one_line(name, 80)
            ))),
        }
    }

    /// A complete tool call (its input is there). Returns what to
    /// show for it, if anything; see the type's docs.
    pub(crate) fn called(&mut self, block: &Value) -> Result<Option<AskEvent>, AskError> {
        Ok(match self.tool(block)? {
            None => None,
            Some(WebTool::Search) => {
                Some(AskEvent::Searching { query: block["input"]["query"].as_str().unwrap_or("").to_string() })
            }
            Some(WebTool::Fetch) => {
                let url = block["input"]["url"].as_str().unwrap_or("");
                if weather::is_allowed(url) {
                    Some(AskEvent::Searching { query: weather::SHOWN.to_string() })
                } else {
                    let host = weather::host_of(url).unwrap_or_else(|| UNREADABLE_ADDRESS.to_string());
                    self.elsewhere.insert(block["id"].as_str().unwrap_or("").to_string(), host);
                    None
                }
            }
        })
    }

    /// The result of a web tool call, fed back to the model: the
    /// search or the weather came back, or a fetch of another address
    /// was refused. An error when such a fetch was not refused.
    pub(crate) fn came_back(&mut self, block: &Value) -> Result<AskEvent, AskError> {
        let Some(host) = self.elsewhere.remove(block["tool_use_id"].as_str().unwrap_or("")) else {
            return Ok(AskEvent::SearchDone);
        };
        if block["is_error"].as_bool().unwrap_or(false) {
            Ok(AskEvent::FetchRefused { host })
        } else {
            Err(AskError::Backend(format!(
                "Claude Code read from a web address an ask may not reach ({host}), so the ask was stopped and its \
answer thrown away. An ask can only read weather data from {}.",
                weather::SOURCE
            )))
        }
    }
}

/// A question from the CLI that it waits on: may this tool run?
#[derive(Debug, PartialEq)]
pub(crate) struct Permission {
    /// The CLI's id of the question; the answer names it.
    pub request_id: String,
    pub verdict: Verdict,
}

/// What to do about a [`Permission`]. Only an ask with tools makes
/// one (`crate::agent::claude`, cargo feature `agent`).
#[derive(Debug, PartialEq)]
#[cfg_attr(not(feature = "agent"), allow(dead_code))]
pub(crate) enum Verdict {
    /// Say no at once, with `message` for the model. `events` go to
    /// the sink.
    Deny { message: String, events: Vec<AskEvent> },
    /// Ask the person and wait: `events` go to the sink first (the
    /// step that waits), then the confirm. On a yes the tool gets
    /// `input` back exactly as it came.
    Confirm { confirm: AskConfirm, input: Value, events: Vec<AskEvent> },
    /// Not a question this app knows how to answer: the CLI is told
    /// so, and nothing runs because of it.
    Unsupported,
    /// The CLI asks before a fetch, because this app told it to
    /// ([`WebTools::hello`]). `deny` is what the model is told when
    /// the address is not a weather host, and the fetch does not
    /// run; `None` leaves the fetch to the CLI's allow list. `events`
    /// go to the sink.
    Fetch { deny: Option<String>, events: Vec<AskEvent> },
}

/// What the model is told when the person said no.
pub(crate) const DENIED_BY_PERSON: &str = "The person declined this action in the panel, so it was not carried out. Do not try it again or in another way. Tell them that it was not done.";
/// What the model is told when nobody answered the confirm in time.
pub(crate) const DENIED_BY_TIMEOUT: &str = "The person did not answer the confirmation in time, so the action was not carried out. Do not try it again or in another way. Tell them that it was not done.";

/// One spawned CLI process: one conversation.
pub(crate) struct Process {
    child: Child,
    stdin: Option<ChildStdin>,
    /// stdout, line by line. Disconnects when the process ends.
    lines: Receiver<String>,
    /// What the process wrote to stderr, complete once it has ended.
    errors: Arc<Mutex<String>>,
    /// The setup this process was started with.
    setup: AskSetup,
    /// Temp file holding the system prompt; deleted on drop.
    prompt_file: PathBuf,
    spawned_at: Instant,
    /// Turns answered so far.
    turns: u32,
    /// A question was sent and its result has not come back: the
    /// model may be working on it.
    mid_turn: bool,
    /// What the lines mean for this kind of conversation.
    reader: Box<dyn Reader>,
    /// Something that has to live exactly as long as the process and
    /// cleans up after itself when dropped (an ask with tools keeps
    /// its private connection file here).
    _keep: Option<Box<dyn std::any::Any + Send>>,
    /// The process was started in a process group of its own (unix,
    /// an ask with tools): ending it ends the group, so the local
    /// servers of its connections go with it.
    #[cfg_attr(not(unix), allow(dead_code))]
    own_group: bool,
}

impl Process {
    /// Start the CLI at `bin` for one conversation. `args` makes the
    /// argument list, given the file the system prompt was written
    /// to (an error from it stops the start); `reader` says what the
    /// lines mean; `keep` is dropped with the process. With
    /// `own_group` (unix only; an ask with tools) the process gets a
    /// process group of its own, which is ended as a whole when the
    /// process is: see [`refine_cli::kill_process_group`]. A plain
    /// ask passes `false` and is started exactly as before.
    pub(crate) fn spawn(
        bin: &Path,
        setup: &AskSetup,
        args: impl FnOnce(&Path) -> Result<Vec<String>, AskError>,
        reader: Box<dyn Reader>,
        keep: Option<Box<dyn std::any::Any + Send>>,
        own_group: bool,
    ) -> Result<Self, AskError> {
        // What an earlier process of this app left behind when it
        // was killed goes before a new file is written.
        refine_cli::sweep_stale_private_files();
        let prompt_file = refine_cli::write_prompt_file("Ask", &setup.system)?;
        let args = match args(&prompt_file) {
            Ok(args) => args,
            Err(e) => {
                let _ = fs::remove_file(&prompt_file);
                return Err(e);
            }
        };
        let mut cmd = refine_cli::command(bin);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let own_group = own_group && cfg!(unix);
        #[cfg(unix)]
        if own_group {
            use std::os::unix::process::CommandExt;
            // The group's id is the process's own id.
            cmd.process_group(0);
        }
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                let _ = fs::remove_file(&prompt_file);
                return Err(AskError::Unavailable(format!(
                    "Could not start Claude Code ({}): {e}",
                    bin.display()
                )));
            }
        };
        let mut stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        // With the fetch tool in play: ask the CLI to check with
        // this process before every fetch. A few hundred bytes into
        // an empty pipe, so the write does not wait. A CLI that is
        // already gone is found out by the first turn.
        if let (Some(hello), Some(stdin)) = (WebTools::hello(setup.web_search), stdin.as_mut()) {
            let _ = stdin.write_all(hello.as_bytes()).and_then(|()| stdin.write_all(b"\n")).and_then(|()| stdin.flush());
        }

        let errors = Arc::new(Mutex::new(String::new()));
        if let Some(mut stderr) = stderr {
            let errors = errors.clone();
            thread::spawn(move || {
                let mut buf = String::new();
                let _ = stderr.read_to_string(&mut buf);
                if let Ok(mut e) = errors.lock() {
                    *e = buf;
                }
            });
        }

        let (tx, lines) = mpsc::channel();
        if let Some(stdout) = stdout {
            thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if tx.send(line).is_err() {
                        // Nobody is listening any more.
                        return;
                    }
                }
                // The process ended. Give the stderr reader a moment
                // before the channel closes and the turn asks what it
                // said.
                thread::sleep(Duration::from_millis(100));
            });
        }

        Ok(Self {
            child,
            stdin,
            lines,
            errors,
            setup: setup.clone(),
            prompt_file,
            spawned_at: Instant::now(),
            turns: 0,
            mid_turn: false,
            reader,
            _keep: keep,
            own_group,
        })
    }

    /// Write one user line and read until that turn's `result` line.
    fn turn(
        &mut self,
        input: &AskInput,
        sink: &mut dyn FnMut(AskEvent),
        cancel: &CancelFlag,
        started: Instant,
        timeout: Duration,
        warm_lead: Option<Duration>,
    ) -> Result<AskReply, AskError> {
        let Some(mut stdin) = self.stdin.take() else {
            return Err(AskError::Backend("Claude Code has no input channel".into()));
        };
        // Written on a thread of its own: the line can be large, and
        // a CLI that is not reading yet lets the write wait. This
        // thread keeps its eye on the cancel flag and the clock
        // meanwhile. When the turn is given up, the process is killed
        // (on drop), which ends a write that is still waiting.
        //
        // The line holds the screenshot as base64 text when the turn
        // has one. It is wiped when the write is over, however that
        // ended: the guard goes with the thread's closure. The copy
        // in the pipe and in the CLI's own memory is not ours to
        // wipe; it ends with that process.
        let line = super::wipe::Text(user_line(input));
        let (wrote_tx, wrote) = mpsc::channel();
        thread::spawn(move || {
            let result = stdin
                .write_all(line.0.as_bytes())
                .and_then(|()| stdin.write_all(b"\n"))
                .and_then(|()| stdin.flush());
            drop(line);
            // stdin goes back for a further turn. When nobody waits
            // for it any more, it closes here.
            let _ = wrote_tx.send((result, stdin));
        });
        self.mid_turn = true;
        let mut writing = true;

        let mut deadline = started + timeout;
        let mut turn = Turn::new(sink, started);
        let mut plan: Option<PlanWindows> = None;
        let mut notice = String::new();
        loop {
            if cancel.is_cancelled() {
                return Err(AskError::Cancelled);
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(AskError::Timeout);
            }
            if writing {
                match wrote.try_recv() {
                    Ok((Ok(()), stdin)) => {
                        self.stdin = Some(stdin);
                        writing = false;
                    }
                    Ok((Err(_), _)) | Err(TryRecvError::Disconnected) => {
                        // The process is already gone; wait for its
                        // last words.
                        return Err(match self.lines.recv_timeout(Duration::from_millis(500)) {
                            Err(RecvTimeoutError::Disconnected) => self.exit_error(),
                            _ => AskError::Backend("Claude Code exited before it was asked".into()),
                        });
                    }
                    Err(TryRecvError::Empty) => {}
                }
            }
            let line = match self.lines.recv_timeout(POLL.min(deadline - now)) {
                Ok(line) => line,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return Err(self.exit_error()),
            };
            match self.reader.read(&line)? {
                Line::Events(events) => events.into_iter().for_each(|event| turn.event(event)),
                Line::Notice(text) => {
                    if notice.is_empty() {
                        notice = text;
                    }
                }
                Line::Permission(permission) => {
                    // The CLI waits for this answer, so the question
                    // is long written and stdin is back. Should the
                    // writer still hold it, wait for it.
                    if writing {
                        if let Ok((Ok(()), stdin)) = wrote.recv_timeout(Duration::from_secs(2)) {
                            self.stdin = Some(stdin);
                        }
                        writing = false;
                    }
                    let waited = self.decide(permission, &mut turn, cancel, deadline)?;
                    // The clock of the turn does not run while the
                    // person reads a confirm.
                    deadline += waited;
                }
                Line::Text(delta) => turn.text(&delta),
                Line::NewMessage => turn.new_message(),
                Line::Plan(windows) => plan = Some(windows),
                Line::Result(text) => {
                    // A CLI that did not stream still answers.
                    if !turn.has_text() {
                        turn.text(&text);
                    }
                    let usage: Option<CallUsage> = refine_cli::parse_usage(&line).map(|mut call| {
                        call.plan = plan.take();
                        call
                    });
                    self.turns += 1;
                    self.mid_turn = false;
                    if notice.is_empty() {
                        notice = self.reader.final_notice().unwrap_or_default();
                    }
                    return Ok(turn.finish(usage, warm_lead, &notice));
                }
                Line::Other => {}
            }
        }
    }

    fn exit_error(&self) -> AskError {
        let said = self.errors.lock().map(|e| e.clone()).unwrap_or_default();
        refine_cli::exit_error(&said).into()
    }

    /// Answer one [`Permission`]: at once, or after the person did.
    /// Returns how long the person was waited on.
    ///
    /// While a confirm waits, this looks at the cancel flag, the
    /// confirm's own clock and the process every [`POLL`]. A cancel
    /// is a no: the CLI is told so, and then the turn ends as
    /// cancelled, which kills the process. The tool cannot have run
    /// in between: the CLI does not start it before it has an
    /// answer. A CLI that stops while the confirm waits closes the
    /// confirm at once ([`ConfirmOutcome::Gone`]): the person is not
    /// left to answer a question nobody is waiting on, and the turn
    /// ends with what the process said.
    fn decide(
        &mut self,
        permission: Permission,
        turn: &mut Turn<'_>,
        cancel: &CancelFlag,
        deadline: Instant,
    ) -> Result<Duration, AskError> {
        let Permission { request_id, verdict } = permission;
        match verdict {
            Verdict::Unsupported => {
                self.respond(&json!({
                    "type": "control_response",
                    "response": { "subtype": "error", "request_id": request_id, "error": "This request is not supported." },
                }))?;
                Ok(Duration::ZERO)
            }
            Verdict::Deny { message, events } => {
                self.respond(&deny(&request_id, &message))?;
                events.into_iter().for_each(|event| turn.event(event));
                Ok(Duration::ZERO)
            }
            Verdict::Fetch { deny, events } => {
                self.respond(&fetch_answer(&request_id, deny.as_deref()))?;
                events.into_iter().for_each(|event| turn.event(event));
                Ok(Duration::ZERO)
            }
            Verdict::Confirm { confirm, input, events } => {
                let asked = Instant::now();
                let patience = Duration::from_secs(confirm.timeout_s);
                // Open before the sink hears of it: an answer given
                // from inside the sink must find the gate.
                let waiting = Waiting::open(&request_id);
                events.into_iter().for_each(|event| turn.event(event));
                turn.event(AskEvent::Confirm(confirm));
                let outcome = loop {
                    if cancel.is_cancelled() {
                        break ConfirmOutcome::Cancelled;
                    }
                    match waiting.answer() {
                        Some(ConfirmDecision::Allow) => break ConfirmOutcome::Allowed,
                        Some(ConfirmDecision::Deny) => break ConfirmOutcome::Denied,
                        None => {}
                    }
                    // The turn's own clock stands still meanwhile,
                    // but an ask that was out of time before the
                    // confirm does not get more through it.
                    if asked.elapsed() >= patience || asked >= deadline {
                        break ConfirmOutcome::TimedOut;
                    }
                    // The CLI is gone: nobody waits for this answer.
                    if matches!(self.child.try_wait(), Ok(Some(_))) {
                        break ConfirmOutcome::Gone;
                    }
                    thread::sleep(POLL);
                };
                drop(waiting);
                if outcome == ConfirmOutcome::Gone {
                    turn.event(AskEvent::ConfirmClosed { request_id: request_id.clone(), outcome });
                    self.reader.decided(&request_id, outcome).into_iter().for_each(|event| turn.event(event));
                    // Let the stderr reader finish, then say why.
                    thread::sleep(Duration::from_millis(150));
                    return Err(match self.exit_error() {
                        AskError::Backend(said) => {
                            AskError::Backend(format!("Claude Code stopped while it waited for your answer, so nothing was run. {said}"))
                        }
                        other => other,
                    });
                }
                let response = match outcome {
                    ConfirmOutcome::Allowed => json!({
                        "type": "control_response",
                        "response": {
                            "subtype": "success",
                            "request_id": request_id,
                            // Exactly what the person was shown.
                            "response": { "behavior": "allow", "updatedInput": input },
                        },
                    }),
                    ConfirmOutcome::TimedOut => deny(&request_id, DENIED_BY_TIMEOUT),
                    ConfirmOutcome::Denied | ConfirmOutcome::Cancelled | ConfirmOutcome::Gone => {
                        deny(&request_id, DENIED_BY_PERSON)
                    }
                };
                let written = self.respond(&response);
                turn.event(AskEvent::ConfirmClosed { request_id: request_id.clone(), outcome });
                self.reader.decided(&request_id, outcome).into_iter().for_each(|event| turn.event(event));
                if outcome == ConfirmOutcome::Cancelled {
                    return Err(AskError::Cancelled);
                }
                written?;
                Ok(asked.elapsed())
            }
        }
    }

    /// Write one line to the CLI's stdin: the answer to a question
    /// it is waiting on, so it is reading.
    fn respond(&mut self, message: &Value) -> Result<(), AskError> {
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(AskError::Backend("Claude Code has no input channel".into()));
        };
        let mut line = message.to_string();
        line.push('\n');
        stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|e| AskError::Backend(format!("Could not answer Claude Code: {e}")))
    }
}

/// The answer "no" to the CLI's question `request_id`, with `message`
/// for the model.
fn deny(request_id: &str, message: &str) -> Value {
    json!({
        "type": "control_response",
        "response": {
            "subtype": "success",
            "request_id": request_id,
            "response": { "behavior": "deny", "message": message },
        },
    })
}

/// The answer to the CLI's question before a fetch
/// ([`WebTools::hello`]): with `deny`, the fetch does not run and the
/// model is told why; without, nothing is decided here and the CLI
/// goes on to its allow list.
fn fetch_answer(request_id: &str, deny: Option<&str>) -> Value {
    let said = match deny {
        Some(reason) => json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": reason,
            },
        }),
        None => json!({}),
    };
    json!({
        "type": "control_response",
        "response": { "subtype": "success", "request_id": request_id, "response": said },
    })
}

impl Drop for Process {
    fn drop(&mut self) {
        // Close stdin first: an idle CLI exits on its own once its
        // input is closed. One that is working on a turn finishes the
        // turn first, so it is killed, and on Windows everything
        // under it too: behind a `claude.cmd` shim the CLI is a
        // grandchild that a plain kill does not reach.
        drop(self.stdin.take());
        // An ask with tools: everything the CLI started goes with
        // it. The group is the CLI's own, so nothing else is in it.
        #[cfg(unix)]
        if self.own_group {
            refine_cli::kill_process_group(self.child.id());
        }
        refine_cli::end_process(&mut self.child, self.mid_turn);
        let _ = fs::remove_file(&self.prompt_file);
    }
}

/// What one line of the CLI's output means for an ask. The last
/// two only come from the reader of an ask with tools.
#[derive(Debug, PartialEq)]
#[cfg_attr(not(feature = "agent"), allow(dead_code))]
pub(crate) enum Line {
    /// A piece of the answer.
    Text(String),
    /// The model started a new message (after a search).
    NewMessage,
    /// The plan's usage windows, for the usage of this turn.
    Plan(PlanWindows),
    /// The turn is over; the whole answer as the CLI reports it.
    Result(String),
    /// Nothing an ask acts on (init, thinking, block boundaries).
    Other,
    /// Events for the sink, as they are: a web search or a read of
    /// the weather that starts or ends, and the steps of an ask with
    /// tools.
    Events(Vec<AskEvent>),
    /// Something the person should know about how the answer was
    /// made ([`AskReply::notice`]). The first one counts.
    Notice(String),
    /// The CLI asks whether a tool may run, and waits.
    Permission(Permission),
}

/// Read one stdout line. An error ends the turn: the CLI reported a
/// failure, the model reached for a tool an ask may not use, or a
/// fetch of an address an ask may not reach came back with content.
pub(crate) fn read_line(line: &str, web: &mut WebTools) -> Result<Line, AskError> {
    let Ok(v) = serde_json::from_str::<Value>(line.trim()) else {
        return Ok(Line::Other);
    };
    match v["type"].as_str() {
        Some("stream_event") => {
            let event = &v["event"];
            match event["type"].as_str() {
                Some("message_start") => Ok(Line::NewMessage),
                Some("content_block_delta") if event["delta"]["type"].as_str() == Some("text_delta") => {
                    Ok(Line::Text(event["delta"]["text"].as_str().unwrap_or("").to_string()))
                }
                // Caught here, as the block starts, and not only on
                // the `assistant` line that follows it.
                Some("content_block_start") => {
                    web.tool(&event["content_block"])?;
                    Ok(Line::Other)
                }
                _ => Ok(Line::Other),
            }
        }
        // The finished message. Its text already arrived as deltas;
        // a tool call is complete here, with its input.
        Some("assistant") => {
            let mut events = Vec::new();
            for block in v["message"]["content"].as_array().into_iter().flatten() {
                events.extend(web.called(block)?);
            }
            Ok(if events.is_empty() { Line::Other } else { Line::Events(events) })
        }
        // The tool's answer, fed back to the model.
        Some("user") => {
            let mut events = Vec::new();
            for block in v["message"]["content"].as_array().into_iter().flatten() {
                if block["type"].as_str() == Some("tool_result") {
                    events.push(web.came_back(block)?);
                }
            }
            Ok(if events.is_empty() { Line::Other } else { Line::Events(events) })
        }
        Some("rate_limit_event") => Ok(refine_cli::parse_rate_limit_line(line).map_or(Line::Other, Line::Plan)),
        Some("result") => match refine_cli::parse_result_line(line) {
            Some(Ok(text)) => Ok(Line::Result(text)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(Line::Other),
        },
        _ => Ok(Line::Other),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::AskImage;
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    /// A fake `claude`: a shell script standing in for the real CLI.
    fn fake_cli(dir: &Path, body: &str) -> ClaudeCliSettings {
        let path = dir.join("claude");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        ClaudeCliSettings { path: Some(path.display().to_string()), ..ClaudeCliSettings::default() }
    }

    /// Records its arguments and prompt file, then answers every line
    /// on stdin the way the real CLI streams: a thinking delta, two
    /// text deltas, the finished message, the plan windows, the
    /// result. The answer names the turn, so a second turn can be
    /// told from the first.
    const STREAM_OK: &str = r#"
echo "$@" > "$(dirname "$0")/args.txt"
prev=""; for a in "$@"; do if [ "$prev" = "--system-prompt-file" ]; then cp "$a" "$(dirname "$0")/system.txt"; fi; prev="$a"; done
n=0
while read -r line; do
  n=$((n+1))
  printf '%s\n' "$line" > "$(dirname "$0")/stdin-$n.txt"
  echo '{"type":"system","subtype":"init"}'
  echo '{"type":"stream_event","event":{"type":"message_start"}}'
  echo '{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}}'
  echo '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"a capital, easy"}}}'
  echo '{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}}'
  echo '{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"The capital of Norway "}}}'
  echo "{\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"is Oslo (turn $n).\"}}}"
  echo "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"The capital of Norway is Oslo (turn $n).\"}]}}"
  echo '{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","unifiedWindows":{"five_hour":{"utilization":0.09,"resetsAt":1794325200},"seven_day":{"utilization":0.21,"resetsAt":1794646800}}}}'
  echo "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"The capital of Norway is Oslo (turn $n).\",\"total_cost_usd\":0.0025,\"usage\":{\"input_tokens\":2,\"cache_creation_input_tokens\":560,\"cache_read_input_tokens\":40,\"output_tokens\":11}}"
done
"#;

    fn setup(web_search: bool) -> AskSetup {
        AskSetup {
            system: "You answer questions. Brennholt is spelled like this.".into(),
            model: "sonnet".into(),
            effort: Some(RefineEffort::Low),
            web_search,
        }
    }

    fn question() -> AskInput {
        AskInput::text("<question>\nwhat is the capital of norway\n</question>")
    }

    /// Send one turn and collect the events.
    fn send(
        cli: &mut ClaudeCliAsk,
        setup: &AskSetup,
        input: &AskInput,
        timeout: Duration,
    ) -> (Result<AskReply, AskError>, Vec<AskEvent>) {
        let mut events = Vec::new();
        let result = cli.send(setup, input, &mut |e| events.push(e), &CancelFlag::new(), timeout);
        (result, events)
    }

    const LONG: Duration = Duration::from_secs(10);

    fn wait_for(path: &Path) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !path.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        // The file exists before its contents are written.
        thread::sleep(Duration::from_millis(50));
        fs::read_to_string(path).unwrap().trim().to_string()
    }

    fn alive(pid: &str) -> bool {
        // `kill -0` fails once the process is gone.
        Command::new("kill").args(["-0", pid]).stderr(Stdio::null()).status().unwrap().success()
    }

    #[test]
    fn flags_are_refines_lean_ones_plus_streaming_and_a_pinned_permission_mode() {
        let args = spawn_args(&setup(false), Path::new("/tmp/prompt.txt"));
        let joined = args.join(" ");
        for flag in [
            "-p",
            "--safe-mode",
            "--strict-mcp-config",
            "--disable-slash-commands",
            "--no-session-persistence",
            "--input-format stream-json",
            "--output-format stream-json",
            "--verbose",
            "--model sonnet",
            "--effort low",
            "--include-partial-messages",
            "--permission-mode default",
            "--system-prompt-file /tmp/prompt.txt",
        ] {
            assert!(joined.contains(flag), "missing {flag} in {joined}");
        }
        // No tools at all: an empty tool list, nothing allowed.
        let tools = args.iter().position(|a| a == "--tools").unwrap();
        assert_eq!(args[tools + 1], "");
        assert!(!args.iter().any(|a| a == "--allowedTools"));
        assert!(!joined.contains("WebSearch") && !joined.contains("WebFetch"));
        assert!(!args.iter().any(|a| a == "--bare" || a == "--dangerously-skip-permissions"));
        // Exactly Refine's flags and the three added ones.
        let refine = refine_cli::spawn_args("sonnet", RefineEffort::Low, Path::new("/tmp/prompt.txt"));
        assert_eq!(args[..refine.len()], refine[..]);
        assert_eq!(args[refine.len()..], ["--include-partial-messages", "--permission-mode", "default"]);
    }

    /// Load-bearing for Vision (TASK-98): an ask can carry a
    /// screenshot, and a screenshot must never be written to disk.
    /// Without `--no-session-persistence` the Claude CLI keeps every
    /// conversation in a session file under its config folder, the
    /// image included (base64, in full), and also drops a temporary
    /// PNG of it. With the flag neither was found on disk (checked
    /// by a run against the real CLI, TASK-98 notes). The flag is one
    /// of Refine's lean flags and arrives here through
    /// `refine_cli::spawn_args`; this test is here so that nobody
    /// takes it out there, or reorders things here, without learning
    /// what it protects.
    #[test]
    fn a_conversation_that_may_carry_a_screenshot_is_never_persisted_by_the_cli() {
        for web_search in [false, true] {
            for effort in [None, Some(RefineEffort::Low), Some(RefineEffort::High)] {
                let setup = AskSetup { effort, ..setup(web_search) };
                let args = spawn_args(&setup, Path::new("/tmp/prompt.txt"));
                assert_eq!(
                    args.iter().filter(|a| *a == "--no-session-persistence").count(),
                    1,
                    "--no-session-persistence must be passed, once: {args:?}"
                );
                // Nothing that would bring a session back in.
                for flag in ["--resume", "--continue", "-c", "-r", "--session-id", "--fork-session"] {
                    assert!(!args.iter().any(|a| a == flag), "{flag} in {args:?}");
                }
            }
        }
        // And an image really is sent the way that flag covers: as a
        // block on stdin, never as a path to a file of ours.
        let line = user_line(&AskInput {
            text: "what is this".into(),
            images: vec![AskImage { media_type: "image/png".into(), data: vec![1, 2, 3] }],
        });
        let sent: Value = serde_json::from_str(&line).unwrap();
        let block = &sent["message"]["content"][0];
        assert_eq!((block["type"].as_str(), block["source"]["type"].as_str()), (Some("image"), Some("base64")));
        assert_eq!(block["source"]["data"], "AQID");
        assert!(!line.contains("/tmp") && !line.contains("file://"));
    }

    #[test]
    fn web_search_adds_exactly_the_two_web_tools_and_their_allow_list() {
        let plain = spawn_args(&setup(false), Path::new("/tmp/p.txt"));
        let web = spawn_args(&setup(true), Path::new("/tmp/p.txt"));
        let tools = web.iter().position(|a| a == "--tools").unwrap();
        assert_eq!(
            web[tools..tools + 4],
            [
                "--tools",
                "WebSearch,WebFetch",
                "--allowedTools",
                "WebSearch,WebFetch(domain:api.open-meteo.com),WebFetch(domain:geocoding-api.open-meteo.com)",
            ]
        );
        // Each list is followed by a flag, so nothing after it can be
        // read as one more tool.
        assert!(web[tools + 4].starts_with("--"), "{web:?}");
        // Nothing else differs.
        let mut without = web.clone();
        without.splice(tools + 1..tools + 4, [String::new()]);
        assert_eq!(without, plain);
        // The fetch tool is named in those two arguments and nowhere
        // else, and the permission mode is still pinned.
        assert_eq!(web.iter().filter(|a| a.contains("WebFetch")).count(), 2);
        assert_eq!(web.iter().filter(|a| a.contains("WebSearch")).count(), 2);
        assert!(web.join(" ").contains("--permission-mode default"));
        assert!(!web.iter().any(|a| a == "--disallowedTools" || a == "--dangerously-skip-permissions"));
    }

    /// The entries of an allow list, as the CLI takes them apart: at
    /// every comma that is not inside brackets.
    fn allow_entries(list: &str) -> Vec<String> {
        let (mut entries, mut entry, mut depth) = (Vec::new(), String::new(), 0i32);
        for c in list.chars() {
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                ',' if depth == 0 => {
                    entries.push(std::mem::take(&mut entry));
                    continue;
                }
                _ => {}
            }
            entry.push(c);
        }
        entries.push(entry);
        entries
    }

    #[test]
    fn the_fetch_tool_is_only_ever_allowed_for_the_weather_hosts() {
        // The whole of what the web puts on an allow list.
        assert_eq!(
            web_allow_rules(),
            ["WebSearch", "WebFetch(domain:api.open-meteo.com)", "WebFetch(domain:geocoding-api.open-meteo.com)"]
        );
        assert_eq!(web_tools(), "WebSearch,WebFetch");
        for effort in [None, Some(RefineEffort::Minimal), Some(RefineEffort::High)] {
            let args = spawn_args(&AskSetup { effort, ..setup(true) }, Path::new("/tmp/p.txt"));
            let allowed = &args[args.iter().position(|a| a == "--allowedTools").unwrap() + 1];
            let entries = allow_entries(allowed);
            let fetch: Vec<&String> = entries.iter().filter(|entry| entry.contains("Fetch")).collect();
            // One entry per host, each exactly `WebFetch(domain:HOST)`.
            assert_eq!(fetch.len(), weather::HOSTS.len(), "{allowed}");
            for host in weather::HOSTS {
                assert!(fetch.contains(&&format!("WebFetch(domain:{host})")), "{host} is missing in {allowed}");
            }
            // Never the bare tool, which allows every address, never
            // a pattern, and no rule for another host.
            for entry in &entries {
                assert_ne!(entry, "WebFetch", "{allowed}");
                assert!(!entry.contains('*') && !entry.contains(' '), "{entry}");
                assert!(entry == "WebSearch" || fetch.contains(&entry), "{entry} in {allowed}");
            }
            assert_eq!(entries.len(), 1 + weather::HOSTS.len(), "{allowed}");
        }
        // With web search off: no fetch tool, no search, no allow
        // list at all.
        for effort in [None, Some(RefineEffort::Low), Some(RefineEffort::High)] {
            let args = spawn_args(&AskSetup { effort, ..setup(false) }, Path::new("/tmp/p.txt"));
            assert!(!args.iter().any(|a| a.contains("WebFetch") || a.contains("WebSearch") || a.contains("open-meteo")), "{args:?}");
            assert!(!args.iter().any(|a| a == "--allowedTools"));
            assert_eq!(args[args.iter().position(|a| a == "--tools").unwrap() + 1], "");
        }
    }

    #[test]
    fn the_cli_never_gets_an_effort_it_does_not_have() {
        let mut s = setup(false);
        s.effort = Some(RefineEffort::Minimal);
        assert!(spawn_args(&s, Path::new("/tmp/p.txt")).join(" ").contains("--effort low"));
        s.effort = None;
        assert!(spawn_args(&s, Path::new("/tmp/p.txt")).join(" ").contains("--effort low"));
        s.effort = Some(RefineEffort::High);
        assert!(spawn_args(&s, Path::new("/tmp/p.txt")).join(" ").contains("--effort high"));
    }

    #[test]
    fn streams_the_text_in_order_and_reports_first_text_and_usage() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), STREAM_OK));
        let setup = setup(false);
        let (reply, events) = send(&mut cli, &setup, &question(), LONG);
        let reply = reply.unwrap();
        // Two text pieces, in order; the thinking never shows.
        assert_eq!(
            events,
            vec![
                AskEvent::Text("The capital of Norway ".into()),
                AskEvent::Text("is Oslo (turn 1).".into()),
            ]
        );
        assert_eq!(reply.text, "The capital of Norway is Oslo (turn 1).");
        let first = reply.first_text_ms.expect("first text time");
        assert!(first <= reply.elapsed_ms, "{first} > {}", reply.elapsed_ms);
        assert_eq!(reply.warm_lead_ms, None, "this call ran cold");
        let usage = reply.usage.unwrap();
        assert_eq!((usage.tokens_in, usage.tokens_cached, usage.tokens_out), (602, 40, 11));
        assert_eq!(usage.cost_usd, Some(0.0025));
        let plan = usage.plan.unwrap();
        assert_eq!(plan.five_hour_used, Some(0.09));
        assert_eq!(plan.seven_day_resets_at, Some(1794646800));

        // The question went in as one user line, as given.
        let sent: Value = serde_json::from_str(&fs::read_to_string(dir.path().join("stdin-1.txt")).unwrap()).unwrap();
        assert_eq!(sent["type"], "user");
        assert_eq!(sent["message"]["content"][0]["text"], "<question>\nwhat is the capital of norway\n</question>");
        // The system prompt went in through the prompt file, and the
        // process was started with the permission mode pinned.
        assert_eq!(fs::read_to_string(dir.path().join("system.txt")).unwrap(), setup.system);
        let args = fs::read_to_string(dir.path().join("args.txt")).unwrap();
        assert!(args.contains("--permission-mode default"), "{args}");
        assert!(args.contains("--include-partial-messages"), "{args}");
        assert!(!args.contains("Brennholt"), "the prompt must not ride on the command line: {args}");
        assert!(!args.contains("WebSearch") && !args.contains("WebFetch"), "{args}");
    }

    #[test]
    fn web_search_reaches_the_command_line_as_the_two_lists() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), STREAM_OK));
        send(&mut cli, &setup(true), &question(), LONG).0.unwrap();
        let args = fs::read_to_string(dir.path().join("args.txt")).unwrap();
        assert!(
            args.contains(
                "--tools WebSearch,WebFetch --allowedTools \
WebSearch,WebFetch(domain:api.open-meteo.com),WebFetch(domain:geocoding-api.open-meteo.com) --strict-mcp-config"
            ),
            "{args}"
        );
        assert!(args.contains("--permission-mode default"), "{args}");
    }

    #[test]
    fn a_warmed_process_answers_and_reports_its_head_start() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), STREAM_OK));
        let setup = setup(false);
        cli.warm(&setup);
        assert!(cli.process.is_some());
        // Up and waiting on stdin while the person talks.
        thread::sleep(Duration::from_millis(150));
        assert!(!dir.path().join("stdin-1.txt").exists());
        let reply = send(&mut cli, &setup, &question(), LONG).0.unwrap();
        assert_eq!(reply.text, "The capital of Norway is Oslo (turn 1).");
        assert!(reply.warm_lead_ms.unwrap() >= 150);
    }

    #[test]
    fn a_second_turn_goes_to_the_same_process() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), STREAM_OK));
        let setup = setup(false);
        let first = send(&mut cli, &setup, &question(), LONG).0.unwrap();
        assert_eq!(first.text, "The capital of Norway is Oslo (turn 1).");
        let (second, events) = send(&mut cli, &setup, &AskInput::text("and of sweden"), LONG);
        // The fake counts turns per process: turn 2 means the same
        // process took the second line.
        let second = second.unwrap();
        assert_eq!(second.text, "The capital of Norway is Oslo (turn 2).");
        assert_eq!(events.len(), 2);
        assert_eq!(second.warm_lead_ms, None, "only a first turn can have had a head start");
        let sent: Value = serde_json::from_str(&fs::read_to_string(dir.path().join("stdin-2.txt")).unwrap()).unwrap();
        assert_eq!(sent["message"]["content"][0]["text"], "and of sweden");
    }

    #[test]
    fn a_changed_setup_starts_a_new_process() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), STREAM_OK));
        cli.warm(&setup(false));
        let mut changed = setup(false);
        changed.system = "New instructions.".into();
        let reply = send(&mut cli, &changed, &question(), LONG).0.unwrap();
        assert_eq!(reply.text, "The capital of Norway is Oslo (turn 1).");
        assert_eq!(reply.warm_lead_ms, None);
        assert_eq!(fs::read_to_string(dir.path().join("system.txt")).unwrap(), "New instructions.");
    }

    #[test]
    fn an_image_travels_as_a_base64_block_before_the_text() {
        let input = AskInput {
            text: "what is this".into(),
            images: vec![AskImage { media_type: "image/png".into(), data: vec![0x89, b'P', b'N', b'G'] }],
        };
        let line: Value = serde_json::from_str(&user_line(&input)).unwrap();
        let content = line["message"]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "image");
        assert_eq!(content[0]["source"]["type"], "base64");
        assert_eq!(content[0]["source"]["media_type"], "image/png");
        assert_eq!(content[0]["source"]["data"], "iVBORw==");
        assert_eq!(content[1], json!({ "type": "text", "text": "what is this" }));
        assert_eq!(line["message"]["role"], "user");
        // One line: the CLI reads its input line by line.
        assert!(!user_line(&input).contains('\n'));
        // And the bytes never show in a debug print.
        assert_eq!(format!("{:?}", input.images[0]), "AskImage(image/png, 4 bytes)");
    }

    /// A turn with a web search in it: the tool call, its result, then
    /// the answer in a second message.
    const SEARCHES: &str = r#"
read -r line
echo '{"type":"stream_event","event":{"type":"message_start"}}'
echo '{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"WebSearch","input":{}}}}'
echo '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"query\": \"ferry"}}}'
echo '{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"WebSearch","input":{"query":"ferry timetable Brennholt","mode":"standard"}}]}}'
echo '{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"Web search results for query"}]}}'
echo '{"type":"stream_event","event":{"type":"message_start"}}'
echo '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"The first ferry leaves at 06:40."}}}'
echo '{"type":"result","subtype":"success","is_error":false,"result":"The first ferry leaves at 06:40."}'
"#;

    #[test]
    fn a_web_search_is_reported_with_its_query_and_its_end() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), SEARCHES));
        let (reply, events) = send(&mut cli, &setup(true), &question(), LONG);
        assert_eq!(
            events,
            vec![
                AskEvent::Searching { query: "ferry timetable Brennholt".into() },
                AskEvent::SearchDone,
                AskEvent::Text("The first ferry leaves at 06:40.".into()),
            ]
        );
        assert_eq!(reply.unwrap().text, "The first ferry leaves at 06:40.");
    }

    #[test]
    fn text_before_and_after_a_search_is_two_paragraphs() {
        let dir = tempfile::tempdir().unwrap();
        let body = SEARCHES.replace(
            r#"echo '{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use""#,
            r#"echo '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Let me check."}}}'
echo '{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use""#,
        );
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), &body));
        let (reply, _) = send(&mut cli, &setup(true), &question(), LONG);
        assert_eq!(reply.unwrap().text, "Let me check.\n\nThe first ferry leaves at 06:40.");
    }

    #[test]
    fn any_tool_but_the_web_search_fails_the_ask() {
        // With web search off, the web search is such a tool too.
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), SEARCHES));
        let (reply, events) = send(&mut cli, &setup(false), &question(), LONG);
        match reply.unwrap_err() {
            AskError::Backend(m) => assert!(m.contains("WebSearch") && m.contains("may not use tools"), "{m}"),
            other => panic!("expected Backend, got {other:?}"),
        }
        assert!(events.is_empty(), "{events:?}");
        // The conversation is over: the process is not kept.
        assert!(cli.process.is_none());

        // With web search on, any other tool still fails.
        let dir = tempfile::tempdir().unwrap();
        let body = SEARCHES.replace("WebSearch", "Bash");
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), &body));
        let (reply, _) = send(&mut cli, &setup(true), &question(), LONG);
        match reply.unwrap_err() {
            AskError::Backend(m) => assert!(m.contains("Bash"), "{m}"),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn a_tool_call_is_caught_on_either_line_that_shows_it() {
        let on = || WebTools::new(true);
        let start = r#"{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"tool_use","name":"Write","input":{}}}}"#;
        assert!(matches!(read_line(start, &mut on()), Err(AskError::Backend(_))));
        let done = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"ok"},{"type":"tool_use","name":"Bash","input":{"command":"ls"}}]}}"#;
        assert!(matches!(read_line(done, &mut on()), Err(AskError::Backend(_))));
        let server = r#"{"type":"assistant","message":{"content":[{"type":"server_tool_use","name":"web_fetch","input":{}}]}}"#;
        assert!(matches!(read_line(server, &mut on()), Err(AskError::Backend(_))));
        // Text and thinking blocks are not tools.
        let off = || WebTools::new(false);
        let text = r#"{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"text","text":""}}}"#;
        assert_eq!(read_line(text, &mut off()), Ok(Line::Other));
        let thinking = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"hm"}}}"#;
        assert_eq!(read_line(thinking, &mut off()), Ok(Line::Other));
        assert_eq!(read_line("not json", &mut off()), Ok(Line::Other));
        // The fetch tool starts as a block like any other tool: with
        // web search on that is no error, with it off it is.
        let fetch = r#"{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"tool_use","id":"toolu_f","name":"WebFetch","input":{}}}}"#;
        assert_eq!(read_line(fetch, &mut on()), Ok(Line::Other));
        match read_line(fetch, &mut off()) {
            Err(AskError::Backend(m)) => assert!(m.contains("WebFetch") && m.contains("may not use tools"), "{m}"),
            other => panic!("{other:?}"),
        }
        // A name is shown on one line, whatever it holds.
        let odd = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Evil\nTool","input":{}}]}}"#;
        match read_line(odd, &mut on()) {
            Err(AskError::Backend(m)) => assert!(!m.contains('\n'), "{m}"),
            other => panic!("{other:?}"),
        }
    }

    /// The check before a fetch, as Claude Code 2.1.325 sends it.
    fn check(request_id: &str, callback: &str, tool: &str, url: &str, tool_use_id: &str) -> String {
        json!({
            "type": "control_request",
            "request_id": request_id,
            "request": {
                "subtype": "hook_callback",
                "callback_id": callback,
                "input": {
                    "hook_event_name": "PreToolUse",
                    "permission_mode": "default",
                    "tool_name": tool,
                    "tool_input": { "url": url, "prompt": "the title" },
                    "tool_use_id": tool_use_id,
                },
                "tool_use_id": tool_use_id,
            },
        })
        .to_string()
    }

    /// A weather ask as the real CLI streams it: it takes the hello,
    /// then the question; the place is turned into coordinates, the
    /// forecast is read (each after a check), then the answer.
    fn reads_the_weather() -> String {
        let geo = "https://geocoding-api.open-meteo.com/v1/search?name=Oslo&count=1";
        let now = "https://api.open-meteo.com/v1/forecast?latitude=59.91&longitude=10.75&current=temperature_2m";
        format!(
            r#"
here="$(dirname "$0")"
read -r hello; printf '%s\n' "$hello" > "$here/hello.txt"
read -r line; printf '%s\n' "$line" > "$here/question.txt"
echo '{{"type":"control_response","response":{{"subtype":"success","request_id":"openwhisper-hello","response":{{"commands":[]}}}}}}'
echo '{{"type":"stream_event","event":{{"type":"message_start"}}}}'
echo '{{"type":"stream_event","event":{{"type":"content_block_start","index":0,"content_block":{{"type":"tool_use","id":"toolu_geo","name":"WebFetch","input":{{}}}}}}}}'
echo '{{"type":"assistant","message":{{"content":[{{"type":"tool_use","id":"toolu_geo","name":"WebFetch","input":{{"url":"{geo}","prompt":"coordinates"}}}}]}}}}'
echo '{check_geo}'
read -r answer; printf '%s\n' "$answer" > "$here/answer-1.txt"
echo '{{"type":"user","message":{{"role":"user","content":[{{"tool_use_id":"toolu_geo","type":"tool_result","content":"latitude 59.91, longitude 10.75"}}]}}}}'
echo '{{"type":"stream_event","event":{{"type":"message_start"}}}}'
echo '{{"type":"assistant","message":{{"content":[{{"type":"tool_use","id":"toolu_now","name":"WebFetch","input":{{"url":"{now}","prompt":"the weather now"}}}}]}}}}'
echo '{check_now}'
read -r answer; printf '%s\n' "$answer" > "$here/answer-2.txt"
echo '{{"type":"user","message":{{"role":"user","content":[{{"tool_use_id":"toolu_now","type":"tool_result","content":"3.2 C, overcast"}}]}}}}'
echo '{{"type":"stream_event","event":{{"type":"message_start"}}}}'
echo '{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"In Oslo it is 3 degrees and overcast (Open-Meteo)."}}}}}}'
echo '{{"type":"result","subtype":"success","is_error":false,"result":"In Oslo it is 3 degrees and overcast (Open-Meteo)."}}'
"#,
            check_geo = check("req-geo", FETCH_CHECK, "WebFetch", geo, "toolu_geo"),
            check_now = check("req-now", FETCH_CHECK, "WebFetch", now, "toolu_now"),
        )
    }

    fn json_file(path: &Path) -> Value {
        serde_json::from_str(&wait_for(path)).unwrap()
    }

    #[test]
    fn a_read_of_the_weather_is_shown_like_a_search() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), &reads_the_weather()));
        let (reply, events) = send(&mut cli, &setup(true), &question(), LONG);
        let reading = || AskEvent::Searching { query: "weather data from open-meteo.com".into() };
        assert_eq!(
            events,
            vec![
                reading(),
                AskEvent::SearchDone,
                reading(),
                AskEvent::SearchDone,
                AskEvent::Text("In Oslo it is 3 degrees and overcast (Open-Meteo).".into()),
            ]
        );
        assert_eq!(reply.unwrap().text, "In Oslo it is 3 degrees and overcast (Open-Meteo).");
        // The address itself is never shown: only where it leads.
        assert!(!format!("{events:?}").contains("latitude"));
        // Both checks were answered, and neither decided anything:
        // a weather host is left to the allow list.
        for (file, id) in [("answer-1.txt", "req-geo"), ("answer-2.txt", "req-now")] {
            let answer = json_file(&dir.path().join(file));
            assert_eq!(answer["type"], "control_response", "{file}");
            assert_eq!(answer["response"], json!({ "subtype": "success", "request_id": id, "response": {} }), "{file}");
        }
    }

    #[test]
    fn the_cli_is_told_to_check_before_every_fetch_only_while_web_search_is_on() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), &reads_the_weather()));
        send(&mut cli, &setup(true), &question(), LONG).0.unwrap();
        // The first line the CLI gets: the check, for the fetch tool
        // and nothing else, and a check that fails counts as a no.
        assert_eq!(
            json_file(&dir.path().join("hello.txt")),
            json!({
                "type": "control_request",
                "request_id": "openwhisper-hello",
                "request": {
                    "subtype": "initialize",
                    "hooks": {
                        "PreToolUse": [{ "matcher": "WebFetch", "hookCallbackIds": ["openwhisper-fetch"], "onFailure": "block" }],
                    },
                },
            })
        );
        // Then the question, as always.
        let sent = json_file(&dir.path().join("question.txt"));
        assert_eq!(sent["message"]["content"][0]["text"], "<question>\nwhat is the capital of norway\n</question>");
        assert_eq!(WebTools::hello(true).as_deref(), Some(fs::read_to_string(dir.path().join("hello.txt")).unwrap().trim()));

        // With web search off there is no fetch tool, and the CLI is
        // told nothing: the first line it gets is the question
        // (`streams_the_text_in_order_and_reports_first_text_and_usage`
        // reads it back).
        assert_eq!(WebTools::hello(false), None);
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), STREAM_OK));
        send(&mut cli, &setup(false), &question(), LONG).0.unwrap();
        let first = json_file(&dir.path().join("stdin-1.txt"));
        assert_eq!(first["type"], "user");
        assert!(!dir.path().join("stdin-2.txt").exists());
    }

    #[test]
    fn with_web_search_off_the_fetch_tool_is_a_tool_like_any_other() {
        let dir = tempfile::tempdir().unwrap();
        let body = SEARCHES.replace("WebSearch", "WebFetch");
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), &body));
        let (reply, events) = send(&mut cli, &setup(false), &question(), LONG);
        match reply.unwrap_err() {
            AskError::Backend(m) => assert!(m.contains("WebFetch") && m.contains("may not use tools"), "{m}"),
            other => panic!("expected Backend, got {other:?}"),
        }
        assert!(events.is_empty() && cli.process.is_none(), "{events:?}");
        // And a check that arrives all the same is not answered with
        // a yes: this ask asked for none.
        let mut off = WebTools::new(false);
        let asked = off.asked(&check("req-1", FETCH_CHECK, "WebFetch", "https://api.open-meteo.com/v1/forecast", "toolu_1"));
        assert_eq!(asked, Some(Permission { request_id: "req-1".into(), verdict: Verdict::Unsupported }));
    }

    /// The model reaches for another address. `CHECK` is the check
    /// the CLI makes first (empty for a CLI that makes none), `RESULT`
    /// what comes back for the call.
    const FETCHES_ELSEWHERE: &str = r#"
here="$(dirname "$0")"
echo $$ > "$here/pid.txt"
read -r hello
read -r line
echo '{"type":"stream_event","event":{"type":"message_start"}}'
echo '{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_x","name":"WebFetch","input":{}}}}'
echo '{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_x","name":"WebFetch","input":{"url":"URL","prompt":"the title"}}]}}'
CHECK
echo 'RESULT'
echo '{"type":"stream_event","event":{"type":"message_start"}}'
echo '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"I cannot read that page from here."}}}'
echo '{"type":"result","subtype":"success","is_error":false,"result":"I cannot read that page from here."}'
sleep 30
"#;

    /// What the CLI feeds back after a no from the check, and after
    /// its own allow list said no (the shapes Claude Code 2.1.325
    /// prints; the second one without its apostrophe).
    const STOPPED_BY_THE_CHECK: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"PreToolUse:WebFetch hook error: Nothing was fetched.","is_error":true,"tool_use_id":"toolu_x"}]}}"#;
    const REFUSED: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"Claude requested permissions to use WebFetch, but you have not granted it yet.","is_error":true,"tool_use_id":"toolu_x"}]},"tool_result_meta":[{"id":"toolu_x","non_execution_kind":"user-rejected"}]}"#;
    const CAME_BACK: &str = r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_x","type":"tool_result","content":"Example Domain"}]}}"#;

    fn fetches(url: &str, with_check: bool, result: &str) -> String {
        let check = if with_check {
            format!(
                "echo '{}'\nread -r answer; printf '%s\\n' \"$answer\" > \"$here/answer.txt\"",
                check("req-x", FETCH_CHECK, "WebFetch", url, "toolu_x")
            )
        } else {
            String::new()
        };
        FETCHES_ELSEWHERE.replace("URL", url).replace("CHECK", &check).replace("RESULT", result)
    }

    const OTHER_ADDRESSES: [(&str, &str); 6] = [
        ("https://example.com/?q=hello", "example.com"),
        ("https://docs.example.com/guide", "docs.example.com"),
        ("https://api.open-meteo.com.example.com/v1/forecast", "api.open-meteo.com.example.com"),
        ("https://api.open-meteo.com@example.com/", "example.com"),
        ("http://api.open-meteo.com/v1/forecast", "api.open-meteo.com"),
        ("not an address", "an address that could not be read"),
    ];

    #[test]
    fn a_fetch_of_another_address_is_stopped_by_the_check_before_it_runs() {
        for (url, host) in OTHER_ADDRESSES {
            let dir = tempfile::tempdir().unwrap();
            let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), &fetches(url, true, STOPPED_BY_THE_CHECK)));
            let (reply, events) = send(&mut cli, &setup(true), &question(), LONG);
            // Never shown as a search: nothing was read.
            assert_eq!(
                events,
                vec![
                    AskEvent::FetchRefused { host: host.to_string() },
                    AskEvent::Text("I cannot read that page from here.".into()),
                ],
                "{url}"
            );
            assert_eq!(reply.unwrap().text, "I cannot read that page from here.", "{url}");
            // The no, as the CLI takes it: the tool does not run, and
            // the model is told why.
            let answer = json_file(&dir.path().join("answer.txt"));
            assert_eq!(answer["response"]["subtype"], "success", "{url}");
            assert_eq!(answer["response"]["request_id"], "req-x", "{url}");
            assert_eq!(
                answer["response"]["response"],
                json!({
                    "hookSpecificOutput": {
                        "hookEventName": "PreToolUse",
                        "permissionDecision": "deny",
                        "permissionDecisionReason": FETCH_ELSEWHERE_DENIED,
                    },
                }),
                "{url}"
            );
        }
        assert!(FETCH_ELSEWHERE_DENIED.contains("api.open-meteo.com and geocoding-api.open-meteo.com"));
    }

    #[test]
    fn a_check_is_a_no_for_everything_but_a_fetch_of_a_weather_host() {
        let mut web = WebTools::new(true);
        let verdict = |web: &mut WebTools, line: &str| web.asked(line).map(|permission| permission.verdict);
        let no = Some(Verdict::Fetch { deny: Some(FETCH_ELSEWHERE_DENIED.to_string()), events: Vec::new() });
        let yes = Some(Verdict::Fetch { deny: None, events: Vec::new() });
        for host in weather::HOSTS {
            let url = format!("https://{host}/v1/x?name=Oslo");
            assert_eq!(verdict(&mut web, &check("r", FETCH_CHECK, "WebFetch", &url, "t")), yes, "{url}");
        }
        for (url, _) in OTHER_ADDRESSES {
            assert_eq!(verdict(&mut web, &check("r", FETCH_CHECK, "WebFetch", url, "t")), no, "{url}");
        }
        // Another tool under this check, with an address that would
        // be fine for a fetch.
        let fine = "https://api.open-meteo.com/v1/forecast";
        assert_eq!(verdict(&mut web, &check("r", FETCH_CHECK, "Bash", fine, "t")), no);
        // A check this app never asked for, and any other question:
        // not answered with a yes or a no.
        assert_eq!(verdict(&mut web, &check("r", "someone-elses", "WebFetch", fine, "t")), Some(Verdict::Unsupported));
        let other = r#"{"type":"control_request","request_id":"r","request":{"subtype":"can_use_tool","tool_name":"WebFetch","input":{"url":"https://api.open-meteo.com/"}}}"#;
        assert_eq!(verdict(&mut web, other), Some(Verdict::Unsupported));
        // Nothing to answer to, and lines that are no question.
        assert_eq!(verdict(&mut web, &check("", FETCH_CHECK, "WebFetch", fine, "t")), None);
        assert_eq!(verdict(&mut web, r#"{"type":"assistant","message":{"content":[{"type":"text","text":"a \"control_request\" is a line"}]}}"#), None);
        assert_eq!(verdict(&mut web, "not json \"control_request\""), None);
    }

    #[test]
    fn a_fetch_of_another_address_is_refused_by_the_allow_list_when_no_check_is_made() {
        // A CLI that does not check first still has the allow list:
        // the address is not on it.
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), &fetches("https://example.com/?q=hello", false, REFUSED)));
        let (reply, events) = send(&mut cli, &setup(true), &question(), LONG);
        assert_eq!(
            events,
            vec![
                AskEvent::FetchRefused { host: "example.com".into() },
                AskEvent::Text("I cannot read that page from here.".into()),
            ]
        );
        assert_eq!(reply.unwrap().text, "I cannot read that page from here.");
    }

    #[test]
    fn a_fetch_of_another_address_that_comes_back_ends_the_ask() {
        // Neither the check nor the allow list stopped it. With
        // these flags that cannot happen, so if it does, they no
        // longer mean what this file assumes, and nothing made from
        // that page is shown.
        for with_check in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let body = fetches("https://docs.example.com/guide?note=hello", with_check, CAME_BACK);
            let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), &body));
            cli.warm(&setup(true));
            let pid = wait_for(&dir.path().join("pid.txt"));
            let (reply, events) = send(&mut cli, &setup(true), &question(), LONG);
            match reply.unwrap_err() {
                AskError::Backend(m) => {
                    assert!(m.contains("a web address an ask may not reach (docs.example.com)"), "{m}");
                    assert!(m.contains("An ask can only read weather data from open-meteo.com."), "{m}");
                    assert!(!m.contains("note=hello") && !m.contains('\n'), "{m}");
                }
                other => panic!("expected Backend, got {other:?}"),
            }
            // Nothing reached the sink, and the process is gone
            // before it could say more.
            assert!(events.is_empty(), "{events:?}");
            assert!(cli.process.is_none());
            assert!(!alive(&pid), "process {pid} survived");
        }
    }

    #[test]
    fn a_cli_that_does_not_stream_still_answers() {
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(
            dir.path(),
            r#"read -r line; echo '{"type":"result","subtype":"success","is_error":false,"result":" Oslo. "}'"#,
        );
        let (reply, events) = send(&mut ClaudeCliAsk::new(settings), &setup(false), &question(), LONG);
        let reply = reply.unwrap();
        assert_eq!(reply.text, "Oslo.");
        assert_eq!(events, vec![AskEvent::Text(" Oslo. ".into())]);
        assert_eq!(reply.usage, None);
    }

    #[test]
    fn cancel_kills_the_process_while_it_is_silent() {
        let dir = tempfile::tempdir().unwrap();
        // Takes the question, then says nothing for a long time.
        let settings = fake_cli(dir.path(), r#"echo $$ > "$(dirname "$0")/pid.txt"; read -r line; sleep 30"#);
        let mut cli = ClaudeCliAsk::new(settings);
        // Up and running before the question goes in (the first run
        // of a new script can take a while to start on macOS).
        cli.warm(&setup(false));
        let pid = wait_for(&dir.path().join("pid.txt"));
        assert!(alive(&pid));
        let cancel = CancelFlag::new();
        let canceller = {
            let cancel = cancel.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(300));
                cancel.cancel();
            })
        };
        let started = Instant::now();
        let err = cli.send(&setup(false), &question(), &mut |_| {}, &cancel, Duration::from_secs(20)).unwrap_err();
        canceller.join().unwrap();
        assert_eq!(err, AskError::Cancelled);
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
        assert!(!alive(&pid), "process {pid} survived the cancel");
        assert!(cli.process.is_none());
    }

    /// A question far larger than a pipe holds, for a CLI that never
    /// reads it.
    fn huge_question() -> AskInput {
        AskInput {
            text: "what is on this screen".into(),
            images: vec![AskImage { media_type: "image/png".into(), data: vec![7u8; 2 * 1024 * 1024] }],
        }
    }

    /// Never reads its input, and stays for half a minute.
    const DEAF: &str = r#"echo $$ > "$(dirname "$0")/pid.txt"; sleep 30"#;

    #[test]
    fn a_question_the_cli_does_not_read_can_still_be_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), DEAF));
        cli.warm(&setup(false));
        let pid = wait_for(&dir.path().join("pid.txt"));
        let cancel = CancelFlag::new();
        let canceller = {
            let cancel = cancel.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(300));
                cancel.cancel();
            })
        };
        let started = Instant::now();
        let err = cli.send(&setup(false), &huge_question(), &mut |_| {}, &cancel, Duration::from_secs(20)).unwrap_err();
        canceller.join().unwrap();
        assert_eq!(err, AskError::Cancelled);
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
        assert!(!alive(&pid), "process {pid} survived the cancel");
    }

    #[test]
    fn a_question_the_cli_does_not_read_still_times_out() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), DEAF));
        cli.warm(&setup(false));
        let pid = wait_for(&dir.path().join("pid.txt"));
        let started = Instant::now();
        let (reply, _) = send(&mut cli, &setup(false), &huge_question(), Duration::from_millis(300));
        assert_eq!(reply.unwrap_err(), AskError::Timeout);
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
        assert!(!alive(&pid), "process {pid} survived the timeout");
    }

    #[test]
    fn a_large_question_reaches_a_cli_that_reads_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), STREAM_OK));
        let reply = send(&mut cli, &setup(false), &huge_question(), LONG).0.unwrap();
        assert_eq!(reply.text, "The capital of Norway is Oslo (turn 1).");
        // All of it arrived, and stdin is back for a further turn.
        let sent = fs::read_to_string(dir.path().join("stdin-1.txt")).unwrap();
        assert!(sent.len() > 2 * 1024 * 1024, "{}", sent.len());
        let again = send(&mut cli, &setup(false), &AskInput::text("and of sweden"), LONG).0.unwrap();
        assert_eq!(again.text, "The capital of Norway is Oslo (turn 2).");
    }

    #[test]
    fn a_slow_cli_times_out_and_is_killed() {
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(dir.path(), r#"echo $$ > "$(dirname "$0")/pid.txt"; read -r line; sleep 30"#);
        let mut cli = ClaudeCliAsk::new(settings);
        cli.warm(&setup(false));
        let pid = wait_for(&dir.path().join("pid.txt"));
        assert!(alive(&pid));
        let started = Instant::now();
        let (reply, _) = send(&mut cli, &setup(false), &question(), Duration::from_millis(300));
        assert_eq!(reply.unwrap_err(), AskError::Timeout);
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
        assert!(!alive(&pid), "process {pid} survived the timeout");
        assert!(cli.process.is_none());
    }

    #[test]
    fn dropping_the_backend_ends_the_conversation() {
        let dir = tempfile::tempdir().unwrap();
        let body = format!("echo $$ > \"$(dirname \"$0\")/pid.txt\"\n{STREAM_OK}");
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), &body));
        send(&mut cli, &setup(false), &question(), LONG).0.unwrap();
        let pid = wait_for(&dir.path().join("pid.txt"));
        // Still there, waiting for a further turn.
        assert!(alive(&pid));
        let args = fs::read_to_string(dir.path().join("args.txt")).unwrap();
        let prompt_file = args.split_whitespace().skip_while(|a| *a != "--system-prompt-file").nth(1).unwrap().to_string();
        assert!(Path::new(&prompt_file).exists());
        drop(cli);
        assert!(!alive(&pid), "process {pid} survived the drop");
        assert!(!Path::new(&prompt_file).exists(), "{prompt_file} was left behind");
    }

    #[test]
    fn errors_read_like_refines() {
        // An error result.
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(
            dir.path(),
            r#"read -r line; echo '{"type":"result","subtype":"success","is_error":true,"result":"API Error: 529 overloaded"}'"#,
        );
        let (reply, _) = send(&mut ClaudeCliAsk::new(settings), &setup(false), &question(), LONG);
        assert_eq!(reply.unwrap_err(), AskError::Backend("Claude Code: API Error: 529 overloaded".into()));

        // Signed out.
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(
            dir.path(),
            r#"read -r line; echo '{"type":"result","subtype":"success","is_error":true,"result":"Invalid API key · Please run /login"}'"#,
        );
        let (reply, _) = send(&mut ClaudeCliAsk::new(settings), &setup(false), &question(), LONG);
        assert_eq!(reply.unwrap_err(), AskError::Unavailable(refine_cli::NOT_SIGNED_IN.into()));

        // Exit without a result: what it wrote to stderr.
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(dir.path(), "echo \"error: unknown option '--permission-mode'\" >&2; exit 1");
        let (reply, _) = send(&mut ClaudeCliAsk::new(settings), &setup(false), &question(), LONG);
        assert_eq!(
            reply.unwrap_err(),
            AskError::Backend("Claude Code: error: unknown option '--permission-mode'".into())
        );

        // Not installed.
        let settings = ClaudeCliSettings { path: Some("/nonexistent/claude".into()), ..ClaudeCliSettings::default() };
        let (reply, _) = send(&mut ClaudeCliAsk::new(settings), &setup(false), &question(), LONG);
        assert_eq!(reply.unwrap_err(), AskError::Unavailable(refine_cli::NOT_INSTALLED.into()));
    }
}

/// The same checks against a fake `claude.cmd`, for the Windows box.
///
/// Written on macOS and compile-checked for `x86_64-pc-windows-gnu`,
/// but not yet run: the first `cargo test` on Windows is what verifies
/// streaming through a `.cmd` shim, the cancel and the kill, including
/// that the process behind the shim is gone after a cancel and after
/// a timeout ([`refine_cli::end_process`]).
#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    fn fake_cli(dir: &Path, body: &str) -> ClaudeCliSettings {
        let path = dir.join("claude.cmd");
        fs::write(&path, format!("@echo off\r\n{body}\r\n")).unwrap();
        ClaudeCliSettings { path: Some(path.display().to_string()), ..ClaudeCliSettings::default() }
    }

    const ANSWER_OK: &str = "set /p line=\r\n\
echo {\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Oslo \"}}}\r\n\
echo {\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"it is.\"}}}\r\n\
echo {\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"Oslo it is.\"}\r\n\
exit /b 0";

    fn setup() -> AskSetup {
        AskSetup { system: "You answer questions.".into(), model: "sonnet".into(), effort: None, web_search: false }
    }

    #[test]
    fn streams_through_a_cmd_shim() {
        let dir = tempfile::tempdir().unwrap();
        let mut events = Vec::new();
        let reply = ClaudeCliAsk::new(fake_cli(dir.path(), ANSWER_OK))
            .send(
                &setup(),
                &AskInput::text("what is the capital of norway"),
                &mut |e| events.push(e),
                &CancelFlag::new(),
                Duration::from_secs(10),
            )
            .unwrap();
        assert_eq!(reply.text, "Oslo it is.");
        assert_eq!(events, vec![AskEvent::Text("Oslo ".into()), AskEvent::Text("it is.".into())]);
    }

    #[test]
    fn warm_then_ask_through_a_cmd_shim() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), ANSWER_OK));
        cli.warm(&setup());
        assert!(cli.process.is_some());
        thread::sleep(Duration::from_millis(300));
        let reply = cli
            .send(&setup(), &AskInput::text("hello"), &mut |_| {}, &CancelFlag::new(), Duration::from_secs(10))
            .unwrap();
        assert_eq!(reply.text, "Oslo it is.");
        assert!(reply.warm_lead_ms.is_some());
    }

    #[test]
    fn a_slow_cmd_shim_times_out() {
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(dir.path(), "set /p line=\r\nping -n 30 127.0.0.1 >nul");
        let started = Instant::now();
        let err = ClaudeCliAsk::new(settings)
            .send(&setup(), &AskInput::text("hello"), &mut |_| {}, &CancelFlag::new(), Duration::from_millis(500))
            .unwrap_err();
        assert_eq!(err, AskError::Timeout);
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    /// A shim that takes the question and then starts a process of
    /// its own that runs for half a minute: the place the real CLI
    /// has behind `claude.cmd`. That process writes its id to
    /// `pid.txt` next to the shim.
    const SLOW_GRANDCHILD: &str = "set /p line=\r\n\
powershell -NoProfile -Command \"$PID | Out-File -Encoding ascii '%~dp0pid.txt'; Start-Sleep -Seconds 30\"";

    /// The id the grandchild wrote, once it is up.
    fn grandchild_pid(dir: &Path) -> String {
        let file = dir.join("pid.txt");
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let pid = fs::read_to_string(&file).unwrap_or_default().trim().to_string();
            if !pid.is_empty() {
                return pid;
            }
            assert!(Instant::now() < deadline, "the grandchild never started");
            thread::sleep(Duration::from_millis(50));
        }
    }

    /// `tasklist` still knows the process `pid`.
    fn alive(pid: &str) -> bool {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).contains(&format!("\"{pid}\""))
    }

    #[test]
    fn cancel_ends_the_process_behind_the_cmd_shim() {
        let dir = tempfile::tempdir().unwrap();
        let cancel = CancelFlag::new();
        // Cancel once the grandchild is up and has said who it is.
        let canceller = {
            let cancel = cancel.clone();
            let dir = dir.path().to_path_buf();
            thread::spawn(move || {
                let pid = grandchild_pid(&dir);
                assert!(alive(&pid), "the grandchild {pid} is not running");
                cancel.cancel();
                pid
            })
        };
        let err = ClaudeCliAsk::new(fake_cli(dir.path(), SLOW_GRANDCHILD))
            .send(&setup(), &AskInput::text("hello"), &mut |_| {}, &cancel, Duration::from_secs(25))
            .unwrap_err();
        let pid = canceller.join().unwrap();
        assert_eq!(err, AskError::Cancelled);
        // The process is dropped when `send` returns an error, and
        // the tree with it: `taskkill` has finished by then.
        assert!(!alive(&pid), "the process behind the shim ({pid}) outlived the cancel");
    }

    #[test]
    fn a_timeout_ends_the_process_behind_the_cmd_shim() {
        let dir = tempfile::tempdir().unwrap();
        let pid = {
            let dir = dir.path().to_path_buf();
            thread::spawn(move || grandchild_pid(&dir))
        };
        let err = ClaudeCliAsk::new(fake_cli(dir.path(), SLOW_GRANDCHILD))
            .send(&setup(), &AskInput::text("hello"), &mut |_| {}, &CancelFlag::new(), Duration::from_secs(8))
            .unwrap_err();
        assert_eq!(err, AskError::Timeout);
        let pid = pid.join().unwrap();
        assert!(!alive(&pid), "the process behind the shim ({pid}) outlived the timeout");
    }

    #[test]
    fn cancel_stops_a_cmd_shim() {
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(dir.path(), "set /p line=\r\nping -n 30 127.0.0.1 >nul");
        let cancel = CancelFlag::new();
        let canceller = {
            let cancel = cancel.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(300));
                cancel.cancel();
            })
        };
        let started = Instant::now();
        let err = ClaudeCliAsk::new(settings)
            .send(&setup(), &AskInput::text("hello"), &mut |_| {}, &cancel, Duration::from_secs(20))
            .unwrap_err();
        canceller.join().unwrap();
        assert_eq!(err, AskError::Cancelled);
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
