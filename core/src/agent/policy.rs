//! Which tools an ask may use, and how: the rule, in one pure place.
//!
//! Every tool is one of three kinds ([`ToolKind`]), decided here from
//! its name and from what its connection says about it
//! ([`ToolInfo::read_only`], [`ToolInfo::destructive`]):
//!
//! - **read**: looks something up and changes nothing;
//! - **draft**: prepares a draft the person can still throw away;
//! - **act**: everything else. Sends, posts, replies, deletes,
//!   updates, creates, runs a query, and every tool that cannot be
//!   told.
//!
//! **This is a heuristic, and it leans one way only.** Nothing but a
//! tool's name and two optional notes from its connection is known
//! about it, and none of the three is ours: a connection can call a
//! tool that sends mail `get_summary`. So a tool is `read` or `draft`
//! only when everything that is known points that way, and `act`
//! otherwise: a name nobody has seen before is treated as if it sent
//! something. The cost of being wrong in that direction is a confirm
//! too many (or, with drafts only, a tool that is not there); the
//! cost of being wrong in the other is a message the person never
//! meant to send. What the heuristic cannot give is a promise that a
//! tool it calls `read` reads only. That is why the kind of every
//! tool is shown to the person where they tick it (the Agent pane,
//! `openwhisper agent tools`): the tick is theirs, with the kind in
//! front of them.
//!
//! The rules ([`classify`]), in order:
//!
//! 1. The connection says the tool can destroy something: `act`,
//!    whatever its name looks like.
//! 2. **Draft** needs all three: the word `draft` in the name, a
//!    word that makes or edits one (`create`, `update`, `save`,
//!    `edit`, `compose`, `write`, `set`, ...), and no other word that
//!    acts or changes anything, in any form (`send`, `scheduled`,
//!    `publish`, `and`, ...). `create_draft` and `update_draft` are
//!    drafts; `send_draft`, `approve_draft`, `draft_order_complete`
//!    and `create_draft_and_send` are not.
//! 3. **Read** needs all of: no word that acts or changes anything
//!    anywhere in the name; the connection not saying that the tool
//!    is *not* read-only; and a positive sign, which is a lookup word
//!    in the name (`get`, `list`, `search`, `read`, `find`, `fetch`,
//!    `lookup`, `describe`, `show`, `view`, `count`, `check`,
//!    `status`, ...) or the connection saying the tool is read-only.
//!    The connection's word never outweighs the name: `create_issue`
//!    is `act` also when its connection calls it read-only. A name
//!    that joins two things (`and`, `then`, `or`) needs both signs.
//! 4. Anything else: `act`. That includes `query`, `graphql`, `sql`,
//!    `run` and `execute`: what such a tool does depends on what it
//!    is handed.
//!
//! **Names are not ours either** ([`full_tool_name`],
//! [`unusable_reason`]). The tools an ask may and may not use are
//! handed to Claude Code as two lists on its command line, by the
//! names Claude Code itself gives them (`mcp__connection__tool`, each
//! part spelled its way, [`normalize_name`]). A tool whose name
//! cannot be passed on safely, or would be spelled like another
//! tool's, is never on a list and never in the prompt: it stays in
//! the listing, marked, and cannot be ticked.
//!
//! From the kinds and the settings comes the plan for one ask
//! ([`plan_tools`]): which tools run without asking (ticked read and
//! draft tools), which raise a confirm (ticked act tools, only while
//! drafts only is off), and which the model never gets to see
//! (everything else the enabled connections are known to have).
//! Nothing in here starts a process or reads a file.

use std::collections::HashMap;

use serde::Serialize;

use crate::settings::{AGENT_TOOL_NAME_MAX_CHARS, AgentConnection, AgentSettings, ToolInfo, tool_name_is_usable};

pub use crate::ask::ToolKind;

/// Words that make or edit something: with `draft` in the name they
/// make a draft tool, anywhere else they change something.
const MAKE: &[&str] = &[
    "create", "creates", "update", "updates", "save", "saves", "edit", "edits", "compose", "write", "writes", "set",
    "sets", "make", "new", "prepare", "modify", "patch", "upsert",
];

