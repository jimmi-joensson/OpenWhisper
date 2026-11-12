//! Agent in the shell (TASK-99.3): the commands behind the Agent
//! pane and the two answers to a confirm in the answer panel. Thin
//! wrappers: core owns the settings block and its rules
//! (`openwhisper_core::settings`), which tools an ask may use and how
//! (`openwhisper_core::agent`), the listing of the connections, and
//! the ask itself, tools included (`openwhisper_core::ask`: the app's
//! one ask reads the Agent block from the file when it starts, so
//! there is no plan to build here).
//!
//! The same settings are reachable headless through
//! `openwhisper agent …`.
//!
//! # What crosses into a window, and what does not
//!
//! How a connection is reached can hold secrets (a server's
//! environment, headers, a token in a URL or an argument). None of
//! it is ever sent to a window:
//!
//! - A connection of the person's Claude Code has its details in
//!   core's memory only (`agent::connections`); this module never
//!   reads them.
//! - A connection the person added by hand has its details in
//!   `settings.json`. A window is told that there is a command or a
//!   URL ([`ConnectionView::via`]) and nothing of it.
//!
//! For the same reason a window cannot send a whole Agent block back.
//! What it sends is the person's choices ([`AgentChoices`]: the two
//! switches, which connections are on, which tools are ticked, the
//! app table), and they are laid over the block as it is on disk
//! ([`with_choices`]). A connection is added only through
//! [`agent_add_connection`] and what a connection is never changes
//! from a window.
//!
//! # Looking the connections up
//!
//! An ask can only use a connection of the person's Claude Code after
//! this process has listed it (core keeps how it is reached in
//! memory, see above). So the connections are listed in the
//! background when the app starts with Agent on and when Agent is
//! switched on, and when the person presses Refresh. A listing starts
//! the Claude CLI without a prompt: no tool is called and the model
//! is not asked anything. It takes a few seconds, so it never runs on
//! the main thread, and only one runs at a time.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread;

use openwhisper_core::agent::{self, policy, AgentRun, ConnectionState, FrontApp, ListedConnection, ToolKind};
use openwhisper_core::ask;
use openwhisper_core::focus;
use openwhisper_core::settings::{
    self as core_settings, AgentConnection, AgentConnectionKind, AgentSettings, AppConnection, CustomServer,
};
use tauri::{AppHandle, Emitter};

use crate::settings::settings_path;

/// Emitted to every window after the Agent settings changed or a
/// listing ended. Payload: [`AgentState`].
pub(crate) const AGENT_CHANGED_EVENT: &str = "agent_changed";

/// Every command of this module.
#[cfg(test)]
const COMMANDS: [&str; 9] = [
    "agent_get",
    "agent_set",
    "agent_refresh",
    "agent_status",
    "agent_add_connection",
    "agent_remove_connection",
    "agent_running_apps",
    "ask_confirm_allow",
    "ask_confirm_deny",
];

// --- what a window is shown ----------------------------------------

/// What the Agent pane loads: the choices in the Agent block, the
/// connections with their tools by kind, the app table in force and
/// where the last listing stands. Nothing in it says how a connection
/// is reached.
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
pub struct AgentState {
    pub enabled: bool,
    pub drafts_only: bool,
    /// How long a confirm waits before it counts as a no.
    pub confirm_timeout_s: u64,
    pub max_turns: u32,
    pub connections: Vec<ConnectionView>,
    /// The app table in force: the person's, or the built-in one.
    pub app_connections: Vec<AppRow>,
    /// The table in force is the built-in one.
    pub app_connections_builtin: bool,
    pub listing: ListingView,
}

/// One connection, as the pane shows it.
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
pub struct ConnectionView {
    pub name: String,
    pub kind: AgentConnectionKind,
    pub enabled: bool,
    /// The ticked tools, by name.
    pub allowed_tools: Vec<String>,
    /// The tools as they were last listed, each with its kind.
    pub tools: Vec<ToolView>,
    /// Where the connection stood in the last listing of this run of
    /// the app: one of core's `ConnectionState` words, `missing` (the
    /// listing did not find it), or `not-looked-up` (there has been
    /// no listing yet).
    pub state: String,
    /// An ask can start with it right now: the person's own entry, or
    /// a connection of their Claude Code that was listed since the
    /// app started.
    pub usable: bool,
    /// Only for the person's own entry: `command` or `url`. Never the
    /// command or the URL itself.
    pub via: Option<&'static str>,
}

/// One tool of a connection.
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
pub struct ToolView {
    pub name: String,
    /// Name to show a person; may be empty.
    pub title: String,
    /// Core's decision (`agent::policy`): `read`, `draft` or `act`.
    pub kind: ToolKind,
}

