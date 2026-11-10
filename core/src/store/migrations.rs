//! Schema migrations chain. Append-only — never edit a migration that has
//! shipped. [`apply_pending`] runs only the unapplied entries based on the
//! `user_version` pragma.

use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

use super::StoreError;

const MIGRATION_1_DICTATIONS: &str = "\
CREATE TABLE dictations (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at    INTEGER NOT NULL,
    duration_ms   INTEGER NOT NULL,
    word_count    INTEGER NOT NULL,
    transcript    TEXT,
    confidence    REAL,
    app_bundle_id TEXT,
    created_at    INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);
CREATE INDEX idx_dictations_started_at ON dictations(started_at);";

/// Wall-clock recording duration alongside the VAD speech window in
/// `duration_ms`. Lets us compare what the user actually saw on the
/// timer (`wall_clock_ms`) against what we counted as active speech
/// (`duration_ms`) — useful for tuning the VAD threshold and the
/// trim behavior. NULL on rows written before this migration so the
/// column carries diagnostic-only weight; existing aggregates
/// (Time Saved) only read `duration_ms`.
const MIGRATION_2_WALL_CLOCK: &str = "\
ALTER TABLE dictations ADD COLUMN wall_clock_ms INTEGER;";

/// One row per Refine run (TASK-93), for the Diagnostics pane and
/// `openwhisper refine history`. Timings, lengths and the reason for a
/// fallback; never any dictated text. See `core::refine::history`.
const MIGRATION_3_REFINE_RUNS: &str = "\
CREATE TABLE refine_runs (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    at_ms        INTEGER NOT NULL,
    source       TEXT NOT NULL,
    status       TEXT NOT NULL,
    backend      TEXT NOT NULL,
    model        TEXT NOT NULL,
    effort       TEXT NOT NULL,
    elapsed_ms   INTEGER NOT NULL,
    warm_lead_ms INTEGER,
    chars_in     INTEGER NOT NULL,
    chars_out    INTEGER NOT NULL,
    instruction  INTEGER NOT NULL,
    detail       TEXT NOT NULL
);";

/// What each Refine call used: tokens, the API list-price cost the
/// backend reports, and how full the subscription's 5-hour and 7-day
/// windows were right after it. All nullable: only some backends
/// report them, and rows written before this migration have none.
const MIGRATION_4_REFINE_USAGE: &str = "\
ALTER TABLE refine_runs ADD COLUMN tokens_in INTEGER;
ALTER TABLE refine_runs ADD COLUMN tokens_cached INTEGER;
ALTER TABLE refine_runs ADD COLUMN tokens_out INTEGER;
ALTER TABLE refine_runs ADD COLUMN cost_usd REAL;
ALTER TABLE refine_runs ADD COLUMN window_5h_used REAL;
ALTER TABLE refine_runs ADD COLUMN window_5h_resets_at INTEGER;
ALTER TABLE refine_runs ADD COLUMN window_7d_used REAL;
ALTER TABLE refine_runs ADD COLUMN window_7d_resets_at INTEGER;";

fn specs() -> Vec<M<'static>> {
    vec![
        M::up(MIGRATION_1_DICTATIONS),
        M::up(MIGRATION_2_WALL_CLOCK),
        M::up(MIGRATION_3_REFINE_RUNS),
        M::up(MIGRATION_4_REFINE_USAGE),
    ]
}

pub fn apply_pending(conn: &mut Connection) -> Result<(), StoreError> {
    let specs = specs();
    if specs.is_empty() {
        return Ok(());
    }
    Migrations::new(specs).to_latest(conn)?;
    Ok(())
}
