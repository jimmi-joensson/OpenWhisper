//! The connections an ask can use: listing them, and handing the
//! enabled ones to one run.
//!
//! **Listing never calls a tool and never asks the model anything.**
//! A Claude Code process is started with stream-json input and gets
//! no prompt. It is sent the `mcp_status` control request, which the
//! CLI answers from what it has connected to: every connection with
//! its state and its tools. Then the process is ended. Measured with
//! Claude Code 2.1.322 on a setup with 34 connections: 3 to 5
//! seconds, no tokens. The person's own hooks do not run for it
//! (`disableAllHooks` in a settings file passed for this one
//! process).
//!
//! **What a listing knows, and where it goes.** For each connection
//! the CLI also returns how it is reached (a command, a URL, an id).
//! That part can hold secrets, so it is treated differently from the
//! rest:
//!
//! - names, kinds, states and tool names go to the caller
//!   ([`ListedConnection`]) and, merged, into the settings
//!   ([`merge_listing`]);
//! - how a connection is reached stays in this process, in memory
//!   ([`Details`]): never written to `settings.json`, never logged,
//!   and with no `Debug` that shows it. The CLI leaves a server's
//!   environment and headers out of its answer, so for the person's
//!   own servers those two are read from Claude Code's config file
//!   (`~/.claude.json`) straight into the same memory.
//!
//! **A run gets exactly the connections it was given.** For one ask
//! the enabled connections are written to a private file
//! ([`write_run_config`]: mode 0600, in the temp folder, removed when
//! the run is over) that the CLI is started with under
//! `--strict-mcp-config`, so a connection that is not enabled does
//! not exist for the model. A connection of the person's Claude Code
//! that has not been listed since this process started has no
//! details in memory; the run then fails and says to refresh, rather
//! than start without it.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Value, json};

use super::AgentError;
use super::policy::server_key;
use crate::refine::claude_cli as refine_cli;
use crate::settings::{AgentConnection, AgentConnectionKind, AgentSettings, ClaudeCliSettings, ToolInfo};

/// How long a listing waits for connections that are still coming up
/// before it reports them as they are.
pub const LIST_WAIT: Duration = Duration::from_secs(20);

/// How long a listing waits for the first connection to show up at
/// all. A setup without any answers with an empty list from the
/// start, and so does one that has not read its config yet.
const EMPTY_SETTLE: Duration = Duration::from_millis(1500);
/// Between two looks at connections that are still coming up.
const POLL_EVERY: Duration = Duration::from_millis(250);

/// Where a listed connection comes from.
#[non_exhaustive]
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectionSource {
    /// A server the person added to their Claude Code.
    User,
    /// A server one of their Claude Code plugins brings.
    Plugin,
    /// A connector of their Claude account.
    Account,
    /// Their own entry in the Agent settings.
    Custom,
    /// Something this version does not know.
    Other,
}

impl ConnectionSource {
    /// Stable id, same as the serialized form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Plugin => "plugin",
            Self::Account => "account",
            Self::Custom => "custom",
            Self::Other => "other",
        }
    }
}

/// Whether a listed connection can be used right now.
#[non_exhaustive]
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectionState {
    Connected,
    /// The person has to sign in to it in Claude Code first.
    NeedsAuth,
    Failed,
    /// Still coming up when the listing gave up waiting.
    Pending,
    /// Switched off in Claude Code.
    Disabled,
    Unknown,
}

impl ConnectionState {
    /// Stable id, same as the serialized form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::NeedsAuth => "needs-auth",
            Self::Failed => "failed",
            Self::Pending => "pending",
            Self::Disabled => "disabled",
            Self::Unknown => "unknown",
        }
    }
}

/// One connection as a listing found it. Holds nothing about how the
/// connection is reached.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct ListedConnection {
    pub name: String,
    pub source: ConnectionSource,
    pub state: ConnectionState,
    /// How the CLI talks to it: `stdio`, `http`, `sse`, or the
    /// account's own relay. Empty when it does not say.
    pub transport: String,
    /// Its tools with what it says about them. Empty unless it is
    /// connected.
    pub tools: Vec<ToolInfo>,
}

// --- how a connection is reached: memory only ----------------------

/// How one connection of the person's Claude Code is reached, as the
/// CLI's listing gave it. Can hold secrets.
#[derive(Clone)]
struct Details(Value);

/// Never prints what is inside.
impl fmt::Debug for Details {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Details(not shown)")
    }
}