/// One row of the app table.
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
pub struct AppRow {
    pub app_id: String,
    pub app_name: String,
    /// The connection the row names, as it is stored.
    pub connection: String,
    /// The connection of that name, switched on or not, when there
    /// is one (core compares names the forgiving way).
    pub known: Option<String>,
    /// An ask in this app would use the connection instead of the
    /// screen right now: it is on and has a tool that can run.
    pub active: bool,
}

/// Where the last listing stands.
#[derive(serde::Serialize, Clone, Debug, PartialEq, Default)]
pub struct ListingView {
    /// A listing is running now.
    pub refreshing: bool,
    /// When the last listing that worked ended, Unix epoch
    /// milliseconds. `None` before the first one of this run.
    pub refreshed_at_ms: Option<i64>,
    /// Why the last listing failed; empty when it worked.
    pub error: String,
    /// Why the person's own entries could not be listed; empty when
    /// they were, or when there are none.
    pub custom_error: String,
}

/// What `agent_status` answers: whether an ask would be an agent ask
/// right now, and why not, in core's words (`agent::readiness`).
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
pub struct AgentStatus {
    pub ready: bool,
    pub message: String,
    pub ask_enabled: bool,
    /// `claude-cli` or `openai-compatible`.
    pub backend: &'static str,
}

/// An app that is open now, for adding a row to the app table.
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
pub struct RunningApp {
    pub app_id: String,
    pub app_name: String,
}

// --- what a window may change --------------------------------------

/// The person's choices, as the Agent pane sends them. Not a whole
/// block: see the module docs. A field a payload leaves out is the
/// careful choice ([`AgentChoices::default`]), never the open one.
#[derive(serde::Deserialize, Clone, Debug)]
#[serde(default)]
pub struct AgentChoices {
    pub enabled: bool,
    pub drafts_only: bool,
    pub connections: Vec<ConnectionChoice>,
    /// `None` = the built-in table.
    pub app_connections: Option<Vec<AppConnection>>,
}

/// The careful choices, like `AgentSettings::default()` in core:
/// Agent off, drafts only on, the built-in table.
impl Default for AgentChoices {
    fn default() -> Self {
        Self { enabled: false, drafts_only: true, connections: Vec::new(), app_connections: None }
    }
}

/// The switch and the ticked tools of one connection.
#[derive(serde::Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct ConnectionChoice {
    pub name: String,
    pub enabled: bool,
    pub allowed_tools: Vec<String>,
}

fn key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// `stored` with the person's `choices` laid over it.
///
/// - The two switches and the app table are taken as sent.
/// - A connection that is in both gets the switch and the ticks of
///   the choice. A tick counts only for a tool the connection is
///   known to have, or one that was ticked already: nobody can tick
///   a tool that has not been seen.
/// - A connection that is only in `choices` is ignored, and one that
///   is only in `stored` stays as it is: a window adds and removes
///   connections through their own commands.
/// - What a connection is (its kind, its tools, how it is reached)
///   is never taken from `choices`.
fn with_choices(mut stored: AgentSettings, choices: &AgentChoices) -> AgentSettings {
    stored.enabled = choices.enabled;
    stored.drafts_only = choices.drafts_only;
    stored.app_connections = choices.app_connections.clone();
    for connection in &mut stored.connections {
        let Some(choice) = choices.connections.iter().find(|c| key(&c.name) == key(&connection.name)) else {
            continue;
        };
        connection.enabled = choice.enabled;
        let ticked: Vec<String> = choice
            .allowed_tools
            .iter()
            .filter(|tool| connection.tool(tool).is_some() || connection.allows(tool))
            .cloned()
            .collect();
        connection.allowed_tools = ticked;
    }
    stored
}

/// `stored` with a connection of the person's own added: switched
/// off, nothing ticked, no tools known until the next listing. The
/// rules are those of `openwhisper agent connections add-custom`.
fn with_custom(
    mut stored: AgentSettings,
    name: &str,
    command: &str,
    args: Vec<String>,
    url: &str,
) -> Result<AgentSettings, String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err("Give the connection a name.".into());
    }
    if stored.connection(&name).is_some() {
        return Err(format!("There is already a connection called \"{name}\"."));
    }
    let custom = CustomServer {
        command: command.trim().to_string(),
        args: args.into_iter().filter(|a| !a.trim().is_empty()).collect(),
        url: url.trim().to_string(),
        ..CustomServer::default()
    };
    if !custom.is_usable() {
        return Err("Give the connection a command or a URL.".into());
    }
    stored.connections.push(AgentConnection {
        name,
        kind: AgentConnectionKind::Custom,
        custom: Some(custom),
        ..AgentConnection::default()
    });
    Ok(stored)
}

// --- the last listing: memory only ---------------------------------

/// What the last listing of this run of the app found: the state of
/// each connection, by name. Nothing about how one is reached.
#[derive(Default)]
struct Listing {
    /// By name in lower case, and whether it is the person's own
    /// entry.
    found: HashMap<(String, bool), ConnectionState>,
    view: ListingView,
}

