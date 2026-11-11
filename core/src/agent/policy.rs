//! Which tools an ask may use, and how: the rule, in one pure place.
//!
//! Every tool is one of three kinds ([`ToolKind`]), decided here from
//! its name and from what its connection says about it
//! ([`ToolInfo::read_only`], [`ToolInfo::destructive`]):
//!
//! - **read**: looks something up and changes nothing;
//! - **draft**: prepares a draft the person can still throw away;
//! - **act**: everything else. Sends, posts, replies, deletes,
//!   updates, creates, and every tool that cannot be told.
//!
//! The decision leans one way only. A tool is `read` or `draft` when
//! there is a positive reason to think so, and `act` otherwise, so a
//! name nobody has seen before is treated as if it sent something.
//! The cost of being wrong in that direction is a confirm too many
//! (or, with drafts only, a tool that is not there); the cost of
//! being wrong in the other is a message the person never meant to
//! send.
//!
//! The order of the rules ([`classify`]):
//!
//! 1. The connection says the tool can destroy something: `act`,
//!    whatever its name looks like.
//! 2. The name has a word that sends or removes (`send`, `post`,
//!    `delete`, `trash`, `publish`, ...): `act`. That includes
//!    `send_draft` and `delete_draft`, and it includes a tool whose
//!    connection calls it read-only.
//! 3. The name has the word `draft`: `read` when it only looks
//!    (`get_draft`, `list_drafts`), `draft` otherwise
//!    (`create_draft`, `update_draft`, `create_reply_draft`).
//! 4. The connection says the tool is read-only: `read`.
//! 5. The name looks like a lookup (`get`, `list`, `search`, `read`,
//!    `find`, `fetch`, `query`, `view`, `describe`, ...) and has no
//!    word that changes anything: `read`.
//! 6. Anything else: `act`.
//!
//! From the kinds and the settings comes the plan for one ask
//! ([`plan_tools`]): which tools run without asking (ticked read and
//! draft tools), which raise a confirm (ticked act tools, only while
//! drafts only is off), and which the model never gets to see
//! (everything else the enabled connections are known to have).
//! Nothing in here starts a process or reads a file.

use crate::settings::{AgentConnection, AgentSettings, ToolInfo};

pub use crate::ask::ToolKind;

/// Words that send something out or take something away. One of
/// them in a name makes the tool `act`, whatever else the name or
/// the connection says.
const HARD_ACT: &[&str] = &[
    "send", "sends", "post", "posts", "reply", "replies", "forward", "delete", "deletes", "remove", "trash", "untrash",
    "discard", "destroy", "purge", "wipe", "erase", "archive", "unarchive", "publish", "unpublish", "merge",
    "transition", "submit", "schedule", "execute", "exec", "run", "invoke", "trigger", "deploy", "cancel", "revoke",
    "approve", "reject", "pay", "charge", "refund", "order", "buy", "book", "invite", "share", "unshare", "kick",
    "ban", "block", "close", "reopen", "resolve", "restore", "revert", "reset", "drop", "truncate", "overwrite",
    "spawn", "stop", "start", "end", "kill", "terminate", "notify", "email", "call", "dial", "broadcast", "respond",
];

/// The hard words that a name with `draft` in it still means as an
/// action on the draft: sending it, or getting rid of it.
const DRAFT_ACT: &[&str] = &[
    "send", "sends", "post", "posts", "forward", "delete", "deletes", "remove", "trash", "discard", "destroy",
    "publish", "submit", "schedule", "execute", "exec", "run", "broadcast", "email",
];

/// Words that change something without sending it anywhere. A name
/// with one of them is not a lookup.
const CHANGE: &[&str] = &[
    "create", "creates", "update", "updates", "add", "adds", "set", "sets", "edit", "edits", "write", "writes",
    "save", "saves", "upload", "uploads", "make", "new", "insert", "patch", "put", "modify", "rename", "copy",
    "duplicate", "clone", "move", "moves", "mark", "unmark", "label", "unlabel", "apply", "register", "generate",
    "import", "export", "download", "convert", "resize", "link", "unlink", "attach", "detach", "assign", "unassign",
    "comment", "react", "reaction", "pin", "unpin", "star", "unstar", "follow", "unfollow", "subscribe",
    "unsubscribe", "join", "leave", "toggle", "enable", "disable", "install", "uninstall", "configure", "connect",
    "disconnect", "sync", "refresh", "replace", "append", "prepend", "compose", "complete", "restructure", "open",
    "login", "logout", "authenticate", "and",
];

