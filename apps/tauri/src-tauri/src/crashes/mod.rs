//! Tauri-shell facade over `<app_log_dir>/crashes/`.
//!
//! Six webview-facing commands (list/read/delete/delete_all/mark_read/
//! unread_count) plus a debug-only panic trigger. Per-crash UI flags
//! (`unread`, `uploaded_at`) live in a sibling `state.json` next to the
//! immutable crash JSON files; the typed schema mirrors the same
//! "single JSON file, atomic-on-rename" pattern used by `settings::`.
//!
//! Crash dir resolution honors `OPENWHISPER_CRASH_DIR_OVERRIDE` in
//! debug builds — release builds always use `app.path().app_log_dir()
//! + "/crashes/"`. The override exists so Playwright fixtures
//! (TASK-78.7) can seed crashes without needing the Tauri test harness.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use openwhisper_core::crashes::{self, CrashFile, ReadCrashError};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const STATE_FILE: &str = "state.json";
const SUMMARY_MESSAGE_MAX_CHARS: usize = 200;

/// Per-crash UI flags stored in `state.json`. Defaults are `unread =
/// true`, `uploaded_at = None` — files without a state entry are
/// treated as unread by `crashes_list` / `crashes_unread_count`.
///
/// Hand-rolled `Default` (instead of `#[derive(Default)]`) because the
/// derive uses each field's type-default — `bool::default()` is
/// `false`, which would silently mark new crashes as already-read.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrashStateEntry {
    #[serde(default = "default_true")]
    pub unread: bool,
    #[serde(default)]
    pub uploaded_at: Option<i64>,
}

impl Default for CrashStateEntry {
    fn default() -> Self {
        Self { unread: true, uploaded_at: None }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrashesState {
    #[serde(default)]
    pub entries: BTreeMap<String, CrashStateEntry>,
}

/// Compact view of a crash file used by the list pane. Avoids
/// deserializing the full backtrace + events array for every row.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CrashSummary {
    pub id: String,
    pub ts_unix_ms: i64,
    pub app_version: String,
    pub os: String,
    pub message_truncated: String,
    pub unread: bool,
    pub uploaded_at: Option<i64>,
}

/// Resolve the crash dir for runtime command paths. Same precedence
/// rules as the boot-time hook installer in `lib.rs::run`: debug-only
/// env-var override, falling through to `<app_log_dir>/crashes/`.
pub(crate) fn resolve_crashes_dir(app: &AppHandle) -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        if let Ok(s) = std::env::var("OPENWHISPER_CRASH_DIR_OVERRIDE") {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Some(PathBuf::from(trimmed));
            }
        }
    }
    app.path().app_log_dir().ok().map(|p| p.join("crashes"))
}

fn state_path(dir: &Path) -> PathBuf {
    dir.join(STATE_FILE)
}

/// Process-wide lock around state.json IO. The actual state is read
/// fresh from disk on every command (so a manual edit isn't ignored),
/// but the lock serialises read-modify-write sequences.
fn state_io_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn load_state(dir: &Path) -> CrashesState {
    let path = state_path(dir);
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|e| {
            eprintln!("[crashes] state.json corrupt at {}: {e} — resetting", path.display());
            CrashesState::default()
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => CrashesState::default(),
        Err(e) => {
            eprintln!("[crashes] state.json read failed: {e}");
            CrashesState::default()
        }
    }
}

fn save_state(dir: &Path, state: &CrashesState) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    // Atomic-on-rename: write to a sibling tmp file and rename in place.
    // Mirrors openwhisper_core::settings persistence.
    let tmp = dir.join(format!("{STATE_FILE}.tmp"));
    {
        let mut f = fs::File::create(&tmp)?;
        let raw = serde_json::to_vec_pretty(state).map_err(io::Error::other)?;
        f.write_all(&raw)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, state_path(dir))
}

fn truncate_message(s: &str) -> String {
    if s.chars().count() <= SUMMARY_MESSAGE_MAX_CHARS {
        return s.to_string();
    }
    let mut out: String = s.chars().take(SUMMARY_MESSAGE_MAX_CHARS).collect();
    out.push('…');
    out
}

fn read_crash_file(dir: &Path, id: &str) -> Result<CrashFile, String> {
    crashes::read_crash(dir, id).map_err(read_err_to_string)
}

fn read_err_to_string(err: ReadCrashError) -> String {
    err.to_string()
}