static LISTING: Mutex<Option<Listing>> = Mutex::new(None);

fn listing<R>(f: impl FnOnce(&mut Listing) -> R) -> R {
    let mut guard = LISTING.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Listing::default))
}

/// Listings asked for and not over yet.
static PENDING: AtomicUsize = AtomicUsize::new(0);
/// Listings that ran to their end.
static RUNS: AtomicU64 = AtomicU64::new(0);

/// A listing that was asked for. Counted from the moment it is asked
/// for, so the state a command returns right after says "refreshing".
struct Pending;

impl Pending {
    fn new() -> Self {
        PENDING.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        PENDING.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The word for where `connection` stood in the last listing.
fn state_word(connection: &AgentConnection, last: &Listing) -> String {
    let custom = connection.kind == AgentConnectionKind::Custom;
    match last.found.get(&(key(&connection.name), custom)) {
        Some(state) => state.as_str().to_string(),
        None if last.view.refreshed_at_ms.is_none() => "not-looked-up".to_string(),
        // The person's own entries are listed apart, and that part
        // can fail while the rest worked.
        None if custom => ConnectionState::Unknown.as_str().to_string(),
        None => "missing".to_string(),
    }
}

/// The state a window is shown, from the stored block and the last
/// listing. `is_listed` says whether core knows how to reach a
/// connection of the person's Claude Code.
fn state(settings: &AgentSettings, last: &Listing, refreshing: bool, is_listed: impl Fn(&str) -> bool) -> AgentState {
    let connections = settings
        .connections
        .iter()
        .map(|connection| {
            let custom = connection.kind == AgentConnectionKind::Custom;
            ConnectionView {
                name: connection.name.clone(),
                kind: connection.kind,
                enabled: connection.enabled,
                allowed_tools: connection.allowed_tools.clone(),
                tools: connection
                    .tools
                    .iter()
                    .map(|tool| ToolView { name: tool.name.clone(), title: tool.title.clone(), kind: policy::kind_of(tool) })
                    .collect(),
                state: state_word(connection, last),
                usable: custom || is_listed(&connection.name),
                via: connection.custom.as_ref().filter(|_| custom).map(|c| {
                    if c.command.trim().is_empty() {
                        "url"
                    } else {
                        "command"
                    }
                }),
            }
        })
        .collect();
    let app_connections = settings
        .effective_app_connections()
        .into_iter()
        .map(|entry| {
            let known = settings
                .connections
                .iter()
                .find(|c| core_settings::connection_name_matches(&entry.connection, &c.name))
                .map(|c| c.name.clone());
            // Core's own routing, asked with this app in front.
            let active = settings.connection_for_app(&entry.app_id).is_some()
                && AgentRun::new(settings, Some(&FrontApp::new(&entry.app_id, &entry.app_name)))
                    .is_some_and(|run| run.route.is_some());
            AppRow { app_id: entry.app_id, app_name: entry.app_name, connection: entry.connection, known, active }
        })
        .collect();
    AgentState {
        enabled: settings.enabled,
        drafts_only: settings.drafts_only,
        confirm_timeout_s: settings.confirm_timeout_s,
        max_turns: settings.max_turns,
        connections,
        app_connections,
        app_connections_builtin: settings.app_connections.is_none(),
        listing: ListingView { refreshing, ..last.view.clone() },
    }
}

/// [`state`] as things stand in this process.
fn current_state(settings: &AgentSettings) -> AgentState {
    let refreshing = PENDING.load(Ordering::SeqCst) > 0;
    listing(|last| state(settings, last, refreshing, agent::connections::is_listed))
}

/// Held by everything in this process that reads the Agent block to
/// change it, or puts one into core's cache. A change is read, edit
/// and save; without the lock two of them could each read the same
/// block and the second save would undo the first. Same reason as
/// the locks in `ask.rs`, `vision.rs` and `modes.rs`.
fn agent_lock() -> MutexGuard<'static, ()> {
    static AGENT: Mutex<()> = Mutex::new(());
    AGENT.lock().unwrap_or_else(|e| e.into_inner())
}

/// Tell the windows, and answer with what they were told.
fn announce(app: &AppHandle, settings: &AgentSettings) -> AgentState {
    let now = current_state(settings);
    if let Err(e) = app.emit(AGENT_CHANGED_EVENT, &now) {
        eprintln!("[agent] {AGENT_CHANGED_EVENT} emit failed: {e}");
    }
    now
}

/// Agent went from off to on: the connections have to be looked up
/// before an ask can use one.
fn refresh_due(before: &AgentSettings, now: &AgentSettings) -> bool {
    !before.enabled && now.enabled
}

// --- listing -------------------------------------------------------

/// What a listing found, kept for the pane.
fn remember(found: &[ListedConnection], custom_error: &str) {
    listing(|last| {
        last.found = found
            .iter()
            .map(|c| ((key(&c.name), c.source == agent::ConnectionSource::Custom), c.state))
            .collect();
        last.view.refreshed_at_ms = Some(openwhisper_core::history::now_ms());
        last.view.error = String::new();
        last.view.custom_error = custom_error.to_string();
    });
}

/// List the connections and bring the Agent block at `path` up to
/// date (`agent::refresh`). One at a time: a caller that waited for
/// another listing takes that one's result instead of starting its
/// own. Blocks for a few seconds. A failure is kept for the pane
/// ([`ListingView::error`]) and changes nothing on disk.
fn run_refresh(path: &Path) {
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
    let seen = RUNS.load(Ordering::SeqCst);
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    if RUNS.load(Ordering::SeqCst) != seen {
        return;
    }
    let cli = core_settings::current_refine_settings().claude_cli;
    match agent::refresh(path, &cli, agent::LIST_WAIT) {
        Ok(refreshed) => {
            remember(&refreshed.listing, &refreshed.custom_error);
            // Core read the block, merged and saved without this
            // module's lock. A change of ours that was between its
            // own read and save at that moment wrote the tool lists
            // from before the listing back; merging once more, under
            // the lock, puts them right. Usually there is nothing to
            // do.
            let _held = agent_lock();
            let current = core_settings::load_agent_settings(path);
            let merged = agent::merge_listing(&current, &refreshed.listing, true);
            if merged != current {
                if let Err(e) = core_settings::save_agent_settings(path, merged) {
                    eprintln!("[agent] the listing could not be saved: {e}");
                }
            }
        }
        Err(e) => {
            eprintln!("[agent] the connections could not be listed: {e}");
            listing(|last| last.view.error = e.to_string());
        }
    }
    RUNS.fetch_add(1, Ordering::SeqCst);
}

/// [`run_refresh`], then tell the windows. `pending` is given up
/// before they are told, so what they are told says the listing is
/// over.
fn refresh_and_announce(app: &AppHandle, pending: Pending) -> Result<AgentState, String> {
    let path = settings_path(app)?;
    run_refresh(&path);
    drop(pending);
    let settings = {
        let _held = agent_lock();
        core_settings::load_agent_settings(&path)
    };
    Ok(announce(app, &settings))
}

/// Look the connections up on a thread of its own.
fn refresh_in_background(app: &AppHandle) {
    let pending = Pending::new();
    let app = app.clone();
    let spawned = thread::Builder::new().name("openwhisper-agent-refresh".into()).spawn(move || {
        if let Err(e) = refresh_and_announce(&app, pending) {
            eprintln!("[agent] refresh: {e}");
        }
    });
    if let Err(e) = spawned {
        eprintln!("[agent] refresh thread: {e}");
    }
}

/// Boot-time: load the Agent block into core's cache, and when Agent
/// is on, look the connections up in the background. Without that
/// the first ask that wants a connection of the person's Claude Code
/// would fail and ask for a refresh.
///
/// Call it after `vision::init`, which tells core where the settings
/// file is.
pub fn init(app: &AppHandle) {
    let Ok(path) = settings_path(app) else {
        return;
    };
    let settings = {
        let _held = agent_lock();
        core_settings::load_agent_settings(&path)
    };
    if settings.enabled {
        refresh_in_background(app);
    }
}

// --- commands ------------------------------------------------------

/// Fresh read from disk (the block is also edited by the headless
/// CLI). Starts nothing and calls no tool. Off the UI thread for the
/// file read.
#[tauri::command]
pub async fn agent_get(app: AppHandle) -> Result<AgentState, String> {
    let path = settings_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (before, settings) = {
            let _held = agent_lock();
            (core_settings::current_agent_settings(), core_settings::load_agent_settings(&path))
        };
        // The file was changed behind this process's back.
        if before != settings {
            if refresh_due(&before, &settings) {
                refresh_in_background(&app);
            }
            return announce(&app, &settings);
        }
        current_state(&settings)
    })
    .await
    .map_err(|e| format!("agent settings: {e}"))
}

