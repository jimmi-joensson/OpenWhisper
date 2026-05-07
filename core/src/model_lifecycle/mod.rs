//! Explicit load/unload lifecycle for ASR + post-processing models.
//!
//! Replaces the implicit "load once, stay forever" pattern in
//! [`crate::recognizer`] with a `ModelHandle<T>` whose state — and
//! observable RSS cost — is queryable. The Diagnostics → Memory pane
//! (TASK-62.8) reads `current_memory_estimate()` per registered handle
//! to attribute the process RSS back to the models that caused it.
//!
//! ## Async runtime
//!
//! No async runtime is required. The idle timer is a single
//! `std::thread::spawn`'d worker per handle, parked on a
//! `std::sync::Condvar` with `wait_timeout`. The plan
//! (`backlog/docs/plans/2026-05-01-model-lifecycle-telemetry.md`,
//! TASK-62.3) explicitly permits the std::thread + condvar fallback
//! for the recognizer's minutes-cadence timer; it avoids pulling
//! Tokio into `core/` until TASK-63's cleanup-LLM async pipeline
//! actually needs it. If a future task wires Tokio into `core/`, the
//! timer can be migrated without changing the public surface
//! (`with_idle_timeout`, `set_idle_timeout`).
//!
//! ## Concurrency model
//!
//! - `load()` is single-flight against itself and idempotent — second
//!   caller while a load is in flight gets a clear error; they retry
//!   once the loader has settled.
//! - `use_with` serializes against itself via the `inner` mutex; only
//!   one closure body runs at a time. Concurrent callers block on the
//!   inner lock — no error.
//! - `unload()` while `Active` is rejected.
//! - The idle timer thread races against `use_with` for the state
//!   lock — whoever wins picks the legal transition. If the timer
//!   wins, the handle goes Releasing → Unloaded and a subsequent
//!   `use_with` simply auto-loads (re-runs the loader). If `use_with`
//!   wins, the timer's `fire_unload` sees state≠Loaded and skips.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, RwLock, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

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
    /// Resident, idle, ready. The idle timer fires here once the
    /// configured `idle_timeout` elapses without a `use_with` call.
    Loaded,
    /// Currently servicing a `use_with` call. Idle timer is paused.
    Active,
    /// Unload in progress. Transient. Falls back to `Unloaded` on
    /// success; `Unloaded` is the only legal exit.
    Releasing,
}

/// Inputs the idle timer thread reacts to. The timer thread sleeps on
/// `cv` with `wait_timeout`; rearming bumps `deadline` and notifies;
/// shutdown bumps `shutdown` and notifies. Drop ordering documented
/// on `ShutdownSignal`.
struct TimerCmd {
    /// `Some(Instant)` = fire `unload()` at or after this time. `None`
    /// = no fire armed; the timer thread waits indefinitely on `cv`.
    deadline: Option<Instant>,
    shutdown: bool,
}

/// Per-handle idle-timer config. Cloned by reference (Arc) into the
/// timer thread so rearm/cancel from the main thread reaches the
/// sleeping worker.
struct IdleControl {
    /// User's configured idle timeout. `set_idle_timeout` writes
    /// this. `apply_keep_warm` does NOT touch this — keep-warm is a
    /// separate flag that overrides without overwriting the user's
    /// preference, so flipping keep-warm off restores the original
    /// timeout.
    configured_timeout: RwLock<Duration>,
    /// Cluster-wide override flipped by
    /// [`apply_keep_warm`]. When `true`, [`effective_timeout`]
    /// returns `Duration::MAX` regardless of `configured_timeout`,
    /// which prevents the timer from firing.
    keep_warm: AtomicBool,
    cmd: Mutex<TimerCmd>,
    cv: Condvar,
}

impl IdleControl {
    fn effective_timeout(&self) -> Duration {
        if self.keep_warm.load(Ordering::Relaxed) {
            Duration::MAX
        } else {
            *self
                .configured_timeout
                .read()
                .expect("configured_timeout rwlock poisoned")
        }
    }
}

