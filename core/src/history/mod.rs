//! Transcript history (TASK-95): the text of past dictations, kept on
//! this machine only, and only after the user switches it on
//! ([`crate::settings::HistorySettings`]).
//!
//! History is not a table of its own. Every dictation already gets a
//! row in `dictations` for the stats; with history on, that row also
//! holds the final text, the text before Refine changed it, and the
//! app the text went into (see [`crate::stats::record`]). With history
//! off those columns stay NULL, exactly as before this feature.
//!
//! Removing text (one entry, all of it, or by age) sets those columns
//! back to NULL and keeps the row, so word counts and Time Saved do
//! not change. The store opens SQLite with `secure_delete`, so the
//! removed text is overwritten in the file.
//!
//! The same functions back the desktop app's Home list and
//! `openwhisper history`.

use rusqlite::{Row, params};
use serde::Serialize;

use crate::settings::HistorySettings;
use crate::store::{Store, StoreError};

/// Most entries one [`list`] call returns.
pub const MAX_PAGE: u32 = 500;

/// One stored dictation.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct HistoryEntry {
    pub id: i64,
    /// When the recording started, Unix epoch milliseconds.
    pub at_ms: i64,
    /// Voiced duration of the recording.
    pub duration_ms: i64,
    pub word_count: i64,
    /// The text that was inserted.
    pub text: String,
    /// The transcript before Refine, present only when Refine changed
    /// it.
    pub original: Option<String>,
    /// See [`crate::focus::FocusContext::app_id`]. `None` when the
    /// app was not known (or the text only went to the clipboard).
    pub app_id: Option<String>,
    pub app_name: Option<String>,
}

/// A slice of the history, newest first.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct HistoryPage {
    pub entries: Vec<HistoryEntry>,
    /// Entries matching the search (all stored entries when the
    /// search is empty), before `limit` and `offset`.
    pub matched: i64,
    /// Entries stored in total.
    pub stored: i64,
}

const COLUMNS: &str =
    "id, started_at, duration_ms, word_count, transcript, raw_transcript, app_bundle_id, app_name";

fn entry_from_row(r: &Row<'_>) -> rusqlite::Result<HistoryEntry> {
    Ok(HistoryEntry {
        id: r.get(0)?,
        at_ms: r.get(1)?,
        duration_ms: r.get(2)?,
        word_count: r.get(3)?,
        text: r.get(4)?,
        original: r.get(5)?,
        app_id: r.get(6)?,
        app_name: r.get(7)?,
    })
}

fn matches(entry: &HistoryEntry, needle: &str) -> bool {
    let has = |s: &str| s.to_lowercase().contains(needle);
    has(&entry.text)
        || entry.original.as_deref().is_some_and(has)
        || entry.app_name.as_deref().is_some_and(has)
}