/// Save the person's choices over the block on disk ([`with_choices`])
/// and answer with the new state. Emits `agent_changed`. When Agent
/// was switched on by this, the connections are looked up in the
/// background.
///
/// Not an async command, like `ask_set` and `vision_set`: the pane
/// sends one of these per click, each with every choice in it, and
/// they have to be applied in the order they were made.
#[tauri::command]
pub fn agent_set(app: AppHandle, choices: AgentChoices) -> Result<AgentState, String> {
    change(&app, |stored| Ok(with_choices(stored, &choices)))
}

/// Read the block, change it and save it, as one step.
fn change(
    app: &AppHandle,
    edit: impl FnOnce(AgentSettings) -> Result<AgentSettings, String>,
) -> Result<AgentState, String> {
    let path = settings_path(app)?;
    let (before, saved) = {
        let _held = agent_lock();
        let before = core_settings::load_agent_settings(&path);
        let saved = core_settings::save_agent_settings(&path, edit(before.clone())?)?;
        (before, saved)
    };
    if refresh_due(&before, &saved) {
        refresh_in_background(app);
    }
    Ok(announce(app, &saved))
}

/// Ask Claude Code which connections it has, with their tools, and
/// bring the block up to date. No tool is called and the model is
/// not asked anything. Takes a few seconds, on a blocking thread.
/// Always answers with the state: a listing that failed says why in
/// `listing.error`.
#[tauri::command]
pub async fn agent_refresh(app: AppHandle) -> Result<AgentState, String> {
    let pending = Pending::new();
    tauri::async_runtime::spawn_blocking(move || refresh_and_announce(&app, pending))
        .await
        .map_err(|e| format!("agent refresh: {e}"))?
}

