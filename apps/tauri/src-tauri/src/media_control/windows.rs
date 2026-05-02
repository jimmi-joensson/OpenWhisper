//! Windows MediaController — pauses currently-playing apps via SMTC
//! while OpenWhisper is recording, then resumes them on stop/cancel.
//!
//! Why SMTC over kMRPause-equivalents: SMTC enumerates EVERY playing
//! session (Spotify desktop, Edge/Chrome tabs that registered media-
//! session, Groove, etc.), so we can pause-and-remember exactly the
//! sessions we touched and resume only those. No "stop starts music"
//! regression risk like the Mac MediaRemote path — TryPlayAsync is
//! per-session, not system-wide.
//!
//! No endpoint-volume fade or mute fallback in this revision. Earlier
//! iteration ramped `IAudioEndpointVolume` from current → 0 over 200 ms
//! before TryPauseAsync (and back up after TryPlayAsync) per the
//! original TASK-61.4 spec, but on the Windows test box the
//! fade-then-snap-back produced an audible hitch at record-start: the
//! snap-back from 0 → original happens immediately after the COM call
//! returns, before the source app's render thread has fully drained,
//! leaving a brief window where the source plays at full volume on a
//! freshly restored endpoint. Pause-send fire-and-forget (let each
//! source app apply its own pause envelope) eliminates the hitch.
//!
//! BT switchback wait: when the user is on Bluetooth headphones the
//! moment the mic opens the OS forces the link from A2DP/stereo to
//! HFP/mono, and the link does NOT switch back instantly when the mic
//! closes — there's a 0–3 s tail where sending `TryPlayAsync` would
//! resume music in mono. We delay `TryPlayAsync` on BT endpoints so
//! the user hears stereo when music returns.
//!
//! Wait shape: **fixed 4 s sleep gated on `is_default_render_bluetooth()`**
//! (PKEY_Device_EnumeratorName == "BTHENUM" / "BTHLEDEVICE"). Wired /
//! USB endpoints skip entirely and pay zero latency.
//!
//! Why a fixed sleep, not a deterministic poll: three approaches were
//! tried before settling here, all dead ends on Win11 + AirPods Pro:
//!  1. Polling plain `IAudioClient::GetMixFormat` sample rate — the
//!     engine's shared-mode mix format is decoupled from the BT codec
//!     layer and stays at 48 kHz across the profile flip.
//!  2. Polling `IAudioClient2 + AudioCategory_Communications +
//!     GetMixFormat` channel count — Microsoft Learn's "Communications
//!     Audio Format Capabilities" doc claims this reflects live codec
//!     state, but verbose-log evidence on AirPods Pro shows the value
//!     stuck at 1 (HFP capability) for the full 3 s timeout window
//!     even after BT had clearly switched back. It's a capability
//!     query in practice, not a state reflection.
//!  3. `OnDefaultDeviceChanged` / `OnDeviceStateChanged` notification
//!     callbacks — Win11 unifies A2DP/HFP into one IMMDevice with a
//!     stable ID, so neither fires on profile flips.
//! See `backlog/tasks/task-61.4*` Implementation Notes for the full
//! research trail.
//!
//! Default 5 s tuned for AirPods Pro on Win11 26200 — empirically
//! 3 s and 4 s both left audible mono tail-end on consecutive
//! recordings (BT codec stays warmer in HFP after repeated mic
//! cycles and takes longer to drop back). User-configurable via
//! Settings → General/Audio → "Bluetooth resume delay" (TASK-61.8)
//! so users on faster radios can dial down toward 0 (instant resume,
//! accept the mono tail) and users with slower stacks can dial up.
//! On stuck-HFP cases (some other app holding the mic) we end up
//! resuming in mono after the configured delay, accepted trade-off
//! vs. hanging the "stop recording" UX.
//!
//! Apartment model: WinRT requires CoInitializeEx. We init MTA per
//! thread (idempotent) so calls from the hotkey thread, emitter
//! thread, or resume worker all succeed regardless of which got there
//! first.

use std::cell::Cell;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use openwhisper_core::verbose_log;

use crate::behavior;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession,
    GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_EnumeratorName;
use windows::Win32::Media::Audio::{
    eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
};

use super::MediaController;

/// Per-thread COM init. `CoInitializeEx` is thread-scoped — `pause_now`
/// runs on the hotkey thread, `resume_now` is spawned on a fresh
/// "openwhisper-audio-resume" worker, so a process-wide OnceLock would
/// leave the resume thread uninitialized and `RequestAsync` would fail
/// silently. `thread_local!` gives us a once-per-thread init that's
/// idempotent across repeated calls within the same thread (S_FALSE on
/// subsequent calls is fine). MTA so we're not bound to a UI thread —
/// SMTC types are agile.
fn ensure_com_initialized() {
    thread_local! {
        static INIT: Cell<bool> = const { Cell::new(false) };
    }
    INIT.with(|flag| {
        if flag.get() {
            return;
        }
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        flag.set(true);
    });
}

