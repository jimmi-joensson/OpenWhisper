//! macOS MediaController — pauses currently-playing audio in Spotify,
//! Apple Music, Podcasts, and TV via AppleScript while OpenWhisper is
//! recording, then resumes after the mic closes and Bluetooth has
//! switched back to A2DP/stereo.
//!
//! Why AppleScript and not MediaRemote (`MRMediaRemoteSendCommand`):
//! - `kMRPause` (opcode 1) is fire-and-forget; we can't tell if the
//!   command actually paused anything. Sending the matching kMRPlay
//!   on stop will resume whatever was the most-recent now-playing app
//!   — including apps the user paused externally before recording —
//!   which produces the "music starts when I stop a recording with
//!   nothing playing" regression.
//! - On macOS 15.4+, `mediaremoted` enforces a `com.apple.*`
//!   entitlement check that further degrades MediaRemote reliability
//!   from non-Apple-signed processes.
//!
//! AppleScript per-app `if player state is playing then pause` is
//! synchronous, returns a deterministic "did this app actually pause"
//! signal, and is naturally pause-only (sending `pause` to a paused
//! or stopped app is a no-op). State (which apps we paused) lives in
//! a `Mutex<State>`; on `resume_now` we play only those apps back.
//!
//! Known v1 limitation: browser-tab media (Safari/Chrome/etc.) is not
//! paused. Browser tabs expose no AppleScript pause command for
//! per-tab media; the MediaRemote path would handle them but
//! reintroduces the bug above. Tracked for future work.
//!
//! Resume timing: Bluetooth headphones (AirPods etc.) switch from
//! A2DP/stereo to HFP/mono the moment the mic opens. Sending `play`
//! before BT has switched back means music briefly resumes in mono.
//! We capture the default-output device's nominal sample rate at
//! pause-time, then on stop poll until the rate has returned to that
//! value (with a 2 s cap) before sending `play`.

use std::ffi::{c_char, c_void};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use objc2::runtime::Bool;
use openwhisper_core::media_gate::{MediaController, PauseDiagnostic};
use openwhisper_core::verbose_log;

/// MediaRemote command codes per `Cykey/ios-reversed-headers`.
/// `kMRPause = 1` is true pause-only (does NOT toggle, does NOT
/// launch a default music app when nothing is playing). We use it as
/// a best-effort fallback for media that AppleScript can't reach —
/// browser tabs primarily. We deliberately do NOT send the matching
/// `kMRPlay` on resume: that would resume externally-paused apps and
/// reintroduce the "stop with nothing playing → music starts"
/// regression we hit earlier. Net effect: a browser tab that this
/// path pauses stays paused; user manually clicks play in the tab.
const KMR_PAUSE: u32 = 1;

const fn fourcc(s: &[u8; 4]) -> u32 {
    ((s[0] as u32) << 24) | ((s[1] as u32) << 16) | ((s[2] as u32) << 8) | (s[3] as u32)
}
const KAUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE: u32 = fourcc(b"dOut");
const KAUDIO_DEVICE_PROPERTY_NOMINAL_SAMPLE_RATE: u32 = fourcc(b"nsrt");
const KAUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: u32 = fourcc(b"glob");
const KAUDIO_OBJECT_SYSTEM_OBJECT: u32 = 1;
const KAUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: u32 = 0;
const RESUME_RATE_WAIT_TIMEOUT_MS: u64 = 2000;
const RESUME_RATE_POLL_MS: u64 = 50;

/// Apps we know how to drive via AppleScript. Restricted to apps that
/// expose the `player state` property in their AppleScript
/// dictionary on macOS 15.x — Apple's Podcasts and TV apps don't, so
/// referring to them inside `tell application "X"` makes the WHOLE
/// script fail to compile (compile errors aren't caught by `try`,
/// only runtime errors). Order is irrelevant.
const PAUSE_TARGETS: &[&str] = &["Spotify", "Music"];

#[repr(C)]
#[derive(Clone, Copy)]
struct AudioObjectPropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
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

