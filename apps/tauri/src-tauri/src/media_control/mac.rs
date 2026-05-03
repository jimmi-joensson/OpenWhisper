//! macOS MediaController — pauses currently-playing audio (Spotify,
//! Apple Music, browser tabs, podcasts, anything else producing
//! output) while OpenWhisper is recording, then resumes after the mic
//! closes and Bluetooth has switched back to A2DP/stereo.
//!
//! Implementation: synthesize the system play/pause media key
//! (`NX_KEYTYPE_PLAY`) via `+[NSEvent otherEventWithType:…]` →
//! `CGEventPost(kCGHIDEventTap, …)`. The OS treats the event
//! identically to a hardware F8 press, so every well-behaved audio
//! source — Spotify, Apple Music, Safari/Chrome/Firefox tab media,
//! podcast apps — toggles its play state. Same path
//! BetterTouchTool / Hammerspoon / Caffeine use. No new TCC prompts:
//! posting at the HID layer reuses our existing Accessibility grant
//! (the hotkey CGEventTap establishes it at boot).
//!
//! Why NOT the previous AppleScript per-app pause: each
//! `tell application "X"` invocation triggers a per-app Automation
//! TCC prompt the first time the user runs it. For a v0.5 headline
//! feature ("the open alternative to Superwhisper"), stacking 2+
//! permission prompts per music app is exactly the friction we want
//! to avoid. AppleScript also can't reach browser-tab media at all.
//!
//! Why NOT MediaRemote (`MRMediaRemoteSendCommand` kMRPause/kMRPlay):
//! macOS 15.4+ enforces a `com.apple.*` entitlement check on
//! `MRMediaRemoteSendCommand` for non-Apple-signed processes, making
//! SET commands unreliable. A media-key synthesis path side-steps
//! the entitlement entirely and works the same on every macOS
//! version we support.
//!
//! Toggle gating (avoids the "stop with nothing playing → music
//! starts" regression and the "user-started-something-else-during-
//! recording → we pause it" regression): media keys toggle, so we
//! probe `kAudioDevicePropertyDeviceIsRunningSomewhere` on the
//! default-output device before each post.
//! - `pause_now`: enumerate active output processes via
//!   `kAudioHardwarePropertyProcessObjectList` +
//!   `kAudioProcessPropertyIsRunningOutput` to size the loop, then
//!   send up to that many media-key toggles with
//!   `INTER_TOGGLE_SLEEP_MS` between (gives `mediaremoted` time to
//!   re-elect the NowPlaying client). `is_audio_playing` is the
//!   actual termination signal, the count is the upper bound.
//!   Records `toggles_sent` so resume can replay the same number.
//! - `resume_now`: replay up to `toggles_sent` toggles, with the same
//!   `is_audio_playing` re-probe before each as an early-exit guard.
//!
//! Known multi-app resume limitation: the OS routes media keys to a
//! single NowPlaying client at a time. With one app paused, resume
//! is straightforward — the toggle goes to that app. With N>1 apps
//! paused, only one (the most recently NowPlaying) resumes; the
//! others stay paused. Continuing the loop after the first app
//! resumes would pause it again rather than resume the next paused
//! client, because NowPlaying election follows whatever is currently
//! producing audio. The early-exit re-probe is the safe choice; the
//! user resumes leftover apps manually. Tracked for follow-up;
//! deterministic multi-app resume requires a per-client SET path
//! (private `MRMediaRemoteSendCommandToApp` or a return to
//! AppleScript), both of which trade other regressions back in.
//!
//! Resume timing: Bluetooth headphones (AirPods etc.) switch from
//! A2DP/stereo to HFP/mono the moment the mic opens. Posting the
//! play key before BT has switched back means music briefly resumes
//! in mono. We capture the default-output device's nominal sample
//! rate at pause-time, then on stop poll until the rate has returned
//! to that value (with a 2 s cap) before posting. The
//! `behavior::bt_resume_delay_ms` setting is Windows-only — Windows
//! has no equivalent live-state signal (see `media_control/windows.rs`
//! and the BT entry in `openwhisper-platform-gotchas`).