/// Words that only look.
const READ: &[&str] = &[
    "get", "gets", "list", "lists", "search", "searches", "read", "reads", "find", "finds", "fetch", "query",
    "view", "describe", "lookup", "show", "count", "whoami",
];

/// The words of a tool name, in lower case: split at `_`, `-`, `.`,
/// space, and where a small letter or a digit is followed by a
/// capital (`getJiraIssue` is `get`, `jira`, `issue`).
pub fn name_words(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut previous: Option<char> = None;
    for c in name.chars() {
        let boundary = !c.is_alphanumeric()
            || (c.is_uppercase() && previous.is_some_and(|p| p.is_lowercase() || p.is_ascii_digit()));
        if boundary && !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
        if c.is_alphanumeric() {
            word.extend(c.to_lowercase());
        }
        previous = Some(c);
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

fn any_in(words: &[String], list: &[&str]) -> bool {
    words.iter().any(|w| list.contains(&w.as_str()))
}

/// The kind of the tool called `name` (its own name, no prefix), with
/// what its connection says about it. See the module docs for the
/// rules; everything in doubt is [`ToolKind::Act`].
pub fn classify(name: &str, read_only: Option<bool>, destructive: Option<bool>) -> ToolKind {
    if destructive == Some(true) {
        return ToolKind::Act;
    }
    let words = name_words(name);
    if words.is_empty() {
        return ToolKind::Act;
    }
    let draft = words.iter().any(|w| w == "draft" || w == "drafts");
    if draft {
        if any_in(&words, DRAFT_ACT) {
            return ToolKind::Act;
        }
        let looks_only = any_in(&words, READ) && !any_in(&words, CHANGE);
        return if looks_only { ToolKind::Read } else { ToolKind::Draft };
    }
    if any_in(&words, HARD_ACT) {
        return ToolKind::Act;
    }
    if read_only == Some(true) {
        return ToolKind::Read;
    }
    if any_in(&words, READ) && !any_in(&words, CHANGE) {
        return ToolKind::Read;
    }
    ToolKind::Act
}

/// [`classify`] for a listed tool.
pub fn kind_of(tool: &ToolInfo) -> ToolKind {
    classify(&tool.name, tool.read_only, tool.destructive)
}

/// A connection's name as it appears inside a tool's full name: the
/// Claude CLI replaces every character that is not a letter, a
/// digit, `_` or `-` with `_` (`claude.ai Team chat` becomes
/// `claude_ai_Team_chat`).
pub fn server_key(connection: &str) -> String {
    connection.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
}

/// The name the model and the CLI use for the tool `tool` of the
/// connection `connection`: `mcp__connection__tool`.
pub fn full_tool_name(connection: &str, tool: &str) -> String {
    format!("mcp__{}__{tool}", server_key(connection))
}

/// The tools of one ask, by what happens when the model calls them.
/// Full names ([`full_tool_name`]), each in exactly one list.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ToolPlan {
    /// Run without asking: ticked read and draft tools.
    pub allowed: Vec<String>,
    /// Raise a confirm every time: ticked act tools. Empty while
    /// drafts only is on.
    pub confirm: Vec<String>,
    /// Never run, and are taken out of what the model sees: every
    /// other tool the enabled connections are known to have, and
    /// with drafts only every act tool, ticked or not.
    pub disallowed: Vec<String>,
}

impl ToolPlan {
    /// The model has at least one tool it can call.
    pub fn has_tools(&self) -> bool {
        !self.allowed.is_empty() || !self.confirm.is_empty()
    }
}

/// One tool of a plan, with what is known about it: for the prompt
/// and for the steps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedTool {
    /// [`full_tool_name`].
    pub full_name: String,
    pub connection: String,
    pub tool: String,
    /// Name to show a person; the tool's name when it has no other.
    pub title: String,
    pub kind: ToolKind,
    /// The call waits for the person's yes.
    pub confirm: bool,
}

/// The enabled connections of `settings` an ask can use at all: each
/// name once (two names the CLI would spell the same count as one,
/// the first), in the order of the settings.
pub fn enabled_connections(settings: &AgentSettings) -> Vec<&AgentConnection> {
    let mut seen = std::collections::HashSet::new();
    settings.connections.iter().filter(|c| c.enabled && seen.insert(server_key(&c.name).to_lowercase())).collect()
}