extern "C" {
    fn dlopen(filename: *const c_char, flag: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}
const RTLD_LAZY: i32 = 0x1;

type MRSendCommandFn = unsafe extern "C" fn(u32, *const c_void) -> Bool;

struct MediaRemoteFns {
    send_command: MRSendCommandFn,
}
unsafe impl Send for MediaRemoteFns {}
unsafe impl Sync for MediaRemoteFns {}

fn load_media_remote() -> Option<&'static MediaRemoteFns> {
    static MR: OnceLock<Option<MediaRemoteFns>> = OnceLock::new();
    MR.get_or_init(|| unsafe {
        let path = c"/System/Library/PrivateFrameworks/MediaRemote.framework/MediaRemote";
        let handle = dlopen(path.as_ptr(), RTLD_LAZY);
        if handle.is_null() {
            verbose_log!("[media_control.mac] dlopen MediaRemote failed");
            return None;
        }
        let send = dlsym(handle, c"MRMediaRemoteSendCommand".as_ptr());
        if send.is_null() {
            verbose_log!("[media_control.mac] dlsym MRMediaRemoteSendCommand failed");
            return None;
        }
        Some(MediaRemoteFns {
            send_command: std::mem::transmute::<*mut c_void, MRSendCommandFn>(send),
        })
    })
    .as_ref()
}

