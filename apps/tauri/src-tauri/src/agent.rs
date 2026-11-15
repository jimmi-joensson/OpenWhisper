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
//! What it sends is one change ([`AgentPatch`]: a switch, one
//! connection's switch or ticks, the app table), and it is laid over
//! the block as it is on disk at that moment ([`with_patch`]), so a
//! change made elsewhere in the meantime (the headless CLI) is kept.
//!
//! A window can never add a connection: a connection of the person's
//! own is a command that is started on this computer, so it is added
//! in a terminal only (`openwhisper agent connections add-custom`).
//! What a connection is never changes from a window either.
//!
//! # Which window may call what
//!
//! A yes or no to a confirm is accepted from the answer window only,
//! and a change of the settings from the main window only
//! ([`check_caller`]). Reading is open to every window.
//!
//! # Looking the connections up
//!
//! An ask can only use a connection of the person's Claude Code after
//! this process has listed it (core keeps how it is reached in
//! memory, see above). A listing starts the Claude CLI without a
//! prompt: no tool is called and the model is not asked anything, but
//! every connection Claude Code has is started. So it runs only while
//! Agent could use the result ([`lookup_refusal`]: Agent on, Ask on,
//! the Claude backend): once in the background when that becomes
//! true ([`lookup_due`]), and when the person presses Refresh. It
//! takes a few seconds, so it never runs on the main thread, and only
//! one runs at a time ([`Turns`]).

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread;

use openwhisper_core::agent::{self, policy, AgentError, AgentRun, ConnectionState, FrontApp, ToolKind};
use openwhisper_core::ask;
use openwhisper_core::focus;
use openwhisper_core::settings::{
    self as core_settings, AgentConnection, AgentConnectionKind, AgentSettings, AppConnection, RefineBackendKind,
};
use tauri::{AppHandle, Emitter, WebviewWindow};

use crate::settings::settings_path;

/// Emitted to every window after the Agent settings changed or a
/// listing ended. Payload: [`AgentState`].
pub(crate) const AGENT_CHANGED_EVENT: &str = "agent_changed";

/// Label of the main window, where the Agent pane is.
const MAIN_LABEL: &str = "main";

/// Every command of this module.
#[cfg(test)]
const COMMANDS: [&str; 8] = [
    "agent_get",
    "agent_set",
    "agent_refresh",
    "agent_status",
    "agent_remove_connection",
    "agent_running_apps",
    "ask_confirm_allow",
    "ask_confirm_deny",
];

// --- which window may call what ------------------------------------

/// The one window `command` is accepted from. `None` for a command
/// that only reads: every window may call it.
fn window_of(command: &str) -> Option<&'static str> {
    match command {
        // A yes or no is given where the confirm is shown, with its
        // whole input in front of the person.
        "ask_confirm_allow" | "ask_confirm_deny" => Some(crate::ask::ANSWER_LABEL),
        // The settings are changed where the Agent pane is.
        "agent_set" | "agent_refresh" | "agent_remove_connection" => Some(MAIN_LABEL),
        _ => None,
    }
}

/// Refuse `command` when it comes from the window `label` and that is
/// not the window it is accepted from ([`window_of`]).
fn check_caller(command: &str, label: &str) -> Result<(), String> {
    match window_of(command) {
        Some(wanted) if wanted != label => Err(format!("{command} is not accepted from this window.")),
        _ => Ok(()),
    }
}

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
    /// no listing yet, or it is an entry of the person's own that was
    /// switched off, which a listing never starts).
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
    /// Why the tool cannot be used at all, as core's sentence for the
    /// person (`policy::unusable_reason`); `None` when it can. Such a
    /// tool is shown and can never be ticked.
    pub problem: Option<&'static str>,
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
    /// they were, or when none is switched on.
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

/// One change from the Agent pane. Only what is named is changed:
/// a field that is left out stays as it is on disk. So a window never
/// writes back what it happened to have on screen, and leaving
/// `drafts_only` out can never switch it off: only an explicit
/// `false` does.
#[derive(serde::Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AgentPatch {
    pub enabled: Option<bool>,
    pub drafts_only: Option<bool>,
    /// A change to one connection.
    pub connection: Option<ConnectionPatch>,
    /// The whole app table. Named and `null`: back to the built-in
    /// table. Left out: not changed.
    #[serde(deserialize_with = "named")]
    pub app_connections: Option<Option<Vec<AppConnection>>>,
}

/// A field that is in the payload, `null` included, as `Some`.
fn named<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    serde::Deserialize::deserialize(deserializer).map(Some)
}

/// A change to one connection: its switch, and tools to tick or to
/// untick. The other ticks of the connection stay as they are.
#[derive(serde::Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ConnectionPatch {
    pub name: String,
    pub enabled: Option<bool>,
    pub tick: Vec<String>,
    pub untick: Vec<String>,
}

fn key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Why the tool `name` of `connection` may not be ticked, as a
/// sentence for the person; `None` when it may.
///
/// - A tool nobody has seen cannot be ticked.
/// - A tool core cannot use at all (`policy::unusable_reason`) can
///   never be ticked.
/// - While drafts only is on, a tool that acts cannot be ticked: it
///   could not run, and a tick made now would come alive unseen the
///   day drafts only is switched off.
fn tick_refusal(connection: &AgentConnection, drafts_only: bool, name: &str) -> Option<String> {
    let shown = ask::safe_to_show(name, false);
    let Some(tool) = connection.tool(name) else {
        return Some(format!("\"{shown}\" is not a tool this connection is known to have. Refresh and try again."));
    };
    if let Some(problem) = policy::unusable_reason(connection, tool) {
        return Some(format!("\"{shown}\" cannot be ticked. {}", problem.sentence()));
    }
    if drafts_only && policy::kind_of(tool) == ToolKind::Act {
        return Some(format!(
            "\"{shown}\" sends, posts, deletes or changes something, so it cannot be ticked while Drafts only is on."
        ));
    }
    None
}