use std::ffi::c_void;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use objc2::encode::{Encode, Encoding};
use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject};
use openwhisper_core::verbose_log;

use super::MediaController;

const fn fourcc(s: &[u8; 4]) -> u32 {
    ((s[0] as u32) << 24) | ((s[1] as u32) << 16) | ((s[2] as u32) << 8) | (s[3] as u32)
}
const KAUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE: u32 = fourcc(b"dOut");
const KAUDIO_DEVICE_PROPERTY_NOMINAL_SAMPLE_RATE: u32 = fourcc(b"nsrt");
/// `kAudioDevicePropertyDeviceIsRunningSomewhere` — non-zero when any
/// process has an active I/O proc on the device. The signal we use to
/// gate the play/pause toggle so it never starts music that wasn't
/// already playing.
const KAUDIO_DEVICE_PROPERTY_DEVICE_IS_RUNNING_SOMEWHERE: u32 = fourcc(b"gone");
/// `kAudioHardwarePropertyProcessObjectList` (macOS 14+) — variable-size
/// array of `AudioObjectID`s, one per process that has registered
/// audio I/O with HAL. We enumerate this to count *how many* clients
/// the play/pause key needs to silence in the multi-app case
/// (Spotify + browser-tab YouTube + Music + …) — each toggle hits
/// only the elected NowPlaying client, so N producers need N toggles.
const KAUDIO_HARDWARE_PROPERTY_PROCESS_OBJECT_LIST: u32 = fourcc(b"prs#");
/// `kAudioProcessPropertyIsRunningOutput` — non-zero on a process
/// AudioObject when that process is currently rendering output. Set
/// per-process; we only count processes where this is true.
const KAUDIO_PROCESS_PROPERTY_IS_RUNNING_OUTPUT: u32 = fourcc(b"piro");
const KAUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: u32 = fourcc(b"glob");
const KAUDIO_OBJECT_SYSTEM_OBJECT: u32 = 1;
const KAUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: u32 = 0;
const RESUME_RATE_WAIT_TIMEOUT_MS: u64 = 2000;
const RESUME_RATE_POLL_MS: u64 = 50;
/// Gap between successive media-key toggles in a multi-app pause/resume
/// loop. Has to give `mediaremoted` time to re-elect the NowPlaying
/// client after the previous toggle silenced (or resumed) one app —
/// CoreAudio device-state updates fast, but NowPlaying election is an
/// XPC bookkeeping pass at the daemon level, slower. 40 ms is ~2.5
/// frames at 60 Hz: imperceptible to the user even at N=3 (80 ms total
/// added latency), but gives mediaremoted reliable headroom on every
/// app class we've smoke-tested. Going lower risks NowPlaying-not-
/// elected-yet failures that look identical to the original
/// "second app didn't pause" bug.
const INTER_TOGGLE_SLEEP_MS: u64 = 40;
/// Fallback cap for the pause loop when process enumeration fails
/// (pre-14.0 host, selector-code drift, hostile audio source). Keeps
/// us from spinning forever if `is_audio_playing` mis-reports.
const MAX_TOGGLE_ITERATIONS: usize = 4;

/// `IOKit/hidsystem/ev_keymap.h` — `NX_KEYTYPE_PLAY` is the
/// system-defined "play/pause" multimedia key code. The OS routes it
/// through the same media-key dispatch as a real F8 press.
const NX_KEYTYPE_PLAY: i64 = 16;
/// State byte inside the data1 payload of an `NSSystemDefined` /
/// subtype-8 event. `0xa = NX_KEYDOWN`, `0xb = NX_KEYUP`. Apps watch
/// for the down→up pair; sending only one half is unreliable.
const NX_KEYDOWN: i64 = 0xa;
const NX_KEYUP: i64 = 0xb;
/// `NSEventType.systemDefined` raw value.
const NS_EVENT_TYPE_SYSTEM_DEFINED: u64 = 14;
/// Subtype tag for `NSSystemDefined` events that carry HID auxiliary
/// (multimedia) key state.
const HID_AUX_KEY_SUBTYPE: i16 = 8;
/// `kCGHIDEventTap` — post media keys at the HID layer so background
/// apps (Spotify minimised, Music in another Space, etc.) see them
/// the same way they'd see a real keyboard press.
const KCG_HID_EVENT_TAP: u32 = 0;