/// Drop guard that lives only on user-facing `ModelHandle` clones —
/// not on the timer thread's captures. When the last clone of the
/// handle is dropped, the strong count on this Arc hits 0, `Drop`
/// runs, and we signal + join the timer thread before the handle's
/// other Arcs (state, inner, loader) deallocate.
///
/// The timer thread does **not** hold an `Arc<ShutdownSignal>` — only
/// the `Arc<IdleControl>` it shares with the handle. That asymmetry
/// is what makes "last handle clone dropped" detectable.
struct ShutdownSignal {
    idle: Arc<IdleControl>,
    timer_thread: Mutex<Option<JoinHandle<()>>>,
}

impl Drop for ShutdownSignal {
    fn drop(&mut self) {
        // Tell the timer thread to exit + wake it.
        {
            let mut cmd = self.idle.cmd.lock().expect("timer cmd poisoned");
            cmd.shutdown = true;
            cmd.deadline = None;
        }
        self.idle.cv.notify_all();
        // Join so we don't return while the timer thread still holds
        // strong refs to the handle's other Arcs.
        if let Some(jh) = self
            .timer_thread
            .lock()
            .expect("timer_thread mutex poisoned")
            .take()
        {
            let _ = jh.join();
        }
    }
}

/// A loadable, unloadable resource. `T` is the underlying handle the
/// model exposes — for the recognizer that's `Box<dyn Recognizer>`,
/// for the future cleanup LLM that's the LLM client.
///
/// Cloning a `ModelHandle` shares state — both clones point at the
/// same model. Internally everything is `Arc<...>`.
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
    /// `None` when constructed via [`ModelHandle::new`] (no timer);
    /// `Some(_)` when constructed via [`ModelHandle::with_idle_timeout`].
    idle: Option<Arc<IdleControl>>,
    /// See [`ShutdownSignal`]. Held only on user-facing clones; not
    /// on the timer thread.
    #[allow(dead_code)]
    shutdown: Option<Arc<ShutdownSignal>>,
}