fn mr_send(cmd: u32) {
    if let Some(fns) = load_media_remote() {
        let _: Bool = unsafe { (fns.send_command)(cmd, std::ptr::null()) };
    }
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

/// Runs an AppleScript via `osascript` and returns trimmed stdout.
/// Returns `None` on any failure (TCC denial, syntax error, missing
/// app); the caller treats `None` as "nothing happened."
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

/// Build an AppleScript that visits each known music app, pauses if
/// it's currently playing, and returns two `||`-separated halves:
///   `paused_apps || error_codes`
/// where `paused_apps` is a comma-list of apps we successfully paused,
/// and `error_codes` is a comma-list of `<App>:<errNum>` entries for
/// apps that raised an AppleEvent error (TCC denial = -1743, app
/// missing = -600 / -1728, etc.). Per-app `try ... on error` so one
/// app's failure doesn't abort the rest, AND we now actually capture
/// the failure code instead of silently swallowing it — this is what
/// lets `pause_now` surface a TCC-denied diagnostic to the UI.
fn build_pause_script() -> String {
    let mut s = String::from("set pausedOut to \"\"\nset errOut to \"\"\n");
    for app in PAUSE_TARGETS {
        s.push_str(&format!(
            "try
    tell application \"{app}\"
        if it is running then
            if player state is playing then
                pause
                set pausedOut to pausedOut & \"{app},\"
            end if
        end if
    end tell
on error errMsg number errNum
    set errOut to errOut & \"{app}:\" & errNum & \",\"
end try
"
        ));
    }
    s.push_str("return pausedOut & \"||\" & errOut");
    s
}

/// Side-effect-free Automation TCC probe. Mirrors the `pause_now`
/// dispatch (per-target `try ... on error errNum`) but reads
/// `player state` instead of calling `pause`, so granted users see no
/// behavior change. Used on app focus regain to clear the
/// NotAuthorized banner the moment the user grants in System Settings,
/// without waiting for the next recording.
///
/// Limitation: AppleScript only dispatches AppleEvents to apps that
/// are currently running, so if neither Spotify nor Music is running
/// the probe yields no errors and we conservatively assume "ok" — any
/// stale NotAuthorized state will be re-confirmed at the next real
/// pause attempt. Acceptable: the Settings-flip flow keeps Spotify
/// running while the user toggles, so the typical recovery path does
/// produce a real signal.
fn build_probe_script() -> String {
    let mut s = String::from("set errOut to \"\"\n");
    for app in PAUSE_TARGETS {
        s.push_str(&format!(
            "try
    tell application \"{app}\"
        if it is running then
            get player state
        end if
    end tell
on error errMsg number errNum
    set errOut to errOut & \"{app}:\" & errNum & \",\"
end try
"
        ));
    }
    s.push_str("return errOut");
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

#[derive(Default)]
struct State {
    /// Apps we paused via AppleScript. `resume_now` plays back exactly
    /// these and no others — never sends a generic kMRPlay that could
    /// resume an externally-paused app.
    paused_apps: Vec<String>,
    /// Default-output device's nominal sample rate at pause-time.
    /// `resume_now` polls until the rate climbs back to this value
    /// (BT profile switchback signal) before sending `play`.
    original_sample_rate: Option<f64>,
}

/// Process-wide diagnostic populated on every `pause_now`. `None` after
/// a successful pause; `Some(reason)` when AppleScript paused nothing
/// AND something appears to have prevented it (most commonly: user has
/// not granted Automation permission for Spotify / Music in System
/// Settings → Privacy & Security → Automation → OpenWhisper). Read via
/// the `MediaController::last_pause_diagnostic` trait method so the UI
/// can render an actionable banner — silent failure was the bug we hit
/// when users on built-in speakers (no Bluetooth profile flip to mask
/// it) wondered why music kept playing during a recording.
static LAST_PAUSE_DIAGNOSTIC: Mutex<Option<PauseDiagnostic>> = Mutex::new(None);

/// Stable machine tags the UI switches on. Match
/// `openwhisper_core::media_gate::PauseDiagnostic::reason` documented
/// values:
const REASON_NOT_AUTHORIZED: &str = "not_authorized";
const REASON_NO_KNOWN_PLAYER: &str = "no_known_player";
const REASON_OTHER: &str = "other";

fn set_last_diagnostic(value: Option<PauseDiagnostic>) {
    if let Ok(mut g) = LAST_PAUSE_DIAGNOSTIC.lock() {
        *g = value;
    }
}

fn read_last_diagnostic() -> Option<PauseDiagnostic> {
    LAST_PAUSE_DIAGNOSTIC.lock().ok().and_then(|g| g.clone())
}

/// Re-probe Automation TCC without taking any pause/resume action.
/// Runs the side-effect-free probe script and updates
/// `LAST_PAUSE_DIAGNOSTIC` based on the per-target error codes:
///
///   - any `-1743`     → `not_authorized` (banner stays / re-appears)
///   - no errors       → `None` (banner clears — assumes grant or no
///                       running target; either way there's no negative
///                       signal to act on)
///   - other errors    → keep prior diagnostic if it was
///                       `not_authorized` (don't lose a real denial
///                       signal because of an unrelated AppleScript
///                       hiccup)
fn probe_authorization_inner() -> Option<PauseDiagnostic> {
    let raw = run_osascript(&build_probe_script()).unwrap_or_default();
    let errors: Vec<(String, i32)> = raw
        .split(',')
        .filter(|p| !p.is_empty())
        .filter_map(|entry| {
            let (app, code) = entry.split_once(':')?;
            code.trim().parse::<i32>().ok().map(|n| (app.to_string(), n))
        })
        .collect();

    if errors.iter().any(|(_, n)| *n == -1743) {
        let detail = errors
            .iter()
            .map(|(a, n)| format!("{a}:{n}"))
            .collect::<Vec<_>>()
            .join(",");
        let next = PauseDiagnostic::new(REASON_NOT_AUTHORIZED, detail);
        set_last_diagnostic(Some(next.clone()));
        return Some(next);
    }

    if errors.is_empty() {
        // No negative signal. Clear `not_authorized` state so the
        // banner dismisses; preserve prior `no_known_player` /
        // `other` reasons since those aren't refuted by a clean probe.
        if let Ok(mut g) = LAST_PAUSE_DIAGNOSTIC.lock() {
            if matches!(
                g.as_ref().map(|d| d.reason),
                Some(REASON_NOT_AUTHORIZED)
            ) {
                *g = None;
            }
        }
    }
    read_last_diagnostic()
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

        // Script returns `paused_apps||error_codes` (see
        // build_pause_script). Split, parse both halves so we can tell
        // "paused nothing because nothing was playing" apart from
        // "paused nothing because user has not granted Automation".
        let raw = run_osascript(&build_pause_script()).unwrap_or_default();
        let (paused_part, errors_part) = raw.split_once("||").unwrap_or((raw.as_str(), ""));
        let paused: Vec<String> = paused_part
            .split(',')
            .filter(|p| !p.is_empty())
            .map(String::from)
            .collect();
        let errors: Vec<(String, i32)> = errors_part
            .split(',')
            .filter(|p| !p.is_empty())
            .filter_map(|entry| {
                let (app, code) = entry.split_once(':')?;
                code.trim().parse::<i32>().ok().map(|n| (app.to_string(), n))
            })
            .collect();

        // Best-effort: send `kMRPause` (opcode 1, true pause-only) so
        // browser-tab media gets paused when it's the elected
        // Now Playing client. No matching `kMRPlay` on resume — that
        // would resume externally-paused apps. Trade-off: a paused
        // browser tab stays paused, user manually clicks play.
        mr_send(KMR_PAUSE);

        if paused.is_empty() {
            // Classify why we paused nothing so the UI can act on it.
            // -1743 is the AppleEvent canonical "Not authorized" code;
            // surface it loudly because it's the high-leverage bug
            // (silent fail when user denies Automation prompt). All
            // -1743 paths bubble to `not_authorized` even if other apps
            // had different errors — granting Automation is the fix
            // and we don't want to dilute the message.
            let diagnostic = if errors.iter().any(|(_, n)| *n == -1743) {
                let detail = errors
                    .iter()
                    .map(|(a, n)| format!("{a}:{n}"))
                    .collect::<Vec<_>>()
                    .join(",");
                eprintln!(
                    "[media_control.mac] pause_now: Automation permission denied for one or more music apps ({detail}). Grant in System Settings → Privacy & Security → Automation → OpenWhisper."
                );
                Some(PauseDiagnostic::new(REASON_NOT_AUTHORIZED, detail))
            } else if errors.is_empty() {
                verbose_log!(
                    "[media_control.mac] pause_now: no known player was playing; sent kMRPause best-effort"
                );
                Some(PauseDiagnostic::new(REASON_NO_KNOWN_PLAYER, String::new()))
            } else {
                let detail = errors
                    .iter()
                    .map(|(a, n)| format!("{a}:{n}"))
                    .collect::<Vec<_>>()
                    .join(",");
                eprintln!(
                    "[media_control.mac] pause_now: AppleScript errors: {detail}"
                );
                Some(PauseDiagnostic::new(REASON_OTHER, detail))
            };
            set_last_diagnostic(diagnostic);
            return false;
        }

        if let Some(d) = default_output_device() {
            state.original_sample_rate = nominal_sample_rate(d);
        }
        state.paused_apps = paused;
        set_last_diagnostic(None);
        verbose_log!(
            "[media_control.mac] pause_now: paused {:?}, original_sample_rate={:?}",
            state.paused_apps,
            state.original_sample_rate
        );
        true
    }

    fn resume_now(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let original_rate = state.original_sample_rate.take();
        let apps = std::mem::take(&mut state.paused_apps);
        drop(state);

        if apps.is_empty() {
            return;
        }

        // Wait for BT to switch back to its pre-recording profile
        // (HFP→A2DP on AirPods, ~500–1000 ms) before sending play.
        // Signal: device's nominal sample rate climbs back to the
        // pre-pause value — adaptive detection, exits as soon as the
        // OS reports the switch is complete. Wired headsets and
        // BT-stayed-A2DP devices exit on the first poll. Hardcoded
        // 2 s timeout is a generous fallback for unusual BT devices
        // that never report rate change; the
        // `behavior::bt_resume_delay_ms` setting is Windows-only
        // (Windows can't observe the switch event, see
        // `media_control/windows.rs`).
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

        let script = build_play_script(&apps);
        let _ = run_osascript(&script);
        verbose_log!("[media_control.mac] resume_now: played {apps:?}");
    }

    fn last_pause_diagnostic(&self) -> Option<PauseDiagnostic> {
        read_last_diagnostic()
    }

    fn probe_authorization(&self) -> Option<PauseDiagnostic> {
        probe_authorization_inner()
    }
}
