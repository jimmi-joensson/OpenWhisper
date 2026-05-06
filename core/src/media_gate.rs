//! Pause/resume gate for "other apps' audio while OpenWhisper is
//! recording". Owns the idempotency invariant — only the first
//! [`pause`] call inside a recording cycle forwards to the platform
//! controller; subsequent calls (preview→recording transition, hotkey
//! double-fire) no-op so [`take_paused_flag`] cleanly pairs with a
//! single resume.
//!
//! Platform behavior lives in shell-side impls of [`MediaController`]
//! (AppleScript MediaRemote on macOS, SMTC on Windows). Core owns the
//! gate state machine and the cross-platform diagnostic shape; the
//! shell wires a controller via [`OnceLock`]/[`Arc`] and invokes the
//! gate fns from the dictation lifecycle.
//!
//! Diagnostic surface ([`PauseDiagnostic`]) is opt-in per-platform via
//! the trait — Mac populates it on AppleScript/TCC denials so the UI
//! can render an actionable "grant Automation" banner; Windows leaves
//! the default `None` because SMTC has no equivalent silent-failure
//! mode.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

/// Platform pause/resume contract. Implemented by the shell's
/// `MacMediaController` / `WindowsMediaController`. The trait is
/// `Send + Sync` so a single `Arc<impl MediaController>` can be shared
/// across the hotkey thread, the resume worker, and the focus-event
/// handler.
pub trait MediaController: Send + Sync {
    /// Pause whatever is currently playing in apps this controller
    /// knows about. Returns true if the call paused at least one
    /// session — false means nothing to pause OR a silent failure
    /// (in which case [`MediaController::last_pause_diagnostic`] may
    /// surface a reason).
    fn pause_now(&self) -> bool;

    /// Resume the sessions paused by the matching prior `pause_now`.
    /// Idempotent — calling without a prior pause is a no-op.
    fn resume_now(&self);

    /// Latest diagnostic from `pause_now`. `None` means either the
    /// most recent pause succeeded (paused at least one session) or
    /// the platform has no diagnostic surface. Default impl returns
    /// `None`; Mac overrides.
    fn last_pause_diagnostic(&self) -> Option<PauseDiagnostic> {
        None
    }

    /// Re-probe per-app authorization (Mac TCC) without taking
    /// pause/resume action. Used on app focus regain to clear the
    /// "grant Automation" banner the moment the user comes back from
    /// System Settings. Default impl returns `None` — platforms
    /// without a per-app TCC layer skip this.
    fn probe_authorization(&self) -> Option<PauseDiagnostic> {
        None
    }
}

/// Cross-platform pause-failure diagnostic. `reason` is a stable
/// machine tag the UI switches on to render the right banner;
/// `detail` is human-readable context (which apps, which error
/// codes) suitable for log surfacing.
///
/// Tag values currently in use:
///
/// - `"not_authorized"` — Mac AppleScript Automation TCC denial.
///   User must grant Automation in System Settings → Privacy &
///   Security → Automation → OpenWhisper.
/// - `"no_known_player"` — Script ran cleanly but no controllable
///   app was both running and playing. Out of scope (browser tabs
///   on Mac, etc.) — not actionable.
/// - `"other"` — generic failure. Detail field has the codes.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize)]
pub struct PauseDiagnostic {
    pub reason: &'static str,
    pub detail: String,
}

impl PauseDiagnostic {
    /// Construct a diagnostic. `#[non_exhaustive]` blocks struct
    /// expressions from outside the crate; this is the supported
    /// constructor.
    pub fn new(reason: &'static str, detail: String) -> Self {
        Self { reason, detail }
    }
}

/// Shared idempotency state for the [`pause`] / [`take_paused_flag`]
/// pair. One `&'static MediaGateState` per process is the typical
/// pattern — reach the canonical instance via [`default_gate_state`].
pub struct MediaGateState {
    paused_by_us: AtomicBool,
}

impl MediaGateState {
    /// Construct a fresh gate. `const` so callers can park instances
    /// in a `static`. Most code wants [`default_gate_state`] instead.
    pub const fn new() -> Self {
        Self {
            paused_by_us: AtomicBool::new(false),
        }
    }

    /// True if a prior `pause` is currently held — i.e. a matching
    /// `take_paused_flag` hasn't run yet. Useful for diagnostics; the
    /// gate fns themselves don't require callers to check this.
    pub fn is_held(&self) -> bool {
        self.paused_by_us.load(Ordering::Relaxed)
    }
}