impl<T: Send + 'static> Clone for ModelHandle<T> {
    fn clone(&self) -> Self {
        Self {
            label: self.label.clone(),
            state: Arc::clone(&self.state),
            inner: Arc::clone(&self.inner),
            last_load_rss_delta: Arc::clone(&self.last_load_rss_delta),
            loader: Arc::clone(&self.loader),
            idle: self.idle.as_ref().map(Arc::clone),
            shutdown: self.shutdown.as_ref().map(Arc::clone),
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
            idle: None,
            shutdown: None,
        }
    }

    /// Build a handle with an idle-timer worker thread that auto-
    /// unloads the model after `idle_timeout` of no `use_with`
    /// activity. Pass [`Duration::MAX`] to disable firing entirely
    /// (the "keep warm" path); the worker still spawns so a later
    /// `set_idle_timeout` can re-enable firing without restarting.
    pub fn with_idle_timeout<F>(label: &str, loader: F, idle_timeout: Duration) -> Self
    where
        F: Fn() -> Result<T, String> + Send + Sync + 'static,
    {
        let h = Self::new(label, loader);
        // Inherit the cluster-wide keep-warm preference at construction
        // time. `apply_keep_warm` will reach this handle on every
        // future flip via the registry below.
        let initial_keep_warm = crate::settings::keep_models_warm();
        let idle = Arc::new(IdleControl {
            configured_timeout: RwLock::new(idle_timeout),
            keep_warm: AtomicBool::new(initial_keep_warm),
            cmd: Mutex::new(TimerCmd {
                deadline: None,
                shutdown: false,
            }),
            cv: Condvar::new(),
        });
        register_handle(&idle);
        // Captures for the worker thread — Arcs of the data it needs
        // to fire `unload`. Crucially does NOT clone `shutdown`, so
        // when the last user-facing handle drops we can detect it.
        let timer_thread = spawn_timer_thread(
            Arc::clone(&idle),
            Arc::clone(&h.state),
            Arc::clone(&h.inner),
            h.label.clone(),
        );
        let shutdown = Arc::new(ShutdownSignal {
            idle: Arc::clone(&idle),
            timer_thread: Mutex::new(Some(timer_thread)),
        });
        Self {
            idle: Some(idle),
            shutdown: Some(shutdown),
            ..h
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

    /// Read-only borrow of the loaded model. Returns `None` when the
    /// handle is `Unloaded` (or transitioning) — does NOT auto-load
    /// and does NOT transition to `Active`. Used for cheap diagnostic
    /// readouts (e.g. `Recognizer::active_ep`) that must not trigger
    /// a 200–500 ms cold load just to render a label.
    ///
    /// Holds the inner mutex for the duration of the closure, so keep
    /// `f` short — concurrent `use_with` callers will block.
    pub fn try_inspect<R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        let inner = self.inner.lock().ok()?;
        inner.as_ref().map(f)
    }

    /// Update the idle timeout. Takes effect immediately for the next
    /// rearm; if currently `Loaded`, also re-arms with the new value
    /// so a "keep warm" → "release after 30 s" flip doesn't have to
    /// wait for a `use_with` cycle to land. Errors if the handle was
    /// constructed without an idle timer.
    pub fn set_idle_timeout(&self, new: Duration) -> Result<(), String> {
        let idle = self
            .idle
            .as_ref()
            .ok_or_else(|| format!("model `{}` has no idle timer", self.label))?;
        *idle
            .configured_timeout
            .write()
            .expect("configured_timeout rwlock poisoned") = new;
        if matches!(self.state(), LifecycleState::Loaded) {
            self.arm_timer();
        }
        Ok(())
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
                drop(state);

                self.arm_timer();
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
        drop(state);

        // No idle deadline makes sense once Unloaded.
        self.cancel_timer();
        Ok(())
    }

    /// Borrow the loaded model for a single call. Auto-loads if
    /// currently `Unloaded`. Concurrent callers serialize through the
    /// inner mutex.
    ///
    /// State sequence per call: caller-state → `Active` → `Loaded`.
    /// While Active the idle timer is cancelled; on return to
    /// `Loaded` it re-arms with the configured timeout.
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

        // Pause the idle timer while we run — guarantees the timer
        // can't race in and unload the model out from under us.
        self.cancel_timer();

        // Transition state Loaded → Active under the state lock,
        // briefly. Reject if we're not in a usable state.
        {
            let mut state = self.state.lock().expect("state lock poisoned");
            match *state {
                LifecycleState::Loaded => *state = LifecycleState::Active,
                LifecycleState::Active => {
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

        // Back to Loaded + re-arm the idle timer.
        let mut state = self.state.lock().expect("state lock poisoned");
        *state = LifecycleState::Loaded;
        drop(state);
        self.arm_timer();
        Ok(result)
    }

    /// (Re)arm the idle timer with the currently-configured timeout.
    /// No-op when the handle has no timer. When the effective timeout
    /// is `Duration::MAX` (either because `configured_timeout` is MAX
    /// or because `keep_warm` is set), clears any pending deadline so
    /// the timer goes fully dormant.
    fn arm_timer(&self) {
        if let Some(idle) = &self.idle {
            let timeout = idle.effective_timeout();
            let mut cmd = idle.cmd.lock().expect("cmd lock poisoned");
            if timeout == Duration::MAX {
                cmd.deadline = None;
            } else {
                cmd.deadline = Some(Instant::now() + timeout);
            }
            drop(cmd);
            idle.cv.notify_all();
        }
    }

    /// Cancel any pending fire. No-op when the handle has no timer.
    fn cancel_timer(&self) {
        if let Some(idle) = &self.idle {
            let mut cmd = idle.cmd.lock().expect("cmd lock poisoned");
            cmd.deadline = None;
            drop(cmd);
            idle.cv.notify_all();
        }
    }
}

/// Process-global registry of every live `ModelHandle` with an idle
/// timer. Stores `Weak<IdleControl>` so dropped handles fall out of
/// the list naturally on the next [`apply_keep_warm`] sweep — we
/// don't need a separate deregister hook on `ShutdownSignal::drop`.
///
/// Read-side usage (TASK-62.4): the Tauri
/// `settings_set_keep_models_warm` command persists the new value,
/// flips the lock-free atomic in [`crate::settings`], then calls
/// [`apply_keep_warm`] which walks this registry and pushes the new
/// flag into every live `IdleControl`.
fn registry() -> &'static Mutex<Vec<Weak<IdleControl>>> {
    static REGISTRY: OnceLock<Mutex<Vec<Weak<IdleControl>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}

fn register_handle(idle: &Arc<IdleControl>) {
    let mut g = registry().lock().expect("registry lock poisoned");
    // Compact dead Weaks while we're here so the list doesn't grow
    // unbounded across long sessions where handles are constructed
    // and dropped.
    g.retain(|w| w.strong_count() > 0);
    g.push(Arc::downgrade(idle));
}

/// Push a new cluster-wide keep-warm value into every registered
/// `ModelHandle` without restarting the app. Called by the Tauri
/// `settings_set_keep_models_warm` command after the JSON write
/// succeeds and the lock-free `KEEP_MODELS_WARM` cache has been
/// updated.
///
/// On `keep_warm = true`: clears any pending deadline on each handle
/// — the timer thread wakes, sees `effective_timeout == MAX`, parks.
///
/// On `keep_warm = false`: re-arms each `Loaded` handle with the
/// user's `configured_timeout`. `Active` / `Loading` / `Unloaded`
/// handles are left alone — the next legal transition will arm the
/// timer.
pub fn apply_keep_warm(keep_warm: bool) {
    let mut g = registry().lock().expect("registry lock poisoned");
    g.retain(|weak| {
        let Some(idle) = weak.upgrade() else {
            return false;
        };
        idle.keep_warm.store(keep_warm, Ordering::Relaxed);
        let mut cmd = idle.cmd.lock().expect("cmd lock poisoned");
        if keep_warm {
            // Cancel any pending fire — keep-warm wins.
            cmd.deadline = None;
        } else {
            // Re-arm only if a deadline was already set (i.e. we
            // were on a fire path that keep-warm hadn't cancelled
            // yet, or we're flipping back from on→off and the
            // handle is currently Loaded — in which case there
            // wasn't a deadline armed). To handle the latter
            // cleanly without re-reading state, we set a fresh
            // deadline here from the configured timeout. The
            // timer's race-tolerant `fire_unload` skips if state
            // isn't `Loaded` at fire time, so doing this when the
            // handle is Active/Loading/Unloaded is safe.
            let timeout = *idle
                .configured_timeout
                .read()
                .expect("configured_timeout rwlock poisoned");
            if timeout < Duration::MAX {
                cmd.deadline = Some(Instant::now() + timeout);
            }
        }
        drop(cmd);
        idle.cv.notify_all();
        true
    });
}

/// Number of registered handles with at least one strong reference.
/// Test helper; kept `pub(crate)` so external consumers don't take a
/// dependency on the count.
#[cfg(test)]
fn registered_handle_count() -> usize {
    let mut g = registry().lock().expect("registry lock poisoned");
    g.retain(|w| w.strong_count() > 0);
    g.len()
}

/// Spawn the per-handle idle-timer worker. Each handle constructed
/// via [`ModelHandle::with_idle_timeout`] gets exactly one of these.
/// The worker runs until `cmd.shutdown == true` is observed, which
/// [`ShutdownSignal::drop`] sets when the last user-facing handle
/// clone is dropped.
fn spawn_timer_thread<T: Send + 'static>(
    idle: Arc<IdleControl>,
    state: Arc<Mutex<LifecycleState>>,
    inner: Arc<Mutex<Option<T>>>,
    label: String,
) -> JoinHandle<()> {
    let thread_label = label.clone();
    thread::Builder::new()
        .name(format!("ow-idle-{label}"))
        .spawn(move || idle_timer_loop(idle, state, inner, thread_label))
        .expect("spawn idle-timer thread")
}

