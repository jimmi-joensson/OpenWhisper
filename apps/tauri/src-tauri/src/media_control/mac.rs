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
//! - `pause_now`: only post (and record `did_pause = true`) if the
//!   device is currently running. Otherwise leave everything alone.
//! - `resume_now`: only post if `did_pause` is true AND the device
//!   is currently NOT running (so we don't pause audio the user
//!   started in the meantime).
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
const KAUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: u32 = fourcc(b"glob");
const KAUDIO_OBJECT_SYSTEM_OBJECT: u32 = 1;
const KAUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: u32 = 0;
const RESUME_RATE_WAIT_TIMEOUT_MS: u64 = 2000;
const RESUME_RATE_POLL_MS: u64 = 50;

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
    /// True iff `pause_now` synthesized a pause toggle. `resume_now`
    /// only synthesizes the matching play toggle when this is true,
    /// so we never resume audio the user had paused externally
    /// before recording, and we never start playing when nothing was
    /// playing to begin with.
    did_pause: bool,
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

        if !is_audio_playing() {
            verbose_log!("[media_control.mac] pause_now: nothing playing, skip");
            return false;
        }

        toggle_play_pause();
        state.did_pause = true;
        if let Some(d) = default_output_device() {
            state.original_sample_rate = nominal_sample_rate(d);
        }
        verbose_log!(
            "[media_control.mac] pause_now: posted play/pause key, original_sample_rate={:?}",
            state.original_sample_rate
        );
        true
    }

    fn resume_now(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if !state.did_pause {
            return;
        }
        let original_rate = state.original_sample_rate.take();
        state.did_pause = false;
        drop(state);

        // Wait for BT to switch back to its pre-recording profile
        // (HFP→A2DP on AirPods, ~500–1000 ms) before posting the
        // play key. Signal: device's nominal sample rate climbs back
        // to the pre-pause value — adaptive detection, exits as soon
        // as the OS reports the switch is complete. Wired headsets
        // and BT-stayed-A2DP devices exit on the first poll.
        if let (Some(d), Some(target)) = (default_output_device(), original_rate) {
            let deadline = Instant::now() + Duration::from_millis(RESUME_RATE_WAIT_TIMEOUT_MS);
            let mut waited_ms = 0u64;
            while Instant::now() < deadline {
                if let Some(rate) = nominal_sample_rate(d) {
                    if (rate - target).abs() < 1.0 {
                        break;
                    }
                }
                thread::sleep(Duration::from_millis(RESUME_RATE_POLL_MS));
                waited_ms += RESUME_RATE_POLL_MS;
            }
            verbose_log!(
                "[media_control.mac] resume_now: waited {waited_ms} ms for sample rate to return to {target}"
            );
        }

        // Re-probe: if the user started something else playing
        // during recording (or tapped the AirPod button to resume
        // music themselves), leave it alone. Only post the toggle
        // if the device is currently idle — i.e. our pause is still
        // the reason audio stopped.
        if is_audio_playing() {
            verbose_log!(
                "[media_control.mac] resume_now: device already running, skip toggle"
            );
            return;
        }
        toggle_play_pause();
        verbose_log!("[media_control.mac] resume_now: posted play/pause key");
    }
}