/// Details by connection name, from the last listing in this
/// process. Not persisted anywhere.
static DETAILS: LazyLock<Mutex<HashMap<String, Details>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn details<R>(f: impl FnOnce(&mut HashMap<String, Details>) -> R) -> R {
    f(&mut DETAILS.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Whether this process knows how to reach the Claude connection
/// `name`: it was in a listing since the process started. A shell
/// uses it to tell whether a refresh is due before an ask.
pub fn is_listed(name: &str) -> bool {
    details(|map| map.contains_key(name))
}

/// Forget every connection's details. After this, a run with a
/// Claude connection asks for a refresh first.
pub fn forget_listing() {
    details(HashMap::clear);
}

/// Put a Claude connection's details into memory without a listing.
/// For tests.
#[cfg(all(test, unix))]
pub(crate) fn remember_for_test(name: &str, config: Value) {
    details(|map| map.insert(name.to_string(), Details(config)));
}

// --- listing -------------------------------------------------------

/// The arguments of a listing process. No prompt, no model, no
/// built-in tool; `settings_file` switches the person's hooks off for
/// this one process. `--tools` takes a list, so it is followed by
/// another flag.
pub fn list_args(settings_file: &Path) -> Vec<String> {
    let file = settings_file.display().to_string();
    [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--tools",
        "",
        "--no-session-persistence",
        "--disable-slash-commands",
        "--permission-mode",
        "default",
        "--settings",
        file.as_str(),
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect()
}

/// [`list_args`] for the person's own entries: only the connections
/// in `config_file`, and none of their Claude Code setup.
pub fn list_custom_args(settings_file: &Path, config_file: &Path) -> Vec<String> {
    let mut args = list_args(settings_file);
    let config = config_file.display().to_string();
    args.extend(
        ["--strict-mcp-config", "--mcp-config", config.as_str(), "--setting-sources", ""].iter().map(|s| (*s).to_string()),
    );
    args
}

/// A private temp file that is removed when this is dropped.
struct TempFile(PathBuf);

impl TempFile {
    fn write(stem: &str, ext: &str, contents: &[u8]) -> Result<Self, AgentError> {
        refine_cli::write_private_file(stem, ext, contents)
            .map(Self)
            .map_err(|e| AgentError::Failed(format!("Could not write a temporary file: {e}")))
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// One listing process. Ended when dropped, with everything under
/// it.
struct Lister {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    errors: Arc<Mutex<String>>,
}

impl Lister {
    fn spawn(bin: &Path, args: Vec<String>) -> Result<Self, AgentError> {
        let mut cmd = refine_cli::command(bin);
        cmd.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd
            .spawn()
            .map_err(|e| AgentError::Unavailable(format!("Could not start Claude Code ({}): {e}", bin.display())))?;
        let stdin = child.stdin.take();
        let errors = Arc::new(Mutex::new(String::new()));
        if let Some(mut stderr) = child.stderr.take() {
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
        if let Some(stdout) = child.stdout.take() {
            thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if tx.send(line).is_err() {
                        return;
                    }
                }
                // Let the stderr reader finish before the channel
                // closes and the caller asks what the process said.
                thread::sleep(Duration::from_millis(100));
            });
        }
        Ok(Self { child, stdin, lines, errors })
    }

    fn ask(&mut self, request_id: &str) -> Result<(), AgentError> {
        let request =
            json!({ "type": "control_request", "request_id": request_id, "request": { "subtype": "mcp_status" } });
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(AgentError::Failed("Claude Code has no input channel".into()));
        };
        let mut line = request.to_string();
        line.push('\n');
        stdin.write_all(line.as_bytes()).and_then(|()| stdin.flush()).map_err(|_| self.exit_error())
    }

    /// Why the process is gone, in the words Refine and Ask use for
    /// the same thing (not installed, not signed in).
    fn exit_error(&self) -> AgentError {
        let said = self.errors.lock().map(|e| e.clone()).unwrap_or_default();
        match refine_cli::exit_error(&said) {
            crate::refine::RefineError::Unavailable(m) => AgentError::Unavailable(m),
            crate::refine::RefineError::Timeout => AgentError::Timeout,
            crate::refine::RefineError::Backend(m) => AgentError::Failed(m),
        }
    }

    /// The servers of the answer to `request_id`. Lines about
    /// anything else are skipped.
    fn answer(&mut self, request_id: &str, deadline: Instant) -> Result<Vec<Value>, AgentError> {
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(AgentError::Timeout);
            }
            let line = match self.lines.recv_timeout(deadline - now) {
                Ok(line) => line,
                Err(RecvTimeoutError::Timeout) => return Err(AgentError::Timeout),
                Err(RecvTimeoutError::Disconnected) => return Err(self.exit_error()),
            };
            let Ok(v) = serde_json::from_str::<Value>(line.trim()) else {
                continue;
            };
            if v["type"].as_str() != Some("control_response") || v["response"]["request_id"].as_str() != Some(request_id)
            {
                continue;
            }
            if v["response"]["subtype"].as_str() != Some("success") {
                return Err(AgentError::Failed(
                    "Claude Code could not list its connections. It may be too old for this: update it and try again."
                        .into(),
                ));
            }
            return Ok(v["response"]["response"]["mcpServers"].as_array().cloned().unwrap_or_default());
        }
    }
}

impl Drop for Lister {
    fn drop(&mut self) {
        drop(self.stdin.take());
        refine_cli::end_process(&mut self.child, true);
    }
}

/// Run one listing: ask until nothing is coming up any more, or
/// `wait` is over. Returns the servers as the CLI gave them.
fn list_raw(bin: &Path, args: Vec<String>, wait: Duration) -> Result<Vec<Value>, AgentError> {
    let started = Instant::now();
    let deadline = started + wait;
    let mut lister = Lister::spawn(bin, args)?;
    let mut asked = 0u32;
    let mut last: Option<Vec<Value>> = None;
    loop {
        asked += 1;
        let request_id = format!("ow-mcp-{asked}");
        lister.ask(&request_id)?;
        let servers = match lister.answer(&request_id, deadline) {
            Ok(servers) => servers,
            // Out of time with an earlier answer in hand: that one.
            Err(AgentError::Timeout) if last.is_some() => return Ok(last.unwrap_or_default()),
            Err(e) => return Err(e),
        };
        let pending = servers.iter().any(|s| s["status"].as_str() == Some("pending"));
        let settling = servers.is_empty() && started.elapsed() < EMPTY_SETTLE;
        if !pending && !settling {
            return Ok(servers);
        }
        last = Some(servers);
        if Instant::now() + POLL_EVERY >= deadline {
            return Ok(last.unwrap_or_default());
        }
        thread::sleep(POLL_EVERY);
    }
}

