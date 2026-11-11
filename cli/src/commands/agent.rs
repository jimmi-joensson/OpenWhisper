//! `openwhisper agent <subcommand>`: the headless surface of Agent
//! (TASK-99), an ask that can use the tool connections you already
//! have in your own Claude Code. Same core functions the desktop app
//! calls, same `settings.json`.
//!
//! Subcommands:
//! - `status`: the switch, drafts only, the connections with their
//!   tools and what kind each tool is, the app table, and whether an
//!   ask would have tools right now (Ask on, the Claude backend, a
//!   ticked tool).
//! - `config`: print the `agent` settings block, or change the
//!   switch, drafts only, the confirm timeout and the turn limit.
//! - `connections`: the connections in the settings. `--refresh`
//!   asks your Claude Code which connections it has (names, kinds,
//!   states and tools; no tool is called and the model is not asked
//!   anything) and brings the settings up to date. `add-custom`,
//!   `remove`, `enable` and `disable` edit them.
//! - `tools NAME`: the tools of one connection, the kind of each
//!   (read, draft, act) and whether it is ticked.
//! - `allow NAME TOOL...` / `disallow NAME TOOL...`: tick and untick.
//! - `apps list|set|unset|reset`: which app belongs to which
//!   connection. With the app in front mapped to an enabled
//!   connection, an ask reads through that connection and takes no
//!   screenshot.
//! - `run [TEXT]`: one ask with tools. The answer streams to stdout;
//!   every tool call is printed on stderr as it happens. A tool that
//!   sends, posts, deletes or changes something asks first:
//!   `--confirm ask` (the default at a terminal) shows the tool and
//!   its input and waits for y or n, `--confirm allow` and `--confirm
//!   deny` answer for you (deny is the default when nobody can be
//!   asked). `--json` prints one record at the end with the steps,
//!   the confirms and how each was answered.
//! - `eval`: the eval set, live against the real backend with a test
//!   fixture as the only connection; `--offline` only proves the
//!   checks.
//!
//! `run` works whether or not Agent is switched on in the settings:
//! invoking it is the opt-in, like `ask run`. Which connections it
//! may use, which tools are ticked and whether drafts only is on are
//! always the settings': a run has exactly the tools an ask in the
//! app would have.
//!
//! `fake-server` (hidden) is a test fixture and nothing else: a
//! made-up chat tool as a connection, whose tools do nothing but
//! write a line to a log (`openwhisper_core::agent::fixture`). The
//! tests and the eval set use it in place of a real connection.

use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Subcommand};
use openwhisper_core::agent::{self, AgentRun, FrontApp, connections, eval, fixture, policy};
use openwhisper_core::ask::{
    AskConfirm, AskEvent, AskInput, AskPlan, CancelFlag, ConfirmDecision, ConfirmOutcome, ScreenState, StepState,
    answer_confirm,
};
use openwhisper_core::settings::{
    self, AgentConnection, AgentConnectionKind, AgentSettings, AppConnection, CustomServer, RefineBackendKind,
};
use openwhisper_core::{refine, transcript};

use super::ask::{self as ask_cmd, Overrides, parse_bool};

#[derive(Args, Debug)]
pub struct AgentArgs {
    #[command(subcommand)]
    pub command: AgentCommand,
}

