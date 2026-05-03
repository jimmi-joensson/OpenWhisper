//! macOS MediaController — pauses currently-playing audio (Spotify,
//! Apple Music, browser tabs, podcasts, anything else producing
//! output) while OpenWhisper is recording, then resumes after the mic
//! closes and Bluetooth has switched back to A2DP/stereo.
//!
//! ## Hybrid implementation: AppleScript for Spotify + Music, media
//! keys for everything else
//!
//! At pause-time we enumerate active output producers via
//! `kAudioHardwarePropertyProcessObjectList`, query each process's
//! `kAudioProcessPropertyBundleID`, and bucket:
//!
//! - **AppleScript track** (`com.spotify.client`, `com.apple.Music`):
//!   per-app `tell application "X" to pause` — deterministic, returns
//!   only the apps that actually transitioned, costs a one-time
//!   Automation TCC prompt per app (Spotify and Music, max two prompts
//!   ever per install) the first time we touch them while they're
//!   playing. Resume replays per-app `tell ... to play`.
//! - **Media-key track** (browser tabs, VLC, Plex, podcast apps,
//!   anything else): synthesize `NX_KEYTYPE_PLAY` via
//!   `+[NSEvent otherEventWithType:…]` → `CGEventPost(kCGHIDEventTap,
//!   …)`. Same pattern BetterTouchTool / Hammerspoon / Caffeine use.
//!   Reuses the existing Accessibility grant — no extra TCC prompt.
//!
//! Pause order: AppleScript first, settle, then media-key burst. AS
//! pause is synchronous so we know which apps it touched; the brief
//! `POST_APPLESCRIPT_SETTLE_MS` lets HAL + `mediaremoted`'s NowPlaying
//! election re-settle before we start posting media keys (otherwise
//! the first toggle can route to a not-yet-de-elected Spotify and
//! waste itself as a no-op).
//!
//! Resume order: media-key replay first, then AppleScript. Reverse
//! of pause for the same election reason — if we resumed Spotify
//! first, the next media-key toggle would route to the now-playing
//! Spotify and pause it again. Doing media-key first targets the
//! browser/VLC/etc. while Spotify is still paused, then AS resumes
//! Spotify cleanly afterwards.
//!
//! ## Why this hybrid (and not pure media-key)
//!
//! macOS routes the play/pause media key to a single elected
//! NowPlaying client at a time. With multiple apps producing output,
//! one media-key toggle hits one app. A burst loop with
//! re-election gaps *should* hit each subsequent app — but in
//! practice `mediaremoted`'s re-election latency exceeds the gap we
//! can afford with imperceptible UX (verified empirically: 40 ms
//! gap → second toggle still routes to the just-paused first app).
//!
//! The deterministic alternatives are gone in macOS 15.4: Apple
//! gated the entire `MediaRemote.framework` (both
//! `MRMediaRemoteSendCommand` SET and `MRMediaRemoteGetNowPlaying*`
//! READ) behind a `com.apple.*` entitlement that no third-party app
//! holds. The only way to use it post-15.4 is the
//! [ungive/mediaremote-adapter](https://github.com/ungive/mediaremote-adapter)
//! Perl-bridge hack — bundling that into a Tauri app is well past
//! "non-hacky" for a v0.5 nicety. BetterTouchTool — the most-shipped
//! app in this niche — landed on exactly the AppleScript-per-app
//! hybrid after the same 15.4 break
//! ([thread](https://community.folivora.ai/t/now-playing-is-no-longer-working-on-macos-15-4/42802)).
//!
//! Trade accepted: two TCC prompts max per install (Spotify, Music),
//! lazy (only fires when those apps are actually playing during a
//! record), in exchange for deterministic multi-app pause + resume
//! covering the realistic case (Spotify + browser tab YouTube etc.).
//!
//! ## Known limitation that remains
//!
//! Multi-app resume is still limited *within the media-key bucket*
//! (3+ media-key targets, e.g. browser tab + VLC + Plex playing
//! simultaneously): only the most-recently-NowPlaying media-key app
//! resumes, others stay paused. Spotify and Music are exempt because
//! they get explicit AppleScript resume. Realistic users hit this
//! near-never; documented for honesty.
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
use std::process::Command;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use core_foundation::base::TCFType;
use core_foundation::string::{CFString, CFStringRef};
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
/// `kAudioProcessPropertyBundleID` — `CFStringRef` bundle identifier
/// for a process AudioObject. We use this to bucket each active
/// producer into the AppleScript track (Spotify, Music) vs. the
/// media-key track (everything else).
const KAUDIO_PROCESS_PROPERTY_BUNDLE_ID: u32 = fourcc(b"pbid");
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
/// Wait between the AppleScript pause/resume call and the media-key
/// burst. AppleScript's `pause`/`play` is synchronous from osascript's
/// view (returns when the target ACKs) but the target's CoreAudio I/O
/// proc + `mediaremoted`'s NowPlaying election still take a beat to
/// settle. Skipping this lets the next media-key toggle route to the
/// not-yet-de-elected AppleScript-paused app and waste itself as a
/// no-op. 150 ms is well below user perception (one fewer than half a
/// frame past 1/8 s) and reliably covers Spotify/Music spin-down on
/// every host we've smoke-tested.
const POST_APPLESCRIPT_SETTLE_MS: u64 = 150;

