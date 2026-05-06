---
id: doc-39
title: Persistence foundation — design
type: specification
created_date: '2026-05-06 06:06'
---

**Backlog parent:** TASK-87
**Research basis:** doc-34 (Handy audit), doc-35 (OpenWhispr audit), doc-36 (three-way synthesis). The data-layer recommendation was researched in chat (Anthropic agent transcript 2026-05-06) and locked by the user before this spec was written.

## Problem

OpenWhisper has no on-disk persistence layer for dictation events. The settings store (hand-rolled JSON) covers preferences, but anything event-shaped — counts, durations, transcripts when history ships — has nowhere to go. The first feature that needs this is the Home-pane stats strip (TASK-88). Future features that will land on the same layer: dictation history list, transcript search, optional cloud sync. Building a one-off JSON counter for stats now would force a rewrite the day history ships.

## Goal

Add a thin SQLite-backed persistence layer that owns one table today (`dictations`) and is shaped so future features layer onto the same row without schema rewrite.

Stack:

- **rusqlite** with the `bundled` feature (links a copy of SQLite into the binary, no platform variance, ~1 MB cost vs Parakeet's ~66 MB — negligible).
- **rusqlite_migration** for ordered, idempotent schema migrations.
- Single DB file, single connection per process, sync API. No async runtime, no ORM, no codegen.

## Non-goals

- **Settings do not move into SQLite.** The existing JSON settings store stays. Reasoning lived in the chat: boot-order coupling, hand-editability, schema rigidity for fluid keys, lock contention, and "settings vs event data are different shapes."
- **No FTS5 yet.** Transcript search is a future history feature, not a v1 surface.
- **No sqlite-vec yet.** Semantic search is a "maybe v2" item.
- **No SQLCipher / encryption at rest.** No secrets in v1; revisit if/when history opt-in flips.
- **No cloud sync.** v2 subscription-tier item (doc-36 §B7).
- **No Tauri-side SQL plugin.** That puts DB access in the React shell, which violates the `openwhisper-orchestration-in-rust` skill.

## Behavior model

### File location

The DB lives at `<app_data_dir>/openwhisper.db` where `<app_data_dir>` is Tauri's `app.path().app_data_dir()`:

- **macOS:** `~/Library/Application Support/com.openwhisper.app/openwhisper.db`
- **Windows:** `%APPDATA%\com.openwhisper.app\openwhisper.db` (Roaming)

These directories survive app update / overwrite, manual `.app` trash on Mac, and standard MSI uninstall on Windows (Tauri's WiX template does not delete `%APPDATA%` on uninstall). Reinstall finds the existing file. **The bundle identifier `com.openwhisper.app` MUST stay stable across the TASK-85 rename sweep.** Only the display name + product name + repo + domain change. If the identifier ever changes, every existing user's data is orphaned. This is a load-bearing constraint and must be enforced wherever the rename touches `tauri.conf.json` or the WiX configuration.

The dev build uses identifier `com.openwhisper.app.dev` (per `tauri.dev.conf.json`) and will create its own DB at `~/Library/Application Support/com.openwhisper.app.dev/openwhisper.db`. Dev and prod data stay isolated, which is the desired behavior — no risk of dev experiments polluting prod stats.

### Module shape

Lives in the `core/` crate at `core/src/store/mod.rs` (new module). Public API:

```rust
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    /// Open or create the DB at `path` and run all pending migrations.
    /// Idempotent — safe to call multiple times across processes
    /// (file-locked via SQLite's own locking).
    pub fn open_or_init(path: &Path) -> Result<Self, StoreError>;

    /// Borrow the connection inside a closure. Holds the mutex for the
    /// duration; closures must be short.
    pub fn with_conn<R>(&self, f: impl FnOnce(&Connection) -> Result<R, StoreError>) -> Result<R, StoreError>;
}

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    Migration(rusqlite_migration::Error),
}
```

The `Mutex<Connection>` is intentional: SQLite's C library is not safe to use from multiple threads on a single connection (without serialized mode), and OW is single-process anyway. Holding a mutex per call is fine — every site is short (a single INSERT or a single SELECT-aggregate). If contention ever shows up in profiling, swap for a connection pool — but `r2d2_sqlite` is overkill until then.

### Migrations

Use `rusqlite_migration::Migrations` with a static `&[M]` array. Migration 1 (this task):

```sql
CREATE TABLE dictations (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at    INTEGER NOT NULL,                              -- unix epoch ms, UTC
    duration_ms   INTEGER NOT NULL,
    word_count    INTEGER NOT NULL,
    transcript    TEXT,                                          -- NULL until history opt-in lands
    confidence    REAL,
    app_bundle_id TEXT,
    created_at    INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);
CREATE INDEX idx_dictations_started_at ON dictations(started_at);
```

Future migrations land as additional `M::up(...)` entries appended to the array. The crate's `to_latest()` runs only the unapplied ones based on the `user_version` pragma. **Never edit a migration that has shipped** — only append. This is the same discipline OpenWhispr's `services/SyncService.ts` follows (per doc-35).

### Tauri startup wiring

Tauri shell calls `Store::open_or_init(app_data_dir.join("openwhisper.db"))` once during the `setup` hook in `apps/tauri/src-tauri/src/lib.rs`. The resulting `Store` is registered as managed state via `app.manage(store)`. Failure paths:

- **Path resolution failure** (impossible in practice but typed): logged at `error!`, app continues without persistence; subsequent stats writes silently no-op (graceful degradation, no panic).
- **File creation / permission failure** (e.g. read-only home): same. The app is still usable for dictation; stats just don't accumulate.
- **Migration failure** (e.g. corrupt DB from a partial old write): same — log and continue. v2 may add a "reset stats DB" recovery path; v1 keeps it simple.

The `dictation_deliver_error` channel is NOT used — store init failure is not a recognizer error and shouldn't put the dictation state machine in PHASE_ERROR. A dedicated `tracing::error!` plus a one-time DevTools-visible warning is enough.

### Concurrency model

Single connection guarded by a mutex. All writes happen on whatever thread `dictation_deliver_transcript` runs on (the dictation worker). Reads from Tauri commands run on the Tauri command thread pool. Mutex contention will be invisible — writes are sub-ms (one INSERT, no transaction needed for a single row), reads aggregate a small table.

### Backup & restore

Out of scope for this task. The DB file is at a known path and SQLite files are byte-stable across machines/architectures, so a future backup feature is just "copy the file." No design decisions need to be made now to keep that door open.

## Why these choices

**rusqlite over sqlx.** OW is a single-user single-process desktop app. Async + compile-time query checking + multi-backend abstraction are sqlx's strengths and they buy nothing here. rusqlite is a thinner wrapper, simpler dev loop, no compile-time DB requirement. The cost of swapping later is a regex over `conn.execute(...)` calls — bounded.

**Bundled SQLite over system.** Removes platform variance (older Linux SQLites lack JSON1 / FTS5 support, Windows doesn't ship one at all). 1 MB binary cost is invisible next to Parakeet.

**rusqlite_migration over hand-rolled.** Three lines of setup, ordered M0 → M1 → ... ladder, idempotent via `user_version`. Avoids the mistake every SQLite project makes the second time it ships a schema change.

**`Mutex<Connection>` over r2d2 pool.** Connection-per-call has setup overhead; pool needs configuration (min/max, idle timeout). One mutex is the simplest thing that handles single-process desktop correctly.

**Failure paths log instead of panic.** Stats are not load-bearing for the core dictation flow. Bricking dictation because the DB is read-only is a worse outcome than silently dropping stats.

## Risks

- **Bundle-identifier drift via TASK-85.** Spelled out under "File location." The TASK-85 plan must add an explicit AC: identifier stays `com.openwhisper.app` post-rename. If TASK-85 lands before TASK-87 starts, this AC needs to be retroactively confirmed.
- **Multi-instance writes.** OW is single-instance (TASK-37) but if two processes ever opened the same DB simultaneously, SQLite handles it via file locks — writes serialize, no corruption. No special handling needed.
- **WAL vs rollback journal.** Default journal mode (DELETE) is fine for the write rate OW will see. WAL would offer better concurrent read-during-write but adds three sidecar files (`-wal`, `-shm`, `-journal`) that confuse "is this file the database?" The simpler default mode is the right call until a profiler says otherwise.
- **Disk-full handling.** A failed INSERT bubbles up as `rusqlite::Error::SqliteFailure(...)` and the `record_dictation` caller (TASK-88) must log-and-continue. Document the contract so the stats wiring doesn't accidentally bubble it into PHASE_ERROR.