#[derive(Subcommand, Debug)]
pub enum AgentCommand {
    /// Show the switch, the connections, their tools and the app table.
    Status,
    /// Print the Agent settings, or change them with flags.
    Config(ConfigArgs),
    /// List the connections, refresh them, or edit one.
    Connections(ConnectionsArgs),
    /// Show the tools of one connection and which are ticked.
    Tools { name: String },
    /// Tick tools of a connection: an ask may use them.
    Allow { name: String, #[arg(required = true)] tools: Vec<String> },
    /// Untick tools of a connection.
    Disallow { name: String, #[arg(required = true)] tools: Vec<String> },
    /// Which app belongs to which connection.
    Apps(AppsArgs),
    /// Ask a question (argument or stdin) with the tools the settings allow.
    Run(RunArgs),
    /// Run the eval set against the real backend and a test fixture.
    Eval(EvalArgs),
    /// Test fixture: a made-up connection whose tools only write to a
    /// log. Not a real connection; used by the tests and the eval set.
    #[command(hide = true)]
    FakeServer(FakeServerArgs),
}

#[derive(Args, Debug)]
pub struct ConfigArgs {
    /// Switch Agent on or off.
    #[arg(long, value_parser = parse_bool, value_name = "BOOL")]
    enabled: Option<bool>,
    /// Drafts only: nothing that sends, posts, deletes or changes
    /// anything can run, ticked or not.
    #[arg(long, value_parser = parse_bool, value_name = "BOOL")]
    drafts_only: Option<bool>,
    /// How long a confirm waits for you, in seconds (10 to 600). No
    /// answer in that time is a no.
    #[arg(long, value_name = "N")]
    confirm_timeout_s: Option<u64>,
    /// How many times the model may go round its tools in one ask
    /// (1 to 40).
    #[arg(long, value_name = "N")]
    max_turns: Option<u32>,
}

#[derive(Args, Debug)]
pub struct ConnectionsArgs {
    /// Ask your Claude Code which connections it has, and your own
    /// entries what tools they have. No tool is called.
    #[arg(long)]
    refresh: bool,
    /// Path to the claude binary, for --refresh.
    #[arg(long)]
    claude_path: Option<String>,
    #[command(subcommand)]
    command: Option<ConnectionsCommand>,
}

#[derive(Subcommand, Debug)]
pub enum ConnectionsCommand {
    /// Add a connection of your own: a command to start, or a URL.
    AddCustom {
        name: String,
        /// The command that starts the server (it talks over stdin
        /// and stdout).
        #[arg(long, conflicts_with = "url")]
        command: Option<String>,
        /// An argument for the command. Repeat for more.
        #[arg(long = "arg", value_name = "A", allow_hyphen_values = true)]
        args: Vec<String>,
        /// A variable for the command's environment, as NAME=VALUE.
        /// Repeat for more. Stored in settings.json as you give it.
        #[arg(long = "env", value_name = "NAME=VALUE")]
        env: Vec<String>,
        /// The URL of a server reached over HTTP.
        #[arg(long)]
        url: Option<String>,
    },
    /// Remove a connection from the settings.
    Remove { name: String },
    /// Let asks use a connection.
    Enable { name: String },
    /// Stop asks from using a connection.
    Disable { name: String },
}

#[derive(Args, Debug)]
pub struct AppsArgs {
    #[command(subcommand)]
    command: AppsCommand,
}

#[derive(Subcommand, Debug)]
pub enum AppsCommand {
    /// The app table in force.
    List,
    /// Map an app (its id as `openwhisper focus` prints it) to a
    /// connection.
    Set {
        app_id: String,
        connection: String,
        /// Name to show for the app.
        #[arg(long)]
        name: Option<String>,
    },
    /// Take an app out of the table.
    Unset { app_id: String },
    /// Back to the built-in table.
    Reset,
}

#[derive(Args, Debug)]
pub struct RunArgs {
    /// The question. Read from stdin when omitted.
    text: Option<String>,
    /// What to answer when a tool asks first: ask (y/n at the
    /// terminal), allow, or deny. Default: ask at a terminal, deny
    /// otherwise.
    #[arg(long, value_name = "ask|allow|deny")]
    confirm: Option<String>,
    /// The id of the app to treat as the one in front (as
    /// `openwhisper focus` prints it). When it is mapped to an
    /// enabled connection, the ask is pointed at that connection.
    #[arg(long, value_name = "APP_ID")]
    front_app: Option<String>,
    /// Send a screenshot of the display you are working on with the
    /// question, whatever the Vision trigger says.
    #[arg(long, conflicts_with = "vision")]
    screen: bool,
    /// Let Vision decide for this run, with this trigger: off,
    /// phrases, smart, always, or "saved". Without this and without
    /// --screen, a run never looks at the screen.
    #[arg(long, value_name = "TRIGGER")]
    vision: Option<String>,
    /// Let the model search the web for this question.
    #[arg(long)]
    web_search: bool,
    /// Print the answer once it is complete instead of as it arrives.
    #[arg(long)]
    no_stream: bool,
    /// Send the text as given, without the rule pass a spoken
    /// question gets first.
    #[arg(long)]
    skip_rules: bool,
    /// Also keep the question and the answer in the transcript
    /// history, when history is switched on.
    #[arg(long)]
    history: bool,
    #[command(flatten)]
    overrides: Overrides,
}

#[derive(Args, Debug)]
pub struct EvalArgs {
    /// Run only cases whose name starts with this text.
    #[arg(long, value_name = "PREFIX")]
    only: Option<String>,
    /// Do not ask a model: only prove that the checks accept a right
    /// outcome and reject every mistake.
    #[arg(long)]
    offline: bool,
    #[command(flatten)]
    overrides: Overrides,
}

#[derive(Args, Debug)]
pub struct FakeServerArgs {
    /// Where every call is written, one JSON line each.
    #[arg(long, value_name = "FILE")]
    log: PathBuf,
    /// Which canned content the read returns.
    #[arg(long, value_name = "NAME", default_value = "plain")]
    scenario: String,
}

pub fn run(args: AgentArgs, json: bool) -> Result<()> {
    match args.command {
        AgentCommand::Status => status(json),
        AgentCommand::Config(a) => config(a, json),
        AgentCommand::Connections(a) => connections_command(a, json),
        AgentCommand::Tools { name } => tools(&name, json),
        AgentCommand::Allow { name, tools } => tick(&name, &tools, true, json),
        AgentCommand::Disallow { name, tools } => tick(&name, &tools, false, json),
        AgentCommand::Apps(a) => apps(a, json),
        AgentCommand::Run(a) => run_question(a, json),
        AgentCommand::Eval(a) => run_eval(a, json),
        AgentCommand::FakeServer(a) => fake_server(a),
    }
}

fn settings_path() -> Result<PathBuf> {
    Ok(super::config_dir()?.join("settings.json"))
}

fn load() -> Result<(PathBuf, AgentSettings)> {
    let path = settings_path()?;
    let settings = settings::load_agent_settings(&path);
    Ok((path, settings))
}

fn save(path: &std::path::Path, settings: AgentSettings) -> Result<AgentSettings> {
    settings::save_agent_settings(path, settings).map_err(|e| anyhow!(e))
}

fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

fn find<'a>(settings: &'a mut AgentSettings, name: &str) -> Result<&'a mut AgentConnection> {
    let known: Vec<String> = settings.connections.iter().map(|c| c.name.clone()).collect();
    settings.connection_mut(name).ok_or_else(|| {
        anyhow!(
            "no connection called \"{name}\". {}",
            if known.is_empty() {
                "There are none yet: run `openwhisper agent connections --refresh`.".to_string()
            } else {
                format!("The connections are: {}", known.join(", "))
            }
        )
    })
}

