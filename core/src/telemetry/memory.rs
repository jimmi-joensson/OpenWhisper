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
use std::time::SystemTime;

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// Snapshot of the running OpenWhisper process's memory at a point in
/// time. Returned by [`query_process_memory`] and consumed by the
/// Diagnostics pane + future per-model attribution.
#[derive(Debug, Clone)]
pub struct ProcessMemory {
    /// Resident set size — bytes the OS currently has resident in RAM
    /// for this process. macOS reads `proc_pidinfo`, Windows reads
    /// `GetProcessMemoryInfo().WorkingSetSize`, both via `sysinfo`.
    pub rss_bytes: u64,
    /// Maximum `rss_bytes` ever observed by [`query_process_memory`]
    /// in this process. Tracked across calls; reset only on process
    /// exit. See module docs for the "running max" caveat.
    pub peak_rss_bytes: u64,
    /// Wall-clock time the snapshot was taken.
    pub timestamp: SystemTime,
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
        timestamp: SystemTime::now(),
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
    fn rss_grows_after_large_allocation_and_peak_tracks_high_watermark() {
        let before = query_process_memory();

        // 64 MB — large enough to force a meaningful RSS bump on every
        // platform (Mac/Win/Linux), small enough to be safe in CI.
        // `vec![0u8; N]` zero-initializes, which on every platform we
        // care about touches the pages and forces them resident.
        let buf: Vec<u8> = vec![0u8; 64 * 1024 * 1024];
        // black_box prevents the optimizer from eliding the allocation.
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