fn idle_timer_loop<T: Send + 'static>(
    idle: Arc<IdleControl>,
    state: Arc<Mutex<LifecycleState>>,
    inner: Arc<Mutex<Option<T>>>,
    label: String,
) {
    let already_fired = AtomicBool::new(false);
    loop {
        let mut cmd = idle.cmd.lock().expect("cmd lock poisoned");
        if cmd.shutdown {
            return;
        }
        match cmd.deadline {
            None => {
                // No deadline armed — sleep until rearm or shutdown.
                let guard = idle.cv.wait(cmd).expect("cv wait poisoned");
                drop(guard);
                continue;
            }
            Some(deadline) => {
                let now = Instant::now();
                if now >= deadline {
                    // Time to fire.
                    cmd.deadline = None;
                    drop(cmd);
                    fire_unload(&state, &inner, &label, &already_fired);
                    continue;
                }
                let wait_for = deadline - now;
                let (guard, _result) = idle
                    .cv
                    .wait_timeout(cmd, wait_for)
                    .expect("cv wait_timeout poisoned");
                drop(guard);
                // Re-loop and re-evaluate. Whether we timed out or
                // were notified (rearm / cancel / shutdown), the
                // top-of-loop checks the cmd anew.
                continue;
            }
        }
    }
}

/// The actual unload action the timer takes when its deadline elapses.
/// Race-tolerant: if state is no longer `Loaded` (because `use_with`
/// transitioned us to `Active` first, or another thread already
/// unloaded), we silently skip — losing the race is fine.
fn fire_unload<T: Send + 'static>(
    state: &Mutex<LifecycleState>,
    inner: &Mutex<Option<T>>,
    label: &str,
    already_fired: &AtomicBool,
) {
    already_fired.store(true, Ordering::Relaxed);
    let _ = label;
    let mut s = state.lock().expect("state lock poisoned");
    if !matches!(*s, LifecycleState::Loaded) {
        return;
    }
    *s = LifecycleState::Releasing;
    drop(s);
    {
        let mut g = inner.lock().expect("inner lock poisoned");
        g.take();
    }
    *state.lock().expect("state lock poisoned") = LifecycleState::Unloaded;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

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

        *h.state.lock().unwrap() = LifecycleState::Loaded;
    }

    #[test]
    fn failed_loader_leaves_state_unloaded_and_propagates_error() {
        let calls = Arc::new(AtomicUsize::new(0));
        let h: ModelHandle<u32> = ModelHandle::new("mock", mock_loader(calls.clone(), 2));

        let err = h.load().expect_err("first load should fail");
        assert!(err.contains("synthetic loader failure"));
        assert_eq!(h.state(), LifecycleState::Unloaded);

        let err = h.load().expect_err("second load should fail");
        assert!(err.contains("synthetic loader failure"));
        assert_eq!(h.state(), LifecycleState::Unloaded);

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
        let est = h.current_memory_estimate();
        assert!(est < u64::MAX / 2, "estimate looks corrupted: {est}");
    }

    // -----------------------------------------------------------------
    // Idle timer (TASK-62.3)
    //
    // Sleep-based timer tests are inherently scheduler-sensitive.
    // We use generous timeouts (200–400 ms) to keep flakiness low on
    // loaded CI runners while still catching the first-order behavior.
    // -----------------------------------------------------------------

    /// Wait up to `max_wait` for `pred` to become true, polling every
    /// 10 ms. Avoids the "sleep then check" flakiness for state-change
    /// assertions that depend on a timer thread's scheduling.
    fn wait_until<F: FnMut() -> bool>(max_wait: Duration, mut pred: F) -> bool {
        let deadline = Instant::now() + max_wait;
        while Instant::now() < deadline {
            if pred() {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        pred()
    }

    #[test]
    fn idle_timer_unloads_after_deadline() {
        let calls = Arc::new(AtomicUsize::new(0));
        let h: ModelHandle<u32> = ModelHandle::with_idle_timeout(
            "mock-idle",
            mock_loader(calls.clone(), 0),
            Duration::from_millis(80),
        );
        h.load().unwrap();
        assert_eq!(h.state(), LifecycleState::Loaded);

        let unloaded = wait_until(Duration::from_millis(500), || {
            h.state() == LifecycleState::Unloaded
        });
        assert!(unloaded, "expected timer to unload; state={:?}", h.state());
    }

    #[test]
    fn use_with_extends_idle_window() {
        let calls = Arc::new(AtomicUsize::new(0));
        let h: ModelHandle<u32> = ModelHandle::with_idle_timeout(
            "mock-extend",
            mock_loader(calls.clone(), 0),
            Duration::from_millis(150),
        );
        h.load().unwrap();
        // Halfway through the original window, use the model — that
        // resets the timer to a fresh 150 ms window.
        thread::sleep(Duration::from_millis(75));
        assert_eq!(h.state(), LifecycleState::Loaded);
        let _ = h.use_with(|t| *t).unwrap();

        // 100 ms after the use_with — beyond the *original* 150 ms
        // deadline (now ~175 ms total) but well inside the new
        // window. Must still be Loaded.
        thread::sleep(Duration::from_millis(100));
        assert_eq!(
            h.state(),
            LifecycleState::Loaded,
            "use_with did not reset the idle window"
        );

        // Now wait long enough for the *new* deadline to elapse.
        let unloaded = wait_until(Duration::from_millis(500), || {
            h.state() == LifecycleState::Unloaded
        });
        assert!(
            unloaded,
            "expected eventual unload after extended window; state={:?}",
            h.state()
        );
    }

    #[test]
    fn keep_warm_via_duration_max_keeps_loaded() {
        let calls = Arc::new(AtomicUsize::new(0));
        let h: ModelHandle<u32> = ModelHandle::with_idle_timeout(
            "mock-warm",
            mock_loader(calls.clone(), 0),
            Duration::MAX,
        );
        h.load().unwrap();
        // Plenty of wall time — if a fire was going to happen it
        // would have by now.
        thread::sleep(Duration::from_millis(250));
        assert_eq!(h.state(), LifecycleState::Loaded);
    }

    #[test]
    fn set_idle_timeout_to_max_cancels_pending_fire() {
        let calls = Arc::new(AtomicUsize::new(0));
        let h: ModelHandle<u32> = ModelHandle::with_idle_timeout(
            "mock-flip",
            mock_loader(calls.clone(), 0),
            Duration::from_millis(80),
        );
        h.load().unwrap();
        // Flip to "keep warm" before the original deadline elapses.
        thread::sleep(Duration::from_millis(20));
        h.set_idle_timeout(Duration::MAX).unwrap();

        thread::sleep(Duration::from_millis(200));
        assert_eq!(
            h.state(),
            LifecycleState::Loaded,
            "set_idle_timeout(MAX) should have cancelled the pending fire"
        );
    }

    #[test]
    fn dropping_last_clone_shuts_down_timer_thread() {
        // Sanity check: a handle constructed with_idle_timeout, then
        // dropped, must not deadlock on the join in
        // `ShutdownSignal::drop`. If the worker fails to honor the
        // shutdown signal this test hangs — Cargo's per-test timeout
        // surfaces that as a failure.
        let calls = Arc::new(AtomicUsize::new(0));
        {
            let h: ModelHandle<u32> = ModelHandle::with_idle_timeout(
                "mock-drop",
                mock_loader(calls.clone(), 0),
                Duration::from_secs(60),
            );
            h.load().unwrap();
            // h goes out of scope here.
        }
        // If we get here without hanging, the timer thread was joined
        // cleanly.
        assert!(true);
    }

    #[test]
    fn set_idle_timeout_errors_on_handle_without_timer() {
        let calls = Arc::new(AtomicUsize::new(0));
        let h: ModelHandle<u32> = ModelHandle::new("mock-no-timer", mock_loader(calls, 0));
        let err = h
            .set_idle_timeout(Duration::from_millis(50))
            .expect_err("must error without timer");
        assert!(err.contains("no idle timer"));
    }

    // -----------------------------------------------------------------
    // Registry + apply_keep_warm (TASK-62.4)
    // -----------------------------------------------------------------

    #[test]
    fn apply_keep_warm_true_cancels_pending_fire_on_registered_handle() {
        let calls = Arc::new(AtomicUsize::new(0));
        let h: ModelHandle<u32> = ModelHandle::with_idle_timeout(
            "mock-keepwarm-on",
            mock_loader(calls, 0),
            Duration::from_millis(80),
        );
        h.load().unwrap();

        // Flip keep-warm ON before the deadline elapses.
        thread::sleep(Duration::from_millis(20));
        apply_keep_warm(true);

        // Wait past the original deadline. Handle must still be
        // Loaded — the registry sweep cancelled the pending fire.
        thread::sleep(Duration::from_millis(150));
        assert_eq!(
            h.state(),
            LifecycleState::Loaded,
            "apply_keep_warm(true) failed to cancel the pending fire"
        );

        // Reset so other tests aren't influenced by the global flag.
        apply_keep_warm(false);
    }

    #[test]
    fn apply_keep_warm_false_rearms_with_configured_timeout() {
        let calls = Arc::new(AtomicUsize::new(0));
        let h: ModelHandle<u32> = ModelHandle::with_idle_timeout(
            "mock-keepwarm-off",
            mock_loader(calls, 0),
            Duration::from_millis(80),
        );
        h.load().unwrap();
        apply_keep_warm(true);
        // Confirm handle is parked Loaded indefinitely.
        thread::sleep(Duration::from_millis(150));
        assert_eq!(h.state(), LifecycleState::Loaded);

        // Flip back OFF — registry sweep re-arms with the original
        // configured timeout. Handle should auto-unload after ~80 ms.
        apply_keep_warm(false);
        let unloaded = wait_until(Duration::from_millis(500), || {
            h.state() == LifecycleState::Unloaded
        });
        assert!(
            unloaded,
            "apply_keep_warm(false) failed to re-arm; state={:?}",
            h.state()
        );
    }

    #[test]
    fn dropped_handles_fall_out_of_registry() {
        // Identify our handle's `IdleControl` Arc via `Weak::ptr_eq`
        // so the test stays correct under parallel execution: other
        // tests' handles may also be in the registry, but we only
        // care that *our specific* Weak is pruned after drop.
        let weak_to_dead_handle = {
            let h: ModelHandle<u32> = ModelHandle::with_idle_timeout(
                "mock-reg-drop",
                mock_loader(Arc::new(AtomicUsize::new(0)), 0),
                Duration::from_secs(60),
            );
            let live_weak = Arc::downgrade(h.idle.as_ref().unwrap());
            assert!(
                live_weak.strong_count() > 0,
                "handle Arc should be live before drop"
            );
            // Sanity: our weak resolves to a registry entry by
            // pointer identity (not just an Arc with the same data).
            let in_registry = registry()
                .lock()
                .unwrap()
                .iter()
                .any(|w| Weak::ptr_eq(w, &live_weak));
            assert!(in_registry, "handle should be registered before drop");
            live_weak
        };

        // Trigger a sweep — apply_keep_warm walks the registry with
        // retain, dropping dead Weaks. After our handle drops, our
        // Weak is dead.
        apply_keep_warm(false);

        let still_present = registry()
            .lock()
            .unwrap()
            .iter()
            .any(|w| Weak::ptr_eq(w, &weak_to_dead_handle));
        assert!(
            !still_present,
            "registry should have pruned the dropped handle's Weak"
        );
    }
}
