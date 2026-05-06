//! `openwhisper crash-dump` — placeholder until Task 8.

use anyhow::{bail, Result};
use clap::Args as ClapArgs;

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

pub fn run(_args: Args, _json: bool) -> Result<()> {
    bail!("`crash-dump` lands in TASK-81.2 / Task 8 — see backlog/tasks/task-81.8*")
}
