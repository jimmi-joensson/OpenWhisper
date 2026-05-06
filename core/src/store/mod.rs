//! On-disk persistence layer for OpenWhisper.
//!
//! Single SQLite file at `<app_data_dir>/openwhisper.db`, opened once per
//! process and shared across threads via a `Mutex<Connection>`. The shell
//! resolves the path (Tauri uses `app.path().app_data_dir()`) and hands it
//! to [`Store::open_or_init`]; core/ owns everything from the connection
//! down so the same code runs on macOS and Windows.
//!
//! Schema migrations live in the (still-empty) [`migrations`] module and
//! are applied by `to_latest` during `open_or_init`. Migration 1 lands in
//! TASK-87.2 and creates the `dictations` table.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

mod migrations;

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    Migration(rusqlite_migration::Error),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "store io: {e}"),
            Self::Sqlite(e) => write!(f, "store sqlite: {e}"),
            Self::Migration(e) => write!(f, "store migration: {e}"),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Sqlite(e) => Some(e),
            Self::Migration(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

impl From<rusqlite_migration::Error> for StoreError {
    fn from(e: rusqlite_migration::Error) -> Self {
        Self::Migration(e)
    }
}

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    /// Open or create the DB at `path` and run all pending migrations.
    /// Idempotent — safe to call across processes (file-locked via
    /// SQLite's own locking).
    pub fn open_or_init(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let mut conn = Connection::open(path)?;
        migrations::apply_pending(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Borrow the connection inside a closure. Holds the mutex for the
    /// duration; closures must be short.
    pub fn with_conn<R>(
        &self,
        f: impl FnOnce(&Connection) -> Result<R, StoreError>,
    ) -> Result<R, StoreError> {
        let guard = self.conn.lock().expect("store mutex poisoned");
        f(&guard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn open_or_init_creates_file_and_user_version_is_zero() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("openwhisper.db");
        let store = Store::open_or_init(&path).expect("open_or_init");
        assert!(path.exists(), "db file should exist after open_or_init");

        let version: i64 = store
            .with_conn(|c| {
                c.query_row("PRAGMA user_version", [], |r| r.get(0))
                    .map_err(StoreError::from)
            })
            .expect("query user_version");
        assert_eq!(version, 0, "no migrations defined yet");
    }

    #[test]
    fn open_or_init_creates_missing_parent_dirs() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("nested").join("a").join("openwhisper.db");
        Store::open_or_init(&path).expect("open_or_init nested");
        assert!(path.exists(), "db file should exist in nested parent");
    }
}
