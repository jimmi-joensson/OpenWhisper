//! Schema migrations chain. Append-only — never edit a migration that has
//! shipped. [`apply_pending`] runs only the unapplied entries based on the
//! `user_version` pragma.
//!
//! Migration 1 (the `dictations` table + index) lands in TASK-87.2; this
//! module currently exposes an empty chain. `Migrations::to_latest` rejects
//! an empty list with `NoMigrationsDefined`, so [`apply_pending`] returns
//! `Ok(())` without invoking the migrator while no migrations exist.

use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

use super::StoreError;

fn specs() -> Vec<M<'static>> {
    vec![]
}

pub fn apply_pending(conn: &mut Connection) -> Result<(), StoreError> {
    let specs = specs();
    if specs.is_empty() {
        return Ok(());
    }
    Migrations::new(specs).to_latest(conn)?;
    Ok(())
}
