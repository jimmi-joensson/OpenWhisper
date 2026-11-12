//! `agent` block of `settings.json` (TASK-99).
//!
//! Agent lets an ask use tool connections the person already has in
//! their own Claude Code: read a thread in the chat tool, prepare a
//! draft, and, only when they allow it each time, send something.
//! This block says whether that is on at all, which connections an
//! ask may use, which of their tools, and which app belongs to which
//! connection.
//!
//! What is stored, and what never is:
//!
//! - A connection the person's Claude Code has ([`AgentConnectionKind::Claude`])
//!   is stored by **name only**, with the switch, the ticked tools and
//!   the tool list as it was last seen. How to reach it (a command, a
//!   URL, a server's environment, headers) can hold secrets and is not
//!   written here: it lives in memory after a listing
//!   (`core::agent::connections`).
//! - A connection the person added by hand ([`AgentConnectionKind::Custom`])
//!   is their own entry and is stored as they typed it
//!   ([`CustomServer`]). Its environment is never printed
//!   ([`CustomServer`]'s `Debug` shows the names of the variables,
//!   not their values).
//!
//! Every default is the careful one: Agent is off, nothing is enabled
//! until the person enables it, no tool is ticked until they tick it,
//! and drafts only is on, so nothing that sends, posts, deletes or
//! changes anything can run. A block that cannot be read is Agent
//! **off** with drafts only **on**: what the person chose cannot be
//! told, so an ask has no tools until they choose again.
//!
//! The app table ([`AppConnection`]) is an `Option` like the lists of
//! the Vision block: `None` means the built-in table
//! ([`default_app_connections`]), so a person who never edited it
//! gets the table of whatever version they run.
//!
//! The types are not behind a cargo feature: a settings file is the
//! same file in every build. The code that runs a tool is
//! (`core::agent`, feature `agent`).

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// How long a confirm waits for the person, in seconds. No answer in
/// that time is a deny.
pub const AGENT_CONFIRM_TIMEOUT_DEFAULT_S: u64 = 120;
pub const AGENT_CONFIRM_TIMEOUT_MIN_S: u64 = 10;
pub const AGENT_CONFIRM_TIMEOUT_MAX_S: u64 = 600;
/// How many times the model may go back and forth with its tools in
/// one ask.
pub const AGENT_MAX_TURNS_DEFAULT: u32 = 12;
pub const AGENT_MAX_TURNS_MIN: u32 = 1;
pub const AGENT_MAX_TURNS_MAX: u32 = 40;
/// Most connections kept, most tools per connection, and the longest
/// name of either, in characters.
pub const AGENT_CONNECTIONS_MAX: usize = 100;
pub const AGENT_TOOLS_MAX: usize = 500;
pub const AGENT_NAME_MAX_CHARS: usize = 120;
/// The longest name a tool can have and still be used. Claude Code
/// drops a tool whose full name (`mcp__connection__tool`) is longer
/// than this (seen with Claude Code 2.1.322), so a longer own name
/// can never be called.
pub const AGENT_TOOL_NAME_MAX_CHARS: usize = 128;
/// Most app-to-connection entries kept.
pub const AGENT_APP_CONNECTIONS_MAX: usize = 200;
/// A custom connection: most arguments and variables, and the longest
/// command, argument, value or URL.
pub const AGENT_CUSTOM_ITEMS_MAX: usize = 50;
pub const AGENT_CUSTOM_FIELD_MAX_CHARS: usize = 2000;

/// Where a connection comes from.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AgentConnectionKind {
    /// One of the connections the person's Claude Code has. Stored by
    /// name; its details are never written to the settings.
    #[default]
    Claude,
    /// The person's own entry: a command to start, or a URL.
    Custom,
}

impl AgentConnectionKind {
    /// Stable id, same as the serialized form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Custom => "custom",
        }
    }
}

/// One tool of a connection, as the connection described it when it
/// was last listed.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(default)]
pub struct ToolInfo {
    /// The tool's own name, without the `mcp__connection__` prefix.
    pub name: String,
    /// Name to show a person ("Read Thread"). May be empty.
    pub title: String,
    /// The connection says the tool changes nothing. `None` when it
    /// does not say.
    pub read_only: Option<bool>,
    /// The connection says the tool can destroy something. `None`
    /// when it does not say.
    pub destructive: Option<bool>,
}