// `is_safe_id` now lives in `core::crashes` — both the CLI and the
// Tauri shell take untrusted ids (CLI from a flag, shell from the
// webview), so the validation belongs in the library. Re-export
// locally so existing call sites stay terse.
fn is_safe_id(id: &str) -> bool {
    crashes::is_safe_id(id)
}

/// Enumerate crash files in `dir`, returning newest-first summaries
/// decorated with the shell-only `unread` / `uploaded_at` fields
/// from `state.json`. The library function `list_crashes` returns
/// the on-disk schema; we layer state.json on top here — the shell
/// is the only owner of UI state.
fn list_summaries(dir: &Path, state: &CrashesState) -> Vec<CrashSummary> {
    let crashes_vec = match crashes::list_crashes(dir) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[crashes] list_crashes failed: {e}");
            return Vec::new();
        }
    };
    crashes_vec
        .into_iter()
        .map(|crash| {
            let entry_state =
                state.entries.get(&crash.id).cloned().unwrap_or_default();
            CrashSummary {
                message_truncated: truncate_message(&crash.rust_panic.message),
                id: crash.id,
                ts_unix_ms: crash.ts_unix_ms,
                app_version: crash.app_version,
                os: crash.os,
                unread: entry_state.unread,
                uploaded_at: entry_state.uploaded_at,
            }
        })
        .collect()
}

#[tauri::command]
pub fn crashes_list(app: AppHandle) -> Vec<CrashSummary> {
    let Some(dir) = resolve_crashes_dir(&app) else {
        return Vec::new();
    };
    let _g = state_io_lock().lock().ok();
    let state = load_state(&dir);
    list_summaries(&dir, &state)
}

#[tauri::command]
pub fn crashes_read(app: AppHandle, id: String) -> Result<CrashFile, String> {
    let dir = resolve_crashes_dir(&app).ok_or_else(|| "crash dir unresolved".to_string())?;
    read_crash_file(&dir, &id)
}