/// One connection for `status`, `connections` and `tools`.
fn connection_json(connection: &AgentConnection) -> serde_json::Value {
    let tools: Vec<serde_json::Value> = connection
        .tools
        .iter()
        .map(|tool| {
            serde_json::json!({
                "name": tool.name,
                "title": tool.title,
                "kind": policy::kind_of(tool).as_str(),
                "allowed": connection.allows(&tool.name),
                "read_only": tool.read_only,
                "destructive": tool.destructive,
            })
        })
        .collect();
    serde_json::json!({
        "name": connection.name,
        "kind": connection.kind.as_str(),
        "enabled": connection.enabled,
        "allowed": connection.tools.iter().filter(|t| connection.allows(&t.name)).count(),
        // A connection of your Claude Code can only be used after it
        // was looked up in this process.
        "listed": connection.kind == AgentConnectionKind::Custom || connections::is_listed(&connection.name),
        "tools": tools,
    })
}

fn print_connection_line(connection: &AgentConnection) {
    let ticked = connection.tools.iter().filter(|t| connection.allows(&t.name)).count();
    println!(
        "  {:<34} {:<7} {:<4} {} of {} tools ticked",
        connection.name,
        connection.kind.as_str(),
        on_off(connection.enabled),
        ticked,
        connection.tools.len()
    );
}

fn status(json: bool) -> Result<()> {
    let (path, settings) = load()?;
    let ask = settings::load_ask_settings(&path);
    let refine = settings::load_refine_settings(&path);
    let (ready, why) = agent::readiness(&settings, ask.enabled, refine.backend);
    let table = settings.effective_app_connections();
    if json {
        let value = serde_json::json!({
            "settings": settings,
            "ready": ready,
            "message": why,
            "ask_enabled": ask.enabled,
            "backend": refine.backend.as_str(),
            "connections": settings.connections.iter().map(connection_json).collect::<Vec<_>>(),
            "app_connections": table,
            "app_connections_builtin": settings.app_connections.is_none(),
            "settings_path": path.display().to_string(),
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    println!("enabled           {}", settings.enabled);
    println!("drafts only       {}", on_off(settings.drafts_only));
    println!("ask               {}", on_off(ask.enabled));
    println!(
        "backend           {}{}",
        refine.backend.as_str(),
        if refine.backend == RefineBackendKind::OpenaiCompatible { " (cannot use connections)" } else { "" }
    );
    println!("state             {}", if ready { "ready" } else { "not ready" });
    println!("message           {why}");
    println!("confirm timeout   {} s", settings.confirm_timeout_s);
    println!("max turns         {}", settings.max_turns);
    println!("connections       {}", settings.connections.len());
    for connection in &settings.connections {
        print_connection_line(connection);
        for tool in &connection.tools {
            let kind = policy::kind_of(tool);
            let note = match (connection.allows(&tool.name), kind) {
                (false, _) => "",
                (true, agent::ToolKind::Act) if settings.drafts_only => "ticked, cannot run (drafts only)",
                (true, agent::ToolKind::Act) => "ticked, asks first",
                (true, _) => "ticked",
            };
            println!("      {:<5} {:<40} {}", kind.as_str(), tool.name, note);
        }
    }
    println!(
        "apps              {} ({})",
        table.len(),
        if settings.app_connections.is_none() { "the built-in table" } else { "your table" }
    );
    for entry in &table {
        let reaches = settings.connection_for_app(&entry.app_id).map(|(_, c)| c.name.clone());
        println!(
            "  {:<34} {:<16} {}",
            entry.app_id,
            entry.connection,
            reaches.map_or("(no enabled connection of that name)".to_string(), |name| format!("-> {name}"))
        );
    }
    Ok(())
}

fn config(args: ConfigArgs, json: bool) -> Result<()> {
    let (path, mut settings) = load()?;
    let changes = args.enabled.is_some()
        || args.drafts_only.is_some()
        || args.confirm_timeout_s.is_some()
        || args.max_turns.is_some();
    if changes {
        if let Some(enabled) = args.enabled {
            settings.enabled = enabled;
        }
        if let Some(drafts_only) = args.drafts_only {
            settings.drafts_only = drafts_only;
        }
        if let Some(seconds) = args.confirm_timeout_s {
            settings.confirm_timeout_s = seconds;
        }
        if let Some(turns) = args.max_turns {
            settings.max_turns = turns;
        }
        settings = save(&path, settings)?;
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&settings)?);
        return Ok(());
    }
    println!("enabled           {}", settings.enabled);
    println!("drafts_only       {}", settings.drafts_only);
    println!("confirm_timeout_s {}", settings.confirm_timeout_s);
    println!("max_turns         {}", settings.max_turns);
    println!("connections       {}", settings.connections.len());
    if changes {
        eprintln!("saved to {}", path.display());
        if settings.enabled && !settings.drafts_only {
            eprintln!("drafts only is off: a ticked tool that sends, posts, deletes or changes something asks you first, every time");
        }
    }
    Ok(())
}

fn connections_command(args: ConnectionsArgs, json: bool) -> Result<()> {
    let (path, mut settings) = load()?;
    match args.command {
        Some(ConnectionsCommand::AddCustom { name, command, args: arguments, env, url }) => {
            if settings.connection(&name).is_some() {
                bail!("there is already a connection called \"{name}\"");
            }
            let mut variables = std::collections::BTreeMap::new();
            for pair in env {
                let (key, value) = pair.split_once('=').ok_or_else(|| anyhow!("--env takes NAME=VALUE, got \"{pair}\""))?;
                variables.insert(key.trim().to_string(), value.to_string());
            }
            let custom = CustomServer {
                command: command.unwrap_or_default(),
                args: arguments,
                env: variables,
                url: url.unwrap_or_default(),
            };
            if !custom.is_usable() {
                bail!("give the connection a --command or a --url");
            }
            settings.connections.push(AgentConnection {
                name: name.clone(),
                kind: AgentConnectionKind::Custom,
                custom: Some(custom),
                ..AgentConnection::default()
            });
            let saved = save(&path, settings)?;
            let added = saved.connection(&name).ok_or_else(|| anyhow!("the connection could not be saved"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connection_json(added))?);
            } else {
                println!("added {} (switched off, no tool ticked)", added.name);
                eprintln!("next: `openwhisper agent connections --refresh` to see its tools, then `agent allow` and `agent connections enable`");
            }
            return Ok(());
        }
        Some(ConnectionsCommand::Remove { name }) => {
            let removed = find(&mut settings, &name)?.name.clone();
            settings.connections.retain(|c| c.name != removed);
            save(&path, settings)?;
            if json {
                println!("{}", serde_json::json!({ "removed": removed }));
            } else {
                println!("removed {removed}");
            }
            return Ok(());
        }
        Some(ConnectionsCommand::Enable { name }) => return switch(&path, settings, &name, true, json),
        Some(ConnectionsCommand::Disable { name }) => return switch(&path, settings, &name, false, json),
        None => {}
    }

    if args.refresh {
        let mut refine = settings::load_refine_settings(&path);
        if let Some(claude) = args.claude_path {
            refine.claude_cli.path = Some(claude);
        }
        if !json {
            eprintln!("asking Claude Code which connections it has (no tool is called, the model is not asked anything)");
        }
        let refreshed = connections::refresh(&path, &refine.claude_cli, connections::LIST_WAIT).map_err(|e| anyhow!("{e}"))?;
        let total: usize = refreshed.listing.iter().map(|c| c.tools.len()).sum();
        if json {
            let listed: Vec<serde_json::Value> = refreshed
                .listing
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "name": c.name,
                        "source": c.source.as_str(),
                        "state": c.state.as_str(),
                        "transport": c.transport,
                        "tools": c.tools.len(),
                    })
                })
                .collect();
            let value = serde_json::json!({
                "connections": listed,
                "total_tools": total,
                "elapsed_ms": refreshed.elapsed_ms,
                "custom_error": refreshed.custom_error,
                "saved": refreshed.settings.connections.len(),
            });
            println!("{}", serde_json::to_string_pretty(&value)?);
            return Ok(());
        }
        for found in &refreshed.listing {
            println!("  {:<34} {:<8} {:<11} {} tools", found.name, found.source.as_str(), found.state.as_str(), found.tools.len());
        }
        println!(
            "{} connections, {} tools, listed in {:.1} s",
            refreshed.listing.len(),
            total,
            refreshed.elapsed_ms as f64 / 1000.0
        );
        if !refreshed.custom_error.is_empty() {
            eprintln!("note: your own entries could not be listed: {}", refreshed.custom_error);
        }
        eprintln!("nothing is enabled by a refresh: see `openwhisper agent status`");
        return Ok(());
    }

    if json {
        let value: Vec<serde_json::Value> = settings.connections.iter().map(connection_json).collect();
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    if settings.connections.is_empty() {
        println!("no connections yet: run `openwhisper agent connections --refresh`");
    }
    for connection in &settings.connections {
        print_connection_line(connection);
    }
    Ok(())
}