/// `stored` with one change from a window laid over it, or why the
/// change is refused (nothing is changed then).
///
/// - Only what the patch names is changed.
/// - A connection that is not in `stored` cannot be changed: a window
///   adds none, and removes one through its own command.
/// - A tick is taken only when [`tick_refusal`] has nothing against
///   it, judged with drafts only as it is after this patch. A tool
///   that is ticked already stays ticked without being asked about
///   again. An untick is always taken.
/// - What a connection is (its kind, its tools, how it is reached)
///   is never taken from a window.
fn with_patch(mut stored: AgentSettings, patch: &AgentPatch) -> Result<AgentSettings, String> {
    if let Some(on) = patch.enabled {
        stored.enabled = on;
    }
    if let Some(on) = patch.drafts_only {
        stored.drafts_only = on;
    }
    if let Some(table) = &patch.app_connections {
        stored.app_connections = table.clone();
    }
    if let Some(change) = &patch.connection {
        let drafts_only = stored.drafts_only;
        let Some(connection) = stored.connection_mut(&change.name) else {
            return Err(format!(
                "There is no connection called \"{}\".",
                ask::safe_to_show(change.name.trim(), false)
            ));
        };
        if let Some(on) = change.enabled {
            connection.enabled = on;
        }
        connection.allowed_tools.retain(|ticked| !change.untick.contains(ticked));
        for name in &change.tick {
            if connection.allows(name) {
                continue;
            }
            if let Some(why) = tick_refusal(connection, drafts_only, name) {
                return Err(why);
            }
            connection.allowed_tools.push(name.clone());
        }
    }
    Ok(stored)
}

// --- the last listing: memory only ---------------------------------

/// What the last listing of this run of the app found: the state of
/// each connection, by name. Nothing about how one is reached.
#[derive(Default)]
struct LastListing {
    /// By name in lower case, and whether it is the person's own
    /// entry.
    found: HashMap<(String, bool), ConnectionState>,
    view: ListingView,
}

static LISTING: Mutex<Option<LastListing>> = Mutex::new(None);

fn listing<R>(f: impl FnOnce(&mut LastListing) -> R) -> R {
    let mut guard = LISTING.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(LastListing::default))
}

/// Longest reason a failed listing is shown and logged with.
const ERROR_MAX_CHARS: usize = 300;

/// Why a listing failed, as it may be shown and logged. The text can
/// carry a line the Claude CLI wrote, which is not ours: it is put on
/// one line, made safe to show (`ask::safe_to_show`) and capped.
fn shown_error(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let safe = ask::safe_to_show(&flat, false);
    if safe.chars().count() <= ERROR_MAX_CHARS {
        return safe;
    }
    let mut cut: String = safe.chars().take(ERROR_MAX_CHARS - 1).collect();
    cut.push('\u{2026}');
    cut
}

impl LastListing {
    /// Keep what a listing that ended at `now_ms` came to. One that
    /// worked replaces what was found before; one that failed says
    /// why and leaves the rest as it was.
    fn took(&mut self, outcome: &Result<agent::Listing, AgentError>, now_ms: i64) {
        match outcome {
            Ok(found) => {
                self.found = found
                    .connections
                    .iter()
                    .map(|c| ((key(&c.name), c.source == agent::ConnectionSource::Custom), c.state))
                    .collect();
                self.view.refreshed_at_ms = Some(now_ms);
                self.view.error = String::new();
                self.view.custom_error = shown_error(&found.custom_error);
            }
            Err(e) => self.view.error = shown_error(&e.to_string()),
        }
    }
}

/// Listings asked for and not over yet.
static PENDING: AtomicUsize = AtomicUsize::new(0);

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
fn state_word(connection: &AgentConnection, last: &LastListing) -> String {
    let custom = connection.kind == AgentConnectionKind::Custom;
    match last.found.get(&(key(&connection.name), custom)) {
        Some(state) => state.as_str().to_string(),
        None if last.view.refreshed_at_ms.is_none() => "not-looked-up".to_string(),
        // The person's own entries are listed apart, only those that
        // are switched on, and that part can fail while the rest
        // worked. One that was tried and is not there: not known.
        None if custom && connection.enabled && !last.view.custom_error.is_empty() => {
            ConnectionState::Unknown.as_str().to_string()
        }
        // Switched off (a listing never starts it), or switched on
        // after the last listing.
        None if custom => "not-looked-up".to_string(),
        None => "missing".to_string(),
    }
}

/// The state a window is shown, from the stored block and the last
/// listing. `is_listed` says whether core knows how to reach a
/// connection of the person's Claude Code.
fn state(settings: &AgentSettings, last: &LastListing, refreshing: bool, is_listed: impl Fn(&str) -> bool) -> AgentState {
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
                    .map(|tool| ToolView {
                        name: tool.name.clone(),
                        title: tool.title.clone(),
                        kind: policy::kind_of(tool),
                        problem: policy::unusable_reason(connection, tool).map(|p| p.sentence()),
                    })
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

// --- when the connections are looked up ----------------------------

/// Why the connections are not looked up right now, as a sentence
/// for the person; `None` when they can be. A lookup starts every
/// connection Claude Code has, so it only runs while an ask could
/// use what it finds: Agent on, Ask on, the Claude backend.
fn lookup_refusal(agent: &AgentSettings, ask_enabled: bool, backend: RefineBackendKind) -> Option<&'static str> {
    if !agent.enabled {
        Some("Agent is switched off, so the connections were not looked up.")
    } else if !ask_enabled {
        Some("Ask is switched off, so the connections were not looked up. Agent only works in an ask.")
    } else if backend == RefineBackendKind::OpenaiCompatible {
        Some("Agent needs the Claude backend, so the connections were not looked up.")
    } else {
        None
    }
}