impl ToolInfo {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string(), ..Self::default() }
    }

    /// [`tool_name_is_usable`] for this tool.
    pub fn has_usable_name(&self) -> bool {
        tool_name_is_usable(&self.name)
    }
}

/// Whether a tool with this name can be used at all: 1 to
/// [`AGENT_TOOL_NAME_MAX_CHARS`] characters, each a letter or digit
/// of ASCII, `_`, `.` or `-`.
///
/// The names of the tools an ask may and may not use are handed to
/// Claude Code as lists on its command line, and a tool's name comes
/// from its connection, which is not ours. A name with a comma in it
/// would be read as two entries (`list,mcp__teamchat` would allow a
/// whole other connection), and a space, `*` or `(` means something
/// to the CLI's rules as well. So a name with anything else in it is
/// not passed on in any form: the tool stays in the listing, marked
/// as unusable so the person sees why, and it can never be ticked,
/// never be on a list, and never be named in the prompt. The same
/// check guards every way a name comes in: a listing, the settings
/// file ([`AgentSettings::cleaned`]) and `openwhisper agent allow`.
pub fn tool_name_is_usable(name: &str) -> bool {
    let count = name.chars().count();
    (1..=AGENT_TOOL_NAME_MAX_CHARS).contains(&count)
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// A tool's name as it is kept. A usable name is kept exactly: it is
/// what the connection calls the tool, and one changed character is
/// another tool. A name that cannot be used is kept for showing
/// only, cut at [`AGENT_TOOL_NAME_MAX_CHARS`] with an ellipsis (which
/// also keeps a name that was cut from ever passing for a usable
/// one).
fn kept_tool_name(name: &str) -> String {
    if tool_name_is_usable(name) || name.chars().count() <= AGENT_TOOL_NAME_MAX_CHARS {
        return name.to_string();
    }
    let mut cut: String = name.chars().take(AGENT_TOOL_NAME_MAX_CHARS).collect();
    cut.push('\u{2026}');
    cut
}

/// How a custom connection is reached: a command with its arguments
/// and environment (a local server that talks over stdin and stdout),
/// or a URL. The command wins when both are set.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(default)]
pub struct CustomServer {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub url: String,
}

/// Never prints the values of the environment: a token does not
/// belong in a log.
impl fmt::Debug for CustomServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CustomServer")
            .field("command", &self.command)
            .field("args", &self.args.len())
            .field("env", &self.env.keys().collect::<Vec<_>>())
            .field("url", &!self.url.is_empty())
            .finish()
    }
}

impl CustomServer {
    /// There is something to start or to reach.
    pub fn is_usable(&self) -> bool {
        !self.command.trim().is_empty() || !self.url.trim().is_empty()
    }
}

/// One connection an ask may use.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(default)]
pub struct AgentConnection {
    /// The connection's name: the one Claude Code knows it by, or the
    /// one the person gave their own entry.
    pub name: String,
    pub kind: AgentConnectionKind,
    /// An ask may use this connection. Off until the person says so.
    pub enabled: bool,
    /// The tools the person ticked, by their own name (no prefix).
    pub allowed_tools: Vec<String>,
    /// The connection's tools as they were last listed. A tool that
    /// is not in here cannot run, ticked or not: nobody has seen what
    /// it is.
    pub tools: Vec<ToolInfo>,
    /// Only for [`AgentConnectionKind::Custom`]. Always `None` for a
    /// Claude connection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom: Option<CustomServer>,
}

impl AgentConnection {
    /// The tool `name` is ticked. Never true for a name that cannot
    /// be used ([`tool_name_is_usable`]).
    pub fn allows(&self, name: &str) -> bool {
        tool_name_is_usable(name) && self.allowed_tools.iter().any(|t| t == name)
    }

    /// The listed tool `name`, if the connection has one.
    pub fn tool(&self, name: &str) -> Option<&ToolInfo> {
        self.tools.iter().find(|t| t.name == name)
    }
}