/// The plan for an ask with these settings. Does not look at
/// [`AgentSettings::enabled`]: that is for the caller, which may be
/// `openwhisper agent run`.
///
/// Only a tool the connection listed can be allowed: a ticked name
/// that is not in [`AgentConnection::tools`] is a tool nobody has
/// seen, so it is left out (and the CLI asks about it when the model
/// finds it anyway, which is then answered with a no).
pub fn plan_tools(settings: &AgentSettings) -> ToolPlan {
    let mut plan = ToolPlan::default();
    for connection in enabled_connections(settings) {
        for tool in &connection.tools {
            let full = full_tool_name(&connection.name, &tool.name);
            let ticked = connection.allows(&tool.name);
            match (ticked, kind_of(tool)) {
                (true, ToolKind::Read | ToolKind::Draft) => plan.allowed.push(full),
                (true, ToolKind::Act) if !settings.drafts_only => plan.confirm.push(full),
                _ => plan.disallowed.push(full),
            }
        }
    }
    plan
}

/// The tools of [`plan_tools`] the model can call, with what is
/// known about each, in the order of the settings.
pub fn planned_tools(settings: &AgentSettings) -> Vec<PlannedTool> {
    let mut tools = Vec::new();
    for connection in enabled_connections(settings) {
        for tool in &connection.tools {
            let kind = kind_of(tool);
            let ticked = connection.allows(&tool.name);
            let confirm = kind == ToolKind::Act;
            if !ticked || (confirm && settings.drafts_only) {
                continue;
            }
            tools.push(PlannedTool {
                full_name: full_tool_name(&connection.name, &tool.name),
                connection: connection.name.clone(),
                tool: tool.name.clone(),
                title: if tool.title.trim().is_empty() { tool.name.clone() } else { tool.title.clone() },
                kind,
                confirm,
            });
        }
    }
    tools
}

#[cfg(test)]
mod tests {
    use super::*;
    use ToolKind::{Act, Draft, Read};

    #[test]
    fn names_split_at_separators_and_at_camel_case() {
        assert_eq!(name_words("slack_read_thread"), ["slack", "read", "thread"]);
        assert_eq!(name_words("getJiraIssue"), ["get", "jira", "issue"]);
        assert_eq!(name_words("notion-get-self"), ["notion", "get", "self"]);
        assert_eq!(name_words("searchJiraIssuesUsingJql"), ["search", "jira", "issues", "using", "jql"]);
        assert_eq!(name_words("HTTPRequest"), ["httprequest"]);
        assert_eq!(name_words("list2Items"), ["list2", "items"]);
        assert_eq!(name_words("  __ "), Vec::<String>::new());
    }