/// Words that send something out, take something away, decide
/// something, or run something. One of them in a name makes the tool
/// `act`, whatever else the name or the connection says.
const ACT: &[&str] = &[
    // Out it goes.
    "send", "sends", "sent", "post", "posts", "reply", "replies", "respond", "forward", "notify", "email", "mail",
    "call", "dial", "broadcast", "publish", "unpublish", "share", "unshare", "invite", "submit", "dispatch", "emit",
    // Gone it is.
    "delete", "deletes", "remove", "removes", "trash", "untrash", "discard", "destroy", "purge", "wipe", "erase",
    "archive", "unarchive", "clear", "flush", "drop", "truncate", "overwrite", "kill", "terminate", "expire",
    // A decision in the person's name.
    "approve", "reject", "decline", "accept", "confirm", "acknowledge", "ack", "escalate", "resolve", "unresolve",
    "assign", "unassign", "merge", "transition", "complete", "finish", "finalize", "close", "reopen", "cancel",
    "revoke", "grant", "authorize", "sign", "rsvp", "dismiss", "snooze",
    // Money and bookings.
    "pay", "charge", "refund", "order", "buy", "purchase", "book", "transfer", "withdraw", "deposit", "checkout",
    "checkin",
    // It runs whatever it is handed.
    "execute", "exec", "eval", "run", "invoke", "trigger", "query", "graphql", "sql", "mutate", "mutation",
    "command", "shell", "bash", "script", "deploy", "apply", "migrate", "rollback", "launch", "spawn", "schedule",
    "reschedule",
    // On, off, back and forth.
    "start", "stop", "restart", "end", "pause", "resume", "abort", "retry", "rerun", "restore", "revert", "undo",
    "redo", "reset", "toggle", "enable", "disable", "activate", "deactivate", "lock", "unlock", "kick", "ban",
    "unban", "block", "unblock", "mute", "unmute", "sync", "rotate", "renew", "provision",
    // In and out of the machine.
    "upload", "import", "export", "download",
    // Small acts that still speak for the person.
    "react", "vote", "upvote", "downvote", "like", "unlike", "pin", "unpin", "star", "unstar", "follow", "unfollow",
    "subscribe", "unsubscribe", "join", "leave", "move", "moves", "rename", "copy", "mark", "unmark", "click",
    "press", "navigate", "fill",
];

/// Words that change something without sending it anywhere. A name
/// with one of them is not a lookup.
const CHANGE: &[&str] = &[
    "add", "adds", "insert", "put", "duplicate", "clone", "label", "unlabel", "tag", "untag", "flag", "unflag",
    "register", "unregister", "generate", "convert", "resize", "link", "unlink", "attach", "detach", "comment",
    "reaction", "install", "uninstall", "upgrade", "downgrade", "configure", "connect", "disconnect", "refresh",
    "replace", "append", "prepend", "restructure", "open", "login", "logout", "authenticate", "increment",
    "decrement", "adjust", "change", "rate",
];

/// Words that are a thing and not an act in the name of a draft tool
/// (`create_reply_draft`, `create_email_draft`, `draftOrderCreate`).
/// They do not keep a name from being a draft; by themselves they
/// still make a tool `act`.
const DRAFT_NOUNS: &[&str] = &["reply", "replies", "email", "mail", "order"];

/// Words that only look.
const READ: &[&str] = &[
    "get", "gets", "list", "lists", "search", "searches", "read", "reads", "find", "finds", "fetch", "fetches",
    "lookup", "describe", "show", "view", "count", "check", "status", "whoami", "retrieve", "inspect", "preview",
    "compare", "browse", "peek",
];

/// Words that join two things in one name: what comes after them may
/// be a second thing the tool does.
const JOINS: &[&str] = &["and", "then", "or", "plus"];

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

/// `word` acts or changes something, in whatever form it is written:
/// it is one of the words, or begins with one of four letters or
/// more (`scheduled`, `sending`, `publishes`).
fn acts_in_any_form(word: &str) -> bool {
    ACT.iter().chain(CHANGE).any(|known| word == *known || (known.len() >= 4 && word.starts_with(known)))
}

