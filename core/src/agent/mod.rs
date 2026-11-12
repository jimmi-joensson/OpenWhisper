//! Agent: an ask that can use the person's own tool connections
//! (TASK-99).
//!
//! ```text
//! ask --> is it an agent ask? --no--> the ask as it always was (no tools)
//!              | yes
//!              v
//!   which tools ([`policy`])        read, draft: run       act: confirm in the panel
//!   which connections ([`connections`])   a private file with only the enabled ones
//!   where to look ([`AgentRun::route`])   the app in front has a connection: no screenshot
//!              |
//!              v
//!   the Claude CLI with those connections and nothing else ([`claude`])
//!       every tool call is a step on the snapshot; a confirm waits for the person
//! ```
//!
//! A plain ask has no tools and cannot change anything
//! ([`crate::ask`]). This is the other path, with its own switch
//! ([`crate::settings::AgentSettings`], off by default), and it is
//! built so that the model alone can never do harm:
//!
//! - **Only what was enabled exists.** The CLI is started with a
//!   strict connection file that holds the enabled connections and
//!   nothing else, with no built-in tools (no shell, no files, no
//!   web fetch), without the person's Claude Code settings, hooks or
//!   plugins, and with the permission mode pinned
//!   ([`claude::spawn_args`]).
//! - **Only ticked tools run.** Per connection the person ticks the
//!   tools an ask may use. Everything else the connection has is
//!   taken out of what the model sees.
//! - **Reading and drafting run; acting asks.** Every tool is a
//!   read, a draft or an act ([`policy::classify`], on the careful
//!   side). A ticked act tool raises a confirm in the panel every
//!   time, with its whole input, and runs only on a yes
//!   ([`crate::ask::AskConfirm`]). With drafts only, which is the
//!   default, act tools do not exist for the ask at all.
//! - **What comes back is data.** A tool result, a screenshot and
//!   anything read through a connection can hold text that tries to
//!   steer the model. The prompt says not to follow it
//!   ([`prompt::section`]), but the protection is the three rules
//!   above: whatever the model is talked into, it has no tool that
//!   acts without the person.
//! - **Every call is shown.** Each tool call is a step on the ask's
//!   snapshot as it happens ([`crate::ask::AskStep`]), and the
//!   history entry lists the tools an ask used, also when the ask
//!   did not finish.
//! - **Names and text are not ours.** A tool's name comes from its
//!   connection, and so does everything a tool returns. A name that
//!   cannot be handed to the CLI safely makes its tool unusable
//!   ([`policy::unusable_reason`]); the names that are handed over
//!   are spelled the CLI's way ([`policy::normalize_name`]) and held
//!   against what the CLI really started with before the model can
//!   call anything ([`claude`]); and whatever is shown of a call is
//!   made safe to show first ([`crate::ask::safe_to_show`]).
//! - **A yes is for what was shown.** A confirm carries the whole
//!   input or is not raised at all, and a yes counts only once the
//!   confirm has been on show for a moment
//!   ([`crate::ask::CONFIRM_ARM_MS`]).
//!
//! **The connection before the screen.** When the app in front has a
//! connection the ask can use ([`AgentRun::route`], from the app
//! table in the settings), the ask is pointed at that connection and
//! takes no screenshot: the content is read through the app's own
//! connection instead of off the screen. A screenshot is still taken
//! when the person asks for the screen in so many words.
//!
//! Only the Claude backend can do this. With an OpenAI-compatible
//! endpoint, or with nothing enabled and ticked, an ask runs exactly
//! as it does without this module.

use std::fmt;

use crate::settings::{AgentConnection, AgentConnectionKind, AgentSettings, RefineBackendKind};

pub mod claude;
pub mod connections;
pub mod eval;
pub mod fixture;
pub mod policy;
pub mod prompt;

pub use connections::{
    ConnectionSource, ConnectionState, LIST_WAIT, ListedConnection, Listing, Refreshed, RunConfig, list_all,
    list_claude_connections, list_custom_connections, merge_listing, refresh, write_run_config,
};
pub use policy::{PlannedTool, ToolKind, ToolPlan, ToolProblem, classify, plan_tools, unusable_reason};

/// Remove what an earlier process of this app left in the temp
/// folder when it was killed before it could clean up: the
/// connection file of an agent run (it can hold tokens) and the
/// prompt file of an ask, and nothing else. See
/// [`crate::refine::claude_cli::sweep_stale_private_files`].
pub use crate::refine::claude_cli::sweep_stale_private_files as sweep_stale_files;