    /// Names as real connections spell them: a chat tool, mail, an
    /// issue tracker, documents, a design tool, a calendar, and a
    /// few that belong nowhere. `None, None` = the connection says
    /// nothing about the tool.
    #[test]
    fn real_world_tool_names_land_on_the_careful_side() {
        let table: &[(&str, Option<bool>, Option<bool>, ToolKind)] = &[
            // --- chat
            ("slack_read_thread", None, None, Read),
            ("slack_read_channel", Some(true), None, Read),
            ("slack_search_public", None, None, Read),
            ("slack_search_public_and_private", Some(true), None, Read),
            // By its name alone the "and" makes it a compound: act.
            ("slack_search_public_and_private", None, None, Act),
            ("slack_list_channel_members", None, None, Read),
            ("slack_send_message", None, None, Act),
            ("slack_send_message", Some(true), None, Act),
            ("slack_schedule_message", None, None, Act),
            ("slack_add_reaction", None, None, Act),
            ("slack_create_canvas", None, None, Act),
            // "send" and "draft" in one name: it could be either, so act.
            ("slack_send_message_draft", None, None, Act),
            ("teams_list_chats", None, None, Read),
            ("teams_send_chat_message", None, None, Act),
            ("teams_reply_channel_message", None, None, Act),
            ("chat_message_search", None, None, Read),
            // --- mail
            ("search_threads", None, None, Read),
            ("get_thread", None, None, Read),
            ("get_message", None, None, Read),
            ("list_labels", None, None, Read),
            ("list_drafts", None, None, Read),
            ("get_draft", None, None, Read),
            ("create_draft", None, None, Draft),
            ("update_draft", None, None, Draft),
            ("outlook_create_draft", None, None, Draft),
            ("outlook_create_reply_draft", None, None, Draft),
            ("outlook_create_reply_all_draft", None, None, Draft),
            ("outlook_update_draft", None, None, Draft),
            ("send_draft", None, None, Act),
            ("outlook_send_draft", None, None, Act),
            ("delete_draft", None, None, Act),
            ("outlook_delete_draft", None, Some(true), Act),
            ("send_message", None, None, Act),
            ("outlook_send_mail", None, None, Act),
            ("reply", None, None, Act),
            ("forward", None, None, Act),
            ("outlook_forward_mail", None, None, Act),
            ("trash_message", None, None, Act),
            ("trash_thread", None, Some(true), Act),
            ("untrash_thread", None, None, Act),
            ("mark_message_spam", None, None, Act),
            ("label_message", None, None, Act),
            ("create_label", None, None, Act),
            ("outlook_batch_delete_messages", None, None, Act),
            ("outlook_set_vacation", None, None, Act),
            // --- issue tracker
            ("getJiraIssue", None, None, Read),
            ("searchJiraIssuesUsingJql", None, None, Read),
            ("listJiraIssueComments", None, None, Read),
            ("createJiraIssue", None, None, Act),
            ("editJiraIssue", None, None, Act),
            ("transitionJiraIssue", None, None, Act),
            ("addOrEditJiraIssueComment", None, None, Act),
            ("executeRead", Some(true), None, Act),
            ("executeDestructive", None, Some(true), Act),
            ("list_issues", None, None, Read),
            ("get_issue", None, None, Read),
            ("create_issue", None, None, Act),
            ("update_issue", None, None, Act),
            ("merge_pull_request", None, None, Act),
            ("close_issue", None, None, Act),
            // --- documents
            ("notion-search", None, None, Read),
            ("notion-fetch", None, None, Read),
            ("notion-get-comments", None, None, Read),
            ("notion-query-data-sources", None, None, Read),
            ("notion-create-pages", None, None, Act),
            ("notion-update-page", None, None, Act),
            ("notion-delete-pages", None, None, Act),
            ("notion-move-pages", None, None, Act),
            ("notion-create-comment", None, None, Act),
            ("notion-duplicate-page", None, None, Act),
            ("getConfluenceContent", None, None, Read),
            ("searchConfluence", None, None, Read),
            ("updateConfluenceContent", None, None, Act),
            ("sharepoint_search", None, None, Read),
            ("sharepoint_delete_item", None, None, Act),
            ("sharepoint_upload_file", None, None, Act),
            // --- design
            ("get_design_context", None, None, Read),
            ("get_screenshot", None, None, Read),
            ("get_metadata", None, None, Read),
            ("search_design_system", None, None, Read),
            ("create_new_file", None, None, Act),
            ("generate_diagram", None, None, Act),
            ("use_figma", None, None, Act),
            ("upload_assets", None, None, Act),
            ("create-brand-template-draft", None, None, Draft),
            ("publish-brand-template", None, None, Act),
            // --- calendar
            ("outlook_calendar_search", None, None, Read),
            ("find_meeting_availability", None, None, Read),
            ("notion-calendar-list-events", None, None, Read),
            ("outlook_create_event", None, None, Act),
            ("outlook_update_event", None, None, Act),
            ("outlook_delete_event", None, None, Act),
            ("outlook_respond_to_event", None, None, Act),
            ("notion-calendar-cancel-events", None, None, Act),
            // --- the edge cases
            // Destructive beats a name that looks like a read.
            ("get_and_purge_cache", None, Some(true), Act),
            ("list_items", None, Some(true), Act),
            ("read_thread", Some(true), Some(true), Act),
            // Read-only by the connection's word, with a neutral name.
            ("whoami", None, None, Read),
            ("thread_contents", Some(true), None, Read),
            ("thread_contents", None, None, Act),
            ("thread_contents", Some(false), None, Act),
            // Unknown is act.
            ("frobnicate", None, None, Act),
            ("do_the_thing", None, None, Act),
            ("", None, None, Act),
            ("___", None, None, Act),
            // A lookup that also changes something is not a lookup.
            ("find_and_replace", None, None, Act),
            ("get_or_create_page", None, None, Act),
            ("read_and_mark_done", None, None, Act),
            ("search_and_destroy", None, None, Act),
            // A draft that is only looked at, and one that is acted on.
            ("list_message_drafts", None, None, Read),
            ("publish_draft", None, None, Act),
            ("publishDraft", None, None, Act),
            ("restoreVersionAsDraft", None, None, Draft),
            ("discard_draft", None, None, Act),
        ];
        for (name, read_only, destructive, expected) in table {
            assert_eq!(
                classify(name, *read_only, *destructive),
                *expected,
                "{name} (read_only {read_only:?}, destructive {destructive:?})"
            );
        }
    }