fn source_of(server: &Value) -> ConnectionSource {
    let word = |key: &str| server[key].as_str().unwrap_or("").to_lowercase();
    let (source, scope) = (word("source"), word("scope"));
    // `source` is the more telling one: a plugin's server has the
    // scope "dynamic", like anything passed on the command line.
    for value in [source.as_str(), scope.as_str()] {
        match value {
            v if v.starts_with("plugin") => return ConnectionSource::Plugin,
            v if v.starts_with("claudeai") => return ConnectionSource::Account,
            "user" | "local" | "project" => return ConnectionSource::User,
            _ => {}
        }
    }
    ConnectionSource::Other
}

fn state_of(server: &Value) -> ConnectionState {
    match server["status"].as_str().unwrap_or("") {
        "connected" => ConnectionState::Connected,
        "needs-auth" => ConnectionState::NeedsAuth,
        "failed" => ConnectionState::Failed,
        "pending" => ConnectionState::Pending,
        "disabled" => ConnectionState::Disabled,
        _ => ConnectionState::Unknown,
    }
}

fn tools_of(server: &Value) -> Vec<ToolInfo> {
    server["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool| {
            let name = tool["name"].as_str()?.trim();
            let notes = &tool["annotations"];
            let flag = |short: &str, hint: &str| notes[short].as_bool().or_else(|| notes[hint].as_bool());
            (!name.is_empty()).then(|| ToolInfo {
                name: name.to_string(),
                title: tool["title"].as_str().or_else(|| notes["title"].as_str()).unwrap_or("").to_string(),
                read_only: flag("readOnly", "readOnlyHint"),
                destructive: flag("destructive", "destructiveHint"),
            })
        })
        .collect()
}

fn listed(server: &Value, source: Option<ConnectionSource>) -> Option<ListedConnection> {
    let name = server["name"].as_str()?.trim();
    (!name.is_empty()).then(|| ListedConnection {
        name: name.to_string(),
        source: source.unwrap_or_else(|| source_of(server)),
        state: state_of(server),
        transport: server["transport"].as_str().or_else(|| server["config"]["type"].as_str()).unwrap_or("").to_string(),
        tools: tools_of(server),
    })
}

/// Where Claude Code keeps the person's own servers:
/// `.claude.json` in `CLAUDE_CONFIG_DIR` when that is set, in the
/// home folder otherwise.
fn claude_config_file() -> Option<PathBuf> {
    match std::env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()) {
        Some(dir) => Some(PathBuf::from(dir).join(".claude.json")),
        None => refine_cli::home_dir().map(|home| home.join(".claude.json")),
    }
}

/// The person's own servers as Claude Code's config file has them,
/// by name. Empty when the file is not there or cannot be read.
fn user_servers(config_file: Option<&Path>) -> serde_json::Map<String, Value> {
    config_file
        .and_then(|path| fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|v| v["mcpServers"].as_object().cloned())
        .unwrap_or_default()
}

/// The details of one listed server: what the CLI's answer has, and
/// for one of the person's own servers the environment and headers
/// its config file entry has, which the answer leaves out. Only when
/// the entry is plainly the same server (same command or URL).
fn details_of(server: &Value, own: &serde_json::Map<String, Value>) -> Option<Details> {
    let mut config = server["config"].as_object()?.clone();
    if source_of(server) == ConnectionSource::User
        && let Some(entry) = server["name"].as_str().and_then(|name| own.get(name))
    {
        let same = |key: &str| config.get(key).is_some() && config.get(key) == entry.get(key);
        if same("command") || same("url") {
            for key in ["env", "headers"] {
                if !config.contains_key(key)
                    && let Some(value) = entry.get(key).filter(|v| v.as_object().is_some_and(|o| !o.is_empty()))
                {
                    config.insert(key.to_string(), value.clone());
                }
            }
        }
    }
    Some(Details(Value::Object(config)))
}

/// List the connections the person's Claude Code has: their own
/// servers, their plugins' servers and their account's connectors,
/// with state and tools. See the module docs: no tool is called, the
/// model is not asked anything, and how each connection is reached
/// goes into this process's memory and nowhere else.
///
/// Takes a few seconds (every connection is started or dialled).
/// Connections still coming up after `wait` are reported as
/// [`ConnectionState::Pending`].
pub fn list_claude_connections(cli: &ClaudeCliSettings, wait: Duration) -> Result<Vec<ListedConnection>, AgentError> {
    list_claude_connections_with(cli, wait, claude_config_file().as_deref())
}