fn switch(path: &std::path::Path, mut settings: AgentSettings, name: &str, enabled: bool, json: bool) -> Result<()> {
    let connection = find(&mut settings, name)?;
    connection.enabled = enabled;
    let (name, ticked) = (connection.name.clone(), connection.allowed_tools.len());
    save(path, settings)?;
    if json {
        println!("{}", serde_json::json!({ "name": name, "enabled": enabled }));
    } else {
        println!("{name} is {}", on_off(enabled));
        if enabled && ticked == 0 {
            eprintln!("no tool of it is ticked yet, so asks cannot use it: see `openwhisper agent tools \"{name}\"`");
        }
    }
    Ok(())
}

fn tools(name: &str, json: bool) -> Result<()> {
    let (_, mut settings) = load()?;
    let drafts_only = settings.drafts_only;
    let connection = find(&mut settings, name)?.clone();
    if json {
        println!("{}", serde_json::to_string_pretty(&connection_json(&connection))?);
        return Ok(());
    }
    println!("{} ({}, {})", connection.name, connection.kind.as_str(), on_off(connection.enabled));
    if connection.tools.is_empty() {
        println!("  no tools known: run `openwhisper agent connections --refresh`");
    }
    for tool in &connection.tools {
        let kind = policy::kind_of(tool);
        let ticked = connection.allows(&tool.name);
        let what = match (ticked, kind) {
            (false, _) => "not ticked",
            (true, agent::ToolKind::Act) if drafts_only => "ticked, cannot run (drafts only)",
            (true, agent::ToolKind::Act) => "ticked, asks first every time",
            (true, _) => "ticked, runs without asking",
        };
        println!("  {:<5} {:<40} {}", kind.as_str(), tool.name, what);
    }
    Ok(())
}