    fn connection(name: &str, enabled: bool, ticked: &[&str]) -> AgentConnection {
        AgentConnection {
            name: name.into(),
            enabled,
            allowed_tools: ticked.iter().map(|t| t.to_string()).collect(),
            tools: vec![
                ToolInfo { read_only: Some(true), ..ToolInfo::new("read_thread") },
                ToolInfo::new("search_messages"),
                ToolInfo { title: "Create Draft".into(), ..ToolInfo::new("create_draft") },
                ToolInfo::new("send_message"),
                ToolInfo { destructive: Some(true), ..ToolInfo::new("delete_message") },
            ],
            ..AgentConnection::default()
        }
    }

    #[test]
    fn full_names_are_spelled_the_way_the_cli_spells_them() {
        assert_eq!(full_tool_name("fake", "read_thread"), "mcp__fake__read_thread");
        assert_eq!(full_tool_name("claude.ai Team chat", "send"), "mcp__claude_ai_Team_chat__send");
        assert_eq!(full_tool_name("plugin:tools:helper-one", "x"), "mcp__plugin_tools_helper-one__x");
        assert_eq!(server_key("caf\u{e9} notes"), "caf__notes");
    }

    #[test]
    fn drafts_only_puts_every_act_tool_out_of_reach_whatever_is_ticked() {
        let all = ["read_thread", "search_messages", "create_draft", "send_message", "delete_message"];
        let settings = AgentSettings {
            drafts_only: true,
            connections: vec![connection("chat", true, &all)],
            ..AgentSettings::default()
        };
        let plan = plan_tools(&settings);
        assert_eq!(plan.allowed, ["mcp__chat__read_thread", "mcp__chat__search_messages", "mcp__chat__create_draft"]);
        assert!(plan.confirm.is_empty());
        assert_eq!(plan.disallowed, ["mcp__chat__send_message", "mcp__chat__delete_message"]);
        assert!(plan.has_tools());
        let names: Vec<String> = planned_tools(&settings).into_iter().map(|t| t.tool).collect();
        assert_eq!(names, ["read_thread", "search_messages", "create_draft"]);
    }

    #[test]
    fn with_drafts_only_off_a_ticked_act_tool_asks_and_an_unticked_one_is_gone() {
        let settings = AgentSettings {
            drafts_only: false,
            connections: vec![
                connection("chat", true, &["read_thread", "send_message", "not_listed"]),
                // Switched off: none of its tools are even named.
                connection("mail", false, &["read_thread", "send_message"]),
            ],
            ..AgentSettings::default()
        };
        let plan = plan_tools(&settings);
        assert_eq!(plan.allowed, ["mcp__chat__read_thread"]);
        assert_eq!(plan.confirm, ["mcp__chat__send_message"]);
        assert_eq!(
            plan.disallowed,
            ["mcp__chat__search_messages", "mcp__chat__create_draft", "mcp__chat__delete_message"]
        );
        // Every listed tool of the enabled connection is in exactly
        // one list, and a ticked name nobody listed is in none.
        let all: Vec<&String> = plan.allowed.iter().chain(&plan.confirm).chain(&plan.disallowed).collect();
        assert_eq!(all.len(), 5);
        assert!(!all.iter().any(|name| name.contains("not_listed") || name.contains("mail")));
        let planned = planned_tools(&settings);
        assert_eq!(planned.len(), 2);
        assert_eq!((planned[0].kind, planned[0].confirm), (Read, false));
        assert_eq!((planned[1].kind, planned[1].confirm, planned[1].title.as_str()), (Act, true, "send_message"));
    }

    #[test]
    fn nothing_ticked_or_nothing_enabled_is_no_tools_at_all() {
        assert!(!plan_tools(&AgentSettings::default()).has_tools());
        let unticked = AgentSettings { connections: vec![connection("chat", true, &[])], ..AgentSettings::default() };
        let plan = plan_tools(&unticked);
        assert!(!plan.has_tools());
        assert_eq!(plan.disallowed.len(), 5);
        // Only act tools ticked, with drafts only: nothing to call.
        let acts = AgentSettings {
            connections: vec![connection("chat", true, &["send_message", "delete_message"])],
            ..AgentSettings::default()
        };
        assert!(!plan_tools(&acts).has_tools());
    }

    #[test]
    fn two_connections_the_cli_would_spell_alike_count_once() {
        let settings = AgentSettings {
            connections: vec![connection("team chat", true, &["read_thread"]), connection("team.chat", true, &["read_thread"])],
            ..AgentSettings::default()
        };
        assert_eq!(enabled_connections(&settings).len(), 1);
        assert_eq!(plan_tools(&settings).allowed, ["mcp__team_chat__read_thread"]);
    }
}
