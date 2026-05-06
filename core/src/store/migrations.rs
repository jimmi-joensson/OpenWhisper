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

fn specs() -> Vec<M<'static>> {
    vec![M::up(MIGRATION_1_DICTATIONS)]
}

pub fn apply_pending(conn: &mut Connection) -> Result<(), StoreError> {
    let specs = specs();
    if specs.is_empty() {
        return Ok(());
    }
    Migrations::new(specs).to_latest(conn)?;
    Ok(())
}
