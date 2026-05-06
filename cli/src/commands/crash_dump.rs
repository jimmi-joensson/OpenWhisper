//! `openwhisper crash-dump` — list / read crash files via the
//! `core::diagnostics::CrashDumpReader` trait. Until TASK-78 lands a
//! concrete reader, [`default_crash_reader`] returns `None` and this
//! handler prints a deferred-feature notice on stderr and exits 0
//! (CI smoke must not break before TASK-78).

use anyhow::{bail, Result};
use clap::Args as ClapArgs;
use openwhisper_core::diagnostics::{self, CrashDumpReader};

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
}

pub fn run(args: Args, json: bool) -> Result<()> {
    let Some(reader) = diagnostics::default_crash_reader() else {
        // TASK-78 swaps in a concrete `FileBackedCrashDumpReader`;
        // until then announce on stderr (so `--json` stdout stays
        // pipeline-safe) and exit 0.
        eprintln!("crash reporting not yet enabled — see TASK-78 / backlog/tasks/task-78*");
        if json {
            println!("{}", serde_json::json!({ "available": false }));
        }
        return Ok(());
    };

    if args.list {
        list(reader.as_ref(), json)
    } else if args.latest {
        show_latest(reader.as_ref(), json)
    } else if let Some(id) = args.id.as_deref() {
        show_by_id(reader.as_ref(), id, json)
    } else {
        // No flag → default to --latest.
        show_latest(reader.as_ref(), json)
    }
}

fn list(_reader: &dyn CrashDumpReader, _json: bool) -> Result<()> {
    // Concrete output shape lands with TASK-78 once `CrashDump` has
    // real fields. Keep the early-bail explicit so a future reader
    // returning Some-but-empty list doesn't fall through silently.
    bail!("crash-dump --list is wired but the concrete reader is TASK-78")
}

fn show_latest(_reader: &dyn CrashDumpReader, _json: bool) -> Result<()> {
    bail!("crash-dump --latest is wired but the concrete reader is TASK-78")
}

fn show_by_id(_reader: &dyn CrashDumpReader, _id: &str, _json: bool) -> Result<()> {
    bail!("crash-dump --id is wired but the concrete reader is TASK-78")
}