#[repr(C)]
#[derive(Clone, Copy)]
struct AudioObjectPropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

/// Local NSPoint mirror with a manual `Encode` impl. We could pull
/// `objc2-foundation` for `CGPoint`/`NSPoint`, but a 16-byte two-f64
/// struct doesn't justify a new direct dependency.
#[repr(C)]
#[derive(Clone, Copy)]
struct NSPoint {
    x: f64,
    y: f64,
}

unsafe impl Encode for NSPoint {
    const ENCODING: Encoding =
        Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
}

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyData(
        in_object_id: u32,
        in_address: *const AudioObjectPropertyAddress,
        in_qualifier_data_size: u32,
        in_qualifier_data: *const c_void,
        io_data_size: *mut u32,
        out_data: *mut c_void,
    ) -> i32;
    fn AudioObjectGetPropertyDataSize(
        in_object_id: u32,
        in_address: *const AudioObjectPropertyAddress,
        in_qualifier_data_size: u32,
        in_qualifier_data: *const c_void,
        out_data_size: *mut u32,
    ) -> i32;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventPost(tap: u32, event: *const c_void);
}

fn default_output_device() -> Option<u32> {
    let addr = AudioObjectPropertyAddress {
        selector: KAUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE,
        scope: KAUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        element: KAUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };
    let mut device_id: u32 = 0;
    let mut size: u32 = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            KAUDIO_OBJECT_SYSTEM_OBJECT,
            &addr,
            0,
            std::ptr::null(),
            &mut size,
            &mut device_id as *mut u32 as *mut c_void,
        )
    };
    if status != 0 || device_id == 0 {
        verbose_log!("[media_control.mac] default output device lookup failed: {status}");
        return None;
    }
    Some(device_id)
}

fn nominal_sample_rate(device: u32) -> Option<f64> {
    let addr = AudioObjectPropertyAddress {
        selector: KAUDIO_DEVICE_PROPERTY_NOMINAL_SAMPLE_RATE,
        scope: KAUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        element: KAUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };
    let mut value: f64 = 0.0;
    let mut size: u32 = std::mem::size_of::<f64>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            device,
            &addr,
            0,
            std::ptr::null(),
            &mut size,
            &mut value as *mut f64 as *mut c_void,
        )
    };
    if status != 0 {
        verbose_log!("[media_control.mac] nominal_sample_rate failed: {status}");
        return None;
    }
    Some(value)
}

/// Returns true when any process is rendering audio through the
/// default output device. `kAudioDevicePropertyDeviceIsRunningSomewhere`
/// is the documented Apple signal for "is the engine currently
/// active" — Spotify, Apple Music, browser tab media, podcasts apps,
/// AirPlay all run through CoreAudio I/O procs and therefore flip
/// this property to non-zero while playing.
fn is_audio_playing() -> bool {
    let Some(device) = default_output_device() else {
        return false;
    };
    let addr = AudioObjectPropertyAddress {
        selector: KAUDIO_DEVICE_PROPERTY_DEVICE_IS_RUNNING_SOMEWHERE,
        scope: KAUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        element: KAUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };
    let mut value: u32 = 0;
    let mut size: u32 = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            device,
            &addr,
            0,
            std::ptr::null(),
            &mut size,
            &mut value as *mut u32 as *mut c_void,
        )
    };
    if status != 0 {
        verbose_log!("[media_control.mac] is_audio_playing probe failed: {status}");
        return false;
    }
    value != 0
}