/// Bundle ID of the Spotify desktop client. Stable for years.
const SPOTIFY_BUNDLE_ID: &str = "com.spotify.client";
/// Bundle ID of the Apple Music app on macOS Catalina+. (Pre-Catalina
/// iTunes used `com.apple.iTunes` but our floor is macOS 14.)
const MUSIC_BUNDLE_ID: &str = "com.apple.Music";

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

/// Per-process bundle-id read. Returns `None` when the property
/// isn't set on the AudioObject (background daemons, processes that
/// haven't published a CFBundleIdentifier).
fn process_bundle_id(pid_obj: u32) -> Option<String> {
    let prop = AudioObjectPropertyAddress {
        selector: KAUDIO_PROCESS_PROPERTY_BUNDLE_ID,
        scope: KAUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        element: KAUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };
    let mut value: *const c_void = std::ptr::null();
    let mut sz: u32 = std::mem::size_of::<*const c_void>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            pid_obj,
            &prop,
            0,
            std::ptr::null(),
            &mut sz,
            &mut value as *mut *const c_void as *mut c_void,
        )
    };
    if status != 0 || value.is_null() {
        return None;
    }
    // The property follows the Create rule — wrap into a CFString
    // that auto-releases on drop.
    let cf_str = unsafe { CFString::wrap_under_create_rule(value as CFStringRef) };
    Some(cf_str.to_string())
}

/// Maps a bundle-id to the AppleScript app name we drive it with,
/// or `None` if the producer should go through the media-key path.
fn applescript_name_for(bundle_id: &str) -> Option<&'static str> {
    match bundle_id {
        SPOTIFY_BUNDLE_ID => Some("Spotify"),
        MUSIC_BUNDLE_ID => Some("Music"),
        _ => None,
    }
}

/// Active output producers, bucketed by which control mechanism we
/// can use against them. Returns `None` only when the
/// `kAudioHardwarePropertyProcessObjectList` enumeration fails
/// (pre-macOS 14 or future API drift) — caller falls back to
/// "media-key burst with `MAX_TOGGLE_ITERATIONS` cap, no AppleScript
/// targets".
fn enumerate_active_producers() -> Option<(Vec<&'static str>, usize)> {
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
        return Some((Vec::new(), 0));
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

    let mut applescript_apps: Vec<&'static str> = Vec::new();
    let mut media_key_count = 0usize;
    for &pid_obj in &ids {
        let prop = AudioObjectPropertyAddress {
            selector: KAUDIO_PROCESS_PROPERTY_IS_RUNNING_OUTPUT,
            scope: KAUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            element: KAUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };
        let mut running: u32 = 0;
        let mut sz: u32 = std::mem::size_of::<u32>() as u32;
        let s = unsafe {
            AudioObjectGetPropertyData(
                pid_obj,
                &prop,
                0,
                std::ptr::null(),
                &mut sz,
                &mut running as *mut u32 as *mut c_void,
            )
        };
        if s != 0 || running == 0 {
            continue;
        }
        match process_bundle_id(pid_obj).as_deref().and_then(applescript_name_for) {
            Some(name) => {
                if !applescript_apps.contains(&name) {
                    applescript_apps.push(name);
                }
            }
            None => media_key_count += 1,
        }
    }
    Some((applescript_apps, media_key_count))
}