/// An app, and the connection that reaches what is in it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(default)]
pub struct AppConnection {
    /// The id `core::focus` reports for the app, in lower case: the
    /// bundle id on macOS, the exe file name on Windows.
    pub app_id: String,
    /// Name to show a person ("Slack"). May be empty.
    pub app_name: String,
    /// Name of the connection. Compared the forgiving way of
    /// [`connection_name_matches`].
    pub connection: String,
}

impl AppConnection {
    pub fn new(app_id: &str, app_name: &str, connection: &str) -> Self {
        Self { app_id: app_id.to_string(), app_name: app_name.to_string(), connection: connection.to_string() }
    }
}

/// The built-in table: apps whose connection is plainly the same
/// product, under the ids they have on macOS and on Windows. Kept
/// short on purpose. A mail app is not in it: which connection reads
/// the mail in it depends on the account, not on the app.
pub fn default_app_connections() -> Vec<AppConnection> {
    [
        ("com.tinyspeck.slackmacgap", "Slack", "Slack"),
        ("slack.exe", "Slack", "Slack"),
        ("notion.id", "Notion", "Notion"),
        ("notion.exe", "Notion", "Notion"),
        ("com.linear", "Linear", "Linear"),
        ("linear.exe", "Linear", "Linear"),
        ("com.figma.desktop", "Figma", "Figma"),
        ("figma.exe", "Figma", "Figma"),
    ]
    .iter()
    .map(|(id, name, connection)| AppConnection::new(id, name, connection))
    .collect()
}

/// A connection name for comparing: lower case, without the prefix
/// Claude Code gives the connectors of the person's account
/// (`claude.ai Slack`), and without anything that is not a letter or
/// a digit.
fn name_key(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    let bare = lower.strip_prefix("claude.ai ").unwrap_or(&lower);
    bare.chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Whether the connection called `name` is the one an app table entry
/// means by `wanted`. "Slack" means `Slack`, `slack` and the account
/// connector `claude.ai Slack`; it does not mean `Slack archive`.
pub fn connection_name_matches(wanted: &str, name: &str) -> bool {
    let wanted = name_key(wanted);
    !wanted.is_empty() && wanted == name_key(name)
}

/// The `agent` settings block.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct AgentSettings {
    /// Agent is on. Off by default. It only does anything in an ask,
    /// so Ask has to be on too, with the Claude backend.
    pub enabled: bool,
    /// Nothing that sends, posts, deletes or changes anything can
    /// run, ticked or not. On by default.
    pub drafts_only: bool,
    pub connections: Vec<AgentConnection>,
    /// Which app belongs to which connection. `None` = the built-in
    /// table ([`default_app_connections`]).
    pub app_connections: Option<Vec<AppConnection>>,
    /// See [`AGENT_CONFIRM_TIMEOUT_DEFAULT_S`].
    pub confirm_timeout_s: u64,
    /// See [`AGENT_MAX_TURNS_DEFAULT`].
    pub max_turns: u32,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            drafts_only: true,
            connections: Vec::new(),
            app_connections: None,
            confirm_timeout_s: AGENT_CONFIRM_TIMEOUT_DEFAULT_S,
            max_turns: AGENT_MAX_TURNS_DEFAULT,
        }
    }
}

// --- the block for showing: no values of the environment ------------

/// A custom connection for showing or printing: [`CustomServer`]
/// with the **names** of its environment variables and none of their
/// values. There is no way from this type back to a value.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct CustomServerView {
    pub command: String,
    pub args: Vec<String>,
    /// The names of the variables, in order. Never a value.
    pub env: Vec<String>,
    pub url: String,
}

/// [`AgentConnection`] for showing or printing.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentConnectionView {
    pub name: String,
    pub kind: AgentConnectionKind,
    pub enabled: bool,
    pub allowed_tools: Vec<String>,
    pub tools: Vec<ToolInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom: Option<CustomServerView>,
}