/// Returns the number of processes currently producing audio output
/// via the system HAL. Used to size the multi-app pause loop: each
/// media-key toggle hits only the elected NowPlaying client, so N
/// active producers need N successive toggles (with re-election gaps)
/// to silence everything.
///
/// Returns `None` when the host doesn't expose
/// `kAudioHardwarePropertyProcessObjectList` (pre-macOS 14, or some
/// future API drift) — caller falls back to `MAX_TOGGLE_ITERATIONS`
/// guarded by `is_audio_playing`.
fn count_active_output_processes() -> Option<usize> {
    let addr = AudioObjectPropertyAddress {
        selector: KAUDIO_HARDWARE_PROPERTY_PROCESS_OBJECT_LIST,
        scope: KAUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        element: KAUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };
    let mut bytes: u32 = 0;
    let status = unsafe {
        AudioObjectGetPropertyDataSize(
            KAUDIO_OBJECT_SYSTEM_OBJECT,
            &addr,
            0,
            std::ptr::null(),
            &mut bytes,
        )
    };
    if status != 0 {
        verbose_log!("[media_control.mac] process-list size lookup failed: {status}");
        return None;
    }
    if bytes == 0 {
        return Some(0);
    }
    let stride = std::mem::size_of::<u32>() as u32;
    let mut ids = vec![0u32; (bytes / stride) as usize];
    let mut io_size = bytes;
    let status = unsafe {
        AudioObjectGetPropertyData(
            KAUDIO_OBJECT_SYSTEM_OBJECT,
            &addr,
            0,
            std::ptr::null(),
            &mut io_size,
            ids.as_mut_ptr() as *mut c_void,
        )
    };
    if status != 0 {
        verbose_log!("[media_control.mac] process-list fetch failed: {status}");
        return None;
    }
    ids.truncate((io_size / stride) as usize);

    let mut active = 0usize;
    for &pid_obj in &ids {
        let prop = AudioObjectPropertyAddress {
            selector: KAUDIO_PROCESS_PROPERTY_IS_RUNNING_OUTPUT,
            scope: KAUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            element: KAUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };
        let mut value: u32 = 0;
        let mut sz: u32 = std::mem::size_of::<u32>() as u32;
        let s = unsafe {
            AudioObjectGetPropertyData(
                pid_obj,
                &prop,
                0,
                std::ptr::null(),
                &mut sz,
                &mut value as *mut u32 as *mut c_void,
            )
        };
        if s == 0 && value != 0 {
            active += 1;
        }
    }
    Some(active)
}

/// Synthesize one half (down or up) of a play/pause media-key press
/// via `+[NSEvent otherEventWithType:…]` → `CGEventPost`. Apple's
/// public API for posting `NSSystemDefined` subtype-8 events is to
/// build them through `NSEvent` and pull the underlying `CGEventRef`
/// — there's no `CGEventCreate` constructor that fills in the
/// HID-aux-key fields directly.
fn post_play_pause_key(state: i64) {
    unsafe {
        let Some(cls) = AnyClass::get(c"NSEvent") else {
            verbose_log!("[media_control.mac] NSEvent class lookup failed");
            return;
        };
        let data1: i64 = (NX_KEYTYPE_PLAY << 16) | (state << 8);
        let zero = NSPoint { x: 0.0, y: 0.0 };
        let event: *mut AnyObject = msg_send![
            cls,
            otherEventWithType: NS_EVENT_TYPE_SYSTEM_DEFINED,
            location: zero,
            modifierFlags: 0xa00u64,
            timestamp: 0.0f64,
            windowNumber: 0i64,
            context: std::ptr::null_mut::<AnyObject>(),
            subtype: HID_AUX_KEY_SUBTYPE,
            data1: data1,
            data2: -1i64,
        ];
        if event.is_null() {
            verbose_log!("[media_control.mac] NSEvent.otherEventWithType returned nil");
            return;
        }
        let cg_event: *const c_void = msg_send![event, CGEvent];
        if cg_event.is_null() {
            verbose_log!("[media_control.mac] NSEvent.CGEvent returned null");
            return;
        }
        CGEventPost(KCG_HID_EVENT_TAP, cg_event);
    }
}

fn toggle_play_pause() {
    post_play_pause_key(NX_KEYDOWN);
    post_play_pause_key(NX_KEYUP);
}

#[derive(Default)]
struct State {
    /// Number of media-key toggles `pause_now` posted to silence the
    /// active producers. `resume_now` replays up to this many toggles
    /// (with re-probe between, so an externally-resumed app stops the
    /// loop early). Zero iff `pause_now` saw nothing playing.
    toggles_sent: usize,
    /// Default-output device's nominal sample rate at pause-time.
    /// `resume_now` polls until the rate climbs back to this value
    /// (BT profile switchback signal) before posting the play key.
    original_sample_rate: Option<f64>,
}