/// [`list_claude_connections`] with Claude Code's config file given.
fn list_claude_connections_with(
    cli: &ClaudeCliSettings,
    wait: Duration,
    config_file: Option<&Path>,
) -> Result<Vec<ListedConnection>, AgentError> {
    let bin = find_binary(cli)?;
    let quiet = TempFile::write("agent-list", "json", br#"{"disableAllHooks":true}"#)?;
    let servers = list_raw(&bin, list_args(&quiet.0), wait)?;
    drop(quiet);
    let own = user_servers(config_file);
    let found: HashMap<String, Details> = servers
        .iter()
        .filter_map(|server| Some((server["name"].as_str()?.trim().to_string(), details_of(server, &own)?)))
        .collect();
    // The whole map at once: a connection that is gone from the
    // person's setup is gone from here too.
    details(|map| *map = found);
    let mut listing: Vec<ListedConnection> = servers.iter().filter_map(|server| listed(server, None)).collect();
    listing.sort_by_key(|c| c.name.to_lowercase());
    Ok(listing)
}

/// List the tools of the person's own entries (`connections`, the
/// custom ones among them), enabled or not, the same way: a CLI
/// process that is given only these, gets no prompt, and is asked
/// what it sees. Empty without any custom entry; no process is
/// started then.
pub fn list_custom_connections(
    cli: &ClaudeCliSettings,
    connections: &[AgentConnection],
    wait: Duration,
) -> Result<Vec<ListedConnection>, AgentError> {
    let custom: Vec<AgentConnection> = connections
        .iter()
        .filter(|c| c.kind == AgentConnectionKind::Custom)
        .map(|c| AgentConnection { enabled: true, ..c.clone() })
        .collect();
    if custom.is_empty() {
        return Ok(Vec::new());
    }
    let bin = find_binary(cli)?;
    let config = write_run_config(&custom)?;
    let quiet = TempFile::write("agent-list", "json", br#"{"disableAllHooks":true}"#)?;
    let servers = list_raw(&bin, list_custom_args(&quiet.0, config.path()), wait)?;
    Ok(servers.iter().filter_map(|server| listed(server, Some(ConnectionSource::Custom))).collect())
}

fn find_binary(cli: &ClaudeCliSettings) -> Result<PathBuf, AgentError> {
    refine_cli::find_binary(cli).ok_or_else(|| AgentError::Unavailable(refine_cli::NOT_INSTALLED.to_string()))
}

/// `settings` after a listing: what the person chose stays, what the
/// connections say about themselves is brought up to date.
///
/// - A connection that is in both keeps its switch and its ticked
///   tools, and gets the tool list of the listing. When the listing
///   could not see its tools (not connected right now), the list
///   from before stays.
/// - A connection that is new arrives switched off with nothing
///   ticked.
/// - A Claude connection that is gone from the listing stays only
///   when the person had touched it (enabled it or ticked a tool),
///   so they see that it is gone rather than lose their choice.
///   `claude_listed` says whether `listing` covers the Claude
///   connections at all; without it they are all left as they are.
/// - The person's own entries are never added or removed here, only
///   their tool lists are updated.
pub fn merge_listing(settings: &AgentSettings, listing: &[ListedConnection], claude_listed: bool) -> AgentSettings {
    let mut merged = settings.clone();
    let key = |name: &str| name.trim().to_lowercase();
    let in_listing = |c: &AgentConnection| {
        listing.iter().find(|l| {
            key(&l.name) == key(&c.name) && (l.source == ConnectionSource::Custom) == (c.kind == AgentConnectionKind::Custom)
        })
    };
    merged.connections.retain(|c| {
        c.kind == AgentConnectionKind::Custom
            || !claude_listed
            || in_listing(c).is_some()
            || c.enabled
            || !c.allowed_tools.is_empty()
    });
    for connection in &mut merged.connections {
        if let Some(found) = in_listing(connection)
            && (found.state == ConnectionState::Connected || !found.tools.is_empty())
        {
            connection.tools = found.tools.clone();
        }
    }
    for found in listing.iter().filter(|l| l.source != ConnectionSource::Custom) {
        let known = merged.connections.iter().any(|c| c.kind == AgentConnectionKind::Claude && key(&c.name) == key(&found.name));
        if !known {
            merged.connections.push(AgentConnection {
                name: found.name.clone(),
                kind: AgentConnectionKind::Claude,
                tools: found.tools.clone(),
                ..AgentConnection::default()
            });
        }
    }
    merged.cleaned()
}

/// What a refresh found and saved.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Refreshed {
    /// The Agent block as it was saved.
    pub settings: AgentSettings,
    /// Every connection that was listed, the person's own entries
    /// included, with its state.
    pub listing: Vec<ListedConnection>,
    /// Why the person's own entries could not be listed; empty when
    /// they were, or when there are none.
    pub custom_error: String,
    pub elapsed_ms: u64,
}

/// List everything, merge it into the Agent block of the settings
/// file at `path` and save it: what the refresh button and
/// `openwhisper agent connections --refresh` do. The block is read
/// from the file right before it is merged, so a change made while
/// the listing ran is kept.
///
/// When the person's own entries cannot be listed, the refresh still
/// counts ([`Refreshed::custom_error`] says why); when Claude Code's
/// connections cannot be listed, nothing is saved.
pub fn refresh(path: &Path, cli: &ClaudeCliSettings, wait: Duration) -> Result<Refreshed, AgentError> {
    let started = Instant::now();
    let mut listing = list_claude_connections(cli, wait)?;
    let before = crate::settings::load_agent_settings(path);
    let (custom, custom_error) = match list_custom_connections(cli, &before.connections, wait) {
        Ok(custom) => (custom, String::new()),
        Err(e) => (Vec::new(), e.to_string()),
    };
    listing.extend(custom);
    // Read again: the listing took seconds.
    let current = crate::settings::load_agent_settings(path);
    let merged = merge_listing(&current, &listing, true);
    let settings = crate::settings::save_agent_settings(path, merged).map_err(AgentError::Failed)?;
    Ok(Refreshed { settings, listing, custom_error, elapsed_ms: started.elapsed().as_millis() as u64 })
}

// --- the connections of one run ------------------------------------

/// The private file one run's connections are in. Removed when this
/// is dropped, which is when the run's process is.
pub struct RunConfig {
    path: PathBuf,
    names: Vec<String>,
}

impl RunConfig {
    /// For `--mcp-config`.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The connections in the file, by name.
    pub fn names(&self) -> &[String] {
        &self.names
    }
}

/// The path and the names; the file's contents are not shown.
impl fmt::Debug for RunConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RunConfig").field("path", &self.path).field("names", &self.names).finish()
    }
}

