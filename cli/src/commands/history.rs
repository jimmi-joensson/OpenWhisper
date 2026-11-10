//! `openwhisper history`: the transcript history the desktop app
//! keeps when the user has switched it on (TASK-95). Reads and edits
//! the same SQLite file and the same `history` settings block as the
//! app, through the same core functions.

use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};
use chrono::{Local, TimeZone};
use clap::{Args as ClapArgs, Subcommand};
use openwhisper_core::history::{self, HistoryEntry};
use openwhisper_core::settings::{self, HistorySettings};
use openwhisper_core::store::Store;

#[derive(ClapArgs, Debug)]
pub struct HistoryArgs {
    #[command(subcommand)]
    command: HistoryCommand,
}

#[derive(Subcommand, Debug)]
enum HistoryCommand {
    /// Show stored dictations, newest first.
    List(ListArgs),
    /// Show stored dictations that contain the text.
    Search(SearchArgs),
    /// Print one entry in full.
    Show(IdArgs),
    /// Remove one entry's text. Its word count stays in the stats.
    Delete(IdArgs),
    /// Remove all stored text. Word counts stay in the stats.
    Clear(ClearArgs),
    /// Show or change whether history is kept, and for how long.
    Config(ConfigArgs),
}

#[derive(ClapArgs, Debug)]
struct ListArgs {
    /// Only entries containing this text (case does not matter).
    #[arg(long)]
    search: Option<String>,
    #[arg(long, default_value_t = 20)]
    limit: u32,
    #[arg(long, default_value_t = 0)]
    offset: u32,
}

#[derive(ClapArgs, Debug)]
struct SearchArgs {
    /// Text to look for in the transcript, the original transcript
    /// and the app name.
    query: String,
    #[arg(long, default_value_t = 20)]
    limit: u32,
    #[arg(long, default_value_t = 0)]
    offset: u32,
}

#[derive(ClapArgs, Debug)]
struct IdArgs {
    /// Entry id, as printed by `history list`.
    id: i64,
}

#[derive(ClapArgs, Debug)]
struct ClearArgs {
    /// Required: confirms that all stored transcript text is removed.
    #[arg(long)]
    yes: bool,
}

#[derive(ClapArgs, Debug)]
struct ConfigArgs {
    /// Switch history on or off. Off stops saving; stored text stays
    /// until it is cleared or ages out.
    #[arg(long, value_parser = parse_bool)]
    enabled: Option<bool>,
    /// Remove stored text older than this many days. 0 keeps it until
    /// you delete it.
    #[arg(long, value_name = "DAYS")]
    retention_days: Option<u32>,
}

fn parse_bool(s: &str) -> Result<bool, String> {
    match s.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        other => Err(format!(
            "expected true/false (or 1/0, yes/no, on/off); got {other}"
        )),
    }
}

pub fn run(args: HistoryArgs, json: bool) -> Result<()> {
    match args.command {
        HistoryCommand::List(a) => list(a.search.as_deref().unwrap_or(""), a.limit, a.offset, json),
        HistoryCommand::Search(a) => list(&a.query, a.limit, a.offset, json),
        HistoryCommand::Show(a) => show(a.id, json),
        HistoryCommand::Delete(a) => delete(a.id, json),
        HistoryCommand::Clear(a) => clear(a, json),
        HistoryCommand::Config(a) => config(a, json),
    }
}

fn settings_path() -> Result<PathBuf> {
    Ok(super::config_dir()?.join("settings.json"))
}

fn open_store() -> Result<Store> {
    let path = super::database_path()?;
    if !path.is_file() {
        bail!(
            "no database at {}: the desktop app has not run yet",
            path.display()
        );
    }
    Store::open_or_init(&path).map_err(|e| anyhow!("open {}: {e}", path.display()))
}

fn local_time(at_ms: i64) -> String {
    Local
        .timestamp_millis_opt(at_ms)
        .single()
        .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| at_ms.to_string())
}

/// First line of the text, shortened to fit a terminal row.
fn preview(text: &str, max_chars: usize) -> String {
    let line = text.lines().next().unwrap_or_default();
    if line.chars().count() <= max_chars && line.len() == text.trim_end().len() {
        return line.to_string();
    }
    let cut: String = line.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{}...", cut.trim_end())
}