/// Why something about the connections did not work.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentError {
    /// Claude Code cannot run at all: not installed, not signed in.
    /// The message says what to do.
    Unavailable(String),
    /// Claude Code did not answer in time.
    Timeout,
    /// It ran and failed.
    Failed(String),
    /// A run needs a connection of the person's Claude Code that has
    /// not been listed since this process started. The message says
    /// to refresh the connections.
    NotListed(String),
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(m) | Self::Failed(m) | Self::NotListed(m) => f.write_str(m),
            Self::Timeout => f.write_str("Claude Code did not answer in time"),
        }
    }
}

impl std::error::Error for AgentError {}

/// An ask that cannot get its connections cannot run: the person is
/// told why, in the place they are told that the backend is missing.
impl From<AgentError> for crate::ask::AskError {
    fn from(e: AgentError) -> Self {
        match e {
            AgentError::Unavailable(m) | AgentError::NotListed(m) => Self::Unavailable(m),
            AgentError::Timeout => Self::Timeout,
            AgentError::Failed(m) => Self::Backend(m),
        }
    }
}

/// The app the person is working in when they ask: its id as
/// [`crate::focus`] reports it, and its name.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FrontApp {
    pub id: String,
    pub name: String,
}

impl FrontApp {
    pub fn new(id: &str, name: &str) -> Self {
        Self { id: id.trim().to_string(), name: name.trim().to_string() }
    }
}

/// The app in front has a connection this ask can use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    pub app_id: String,
    /// Name to show a person: the app's own name, or the one in the
    /// app table.
    pub app_name: String,
    /// Name of the connection.
    pub connection: String,
}

/// Everything the tools of one ask are made of. Fixed for the ask:
/// worked out from the settings as they are on disk when it starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRun {
    /// What happens when the model calls which tool.
    pub tools: ToolPlan,
    /// The tools the model can call, with what is known about each.
    pub planned: Vec<PlannedTool>,
    /// The connections the run is started with: the enabled ones
    /// that have at least one tool the model can call.
    pub connections: Vec<AgentConnection>,
    pub drafts_only: bool,
    pub max_turns: u32,
    pub confirm_timeout_s: u64,
    /// Set when the app in front has one of [`AgentRun::connections`].
    pub route: Option<Route>,
}

impl AgentRun {
    /// The run these settings give, with `front` as the app the
    /// person is working in. `None` when the model would have no
    /// tool at all: nothing enabled, or nothing ticked that can run
    /// (with drafts only, a connection with only act tools ticked
    /// counts for nothing).
    ///
    /// Does not look at [`AgentSettings::enabled`] or at the backend:
    /// [`AgentRun::for_ask`] does.
    pub fn new(settings: &AgentSettings, front: Option<&FrontApp>) -> Option<Self> {
        let mut settings = settings.clone().cleaned();
        // A connection without a tool the model can call brings
        // nothing but its start-up time: leave it out of the run.
        let usable: Vec<String> = policy::planned_tools(&settings).into_iter().map(|t| t.connection).collect();
        settings.connections.retain(|c| c.enabled && usable.contains(&c.name));
        let tools = plan_tools(&settings);
        if !tools.has_tools() {
            return None;
        }
        let route = front.and_then(|front| {
            let (entry, connection) = settings.connection_for_app(&front.id)?;
            Some(Route {
                app_id: entry.app_id.clone(),
                app_name: if front.name.is_empty() { entry.app_name.clone() } else { front.name.clone() },
                connection: connection.name.clone(),
            })
        });
        Some(Self {
            planned: policy::planned_tools(&settings),
            connections: policy::enabled_connections(&settings).into_iter().cloned().collect(),
            tools,
            drafts_only: settings.drafts_only,
            max_turns: settings.max_turns,
            confirm_timeout_s: settings.confirm_timeout_s,
            route,
        })
    }

    /// [`AgentRun::new`] for an ask in the app: only while Agent is
    /// switched on and the backend is the Claude CLI. `None` means
    /// the ask runs as a plain ask.
    ///
    /// A connection of the person's Claude Code that this process
    /// has not looked up is left out
    /// ([`AgentRun::for_ask_with_notice`], which also says so).
    pub fn for_ask(settings: &AgentSettings, backend: RefineBackendKind, front: Option<&FrontApp>) -> Option<Self> {
        Self::for_ask_with_notice(settings, backend, front).0
    }

