---
id: TASK-62.1
title: 'Plan Task 1: Memory query primitive'
status: In Review
assignee: []
created_date: '2026-04-30 22:25'
updated_date: '2026-05-07 00:00'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
ordinal: 4000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 ProcessMemory type defined with rss_bytes, peak_rss_bytes, timestamp
- [x] #2 query_process_memory() returns non-zero RSS on the current process
- [x] #3 Unit test covers RSS grows after allocation; peak >= current
- [x] #4 cargo check and cargo test clean for openwhisper-core
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
- Commit: e671946.
- New module `core/src/telemetry/{mod.rs,memory.rs}`; `pub mod telemetry;` wired into `core/src/lib.rs`.
- Added `sysinfo = "0.32"` (default-features off, `system` only) to `core/Cargo.toml`. Cross-platform per-process RSS via `sysinfo::Process::memory()` (Mac proc_pidinfo / Win GetProcessMemoryInfo under the hood).
- `peak_rss_bytes` is a process-global running max of every observed RSS (`AtomicU64::fetch_max`). `sysinfo` 0.32 doesn't expose a peak field; spec note kept — switch to native `task_info().resident_size_max` / `PeakWorkingSetSize` in a follow-up if accuracy bites.
- `cargo test -p openwhisper-core --lib` 64/64 green. `cargo check -p openwhisper-core` and `cargo check -p openwhisper-tauri` clean.
- Awaiting user QA — pure-Rust core primitive with no UI surface yet (UI lands in TASK-62.8); reviewer can verify via `cargo test -p openwhisper-core --lib telemetry`.
- Win-side re-check follow-up, commit db5d7db: `rss_grows_after_large_allocation_and_peak_tracks_high_watermark` was passing on Mac but failed on Windows with `before == after` after a 64 MB `vec![0u8; N]`. Cause: `alloc_zeroed` for large allocs on Windows goes through `VirtualAlloc(MEM_COMMIT)`, charging commit but not Working Set until first touch. Fixed by faulting pages in via volatile writes at 4 KB stride. Product code unchanged — `sysinfo` → `WorkingSetSize` is still the correct user-visible metric.
<!-- SECTION:NOTES:END -->
