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
//!   history entry lists the tools an ask used.
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

use crate::settings::{AgentConnection, AgentSettings, RefineBackendKind};

pub mod claude;
pub mod connections;
pub mod eval;
pub mod fixture;
pub mod policy;
pub mod prompt;

pub use connections::{
    ConnectionSource, ConnectionState, LIST_WAIT, ListedConnection, Refreshed, RunConfig, list_claude_connections,
    list_custom_connections, merge_listing, refresh, write_run_config,
};
pub use policy::{PlannedTool, ToolKind, ToolPlan, classify, plan_tools};

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
    pub fn for_ask(settings: &AgentSettings, backend: RefineBackendKind, front: Option<&FrontApp>) -> Option<Self> {
        if !settings.enabled || backend == RefineBackendKind::OpenaiCompatible {
            return None;
        }
        Self::new(settings, front)
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

    fn chat(name: &str, enabled: bool, ticked: &[&str]) -> AgentConnection {
        AgentConnection {
            name: name.into(),
            enabled,
            allowed_tools: ticked.iter().map(|t| t.to_string()).collect(),
            tools: vec![
                ToolInfo { read_only: Some(true), ..ToolInfo::new("read_thread") },
                ToolInfo::new("create_draft"),
                ToolInfo::new("send_message"),
            ],
            ..AgentConnection::default()
        }
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