/// The name, written together in lower case, holds a word that acts
/// or changes something (of four letters or more): `sendmessage`.
/// For a name that gives no sign of its own that it only looks.
fn hides_an_act(name: &str) -> bool {
    let glued: String = name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect();
    ACT.iter().chain(CHANGE).chain(MAKE).any(|known| known.len() >= 4 && glued.contains(known))
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
    let joins = any_in(&words, JOINS);
    // "check in", "check out": not a look.
    let checks_in = words.windows(2).any(|pair| pair[0] == "check" && matches!(pair[1].as_str(), "in" | "out" | "off"));
    let makes = any_in(&words, MAKE);
    if makes && words.iter().any(|w| w == "draft" || w == "drafts") {
        let acts_on_it = words
            .iter()
            .filter(|w| !MAKE.contains(&w.as_str()) && !DRAFT_NOUNS.contains(&w.as_str()))
            .any(|w| acts_in_any_form(w));
        if !acts_on_it && !joins && !checks_in {
            return ToolKind::Draft;
        }
    }
    let changes = makes || any_in(&words, ACT) || any_in(&words, CHANGE) || checks_in;
    if changes {
        return ToolKind::Act;
    }
    let looks = any_in(&words, READ);
    let read = match read_only {
        // The connection itself says it is not a lookup.
        Some(false) => false,
        // Its word, and nothing in the name against it. A name that
        // joins two things needs a lookup word as well.
        Some(true) => {
            if looks {
                true
            } else {
                !joins && !hides_an_act(name)
            }
        }
        None => looks && !joins,
    };
    if read { ToolKind::Read } else { ToolKind::Act }
}

/// [`classify`] for a listed tool.
pub fn kind_of(tool: &ToolInfo) -> ToolKind {
    classify(&tool.name, tool.read_only, tool.destructive)
}

// --- names, the way Claude Code spells them ------------------------

/// A connection's or a tool's name as Claude Code spells it inside a
/// tool's full name. Established by listing made-up connections with
/// Claude Code 2.1.322 and reading the tool names of its `init` line
/// (TASK-99 notes; the table test below is what was seen):
///
/// - every UTF-16 unit of a character that is not an ASCII letter, a
///   digit, `_` or `-` becomes one `_` (`my.notes` is `my_notes`,
///   `caf\u{e9} notes` is `caf__notes`, an emoji is two);
/// - a name that begins with `claude.ai ` (a connector of the
///   person's account) then has every run of `_` made one and a `_`
///   at either end taken off (`claude.ai Team (work)` is
///   `claude_ai_Team_work`).
pub fn normalize_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
        } else {
            out.extend(std::iter::repeat_n('_', c.len_utf16()));
        }
    }
    if !name.starts_with("claude.ai ") {
        return out;
    }
    let mut collapsed = String::with_capacity(out.len());
    for c in out.chars() {
        if !(c == '_' && collapsed.ends_with('_')) {
            collapsed.push(c);
        }
    }
    let trimmed = collapsed.strip_prefix('_').unwrap_or(&collapsed);
    trimmed.strip_suffix('_').unwrap_or(trimmed).to_string()
}

/// A connection's name as it appears inside a tool's full name
/// ([`normalize_name`]).
pub fn server_key(connection: &str) -> String {
    normalize_name(connection)
}

/// The name the model and the CLI use for the tool `tool` of the
/// connection `connection`: `mcp__connection__tool`, each part
/// spelled the CLI's way ([`normalize_name`]).
pub fn full_tool_name(connection: &str, tool: &str) -> String {
    format!("mcp__{}__{}", normalize_name(connection), normalize_name(tool))
}

/// Why a listed tool cannot be used at all.
#[non_exhaustive]
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ToolProblem {
    /// Its name has a character that cannot be passed on
    /// ([`tool_name_is_usable`]), or is empty or too long.
    Name,
    /// With its connection's name in front, its name is longer than
    /// Claude Code keeps.
    TooLong,
    /// Claude Code spells its name exactly like another tool's of the
    /// same connection (`a.b` and `a_b`), so the two cannot be told
    /// apart.
    Twin,
}