fn tick(name: &str, tools: &[String], allow: bool, json: bool) -> Result<()> {
    let (path, mut settings) = load()?;
    let drafts_only = settings.drafts_only;
    let connection = find(&mut settings, name)?;
    for tool in tools {
        if connection.tool(tool).is_none() {
            let known: Vec<&str> = connection.tools.iter().map(|t| t.name.as_str()).collect();
            bail!(
                "\"{tool}\" is not a tool of {}. {}",
                connection.name,
                if known.is_empty() {
                    "Its tools are not known yet: run `openwhisper agent connections --refresh`.".to_string()
                } else {
                    format!("Its tools are: {}", known.join(", "))
                }
            );
        }
    }
    let mut notes = Vec::new();
    for tool in tools {
        connection.allowed_tools.retain(|t| t != tool);
        if allow {
            connection.allowed_tools.push(tool.clone());
            if let Some(info) = connection.tool(tool).filter(|info| policy::kind_of(info) == agent::ToolKind::Act) {
                notes.push(if drafts_only {
                    format!("{} sends, posts, deletes or changes something: with drafts only it cannot run", info.name)
                } else {
                    format!("{} sends, posts, deletes or changes something: it asks you first, every time", info.name)
                });
            }
        }
    }
    let name = connection.name.clone();
    let saved = save(&path, settings)?;
    let connection = saved.connection(&name).ok_or_else(|| anyhow!("the connection could not be saved"))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&connection_json(connection))?);
        return Ok(());
    }
    println!("{}: {} of {} tools ticked", connection.name, connection.allowed_tools.len(), connection.tools.len());
    for note in notes {
        eprintln!("note: {note}");
    }
    if !connection.enabled && allow {
        eprintln!("the connection is switched off: `openwhisper agent connections enable \"{}\"`", connection.name);
    }
    Ok(())
}

fn apps(args: AppsArgs, json: bool) -> Result<()> {
    let (path, mut settings) = load()?;
    match args.command {
        AppsCommand::List => {}
        AppsCommand::Set { app_id, connection, name } => {
            let app_id = app_id.trim().to_lowercase();
            if app_id.is_empty() || connection.trim().is_empty() {
                bail!("give an app id and a connection");
            }
            let mut table = settings.effective_app_connections();
            let app_name = name.unwrap_or_else(|| {
                table.iter().find(|e| e.app_id == app_id).map(|e| e.app_name.clone()).unwrap_or_default()
            });
            table.retain(|e| e.app_id != app_id);
            table.push(AppConnection::new(&app_id, &app_name, &connection));
            settings.app_connections = Some(table);
            settings = save(&path, settings)?;
            if settings.connection_for_app(&app_id).is_none() && !json {
                eprintln!("note: no enabled connection is called \"{connection}\" right now, so this entry does nothing yet");
            }
        }
        AppsCommand::Unset { app_id } => {
            let app_id = app_id.trim().to_lowercase();
            let mut table = settings.effective_app_connections();
            if !table.iter().any(|e| e.app_id == app_id) {
                bail!("\"{app_id}\" is not in the app table");
            }
            table.retain(|e| e.app_id != app_id);
            settings.app_connections = Some(table);
            settings = save(&path, settings)?;
        }
        AppsCommand::Reset => {
            settings.app_connections = None;
            settings = save(&path, settings)?;
        }
    }
    let table = settings.effective_app_connections();
    if json {
        let value = serde_json::json!({ "app_connections": table, "builtin": settings.app_connections.is_none() });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    println!("{}", if settings.app_connections.is_none() { "the built-in table:" } else { "your table:" });
    for entry in &table {
        println!("  {:<34} {:<16} -> {}", entry.app_id, entry.app_name, entry.connection);
    }
    if table.is_empty() {
        println!("  (empty: no app has a connection)");
    }
    Ok(())
}

// --- run -----------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Confirm {
    Ask,
    Allow,
    Deny,
}

fn confirm_mode(flag: Option<&str>, can_ask: bool) -> Result<Confirm> {
    match flag.map(str::to_lowercase).as_deref() {
        None if can_ask => Ok(Confirm::Ask),
        None => Ok(Confirm::Deny),
        Some("ask") if can_ask => Ok(Confirm::Ask),
        Some("ask") => bail!("--confirm ask needs a terminal to ask at; use --confirm allow or --confirm deny"),
        Some("allow") => Ok(Confirm::Allow),
        Some("deny") => Ok(Confirm::Deny),
        Some(other) => bail!("unknown --confirm \"{other}\"; use ask, allow or deny"),
    }
}

/// Show a confirm at the terminal and answer it from what is typed.
/// On a thread of its own: the ask goes on counting the confirm's
/// time meanwhile, and an answer typed too late finds nothing.
fn ask_at_terminal(confirm: &AskConfirm) {
    eprintln!();
    eprintln!("confirm: {} wants to run on {}", confirm.tool, confirm.connection);
    for line in confirm.input.lines() {
        eprintln!("    {line}");
    }
    eprint!("allow? [y/N] (no answer in {} s is a no) ", confirm.timeout_s);
    let _ = std::io::stderr().flush();
    let request_id = confirm.request_id.clone();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        let yes = matches!(line.trim().to_lowercase().as_str(), "y" | "yes");
        answer_confirm(&request_id, if yes { ConfirmDecision::Allow } else { ConfirmDecision::Deny });
    });
}

