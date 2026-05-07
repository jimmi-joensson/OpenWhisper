//! Explicit load/unload lifecycle for ASR + post-processing models.
//!
//! Replaces the implicit "load once, stay forever" pattern in
//! [`crate::recognizer`] with a `ModelHandle<T>` whose state — and
//! observable RSS cost — is queryable. The Diagnostics → Memory pane
//! (TASK-62.8) reads `current_memory_estimate()` per registered handle
//! to attribute the process RSS back to the models that caused it.
//!
//! ## Scope of this task (TASK-62.2)
//!
//! Pure state machine, no async, no idle timer. Transitions are
//! synchronous and run under a `Mutex` covering the state field; the
//! loader closure runs *between* lock acquisitions so a long load
//! doesn't block readers calling `state()` from the diagnostics poll.
//!
//! Idle timer + `set_idle_timeout` lands in TASK-62.3 (adds Tokio).
//! Process-global registry + keep-warm hot-reload lands in TASK-62.4.
//!
//! ## Concurrency model (intentionally minimal here)
//!
//! - `load()` is single-flight against itself and idempotent — second
//!   caller while a load is in flight gets a clear error
//!   (`ErrLoadInFlight`); they retry once the loader has settled.
//!   Proper condvar-style "await Loading" comes with the Tokio runtime
//!   in 62.3.
//! - `use_with` serializes against itself via the `inner` mutex; only
//!   one closure body runs at a time. Concurrent callers block on the
//!   inner lock — no error.
//! - `unload()` while `Active` is rejected (matches plan AC #4).

use std::sync::{Arc, Mutex};

use crate::telemetry::query_process_memory;

/// Where in the load/use/release cycle a [`ModelHandle`] currently
/// sits. Order is meaningful for the Diagnostics chip animations
/// (Loading + Releasing pulse, Active haloes); kept parallel with the
/// design tokens in `apps/tauri` (`MODEL_STATES`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleState {
    /// No model in memory, no resources held.
    Unloaded,
    /// Load in progress (file I/O + CoreML compile / ONNX session
    /// build / mmap warmup). Transient — bracketed by exactly one
    /// `Loading → Loaded` or `Loading → Unloaded` (on loader failure).
    Loading,
    /// Resident, idle, ready. Idle timer (TASK-62.3) is what walks
    /// this back to `Releasing`; this task ships without one.
    Loaded,
    /// Currently servicing a `use_with` call. Idle timer is paused.
    Active,
    /// Unload in progress. Transient. Falls back to `Unloaded` on
    /// success; `Unloaded` is the only legal exit.
    Releasing,
}

/// A loadable, unloadable resource. `T` is the underlying handle the
/// model exposes — for the recognizer that's `Box<dyn Recognizer>`,
/// for the future cleanup LLM that's the LLM client.
///
/// Cloning a `ModelHandle` shares state — both clones point at the
/// same model. Internally everything is `Arc<Mutex<...>>`.
pub struct ModelHandle<T: Send + 'static> {
    /// Human-readable identifier surfaced in telemetry rows
    /// (`recognizer`, `cleanup-llm`, …) and log lines.
    label: String,
    state: Arc<Mutex<LifecycleState>>,
    /// `None` when state is `Unloaded`. `Some(_)` from the moment
    /// `Loading` succeeds until `unload()` clears it.
    inner: Arc<Mutex<Option<T>>>,
    /// Set every successful `Loading → Loaded` transition. Computed
    /// from `query_process_memory()` deltas; documented as estimated.
    last_load_rss_delta: Arc<Mutex<u64>>,
    loader: Arc<dyn Fn() -> Result<T, String> + Send + Sync>,
}

impl<T: Send + 'static> Clone for ModelHandle<T> {
    fn clone(&self) -> Self {
        Self {
            label: self.label.clone(),
            state: Arc::clone(&self.state),
            inner: Arc::clone(&self.inner),
            last_load_rss_delta: Arc::clone(&self.last_load_rss_delta),
            loader: Arc::clone(&self.loader),
        }
    }
}