fn list(search: &str, limit: u32, offset: u32, json: bool) -> Result<()> {
    let store = open_store()?;
    let page = history::list(&store, search, limit, offset).map_err(|e| anyhow!("read history: {e}"))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&page)?);
        return Ok(());
    }
    if page.stored == 0 {
        let on = settings::load_history_settings(&settings_path()?).enabled;
        println!(
            "No transcripts stored. History is {}.",
            if on { "on" } else { "off (switch it on with: openwhisper history config --enabled true)" }
        );
        return Ok(());
    }
    if page.entries.is_empty() {
        println!("No stored transcript matches.");
        return Ok(());
    }
    for e in &page.entries {
        let app = e.app_name.as_deref().or(e.app_id.as_deref()).unwrap_or("-");
        println!("{:>6}  {}  {:<16}  {}", e.id, local_time(e.at_ms), preview(app, 16), preview(&e.text, 80));
    }
    eprintln!(
        "showing {} of {} {}",
        page.entries.len(),
        page.matched,
        if search.trim().is_empty() { "stored" } else { "matching" }
    );
    Ok(())
}

fn print_entry(e: &HistoryEntry) {
    println!("id        {}", e.id);
    println!("time      {}", local_time(e.at_ms));
    println!("words     {}", e.word_count);
    println!("duration  {:.1} s", e.duration_ms as f64 / 1000.0);
    if let Some(name) = e.app_name.as_deref().or(e.app_id.as_deref()) {
        match e.app_id.as_deref() {
            Some(id) if id != name => println!("app       {name} ({id})"),
            _ => println!("app       {name}"),
        }
    }
    println!();
    println!("{}", e.text);
    if let Some(original) = &e.original {
        println!();
        println!("before Refine:");
        println!("{original}");
    }
}

fn show(id: i64, json: bool) -> Result<()> {
    let store = open_store()?;
    let Some(entry) = history::get(&store, id).map_err(|e| anyhow!("read history: {e}"))? else {
        bail!("no stored transcript with id {id}");
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&entry)?);
    } else {
        print_entry(&entry);
    }
    Ok(())
}

fn delete(id: i64, json: bool) -> Result<()> {
    let store = open_store()?;
    let deleted = history::delete(&store, id).map_err(|e| anyhow!("delete: {e}"))?;
    if json {
        println!("{}", serde_json::json!({ "deleted": deleted }));
    } else if deleted {
        println!("Removed the text of entry {id}.");
    } else {
        bail!("no stored transcript with id {id}");
    }
    Ok(())
}

fn clear(args: ClearArgs, json: bool) -> Result<()> {
    if !args.yes {
        bail!("this removes all stored transcript text; run it again with --yes to confirm");
    }
    let store = open_store()?;
    let removed = history::clear(&store).map_err(|e| anyhow!("clear: {e}"))?;
    if json {
        println!("{}", serde_json::json!({ "removed": removed }));
    } else {
        println!("Removed the text of {removed} {}.", if removed == 1 { "entry" } else { "entries" });
    }
    Ok(())
}

fn retention_label(s: &HistorySettings) -> String {
    match s.retention_days {
        0 => "kept until you delete it".to_string(),
        1 => "removed after 1 day".to_string(),
        n => format!("removed after {n} days"),
    }
}

fn config(args: ConfigArgs, json: bool) -> Result<()> {
    let path = settings_path()?;
    let mut current = settings::load_history_settings(&path);
    let changing = args.enabled.is_some() || args.retention_days.is_some();
    let mut pruned = 0;
    if changing {
        if let Some(enabled) = args.enabled {
            current.enabled = enabled;
        }
        if let Some(days) = args.retention_days {
            current.retention_days = days;
        }
        current = settings::save_history_settings(&path, current).map_err(|e| anyhow!("save: {e}"))?;
        // A shorter retention takes effect at once, like in the app.
        if super::database_path()?.is_file() {
            let store = open_store()?;
            pruned = history::prune(&store, &current, history::now_ms())
                .map_err(|e| anyhow!("apply retention: {e}"))?;
        }
    }
    if json {
        let mut value = serde_json::to_value(current)?;
        value["database"] = serde_json::json!(super::database_path()?.display().to_string());
        if changing {
            value["pruned"] = serde_json::json!(pruned);
        }
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    println!("history    {}", if current.enabled { "on" } else { "off" });
    println!("retention  {}", retention_label(&current));
    println!("stored in  {}", super::database_path()?.display());
    if pruned > 0 {
        println!("Removed the text of {pruned} {} older than the retention.", if pruned == 1 { "entry" } else { "entries" });
    }
    if changing {
        eprintln!("a running app picks this up the next time Home or Settings is opened");
    }
    Ok(())
}