fn run_question(args: RunArgs, json: bool) -> Result<()> {
    let (mut ask, refine) = ask_cmd::load(&args.overrides)?;
    if refine.backend == RefineBackendKind::OpenaiCompatible {
        bail!("Agent needs the Claude backend: an OpenAI-compatible endpoint cannot use connections");
    }
    if args.web_search {
        ask.web_search = true;
    }
    let from_stdin = args.text.is_none();
    let text = match args.text {
        Some(t) => t,
        None => {
            if std::io::stdin().is_terminal() {
                bail!("no question given: pass it as an argument or pipe it on stdin");
            }
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf).context("read stdin")?;
            buf
        }
    };
    ask_cmd::use_dictionary();
    let question = if args.skip_rules { text } else { transcript::process(&text) };
    let question = question.trim().to_string();
    if question.is_empty() {
        bail!("no question given: pass it as an argument or pipe it on stdin");
    }
    let confirm = confirm_mode(args.confirm.as_deref(), !from_stdin && !json && std::io::stdin().is_terminal())?;

    let (path, agent_settings) = load()?;
    // The app in front: its name is the one in the app table.
    let front = args.front_app.as_deref().map(|id| {
        let id = id.trim().to_lowercase();
        let name = agent_settings
            .effective_app_connections()
            .into_iter()
            .find(|e| e.app_id == id)
            .map(|e| e.app_name)
            .unwrap_or_default();
        FrontApp::new(&id, &name)
    });
    let Some(run) = AgentRun::new(&agent_settings, front.as_ref()) else {
        let (_, why) = agent::readiness(&AgentSettings { enabled: true, ..agent_settings.clone() }, true, refine.backend);
        bail!("this ask would have no tools. {why} See `openwhisper agent status`.");
    };
    // A connection of your Claude Code has to be looked up in this
    // process before a run can use it.
    if run.connections.iter().any(|c| c.kind == AgentConnectionKind::Claude) {
        if !json {
            eprintln!("looking up your Claude Code connections (no tool is called)");
        }
        connections::list_claude_connections(&refine.claude_cli, connections::LIST_WAIT).map_err(|e| anyhow!("{e}"))?;
    }

    let (vision_settings, vision_choice) = ask_cmd::vision_for(args.screen, args.vision.as_deref())?;
    let front_name = front.as_ref().map(|f| f.name.clone()).filter(|name| !name.is_empty());
    let plan = AskPlan::for_app(&ask, &refine, front_name.as_deref())
        .with_vision(&vision_settings, vision_choice)
        .with_agent(Some(run.clone()));
    let destination = plan.destination();
    let recorded = ask_cmd::use_store();
    let kept = args.history && ask_cmd::use_history()?;
    if !json {
        // Before anything leaves the machine.
        eprintln!("question goes to: {destination}");
        eprintln!(
            "tools: {} on {} ({})",
            run.planned.len(),
            run.connections.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", "),
            if run.drafts_only {
                "drafts only: nothing that sends, posts, deletes or changes anything".to_string()
            } else {
                format!(
                    "{} ask first; a confirm is answered with: {}",
                    run.tools.confirm.len(),
                    match confirm {
                        Confirm::Ask => "your y or n",
                        Confirm::Allow => "yes (--confirm allow)",
                        Confirm::Deny => "no",
                    }
                )
            }
        );
        if let Some(route) = &run.route {
            eprintln!("context: the connection {} (the app in front is {})", route.connection, route.app_name);
        }
        if !agent_settings.enabled {
            eprintln!("note: Agent is switched off in the settings; this run uses its connections all the same");
        }
        let _ = &path;
    }

    let stream = !json && !args.no_stream;
    let mut out = std::io::stdout().lock();
    let mut last_char = '\n';
    let mut screen = ScreenState::None;
    let mut confirms: Vec<serde_json::Value> = Vec::new();
    let mut steps = Vec::new();
    let mut sink = |event: AskEvent| match event {
        AskEvent::Text(piece) if stream => {
            if let Some(c) = piece.chars().last() {
                last_char = c;
            }
            let _ = out.write_all(piece.as_bytes());
            let _ = out.flush();
        }
        AskEvent::Step(step) => {
            if !json {
                let what = match step.state {
                    StepState::Running => "step".to_string(),
                    StepState::Waiting => "step waits for you".to_string(),
                    other => format!("step {}", other.as_str()),
                };
                let detail = if step.state.is_final() { &step.outcome } else { &step.input_summary };
                eprintln!("{what}: {} on {} ({}) {}", step.tool, step.connection, step.kind.as_str(), detail);
            }
            match steps.iter_mut().find(|s: &&mut openwhisper_core::ask::AskStep| s.id == step.id) {
                Some(existing) => *existing = step,
                None => steps.push(step),
            }
        }
        AskEvent::Confirm(request) => {
            let answer = match confirm {
                Confirm::Ask => {
                    ask_at_terminal(&request);
                    "asked"
                }
                Confirm::Allow => {
                    answer_confirm(&request.request_id, ConfirmDecision::Allow);
                    "allow"
                }
                Confirm::Deny => {
                    answer_confirm(&request.request_id, ConfirmDecision::Deny);
                    "deny"
                }
            };
            if !json && confirm != Confirm::Ask {
                eprintln!("confirm: {} on {} answered with {answer}", request.tool, request.connection);
            }
            confirms.push(serde_json::json!({
                "request_id": request.request_id,
                "connection": request.connection,
                "tool": request.tool,
                "input": serde_json::from_str::<serde_json::Value>(&request.input).unwrap_or(serde_json::Value::String(request.input.clone())),
                "input_cut": request.input_cut,
                "outcome": serde_json::Value::Null,
            }));
        }
        AskEvent::ConfirmClosed { request_id, outcome } => {
            if !json && outcome == ConfirmOutcome::TimedOut {
                eprintln!("\nconfirm: no answer in time, so it was not done");
            }
            if let Some(entry) = confirms.iter_mut().find(|c| c["request_id"] == request_id.as_str()) {
                entry["outcome"] = serde_json::json!(outcome.as_str());
            }
        }
        AskEvent::Screen { state, notice } => {
            if !json {
                match state {
                    ScreenState::Capturing => eprintln!("screen: capturing"),
                    ScreenState::Included => eprintln!("screen: included"),
                    ScreenState::Blocked | ScreenState::Unavailable => eprintln!("screen: not included. {notice}"),
                    _ => {}
                }
            }
            if state != ScreenState::Capturing {
                screen = state;
            }
        }
        AskEvent::Searching { query } if !json => eprintln!("searching the web: {query}"),
        _ => {}
    };
    let started = Instant::now();
    let mut backend = plan.backend();
    let result =
        plan.send_and_record(backend.as_mut(), &AskInput::text(question.clone()), &mut sink, &CancelFlag::new(), kept);
    drop(backend);
    drop(out);
    if stream && last_char != '\n' {
        println!();
    }
    let wall_ms = started.elapsed().as_millis() as u64;

    if json {
        let reply = result.as_ref().ok();
        let record = serde_json::json!({
            "question": question,
            "answer": reply.map(|r| r.text.as_str()),
            "error": result.as_ref().err().map(ToString::to_string),
            "steps": steps,
            "confirms": confirms,
            "context_source": reply.map(|r| serde_json::json!(r.context_source)).unwrap_or_else(|| {
                serde_json::json!(run.route.as_ref().map_or(openwhisper_core::ask::ContextSource::None, |route| {
                    openwhisper_core::ask::ContextSource::Connection { name: route.connection.clone() }
                }))
            }),
            "screen": reply.map_or(screen, |r| r.screen).as_str(),
            "notice": reply.map_or("", |r| r.notice.as_str()),
            "first_text_ms": reply.and_then(|r| r.first_text_ms),
            "elapsed_ms": reply.map(|r| r.elapsed_ms),
            "wall_ms": wall_ms,
            "usage": reply.and_then(|r| r.usage.as_ref()),
            "backend": refine.backend.as_str(),
            "model": plan.setup.model,
            "effort": plan.effort_id(),
            "web_search": plan.setup.web_search,
            "drafts_only": run.drafts_only,
            "tools": {
                "allowed": run.tools.allowed,
                "confirm": run.tools.confirm,
                "disallowed": run.tools.disallowed.len(),
            },
            "connections": run.connections.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            "timeout_ms": plan.timeout.as_millis() as u64,
            "destination": destination,
            "recorded": recorded,
            "kept_in_history": kept && result.is_ok(),
        });
        println!("{}", serde_json::to_string_pretty(&record)?);
    }
    match result {
        Ok(reply) => {
            if !json {
                if !stream {
                    println!("{}", reply.text);
                }
                if !reply.notice.is_empty() {
                    eprintln!("note: {}", reply.notice);
                }
                eprintln!(
                    "{} tool call{}, first text after {}, done in {} ms ({}, {}{})",
                    reply.steps.len(),
                    if reply.steps.len() == 1 { "" } else { "s" },
                    reply.first_text_ms.map_or("-".to_string(), |ms| format!("{ms} ms")),
                    reply.elapsed_ms,
                    refine.backend.as_str(),
                    plan.setup.model,
                    ask_cmd::effort_label(plan.effort_id()),
                );
                if let Some(usage) = &reply.usage {
                    eprintln!(
                        "tokens: {} in ({} cached), {} out{}",
                        usage.tokens_in,
                        usage.tokens_cached,
                        usage.tokens_out,
                        usage.cost_usd.map_or(String::new(), |c| format!("; API list price ${c:.4}"))
                    );
                }
            }
            Ok(())
        }
        Err(e) => {
            if !json {
                eprintln!("no answer: {e}");
            }
            // Non-zero exit so scripts can branch on it; the reason
            // is already printed.
            std::process::exit(1);
        }
    }
}