/// Run an AppleScript via `osascript`, return trimmed stdout.
/// `None` on any failure — TCC denial, syntax error, missing app.
/// Caller treats `None` as "nothing happened" and recovers by
/// leaving the corresponding state empty.
fn run_osascript(script: &str) -> Option<String> {
    let output = Command::new("osascript").arg("-e").arg(script).output().ok()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        verbose_log!(
            "[media_control.mac] osascript failed (status={:?}): {}",
            output.status,
            stderr.trim()
        );
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Build a script that pauses each AppleScript-track app currently
/// playing and emits a comma-separated list of the apps it actually
/// touched. Each `tell` is wrapped in `try` so a missing-app or
/// per-app TCC denial doesn't break the rest. Both `Spotify` and
/// `Music` expose `player state` on macOS 14+ — Podcasts/TV don't,
/// which is why they're not in the AppleScript track at all (a
/// reference to one inside a `tell` would fail compile, not runtime,
/// and `try` doesn't catch compile errors).
fn build_pause_script(apps: &[&str]) -> String {
    let mut s = String::from("set output to \"\"\n");
    for app in apps {
        s.push_str(&format!(
            "try
    tell application \"{app}\"
        if it is running then
            if player state is playing then
                pause
                set output to output & \"{app},\"
            end if
        end if
    end tell
end try
"
        ));
    }
    s.push_str("return output");
    s
}

fn build_play_script(apps: &[String]) -> String {
    let mut s = String::new();
    for app in apps {
        s.push_str(&format!(
            "try
    tell application \"{app}\" to if it is running then play
end try
"
        ));
    }
    s
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
    /// AppleScript-track apps that `pause_now` actually transitioned
    /// (their `player state` was `playing` and the script reported
    /// back). `resume_now` plays back exactly these via per-app
    /// AppleScript — never a generic media-key on these, since that
    /// would race the same NowPlaying election we're trying to avoid.
    paused_via_applescript: Vec<String>,
    /// Number of media-key toggles `pause_now` posted to silence the
    /// non-AppleScript producers. `resume_now` replays up to this
    /// many with the same `is_audio_playing` re-probe early-exit.
    /// Subject to the documented "only the most-recently-NowPlaying
    /// media-key app actually resumes" limitation.
    toggles_sent: usize,
    /// Default-output device's nominal sample rate at pause-time.
    /// `resume_now` polls until the rate climbs back to this value
    /// (BT profile switchback signal) before any play key / script.
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

        let (ascript_targets, media_count) = match enumerate_active_producers() {
            Some(buckets) => buckets,
            None => {
                verbose_log!(
                    "[media_control.mac] pause_now: enumeration unavailable, falling back to media-key burst with cap"
                );
                (Vec::new(), if is_audio_playing() { MAX_TOGGLE_ITERATIONS } else { 0 })
            }
        };

        if ascript_targets.is_empty() && media_count == 0 {
            verbose_log!("[media_control.mac] pause_now: nothing playing, skip");
            return false;
        }

        // AppleScript track first: deterministic, returns exactly which
        // apps actually transitioned (player state was `playing` and
        // the `tell ... pause` ACK'd).
        if !ascript_targets.is_empty() {
            let script = build_pause_script(&ascript_targets);
            let actually_paused: Vec<String> = run_osascript(&script)
                .map(|out| {
                    out.split(',')
                        .filter(|p| !p.is_empty())
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            state.paused_via_applescript = actually_paused;

            // Settle: AppleScript pause ACKs as soon as the app
            // received the command, but the app's CoreAudio I/O proc
            // takes a beat to actually stop and `mediaremoted` takes
            // another beat to re-elect the next NowPlaying claimant.
            // Skipping this lets the first media-key toggle below
            // route to the not-yet-de-elected Spotify/Music and
            // waste itself.
            if !state.paused_via_applescript.is_empty() && media_count > 0 {
                thread::sleep(Duration::from_millis(POST_APPLESCRIPT_SETTLE_MS));
            }
        }

        // Media-key track: burst loop sized to the count of
        // non-AppleScript producers. `is_audio_playing` is the safety
        // gate (skip the burst if nothing's left to silence after the
        // AppleScript pass) but is NOT used for early-exit inside the
        // loop — we trust the count, since the loop is bounded.
        if media_count > 0 && is_audio_playing() {
            let target = media_count.min(MAX_TOGGLE_ITERATIONS);
            let mut sent = 0usize;
            while sent < target {
                toggle_play_pause();
                sent += 1;
                if sent < target {
                    thread::sleep(Duration::from_millis(INTER_TOGGLE_SLEEP_MS));
                }
            }
            state.toggles_sent = sent;
        }

        if state.paused_via_applescript.is_empty() && state.toggles_sent == 0 {
            return false;
        }

        if let Some(d) = default_output_device() {
            state.original_sample_rate = nominal_sample_rate(d);
        }
        verbose_log!(
            "[media_control.mac] pause_now: applescript={:?} media_key={}/{} (targets {:?}), original_sample_rate={:?}",
            state.paused_via_applescript,
            state.toggles_sent,
            media_count,
            ascript_targets,
            state.original_sample_rate
        );
        true
    }

    fn resume_now(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let ascript_apps = std::mem::take(&mut state.paused_via_applescript);
        let toggles = state.toggles_sent;
        state.toggles_sent = 0;
        let original_rate = state.original_sample_rate.take();
        drop(state);

        if ascript_apps.is_empty() && toggles == 0 {
            return;
        }

        // Wait for BT to switch back to its pre-recording profile
        // (HFP→A2DP on AirPods, ~500–1000 ms). Signal: device's
        // nominal sample rate climbs back to the pre-pause value —
        // adaptive, exits as soon as the OS reports the switch.
        // Wired headsets / BT-stayed-A2DP devices exit on first poll.
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

        // Media-key replay FIRST. If we resumed AppleScript apps
        // first, they'd take NowPlaying immediately — and the next
        // media-key toggle would route to them and pause them again.
        // Doing media-key first targets the still-paused browser/VLC/
        // etc. cleanly. Re-probe is the early-exit guard against the
        // user starting something else mid-record.
        let mut replayed = 0usize;
        while replayed < toggles {
            if is_audio_playing() {
                verbose_log!(
                    "[media_control.mac] resume_now: device running, stop media-key replay at {replayed}/{toggles}"
                );
                break;
            }
            toggle_play_pause();
            replayed += 1;
            if replayed < toggles {
                thread::sleep(Duration::from_millis(INTER_TOGGLE_SLEEP_MS));
            }
        }

        // AppleScript replay. Per-app `tell ... to play` is
        // deterministic — runs against exactly the apps `pause_now`
        // recorded as actually-paused, never resumes a Spotify the
        // user paused externally before recording.
        if !ascript_apps.is_empty() {
            let script = build_play_script(&ascript_apps);
            let _ = run_osascript(&script);
            verbose_log!(
                "[media_control.mac] resume_now: applescript played {ascript_apps:?}"
            );
        }
        verbose_log!(
            "[media_control.mac] resume_now: replayed {replayed}/{toggles} media-key toggles"
        );
    }
}
