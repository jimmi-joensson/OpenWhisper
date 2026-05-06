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

use chrono::{Duration as ChronoDuration, Local, NaiveTime, TimeZone};
use rusqlite::params;
use serde::Serialize;

use crate::store::{Store, StoreError};

static STORE: OnceLock<Arc<Store>> = OnceLock::new();
static ON_INSERT: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

/// Aggregate counters returned by [`get_summary`]. Today / week buckets
/// are computed against the user's local timezone so a dictation at
/// 11 PM and another at 1 AM both count to "today" and "yesterday"
/// respectively, regardless of UTC offset.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct StatsSummary {
    pub words_today: i64,
    pub words_week: i64,
    pub words_all_time: i64,
    /// Sum of `duration_ms` across all rows, in seconds. Frontend
    /// formats into Time Saved using the WPM setting.
    pub seconds_total: f64,
}

impl StatsSummary {
    pub fn empty() -> Self {
        Self {
            words_today: 0,
            words_week: 0,
            words_all_time: 0,
            seconds_total: 0.0,
        }
    }
}

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

/// Register a callback invoked after every successful insert and after
/// every successful [`reset`]. The shell wires this to
/// `app_handle.emit("stats_changed", ())` so the frontend can refetch
/// the summary. First-call-wins; mirrors [`set_store`].
///
/// Living here (rather than in the shell) keeps core unaware of Tauri
/// events while still giving the shell a single hook point for the
/// stats refresh signal.
pub fn set_on_insert(cb: Box<dyn Fn() + Send + Sync>) {
    let _ = ON_INSERT.set(cb);
}

fn fire_stats_changed() {
    if let Some(cb) = ON_INSERT.get() {
        cb();
    }
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
    match result {
        Ok(()) => fire_stats_changed(),
        Err(e) => eprintln!("[stats] record_dictation failed: {e}"),
    }
}

/// Local-time start-of-day for the given epoch ms, returned as epoch ms.
fn local_day_start_ms(now_ms: i64) -> i64 {
    let now = Local
        .timestamp_millis_opt(now_ms)
        .single()
        .unwrap_or_else(|| Local.timestamp_millis_opt(0).single().unwrap());
    let midnight = now
        .date_naive()
        .and_time(NaiveTime::MIN)
        .and_local_timezone(Local)
        .single()
        // Spring-forward DST gap on the user's local midnight: fall back
        // to `now` so today's bucket simply starts at the time we ran
        // the query — undercount by minutes once a year is acceptable.
        .unwrap_or(now);
    midnight.timestamp_millis()
}

/// Local-time start-of-day for (today − 6 days), returned as epoch ms.
/// Yields a 7-day rolling window aligned to local midnight.
fn local_week_start_ms(now_ms: i64) -> i64 {
    let day_start = local_day_start_ms(now_ms);
    day_start - ChronoDuration::days(6).num_milliseconds()
}

/// Read-side aggregator. `now_ms` is passed in (rather than read from
/// `SystemTime::now()` inside) so tests can pin time without monkey-
/// patching the clock.
pub fn get_summary(store: &Store, now_ms: i64) -> Result<StatsSummary, StoreError> {
    let day_start = local_day_start_ms(now_ms);
    let week_start = local_week_start_ms(now_ms);
    store.with_conn(|c| {
        let words_today: i64 = c
            .query_row(
                "SELECT COALESCE(SUM(word_count), 0) FROM dictations WHERE started_at >= ?",
                params![day_start],
                |r| r.get(0),
            )
            .map_err(StoreError::from)?;
        let words_week: i64 = c
            .query_row(
                "SELECT COALESCE(SUM(word_count), 0) FROM dictations WHERE started_at >= ?",
                params![week_start],
                |r| r.get(0),
            )
            .map_err(StoreError::from)?;
        let (words_all_time, total_ms): (i64, i64) = c
            .query_row(
                "SELECT COALESCE(SUM(word_count), 0), COALESCE(SUM(duration_ms), 0) FROM dictations",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(StoreError::from)?;
        Ok(StatsSummary {
            words_today,
            words_week,
            words_all_time,
            seconds_total: total_ms as f64 / 1000.0,
        })
    })
}

/// Wipe all rows. Fires the stats_changed callback on success.
pub fn reset(store: &Store) -> Result<(), StoreError> {
    store.with_conn(|c| {
        c.execute("DELETE FROM dictations", [])
            .map(|_| ())
            .map_err(StoreError::from)
    })?;
    fire_stats_changed();
    Ok(())
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

    fn insert_raw(store: &Store, started_at_ms: i64, duration_ms: i64, word_count: i64) {
        store
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO dictations (started_at, duration_ms, word_count) VALUES (?, ?, ?)",
                    params![started_at_ms, duration_ms, word_count],
                )
                .map(|_| ())
                .map_err(StoreError::from)
            })
            .expect("insert raw");
    }

    fn now_local_ms() -> i64 {
        Local::now().timestamp_millis()
    }

    #[test]
    fn get_summary_zero_rows_is_empty() {
        let (_d, store) = fresh_store();
        let summary = get_summary(&store, now_local_ms()).expect("summary");
        assert_eq!(summary, StatsSummary::empty());
    }

    #[test]
    fn get_summary_buckets_today_week_all_time() {
        let (_d, store) = fresh_store();
        let now_ms = now_local_ms();
        let day_start = local_day_start_ms(now_ms);
        // Today: two rows after local midnight.
        insert_raw(&store, day_start + 1_000, 2_000, 5);
        insert_raw(&store, day_start + 60_000, 3_000, 7);
        // Yesterday: one row before today's midnight but inside the
        // 7-day window.
        let yesterday = day_start - 60_000;
        insert_raw(&store, yesterday, 1_500, 4);
        // Year ago: outside both today and week windows.
        let year_ago = now_ms - ChronoDuration::days(365).num_milliseconds();
        insert_raw(&store, year_ago, 9_000, 11);

        let summary = get_summary(&store, now_ms).expect("summary");
        assert_eq!(summary.words_today, 12, "5 + 7");
        assert_eq!(summary.words_week, 16, "5 + 7 + 4");
        assert_eq!(summary.words_all_time, 27, "5 + 7 + 4 + 11");
        assert!(
            (summary.seconds_total - 15.5).abs() < 0.001,
            "total seconds = (2000 + 3000 + 1500 + 9000) / 1000 = 15.5; got {}",
            summary.seconds_total,
        );
    }

    #[test]
    fn reset_empties_table_and_summary() {
        let (_d, store) = fresh_store();
        let now_ms = now_local_ms();
        insert_raw(&store, now_ms, 1_000, 3);
        assert_eq!(row_count(&store), 1);
        reset(&store).expect("reset");
        assert_eq!(row_count(&store), 0);
        let summary = get_summary(&store, now_ms).expect("summary");
        assert_eq!(summary, StatsSummary::empty());
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
