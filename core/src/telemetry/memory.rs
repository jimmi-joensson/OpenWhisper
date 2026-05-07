//! Cross-platform per-process memory readout. Foundation for the
//! Diagnostics → Memory pane (TASK-62.8) and the per-model RSS-delta
//! attribution that lands with `ModelHandle` (TASK-62.2/.7).
//!
//! ## Peak RSS
//!
//! `sysinfo` 0.32 surfaces current RSS but not a peak field — both
//! macOS (`task_info`) and Windows (`GetProcessMemoryInfo`) expose a
//! native peak that we'll switch to in a follow-up. For now we track
//! the running max of every RSS we've observed via this function in a
//! process-global `AtomicU64`; that's enough for the AC ("peak ≥
//! current") and for the diagnostics chart's "RSS peak" readout, since
//! the chart polls at ~1 Hz.
//!
//! Concretely: peak is "max RSS seen across all `query_process_memory`
//! calls in this process so far". A test that allocates and then
//! queries always sees the post-alloc peak; one that allocs, frees,
//! and queries sees a peak ≥ the post-alloc bump too. That matches
//! what users expect from an "Activity Monitor"-style peak readout.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sysinfo::{MemoryRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// Snapshot of the running OpenWhisper process's memory at a point in
/// time. Returned by [`query_process_memory`] and consumed by the
/// Diagnostics pane + future per-model attribution.
///
/// Serializable so the Tauri `telemetry_get_memory` command (TASK-
/// 62.7) can return it across the IPC boundary without a hand-rolled
/// bridge. `timestamp_unix_ms` (rather than a `SystemTime` field)
/// because `SystemTime` has no serde-default; unix-ms round-trips
/// cleanly to the React side and the CLI JSON output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMemory {
    /// Resident set size — bytes the OS currently has resident in RAM
    /// for this process. macOS reads `proc_pidinfo`, Windows reads
    /// `GetProcessMemoryInfo().WorkingSetSize`, both via `sysinfo`.
    pub rss_bytes: u64,
    /// Maximum `rss_bytes` ever observed by [`query_process_memory`]
    /// in this process. Tracked across calls; reset only on process
    /// exit. See module docs for the "running max" caveat.
    pub peak_rss_bytes: u64,
    /// Wall-clock time the snapshot was taken, as Unix epoch
    /// milliseconds. `0` if the system clock is set before the Unix
    /// epoch (which would mean the host is broken in other ways too).
    pub timestamp_unix_ms: u64,
}

/// Host-wide memory snapshot. Surfaced alongside [`ProcessMemory`] so
/// the Diagnostics → Memory pane can answer "how is my system holding
/// up?" without forcing the user to alt-tab to Activity Monitor.
///
/// Cross-platform via `sysinfo`; values come from `host_statistics64`
/// on macOS and `GlobalMemoryStatusEx` on Windows. We deliberately
/// stick to the four fields both platforms agree on (total / used /
/// available / swap) rather than surface the macOS-specific
/// compressed/wired/cached split — matches what the design's
/// "System Memory Used" readout consumes and stays honest on Windows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMemory {
    /// Total physical memory installed on the host, in bytes.
    pub total_bytes: u64,
    /// Bytes currently in use by the system. macOS counts active +
    /// wired; Windows reports the in-use working set across processes.
    pub used_bytes: u64,
    /// Bytes immediately available for new allocations without paging.
    /// `total_bytes - used_bytes` would not equal this on Mac (cached
    /// + compressed pages can be reclaimed), so we surface the OS's
    /// own number.
    pub available_bytes: u64,
    /// Total swap (page-file) the OS can grow into.
    pub swap_total_bytes: u64,
    /// Swap currently committed.
    pub swap_used_bytes: u64,
}

/// Process-global running max of observed RSS. Updated on every
/// `query_process_memory` call. `OnceLock` is overkill for an
/// `AtomicU64` (no init cost) but keeps the pattern consistent with
/// the rest of `core/` (settings, recognizer ENGINE).
static PEAK_RSS_BYTES: AtomicU64 = AtomicU64::new(0);

/// Cached `System` handle. `sysinfo::System::new()` is cheap but
/// creating + dropping it on every call would re-allocate the
/// internal process map; reusing it across calls is the documented
/// pattern. Wrapped in `OnceLock<std::sync::Mutex<...>>` so the
/// 1 Hz poll the Diagnostics pane fires can't race with itself if
/// future code calls from multiple threads.
static SYS: OnceLock<std::sync::Mutex<System>> = OnceLock::new();