/// The Agent block for showing or printing ([`AgentSettings::for_output`]):
/// the same fields as [`AgentSettings`], except that the environment
/// of a custom connection is the names of its variables only.
///
/// [`AgentSettings`] itself serializes with the values, because that
/// is how the person's own entries are written to `settings.json`.
/// Everything that goes anywhere else (a terminal, `--json`, a
/// window, a log) is made from this type instead, so a token cannot
/// leave through an output by being forgotten: the value is not in
/// the type.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentSettingsView {
    pub enabled: bool,
    pub drafts_only: bool,
    pub connections: Vec<AgentConnectionView>,
    pub app_connections: Option<Vec<AppConnection>>,
    pub confirm_timeout_s: u64,
    pub max_turns: u32,
}

impl CustomServer {
    /// For showing or printing: see [`CustomServerView`].
    pub fn for_output(&self) -> CustomServerView {
        CustomServerView {
            command: self.command.clone(),
            args: self.args.clone(),
            env: self.env.keys().cloned().collect(),
            url: self.url.clone(),
        }
    }
}

impl AgentConnection {
    /// For showing or printing: see [`AgentSettingsView`].
    pub fn for_output(&self) -> AgentConnectionView {
        AgentConnectionView {
            name: self.name.clone(),
            kind: self.kind,
            enabled: self.enabled,
            allowed_tools: self.allowed_tools.clone(),
            tools: self.tools.clone(),
            custom: self.custom.as_ref().map(CustomServer::for_output),
        }
    }
}

impl AgentSettings {
    /// For showing or printing: see [`AgentSettingsView`].
    pub fn for_output(&self) -> AgentSettingsView {
        AgentSettingsView {
            enabled: self.enabled,
            drafts_only: self.drafts_only,
            connections: self.connections.iter().map(AgentConnection::for_output).collect(),
            app_connections: self.app_connections.clone(),
            confirm_timeout_s: self.confirm_timeout_s,
            max_turns: self.max_turns,
        }
    }
}

/// For a field that holds an [`AgentSettings`] inside something that
/// is serialized for output (`#[serde(serialize_with = ...)]`).
pub fn serialize_agent_settings_for_output<S: serde::Serializer>(settings: &AgentSettings, serializer: S) -> Result<S::Ok, S::Error> {
    settings.for_output().serialize(serializer)
}

fn capped(s: &str, max: usize) -> String {
    s.trim().chars().take(max).collect()
}

/// A name on one line, capped.
fn tidy_name(s: &str) -> String {
    capped(&s.split_whitespace().collect::<Vec<_>>().join(" "), AGENT_NAME_MAX_CHARS)
}

impl CustomServer {
    fn cleaned(mut self) -> Self {
        self.command = capped(&self.command, AGENT_CUSTOM_FIELD_MAX_CHARS);
        self.url = capped(&self.url, AGENT_CUSTOM_FIELD_MAX_CHARS);
        self.args.truncate(AGENT_CUSTOM_ITEMS_MAX);
        for arg in &mut self.args {
            *arg = arg.chars().take(AGENT_CUSTOM_FIELD_MAX_CHARS).collect();
        }
        self.env = self
            .env
            .into_iter()
            .filter(|(key, _)| !key.trim().is_empty())
            .take(AGENT_CUSTOM_ITEMS_MAX)
            .map(|(key, value)| (capped(&key, AGENT_NAME_MAX_CHARS), value.chars().take(AGENT_CUSTOM_FIELD_MAX_CHARS).collect()))
            .collect();
        self
    }
}

impl AgentConnection {
    fn cleaned(mut self) -> Self {
        self.name = tidy_name(&self.name);
        let mut seen = std::collections::HashSet::new();
        self.tools = self
            .tools
            .into_iter()
            .map(|mut tool| {
                // Never tidied: see `kept_tool_name`.
                tool.name = kept_tool_name(&tool.name);
                tool.title = tidy_name(&tool.title);
                tool
            })
            .filter(|tool| !tool.name.trim().is_empty() && seen.insert(tool.name.clone()))
            .take(AGENT_TOOLS_MAX)
            .collect();
        // A tick is a name that will be handed to Claude Code: only
        // a usable one can be a tick, whatever a file says.
        let mut seen = std::collections::HashSet::new();
        self.allowed_tools = self
            .allowed_tools
            .into_iter()
            .filter(|name| tool_name_is_usable(name) && seen.insert(name.clone()))
            .take(AGENT_TOOLS_MAX)
            .collect();
        self.custom = match self.kind {
            // Never stored, whatever a file says.
            AgentConnectionKind::Claude => None,
            AgentConnectionKind::Custom => self.custom.map(CustomServer::cleaned),
        };
        self
    }