/// Stored entries, newest first. `search` is matched without regard
/// to case against the text, the original text and the app name; an
/// empty search returns everything. `limit` is capped at [`MAX_PAGE`].
///
/// The search runs here and not in SQL because SQLite's `LIKE` only
/// folds ASCII case, which is wrong for Danish text.
pub fn list(store: &Store, search: &str, limit: u32, offset: u32) -> Result<HistoryPage, StoreError> {
    let needle = search.trim().to_lowercase();
    let limit = limit.clamp(1, MAX_PAGE);
    store.with_conn(|c| {
        let stored: i64 = c.query_row(
            "SELECT COUNT(*) FROM dictations WHERE transcript IS NOT NULL",
            [],
            |r| r.get(0),
        )?;
        if needle.is_empty() {
            let mut stmt = c.prepare(&format!(
                "SELECT {COLUMNS} FROM dictations WHERE transcript IS NOT NULL \
                 ORDER BY started_at DESC, id DESC LIMIT ? OFFSET ?"
            ))?;
            let entries = stmt
                .query_map(params![limit, offset], entry_from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            return Ok(HistoryPage { entries, matched: stored, stored });
        }
        let mut stmt = c.prepare(&format!(
            "SELECT {COLUMNS} FROM dictations WHERE transcript IS NOT NULL \
             ORDER BY started_at DESC, id DESC"
        ))?;
        let hits: Vec<HistoryEntry> = stmt
            .query_map([], entry_from_row)?
            .filter(|row| row.as_ref().map_or(true, |e| matches(e, &needle)))
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let matched = hits.len() as i64;
        let entries = hits
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();
        Ok(HistoryPage { entries, matched, stored })
    })
}

/// One stored entry. `None` when there is no such dictation or its
/// text is not stored.
pub fn get(store: &Store, id: i64) -> Result<Option<HistoryEntry>, StoreError> {
    store.with_conn(|c| {
        let mut stmt = c.prepare(&format!(
            "SELECT {COLUMNS} FROM dictations WHERE id = ? AND transcript IS NOT NULL"
        ))?;
        let mut rows = stmt.query_map(params![id], entry_from_row)?;
        Ok(rows.next().transpose()?)
    })
}

const FORGET: &str = "UPDATE dictations \
     SET transcript = NULL, raw_transcript = NULL, app_bundle_id = NULL, app_name = NULL";

/// Remove one entry's text. The stats row stays. `false` when there
/// was nothing to remove.
pub fn delete(store: &Store, id: i64) -> Result<bool, StoreError> {
    let changed = store.with_conn(|c| {
        Ok(c.execute(
            &format!("{FORGET} WHERE id = ? AND transcript IS NOT NULL"),
            params![id],
        )?)
    })?;
    if changed > 0 {
        crate::stats::notify_changed();
    }
    Ok(changed > 0)
}

/// Remove all stored text. The stats rows stay. Returns how many
/// entries were removed.
pub fn clear(store: &Store) -> Result<usize, StoreError> {
    let changed = store.with_conn(|c| {
        Ok(c.execute(&format!("{FORGET} WHERE transcript IS NOT NULL"), [])?)
    })?;
    if changed > 0 {
        crate::stats::notify_changed();
    }
    Ok(changed)
}

/// Remove text older than the retention in `settings`. Applies
/// whether history is on or off: switching history off must not
/// freeze old text in place. Returns how many entries were removed.
pub fn prune(store: &Store, settings: &HistorySettings, now_ms: i64) -> Result<usize, StoreError> {
    let Some(cutoff) = settings.cutoff_ms(now_ms) else {
        return Ok(0);
    };
    let changed = store.with_conn(|c| {
        Ok(c.execute(
            &format!("{FORGET} WHERE started_at < ? AND transcript IS NOT NULL"),
            params![cutoff],
        )?)
    })?;
    if changed > 0 {
        crate::stats::notify_changed();
    }
    Ok(changed)
}

/// Unix epoch milliseconds now.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::{DictationRecord, HistoryFields, record};
    use tempfile::tempdir;

    const DAY: i64 = 24 * 60 * 60 * 1000;

    fn fresh_store() -> (tempfile::TempDir, Store) {
        let dir = tempdir().expect("tempdir");
        let store = Store::open_or_init(&dir.path().join("openwhisper.db")).expect("open");
        (dir, store)
    }

    fn add(store: &Store, at_ms: i64, text: &str, original: Option<&str>, app: (&str, &str)) {
        record(
            store,
            &DictationRecord {
                started_at_ms: at_ms,
                duration_ms: 1000,
                wall_clock_ms: 1200,
                text,
                history: Some(HistoryFields { original, app_id: app.0, app_name: app.1 }),
            },
        );
    }

    fn add_without_history(store: &Store, at_ms: i64, text: &str) {
        record(
            store,
            &DictationRecord {
                started_at_ms: at_ms,
                duration_ms: 1000,
                wall_clock_ms: 1200,
                text,
                history: None,
            },
        );
    }

    fn words_all_time(store: &Store) -> i64 {
        crate::stats::get_summary(store, 100 * DAY).unwrap().words_all_time
    }

    #[test]
    fn lists_newest_first_and_skips_rows_without_text() {
        let (_d, store) = fresh_store();
        add(&store, 1000, "first one ", None, ("com.example.notes", "Notes"));
        add_without_history(&store, 2000, "not stored at all");
        add(&store, 3000, "third one ", Some("third won "), ("", ""));

        let page = list(&store, "", 50, 0).unwrap();
        assert_eq!(page.stored, 2);
        assert_eq!(page.matched, 2);
        let texts: Vec<&str> = page.entries.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, ["third one", "first one"]);

        let newest = &page.entries[0];
        assert_eq!(newest.original.as_deref(), Some("third won"));
        assert_eq!(newest.app_id, None);
        assert_eq!(newest.app_name, None);
        assert_eq!(newest.word_count, 2);

        let oldest = &page.entries[1];
        assert_eq!(oldest.original, None);
        assert_eq!(oldest.app_id.as_deref(), Some("com.example.notes"));
        assert_eq!(oldest.app_name.as_deref(), Some("Notes"));
    }

    #[test]
    fn original_is_kept_only_when_it_differs() {
        let (_d, store) = fresh_store();
        add(&store, 1000, "same text ", Some(" same text"), ("", ""));
        let page = list(&store, "", 50, 0).unwrap();
        assert_eq!(page.entries[0].original, None);
    }

    #[test]
    fn pages_with_limit_and_offset() {
        let (_d, store) = fresh_store();
        for i in 0..5 {
            add(&store, 1000 + i, &format!("entry {i}"), None, ("", ""));
        }
        let page = list(&store, "", 2, 1).unwrap();
        assert_eq!(page.matched, 5);
        let texts: Vec<&str> = page.entries.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, ["entry 3", "entry 2"]);
    }

    #[test]
    fn search_ignores_case_including_danish_letters() {
        let (_d, store) = fresh_store();
        add(&store, 1000, "Vi ses på Ærø i morgen", None, ("", ""));
        add(&store, 2000, "Send the report", Some("send the reaport"), ("", ""));
        add(&store, 3000, "Nothing to see", None, ("com.example.chat", "Chatter"));

        let hit = |q: &str| -> Vec<String> {
            list(&store, q, 50, 0).unwrap().entries.into_iter().map(|e| e.text).collect()
        };
        assert_eq!(hit("ærø"), ["Vi ses på Ærø i morgen"]);
        assert_eq!(hit("  REPORT "), ["Send the report"]);
        // The original text and the app name are searched too.
        assert_eq!(hit("reaport"), ["Send the report"]);
        assert_eq!(hit("chatter"), ["Nothing to see"]);
        assert!(hit("no such words").is_empty());

        let page = list(&store, "the", 50, 0).unwrap();
        assert_eq!(page.matched, 1);
        assert_eq!(page.stored, 3);
    }

    #[test]
    fn search_pages_over_the_matches() {
        let (_d, store) = fresh_store();
        for i in 0..4 {
            add(&store, 1000 + i, &format!("match {i}"), None, ("", ""));
            add(&store, 2000 + i, &format!("other {i}"), None, ("", ""));
        }
        let page = list(&store, "match", 2, 2).unwrap();
        assert_eq!(page.matched, 4);
        let texts: Vec<&str> = page.entries.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, ["match 1", "match 0"]);
    }

    #[test]
    fn get_returns_only_stored_text() {
        let (_d, store) = fresh_store();
        add(&store, 1000, "kept ", None, ("", ""));
        add_without_history(&store, 2000, "never stored");
        let id = list(&store, "", 1, 0).unwrap().entries[0].id;
        assert_eq!(get(&store, id).unwrap().unwrap().text, "kept");
        assert_eq!(get(&store, id + 1).unwrap(), None);
        assert_eq!(get(&store, 9999).unwrap(), None);
    }

    #[test]
    fn delete_removes_text_and_keeps_the_stats_row() {
        let (_d, store) = fresh_store();
        add(&store, 1000, "one two three ", Some("won two three"), ("a.b", "AB"));
        add(&store, 2000, "four five ", None, ("", ""));
        let before = words_all_time(&store);
        let id = list(&store, "one", 1, 0).unwrap().entries[0].id;

        assert!(delete(&store, id).unwrap());
        assert!(!delete(&store, id).unwrap(), "second delete finds nothing");

        let page = list(&store, "", 50, 0).unwrap();
        assert_eq!(page.stored, 1);
        assert_eq!(page.entries[0].text, "four five");
        assert_eq!(words_all_time(&store), before);

        // Every history column of the removed entry is empty.
        let leftovers: i64 = store
            .with_conn(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM dictations WHERE id = ? AND (transcript IS NOT NULL \
                     OR raw_transcript IS NOT NULL OR app_bundle_id IS NOT NULL OR app_name IS NOT NULL)",
                    params![id],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn clear_removes_all_text_and_keeps_stats() {
        let (_d, store) = fresh_store();
        add(&store, 1000, "one two ", None, ("", ""));
        add(&store, 2000, "three ", None, ("", ""));
        add_without_history(&store, 3000, "four five six");
        let before = words_all_time(&store);

        assert_eq!(clear(&store).unwrap(), 2);
        assert_eq!(clear(&store).unwrap(), 0);
        assert_eq!(list(&store, "", 50, 0).unwrap().stored, 0);
        assert_eq!(words_all_time(&store), before);
    }

    #[test]
    fn prune_removes_text_older_than_retention() {
        let (_d, store) = fresh_store();
        let now = 100 * DAY;
        add(&store, now - 40 * DAY, "old ", None, ("", ""));
        add(&store, now - 8 * DAY, "middle ", None, ("", ""));
        add(&store, now - DAY, "new ", None, ("", ""));
        let before = words_all_time(&store);

        let thirty = HistorySettings { enabled: true, retention_days: 30 };
        assert_eq!(prune(&store, &thirty, now).unwrap(), 1);
        assert_eq!(list(&store, "", 50, 0).unwrap().stored, 2);

        // Retention still applies while history is switched off.
        let seven_off = HistorySettings { enabled: false, retention_days: 7 };
        assert_eq!(prune(&store, &seven_off, now).unwrap(), 1);
        let left: Vec<String> =
            list(&store, "", 50, 0).unwrap().entries.into_iter().map(|e| e.text).collect();
        assert_eq!(left, ["new"]);
        assert_eq!(words_all_time(&store), before);
    }

    #[test]
    fn prune_keeps_everything_when_retention_is_forever() {
        let (_d, store) = fresh_store();
        add(&store, 1000, "ancient ", None, ("", ""));
        let forever = HistorySettings { enabled: true, retention_days: 0 };
        assert_eq!(prune(&store, &forever, 1000 * DAY).unwrap(), 0);
        assert_eq!(list(&store, "", 50, 0).unwrap().stored, 1);
    }

    #[test]
    fn removed_text_is_not_left_in_the_file() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("openwhisper.db");
        let marker = "zebra-quartz-marker";
        {
            let store = Store::open_or_init(&path).expect("open");
            // Enough text to fill several pages, so removal frees some.
            for i in 0..200 {
                add(&store, 1000 + i, &format!("{marker} entry {i} {}", "word ".repeat(40)), None, ("", ""));
            }
            assert_eq!(clear(&store).unwrap(), 200);
        }
        let bytes = std::fs::read(&path).expect("read db file");
        let found = bytes.windows(marker.len()).any(|w| w == marker.as_bytes());
        assert!(!found, "cleared transcript text is still readable in the database file");
    }
}