/// Snapshot the current process's memory.
///
/// Returns rss=0 only if the OS-level lookup fails (which we've never
/// seen on Mac or Windows for the running process; sysinfo refuses to
/// surface absent processes in its map). The peak field always
/// reflects the running max in this process.
pub fn query_process_memory() -> ProcessMemory {
    let pid = Pid::from_u32(std::process::id());

    let sys_lock = SYS.get_or_init(|| std::sync::Mutex::new(System::new()));
    let mut sys = sys_lock.lock().expect("telemetry::SYS poisoned");
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::new().with_memory(),
    );
    let rss_bytes = sys
        .process(pid)
        .map(|p| p.memory())
        .unwrap_or(0);
    drop(sys);

    // Update + read the running peak. fetch_max returns the previous
    // value; we want the post-update max so we max() against rss
    // ourselves.
    let prev_peak = PEAK_RSS_BYTES.fetch_max(rss_bytes, Ordering::Relaxed);
    let peak_rss_bytes = prev_peak.max(rss_bytes);

    ProcessMemory {
        rss_bytes,
        peak_rss_bytes,
        timestamp_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
    }
}

/// Snapshot the host-wide memory + swap counters. Same `System` cache
/// the per-process query reuses — `refresh_memory_specifics` is the
/// only call that hits the OS, and it's cheap (microseconds).
///
/// Returns zeros on the rare platforms where `sysinfo` declines to
/// supply a value (we've never seen this on Mac or Windows). The
/// Diagnostics pane treats `total_bytes == 0` as "telemetry not
/// available yet" and hides the system readout until the next poll.
pub fn query_system_memory() -> SystemMemory {
    let sys_lock = SYS.get_or_init(|| std::sync::Mutex::new(System::new()));
    let mut sys = sys_lock.lock().expect("telemetry::SYS poisoned");
    sys.refresh_memory_specifics(MemoryRefreshKind::new().with_ram().with_swap());
    SystemMemory {
        total_bytes: sys.total_memory(),
        used_bytes: sys.used_memory(),
        available_bytes: sys.available_memory(),
        swap_total_bytes: sys.total_swap(),
        swap_used_bytes: sys.used_swap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss_is_nonzero_for_current_process() {
        let m = query_process_memory();
        assert!(
            m.rss_bytes > 0,
            "expected non-zero RSS for the running test binary, got {}",
            m.rss_bytes
        );
        assert!(
            m.peak_rss_bytes >= m.rss_bytes,
            "peak ({}) must be >= current ({})",
            m.peak_rss_bytes,
            m.rss_bytes
        );
    }

    #[test]
    fn system_memory_reports_nonzero_total_and_consistent_used() {
        let s = query_system_memory();
        assert!(
            s.total_bytes > 0,
            "expected non-zero total system memory, got {}",
            s.total_bytes
        );
        assert!(
            s.used_bytes <= s.total_bytes,
            "used ({}) must be <= total ({})",
            s.used_bytes,
            s.total_bytes
        );
        assert!(
            s.available_bytes <= s.total_bytes,
            "available ({}) must be <= total ({})",
            s.available_bytes,
            s.total_bytes
        );
        assert!(
            s.swap_used_bytes <= s.swap_total_bytes
                || s.swap_total_bytes == 0,
            "swap_used ({}) must be <= swap_total ({}) when swap is enabled",
            s.swap_used_bytes,
            s.swap_total_bytes
        );
    }

    #[test]
    fn rss_grows_after_large_allocation_and_peak_tracks_high_watermark() {
        let before = query_process_memory();

        // 64 MB — large enough to force a meaningful RSS bump on every
        // platform (Mac/Win/Linux), small enough to be safe in CI.
        // `vec![0u8; N]` allocates committed but unfaulted pages on
        // Windows (VirtualAlloc returns zero pages charged against
        // commit, not Working Set, until first touch). Touch one byte
        // per 4 KB page via volatile writes to fault them in so RSS
        // (Working Set on Win, resident pages on Mac/Linux) actually
        // grows.
        let mut buf: Vec<u8> = vec![0u8; 64 * 1024 * 1024];
        let len = buf.len();
        let ptr = buf.as_mut_ptr();
        let mut i = 0;
        while i < len {
            // SAFETY: i < len, ptr is valid for `len` bytes, and the
            // write is to our exclusive &mut Vec storage.
            unsafe { std::ptr::write_volatile(ptr.add(i), 1) };
            i += 4096;
        }
        std::hint::black_box(&buf);

        let after = query_process_memory();
        assert!(
            after.rss_bytes > before.rss_bytes,
            "RSS should grow after a 64 MB allocation: before={} after={}",
            before.rss_bytes,
            after.rss_bytes
        );
        assert!(
            after.peak_rss_bytes >= after.rss_bytes,
            "peak ({}) must remain >= current ({}) after growth",
            after.peak_rss_bytes,
            after.rss_bytes
        );

        // Drop the buffer and confirm peak does NOT regress — the
        // running-max guarantee is the load-bearing property for the
        // Diagnostics chart's "peak" readout.
        let peak_before_drop = after.peak_rss_bytes;
        drop(buf);
        let post_drop = query_process_memory();
        assert!(
            post_drop.peak_rss_bytes >= peak_before_drop,
            "peak regressed after drop: was {} now {}",
            peak_before_drop,
            post_drop.peak_rss_bytes
        );
    }
}