    /// [`AgentRun::for_ask`], and what to tell the person about it.
    ///
    /// A connection of the person's Claude Code can only be started
    /// once this process has looked it up (how it is reached is kept
    /// in memory, never on disk: [`connections`]). One that is
    /// enabled and has a tool the ask could call, but has not been
    /// looked up since the app started, does not stop the ask: it is
    /// left out, the ask runs with the connections that remain, or
    /// as a plain ask when none does, and the notice (the second
    /// value, empty when there is nothing to say) names it and says
    /// to refresh. A question that needs no connection at all is
    /// answered either way.
    pub fn for_ask_with_notice(
        settings: &AgentSettings,
        backend: RefineBackendKind,
        front: Option<&FrontApp>,
    ) -> (Option<Self>, String) {
        if !settings.enabled || backend == RefineBackendKind::OpenaiCompatible {
            return (None, String::new());
        }
        let mut settings = settings.clone().cleaned();
        // The connections that would bring a tool, and cannot be
        // started.
        let wanted: Vec<String> = policy::planned_tools(&settings).into_iter().map(|t| t.connection).collect();
        let unknown = |c: &AgentConnection| {
            c.enabled && c.kind == AgentConnectionKind::Claude && wanted.contains(&c.name) && !connections::is_listed(&c.name)
        };
        let left_out: Vec<String> = settings.connections.iter().filter(|c| unknown(c)).map(|c| c.name.clone()).collect();
        settings.connections.retain(|c| !unknown(c));
        let shown = |name: &String| format!("\"{}\"", crate::ask::safe_to_show(name, false));
        let notice = match left_out.as_slice() {
            [] => String::new(),
            [one] => format!(
                "The connection {} has not been looked up since the app started, so this answer was made without it. \
Refresh the connections in the Agent settings to use it.",
                shown(one)
            ),
            many => format!(
                "{} connections have not been looked up since the app started ({}), so this answer was made without them. \
Refresh the connections in the Agent settings to use them.",
                many.len(),
                many.iter().map(shown).collect::<Vec<_>>().join(", ")
            ),
        };
        (Self::new(&settings, front), notice)
    }

    /// The planned tool with this full name (`mcp__connection__tool`).
    pub fn tool(&self, full_name: &str) -> Option<&PlannedTool> {
        self.planned.iter().find(|t| t.full_name == full_name)
    }
}