/// Whether an ask would use tools right now, and why not, in core's
/// words. Depends on the Ask switch and on the backend Refine is set
/// up with as well as on the Agent block.
#[tauri::command]
pub async fn agent_status(app: AppHandle) -> Result<AgentStatus, String> {
    let path = settings_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let settings = {
            let _held = agent_lock();
            core_settings::load_agent_settings(&path)
        };
        let ask_enabled = core_settings::current_ask_settings().enabled;
        let backend = core_settings::current_refine_settings().backend;
        let (ready, message) = agent::readiness(&settings, ask_enabled, backend);
        AgentStatus { ready, message, ask_enabled, backend: backend.as_str() }
    })
    .await
    .map_err(|e| format!("agent status: {e}"))
}

/// Add a connection of the person's own: a command with its
/// arguments, or a URL. It arrives switched off with nothing ticked,
/// and its tools are not known until the next refresh. Variables for
/// its environment are not taken here: `openwhisper agent
/// connections add-custom --env` sets them.
#[tauri::command]
pub fn agent_add_connection(
    app: AppHandle,
    name: String,
    command: String,
    args: Vec<String>,
    url: String,
) -> Result<AgentState, String> {
    let wanted = key(&name.split_whitespace().collect::<Vec<_>>().join(" "));
    let added = change(&app, |stored| with_custom(stored, &name, &command, args, &url))?;
    // Core drops an entry it cannot keep (too many connections).
    if !added.connections.iter().any(|c| key(&c.name) == wanted) {
        return Err("The connection could not be saved.".into());
    }
    Ok(added)
}

/// Remove a connection from the block, with its ticks. A connection
/// of the person's Claude Code comes back, switched off, with the
/// next refresh for as long as Claude Code has it.
#[tauri::command]
pub fn agent_remove_connection(app: AppHandle, name: String) -> Result<AgentState, String> {
    change(&app, |mut stored| {
        let Some(found) = stored.connection(&name).map(|c| c.name.clone()) else {
            return Err(format!("There is no connection called \"{}\".", name.trim()));
        };
        stored.connections.retain(|c| c.name != found);
        Ok(stored)
    })
}

/// The apps that are open now, for adding one to the app table. Ids
/// are in lower case, like the ids the table holds. Asks the OS which
/// apps run, so off the UI thread.
#[tauri::command]
pub async fn agent_running_apps() -> Result<Vec<RunningApp>, String> {
    tauri::async_runtime::spawn_blocking(|| running(focus::running_apps()))
        .await
        .map_err(|e| format!("agent apps: {e}"))
}

fn running(apps: Vec<(String, String)>) -> Vec<RunningApp> {
    apps.into_iter().map(|(id, name)| RunningApp { app_id: id.to_lowercase(), app_name: name }).collect()
}

/// Yes to the confirm `request_id` of the ask `ask_id`: the tool runs
/// with exactly the input the panel showed. Only while that confirm
/// is the one the ask waits on (`ask::confirm_allow`); returns
/// whether it was.
#[tauri::command]
pub fn ask_confirm_allow(ask_id: u64, request_id: String) -> bool {
    ask::confirm_allow(ask_id, &request_id)
}