    /// Worth keeping: it has a name, and a custom one says how it is
    /// reached.
    fn is_valid(&self) -> bool {
        !self.name.is_empty()
            && match self.kind {
                AgentConnectionKind::Claude => true,
                AgentConnectionKind::Custom => self.custom.as_ref().is_some_and(CustomServer::is_usable),
            }
    }
}

impl AgentSettings {
    /// The block as it is stored and used: connection names on one
    /// line and capped, tool names exactly as the connection gave
    /// them (a tick only for a name that can be used,
    /// [`tool_name_is_usable`]), each connection once (the first of
    /// two with the same name, ignoring case), custom connections
    /// that say nothing
    /// about how they are reached dropped, no details on a Claude
    /// connection, the numbers within their limits, app ids in lower
    /// case and each app once.
    pub fn cleaned(mut self) -> Self {
        let mut seen = std::collections::HashSet::new();
        self.connections = self
            .connections
            .into_iter()
            .map(AgentConnection::cleaned)
            .filter(|c| c.is_valid() && seen.insert(c.name.to_lowercase()))
            .take(AGENT_CONNECTIONS_MAX)
            .collect();
        self.app_connections = self.app_connections.map(|list| {
            let mut seen = std::collections::HashSet::new();
            list.into_iter()
                .map(|entry| AppConnection {
                    app_id: tidy_name(&entry.app_id).to_lowercase(),
                    app_name: tidy_name(&entry.app_name),
                    connection: tidy_name(&entry.connection),
                })
                .filter(|entry| !entry.app_id.is_empty() && !entry.connection.is_empty() && seen.insert(entry.app_id.clone()))
                .take(AGENT_APP_CONNECTIONS_MAX)
                .collect()
        });
        self.confirm_timeout_s = self.confirm_timeout_s.clamp(AGENT_CONFIRM_TIMEOUT_MIN_S, AGENT_CONFIRM_TIMEOUT_MAX_S);
        self.max_turns = self.max_turns.clamp(AGENT_MAX_TURNS_MIN, AGENT_MAX_TURNS_MAX);
        self
    }

    /// The app table in force: the person's, or the built-in one.
    pub fn effective_app_connections(&self) -> Vec<AppConnection> {
        self.app_connections.clone().unwrap_or_else(default_app_connections)
    }

    /// The connection called `name`, ignoring case.
    pub fn connection(&self, name: &str) -> Option<&AgentConnection> {
        let wanted = name.trim().to_lowercase();
        self.connections.iter().find(|c| c.name.to_lowercase() == wanted)
    }

    /// [`AgentSettings::connection`], to change it.
    pub fn connection_mut(&mut self, name: &str) -> Option<&mut AgentConnection> {
        let wanted = name.trim().to_lowercase();
        self.connections.iter_mut().find(|c| c.name.to_lowercase() == wanted)
    }

