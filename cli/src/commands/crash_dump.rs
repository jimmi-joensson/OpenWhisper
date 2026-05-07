//! `openwhisper crash-dump` — list / read crash files via the
//! `core::diagnostics::CrashDumpReader` trait.
//!
//! Three sub-modes (mutually exclusive flags):
//!
//! - `--list`            — every crash on disk, newest-first.
//! - `--latest`          — show the most recent crash. Default if no
//!                         flag is given.
//! - `--id <ID>`         — show a specific crash by id.
//!
//! Output format toggled with the global `--json` flag:
//! - Plain: human-readable summary (one line per row for `--list`,
//!   pretty-printed report for `--latest` / `--id`).
//! - JSON: raw [`CrashDump`] (full file) or array thereof.
//!
//! Crash dir resolution: defaults to the OS-default release-bundle
//! path (`~/Library/Logs/com.openwhisper.app/crashes` on macOS,
//! `%LOCALAPPDATA%\com.openwhisper.app\logs\crashes` on Windows). Use
//! `--dir <PATH>` to inspect dev-build crashes
//! (`com.openwhisper.dev`) or a Playwright fixture dir.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;
use openwhisper_core::crashes::build_github_issue_url;
use openwhisper_core::diagnostics::{
    self, CrashDump, CrashDumpReader, CrashId, FileBackedCrashDumpReader,
    ReadError,
};

const GITHUB_OWNER: &str = "jimmi-joensson";
const GITHUB_REPO: &str = "OpenWhisper";

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Show the most recent crash. Mutually exclusive with `--id`
    /// and `--list`.
    #[arg(long, conflicts_with_all = ["id", "list"])]
    pub latest: bool,

    /// Show a specific crash by id.
    #[arg(long, value_name = "ID", conflicts_with_all = ["latest", "list"])]
    pub id: Option<String>,

    /// List all crashes newest-first.
    #[arg(long, conflicts_with_all = ["latest", "id"])]
    pub list: bool,

    /// Override the crash directory. Defaults to the OS-specific
    /// app log dir for the release bundle (`com.openwhisper.app`).
    /// Use `--dir <path>` to inspect dev-build crashes
    /// (`com.openwhisper.dev`) or a temp fixture dir.
    #[arg(long, value_name = "PATH")]
    pub dir: Option<PathBuf>,

    /// Print a prefilled GitHub Issues URL instead of the
    /// human-readable report. Compose with `--latest` (default) or
    /// `--id <ID>` to target a specific crash; mutually exclusive
    /// with `--list`. With `--json`, emits `{ "url": "..." }`.
    #[arg(long, conflicts_with = "list")]
    pub github_url: bool,
}

pub fn run(args: Args, json: bool) -> Result<()> {
    let reader = resolve_reader(args.dir.as_deref())?;

    if args.list {
        list(reader.as_ref(), json)
    } else if args.github_url {
        github_url(reader.as_ref(), args.id.as_deref(), json)
    } else if let Some(id) = args.id.as_deref() {
        show_by_id(reader.as_ref(), id, json)
    } else {
        // No flag, or `--latest` explicitly → show the newest crash.
        show_latest(reader.as_ref(), json)
    }
}

fn github_url(
    reader: &dyn CrashDumpReader,
    id: Option<&str>,
    json: bool,
) -> Result<()> {
    let crash = match id {
        Some(s) => match reader.read(&CrashId::new(s)) {
            Ok(c) => c,
            Err(ReadError::NotFound) => bail!("crash {s} not found"),
            Err(ReadError::UnsafeId(x)) => bail!("invalid crash id: {x}"),
            Err(e) => return Err(anyhow::anyhow!(e)),
        },
        None => match reader.list().drain(..).next() {
            Some(c) => c,
            None => bail!("no crashes recorded"),
        },
    };
    let url = build_github_issue_url(&crash, GITHUB_OWNER, GITHUB_REPO);
    if json {
        println!("{}", serde_json::json!({ "url": url }));
    } else {
        println!("{url}");
    }
    Ok(())
}

fn resolve_reader(
    dir_override: Option<&std::path::Path>,
) -> Result<Box<dyn CrashDumpReader>> {
    if let Some(dir) = dir_override {
        return Ok(Box::new(FileBackedCrashDumpReader::new(dir.to_path_buf())));
    }
    diagnostics::default_crash_reader().with_context(|| {
        "no default crash dir resolved on this platform — pass --dir to point at one explicitly"
    })
}

fn list(reader: &dyn CrashDumpReader, json: bool) -> Result<()> {
    let dumps = reader.list();
    if json {
        println!("{}", serde_json::to_string_pretty(&dumps)?);
        return Ok(());
    }
    if dumps.is_empty() {
        eprintln!("no crashes recorded");
        return Ok(());
    }
    for dump in dumps {
        println!(
            "{id}\t{ts}\t{app}\t{os}\t{msg}",
            id = dump.id,
            ts = dump.ts_unix_ms,
            app = dump.app_version,
            os = dump.os,
            msg = truncate_message(&dump.rust_panic.message, 80),
        );
    }
    Ok(())
}

fn show_latest(reader: &dyn CrashDumpReader, json: bool) -> Result<()> {
    let mut dumps = reader.list();
    let Some(latest) = dumps.drain(..).next() else {
        if json {
            println!("null");
        } else {
            eprintln!("no crashes recorded");
        }
        return Ok(());
    };
    print_dump(&latest, json)
}

fn show_by_id(
    reader: &dyn CrashDumpReader,
    id: &str,
    json: bool,
) -> Result<()> {
    match reader.read(&CrashId::new(id)) {
        Ok(dump) => print_dump(&dump, json),
        Err(ReadError::NotFound) => bail!("crash {id} not found"),
        Err(ReadError::UnsafeId(s)) => bail!("invalid crash id: {s}"),
        Err(e) => Err(anyhow::anyhow!(e)),
    }
}

fn print_dump(dump: &CrashDump, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(dump)?);
        return Ok(());
    }
    // Plain: a human-readable summary of the crash. Backtrace prints
    // last so `openwhisper crash-dump --latest | head -20` shows the
    // identity block first.
    println!("crash {} ({})", dump.id, dump.ts_unix_ms);
    println!("  app:       {}", dump.app_version);
    println!("  os:        {}", dump.os);
    println!("  thread:    {}", dump.rust_panic.thread_name);
    println!("  panic at:  {}", dump.rust_panic.location);
    println!("  message:   {}", dump.rust_panic.message);
    if let Some(rs) = dump.recording_state.as_ref() {
        println!(
            "  recording: {} ({} ms, {} samples)",
            rs.status_message_at_crash, rs.duration_ms, rs.samples_captured,
        );
        if let Some(model) = rs.model_kind.as_ref() {
            println!("  model:     {model}");
        }
    } else {
        println!("  recording: <none>");
    }
    println!("  events:    {}", dump.events.len());
    println!();
    println!("backtrace:");
    println!("{}", dump.rust_panic.backtrace);
    Ok(())
}

fn truncate_message(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}
