---
id: doc-40
title: Persistence foundation — implementation plan
type: specification
created_date: '2026-05-06 06:06'
---

**Backlog parent:** TASK-87
**Spec:** backlog/docs/specs/doc-39 - Persistence-foundation-—-design.md

Four tasks, mostly sequential. Task 1 (deps + module skeleton) precedes everything. Task 2 (migration 1) and Task 3 (Tauri wiring) are independent after Task 1 and can run in parallel. Task 4 (test sweep) depends on 2 + 3.

---

### Task 1: Add rusqlite + rusqlite_migration deps and the Store module skeleton

Wires up the crates and the `Store` struct in `core/src/store/mod.rs` without any migrations or schema yet. The struct can `open_or_init` an empty file and `with_conn` execute arbitrary SQL — Task 2 fills the migration.

**Files:**

- `core/Cargo.toml` — add `rusqlite = { version = "0.32", features = ["bundled"] }` and `rusqlite_migration = "1.2"`. Use the latest minor at land time; pin major.
- `core/src/lib.rs` — add `pub mod store;` next to existing module declarations.
- `core/src/store/mod.rs` (new) — declare `Store`, `StoreError`, and the two methods from the spec. `open_or_init` opens the connection (creating parent dirs if missing), runs the (still-empty) migrations chain, returns `Self`. `with_conn` locks the mutex and runs the closure.
- Decision lock: license check on `rusqlite_migration` — confirm MIT or Apache-2.0 before landing the dep. Spec assumed MIT-compatible; verify in actual `Cargo.toml`/registry metadata.

**Outcome ACs:**

- `core/Cargo.toml` lists both deps with the `bundled` feature on rusqlite; the `openwhisper-core` crate compiles cleanly with both deps resolved (no version conflicts, no missing-feature errors).
- `Store::open_or_init(path)` creates the file at `path` (and any missing parent dirs) and returns `Ok(Store)`.
- `Store::with_conn(|c| c.execute("SELECT 1", []))` succeeds against an opened store.
- `StoreError` enum exists with `Io`, `Sqlite`, `Migration` variants; `From` impls wired for the underlying error types.

**Verification:**

- `cargo build -p openwhisper-core` clean.
- A throwaway `cargo test` runs `Store::open_or_init` against a `tempfile::tempdir()` path, then `with_conn` runs `PRAGMA user_version` and asserts it returns `0` (no migrations yet).

---

### Task 2: Define migration 1 — `dictations` table + index

Adds the schema migration array and the first migration. Future tasks (TASK-88 stats writer, future history) append migrations here.

**Files:**

- `core/src/store/migrations.rs` (new) — `pub fn migrations() -> Migrations<'static>` returning the static array. Migration 1 is the spec's CREATE TABLE + CREATE INDEX as a single SQL string.
- `core/src/store/mod.rs` — `open_or_init` calls `migrations().to_latest(&mut conn)?` after opening.

**Migration 1 SQL** (verbatim from spec):

```sql
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
CREATE INDEX idx_dictations_started_at ON dictations(started_at);
```

**Outcome ACs:**

- `core/src/store/migrations.rs` exposes `migrations()` returning a `Migrations<'static>` containing exactly one migration with the SQL above.
- After `open_or_init`, `PRAGMA user_version` returns `1`.
- `sqlite_master` reflects a `dictations` table with the seven columns specified in the spec and the `idx_dictations_started_at` index.
- A second `open_or_init` on the same file is a no-op (idempotency): `user_version` stays `1`, no error, no schema change.

**Verification:**

- `cargo test -p openwhisper-core store::` covers: fresh-init applies migration 1; reopen-init is no-op; an `INSERT INTO dictations (started_at, duration_ms, word_count) VALUES (?, ?, ?)` with three integers + a follow-up `SELECT COUNT(*) FROM dictations` returns 1.

---

### Task 3: Tauri startup wiring — open store at app_data_dir, register as managed state