/// True iff the default render endpoint reports its enumerator as
/// Bluetooth Classic ("BTHENUM") or Bluetooth LE Audio
/// ("BTHLEDEVICE"). Used to gate the BT switchback poll: wired/USB
/// endpoints (USB / HDAUDIO / etc.) skip the wait entirely. Returns
/// false on any COM failure rather than panicking — false-negative
/// just means we send play immediately, which is correct for
/// non-BT and accepts a mono blip on the rare BT case where the
/// property store read fails.
fn is_default_render_bluetooth() -> bool {
    unsafe {
        let Ok(enumerator): Result<IMMDeviceEnumerator, _> =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
        else {
            return false;
        };
        let Ok(device) = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) else {
            return false;
        };
        let Ok(props) = device.OpenPropertyStore(STGM_READ) else {
            return false;
        };
        let Ok(value) = props.GetValue(&PKEY_Device_EnumeratorName) else {
            return false;
        };
        // PROPVARIANT::to_string handles the VT_LPWSTR unwrap and
        // its Drop impl calls PropVariantClear so the embedded heap
        // string is freed when `value` goes out of scope.
        let s = value.to_string();
        matches!(s.as_str(), "BTHENUM" | "BTHLEDEVICE")
    }
}

fn list_playing_sessions() -> Vec<GlobalSystemMediaTransportControlsSession> {
    let manager = match GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
        Ok(op) => match op.get() {
            Ok(m) => m,
            Err(e) => {
                verbose_log!("[media_control.win] SessionManager.RequestAsync.get failed: {e}");
                return Vec::new();
            }
        },
        Err(e) => {
            verbose_log!("[media_control.win] SessionManager::RequestAsync failed: {e}");
            return Vec::new();
        }
    };
    let sessions = match manager.GetSessions() {
        Ok(s) => s,
        Err(e) => {
            verbose_log!("[media_control.win] manager.GetSessions failed: {e}");
            return Vec::new();
        }
    };
    let size = sessions.Size().unwrap_or(0);
    let mut playing = Vec::new();
    for i in 0..size {
        let Ok(session) = sessions.GetAt(i) else {
            continue;
        };
        let Ok(info) = session.GetPlaybackInfo() else {
            continue;
        };
        let Ok(status) = info.PlaybackStatus() else {
            continue;
        };
        if status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing {
            playing.push(session);
        }
    }
    playing
}

fn session_id(session: &GlobalSystemMediaTransportControlsSession) -> String {
    session
        .SourceAppUserModelId()
        .ok()
        .map(|h| h.to_string_lossy())
        .unwrap_or_default()
}

#[derive(Default)]
struct State {
    /// Sessions we successfully paused. `resume_now` plays back exactly
    /// these — never a generic "play all" that could resume a session
    /// the user paused themselves before recording.
    paused_sessions: Vec<GlobalSystemMediaTransportControlsSession>,
}

pub struct WindowsMediaController {
    state: Mutex<State>,
}

impl WindowsMediaController {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State::default()),
        }
    }
}

impl MediaController for WindowsMediaController {
    fn pause_now(&self) -> bool {
        ensure_com_initialized();
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        *state = State::default();

        let playing = list_playing_sessions();
        if playing.is_empty() {
            verbose_log!("[media_control.win] pause_now: no SMTC sessions Playing");
            return false;
        }

        let mut paused = Vec::with_capacity(playing.len());
        for session in &playing {
            let id = session_id(session);
            let result = session.TryPauseAsync().and_then(|op| op.get());
            match result {
                Ok(true) => paused.push(session.clone()),
                Ok(false) => {
                    verbose_log!(
                        "[media_control.win] TryPauseAsync returned false for {id}"
                    );
                }
                Err(e) => {
                    verbose_log!("[media_control.win] TryPauseAsync failed for {id}: {e}");
                }
            }
        }
        if paused.is_empty() {
            return false;
        }
        state.paused_sessions = paused;
        verbose_log!(
            "[media_control.win] pause_now: paused {} session(s)",
            state.paused_sessions.len()
        );
        true
    }

    fn resume_now(&self) {
        ensure_com_initialized();
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let sessions = std::mem::take(&mut state.paused_sessions);
        drop(state);

        if sessions.is_empty() {
            return;
        }

        // BT switchback wait. Gated on the default render endpoint's
        // PKEY_Device_EnumeratorName so wired/USB pays zero latency;
        // see module docs for why this is a fixed sleep rather than a
        // state-signal poll. Duration is user-tunable via Settings →
        // General/Audio → "Bluetooth resume delay" (TASK-61.8); cache
        // is hydrated at boot in `lib.rs::setup`. delay_ms == 0 is a
        // valid "I want instant resume" choice — skip the sleep.
        if is_default_render_bluetooth() {
            let delay_ms = behavior::bt_resume_delay_ms();
            if delay_ms > 0 {
                verbose_log!(
                    "[media_control.win] resume_now: BT endpoint detected, sleeping {delay_ms} ms before play"
                );
                thread::sleep(Duration::from_millis(delay_ms));
            } else {
                verbose_log!(
                    "[media_control.win] resume_now: BT endpoint detected but delay_ms=0, skipping wait"
                );
            }
        }

        for session in &sessions {
            let id = session_id(session);
            let result = session.TryPlayAsync().and_then(|op| op.get());
            match result {
                Ok(true) => {}
                Ok(false) => {
                    verbose_log!(
                        "[media_control.win] TryPlayAsync returned false for {id}"
                    );
                }
                Err(e) => {
                    verbose_log!("[media_control.win] TryPlayAsync failed for {id}: {e}");
                }
            }
        }
        verbose_log!(
            "[media_control.win] resume_now: resumed {} session(s)",
            sessions.len()
        );
    }
}