pub struct MacMediaController {
    state: Mutex<State>,
}

impl MacMediaController {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State::default()),
        }
    }
}

impl MediaController for MacMediaController {
    fn pause_now(&self) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        *state = State::default();

        // Cap the loop at the count of currently-active output
        // producers (one toggle per app — media keys hit only the
        // elected NowPlaying client). Fall back to MAX_TOGGLE_ITERATIONS
        // when enumeration is unavailable; `is_audio_playing` is the
        // actual termination signal either way.
        let target_n = match count_active_output_processes() {
            Some(n) => n.min(MAX_TOGGLE_ITERATIONS),
            None => MAX_TOGGLE_ITERATIONS,
        };

        if target_n == 0 || !is_audio_playing() {
            verbose_log!(
                "[media_control.mac] pause_now: nothing playing (target_n={target_n}), skip"
            );
            return false;
        }

        let mut sent = 0usize;
        while sent < target_n {
            if !is_audio_playing() {
                break;
            }
            toggle_play_pause();
            sent += 1;
            if sent < target_n {
                thread::sleep(Duration::from_millis(INTER_TOGGLE_SLEEP_MS));
            }
        }

        if sent == 0 {
            return false;
        }

        state.toggles_sent = sent;
        if let Some(d) = default_output_device() {
            state.original_sample_rate = nominal_sample_rate(d);
        }
        verbose_log!(
            "[media_control.mac] pause_now: sent {sent}/{target_n} toggles, original_sample_rate={:?}",
            state.original_sample_rate
        );
        true
    }

    fn resume_now(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.toggles_sent == 0 {
            return;
        }
        let target = state.toggles_sent;
        let original_rate = state.original_sample_rate.take();
        state.toggles_sent = 0;
        drop(state);

        // Wait for BT to switch back to its pre-recording profile
        // (HFP→A2DP on AirPods, ~500–1000 ms) before posting the
        // play key. Signal: device's nominal sample rate climbs back
        // to the pre-pause value — adaptive detection, exits as soon
        // as the OS reports the switch is complete. Wired headsets
        // and BT-stayed-A2DP devices exit on the first poll.
        if let (Some(d), Some(rate)) = (default_output_device(), original_rate) {
            let deadline = Instant::now() + Duration::from_millis(RESUME_RATE_WAIT_TIMEOUT_MS);
            let mut waited_ms = 0u64;
            while Instant::now() < deadline {
                if let Some(current) = nominal_sample_rate(d) {
                    if (current - rate).abs() < 1.0 {
                        break;
                    }
                }
                thread::sleep(Duration::from_millis(RESUME_RATE_POLL_MS));
                waited_ms += RESUME_RATE_POLL_MS;
            }
            verbose_log!(
                "[media_control.mac] resume_now: waited {waited_ms} ms for sample rate to return to {rate}"
            );
        }

        // Replay up to `target` toggles. The re-probe before each
        // toggle is the early-exit guard: once any audio source is
        // producing again (a paused app resumed, OR the user started
        // something else mid-record, OR we caught the AirPod button),
        // stop — the next toggle would otherwise pause the running
        // app instead of resuming the next paused one. NowPlaying
        // election doesn't cleanly route media keys across multiple
        // *paused* clients, so multi-app resume is best-effort here:
        // the most-recently-NowPlaying client resumes, others stay
        // paused (user resumes them manually). Tracked as a known
        // limitation in the file header.
        let mut replayed = 0usize;
        while replayed < target {
            if is_audio_playing() {
                verbose_log!(
                    "[media_control.mac] resume_now: device running after {replayed}/{target} toggles, stop"
                );
                break;
            }
            toggle_play_pause();
            replayed += 1;
            if replayed < target {
                thread::sleep(Duration::from_millis(INTER_TOGGLE_SLEEP_MS));
            }
        }
        verbose_log!(
            "[media_control.mac] resume_now: replayed {replayed}/{target} toggles"
        );
    }
}