impl Default for MediaGateState {
    fn default() -> Self {
        Self::new()
    }
}

/// Process-wide gate. Lazy-init via `static` so callers can pass the
/// same `&'static MediaGateState` from any thread.
pub fn default_gate_state() -> &'static MediaGateState {
    static GATE: MediaGateState = MediaGateState::new();
    &GATE
}

/// Idempotent pause: only forwards to `controller.pause_now()` if
/// the gate isn't already held. Returns `true` if a real pause was
/// issued (and the gate is now held), `false` if the call was a
/// no-op (gate already held, or controller paused nothing).
///
/// Caller is responsible for the higher-level "should we even
/// attempt to pause" decision — e.g. the user-facing
/// `pause_audio_during_dictation` setting. The gate is purely about
/// not double-pausing inside one recording cycle.
pub fn pause<C: MediaController + ?Sized>(controller: &C, gate: &MediaGateState) -> bool {
    if gate.paused_by_us.load(Ordering::Relaxed) {
        return false;
    }
    let did_pause = controller.pause_now();
    if did_pause {
        gate.paused_by_us.store(true, Ordering::Relaxed);
    }
    did_pause
}

/// Take ownership of the gate flag if it is currently held. Returns
/// `true` if the caller now owns a matching `resume_now` call;
/// `false` means no resume is needed (no prior `pause` issued one,
/// or another caller already took the flag).
///
/// Used by the shell's `resume_audio_after_recording` to decide
/// whether to spawn the resume worker thread. The shell owns the
/// thread spawn because `resume_now` blocks on platform polls
/// (Mac CoreAudio sample-rate watch; Win BT switchback sleep) and
/// the dictation hotkey thread mustn't block on those.
pub fn take_paused_flag(gate: &MediaGateState) -> bool {
    gate.paused_by_us.swap(false, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubController {
        pause_returns: bool,
        pause_calls: AtomicBool,
        resume_calls: AtomicBool,
    }

    impl StubController {
        fn new(pause_returns: bool) -> Self {
            Self {
                pause_returns,
                pause_calls: AtomicBool::new(false),
                resume_calls: AtomicBool::new(false),
            }
        }
    }

    impl MediaController for StubController {
        fn pause_now(&self) -> bool {
            self.pause_calls.store(true, Ordering::Relaxed);
            self.pause_returns
        }

        fn resume_now(&self) {
            self.resume_calls.store(true, Ordering::Relaxed);
        }
    }

    #[test]
    fn pause_holds_gate_when_controller_paused() {
        let gate = MediaGateState::new();
        let ctrl = StubController::new(true);
        assert!(pause(&ctrl, &gate));
        assert!(gate.is_held());
    }

    #[test]
    fn pause_does_not_hold_gate_when_controller_paused_nothing() {
        let gate = MediaGateState::new();
        let ctrl = StubController::new(false);
        assert!(!pause(&ctrl, &gate));
        assert!(!gate.is_held());
    }

    #[test]
    fn second_pause_inside_held_gate_is_noop() {
        let gate = MediaGateState::new();
        let first = StubController::new(true);
        assert!(pause(&first, &gate));

        let second = StubController::new(true);
        assert!(!pause(&second, &gate));
        // Second controller never had pause_now invoked.
        assert!(!second.pause_calls.load(Ordering::Relaxed));
    }

    #[test]
    fn take_paused_flag_clears_held_state() {
        let gate = MediaGateState::new();
        let ctrl = StubController::new(true);
        assert!(pause(&ctrl, &gate));
        assert!(take_paused_flag(&gate));
        assert!(!gate.is_held());
        // Second take returns false — nothing to take.
        assert!(!take_paused_flag(&gate));
    }

    #[test]
    fn take_paused_flag_returns_false_when_never_paused() {
        let gate = MediaGateState::new();
        assert!(!take_paused_flag(&gate));
    }

    #[test]
    fn pause_after_take_re_engages_controller() {
        let gate = MediaGateState::new();
        let ctrl = StubController::new(true);
        assert!(pause(&ctrl, &gate));
        assert!(take_paused_flag(&gate));
        // New cycle: pause should hit the controller again.
        let ctrl2 = StubController::new(true);
        assert!(pause(&ctrl2, &gate));
        assert!(ctrl2.pause_calls.load(Ordering::Relaxed));
    }
}