impl ToolProblem {
    /// Stable id, same as the serialized form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::TooLong => "too-long",
            Self::Twin => "twin",
        }
    }

    /// One plain sentence for the person.
    pub fn sentence(self) -> &'static str {
        match self {
            Self::Name => "Its name has characters that cannot be passed on safely, so it cannot be used.",
            Self::TooLong => "Its name is too long for Claude Code, so it cannot be used.",
            Self::Twin => "Another tool of this connection has a name that is spelled the same way, so neither can be used.",
        }
    }
}

/// Why the tool `tool` of `connection` cannot be used, or `None`
/// when it can. A tool with a problem is never ticked, never on a
/// list for the CLI and never in the prompt.
pub fn unusable_reason(connection: &AgentConnection, tool: &ToolInfo) -> Option<ToolProblem> {
    if !tool_name_is_usable(&tool.name) {
        return Some(ToolProblem::Name);
    }
    if full_tool_name(&connection.name, &tool.name).chars().count() > AGENT_TOOL_NAME_MAX_CHARS {
        return Some(ToolProblem::TooLong);
    }
    let spelled = normalize_name(&tool.name);
    let twin = connection
        .tools
        .iter()
        .any(|other| other.name != tool.name && tool_name_is_usable(&other.name) && normalize_name(&other.name) == spelled);
    twin.then_some(ToolProblem::Twin)
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

/// Every tool of the enabled connections that can be used at all,
/// with its full name: no tool with a problem of its own
/// ([`unusable_reason`]), and no tool whose full name is also another
/// tool's (the connection `a` with a tool `b__c` and the connection
/// `a__b` with a tool `c` are both `mcp__a__b__c`: neither is
/// passed on).
fn usable_tools(settings: &AgentSettings) -> Vec<(&AgentConnection, &ToolInfo, String)> {
    let mut all = Vec::new();
    for connection in enabled_connections(settings) {
        for tool in &connection.tools {
            if unusable_reason(connection, tool).is_none() {
                all.push((connection, tool, full_tool_name(&connection.name, &tool.name)));
            }
        }
    }
    let mut times: HashMap<String, usize> = HashMap::new();
    for (_, _, full) in &all {
        *times.entry(full.clone()).or_default() += 1;
    }
    all.retain(|(_, _, full)| times.get(full) == Some(&1));
    all
}

/// The plan for an ask with these settings. Does not look at
/// [`AgentSettings::enabled`]: that is for the caller, which may be
/// `openwhisper agent run`.
///
/// Only a tool the connection listed can be allowed: a ticked name
/// that is not in [`AgentConnection::tools`] is a tool nobody has
/// seen, so it is left out (and the CLI asks about it when the model
/// finds it anyway, which is then answered with a no). The same goes
/// for a tool that cannot be used ([`unusable_reason`]): it is on no
/// list at all.
pub fn plan_tools(settings: &AgentSettings) -> ToolPlan {
    let mut plan = ToolPlan::default();
    for (connection, tool, full) in usable_tools(settings) {
        let ticked = connection.allows(&tool.name);
        match (ticked, kind_of(tool)) {
            (true, ToolKind::Read | ToolKind::Draft) => plan.allowed.push(full),
            (true, ToolKind::Act) if !settings.drafts_only => plan.confirm.push(full),
            _ => plan.disallowed.push(full),
        }
    }
    plan
}

/// The tools of [`plan_tools`] the model can call, with what is
/// known about each, in the order of the settings.
pub fn planned_tools(settings: &AgentSettings) -> Vec<PlannedTool> {
    let mut tools = Vec::new();
    for (connection, tool, full_name) in usable_tools(settings) {
        let kind = kind_of(tool);
        let ticked = connection.allows(&tool.name);
        let confirm = kind == ToolKind::Act;
        if !ticked || (confirm && settings.drafts_only) {
            continue;
        }
        tools.push(PlannedTool {
            full_name,
            connection: connection.name.clone(),
            tool: tool.name.clone(),
            title: if tool.title.trim().is_empty() { tool.name.clone() } else { tool.title.clone() },
            kind,
            confirm,
        });
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
    /// issue tracker, documents, a design tool, a calendar, a CMS,
    /// analytics, a file server, and a few that belong nowhere.
    /// `None, None` = the connection says nothing about the tool.
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
            ("discord_read_messages", None, None, Read),
            ("discord_send_message", None, None, Act),
            ("get_channel_history", None, None, Read),
            ("list_channels", None, None, Read),
            ("join_channel", None, None, Act),
            ("create_channel", None, None, Act),
            ("get_user_profile", None, None, Read),
            ("set_status", None, None, Act),
            ("pin_message", None, None, Act),
            ("search_users", None, None, Read),
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
            ("gmail_create_draft", None, None, Draft),
            ("create_email_draft", None, None, Draft),
            ("save_draft", None, None, Draft),
            ("edit_draft", None, None, Draft),
            ("compose_draft", None, None, Draft),
            ("set_draft_body", None, None, Draft),
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
            ("gmail_modify_labels", None, None, Act),
            ("list_attachments", None, None, Read),
            ("download_attachment", None, None, Act),
            ("create_filter", None, None, Act),
            // "draft" as the verb: nothing says it only prepares.
            ("draft_email", None, None, Act),
            ("draft_reply", None, None, Act),
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
            ("reopen_issue", None, None, Act),
            ("assign_issue", None, None, Act),
            ("add_issue_comment", None, None, Act),
            ("get_pull_request", None, None, Read),
            ("list_pull_requests", None, None, Read),
            ("create_pull_request", None, None, Act),
            ("get_file_contents", None, None, Read),
            ("create_or_update_file", None, None, Act),
            ("push_files", None, None, Act),
            ("fork_repository", None, None, Act),
            ("list_commits", None, None, Read),
            ("search_code", None, None, Read),
            ("linear_list_issues", None, None, Read),
            ("linear_create_comment", None, None, Act),
            ("resolve_thread", None, None, Act),
            // --- documents
            ("notion-search", None, None, Read),
            ("notion-fetch", None, None, Read),
            ("notion-get-comments", None, None, Read),
            // A query does what it is handed.
            ("notion-query-data-sources", None, None, Act),
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
            ("retrieve_page", None, None, Read),
            ("append_block_children", None, None, Act),
            ("export_document", None, None, Act),
            ("share_document", None, None, Act),
            ("archive_page", None, None, Act),
            ("restore_page", None, None, Act),
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
            ("list_events", None, None, Read),
            ("get_freebusy", None, None, Read),
            ("find_free_time", None, None, Read),
            ("outlook_create_event", None, None, Act),
            ("outlook_update_event", None, None, Act),
            ("outlook_delete_event", None, None, Act),
            ("outlook_respond_to_event", None, None, Act),
            ("notion-calendar-cancel-events", None, None, Act),
            ("quick_add_event", None, None, Act),
            ("move_event", None, None, Act),
            ("respond_to_invite", None, None, Act),
            // --- a CMS
            ("findPages", None, None, Read),
            ("describeFields", None, None, Read),
            ("compareVersions", None, None, Read),
            ("findPreviewUrl", None, None, Read),
            ("createPages", None, None, Act),
            ("updatePages", None, None, Act),
            ("saveSettings", None, None, Act),
            ("patchBlock", None, None, Act),
            ("restructureBlocks", None, None, Act),
            ("uploadMedia", None, None, Act),
            ("publish_entry", None, None, Act),
            ("unpublish_entry", None, None, Act),
            ("list_content_types", None, None, Read),
            ("create_draft_entry", None, None, Draft),
            // --- analytics and databases
            ("get_report", None, None, Read),
            ("list_dashboards", None, None, Read),
            ("get_insights", None, None, Read),
            ("describe_table", None, None, Read),
            ("list_tables", None, None, Read),
            ("run_report", None, None, Act),
            ("run_query", None, None, Act),
            ("read_query", None, None, Act),
            ("execute_sql", None, None, Act),
            ("query_metrics", None, None, Act),
            ("query", None, None, Act),
            ("query", Some(true), None, Act),
            ("graphql_query", None, None, Act),
            ("graphql_query", Some(true), None, Act),
            ("sql", Some(true), None, Act),
            ("export_data", None, None, Act),
            ("create_annotation", None, None, Act),
            // --- a file server
            ("read_file", None, None, Read),
            ("read_multiple_files", None, None, Read),
            ("list_directory", None, None, Read),
            ("search_files", None, None, Read),
            ("get_file_info", None, None, Read),
            ("list_allowed_directories", None, None, Read),
            ("directory_tree", Some(true), None, Read),
            ("directory_tree", None, None, Act),
            ("write_file", None, None, Act),
            ("edit_file", None, None, Act),
            ("create_directory", None, None, Act),
            ("move_file", None, None, Act),
            ("copy_file", None, None, Act),
            ("rename_file", None, None, Act),
            ("delete_file", None, None, Act),
            ("download_file", None, None, Act),
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
            // The connection's word never outweighs the name.
            ("create_issue", Some(true), None, Act),
            ("delete_everything", Some(true), None, Act),
            ("sendmessage", Some(true), None, Act),
            ("postupdate", Some(true), None, Act),
            // And a lookup the connection says is not one is not one.
            ("get_thread", Some(false), None, Act),
            ("list_drafts", Some(false), None, Act),
            // Unknown is act.
            ("frobnicate", None, None, Act),
            ("do_the_thing", None, None, Act),
            ("", None, None, Act),
            ("___", None, None, Act),
            // A lookup that also does something is not a lookup.
            ("find_and_replace", None, None, Act),
            ("get_or_create_page", None, None, Act),
            ("read_and_mark_done", None, None, Act),
            ("search_and_destroy", None, None, Act),
            ("read_then_acknowledge_alert", None, None, Act),
            ("read_then_acknowledge_alert", Some(true), None, Act),
            ("find_escalate_ticket", None, None, Act),
            ("get_accept_invitation", None, None, Act),
            ("lookup_confirm_payment", None, None, Act),
            ("lookup_confirm_payment", Some(true), None, Act),
            ("check_in_guest", None, None, Act),
            ("get_and_frobnicate", None, None, Act),
            ("get_and_frobnicate", Some(true), None, Read),
            ("frobnicate_and_tell", Some(true), None, Act),
            // A draft that is only looked at, and one that is acted on.
            ("list_message_drafts", None, None, Read),
            ("publish_draft", None, None, Act),
            ("publishDraft", None, None, Act),
            ("restoreVersionAsDraft", None, None, Act),
            ("discard_draft", None, None, Act),
            // "draft" in the name is not enough: without a word that
            // makes one, or with a word that acts on one, it is act.
            ("draft_order_complete", None, None, Act),
            ("draftOrderComplete", None, None, Act),
            ("draftOrderInvoiceSend", None, None, Act),
            ("draftOrderCreate", None, None, Draft),
            ("draftOrderUpdate", None, None, Draft),
            ("mark_draft_ready_for_review", None, None, Act),
            ("finalize_draft_invoice", None, None, Act),
            ("share_draft", None, None, Act),
            ("approve_draft", None, None, Act),
            ("purge_drafts", None, None, Act),
            ("merge_draft_pr", None, None, Act),
            ("release_draft", None, None, Act),
            ("schedule_draft", None, None, Act),
            ("create_draft_and_send", None, None, Act),
            ("update_draft_and_schedule", None, None, Act),
            ("create_scheduled_draft", None, None, Act),
            ("create_draft_sending_now", None, None, Act),
            ("save_draft_then_publish", None, None, Act),
            ("create_draft", Some(true), None, Draft),
            ("create_draft", Some(false), None, Draft),
            ("create_draft", None, Some(true), Act),
        ];
        for (name, read_only, destructive, expected) in table {
            assert_eq!(
                classify(name, *read_only, *destructive),
                *expected,
                "{name} (read_only {read_only:?}, destructive {destructive:?})"
            );
        }
        assert!(table.len() > 200, "{}", table.len());
    }

    /// What Claude Code 2.1.322 printed in its `init` line for these
    /// names, each given to it as a made-up connection with a test
    /// fixture behind it (and, for the tools, a fixture that listed
    /// tools with these names). The rule in [`normalize_name`] is
    /// read off this table, not the other way round.
    #[test]
    fn names_are_spelled_the_way_claude_code_was_seen_to_spell_them() {
        let connections: &[(&str, &str)] = &[
            ("teamchat", "teamchat"),
            ("claude.ai Team (work)", "claude_ai_Team_work"),
            ("my.notes.v2", "my_notes_v2"),
            ("team chat", "team_chat"),
            ("team-chat", "team-chat"),
            ("caf\u{e9} notes", "caf__notes"),
            ("a__b", "a__b"),
            ("a___b", "a___b"),
            ("plugin:helper:notes", "plugin_helper_notes"),
            ("UPPER lower", "UPPER_lower"),
            ("x (1) [2] {3}", "x__1___2___3_"),
            ("tab\tname", "tab_name"),
            ("end.", "end_"),
            ("  lead", "__lead"),
            ("\u{65e5}\u{672c}\u{8a9e}", "___"),
            ("a/b\\c", "a_b_c"),
            ("at@sign+plus", "at_sign_plus"),
            ("under_score", "under_score"),
            ("dots..twice", "dots__twice"),
            ("trailing_", "trailing_"),
            // One character, two UTF-16 units, two underscores.
            ("emoji\u{1F600}srv", "emoji__srv"),
            // A connector of the account: runs made one, ends taken off.
            ("claude.ai  Double  space", "claude_ai_Double_space"),
            ("claude.ai_x", "claude_ai_x"),
            ("claude.ai -x-", "claude_ai_-x-"),
            ("claude.ai a__b", "claude_ai_a_b"),
            ("claude.ai caf\u{e9} (x)", "claude_ai_caf_x"),
            // Only in exactly that spelling.
            ("Claude.ai Upper", "Claude_ai_Upper"),
        ];
        for (name, spelled) in connections {
            assert_eq!(server_key(name), *spelled, "{name:?}");
            assert_eq!(full_tool_name(name, "create_draft"), format!("mcp__{spelled}__create_draft"), "{name:?}");
        }
        let tools: &[(&str, &str)] = &[
            ("read_thread", "mcp__odd__read_thread"),
            ("dots.name", "mcp__odd__dots_name"),
            ("dash-name", "mcp__odd__dash-name"),
            ("under_score", "mcp__odd__under_score"),
            ("UPPER", "mcp__odd__UPPER"),
            ("a__b", "mcp__odd__a__b"),
            ("trail_", "mcp__odd__trail_"),
            // Names this app never passes on; the CLI spells them so.
            ("list,mcp__teamchat", "mcp__odd__list_mcp__teamchat"),
            ("read thread", "mcp__odd__read_thread"),
            ("get*", "mcp__odd__get_"),
            ("a(b)", "mcp__odd__a_b_"),
            ("caf\u{e9}", "mcp__odd__caf_"),
            ("emoji\u{1F600}x", "mcp__odd__emoji__x"),
            ("tab\tname", "mcp__odd__tab_name"),
            ("slash/name", "mcp__odd__slash_name"),
            ("colon:name", "mcp__odd__colon_name"),
            (".lead", "mcp__odd___lead"),
        ];
        for (name, full) in tools {
            assert_eq!(full_tool_name("odd", name), *full, "{name:?}");
        }
        // Whatever goes in, what comes out is safe in a list.
        for (name, _) in connections.iter().chain(tools) {
            let full = full_tool_name(name, name);
            assert!(full.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'), "{full}");
        }
    }

    #[test]
    fn a_tool_that_cannot_be_passed_on_is_on_no_list_and_not_in_the_plan() {
        let comma = "list,mcp__teamchat";
        let long = format!("list_{}", "x".repeat(110));
        let notes = AgentConnection {
            name: "notes".into(),
            enabled: true,
            // Ticks as a file from before this rule could have them.
            allowed_tools: vec![comma.into(), "list".into(), "a.b".into(), "a_b".into(), long.clone(), "get*".into()],
            tools: vec![
                ToolInfo { read_only: Some(true), ..ToolInfo::new(comma) },
                ToolInfo::new("list"),
                ToolInfo::new("get*"),
                // Two names the CLI spells the same.
                ToolInfo { read_only: Some(true), ..ToolInfo::new("a.b") },
                ToolInfo::new("a_b"),
                ToolInfo::new("read thread"),
            ],
            ..AgentConnection::default()
        };
        // With a long connection name in front, a name is too long.
        let long_name = AgentConnection {
            name: "a-connection-with-a-rather-long-name".into(),
            enabled: true,
            allowed_tools: vec![long.clone(), "list".into()],
            tools: vec![ToolInfo::new(&long), ToolInfo::new("list")],
            ..AgentConnection::default()
        };
        let teamchat = connection("teamchat", true, &["read_thread"]);
        let reason = |c: &AgentConnection, name: &str| unusable_reason(c, c.tool(name).unwrap());
        assert_eq!(reason(&notes, comma), Some(ToolProblem::Name));
        assert_eq!(reason(&notes, "get*"), Some(ToolProblem::Name));
        assert_eq!(reason(&notes, "read thread"), Some(ToolProblem::Name));
        assert_eq!(reason(&notes, "a.b"), Some(ToolProblem::Twin));
        assert_eq!(reason(&notes, "a_b"), Some(ToolProblem::Twin));
        assert_eq!(reason(&notes, "list"), None);
        assert_eq!(reason(&long_name, &long), Some(ToolProblem::TooLong));
        assert_eq!(reason(&long_name, "list"), None);
        for problem in [ToolProblem::Name, ToolProblem::TooLong, ToolProblem::Twin] {
            assert_eq!(serde_json::to_value(problem).unwrap(), problem.as_str());
            assert!(problem.sentence().ends_with('.'));
        }

        let settings = AgentSettings {
            drafts_only: false,
            connections: vec![notes, long_name, teamchat],
            ..AgentSettings::default()
        };
        let plan = plan_tools(&settings);
        // The comma name, which would have allowed every tool of
        // teamchat, is nowhere; neither is anything else unusable.
        assert_eq!(
            plan.allowed,
            ["mcp__notes__list", "mcp__a-connection-with-a-rather-long-name__list", "mcp__teamchat__read_thread"]
        );
        assert!(plan.confirm.is_empty());
        assert_eq!(
            plan.disallowed,
            [
                "mcp__teamchat__search_messages",
                "mcp__teamchat__create_draft",
                "mcp__teamchat__send_message",
                "mcp__teamchat__delete_message"
            ]
        );
        for entry in plan.allowed.iter().chain(&plan.confirm).chain(&plan.disallowed) {
            assert!(entry.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'), "{entry}");
            assert!(entry.chars().count() <= AGENT_TOOL_NAME_MAX_CHARS, "{entry}");
        }
        let planned: Vec<String> = planned_tools(&settings).into_iter().map(|t| t.full_name).collect();
        assert_eq!(planned, plan.allowed);
    }

    #[test]
    fn two_tools_with_one_full_name_are_both_left_out() {
        // `mcp__a__b__read` twice: the tool `b__read` of `a`, and the
        // tool `read` of `a__b`.
        let one = AgentConnection {
            name: "a".into(),
            enabled: true,
            allowed_tools: vec!["b__read".into(), "list".into()],
            tools: vec![ToolInfo::new("b__read"), ToolInfo::new("list")],
            ..AgentConnection::default()
        };
        let other = AgentConnection {
            name: "a__b".into(),
            enabled: true,
            allowed_tools: vec!["read".into()],
            tools: vec![ToolInfo::new("read"), ToolInfo::new("send")],
            ..AgentConnection::default()
        };
        let settings = AgentSettings { connections: vec![one, other], ..AgentSettings::default() };
        let plan = plan_tools(&settings);
        assert_eq!(plan.allowed, ["mcp__a__list"]);
        assert_eq!(plan.disallowed, ["mcp__a__b__send"]);
        assert_eq!(planned_tools(&settings).len(), 1);
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