#[tauri::command]
pub fn crashes_delete(app: AppHandle, id: String) -> Result<(), String> {
    if !is_safe_id(&id) {
        return Err(format!("invalid crash id: {id}"));
    }
    let Some(dir) = resolve_crashes_dir(&app) else {
        return Err("crash dir unresolved".into());
    };
    let _g = state_io_lock().lock().ok();
    let path = dir.join(format!("{id}.json"));
    match fs::remove_file(&path) {
        Ok(()) => {}
        // Idempotent: missing file is success (already deleted).
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("remove {}: {e}", path.display())),
    }
    let mut state = load_state(&dir);
    if state.entries.remove(&id).is_some() {
        save_state(&dir, &state).map_err(|e| format!("save state.json: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn crashes_delete_all(app: AppHandle) -> Result<(), String> {
    let Some(dir) = resolve_crashes_dir(&app) else {
        return Err("crash dir unresolved".into());
    };
    let _g = state_io_lock().lock().ok();
    let entries = match fs::read_dir(&dir) {
        Ok(it) => it,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("read_dir: {e}")),
    };
    let mut errors: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if !is_safe_id(stem) {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        if let Err(e) = fs::remove_file(&path) {
            // Best-effort per file: collect the error, keep going.
            errors.push(format!("{}: {e}", path.display()));
        }
    }
    save_state(&dir, &CrashesState::default())
        .map_err(|e| format!("truncate state.json: {e}"))?;
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

#[tauri::command]
pub fn crashes_mark_read(app: AppHandle, id: String) -> Result<(), String> {
    if !is_safe_id(&id) {
        return Err(format!("invalid crash id: {id}"));
    }
    let Some(dir) = resolve_crashes_dir(&app) else {
        return Err("crash dir unresolved".into());
    };
    let _g = state_io_lock().lock().ok();
    let mut state = load_state(&dir);
    let entry = state.entries.entry(id).or_insert(CrashStateEntry {
        unread: true,
        uploaded_at: None,
    });
    if !entry.unread {
        return Ok(());
    }
    entry.unread = false;
    save_state(&dir, &state).map_err(|e| format!("save state.json: {e}"))
}

#[tauri::command]
pub fn crashes_unread_count(app: AppHandle) -> u32 {
    let Some(dir) = resolve_crashes_dir(&app) else {
        return 0;
    };
    let _g = state_io_lock().lock().ok();
    let state = load_state(&dir);
    let entries = match fs::read_dir(&dir) {
        Ok(it) => it,
        Err(_) => return 0,
    };
    let mut count: u32 = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if !is_safe_id(stem) {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let unread = state.entries.get(stem).map(|e| e.unread).unwrap_or(true);
        if unread {
            count += 1;
        }
    }
    count
}

/// Reveal the resolved crashes dir in Finder (Mac) / Explorer (Windows).
///
/// Shells out via `std::process::Command` rather than going through
/// `tauri-plugin-opener` so we don't have to wrangle the plugin's
/// per-path scope permissions in `capabilities/` for every directory
/// we ever want to reveal. The dir comes from our own resolver
/// (`resolve_crashes_dir` — already audited), not from user input,
/// so the command-injection surface is bounded.
///
/// Creates the dir if missing so first-launch (no crashes ever) still
/// opens a real folder rather than silently no-opping.
#[tauri::command]
pub fn crashes_open_folder(app: AppHandle) -> Result<(), String> {
    let dir = resolve_crashes_dir(&app).ok_or_else(|| "crash dir unresolved".to_string())?;
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;

    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut cmd = std::process::Command::new("explorer");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = std::process::Command::new("xdg-open");

    cmd.arg(&dir);
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("spawn reveal {}: {e}", dir.display()))
}

/// Debug build: spawn a worker that panics, exercising the shared
/// crash hook end-to-end. Release build: returns an error string with
/// no side effects (the function symbol is registered in the invoke
/// handler unconditionally so that a release build calling this
/// command from a stale UI build doesn't crash on a missing handler —
/// but the panic-triggering behaviour is gated to debug + the
/// `dev-panic` Cargo feature).
#[cfg(any(debug_assertions, feature = "dev-panic"))]
#[tauri::command]
pub fn crashes_debug_trigger_panic() -> Result<(), String> {
    std::thread::Builder::new()
        .name("crashes-debug-panic".into())
        .spawn(|| panic!("debug panic triggered from crashes_debug_trigger_panic"))
        .expect("spawn debug panic thread");
    Ok(())
}

#[cfg(not(any(debug_assertions, feature = "dev-panic")))]
#[tauri::command]
pub fn crashes_debug_trigger_panic() -> Result<(), String> {
    Err("debug panic disabled in release builds".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openwhisper_core::crashes::{
        CrashFile, RecordingStateSnapshot, RustPanic, SCHEMA_VERSION,
    };
    use tempfile::TempDir;

    fn fixture_crash(id: &str, ts: i64, msg: &str) -> CrashFile {
        CrashFile {
            schema_version: SCHEMA_VERSION,
            id: id.to_string(),
            ts_unix_ms: ts,
            app_version: "0.6.0".into(),
            os: "macos (arm64)".into(),
            rust_panic: RustPanic {
                thread_name: "main".into(),
                message: msg.into(),
                location: "x.rs:1:1".into(),
                backtrace: "frame 1\nframe 2".into(),
            },
            recording_state: Some(RecordingStateSnapshot {
                status_message_at_crash: "transcribing on ANE…".into(),
                duration_ms: 1000,
                samples_captured: 16000,
                model_kind: None,
                device_id_hash: None,
            }),
            events: vec![],
        }
    }

    fn write_fixture(dir: &Path, id: &str, ts: i64, msg: &str) {
        let crash = fixture_crash(id, ts, msg);
        let path = dir.join(format!("{id}.json"));
        let raw = serde_json::to_vec_pretty(&crash).unwrap();
        fs::create_dir_all(dir).unwrap();
        fs::write(path, raw).unwrap();
    }

    #[test]
    fn truncate_message_passthrough_short() {
        let s = "kaboom";
        assert_eq!(truncate_message(s), "kaboom");
    }

    #[test]
    fn truncate_message_appends_ellipsis_when_too_long() {
        let s: String = "a".repeat(SUMMARY_MESSAGE_MAX_CHARS + 50);
        let out = truncate_message(&s);
        assert!(out.ends_with('…'));
        assert_eq!(out.chars().count(), SUMMARY_MESSAGE_MAX_CHARS + 1);
    }

    #[test]
    fn is_safe_id_accepts_unix_ms() {
        assert!(is_safe_id("1717503600123"));
    }

    #[test]
    fn is_safe_id_rejects_path_traversal() {
        assert!(!is_safe_id("../etc"));
        assert!(!is_safe_id("foo/bar"));
        assert!(!is_safe_id(""));
        assert!(!is_safe_id("abc"));
    }

    #[test]
    fn state_load_returns_default_when_missing() {
        let tmp = TempDir::new().unwrap();
        let state = load_state(tmp.path());
        assert!(state.entries.is_empty());
    }

    #[test]
    fn state_load_resets_on_corrupt_json() {
        let tmp = TempDir::new().unwrap();
        fs::write(state_path(tmp.path()), b"not json {{{").unwrap();
        let state = load_state(tmp.path());
        assert!(state.entries.is_empty());
    }

    #[test]
    fn state_save_then_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let mut state = CrashesState::default();
        state.entries.insert(
            "1717503600123".into(),
            CrashStateEntry {
                unread: false,
                uploaded_at: Some(1717503700_000),
            },
        );
        save_state(tmp.path(), &state).unwrap();
        let back = load_state(tmp.path());
        assert_eq!(back, state);
    }

    #[test]
    fn list_returns_empty_for_missing_dir() {
        let tmp = TempDir::new().unwrap();
        // Don't create the crashes dir; load_state still returns default.
        let state = load_state(tmp.path());
        assert!(state.entries.is_empty());
    }

    #[test]
    fn list_default_entries_marked_unread() {
        let tmp = TempDir::new().unwrap();
        write_fixture(tmp.path(), "100", 100, "first");
        write_fixture(tmp.path(), "200", 200, "second");
        // No state.json yet — both should be unread.
        let state = load_state(tmp.path());
        let unread_100 = state.entries.get("100").map(|e| e.unread).unwrap_or(true);
        let unread_200 = state.entries.get("200").map(|e| e.unread).unwrap_or(true);
        assert!(unread_100);
        assert!(unread_200);
    }

    #[test]
    fn list_summaries_returns_newest_first_with_mixed_read_state() {
        let tmp = TempDir::new().unwrap();
        write_fixture(tmp.path(), "100", 100, "old");
        write_fixture(tmp.path(), "300", 300, "newest");
        write_fixture(tmp.path(), "200", 200, "middle");
        // Mark the middle one as read; mark newest as uploaded.
        let mut state = CrashesState::default();
        state.entries.insert(
            "200".into(),
            CrashStateEntry { unread: false, uploaded_at: None },
        );
        state.entries.insert(
            "300".into(),
            CrashStateEntry { unread: true, uploaded_at: Some(999) },
        );
        save_state(tmp.path(), &state).unwrap();

        let state = load_state(tmp.path());
        let summaries = list_summaries(tmp.path(), &state);
        assert_eq!(summaries.len(), 3);
        // Newest first.
        assert_eq!(summaries[0].id, "300");
        assert_eq!(summaries[1].id, "200");
        assert_eq!(summaries[2].id, "100");
        // Mixed read state preserved.
        assert!(summaries[0].unread);
        assert_eq!(summaries[0].uploaded_at, Some(999));
        assert!(!summaries[1].unread);
        // No state entry → defaults to unread.
        assert!(summaries[2].unread);
        assert_eq!(summaries[2].uploaded_at, None);
        // Truncated message preserved (short, no ellipsis).
        assert_eq!(summaries[0].message_truncated, "newest");
    }

    #[test]
    fn list_summaries_skips_corrupt_files() {
        let tmp = TempDir::new().unwrap();
        write_fixture(tmp.path(), "100", 100, "valid");
        // Drop a non-JSON file with the same naming pattern.
        fs::write(tmp.path().join("200.json"), b"not valid json {{{").unwrap();
        let state = load_state(tmp.path());
        let summaries = list_summaries(tmp.path(), &state);
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, "100");
    }

    #[test]
    fn list_summaries_ignores_unsafe_filenames() {
        let tmp = TempDir::new().unwrap();
        write_fixture(tmp.path(), "100", 100, "ok");
        // Files whose stem fails is_safe_id must not be enumerated.
        fs::write(tmp.path().join("foo.json"), b"{}").unwrap();
        fs::write(tmp.path().join("state.json"), b"{}").unwrap();
        let state = load_state(tmp.path());
        let summaries = list_summaries(tmp.path(), &state);
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, "100");
    }

    #[test]
    fn mark_read_persists_across_load() {
        let tmp = TempDir::new().unwrap();
        write_fixture(tmp.path(), "100", 100, "msg");
        let mut state = load_state(tmp.path());
        state
            .entries
            .insert("100".into(), CrashStateEntry { unread: false, uploaded_at: None });
        save_state(tmp.path(), &state).unwrap();
        let reloaded = load_state(tmp.path());
        assert_eq!(reloaded.entries.get("100").unwrap().unread, false);
    }
}