/// [`lookup_refusal`] with the Ask switch and the backend as core
/// has them now.
fn lookup_refusal_now(agent: &AgentSettings) -> Option<&'static str> {
    lookup_refusal(
        agent,
        core_settings::current_ask_settings().enabled,
        core_settings::current_refine_settings().backend,
    )
}

/// Whether a lookup is due now. `asked` remembers that one was asked
/// for since Agent last became able to use connections (`able`: no
/// [`lookup_refusal`]).
///
/// Due once each time `able` goes from false to true, whoever notices
/// first: the start of the app, a save from the pane, or any read
/// that finds the block (or the Ask switch, or the backend) changed
/// by someone else. Nothing here compares a block with the one
/// before, so it does not matter which command read the file first.
fn lookup_due(asked: &mut bool, able: bool) -> bool {
    if !able {
        *asked = false;
        return false;
    }
    !std::mem::replace(asked, true)
}

/// See [`lookup_due`].
static ASKED: Mutex<bool> = Mutex::new(false);

/// Look the connections up in the background when that is due
/// ([`lookup_due`]). Called with the Agent block wherever it was just
/// read or saved.
fn look_up_if_due(app: &AppHandle, settings: &AgentSettings) {
    let able = lookup_refusal_now(settings).is_none();
    let due = lookup_due(&mut ASKED.lock().unwrap_or_else(|e| e.into_inner()), able);
    if due {
        refresh_in_background(app);
    }
}

/// The Ask switch or the Refine backend changed: Agent may have
/// become able to use connections by it. Called by `ask.rs` and
/// `refine.rs` after a save.
pub(crate) fn conditions_changed(app: &AppHandle) {
    look_up_if_due(app, &core_settings::current_agent_settings());
}

// --- listing -------------------------------------------------------

/// Work that runs one at a time, where a caller that had to wait for
/// another's run takes that run's result instead of starting its own.
struct Turns {
    /// Runs that came to their end.
    done: AtomicU64,
    running: Mutex<()>,
}

/// How many runs had ended when a caller asked for one.
struct Ticket(u64);

impl Turns {
    const fn new() -> Self {
        Self { done: AtomicU64::new(0), running: Mutex::new(()) }
    }

    /// Taken before waiting for a turn.
    fn ticket(&self) -> Ticket {
        Ticket(self.done.load(Ordering::SeqCst))
    }

    /// Wait for a turn, then run `work`, unless a run ended since
    /// `ticket` was taken: that one's result is as fresh as the
    /// caller asked for. Returns whether `work` ran.
    fn run(&self, ticket: Ticket, work: impl FnOnce()) -> bool {
        let _turn = self.running.lock().unwrap_or_else(|e| e.into_inner());
        if self.done.load(Ordering::SeqCst) != ticket.0 {
            return false;
        }
        work();
        self.done.fetch_add(1, Ordering::SeqCst);
        true
    }
}

static LISTINGS: Turns = Turns::new();

/// List the connections and bring the Agent block at `path` up to
/// date. One at a time ([`Turns`]). Blocks for a few seconds. A
/// failure is kept for the pane ([`ListingView::error`]) and changes
/// nothing on disk.
///
/// The listing itself (`agent::list_all`) saves nothing and runs
/// without this module's lock. What it found is merged into the block
/// as it is on disk when the listing is over, read and saved under
/// the lock, so a change made while the listing ran is kept.
fn run_refresh(path: &Path) {
    let ticket = LISTINGS.ticket();
    LISTINGS.run(ticket, || {
        let cli = core_settings::current_refine_settings().claude_cli;
        let asked_with = {
            let _held = agent_lock();
            core_settings::load_agent_settings(path)
        };
        let outcome = agent::list_all(&asked_with, &cli, agent::LIST_WAIT);
        listing(|last| last.took(&outcome, openwhisper_core::history::now_ms()));
        match outcome {
            Ok(found) => {
                let _held = agent_lock();
                let current = core_settings::load_agent_settings(path);
                let merged = agent::merge_listing(&current, &found.connections, true);
                if merged != current {
                    if let Err(e) = core_settings::save_agent_settings(path, merged) {
                        eprintln!("[agent] the listing could not be saved: {e}");
                    }
                }
            }
            Err(e) => eprintln!("[agent] the connections could not be listed: {}", shown_error(&e.to_string())),
        }
    });
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

/// Boot-time: load the Agent block into core's cache, and when an ask
/// could use connections ([`lookup_refusal`]), look them up in the
/// background. Without that the first ask that wants a connection of
/// the person's Claude Code would be made without it.
///
/// Call it after `ask::init`, `refine::init` and `vision::init`: the
/// Ask switch and the backend decide whether anything is looked up,
/// and `vision::init` tells core where the settings file is.
pub fn init(app: &AppHandle) {
    let Ok(path) = settings_path(app) else {
        return;
    };
    let settings = {
        let _held = agent_lock();
        core_settings::load_agent_settings(&path)
    };
    if settings.enabled {
        if let Some(why) = lookup_refusal_now(&settings) {
            eprintln!("[agent] {why}");
        }
    }
    look_up_if_due(app, &settings);
}

/// The Agent block as it is on disk now (it is also edited by the
/// headless CLI), put into core's cache. When that is not what this
/// process had, the windows are told; and when Agent became able to
/// use connections, they are looked up in the background. Blocking:
/// a file read.
fn fresh(app: &AppHandle, path: &Path) -> AgentSettings {
    let (before, settings) = {
        let _held = agent_lock();
        (core_settings::current_agent_settings(), core_settings::load_agent_settings(path))
    };
    look_up_if_due(app, &settings);
    // The file was changed behind this process's back.
    if before != settings {
        announce(app, &settings);
    }
    settings
}

// --- commands ------------------------------------------------------

/// Fresh read from disk ([`fresh`]). Calls no tool. Off the UI thread
/// for the file read.
#[tauri::command]
pub async fn agent_get(app: AppHandle) -> Result<AgentState, String> {
    let path = settings_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || current_state(&fresh(&app, &path)))
        .await
        .map_err(|e| format!("agent settings: {e}"))
}