Wires `Store::open_or_init` into the shell so the rest of the codebase (TASK-88) can reach it via `app.state::<Store>()`.

> **Failure-channel reminder:** the spec explicitly states that store-init failure must NOT use `dictation_deliver_error`. That channel exists for recognizer errors and routes to `PHASE_ERROR`, which would make a missing stats DB look to the user like a broken recognizer. Use `tracing::error!` only and let the app continue without persistence.

**Files:**

- `apps/tauri/src-tauri/src/lib.rs` — in the `tauri::Builder::default().setup(|app| { ... })` closure, resolve `app.path().app_data_dir()`, call `Store::open_or_init(dir.join("openwhisper.db"))`, then `app.manage(store)`. On error: `tracing::error!` and continue (do NOT propagate to `setup`'s `Result` — that would block app launch).
- `apps/tauri/src-tauri/Cargo.toml` — confirm the workspace already exposes `openwhisper-core` as a dep (it does); add `tracing` if not already pulled in transitively.
- No new Tauri command in this task; the writer for `dictations` ships in TASK-88.

**Outcome ACs:**

- App launches successfully on macOS and Windows. After first launch, the file `<app_data_dir>/openwhisper.db` exists (verified manually with `ls "$HOME/Library/Application Support/com.openwhisper.app.dev/"` for the dev build, or `dir %APPDATA%\com.openwhisper.app.dev\` on Windows).
- A simulated path-resolution failure (force `app_data_dir()` to return an unwritable path in a test build) does NOT panic; app continues, error is logged.
- `app.state::<Store>()` resolves inside any Tauri command after `setup` completes (verified by a throwaway command added during Task 4).

**Verification:**

- `pnpm tauri dev` on macOS — open the app, confirm DB file appears at the expected path, kill the app, relaunch, confirm file is reused (not recreated), `user_version` still 1.
- Same on Windows (manual smoke).
- Failure path verified by a unit test in `apps/tauri/src-tauri` that builds the setup-equivalent against an unwritable path and asserts no panic.

---

### Task 4: Unit test sweep + concurrent-read safety

Final task: pulls the loose threads into a coherent test suite covering open/reopen, migration idempotency, and concurrent reads.

**Files:**

- `core/src/store/mod.rs` — add a `#[cfg(test)] mod tests { ... }` block (or split into `core/src/store/tests.rs` if it grows past ~150 LOC) covering:
  - `open_or_init` creates the file and parent dir.
  - `open_or_init` twice on the same path is idempotent: second call returns `Ok` and `user_version` is unchanged.
  - INSERT a row, reopen, SELECT — round-trip works.
  - Two threads each running `with_conn` to SELECT concurrently against the same `Store` do not deadlock and return consistent counts.
- No production code changes in this task; if a test surfaces a bug, the fix lands as a sub-commit and the AC stays "the test exists and is green."

**Outcome ACs:**

- `cargo test -p openwhisper-core store::` includes ≥4 tests covering the cases above and is green.
- Test for concurrent reads spawns 8 threads × 100 iterations and asserts no panics, no deadlock (use a 5-second timeout in the test wrapper).

**Verification:**

- `cargo test -p openwhisper-core` — must run, not just compile.
- One run on macOS, one on Windows (CI is sufficient if it covers both).

---

## Cross-task notes

- **Parallelism:** Task 1 first. Tasks 2 and 3 in parallel after that. Task 4 last.
- **No deferred design decisions.** Schema is locked in spec + Task 2. Mutex-vs-pool is locked. Failure-path policy is locked.
- **Bundle identifier dependency on TASK-85.** Add to TASK-85's plan (or its remaining subtasks): "AC: bundle identifier `com.openwhisper.app` does not change as part of the rename." If TASK-85 ships before TASK-87, retroactively confirm the AC was met.
- **Out-of-scope reminders:** No FTS5, no sqlite-vec, no SQLCipher, no settings migration to SQLite, no backup UI. Stats writers ship in TASK-88; that task depends on this one.