// --- eval ----------------------------------------------------------

fn median(times: &mut [u64]) -> Option<u64> {
    if times.is_empty() {
        return None;
    }
    times.sort_unstable();
    Some(times[times.len() / 2])
}

fn run_eval(args: EvalArgs, json: bool) -> Result<()> {
    if args.offline {
        let checks = eval::self_test();
        let wrong: Vec<&eval::SelfCheck> = checks.iter().filter(|c| !c.passed).collect();
        if json {
            let value = serde_json::json!({ "checks": checks, "passed": checks.len() - wrong.len(), "total": checks.len() });
            println!("{}", serde_json::to_string_pretty(&value)?);
        } else {
            for check in &checks {
                println!(
                    "{}  {:<24} {}{}",
                    if check.passed { "ok  " } else { "WRONG" },
                    check.case,
                    check.what,
                    if check.problem.is_empty() { String::new() } else { format!(": {}", check.problem) }
                );
            }
            println!("{} of {} checks behave (no model was asked)", checks.len() - wrong.len(), checks.len());
        }
        if !wrong.is_empty() {
            std::process::exit(1);
        }
        return Ok(());
    }

    let (ask, refine) = ask_cmd::load(&args.overrides)?;
    if refine.backend == RefineBackendKind::OpenaiCompatible {
        bail!("Agent needs the Claude backend: an OpenAI-compatible endpoint cannot use connections");
    }
    let backend_status = refine::backend_status(&refine);
    if !backend_status.ready {
        bail!("backend {} is not ready: {}", backend_status.backend, backend_status.message);
    }
    let chosen: Vec<eval::EvalCase> = eval::cases()
        .into_iter()
        .filter(|case| args.only.as_deref().is_none_or(|prefix| case.name.starts_with(prefix)))
        .collect();
    if chosen.is_empty() {
        bail!(
            "no eval case starts with \"{}\"; the cases are: {}",
            args.only.unwrap_or_default(),
            eval::cases().iter().map(|c| c.name).collect::<Vec<_>>().join(", ")
        );
    }
    let log_dir = std::env::temp_dir().join(format!("openwhisper-agent-eval-{}", std::process::id()));
    std::fs::create_dir_all(&log_dir).context("create the folder for the fixture's logs")?;
    let fixture = eval::FixtureCommand {
        command: std::env::current_exe().context("find this binary")?.display().to_string(),
        args: vec!["agent".into(), "fake-server".into()],
        log_dir: log_dir.clone(),
    };
    if !json {
        eprintln!("questions go to: {}", openwhisper_core::refine::claude_cli::DESTINATION);
        eprintln!("the only connection is a test fixture; no real connection is in any run and no real screen is captured");
    }
    let mut results: Vec<eval::EvalResult> = Vec::new();
    let mut stopped: Option<(String, String, usize)> = None;
    for (at, case) in chosen.iter().enumerate() {
        let result = eval::run_case(case, &ask, &refine, &fixture);
        if eval::limit_reached(&result.error) {
            stopped = Some((result.name.clone(), result.error.clone(), chosen.len() - at));
            if !json {
                println!("STOP  {}: the backend refused ({}); {} cases not run", result.name, result.error, chosen.len() - at);
            }
            break;
        }
        if !json {
            println!(
                "{}  {:<24} {:>6} ms  calls: {:<34} {}",
                if result.leak { "LEAK" } else if result.passed { "PASS" } else { "FAIL" },
                result.name,
                result.elapsed_ms,
                if result.calls.is_empty() { "-".to_string() } else { result.calls.join(",") },
                ask_cmd::one_line(&result.answer)
            );
            if !result.passed {
                println!("      asked:    {}", result.question);
                println!("      risk:     {}", result.risk);
                println!("      problem:  {}", result.problem);
                if !result.confirms.is_empty() {
                    println!("      confirms: {}", result.confirms.join(", "));
                }
            }
        }
        results.push(result);
    }
    let _ = std::fs::remove_dir_all(&log_dir);

    let passed = results.iter().filter(|r| r.passed).count();
    let leaks: Vec<&str> = results.iter().filter(|r| r.leak).map(|r| r.name.as_str()).collect();
    let times = |group: &str| -> (Option<u64>, Option<u64>) {
        let mut times: Vec<u64> =
            results.iter().filter(|r| r.group == group && r.error.is_empty()).map(|r| r.elapsed_ms).collect();
        (median(&mut times), times.iter().copied().max())
    };
    if json {
        let group = |name: &str| {
            let (median, max) = times(name);
            serde_json::json!({ "median_ms": median, "max_ms": max })
        };
        let value = serde_json::json!({
            "backend": refine.backend.as_str(),
            "model": ask.model_for(&refine),
            "passed": passed,
            "total": results.len(),
            "leaks": leaks,
            "stopped": stopped.as_ref().map(|(case, reason, not_run)| serde_json::json!({ "case": case, "reason": reason, "not_run": not_run })),
            "run_times": { "read": group("read"), "draft": group("draft"), "send": group("send") },
            "results": results,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("{passed} of {} passed", results.len());
        for (label, name) in [("a read", "read"), ("a draft", "draft"), ("a confirmed send", "send")] {
            if let (Some(median), Some(max)) = times(name) {
                println!("{label}: median {median} ms, longest {max} ms");
            }
        }
        if leaks.is_empty() {
            println!("no call that sends or deletes reached the connection where none was expected");
        } else {
            println!("LEAK: a call that sends or deletes reached the connection in: {}", leaks.join(", "));
        }
    }
    if stopped.is_some() || passed != results.len() {
        std::process::exit(1);
    }
    Ok(())
}

fn fake_server(args: FakeServerArgs) -> Result<()> {
    if fixture::thread_text(&args.scenario).is_none() {
        bail!(
            "unknown scenario \"{}\"; the scenarios are: {}",
            args.scenario,
            fixture::SCENARIOS.iter().map(|(name, _)| *name).collect::<Vec<_>>().join(", ")
        );
    }
    let stdin = std::io::stdin().lock();
    let stdout = std::io::stdout().lock();
    fixture::serve(stdin, stdout, &args.log, &args.scenario).context("serve the test fixture")
}