/// Lay one change over the block on disk ([`with_patch`]) and answer
/// with the new state. Emits `agent_changed`. When Agent became able
/// to use connections by this, they are looked up in the background.
/// Only from the main window.
///
/// Not an async command, like `ask_set` and `vision_set`: the pane
/// sends one of these per click, and they have to be applied in the
/// order they were made.
#[tauri::command]
pub fn agent_set(window: WebviewWindow, app: AppHandle, patch: AgentPatch) -> Result<AgentState, String> {
    check_caller("agent_set", window.label())?;
    change(&app, |stored| with_patch(stored, &patch))
}

/// Read the block, change it and save it, as one step.
fn change(
    app: &AppHandle,
    edit: impl FnOnce(AgentSettings) -> Result<AgentSettings, String>,
) -> Result<AgentState, String> {
    let path = settings_path(app)?;
    let saved = {
        let _held = agent_lock();
        let before = core_settings::load_agent_settings(&path);
        core_settings::save_agent_settings(&path, edit(before)?)?
    };
    look_up_if_due(app, &saved);
    Ok(announce(app, &saved))
}

/// Ask Claude Code which connections it has, with their tools, and
/// bring the block up to date. No tool is called and the model is
/// not asked anything, but every connection Claude Code has is
/// started. Takes a few seconds, on a blocking thread. Only from the
/// main window, and only while an ask could use what is found:
/// otherwise nothing is started and the error says why
/// ([`lookup_refusal`]). A listing that ran and failed answers with
/// the state, and says why in `listing.error`.
#[tauri::command]
pub async fn agent_refresh(window: WebviewWindow, app: AppHandle) -> Result<AgentState, String> {
    check_caller("agent_refresh", window.label())?;
    let path = settings_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let settings = {
            let _held = agent_lock();
            core_settings::load_agent_settings(&path)
        };
        if let Some(why) = lookup_refusal_now(&settings) {
            return Err(why.to_string());
        }
        // This is the lookup a change of the switches would ask for.
        *ASKED.lock().unwrap_or_else(|e| e.into_inner()) = true;
        refresh_and_announce(&app, Pending::new())
    })
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
        let settings = fresh(&app, &path);
        let ask_enabled = core_settings::current_ask_settings().enabled;
        let backend = core_settings::current_refine_settings().backend;
        let (ready, message) = agent::readiness(&settings, ask_enabled, backend);
        AgentStatus { ready, message, ask_enabled, backend: backend.as_str() }
    })
    .await
    .map_err(|e| format!("agent status: {e}"))
}

