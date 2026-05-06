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

    /// Test-only: build a Store from an arbitrary connection without
    /// running migrations. Lets `stats::tests` exercise failure paths
    /// (e.g. an INSERT against a connection that has no `dictations`
    /// table) without poking at private fields from outside the module.
    #[cfg(test)]
    pub(crate) fn from_connection_for_test(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn user_version(store: &Store) -> i64 {
        store
            .with_conn(|c| {
                c.query_row("PRAGMA user_version", [], |r| r.get(0))
                    .map_err(StoreError::from)
            })
            .expect("query user_version")
    }

    #[test]
    fn open_or_init_creates_file_and_runs_migration_1() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("openwhisper.db");
        let store = Store::open_or_init(&path).expect("open_or_init");
        assert!(path.exists(), "db file should exist after open_or_init");
        assert_eq!(user_version(&store), 1, "migration 1 should be applied");
    }

    #[test]
    fn open_or_init_creates_missing_parent_dirs() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("nested").join("a").join("openwhisper.db");
        Store::open_or_init(&path).expect("open_or_init nested");
        assert!(path.exists(), "db file should exist in nested parent");
    }

    #[test]
    fn dictations_table_and_index_exist_after_init() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("openwhisper.db");
        let store = Store::open_or_init(&path).expect("open_or_init");

        let columns: Vec<String> = store
            .with_conn(|c| {
                let mut stmt = c.prepare("PRAGMA table_info(dictations)")?;
                let rows = stmt
                    .query_map([], |r| r.get::<_, String>(1))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .expect("table_info");
        assert_eq!(
            columns,
            vec![
                "id",
                "started_at",
                "duration_ms",
                "word_count",
                "transcript",
                "confidence",
                "app_bundle_id",
                "created_at",
            ],
            "dictations columns",
        );

        let index_exists: i64 = store
            .with_conn(|c| {
                c.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_dictations_started_at'",
                    [],
                    |r| r.get(0),
                )
                .map_err(StoreError::from)
            })
            .expect("count index");
        assert_eq!(index_exists, 1, "idx_dictations_started_at should exist");
    }

    #[test]
    fn reopen_is_idempotent() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("openwhisper.db");
        {
            let store = Store::open_or_init(&path).expect("first open");
            assert_eq!(user_version(&store), 1);
        }
        let store = Store::open_or_init(&path).expect("second open");
        assert_eq!(user_version(&store), 1, "second open is no-op");
    }

    #[test]
    fn concurrent_reads_dont_deadlock_or_panic() {
        use std::sync::Arc;
        use std::thread;
        use std::time::{Duration, Instant};

        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("openwhisper.db");
        let store = Arc::new(Store::open_or_init(&path).expect("open_or_init"));

        // Seed three rows so SELECT COUNT returns a stable, non-zero
        // number every iteration — readers can compare against the
        // same expected value without coordinating.
        for i in 0..3i64 {
            store
                .with_conn(|c| {
                    c.execute(
                        "INSERT INTO dictations (started_at, duration_ms, word_count) VALUES (?, ?, ?)",
                        [i * 1000, 500, 5],
                    )
                    .map(|_| ())
                    .map_err(StoreError::from)
                })
                .expect("seed insert");
        }

        const THREADS: usize = 8;
        const ITERS: usize = 100;
        let started = Instant::now();
        let handles: Vec<_> = (0..THREADS)
            .map(|_| {
                let store = Arc::clone(&store);
                thread::spawn(move || {
                    for _ in 0..ITERS {
                        let count: i64 = store
                            .with_conn(|c| {
                                c.query_row("SELECT COUNT(*) FROM dictations", [], |r| r.get(0))
                                    .map_err(StoreError::from)
                            })
                            .expect("concurrent count");
                        assert_eq!(count, 3, "concurrent reader saw inconsistent count");
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().expect("reader thread panicked");
        }
        // Soft 5 s ceiling — single-mutex serialization of 800 trivial
        // SELECTs should finish in tens of ms; anything close to the
        // ceiling indicates a real-world deadlock or contention bug.
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "8x100 concurrent reads exceeded 5 s budget — possible deadlock",
        );
    }

    #[test]
    fn dictations_insert_select_round_trip() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("openwhisper.db");
        let store = Store::open_or_init(&path).expect("open_or_init");

        store
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO dictations (started_at, duration_ms, word_count) VALUES (?, ?, ?)",
                    [1_000_i64, 2_500_i64, 7_i64],
                )
                .map(|_| ())
                .map_err(StoreError::from)
            })
            .expect("insert");

        let count: i64 = store
            .with_conn(|c| {
                c.query_row("SELECT COUNT(*) FROM dictations", [], |r| r.get(0))
                    .map_err(StoreError::from)
            })
            .expect("count");
        assert_eq!(count, 1);
    }
}