impl Drop for RunConfig {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// How a custom connection is written for the CLI.
fn custom_entry(connection: &AgentConnection) -> Option<Value> {
    let custom = connection.custom.as_ref()?;
    if !custom.command.trim().is_empty() {
        Some(json!({ "type": "stdio", "command": custom.command, "args": custom.args, "env": custom.env }))
    } else if !custom.url.trim().is_empty() {
        Some(json!({ "type": "http", "url": custom.url }))
    } else {
        None
    }
}

/// What a run says when it needs a connection nobody has listed
/// since the app started.
pub fn refresh_first(name: &str) -> String {
    format!(
        "Refresh the connections first: the app has not looked up \"{name}\" since it started, so it does not know \
how to reach it. Open the Agent settings and refresh, or run: openwhisper agent connections --refresh"
    )
}

/// Write the file one run is started with: `mcpServers` for exactly
/// the enabled connections among `connections`, each name once. The
/// person's own entries come from the settings; a connection of
/// their Claude Code comes from this process's memory, and when it
/// is not there the run fails ([`AgentError::NotListed`]) rather
/// than start without it.
///
/// The file is private to this user (mode 0600 on unix), lies in the
/// temp folder, and is removed when the returned value is dropped.
pub fn write_run_config(connections: &[AgentConnection]) -> Result<RunConfig, AgentError> {
    let mut servers = serde_json::Map::new();
    let mut seen = std::collections::HashSet::new();
    for connection in connections.iter().filter(|c| c.enabled) {
        if !seen.insert(server_key(&connection.name).to_lowercase()) {
            continue;
        }
        let entry = match connection.kind {
            AgentConnectionKind::Custom => custom_entry(connection)
                .ok_or_else(|| AgentError::Failed(format!("The connection \"{}\" has no command and no URL.", connection.name)))?,
            AgentConnectionKind::Claude => details(|map| map.get(&connection.name).map(|d| d.0.clone()))
                .ok_or_else(|| AgentError::NotListed(refresh_first(&connection.name)))?,
        };
        servers.insert(connection.name.clone(), entry);
    }
    let names: Vec<String> = servers.keys().cloned().collect();
    let mut text = json!({ "mcpServers": servers }).to_string();
    let written = refine_cli::write_private_file("agent-run", "json", text.as_bytes());
    // The text holds what the file holds; it goes with this call.
    crate::ask::wipe::string(&mut text);
    let path = written.map_err(|e| AgentError::Failed(format!("Could not write the connections of this run: {e}")))?;
    Ok(RunConfig { path, names })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::settings::CustomServer;
    use std::os::unix::fs::PermissionsExt;

    /// A fake `claude`: a shell script standing in for the real CLI.
    fn fake_cli(dir: &Path, body: &str) -> ClaudeCliSettings {
        let path = dir.join("claude");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        ClaudeCliSettings { path: Some(path.display().to_string()), ..ClaudeCliSettings::default() }
    }

    /// Answers every `mcp_status` request the way the real CLI does:
    /// the first time one server is still coming up, after that all
    /// are settled. Records its arguments and whatever else it is
    /// sent. `SERVERS_1` and `SERVERS_2` are replaced by the test.
    const LISTING: &str = r#"
here="$(dirname "$0")"
echo "$@" > "$here/args.txt"
prev=""; for a in "$@"; do
  if [ "$prev" = "--settings" ]; then cp "$a" "$here/settings-arg.json"; fi
  if [ "$prev" = "--mcp-config" ]; then cp "$a" "$here/mcp-arg.json"; fi
  prev="$a"
done
n=0
while read -r line; do
  printf '%s\n' "$line" >> "$here/stdin.txt"
  id=$(printf '%s' "$line" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
  n=$((n+1))
  echo '{"type":"system","subtype":"something_else"}'
  if [ "$n" = "1" ]; then
    echo "{\"type\":\"control_response\",\"response\":{\"subtype\":\"success\",\"request_id\":\"$id\",\"response\":{\"mcpServers\":SERVERS_1}}}"
  else
    echo "{\"type\":\"control_response\",\"response\":{\"subtype\":\"success\",\"request_id\":\"$id\",\"response\":{\"mcpServers\":SERVERS_2}}}"
  fi
done
"#;

    fn listing_script(first: &Value, then: &Value) -> String {
        // Inside a double-quoted shell string.
        let quote = |v: &Value| v.to_string().replace('\\', "\\\\").replace('"', "\\\"");
        LISTING.replace("SERVERS_1", &quote(first)).replace("SERVERS_2", &quote(then))
    }

    const SECRET: &str = "tok-made-up-789";

    fn servers(chat_status: &str) -> Value {
        json!([
            {
                "name": "team-chat", "status": chat_status, "scope": "user", "source": "user", "transport": "stdio",
                "config": { "type": "stdio", "command": "chat-server", "args": ["--stdio"] },
                "tools": if chat_status == "connected" { json!([
                    { "name": "read_thread", "annotations": { "readOnly": true } },
                    { "name": "send_message", "annotations": {} },
                    { "name": "delete_message", "annotations": { "destructive": true } },
                ]) } else { json!(null) },
            },
            {
                "name": "claude.ai Docs", "status": "needs-auth", "scope": "claudeai", "source": "claudeai",
                "transport": "claudeai-proxy", "config": { "type": "claudeai-proxy", "url": "https://example.com/relay", "id": "made-up-id" },
            },
            {
                "name": "plugin:helper:notes", "status": "connected", "scope": "dynamic", "source": "plugin:helper",
                "transport": "http", "config": { "type": "http", "url": "https://example.com/notes" },
                "tools": [ { "name": "search_notes", "annotations": { "readOnlyHint": true, "title": "Search Notes" } } ],
            },
        ])
    }

    /// The details map is one per process: tests that fill it or
    /// clear it take turns.
    static DETAILS_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn a_listing_sends_no_prompt_waits_for_pending_and_keeps_details_in_memory_only() {
        let _lock = DETAILS_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let cli = fake_cli(dir.path(), &listing_script(&servers("pending"), &servers("connected")));
        // The person's own server has a token in Claude Code's file,
        // which the CLI's answer leaves out.
        let claude_config = dir.path().join("claude.json");
        fs::write(
            &claude_config,
            json!({ "mcpServers": {
                "team-chat": { "type": "stdio", "command": "chat-server", "args": ["--stdio"], "env": { "CHAT_TOKEN": SECRET } },
                // Same name as a plugin's server would be a different thing.
                "plugin:helper:notes": { "type": "http", "url": "https://example.com/notes", "headers": { "Authorization": SECRET } },
            } })
            .to_string(),
        )
        .unwrap();
        let started = Instant::now();
        let listing = list_claude_connections_with(&cli, Duration::from_secs(10), Some(&claude_config)).unwrap();
        assert!(started.elapsed() < Duration::from_secs(5));

        // Sorted by name; kinds, states and tools as the CLI said.
        let names: Vec<&str> = listing.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["claude.ai Docs", "plugin:helper:notes", "team-chat"]);
        assert_eq!((listing[0].source, listing[0].state), (ConnectionSource::Account, ConnectionState::NeedsAuth));
        assert!(listing[0].tools.is_empty());
        assert_eq!((listing[1].source, listing[1].state), (ConnectionSource::Plugin, ConnectionState::Connected));
        assert_eq!(
            listing[1].tools,
            [ToolInfo { name: "search_notes".into(), title: "Search Notes".into(), read_only: Some(true), destructive: None }]
        );
        assert_eq!((listing[2].source, listing[2].state), (ConnectionSource::User, ConnectionState::Connected));
        assert_eq!(listing[2].transport, "stdio");
        assert_eq!(listing[2].tools.len(), 3);
        assert_eq!((listing[2].tools[0].read_only, listing[2].tools[2].destructive), (Some(true), Some(true)));
        assert_eq!((listing[2].tools[1].read_only, listing[2].tools[1].destructive), (None, None));

        // It asked twice (the first answer had a server coming up),
        // and every line it sent was that request: no prompt.
        let sent = fs::read_to_string(dir.path().join("stdin.txt")).unwrap();
        let lines: Vec<&str> = sent.lines().collect();
        assert_eq!(lines.len(), 2, "{sent}");
        for line in lines {
            let v: Value = serde_json::from_str(line).unwrap();
            assert_eq!((v["type"].as_str(), v["request"]["subtype"].as_str()), (Some("control_request"), Some("mcp_status")));
        }
        assert!(!sent.contains("\"user\""));
        // The flags: no model, no built-in tools, nothing persisted,
        // the person's hooks off.
        let args = fs::read_to_string(dir.path().join("args.txt")).unwrap();
        for flag in ["-p", "--input-format stream-json", "--no-session-persistence", "--permission-mode default", "--settings "] {
            assert!(args.contains(flag), "{flag} missing in {args}");
        }
        assert!(!args.contains("--model") && !args.contains("--strict-mcp-config"), "{args}");
        assert_eq!(fs::read_to_string(dir.path().join("settings-arg.json")).unwrap(), r#"{"disableAllHooks":true}"#);

        // Nothing about how a connection is reached is in what the
        // listing returns, however it is printed.
        let printed = format!("{listing:?} {}", serde_json::to_string(&listing).unwrap());
        for hidden in [SECRET, "chat-server", "example.com", "made-up-id", "CHAT_TOKEN"] {
            assert!(!printed.contains(hidden), "{hidden} in {printed}");
        }
        // It is in memory, with the token of the person's own
        // server, and not with a header of a file entry that only
        // shares a plugin server's name.
        assert!(is_listed("team-chat") && is_listed("claude.ai Docs") && is_listed("plugin:helper:notes"));
        let chat = details(|map| map["team-chat"].0.clone());
        assert_eq!(chat["env"]["CHAT_TOKEN"], SECRET);
        assert_eq!(chat["command"], "chat-server");
        assert!(details(|map| map["plugin:helper:notes"].0.clone()).get("headers").is_none());
        assert_eq!(format!("{:?}", details(|map| map["team-chat"].clone())), "Details(not shown)");

        // Merged into settings and saved: names and tools, and not a
        // trace of the details.
        let merged = merge_listing(&AgentSettings::default(), &listing, true);
        assert_eq!(merged.connections.len(), 3);
        assert!(merged.connections.iter().all(|c| !c.enabled && c.allowed_tools.is_empty() && c.custom.is_none()));
        let file = dir.path().join("settings.json");
        let _cache = crate::settings::CACHE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        crate::settings::save_agent_settings(&file, merged).unwrap();
        crate::settings::set_agent_settings_cache(AgentSettings::default());
        let saved = fs::read_to_string(&file).unwrap();
        assert!(saved.contains("team-chat") && saved.contains("read_thread"), "{saved}");
        for hidden in [SECRET, "chat-server", "example.com", "made-up-id", "CHAT_TOKEN", "--stdio"] {
            assert!(!saved.contains(hidden), "{hidden} in {saved}");
        }
        forget_listing();
        assert!(!is_listed("team-chat"));
    }

    #[test]
    fn a_listing_that_runs_out_of_time_reports_what_is_still_coming_up() {
        let _lock = DETAILS_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let cli = fake_cli(dir.path(), &listing_script(&servers("pending"), &servers("pending")));
        let started = Instant::now();
        let listing = list_claude_connections_with(&cli, Duration::from_millis(2500), None).unwrap();
        assert!(started.elapsed() < Duration::from_millis(3500), "it waited past its limit");
        let chat = listing.iter().find(|c| c.name == "team-chat").unwrap();
        assert_eq!(chat.state, ConnectionState::Pending);
        assert!(chat.tools.is_empty());
        forget_listing();
    }

    #[test]
    fn a_cli_that_is_missing_exits_or_does_not_know_the_request_is_a_plain_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = ClaudeCliSettings { path: Some("/nonexistent/claude".into()), ..ClaudeCliSettings::default() };
        assert!(matches!(
            list_claude_connections_with(&missing, Duration::from_secs(2), None),
            Err(AgentError::Unavailable(_))
        ));
        let signed_out = fake_cli(dir.path(), "echo 'Not logged in. Please run /login' >&2; exit 1");
        match list_claude_connections_with(&signed_out, Duration::from_secs(5), None) {
            Err(AgentError::Unavailable(message)) => assert!(message.to_lowercase().contains("sign"), "{message}"),
            other => panic!("{other:?}"),
        }
        let silent = fake_cli(dir.path(), "sleep 30");
        assert_eq!(list_claude_connections_with(&silent, Duration::from_millis(300), None), Err(AgentError::Timeout));
        let old = fake_cli(
            dir.path(),
            r#"while read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
  echo "{\"type\":\"control_response\",\"response\":{\"subtype\":\"error\",\"request_id\":\"$id\",\"error\":\"unknown\"}}"
done"#,
        );
        match list_claude_connections_with(&old, Duration::from_secs(5), None) {
            Err(AgentError::Failed(message)) => assert!(message.contains("update"), "{message}"),
            other => panic!("{other:?}"),
        }
    }

    fn own(name: &str, enabled: bool) -> AgentConnection {
        AgentConnection {
            name: name.into(),
            kind: AgentConnectionKind::Custom,
            enabled,
            custom: Some(CustomServer {
                command: "/opt/example/chat-server".into(),
                args: vec!["--log".into(), "/tmp/example.log".into()],
                env: std::collections::BTreeMap::from([("CHAT_TOKEN".to_string(), SECRET.to_string())]),
                ..CustomServer::default()
            }),
            ..AgentConnection::default()
        }
    }

    fn claude(name: &str, enabled: bool) -> AgentConnection {
        AgentConnection { name: name.into(), enabled, ..AgentConnection::default() }
    }

    #[test]
    fn the_run_file_holds_exactly_the_enabled_connections_is_private_and_goes_away() {
        let _lock = DETAILS_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        forget_listing();
        remember_for_test("claude.ai Docs", json!({ "type": "claudeai-proxy", "url": "https://example.com/relay", "id": "made-up-id" }));
        remember_for_test("Unused", json!({ "type": "http", "url": "https://example.com/unused" }));
        let by_url = AgentConnection {
            name: "notes".into(),
            kind: AgentConnectionKind::Custom,
            enabled: true,
            custom: Some(CustomServer { url: "https://example.com/notes".into(), ..CustomServer::default() }),
            ..AgentConnection::default()
        };
        let connections =
            [own("chat", true), own("chat-off", false), claude("claude.ai Docs", true), claude("Unused", false), by_url];
        let config = write_run_config(&connections).unwrap();
        let path = config.path().to_path_buf();
        assert_eq!(config.names().len(), 3);
        assert!(path.starts_with(std::env::temp_dir()));
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        let file: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let servers = file["mcpServers"].as_object().unwrap();
        let mut names: Vec<&String> = servers.keys().collect();
        names.sort();
        assert_eq!(names, ["chat", "claude.ai Docs", "notes"]);
        assert_eq!(
            servers["chat"],
            json!({ "type": "stdio", "command": "/opt/example/chat-server", "args": ["--log", "/tmp/example.log"], "env": { "CHAT_TOKEN": SECRET } })
        );
        assert_eq!(servers["claude.ai Docs"]["id"], "made-up-id");
        assert_eq!(servers["notes"], json!({ "type": "http", "url": "https://example.com/notes" }));
        // The value prints its path and names, not what is in it.
        let printed = format!("{config:?}");
        assert!(!printed.contains(SECRET) && !printed.contains("made-up-id"), "{printed}");
        drop(config);
        assert!(!path.exists(), "the file is removed with the run");

        // Nothing enabled: a file with no server in it.
        let none = write_run_config(&[own("chat", false)]).unwrap();
        let file: Value = serde_json::from_slice(&fs::read(none.path()).unwrap()).unwrap();
        assert_eq!(file, json!({ "mcpServers": {} }));
        forget_listing();
    }

    #[test]
    fn a_claude_connection_nobody_listed_stops_the_run_and_says_to_refresh() {
        let _lock = DETAILS_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        forget_listing();
        match write_run_config(&[own("chat", true), claude("claude.ai Docs", true)]) {
            Err(AgentError::NotListed(message)) => {
                assert!(message.starts_with("Refresh the connections first"), "{message}");
                assert!(message.contains("claude.ai Docs"), "{message}");
            }
            other => panic!("{other:?}"),
        }
        // The same connection switched off is not needed, so not missed.
        assert!(write_run_config(&[own("chat", true), claude("claude.ai Docs", false)]).is_ok());
    }

    #[test]
    fn the_persons_own_entries_are_listed_through_a_strict_config_of_their_own() {
        let dir = tempfile::tempdir().unwrap();
        let answer = json!([{
            "name": "chat", "status": "connected", "scope": "dynamic", "source": "dynamic", "transport": "stdio",
            "config": { "type": "stdio", "command": "/opt/example/chat-server", "args": [] },
            "tools": [ { "name": "read_thread", "annotations": { "readOnly": true } } ],
        }]);
        let cli = fake_cli(dir.path(), &listing_script(&answer, &answer));
        // Listed whether or not it is enabled: the person ticks
        // tools before they switch it on.
        let listing =
            list_custom_connections(&cli, &[own("chat", false), claude("claude.ai Docs", true)], Duration::from_secs(10)).unwrap();
        assert_eq!(listing.len(), 1);
        assert_eq!((listing[0].source, listing[0].state), (ConnectionSource::Custom, ConnectionState::Connected));
        let args = fs::read_to_string(dir.path().join("args.txt")).unwrap();
        assert!(args.contains("--strict-mcp-config --mcp-config "), "{args}");
        assert!(args.trim_end().ends_with("--setting-sources"), "the empty value is last: {args}");
        // The config it was given: that one entry and nothing else.
        let given: Value = serde_json::from_slice(&fs::read(dir.path().join("mcp-arg.json")).unwrap()).unwrap();
        assert_eq!(given["mcpServers"].as_object().unwrap().keys().collect::<Vec<_>>(), ["chat"]);
        // No custom entry: no process at all.
        let nobody = ClaudeCliSettings { path: Some("/nonexistent/claude".into()), ..ClaudeCliSettings::default() };
        assert_eq!(list_custom_connections(&nobody, &[claude("x", true)], Duration::from_secs(1)), Ok(Vec::new()));
    }

    fn found(name: &str, source: ConnectionSource, state: ConnectionState, tools: &[&str]) -> ListedConnection {
        ListedConnection {
            name: name.into(),
            source,
            state,
            transport: String::new(),
            tools: tools.iter().map(|t| ToolInfo::new(t)).collect(),
        }
    }

    #[test]
    fn merging_a_listing_keeps_what_the_person_chose() {
        let settings = AgentSettings {
            enabled: true,
            connections: vec![
                AgentConnection {
                    allowed_tools: vec!["read_thread".into()],
                    tools: vec![ToolInfo::new("read_thread")],
                    ..claude("team-chat", true)
                },
                // Touched by the person, and gone from their setup.
                AgentConnection { allowed_tools: vec!["search".into()], tools: vec![ToolInfo::new("search")], ..claude("Old docs", false) },
                // Never touched, and gone.
                AgentConnection { tools: vec![ToolInfo::new("x")], ..claude("Passing by", false) },
                // Not connected right now: its tool list stays.
                AgentConnection { tools: vec![ToolInfo::new("list_issues")], ..claude("Tracker", true) },
                AgentConnection { tools: vec![ToolInfo::new("old_tool")], ..own("mine", true) },
            ],
            ..AgentSettings::default()
        };
        let listing = [
            found("Team-Chat", ConnectionSource::User, ConnectionState::Connected, &["read_thread", "send_message"]),
            found("Tracker", ConnectionSource::Account, ConnectionState::NeedsAuth, &[]),
            found("New one", ConnectionSource::Account, ConnectionState::Connected, &["search"]),
            found("mine", ConnectionSource::Custom, ConnectionState::Connected, &["read_thread"]),
            // A Claude connection that happens to share a custom
            // entry's name is another connection.
            found("Mine", ConnectionSource::User, ConnectionState::Connected, &["other"]),
        ];
        let merged = merge_listing(&settings, &listing, true);
        let by = |name: &str, kind: AgentConnectionKind| {
            merged.connections.iter().find(|c| c.name == name && c.kind == kind).unwrap_or_else(|| panic!("{name}"))
        };
        let chat = by("team-chat", AgentConnectionKind::Claude);
        assert!(chat.enabled);
        assert_eq!(chat.allowed_tools, ["read_thread"]);
        assert_eq!(chat.tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["read_thread", "send_message"]);
        assert_eq!(by("Old docs", AgentConnectionKind::Claude).allowed_tools, ["search"]);
        assert!(!merged.connections.iter().any(|c| c.name == "Passing by"));
        assert_eq!(by("Tracker", AgentConnectionKind::Claude).tools, [ToolInfo::new("list_issues")]);
        let new = by("New one", AgentConnectionKind::Claude);
        assert!(!new.enabled && new.allowed_tools.is_empty());
        assert_eq!(new.tools, [ToolInfo::new("search")]);
        let mine = by("mine", AgentConnectionKind::Custom);
        assert_eq!(mine.tools, [ToolInfo::new("read_thread")]);
        assert!(mine.custom.is_some() && mine.enabled);
        assert!(merged.enabled);
        // A listing of the person's own entries alone removes no
        // Claude connection.
        let only_custom = merge_listing(&settings, &listing[3..4], false);
        assert!(only_custom.connections.iter().any(|c| c.name == "Passing by"));
        assert_eq!(only_custom.connections.len(), settings.connections.len());
    }
}