/// Remove a connection from the block, with its ticks. A connection
/// of the person's Claude Code comes back, switched off, with the
/// next refresh for as long as Claude Code has it. Only from the main
/// window.
#[tauri::command]
pub fn agent_remove_connection(window: WebviewWindow, app: AppHandle, name: String) -> Result<AgentState, String> {
    check_caller("agent_remove_connection", window.label())?;
    change(&app, |mut stored| {
        let Some(found) = stored.connection(&name).map(|c| c.name.clone()) else {
            return Err(format!("There is no connection called \"{}\".", ask::safe_to_show(name.trim(), false)));
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
/// with exactly the input the panel showed. Only from the answer
/// window, only while that confirm is the one the ask waits on, and
/// only once it has been on show for a moment (`ask::confirm_allow`);
/// returns whether the yes was taken.
#[tauri::command]
pub fn ask_confirm_allow(window: WebviewWindow, ask_id: u64, request_id: String) -> Result<bool, String> {
    check_caller("ask_confirm_allow", window.label())?;
    Ok(ask::confirm_allow(ask_id, &request_id))
}

/// No to the confirm `request_id` of the ask `ask_id`: the tool does
/// not run and the ask goes on to its answer (`ask::confirm_deny`).
/// Only from the answer window.
#[tauri::command]
pub fn ask_confirm_deny(window: WebviewWindow, ask_id: u64, request_id: String) -> Result<bool, String> {
    check_caller("ask_confirm_deny", window.label())?;
    Ok(ask::confirm_deny(ask_id, &request_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openwhisper_core::agent::{ConnectionSource, ListedConnection};
    use openwhisper_core::settings::{CustomServer, ToolInfo};
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
        state(settings, &LastListing::default(), false, |_| false)
    }

    fn patch(json: &str) -> AgentPatch {
        serde_json::from_str(json).expect("a patch")
    }

    #[test]
    fn the_commands_of_this_module_are_the_eight_listed_and_all_are_registered() {
        assert_eq!(command_names(shipped()), COMMANDS);
        let lib = include_str!("lib.rs");
        let start = lib.find("generate_handler![").expect("handler list");
        let list = &lib[start..start + lib[start..].find(']').expect("end of handler list")];
        for name in COMMANDS {
            assert!(list.contains(&format!("agent::{name},")), "{name} is not registered");
        }
    }

    #[test]
    fn no_command_adds_a_connection() {
        // A connection of the person's own is a command that is
        // started on this computer: it is added in a terminal only.
        let lib = include_str!("lib.rs");
        assert!(!lib.contains("agent_add_connection"));
        assert!(!command_names(shipped()).iter().any(|name| name.contains("add")));
        // Nothing that ships here can make one, either.
        let builds = shipped().lines().filter(|l| !l.trim_start().starts_with("//")).any(|l| l.contains("CustomServer"));
        assert!(!builds, "the shell builds a custom connection");
    }

    #[test]
    fn a_yes_or_no_is_taken_from_the_answer_window_and_a_change_from_the_main_window() {
        for command in ["ask_confirm_allow", "ask_confirm_deny"] {
            assert_eq!(check_caller(command, "answer"), Ok(()), "{command}");
            for other in ["main", "pill", "", "Answer", "answer2"] {
                let refused = check_caller(command, other).unwrap_err();
                assert!(refused.contains("not accepted from this window"), "{command} from {other:?}: {refused}");
            }
        }
        for command in ["agent_set", "agent_refresh", "agent_remove_connection"] {
            assert_eq!(check_caller(command, "main"), Ok(()), "{command}");
            for other in ["answer", "pill", "", "Main"] {
                assert!(check_caller(command, other).is_err(), "{command} from {other:?}");
            }
        }
        // Reading is open to every window.
        for command in ["agent_get", "agent_status", "agent_running_apps"] {
            for label in ["main", "pill", "answer"] {
                assert_eq!(check_caller(command, label), Ok(()), "{command} from {label}");
            }
        }
        // Every command has its rule here, and each one that is bound
        // to a window asks for it by its own name.
        for command in COMMANDS {
            let bound = window_of(command).is_some();
            let asks = shipped().contains(&format!("check_caller(\"{command}\", window.label())?;"));
            assert_eq!(bound, asks, "{command}");
        }
        assert_eq!(window_of("ask_confirm_allow"), Some(crate::ask::ANSWER_LABEL));
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
        assert_eq!(keys(&value["connections"][0]["tools"][0]), ["kind", "name", "problem", "title"]);
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
        assert_eq!(value["connections"][0]["tools"][0]["problem"], serde_json::Value::Null);
        assert_eq!(value["listing"]["refreshing"], false);
        assert_eq!(value["listing"]["refreshed_at_ms"], serde_json::Value::Null);
    }

    #[test]
    fn a_tool_that_cannot_be_used_says_why_in_cores_words() {
        let mut teamchat = chat("teamchat");
        teamchat.tools.push(ToolInfo::new("list,all"));
        teamchat.tools.push(ToolInfo::new("read.thread"));
        teamchat.tools.push(ToolInfo::new("read_thread_"));
        teamchat.tools.push(ToolInfo::new("read-thread"));
        let now = shown(&settings(vec![teamchat.clone()]));
        let problem = |name: &str| now.connections[0].tools.iter().find(|t| t.name == name).unwrap().problem;
        assert_eq!(problem("send_message"), None);
        assert_eq!(problem("list,all"), Some(policy::ToolProblem::Name.sentence()));
        // `read.thread` and `read-thread` are spelled like `read_thread`
        // by the time Claude Code has them: none of the three can be
        // told from the others.
        assert_eq!(problem("read.thread"), Some(policy::ToolProblem::Twin.sentence()));
        assert_eq!(problem("read_thread"), Some(policy::ToolProblem::Twin.sentence()));
        assert_eq!(problem("read_thread_"), None);
        // Exactly what core says, tool by tool.
        for (shown, tool) in now.connections[0].tools.iter().zip(&teamchat.tools) {
            assert_eq!(shown.problem, policy::unusable_reason(&teamchat, tool).map(|p| p.sentence()), "{}", tool.name);
        }
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
    fn a_patch_changes_what_it_names_and_nothing_else() {
        let mut teamchat = chat("teamchat");
        teamchat.enabled = true;
        teamchat.allowed_tools = vec!["read_thread".into()];
        let stored = AgentSettings {
            enabled: true,
            drafts_only: false,
            confirm_timeout_s: 60,
            app_connections: Some(vec![AppConnection::new("com.example.chat", "Chat", "teamchat")]),
            ..settings(vec![teamchat, own("notes")])
        };
        // Nothing named: nothing changed. Above all, drafts only is
        // not switched back on or off by being left out.
        assert_eq!(with_patch(stored.clone(), &patch("{}")).unwrap(), stored);

        // One switch.
        let off = with_patch(stored.clone(), &patch(r#"{"enabled":false}"#)).unwrap();
        assert_eq!(off, AgentSettings { enabled: false, ..stored.clone() });
        let careful = with_patch(stored.clone(), &patch(r#"{"drafts_only":true}"#)).unwrap();
        assert_eq!(careful, AgentSettings { drafts_only: true, ..stored.clone() });

        // One connection's switch: its ticks and the other connection
        // stay, details and all.
        let switched =
            with_patch(stored.clone(), &patch(r#"{"connection":{"name":" TeamChat ","enabled":false}}"#)).unwrap();
        assert!(!switched.connections[0].enabled);
        assert_eq!(switched.connections[0].allowed_tools, ["read_thread"]);
        assert_eq!(switched.connections[0].tools, stored.connections[0].tools);
        assert_eq!(switched.connections[1], stored.connections[1]);
        assert_eq!((switched.enabled, switched.drafts_only, switched.confirm_timeout_s), (true, false, 60));
        assert_eq!(switched.app_connections, stored.app_connections);

        // Ticks of one connection: the ones named, the rest as they
        // are.
        let ticked = with_patch(
            stored.clone(),
            &patch(r#"{"connection":{"name":"teamchat","tick":["create_draft","read_thread"]}}"#),
        )
        .unwrap();
        assert_eq!(ticked.connections[0].allowed_tools, ["read_thread", "create_draft"]);
        assert!(ticked.connections[0].enabled);
        let unticked =
            with_patch(ticked, &patch(r#"{"connection":{"name":"teamchat","untick":["read_thread","never_ticked"]}}"#))
                .unwrap();
        assert_eq!(unticked.connections[0].allowed_tools, ["create_draft"]);

        // The app table: a new one, or back to the built-in one.
        let table = with_patch(
            stored.clone(),
            &patch(r#"{"app_connections":[{"app_id":"com.example.mail","app_name":"Mail","connection":"notes"}]}"#),
        )
        .unwrap();
        assert_eq!(table.app_connections, Some(vec![AppConnection::new("com.example.mail", "Mail", "notes")]));
        assert_eq!(table.connections, stored.connections);
        let builtin = with_patch(stored.clone(), &patch(r#"{"app_connections":null}"#)).unwrap();
        assert_eq!(builtin.app_connections, None);
        assert_eq!(AgentSettings { app_connections: stored.app_connections.clone(), ..builtin }, stored);
    }

    #[test]
    fn a_patch_can_only_switch_drafts_only_off_by_saying_so() {
        // Left out is "not changed", whatever it is now.
        assert_eq!(patch("{}").drafts_only, None);
        assert_eq!(patch(r#"{"enabled":true}"#).drafts_only, None);
        for stored in [AgentSettings::default(), AgentSettings { drafts_only: false, ..AgentSettings::default() }] {
            let after = with_patch(stored.clone(), &patch(r#"{"enabled":true}"#)).unwrap();
            assert_eq!(after.drafts_only, stored.drafts_only);
        }
        assert!(!with_patch(AgentSettings::default(), &patch(r#"{"drafts_only":false}"#)).unwrap().drafts_only);
        // Something that is not a yes or no is not taken for one.
        for bad in [r#"{"drafts_only":null}"#, r#"{"drafts_only":"no"}"#, r#"{"drafts_only":0}"#] {
            match serde_json::from_str::<AgentPatch>(bad) {
                Ok(read) => assert_eq!(read.drafts_only, None, "{bad}"),
                Err(_) => {}
            }
        }
    }

    #[test]
    fn a_patch_cannot_add_a_connection_or_say_what_one_is() {
        let stored = settings(vec![chat("teamchat")]);
        // A connection that is not on disk.
        let refused = with_patch(stored.clone(), &patch(r#"{"connection":{"name":"sneaked","enabled":true}}"#));
        assert_eq!(refused.unwrap_err(), "There is no connection called \"sneaked\".");
        // The whole list, the old way: not read as anything.
        assert!(serde_json::from_str::<AgentPatch>(r#"{"connections":[{"name":"teamchat","enabled":true}]}"#).is_err());
        // What a connection is, how it is reached, its tools.
        for field in [r#""kind":"custom""#, r#""custom":{"command":"sh"}"#, r#""tools":[{"name":"run"}]"#, r#""allowed_tools":["send_message"]"#] {
            let sent = format!(r#"{{"connection":{{"name":"teamchat",{field}}}}}"#);
            assert!(serde_json::from_str::<AgentPatch>(&sent).is_err(), "{sent}");
        }
        assert!(serde_json::from_str::<AgentPatch>(r#"{"confirm_timeout_s":600}"#).is_err());
    }

    #[test]
    fn a_tool_that_acts_cannot_be_ticked_under_drafts_only_and_an_unusable_one_never() {
        let mut teamchat = chat("teamchat");
        teamchat.enabled = true;
        teamchat.tools.push(ToolInfo::new("list,all"));
        teamchat.tools.push(ToolInfo::new("post.note"));
        teamchat.tools.push(ToolInfo::new("post_note"));

        // Drafts only on: read and draft tools, not one that acts.
        assert_eq!(tick_refusal(&teamchat, true, "read_thread"), None);
        assert_eq!(tick_refusal(&teamchat, true, "create_draft"), None);
        let acts = tick_refusal(&teamchat, true, "send_message").expect("refused");
        assert!(acts.contains("cannot be ticked while Drafts only is on"), "{acts}");
        // Drafts only off: it can.
        assert_eq!(tick_refusal(&teamchat, false, "send_message"), None);
        // Unusable: never, with core's reason.
        for drafts_only in [true, false] {
            let name = tick_refusal(&teamchat, drafts_only, "list,all").expect("refused");
            assert!(name.ends_with(policy::ToolProblem::Name.sentence()), "{name}");
            let twin = tick_refusal(&teamchat, drafts_only, "post.note").expect("refused");
            assert!(twin.ends_with(policy::ToolProblem::Twin.sentence()), "{twin}");
            // A tool nobody has seen.
            let unseen = tick_refusal(&teamchat, drafts_only, "wipe_everything").expect("refused");
            assert!(unseen.contains("is not a tool this connection is known to have"), "{unseen}");
        }
        // A name is shown safely in the refusal.
        let hidden = tick_refusal(&teamchat, false, "wipe\u{202E}everything").expect("refused");
        assert!(hidden.contains("wipe\\u{202E}everything") && !hidden.contains('\u{202E}'), "{hidden}");

        // The same through a patch, and a refusal changes nothing.
        let tick = |stored: &AgentSettings, tool: &str| {
            with_patch(stored.clone(), &patch(&format!(r#"{{"connection":{{"name":"teamchat","tick":["read_thread","{tool}"]}}}}"#)))
        };
        let careful = settings(vec![teamchat.clone()]);
        assert!(tick(&careful, "send_message").unwrap_err().contains("Drafts only"));
        assert!(tick(&careful, "list,all").is_err());
        assert_eq!(tick(&careful, "create_draft").unwrap().connections[0].allowed_tools, ["read_thread", "create_draft"]);
        let open = AgentSettings { drafts_only: false, ..careful.clone() };
        assert_eq!(tick(&open, "send_message").unwrap().connections[0].allowed_tools, ["read_thread", "send_message"]);
        assert!(tick(&open, "list,all").is_err());
        // Drafts only as it is after the patch decides: switched off
        // in the same patch, the tick is taken; switched on, it is
        // not.
        let both = with_patch(
            careful.clone(),
            &patch(r#"{"drafts_only":false,"connection":{"name":"teamchat","tick":["send_message"]}}"#),
        )
        .unwrap();
        assert_eq!(both.connections[0].allowed_tools, ["send_message"]);
        assert!(with_patch(
            open.clone(),
            &patch(r#"{"drafts_only":true,"connection":{"name":"teamchat","tick":["send_message"]}}"#)
        )
        .is_err());

        // A tool that acts and was ticked before drafts only went on
        // stays ticked without being asked about again, and can be
        // unticked. Once unticked it cannot be ticked back.
        let mut before = teamchat.clone();
        before.allowed_tools = vec!["send_message".into()];
        let kept = settings(vec![before]);
        assert_eq!(tick(&kept, "send_message").unwrap().connections[0].allowed_tools, ["send_message", "read_thread"]);
        let unticked =
            with_patch(kept, &patch(r#"{"connection":{"name":"teamchat","untick":["send_message"]}}"#)).unwrap();
        assert!(unticked.connections[0].allowed_tools.is_empty());
        assert!(tick(&unticked, "send_message").is_err());
    }

    #[test]
    fn a_connection_says_where_it_stood_in_the_last_listing() {
        let mut on = own("mine");
        on.enabled = true;
        let s = settings(vec![chat("teamchat"), chat("tracker"), own("notes"), on]);
        let words = |last: &LastListing| -> Vec<String> {
            state(&s, last, false, |name| name == "teamchat").connections.iter().map(|c| c.state.clone()).collect()
        };
        // Before the first listing of this run.
        assert_eq!(words(&LastListing::default()), ["not-looked-up"; 4]);
        let mut last = LastListing::default();
        last.view.refreshed_at_ms = Some(1);
        last.found.insert(("teamchat".into(), false), ConnectionState::NeedsAuth);
        // A connection of Claude Code the listing did not find is
        // gone. An entry of the person's own that is switched off is
        // never in a listing: it was not looked up. One that is on
        // and not there was switched on after the listing.
        assert_eq!(words(&last), ["needs-auth", "missing", "not-looked-up", "not-looked-up"]);
        // The person's own entries could not be listed: the one that
        // was tried is not known, the one that is off was not tried.
        last.view.custom_error = "Claude Code did not answer in time".into();
        assert_eq!(words(&last), ["needs-auth", "missing", "not-looked-up", "unknown"]);
        last.view.custom_error = String::new();
        last.found.insert(("mine".into(), true), ConnectionState::Connected);
        // The same name as an entry of the other kind is not it.
        last.found.insert(("tracker".into(), true), ConnectionState::Connected);
        assert_eq!(words(&last), ["needs-auth", "missing", "not-looked-up", "connected"]);
        let now = state(&s, &last, true, |name| name == "teamchat");
        assert_eq!(now.connections.iter().map(|c| c.usable).collect::<Vec<_>>(), [true, false, true, true]);
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
    fn the_connections_are_only_looked_up_while_an_ask_could_use_them() {
        use RefineBackendKind::{ClaudeCli, OpenaiCompatible};
        let off = AgentSettings::default();
        let on = AgentSettings { enabled: true, ..AgentSettings::default() };
        assert_eq!(lookup_refusal(&on, true, ClaudeCli), None);
        // Each thing that is missing is named, Agent's own switch
        // first.
        let agent_off = lookup_refusal(&off, true, ClaudeCli).expect("refused");
        assert!(agent_off.starts_with("Agent is switched off"), "{agent_off}");
        assert!(lookup_refusal(&off, false, OpenaiCompatible).unwrap().starts_with("Agent is switched off"));
        let ask_off = lookup_refusal(&on, false, ClaudeCli).expect("refused");
        assert!(ask_off.starts_with("Ask is switched off"), "{ask_off}");
        let other = lookup_refusal(&on, true, OpenaiCompatible).expect("refused");
        assert!(other.starts_with("Agent needs the Claude backend"), "{other}");
        for why in [agent_off, ask_off, other] {
            assert!(why.contains("the connections were not looked up"), "{why}");
        }
        // No enabled connection and no tick is needed: the lookup is
        // what brings the connections to choose from.
        assert!(on.connections.is_empty());
    }

    #[test]
    fn a_lookup_is_due_once_each_time_agent_becomes_able_whoever_notices() {
        let mut asked = false;
        // Not able: nothing, however often it is looked at.
        assert!(!lookup_due(&mut asked, false));
        assert!(!lookup_due(&mut asked, false));
        // Able: the first one to notice starts the lookup, whichever
        // command that is. Everyone after it finds it asked for.
        assert!(lookup_due(&mut asked, true));
        assert!(!lookup_due(&mut asked, true));
        assert!(!lookup_due(&mut asked, true));
        // Off and on again: due again, once.
        assert!(!lookup_due(&mut asked, false));
        assert!(lookup_due(&mut asked, true));
        assert!(!lookup_due(&mut asked, true));
        // A lookup the person asked for themselves counts: nothing
        // more is due after it.
        let mut asked = true;
        assert!(!lookup_due(&mut asked, true));

        // The order two readers come in does not matter: Agent was
        // switched on in a terminal, and the status is read before
        // the settings (as the pane does) or after.
        for status_first in [true, false] {
            let mut asked = false;
            let mut started = 0;
            let mut read = |_who: &str| {
                if lookup_due(&mut asked, true) {
                    started += 1;
                }
            };
            if status_first {
                read("agent_status");
                read("agent_get");
            } else {
                read("agent_get");
                read("agent_status");
            }
            assert_eq!(started, 1, "status first: {status_first}");
        }
        // And both readers go through the one place that asks.
        let fresh_reads = shipped().matches("fresh(&app, &path)").count();
        assert_eq!(fresh_reads, 2, "agent_get and agent_status");
        assert!(!shipped().contains("fn refresh_due"));
    }

    #[test]
    fn listings_run_one_at_a_time_and_a_caller_that_waited_takes_the_result() {
        let turns = Turns::new();
        let mut ran = Vec::new();
        // Two callers ask while nothing has ended yet. The first runs.
        let first = turns.ticket();
        let second = turns.ticket();
        assert!(turns.run(first, || ran.push("first")));
        // The second waited for the first: a run ended since it
        // asked, so it takes that result and starts nothing.
        assert!(!turns.run(second, || ran.push("second")));
        // A caller that asks after that gets a run of its own.
        let third = turns.ticket();
        assert!(turns.run(third, || ran.push("third")));
        assert_eq!(ran, ["first", "third"]);

        // Really one at a time: the second caller cannot start while
        // the first is inside.
        let turns = std::sync::Arc::new(Turns::new());
        let (inside, entered) = std::sync::mpsc::channel::<()>();
        let (release, held) = std::sync::mpsc::channel::<()>();
        let (finished, second_done) = std::sync::mpsc::channel::<bool>();
        let a = {
            let turns = turns.clone();
            thread::spawn(move || {
                let ticket = turns.ticket();
                turns.run(ticket, || {
                    inside.send(()).unwrap();
                    held.recv().unwrap();
                })
            })
        };
        entered.recv().unwrap();
        // Asked while the first is running.
        let ticket = turns.ticket();
        let b = {
            let turns = turns.clone();
            thread::spawn(move || finished.send(turns.run(ticket, || unreachable!("a second listing"))).unwrap())
        };
        assert!(second_done.try_recv().is_err(), "the second caller did not wait");
        release.send(()).unwrap();
        assert!(a.join().unwrap());
        assert!(!second_done.recv().unwrap());
        b.join().unwrap();
    }

    fn listed(name: &str, source: ConnectionSource, state: ConnectionState) -> ListedConnection {
        ListedConnection { name: name.into(), source, state, transport: String::new(), tools: Vec::new() }
    }

    #[test]
    fn what_a_listing_came_to_is_kept_for_the_pane() {
        let mut last = LastListing::default();
        // One that failed before any worked: why, and nothing else.
        last.took(&Err(AgentError::Unavailable("Claude Code is not installed.".into())), 5);
        assert_eq!(last.view.error, "Claude Code is not installed.");
        assert_eq!(last.view.refreshed_at_ms, None);
        assert!(last.found.is_empty());
        // One that worked: what was found, when, and no error left.
        let found = agent::Listing {
            connections: vec![
                listed("TeamChat", ConnectionSource::User, ConnectionState::Connected),
                listed("notes", ConnectionSource::Custom, ConnectionState::Failed),
            ],
            custom_error: String::new(),
            elapsed_ms: 1200,
        };
        last.took(&Ok(found), 10);
        assert_eq!(last.view, ListingView { refreshing: false, refreshed_at_ms: Some(10), error: String::new(), custom_error: String::new() });
        assert_eq!(last.found.get(&("teamchat".into(), false)), Some(&ConnectionState::Connected));
        assert_eq!(last.found.get(&("notes".into(), true)), Some(&ConnectionState::Failed));
        // A later one that fails says why and keeps what was found.
        last.took(&Err(AgentError::Timeout), 20);
        assert_eq!(last.view.error, "Claude Code did not answer in time");
        assert_eq!(last.view.refreshed_at_ms, Some(10));
        assert_eq!(last.found.len(), 2);
        // The person's own entries could not be listed: the listing
        // still counts.
        let partly = agent::Listing {
            connections: vec![listed("teamchat", ConnectionSource::Account, ConnectionState::NeedsAuth)],
            custom_error: "notes: no such file".into(),
            elapsed_ms: 900,
        };
        last.took(&Ok(partly), 30);
        assert_eq!((last.view.error.as_str(), last.view.custom_error.as_str()), ("", "notes: no such file"));
        assert_eq!(last.view.refreshed_at_ms, Some(30));
        assert_eq!(last.found.len(), 1);
    }

    #[test]
    fn what_the_windows_are_told_after_a_listing_says_that_it_is_over() {
        // `refresh_and_announce` gives its `Pending` up before it
        // tells the windows; the state they are told is built from
        // the count at that moment.
        let source = shipped();
        let body = &source[source.find("fn refresh_and_announce").expect("the function")..];
        let body = &body[..body.find("\n}\n").expect("its end")];
        let (dropped, told) = (body.find("drop(pending);").expect("gives it up"), body.find("Ok(announce(app").expect("tells"));
        assert!(body.find("run_refresh(&path);").expect("lists") < dropped && dropped < told);
        // And the state follows the count.
        let s = settings(vec![chat("teamchat")]);
        assert!(state(&s, &LastListing::default(), true, |_| false).listing.refreshing);
        assert!(!state(&s, &LastListing::default(), false, |_| false).listing.refreshing);
    }

    #[test]
    fn why_a_listing_failed_is_made_safe_to_show_and_capped() {
        // A line the Claude CLI wrote: an escape sequence, a
        // right-to-left override, more than one line.
        let raw = "error: \u{1B}[2Jcould not start \u{202E}txt.exe\nat line 2";
        let safe = shown_error(raw);
        assert_eq!(safe, "error: \\u{1B}[2Jcould not start \\u{202E}txt.exe at line 2");
        // Exactly core's escaping, nothing of this module's own.
        assert_eq!(safe, ask::safe_to_show(&raw.replace('\n', " "), false));
        assert_eq!(shown_error("  plain words  "), "plain words");
        // Long: cut, with an ellipsis.
        let long = shown_error(&"x".repeat(5000));
        assert_eq!(long.chars().count(), ERROR_MAX_CHARS);
        assert!(long.ends_with('\u{2026}'));
        assert_eq!(shown_error(&"x".repeat(ERROR_MAX_CHARS)).chars().count(), ERROR_MAX_CHARS);
        // Through a failed listing: this is what reaches a window.
        let mut last = LastListing::default();
        last.took(&Err(AgentError::Failed(raw.into())), 1);
        assert_eq!(last.view.error, safe);
        let found = agent::Listing { connections: Vec::new(), custom_error: raw.into(), elapsed_ms: 1 };
        last.took(&Ok(found), 2);
        assert_eq!(last.view.custom_error, safe);
    }

    #[test]
    fn a_listing_counts_as_running_from_the_moment_it_is_asked_for() {
        // Other tests of this module start no listing, so the count
        // only moves here.
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
        let (ready, message) = agent::readiness(&AgentSettings::default(), true, RefineBackendKind::ClaudeCli);
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