/// Why an ask with these settings is or is not an agent ask, in one
/// plain sentence: for the Agent pane and `openwhisper agent status`.
pub fn readiness(settings: &AgentSettings, ask_enabled: bool, backend: RefineBackendKind) -> (bool, String) {
    let settings = settings.clone().cleaned();
    if !settings.enabled {
        return (false, "Agent is switched off.".into());
    }
    if !ask_enabled {
        return (false, "Agent only works in an ask, and Ask is switched off.".into());
    }
    if backend == RefineBackendKind::OpenaiCompatible {
        return (false, "Agent needs the Claude backend. An OpenAI-compatible endpoint cannot use connections.".into());
    }
    if !settings.connections.iter().any(|c| c.enabled) {
        return (false, "No connection is enabled yet.".into());
    }
    match AgentRun::new(&settings, None) {
        Some(run) => {
            let confirm = run.tools.confirm.len();
            let mut text = format!(
                "An ask can use {} tool{} on {} connection{}.",
                run.planned.len(),
                if run.planned.len() == 1 { "" } else { "s" },
                run.connections.len(),
                if run.connections.len() == 1 { "" } else { "s" }
            );
            if settings.drafts_only {
                text.push_str(" Drafts only: nothing that sends, posts, deletes or changes anything can run.");
            } else if confirm > 0 {
                text.push_str(&format!(" {confirm} of them ask you first, every time."));
            }
            (true, text)
        }
        None if settings.drafts_only => (
            false,
            "No tool that can run is ticked. With drafts only, tools that send, post, delete or change something do not count."
                .into(),
        ),
        None => (false, "No tool is ticked on an enabled connection.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{AppConnection, ToolInfo};

    /// One of the person's own entries: it needs no listing to be
    /// started.
    fn chat(name: &str, enabled: bool, ticked: &[&str]) -> AgentConnection {
        AgentConnection {
            name: name.into(),
            kind: AgentConnectionKind::Custom,
            enabled,
            allowed_tools: ticked.iter().map(|t| t.to_string()).collect(),
            tools: vec![
                ToolInfo { read_only: Some(true), ..ToolInfo::new("read_thread") },
                ToolInfo::new("create_draft"),
                ToolInfo::new("send_message"),
            ],
            custom: Some(crate::settings::CustomServer { command: "/opt/example/chat-server".into(), ..Default::default() }),
        }
    }

    /// A connection of the person's Claude Code with the same tools.
    fn of_claude(name: &str, ticked: &[&str]) -> AgentConnection {
        AgentConnection { kind: AgentConnectionKind::Claude, custom: None, ..chat(name, true, ticked) }
    }

    #[cfg(unix)]
    #[test]
    fn a_connection_that_was_not_looked_up_is_left_out_of_an_ask_and_named_in_a_notice() {
        let _lock = connections::DETAILS_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        connections::forget_listing();
        let claude = RefineBackendKind::ClaudeCli;
        let s = settings(vec![
            chat("chat", true, &["read_thread"]),
            of_claude("claude.ai Docs", &["read_thread"]),
            // Enabled, but nothing of it could run: not missed.
            of_claude("claude.ai Idle", &[]),
            of_claude("claude.ai Sender", &["send_message"]),
        ]);
        // Nothing was looked up since the app started: the ask still
        // runs, with the connection that can be started, and says
        // which one it went without.
        let (run, notice) = AgentRun::for_ask_with_notice(&s, claude, None);
        let run = run.expect("an ask with the remaining connection");
        assert_eq!(run.connections.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["chat"]);
        assert_eq!(run.tools.allowed, ["mcp__chat__read_thread"]);
        assert!(!run.tools.disallowed.iter().any(|t| t.contains("claude_ai")), "{:?}", run.tools.disallowed);
        assert_eq!(
            notice,
            "The connection \"claude.ai Docs\" has not been looked up since the app started, so this answer was made without it. \
Refresh the connections in the Agent settings to use it."
        );
        assert_eq!(AgentRun::for_ask(&s, claude, None), Some(run));
        // The run can be started: nothing in it needs a listing.
        assert!(connections::write_run_config(&AgentRun::for_ask(&s, claude, None).unwrap().connections).is_ok());

        // Only connections that were not looked up: a plain ask,
        // which says why it has no tools. Two are named.
        let only = AgentSettings {
            drafts_only: false,
            ..settings(vec![of_claude("claude.ai Docs", &["read_thread"]), of_claude("claude.ai Sender", &["send_message"])])
        };
        let (run, notice) = AgentRun::for_ask_with_notice(&only, claude, None);
        assert_eq!(run, None);
        assert!(notice.starts_with("2 connections have not been looked up since the app started (\"claude.ai Docs\", \"claude.ai Sender\")"), "{notice}");

        // Looked up (in any spelling of its name): it is in the run
        // and there is nothing to say.
        connections::remember_for_test("Claude.ai DOCS", serde_json::json!({ "type": "http", "url": "https://example.com/docs" }));
        let (run, notice) = AgentRun::for_ask_with_notice(&s, claude, None);
        assert_eq!(run.unwrap().connections.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["chat", "claude.ai Docs"]);
        assert_eq!(notice, "");
        // Agent off, or another backend: no run and nothing to say.
        let off = AgentSettings { enabled: false, ..s.clone() };
        assert_eq!(AgentRun::for_ask_with_notice(&off, claude, None), (None, String::new()));
        assert_eq!(AgentRun::for_ask_with_notice(&s, RefineBackendKind::OpenaiCompatible, None), (None, String::new()));
        connections::forget_listing();
    }

    fn settings(connections: Vec<AgentConnection>) -> AgentSettings {
        AgentSettings { enabled: true, connections, ..AgentSettings::default() }
    }

    #[test]
    fn an_ask_is_an_agent_ask_only_with_the_switch_the_claude_backend_and_a_tool_to_call() {
        let ready = settings(vec![chat("chat", true, &["read_thread"])]);
        assert!(AgentRun::for_ask(&ready, RefineBackendKind::ClaudeCli, None).is_some());
        // The switch.
        let off = AgentSettings { enabled: false, ..ready.clone() };
        assert!(AgentRun::for_ask(&off, RefineBackendKind::ClaudeCli, None).is_none());
        // The backend.
        assert!(AgentRun::for_ask(&ready, RefineBackendKind::OpenaiCompatible, None).is_none());
        // A connection, enabled.
        assert!(AgentRun::for_ask(&settings(vec![]), RefineBackendKind::ClaudeCli, None).is_none());
        assert!(AgentRun::for_ask(&settings(vec![chat("chat", false, &["read_thread"])]), RefineBackendKind::ClaudeCli, None).is_none());
        // A ticked tool.
        assert!(AgentRun::for_ask(&settings(vec![chat("chat", true, &[])]), RefineBackendKind::ClaudeCli, None).is_none());
        // One that can run: with drafts only an act tool cannot.
        assert!(AgentRun::for_ask(&settings(vec![chat("chat", true, &["send_message"])]), RefineBackendKind::ClaudeCli, None).is_none());
        let acting = AgentSettings { drafts_only: false, ..settings(vec![chat("chat", true, &["send_message"])]) };
        let run = AgentRun::for_ask(&acting, RefineBackendKind::ClaudeCli, None).unwrap();
        assert_eq!(run.tools.confirm, ["mcp__chat__send_message"]);
        assert!(run.tools.allowed.is_empty());
        assert!(!run.drafts_only);
    }

    #[test]
    fn a_run_holds_only_the_connections_that_bring_a_tool() {
        let s = settings(vec![
            chat("chat", true, &["read_thread", "create_draft", "send_message"]),
            chat("idle", true, &[]),
            chat("off", false, &["read_thread"]),
        ]);
        let run = AgentRun::new(&s, None).unwrap();
        assert_eq!(run.connections.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["chat"]);
        assert_eq!(run.tools.allowed, ["mcp__chat__read_thread", "mcp__chat__create_draft"]);
        assert_eq!(run.tools.disallowed, ["mcp__chat__send_message"]);
        assert_eq!((run.max_turns, run.confirm_timeout_s, run.drafts_only), (12, 120, true));
        assert_eq!(run.tool("mcp__chat__create_draft").map(|t| t.kind), Some(ToolKind::Draft));
        assert!(run.tool("mcp__chat__send_message").is_none());
        assert!(run.tool("mcp__idle__read_thread").is_none());
    }

    #[test]
    fn the_app_in_front_is_routed_to_its_connection_only_when_the_run_has_it() {
        let table = Some(vec![
            AppConnection::new("com.example.chat", "Chat", "chat"),
            AppConnection::new("com.example.idle", "Idle", "idle"),
            AppConnection::new("com.example.gone", "Gone", "nowhere"),
        ]);
        let s = AgentSettings {
            app_connections: table,
            ..settings(vec![chat("chat", true, &["read_thread"]), chat("idle", true, &[])])
        };
        let route = |id: &str, name: &str| AgentRun::new(&s, Some(&FrontApp::new(id, name))).unwrap().route;
        assert_eq!(
            route("COM.Example.Chat", "Chat Desktop"),
            Some(Route { app_id: "com.example.chat".into(), app_name: "Chat Desktop".into(), connection: "chat".into() })
        );
        // Without a name from the system, the table's.
        assert_eq!(route("com.example.chat", "").unwrap().app_name, "Chat");
        // Mapped to a connection with nothing ticked, to one that
        // does not exist, and not mapped at all.
        assert_eq!(route("com.example.idle", "Idle"), None);
        assert_eq!(route("com.example.gone", "Gone"), None);
        assert_eq!(route("com.example.editor", "Editor"), None);
        assert_eq!(AgentRun::new(&s, None).unwrap().route, None);
    }

    #[test]
    fn readiness_says_in_plain_words_what_is_missing() {
        let claude = RefineBackendKind::ClaudeCli;
        let ready = settings(vec![chat("chat", true, &["read_thread", "send_message"])]);
        let says = |s: &AgentSettings, ask: bool, backend| readiness(s, ask, backend);
        assert_eq!(says(&AgentSettings::default(), true, claude), (false, "Agent is switched off.".into()));
        assert!(!says(&ready, false, claude).0);
        let (ok, why) = says(&ready, true, RefineBackendKind::OpenaiCompatible);
        assert!(!ok && why.contains("Claude backend"), "{why}");
        assert_eq!(says(&settings(vec![]), true, claude).1, "No connection is enabled yet.");
        let (ok, why) = says(&settings(vec![chat("chat", true, &["send_message"])]), true, claude);
        assert!(!ok && why.contains("drafts only"), "{why}");
        let (ok, why) = says(&ready, true, claude);
        assert!(ok && why.starts_with("An ask can use 1 tool on 1 connection.") && why.contains("Drafts only"), "{why}");
        let open = AgentSettings { drafts_only: false, ..ready };
        let (ok, why) = says(&open, true, claude);
        assert!(ok && why.contains("2 tools") && why.contains("1 of them ask you first"), "{why}");
    }

    #[test]
    fn errors_read_plainly_and_become_the_asks_own() {
        use crate::ask::AskError;
        assert_eq!(AgentError::Timeout.to_string(), "Claude Code did not answer in time");
        assert_eq!(AskError::from(AgentError::Timeout), AskError::Timeout);
        assert_eq!(AskError::from(AgentError::NotListed("refresh".into())), AskError::Unavailable("refresh".into()));
        assert_eq!(AskError::from(AgentError::Failed("broke".into())), AskError::Backend("broke".into()));
    }
}