/// No to the confirm `request_id` of the ask `ask_id`: the tool does
/// not run and the ask goes on to its answer (`ask::confirm_deny`).
#[tauri::command]
pub fn ask_confirm_deny(ask_id: u64, request_id: String) -> bool {
    ask::confirm_deny(ask_id, &request_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openwhisper_core::settings::ToolInfo;
    use std::collections::BTreeMap;

    /// This file up to its tests: the part that ships.
    fn shipped() -> &'static str {
        let source = include_str!("agent.rs");
        let end = source.find("#[cfg(test)]\nmod tests").expect("tests module");
        &source[..end]
    }

    /// The names of the functions marked `#[tauri::command]` in
    /// `source`, in order.
    fn command_names(source: &str) -> Vec<String> {
        let mut names = Vec::new();
        let mut rest = source;
        while let Some(at) = rest.find("#[tauri::command") {
            rest = &rest[at + 1..];
            let Some(f) = rest.find("fn ") else { break };
            names.push(rest[f + 3..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect());
        }
        names
    }

    fn chat(name: &str) -> AgentConnection {
        AgentConnection {
            name: name.into(),
            tools: vec![
                ToolInfo { read_only: Some(true), title: "Read thread".into(), ..ToolInfo::new("read_thread") },
                ToolInfo::new("create_draft"),
                ToolInfo::new("send_message"),
            ],
            ..AgentConnection::default()
        }
    }

    fn own(name: &str) -> AgentConnection {
        AgentConnection {
            name: name.into(),
            kind: AgentConnectionKind::Custom,
            custom: Some(CustomServer {
                command: "/opt/notes/server".into(),
                args: vec!["--token".into(), "arg-secret-1".into()],
                env: BTreeMap::from([("NOTES_TOKEN".to_string(), "env-secret-2".to_string())]),
                url: String::new(),
            }),
            tools: vec![ToolInfo::new("list_notes")],
            ..AgentConnection::default()
        }
    }

    fn settings(connections: Vec<AgentConnection>) -> AgentSettings {
        AgentSettings { connections, ..AgentSettings::default() }
    }

    fn shown(settings: &AgentSettings) -> AgentState {
        state(settings, &Listing::default(), false, |_| false)
    }

    #[test]
    fn the_commands_of_this_module_are_the_nine_listed_and_all_are_registered() {
        assert_eq!(command_names(shipped()), COMMANDS);
        let lib = include_str!("lib.rs");
        let start = lib.find("generate_handler![").expect("handler list");
        let list = &lib[start..start + lib[start..].find(']').expect("end of handler list")];
        for name in COMMANDS {
            assert!(list.contains(&format!("agent::{name},")), "{name} is not registered");
        }
    }

    #[test]
    fn the_state_payload_is_the_choices_the_connections_the_table_and_the_listing() {
        let value = serde_json::to_value(shown(&settings(vec![chat("teamchat")]))).unwrap();
        let keys = |v: &serde_json::Value| {
            let mut keys: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
            keys.sort_unstable();
            keys
        };
        // The fields `AgentState` in src/lib/agent.ts mirrors.
        assert_eq!(
            keys(&value),
            [
                "app_connections",
                "app_connections_builtin",
                "confirm_timeout_s",
                "connections",
                "drafts_only",
                "enabled",
                "listing",
                "max_turns"
            ]
        );
        assert_eq!(
            keys(&value["connections"][0]),
            ["allowed_tools", "enabled", "kind", "name", "state", "tools", "usable", "via"]
        );
        assert_eq!(keys(&value["connections"][0]["tools"][0]), ["kind", "name", "title"]);
        assert_eq!(keys(&value["app_connections"][0]), ["active", "app_id", "app_name", "connection", "known"]);
        assert_eq!(keys(&value["listing"]), ["custom_error", "error", "refreshed_at_ms", "refreshing"]);
        // The careful defaults, and the words a window reads.
        assert_eq!(value["enabled"], false);
        assert_eq!(value["drafts_only"], true);
        assert_eq!(value["confirm_timeout_s"], 120);
        assert_eq!(value["app_connections_builtin"], true);
        assert_eq!(value["connections"][0]["kind"], "claude");
        assert_eq!(value["connections"][0]["state"], "not-looked-up");
        assert_eq!(value["connections"][0]["usable"], false);
        assert_eq!(value["connections"][0]["via"], serde_json::Value::Null);
        let kinds: Vec<&str> =
            value["connections"][0]["tools"].as_array().unwrap().iter().map(|t| t["kind"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["read", "draft", "act"]);
        assert_eq!(value["connections"][0]["tools"][0]["title"], "Read thread");
        assert_eq!(value["listing"]["refreshing"], false);
        assert_eq!(value["listing"]["refreshed_at_ms"], serde_json::Value::Null);
    }

    #[test]
    fn how_a_connection_is_reached_never_reaches_a_window() {
        let text = serde_json::to_string(&shown(&settings(vec![chat("teamchat"), own("notes")]))).unwrap();
        for secret in ["arg-secret-1", "env-secret-2", "NOTES_TOKEN", "/opt/notes/server", "--token"] {
            assert!(!text.contains(secret), "{secret} is in the payload");
        }
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["connections"][1]["kind"], "custom");
        assert_eq!(value["connections"][1]["via"], "command");
        // The person's own entry needs no listing to be used.
        assert_eq!(value["connections"][1]["usable"], true);
        // By URL: the word, not the address.
        let mut by_url = own("notes");
        by_url.custom = Some(CustomServer { url: "https://notes.example/mcp?key=url-secret-3".into(), ..CustomServer::default() });
        let text = serde_json::to_string(&shown(&settings(vec![by_url]))).unwrap();
        assert!(!text.contains("url-secret-3") && !text.contains("notes.example"), "{text}");
        assert!(text.contains(r#""via":"url""#), "{text}");
        // And nothing in this file reads the details core keeps in
        // memory, or names the file a run is started with.
        for word in ["write_run_config", "RunConfig", ".env", "headers"] {
            let uses = shipped().lines().filter(|l| !l.trim_start().starts_with("//")).any(|l| l.contains(word));
            assert!(!uses, "the shell names {word}");
        }
    }

    #[test]
    fn choices_change_the_switches_the_ticks_and_the_table_and_nothing_else() {
        let stored = AgentSettings { confirm_timeout_s: 60, ..settings(vec![chat("teamchat"), own("notes")]) };
        let choices = AgentChoices {
            enabled: true,
            drafts_only: false,
            connections: vec![
                ConnectionChoice {
                    name: " TeamChat ".into(),
                    enabled: true,
                    // The last one is not a tool anybody has seen.
                    allowed_tools: vec!["read_thread".into(), "send_message".into(), "wipe_everything".into()],
                },
                // Not a connection on disk: a window cannot add one
                // this way.
                ConnectionChoice { name: "sneaked".into(), enabled: true, allowed_tools: vec!["run".into()] },
            ],
            app_connections: Some(vec![AppConnection::new("com.example.chat", "Chat", "teamchat")]),
        };
        let merged = with_choices(stored.clone(), &choices);
        assert!(merged.enabled && !merged.drafts_only);
        assert_eq!(merged.confirm_timeout_s, 60);
        assert_eq!(merged.connections.len(), 2);
        let teamchat = &merged.connections[0];
        assert!(teamchat.enabled);
        assert_eq!(teamchat.allowed_tools, ["read_thread", "send_message"]);
        assert_eq!(teamchat.tools, stored.connections[0].tools);
        // Not named in the choices: untouched, details and all.
        assert_eq!(merged.connections[1], stored.connections[1]);
        assert_eq!(merged.app_connections, choices.app_connections);
        // A tick that was there stays there, seen or not, until it
        // is taken away.
        let mut old = stored.clone();
        old.connections[0].allowed_tools = vec!["gone_tool".into()];
        let kept = with_choices(
            old,
            &AgentChoices {
                connections: vec![ConnectionChoice {
                    name: "teamchat".into(),
                    enabled: false,
                    allowed_tools: vec!["gone_tool".into(), "read_thread".into()],
                }],
                ..AgentChoices::default()
            },
        );
        assert_eq!(kept.connections[0].allowed_tools, ["gone_tool", "read_thread"]);
        // No table = the built-in one.
        assert_eq!(with_choices(merged, &AgentChoices::default()).app_connections, None);
    }

    #[test]
    fn a_payload_that_names_nothing_switches_nothing_on() {
        let choices: AgentChoices = serde_json::from_str("{}").unwrap();
        assert!(!choices.enabled);
        // Left out is drafts only on, not off.
        assert!(choices.drafts_only);
        let open = AgentSettings { enabled: true, drafts_only: false, ..settings(vec![chat("teamchat")]) };
        let merged = with_choices(open, &choices);
        assert!(!merged.enabled);
        assert!(merged.drafts_only);
        assert!(!merged.connections[0].enabled);
        // A switch that is named is taken as named.
        let named: AgentChoices = serde_json::from_str(r#"{"enabled":true,"drafts_only":false}"#).unwrap();
        assert!(named.enabled && !named.drafts_only);
    }

    #[test]
    fn a_connection_of_the_persons_own_needs_a_name_of_its_own_and_a_way_to_reach_it() {
        let stored = settings(vec![chat("teamchat")]);
        let add = |name: &str, command: &str, url: &str| {
            with_custom(stored.clone(), name, command, vec!["--stdio".into(), "  ".into()], url)
        };
        assert_eq!(add("  ", "notes-server", "").unwrap_err(), "Give the connection a name.");
        assert!(add("TeamChat", "notes-server", "").unwrap_err().contains("already a connection"));
        assert_eq!(add("notes", " ", "").unwrap_err(), "Give the connection a command or a URL.");
        let added = add(" my   notes ", " notes-server ", "").unwrap();
        let notes = added.connection("my notes").expect("added");
        assert_eq!(notes.kind, AgentConnectionKind::Custom);
        // Off, nothing ticked, no tools known.
        assert!(!notes.enabled && notes.allowed_tools.is_empty() && notes.tools.is_empty());
        let custom = notes.custom.as_ref().unwrap();
        assert_eq!((custom.command.as_str(), custom.args.as_slice()), ("notes-server", &["--stdio".to_string()][..]));
        // Core keeps it as it was added.
        assert_eq!(added.clone().cleaned(), added);
        assert!(add("web notes", "", "https://notes.example/mcp").is_ok());
    }

    #[test]
    fn a_connection_says_where_it_stood_in_the_last_listing() {
        let s = settings(vec![chat("teamchat"), chat("tracker"), own("notes")]);
        let words = |last: &Listing| -> Vec<String> {
            state(&s, last, false, |name| name == "teamchat").connections.iter().map(|c| c.state.clone()).collect()
        };
        // Before the first listing of this run.
        assert_eq!(words(&Listing::default()), ["not-looked-up", "not-looked-up", "not-looked-up"]);
        let mut last = Listing::default();
        last.view.refreshed_at_ms = Some(1);
        last.found.insert(("teamchat".into(), false), ConnectionState::NeedsAuth);
        // A connection of Claude Code the listing did not find is
        // gone; the person's own entries are listed apart.
        assert_eq!(words(&last), ["needs-auth", "missing", "unknown"]);
        last.found.insert(("notes".into(), true), ConnectionState::Connected);
        // The same name as an entry of the other kind is not it.
        last.found.insert(("tracker".into(), true), ConnectionState::Connected);
        assert_eq!(words(&last), ["needs-auth", "missing", "connected"]);
        let now = state(&s, &last, true, |name| name == "teamchat");
        assert_eq!(now.connections.iter().map(|c| c.usable).collect::<Vec<_>>(), [true, false, true]);
        assert!(now.listing.refreshing);
        assert_eq!(now.listing.refreshed_at_ms, Some(1));
    }

    #[test]
    fn the_app_table_says_which_rows_an_ask_would_follow() {
        let mut teamchat = chat("teamchat");
        teamchat.enabled = true;
        teamchat.allowed_tools = vec!["read_thread".into()];
        let mut idle = chat("tracker");
        idle.enabled = true;
        let s = AgentSettings {
            app_connections: Some(vec![
                AppConnection::new("com.example.chat", "Chat", "TeamChat"),
                AppConnection::new("com.example.tracker", "Tracker", "tracker"),
                AppConnection::new("com.example.gone", "Gone", "nowhere"),
            ]),
            ..settings(vec![teamchat, idle])
        };
        let now = shown(&s);
        assert!(!now.app_connections_builtin);
        let rows: Vec<(&str, Option<&str>, bool)> =
            now.app_connections.iter().map(|r| (r.app_id.as_str(), r.known.as_deref(), r.active)).collect();
        assert_eq!(
            rows,
            [
                ("com.example.chat", Some("teamchat"), true),
                // On, but nothing ticked: an ask would not follow it.
                ("com.example.tracker", Some("tracker"), false),
                ("com.example.gone", None, false),
            ]
        );
        // The built-in table, for someone who never edited it.
        let builtin = shown(&settings(vec![]));
        assert!(builtin.app_connections_builtin);
        assert_eq!(builtin.app_connections.len(), core_settings::default_app_connections().len());
        assert!(builtin.app_connections.iter().all(|r| r.known.is_none() && !r.active));
    }

    #[test]
    fn the_connections_are_looked_up_when_agent_goes_on() {
        let off = AgentSettings::default();
        let on = AgentSettings { enabled: true, ..AgentSettings::default() };
        assert!(refresh_due(&off, &on));
        assert!(!refresh_due(&on, &on));
        assert!(!refresh_due(&on, &off));
        assert!(!refresh_due(&off, &off));
    }

    #[test]
    fn a_listing_counts_as_running_from_the_moment_it_is_asked_for() {
        let before = PENDING.load(Ordering::SeqCst);
        let pending = Pending::new();
        assert_eq!(PENDING.load(Ordering::SeqCst), before + 1);
        drop(pending);
        assert_eq!(PENDING.load(Ordering::SeqCst), before);
    }

    #[test]
    fn open_apps_are_offered_with_their_ids_in_lower_case() {
        let apps = running(vec![("com.Example.Chat".into(), "Chat".into())]);
        assert_eq!(apps, [RunningApp { app_id: "com.example.chat".into(), app_name: "Chat".into() }]);
        let value = serde_json::to_value(&apps[0]).unwrap();
        assert_eq!(value, serde_json::json!({ "app_id": "com.example.chat", "app_name": "Chat" }));
    }

    #[test]
    fn the_status_payload_is_cores_readiness() {
        let (ready, message) =
            agent::readiness(&AgentSettings::default(), true, openwhisper_core::settings::RefineBackendKind::ClaudeCli);
        let value = serde_json::to_value(AgentStatus { ready, message, ask_enabled: true, backend: "claude-cli" }).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "ready": false,
                "message": "Agent is switched off.",
                "ask_enabled": true,
                "backend": "claude-cli",
            })
        );
    }
}