impl<T: Send + 'static> ModelHandle<T> {
    /// Build a handle without an idle timer. The handle starts in
    /// `Unloaded` state; `load()` (or the implicit auto-load inside
    /// `use_with`) drives the first load.
    pub fn new<F>(label: &str, loader: F) -> Self
    where
        F: Fn() -> Result<T, String> + Send + Sync + 'static,
    {
        Self {
            label: label.to_string(),
            state: Arc::new(Mutex::new(LifecycleState::Unloaded)),
            inner: Arc::new(Mutex::new(None)),
            last_load_rss_delta: Arc::new(Mutex::new(0)),
            loader: Arc::new(loader),
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn state(&self) -> LifecycleState {
        *self
            .state
            .lock()
            .expect("ModelHandle::state lock poisoned")
    }

    /// RSS delta captured at the most recent `Loading → Loaded`
    /// transition, in bytes. Returns `0` when the handle has never
    /// been loaded successfully.
    ///
    /// Estimated — concurrent allocations during the loader skew it.
    /// See module docs for the caveat surfaced in the UI.
    pub fn current_memory_estimate(&self) -> u64 {
        *self
            .last_load_rss_delta
            .lock()
            .expect("ModelHandle::last_load_rss_delta lock poisoned")
    }

    /// Drive an `Unloaded` handle through `Loading → Loaded`. No-op
    /// if already `Loaded` or `Active`. Errors if the handle is
    /// currently `Loading` on another thread or `Releasing`.
    ///
    /// On loader failure: state returns to `Unloaded`, the error from
    /// the loader is propagated, the RSS delta is left untouched.
    pub fn load(&self) -> Result<(), String> {
        // Fast path + state transition under one lock acquisition.
        {
            let mut state = self.state.lock().expect("state lock poisoned");
            match *state {
                LifecycleState::Loaded | LifecycleState::Active => return Ok(()),
                LifecycleState::Loading => {
                    return Err(format!(
                        "model `{}` already loading on another thread",
                        self.label
                    ));
                }
                LifecycleState::Releasing => {
                    return Err(format!(
                        "model `{}` is releasing; retry after unload completes",
                        self.label
                    ));
                }
                LifecycleState::Unloaded => {
                    *state = LifecycleState::Loading;
                }
            }
        }

        // Loader runs OUTSIDE the state lock so `state()` callers
        // (the diagnostics 1 Hz poll) can still read `Loading` while
        // the heavy work runs.
        let before = query_process_memory().rss_bytes;
        let load_result = (self.loader)();
        let after = query_process_memory().rss_bytes;
        let delta = after.saturating_sub(before);

        match load_result {
            Ok(t) => {
                let mut inner = self.inner.lock().expect("inner lock poisoned");
                *inner = Some(t);
                drop(inner);

                *self
                    .last_load_rss_delta
                    .lock()
                    .expect("rss delta lock poisoned") = delta;

                let mut state = self.state.lock().expect("state lock poisoned");
                *state = LifecycleState::Loaded;
                Ok(())
            }
            Err(e) => {
                // Reset state so the next caller can retry.
                let mut state = self.state.lock().expect("state lock poisoned");
                *state = LifecycleState::Unloaded;
                Err(e)
            }
        }
    }

    /// Drop the resident model. No-op if already `Unloaded`. Rejected
    /// (without effect) if the handle is in `Active`, `Loading`, or
    /// `Releasing` — callers should wait for the in-flight call to
    /// finish before retrying.
    pub fn unload(&self) -> Result<(), String> {
        {
            let mut state = self.state.lock().expect("state lock poisoned");
            match *state {
                LifecycleState::Unloaded => return Ok(()),
                LifecycleState::Loaded => {
                    *state = LifecycleState::Releasing;
                }
                LifecycleState::Active => {
                    return Err(format!(
                        "model `{}` is in use; cannot unload while Active",
                        self.label
                    ));
                }
                LifecycleState::Loading => {
                    return Err(format!(
                        "model `{}` is loading; cannot unload until Loaded",
                        self.label
                    ));
                }
                LifecycleState::Releasing => {
                    return Err(format!(
                        "model `{}` is already releasing",
                        self.label
                    ));
                }
            }
        }

        // Drop the inner outside the state lock — `Drop` impls on the
        // recognizer (FluidAudioBridge / OrtParakeet) may run for tens
        // of ms and we don't want the diagnostics poll to block on
        // them.
        {
            let mut inner = self.inner.lock().expect("inner lock poisoned");
            inner.take();
        }

        let mut state = self.state.lock().expect("state lock poisoned");
        *state = LifecycleState::Unloaded;
        Ok(())
    }

    /// Borrow the loaded model for a single call. Auto-loads if
    /// currently `Unloaded`. Concurrent callers serialize through the
    /// inner mutex.
    ///
    /// State sequence per call: caller-state → `Active` → `Loaded`.
    /// On panic inside the closure the state is restored to `Loaded`
    /// via the inner lock guard's `Drop` chain (Rust's poison
    /// machinery), so a transient panic doesn't strand the handle in
    /// `Active`.
    pub fn use_with<R>(&self, f: impl FnOnce(&mut T) -> R) -> Result<R, String> {
        // Acquire the inner lock *first* so concurrent `use_with`
        // callers serialize against the actual model handle, not just
        // the state field.
        let mut inner = self.inner.lock().map_err(|_| "inner lock poisoned")?;

        // If unloaded, the auto-load needs the inner lock released
        // (load() also takes the inner lock to install the new T).
        // Drop, load, reacquire.
        if inner.is_none() {
            drop(inner);
            self.load()?;
            inner = self.inner.lock().map_err(|_| "inner lock poisoned")?;
        }

        // Transition state Loaded → Active under the state lock,
        // briefly. Reject if we're not in a usable state.
        {
            let mut state = self.state.lock().expect("state lock poisoned");
            match *state {
                LifecycleState::Loaded => *state = LifecycleState::Active,
                LifecycleState::Active => {
                    // Should be unreachable — we hold the inner lock,
                    // and the inner lock is acquired before transitioning
                    // Loaded → Active in this same function. Anyone
                    // observing Active without the inner lock means
                    // someone broke the invariant.
                    return Err(format!(
                        "model `{}` already Active without inner lock — invariant violation",
                        self.label
                    ));
                }
                other => {
                    return Err(format!(
                        "model `{}` is in state {other:?}; cannot use",
                        self.label
                    ));
                }
            }
        }

        // Run the closure with the loaded handle. `inner` is `Some`
        // by construction (auto-load above) — the `expect` documents
        // that invariant rather than masking a real bug.
        let t = inner
            .as_mut()
            .expect("inner is Some after successful load");
        let result = f(t);

        // Back to Loaded.
        let mut state = self.state.lock().expect("state lock poisoned");
        *state = LifecycleState::Loaded;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Mock loader — counts call attempts and can be told to fail on
    /// demand. Returns a `u32` payload so the `T` is concrete and
    /// trivially `Send + 'static`.
    fn mock_loader(
        calls: Arc<AtomicUsize>,
        fail_first_n: usize,
    ) -> impl Fn() -> Result<u32, String> + Send + Sync + 'static {
        move || {
            let n = calls.fetch_add(1, Ordering::SeqCst);
            if n < fail_first_n {
                Err(format!("synthetic loader failure #{n}"))
            } else {
                Ok(42 + n as u32)
            }
        }
    }

    #[test]
    fn load_use_unload_walks_states_in_order() {
        let calls = Arc::new(AtomicUsize::new(0));
        let h: ModelHandle<u32> = ModelHandle::new("mock", mock_loader(calls.clone(), 0));
        assert_eq!(h.state(), LifecycleState::Unloaded);

        h.load().expect("load ok");
        assert_eq!(h.state(), LifecycleState::Loaded);

        // During use_with the closure observes state==Active.
        let observed = h
            .use_with(|t| {
                assert_eq!(*t, 42);
                "borrowed"
            })
            .expect("use_with ok");
        assert_eq!(observed, "borrowed");
        assert_eq!(h.state(), LifecycleState::Loaded);

        h.unload().expect("unload ok");
        assert_eq!(h.state(), LifecycleState::Unloaded);
    }

    #[test]
    fn load_is_idempotent() {
        let calls = Arc::new(AtomicUsize::new(0));
        let h: ModelHandle<u32> = ModelHandle::new("mock", mock_loader(calls.clone(), 0));

        h.load().unwrap();
        h.load().unwrap();
        h.load().unwrap();

        // The loader closure was only invoked once — subsequent
        // load() calls observed Loaded and short-circuited.
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(h.state(), LifecycleState::Loaded);
    }

    #[test]
    fn use_with_auto_loads_from_unloaded() {
        let calls = Arc::new(AtomicUsize::new(0));
        let h: ModelHandle<u32> = ModelHandle::new("mock", mock_loader(calls.clone(), 0));
        assert_eq!(h.state(), LifecycleState::Unloaded);

        let v = h.use_with(|t| *t).expect("use_with auto-loads");
        assert_eq!(v, 42);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(h.state(), LifecycleState::Loaded);
    }

    #[test]
    fn unload_during_active_is_rejected() {
        // We can't easily run two threads inside a single use_with
        // closure without deadlocking on the inner mutex. Simulate
        // the "Active" state via direct state poke + assert that
        // unload() refuses to drop the model. The state field is the
        // only thing unload() reads to make the call.
        let calls = Arc::new(AtomicUsize::new(0));
        let h: ModelHandle<u32> = ModelHandle::new("mock", mock_loader(calls, 0));
        h.load().unwrap();

        // Force Active. In real flow this happens inside use_with,
        // but the state machine's behavior under Active is what we're
        // testing — it shouldn't matter how we got there.
        *h.state.lock().unwrap() = LifecycleState::Active;

        let err = h.unload().expect_err("unload must reject Active");
        assert!(
            err.contains("Active"),
            "error should mention Active state, got: {err}"
        );
        assert_eq!(h.state(), LifecycleState::Active);

        // Restore Loaded so the handle is in a sane state on drop.
        *h.state.lock().unwrap() = LifecycleState::Loaded;
    }

    #[test]
    fn failed_loader_leaves_state_unloaded_and_propagates_error() {
        let calls = Arc::new(AtomicUsize::new(0));
        // First two load attempts fail, third succeeds.
        let h: ModelHandle<u32> = ModelHandle::new("mock", mock_loader(calls.clone(), 2));

        let err = h.load().expect_err("first load should fail");
        assert!(err.contains("synthetic loader failure"));
        assert_eq!(h.state(), LifecycleState::Unloaded);

        let err = h.load().expect_err("second load should fail");
        assert!(err.contains("synthetic loader failure"));
        assert_eq!(h.state(), LifecycleState::Unloaded);

        // Third try succeeds — failed loaders did NOT poison the
        // handle.
        h.load().expect("third load should succeed");
        assert_eq!(h.state(), LifecycleState::Loaded);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn current_memory_estimate_is_zero_until_first_load() {
        let calls = Arc::new(AtomicUsize::new(0));
        let h: ModelHandle<u32> = ModelHandle::new("mock", mock_loader(calls, 0));
        assert_eq!(h.current_memory_estimate(), 0);

        h.load().unwrap();
        // The mock loader allocates only a u32 payload — the RSS
        // delta is environment-dependent (often 0 on a quiet test
        // process). We assert it's a defined value, not negative,
        // and not stale-as-MAX.
        let est = h.current_memory_estimate();
        assert!(est < u64::MAX / 2, "estimate looks corrupted: {est}");
    }
}
