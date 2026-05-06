//! Dictation stats — write-side and (in TASK-88.2) read-side aggregates.
//!
//! The shell registers a single [`Store`] handle at boot via [`set_store`]
//! (mirrors `dictation::set_injector`). Once registered, any path that has
//! a successful transcript ready calls [`record_dictation`] with the
//! recording's wall-clock start, duration, and text. Empty text inserts
//! nothing — the dictation flow already short-circuits empty paths but
//! the writer guards too in case a future caller forgets.
//!
//! Failures here log to stderr and return — they MUST NOT panic and MUST
//! NOT propagate, because [`crate::dictation::dictation_deliver_transcript`]
//! invokes us after setting `PHASE_DONE`. Any error that surfaced would
//! need to either roll the phase back to `PHASE_ERROR` (mis-blaming the
//! recognizer) or be silently swallowed by the call site. Swallowing
//! here, at the source, keeps the contract local.

use std::sync::{Arc, OnceLock};

use rusqlite::params;

use crate::store::{Store, StoreError};

static STORE: OnceLock<Arc<Store>> = OnceLock::new();

/// Register the persistence handle. First call wins — subsequent calls
/// are silently dropped, matching the `INJECTOR` pattern in `dictation`.
/// The shell calls this once during Tauri's `setup` hook after
/// `Store::open_or_init` succeeds.
pub fn set_store(store: Arc<Store>) {
    let _ = STORE.set(store);
}

/// Return the registered store, if any. Used by the dictation call site
/// to decide whether to attempt a write.
pub fn store() -> Option<&'static Arc<Store>> {
    STORE.get()
}

/// Record one successful dictation. Empty / whitespace-only text returns
/// without inserting (mirrors the dictation flow's empty-sample drain).
/// Word count is whitespace-split on the trimmed text. The transcript
/// itself is NOT persisted today — `transcript` stays NULL until the
/// history opt-in lands; only counters and timing are kept.
pub fn record_dictation(store: &Store, started_at_ms: i64, duration_ms: i64, text: &str) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    let word_count = trimmed.split_whitespace().count() as i64;
    let result = store.with_conn(|c| {
        c.execute(
            "INSERT INTO dictations \
             (started_at, duration_ms, word_count, transcript, confidence, app_bundle_id) \
             VALUES (?, ?, ?, NULL, NULL, NULL)",
            params![started_at_ms, duration_ms, word_count],
        )
        .map(|_| ())
        .map_err(StoreError::from)
    });
    if let Err(e) = result {
        eprintln!("[stats] record_dictation failed: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn fresh_store() -> (tempfile::TempDir, Store) {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("openwhisper.db");
        let store = Store::open_or_init(&path).expect("open_or_init");
        (dir, store)
    }

    fn row_count(store: &Store) -> i64 {
        store
            .with_conn(|c| {
                c.query_row("SELECT COUNT(*) FROM dictations", [], |r| r.get(0))
                    .map_err(StoreError::from)
            })
            .expect("count")
    }

    #[test]
    fn empty_text_inserts_nothing() {
        let (_d, store) = fresh_store();
        record_dictation(&store, 1_000, 500, "");
        record_dictation(&store, 1_000, 500, "   \t\n  ");
        assert_eq!(row_count(&store), 0);
    }

    #[test]
    fn writes_row_with_word_count() {
        let (_d, store) = fresh_store();
        record_dictation(&store, 1_700_000_000_000, 2_500, "hello world from openwhisper");
        let (started, duration, words, transcript): (i64, i64, i64, Option<String>) = store
            .with_conn(|c| {
                c.query_row(
                    "SELECT started_at, duration_ms, word_count, transcript FROM dictations",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .map_err(StoreError::from)
            })
            .expect("select row");
        assert_eq!(started, 1_700_000_000_000);
        assert_eq!(duration, 2_500);
        assert_eq!(words, 4);
        assert!(transcript.is_none(), "transcript stays NULL until history opt-in");
    }

    #[test]
    fn db_failure_does_not_panic() {
        // Build a Store whose connection has no `dictations` table —
        // every INSERT fails. The test-only constructor bypasses
        // `open_or_init` so migrations never run.
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory");
        let store = Store::from_connection_for_test(conn);
        record_dictation(&store, 1_000, 500, "hello world");
        // No panic = test passes; the eprintln is observable in test
        // output but not asserted.
    }
}
