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

fn specs() -> Vec<M<'static>> {
    vec![
        M::up(MIGRATION_1_DICTATIONS),
        M::up(MIGRATION_2_WALL_CLOCK),
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