    /// The app table entry for the app `app_id` (compared ignoring
    /// case), and the enabled connection it names, when there is one.
    /// Says nothing about tools: whether an ask can use the
    /// connection is for `core::agent` to decide.
    pub fn connection_for_app(&self, app_id: &str) -> Option<(AppConnection, &AgentConnection)> {
        let app_id = app_id.trim().to_lowercase();
        if app_id.is_empty() {
            return None;
        }
        self.effective_app_connections().into_iter().filter(|entry| entry.app_id.to_lowercase() == app_id).find_map(
            |entry| {
                let connection =
                    self.connections.iter().find(|c| c.enabled && connection_name_matches(&entry.connection, &c.name))?;
                Some((entry, connection))
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom(name: &str, command: &str) -> AgentConnection {
        AgentConnection {
            name: name.into(),
            kind: AgentConnectionKind::Custom,
            custom: Some(CustomServer { command: command.into(), ..CustomServer::default() }),
            ..AgentConnection::default()
        }
    }

    #[test]
    fn the_defaults_are_the_careful_ones() {
        let s = AgentSettings::default();
        assert!(!s.enabled);
        assert!(s.drafts_only);
        assert!(s.connections.is_empty());
        assert_eq!(s.app_connections, None);
        assert_eq!((s.confirm_timeout_s, s.max_turns), (120, 12));
        // A file from before Agent existed, and an empty block.
        assert_eq!(serde_json::from_str::<AgentSettings>("{}").unwrap(), s);
        // A connection read from a file starts switched off with
        // nothing ticked.
        let c: AgentConnection = serde_json::from_str(r#"{"name":"Chat"}"#).unwrap();
        assert_eq!((c.enabled, c.allowed_tools.len(), c.kind), (false, 0, AgentConnectionKind::Claude));
    }

    #[test]
    fn cleaning_clamps_caps_and_keeps_each_connection_once() {
        let s = AgentSettings {
            confirm_timeout_s: 1,
            max_turns: 500,
            connections: vec![
                custom("  Chat \n tool ", "chat-server"),
                custom("chat TOOL", "another"),
                custom("", "nameless"),
                // A custom connection that does not say how to reach it.
                AgentConnection { name: "Empty".into(), kind: AgentConnectionKind::Custom, ..AgentConnection::default() },
                AgentConnection {
                    name: "Docs".into(),
                    allowed_tools: vec![" search ".into(), "search".into(), "".into()],
                    tools: vec![ToolInfo::new("search"), ToolInfo::new("search"), ToolInfo::new(" ")],
                    ..AgentConnection::default()
                },
            ],
            ..AgentSettings::default()
        }
        .cleaned();
        assert_eq!((s.confirm_timeout_s, s.max_turns), (AGENT_CONFIRM_TIMEOUT_MIN_S, AGENT_MAX_TURNS_MAX));
        let names: Vec<&str> = s.connections.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Chat tool", "Docs"]);
        assert_eq!(s.connections[0].custom.as_ref().unwrap().command, "chat-server");
        assert_eq!(s.connections[1].allowed_tools, ["search"]);
        assert_eq!(s.connections[1].tools.len(), 1);
        let low = AgentSettings { confirm_timeout_s: 100_000, max_turns: 0, ..AgentSettings::default() }.cleaned();
        assert_eq!((low.confirm_timeout_s, low.max_turns), (AGENT_CONFIRM_TIMEOUT_MAX_S, AGENT_MAX_TURNS_MIN));
        // Cleaning twice changes nothing.
        assert_eq!(s.clone().cleaned(), s);
    }

    #[test]
    fn a_claude_connection_never_carries_details_and_a_debug_print_never_shows_a_value() {
        let secret = "tok-made-up-123";
        let with_details = AgentConnection {
            name: "Chat".into(),
            kind: AgentConnectionKind::Claude,
            custom: Some(CustomServer {
                command: "server".into(),
                env: BTreeMap::from([("CHAT_TOKEN".to_string(), secret.to_string())]),
                ..CustomServer::default()
            }),
            ..AgentConnection::default()
        };
        let own = AgentConnection { kind: AgentConnectionKind::Custom, name: "Own".into(), ..with_details.clone() };
        let s = AgentSettings { connections: vec![with_details, own], ..AgentSettings::default() }.cleaned();
        assert_eq!(s.connections[0].custom, None);
        let stored = serde_json::to_string(&s.connections[0]).unwrap();
        assert!(!stored.contains("custom") && !stored.contains(secret), "{stored}");
        // The person's own entry keeps its environment in the file
        // (it is their entry) and out of every debug print.
        assert!(serde_json::to_string(&s.connections[1]).unwrap().contains(secret));
        let printed = format!("{s:?}");
        assert!(printed.contains("CHAT_TOKEN"), "{printed}");
        assert!(!printed.contains(secret), "{printed}");
    }

    #[test]
    fn the_app_table_is_built_in_until_it_is_edited() {
        let s = AgentSettings::default();
        let table = s.effective_app_connections();
        assert_eq!(table, default_app_connections());
        // Both platforms' ids for each product, in lower case.
        for id in ["com.tinyspeck.slackmacgap", "slack.exe", "notion.id", "com.linear", "com.figma.desktop", "figma.exe"] {
            assert!(table.iter().any(|e| e.app_id == id), "{id}");
        }
        assert!(table.iter().all(|e| e.app_id == e.app_id.to_lowercase() && !e.connection.is_empty()));
        // Nothing about mail: the app does not say which account.
        assert!(!table.iter().any(|e| e.app_id.contains("mail") || e.connection.to_lowercase().contains("mail")));

        let edited = AgentSettings {
            app_connections: Some(vec![
                AppConnection::new(" COM.Example.Chat ", " Chat ", "Team chat"),
                AppConnection::new("com.example.chat", "Twice", "Other"),
                AppConnection::new("", "No id", "Other"),
                AppConnection::new("com.example.docs", "Docs", " "),
            ]),
            ..AgentSettings::default()
        }
        .cleaned();
        assert_eq!(edited.effective_app_connections(), [AppConnection::new("com.example.chat", "Chat", "Team chat")]);
        // An empty table is the person's choice: no app has a connection.
        let none = AgentSettings { app_connections: Some(Vec::new()), ..AgentSettings::default() }.cleaned();
        assert!(none.effective_app_connections().is_empty());
    }

    #[test]
    fn an_app_finds_its_connection_by_a_forgiving_name_and_only_when_enabled() {
        assert!(connection_name_matches("Slack", "slack"));
        assert!(connection_name_matches("Slack", "claude.ai Slack"));
        assert!(connection_name_matches("team chat", "Team-Chat"));
        assert!(!connection_name_matches("Slack", "Slack archive"));
        assert!(!connection_name_matches("", ""));

        let mut s = AgentSettings {
            connections: vec![
                AgentConnection { name: "claude.ai Slack".into(), ..AgentConnection::default() },
                AgentConnection { name: "Notion".into(), enabled: true, ..AgentConnection::default() },
            ],
            ..AgentSettings::default()
        };
        // Slack is mapped, and its connection is switched off.
        assert!(s.connection_for_app("com.tinyspeck.slackmacgap").is_none());
        let (entry, connection) = s.connection_for_app("Notion.ID").expect("notion is mapped and enabled");
        assert_eq!((entry.app_name.as_str(), connection.name.as_str()), ("Notion", "Notion"));
        s.connections[0].enabled = true;
        let (_, connection) = s.connection_for_app("com.tinyspeck.slackmacgap").unwrap();
        assert_eq!(connection.name, "claude.ai Slack");
        // An app nobody mapped, and no app at all.
        assert!(s.connection_for_app("com.example.editor").is_none());
        assert!(s.connection_for_app("  ").is_none());
    }

    #[test]
    fn a_tool_name_is_usable_only_when_it_is_made_of_plain_characters() {
        for good in ["read_thread", "getJiraIssue", "notion-search", "dots.name", "a", "x9", &"x".repeat(128)] {
            assert!(tool_name_is_usable(good), "{good}");
        }
        for bad in [
            "",
            " ",
            "list,mcp__teamchat",
            "read thread",
            "read_thread ",
            "\nread_thread",
            "get*",
            "a(b)",
            "caf\u{e9}",
            "tab\tname",
            "slash/name",
            "colon:name",
            "quote\"name",
            "rtl\u{202e}name",
            &"x".repeat(129),
        ] {
            assert!(!tool_name_is_usable(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_tool_whose_name_cannot_be_used_stays_listed_as_it_is_and_can_never_be_ticked() {
        let comma = "list,mcp__teamchat";
        let long = "y".repeat(300);
        let s = AgentSettings {
            connections: vec![AgentConnection {
                name: "notes".into(),
                enabled: true,
                // As a file edited by hand, or written by an older
                // version, could have them.
                allowed_tools: vec![comma.into(), "list".into(), "read thread".into(), " list ".into(), long.clone()],
                tools: vec![
                    ToolInfo::new(comma),
                    ToolInfo::new("list"),
                    ToolInfo::new("read thread"),
                    // A name with a line break is not tidied into
                    // another tool's name.
                    ToolInfo::new("\nlist"),
                    ToolInfo::new(&long),
                ],
                ..AgentConnection::default()
            }],
            ..AgentSettings::default()
        }
        .cleaned();
        let notes = &s.connections[0];
        // Only the usable name is a tick.
        assert_eq!(notes.allowed_tools, ["list"]);
        assert!(notes.allows("list"));
        assert!(!notes.allows(comma));
        let names: Vec<&str> = notes.tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names[..4], [comma, "list", "read thread", "\nlist"]);
        // A very long name is cut for showing, and the cut one is
        // not usable either.
        assert_eq!(names[4].chars().count(), AGENT_TOOL_NAME_MAX_CHARS + 1);
        assert!(names[4].ends_with('\u{2026}'));
        let usable: Vec<bool> = notes.tools.iter().map(ToolInfo::has_usable_name).collect();
        assert_eq!(usable, [false, true, false, false, false]);
        // A tick smuggled in after cleaning still does not count.
        let mut smuggled = notes.clone();
        smuggled.allowed_tools.push(comma.into());
        assert!(!smuggled.allows(comma));
        // Cleaning twice changes nothing.
        assert_eq!(s.clone().cleaned(), s);
    }

    #[test]
    fn the_block_for_output_has_the_names_of_the_variables_and_no_value() {
        let secret = "tok-made-up-456";
        let s = AgentSettings {
            connections: vec![AgentConnection {
                name: "Own".into(),
                kind: AgentConnectionKind::Custom,
                custom: Some(CustomServer {
                    command: "server".into(),
                    args: vec!["--stdio".into()],
                    env: BTreeMap::from([("CHAT_TOKEN".to_string(), secret.to_string()), ("MODE".to_string(), "test".to_string())]),
                    ..CustomServer::default()
                }),
                ..AgentConnection::default()
            }],
            ..AgentSettings::default()
        }
        .cleaned();
        // The file has the value: it is the person's own entry.
        assert!(serde_json::to_string(&s).unwrap().contains(secret));
        // What is shown or printed has the names only.
        let shown = serde_json::to_value(s.for_output()).unwrap();
        assert_eq!(shown["connections"][0]["custom"]["env"], serde_json::json!(["CHAT_TOKEN", "MODE"]));
        assert_eq!(shown["connections"][0]["custom"]["command"], "server");
        let text = format!("{shown} {:?}", s.for_output());
        assert!(!text.contains(secret) && !text.contains("\"test\""), "{text}");
        // Every other field is the block's own.
        let mut file = serde_json::to_value(&s).unwrap();
        file["connections"][0]["custom"]["env"] = serde_json::json!(["CHAT_TOKEN", "MODE"]);
        assert_eq!(shown, file);
        // Inside something else that is serialized for output.
        #[derive(Serialize)]
        struct Wrapper {
            #[serde(serialize_with = "serialize_agent_settings_for_output")]
            settings: AgentSettings,
        }
        assert!(!serde_json::to_string(&Wrapper { settings: s }).unwrap().contains(secret));
    }

    #[test]
    fn a_block_round_trips_through_json() {
        let s = AgentSettings {
            enabled: true,
            drafts_only: false,
            connections: vec![AgentConnection {
                name: "Chat".into(),
                kind: AgentConnectionKind::Custom,
                enabled: true,
                allowed_tools: vec!["read_thread".into()],
                tools: vec![ToolInfo { name: "read_thread".into(), title: "Read Thread".into(), read_only: Some(true), destructive: None }],
                custom: Some(CustomServer { command: "chat-server".into(), args: vec!["--stdio".into()], ..CustomServer::default() }),
            }],
            app_connections: Some(vec![AppConnection::new("com.example.chat", "Chat", "Chat")]),
            confirm_timeout_s: 30,
            max_turns: 6,
        }
        .cleaned();
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<AgentSettings>(&json).unwrap(), s);
        assert!(json.contains(r#""kind":"custom""#), "{json}");
    }
}
