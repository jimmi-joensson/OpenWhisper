//! Persistent user settings — schema + JSON IO + lock-free caches.
//!
//! Stored as a single JSON file. The shell resolves the on-disk path
//! (Tauri's `app.path().app_config_dir()`, SwiftUI's `~/Library/.../`)
//! and passes it as `&Path` to every `load_*` / `save_*` call —
//! `core::settings` doesn't know about platform-specific directory
//! conventions.
//!
//! Five schema blocks live in one envelope:
//!
//! - `hotkeys` — toggle + cancel slots, each `ModifierTap` (Mac
//!   default Right ⌘) or `Chord` (Win default Ctrl+Space). One
//!   optional slot, `cycle_mode`, switches to the next mode
//!   (TASK-96); it is unbound until the user sets it. Another, `ask`,
//!   starts an ask instead of a dictation (TASK-97): it has a default
//!   key, can be cleared, and is only installed while Ask is on.
//! - `audio` — selected input device id (cpal `DeviceId` Display
//!   form). `None` = use host default.
//! - `behavior` — `show_in_fullscreen`,
//!   `pause_audio_during_dictation`, `bt_resume_delay_ms`. Hot-path
//!   readers (fullscreen detector, dictation phase observer,
//!   MediaController) hit the lock-free `AtomicBool` / `AtomicU64`
//!   caches via [`show_in_fullscreen`] / [`pause_audio_during_dictation`]
//!   / [`bt_resume_delay_ms`].
//! - `pill` — `follow_active_screen` (pill HUD jumps to the focused
//!   app's monitor on every foreground change). Cached as
//!   [`AtomicBool`] for the 500 ms watcher loop.
//! - `stats` — `user_wpm` calibration for the Time Saved formula on
//!   the Home pane.
//! - `performance` — `keep_models_warm` (when true, registered
//!   `ModelHandle`s never auto-release their resident model). Cached
//!   as [`AtomicBool`] for the lock-free read inside `apply_keep_warm`.
//! - `refine` — the optional cloud cleanup step after transcription
//!   ([`RefineSettings`], TASK-93). Kept as raw JSON in the envelope
//!   so a malformed block falls back to defaults without taking the
//!   sibling blocks down with it. The API key is not stored here.
//! - `history`: whether dictated text is kept on disk, for how long
//!   and in which folder ([`HistorySettings`], TASK-95). Off by
//!   default. The text itself lives in Markdown files, see
//!   [`crate::history`]. Raw JSON in
//!   the envelope for the same reason as `refine`.
//! - `modes`: the user's own modes for the Refine step, which one is
//!   selected and which apps have a default mode ([`ModesSettings`],
//!   TASK-96). The built-in Dictation mode is not stored. Raw JSON in
//!   the envelope for the same reason as `refine`.
//! - `ask`: the switch, web search, standing instructions, model,
//!   effort and timeout of Ask ([`AskSettings`], TASK-97). Off by
//!   default. The backend is the one in `refine`. Raw JSON in the
//!   envelope for the same reason as `refine`.
//! - `vision`: when an ask includes a screenshot, the phrases that
//!   ask for one and the apps that are never captured
//!   ([`VisionSettings`], TASK-98). Raw JSON in the envelope for the
//!   same reason as `refine`; the block itself is read field by field
//!   and settles every doubt on the careful side.
//!
//! Migration: the legacy `hotkey` (single-slot) field is read on first
//!  load after upgrade and folded into the current `hotkeys` shape;
//! we always write the new shape on save.
//!
//! Writers: every `save_*` fn is a read-modify-write of the whole
//! file (read it, replace one block, write it back), and they are
//! called from more than one thread: the main thread for the settings
//! panes, the hotkey thread and the tray thread for a mode switch.
//! One process-wide lock ([`SAVE_LOCK`]) makes each of them one step,
//! so a save of one block cannot write back a stale copy of another.
//! The file itself is replaced by rename ([`write_file`]), so a reader
//! in this or another process (the headless CLI) sees the old file or
//! the new one, never a half-written one.
//!
//! Cache hydration order (typical):
//!   1. Shell resolves config-dir path at boot.
//!   2. Shell calls [`load_settings`] / [`load_audio_settings`] /
//!      [`load_behavior_settings`] / [`load_stats_settings`]; each
//!      reads the file, populates the matching `Mutex<Option<T>>`
//!      cache, and (for `behavior` / `pill`) writes through to the
//!      lock-free atomic mirrors.
//!   3. Shell calls `set_*_cache` setters with the same values to
//!      seed the atomic mirrors before any hot-path reader fires.
//!      (Some `load_*` fns already do this — `set_*_cache` is the
//!      explicit entry point for shells that want to bypass disk IO.)

use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

mod ask;
pub use ask::{
    ASK_INSTRUCTIONS_MAX_CHARS, ASK_TIMEOUT_DEFAULT_MS, ASK_TIMEOUT_MAX_MS, ASK_TIMEOUT_MIN_MS, AskSettings,
};

mod history;
pub use history::{HISTORY_RETENTION_DEFAULT_DAYS, HISTORY_RETENTION_MAX_DAYS, HistorySettings};

mod modes;
pub use modes::{
    AppDefault, BUILTIN_MODE_ID, BUILTIN_MODE_NAME, MODE_INSTRUCTIONS_MAX_CHARS, MODE_NAME_MAX_CHARS,
    MODES_MAX, Mode, ModesSettings, app_key, mode_name_key, mode_slug,
};

mod vision;
pub use vision::{
    NeverCaptureApp, VISION_APP_FIELD_MAX_CHARS, VISION_NEVER_CAPTURE_MAX, VISION_PHRASE_MAX_CHARS,
    VISION_PHRASES_MAX, VisionSettings, VisionTrigger, default_never_capture, default_phrases,
};

mod refine;
pub use refine::{
    ClaudeCliSettings, DEFAULT_CLAUDE_MODEL, OpenAiSettings, REFINE_MIN_WORDS_MAX, REFINE_TIMEOUT_MAX_MS,
    REFINE_TIMEOUT_MIN_MS, RefineBackendKind, RefineEffort, RefineSettings,
};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HotkeyKind {
    /// Single modifier key, fired on release if no other key was pressed
    /// while it was held. Mac-only today.
    ModifierTap,
    /// Modifier(s) + non-modifier key, fired on the non-modifier KeyDown.
    Chord,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HotkeyConfig {
    pub kind: HotkeyKind,
    pub code: String,
    #[serde(default)]
    pub mods: Vec<String>,
}

impl HotkeyConfig {
    /// Mac-only constructor for the `RightCommand` default. The Windows
    /// hotkey backend doesn't accept `ModifierTap`, so this is dead on
    /// Windows builds — `#[allow(dead_code)]` rather than cfg-gating
    /// the call sites.
    #[allow(dead_code)]
    pub fn modifier_tap(code: &str) -> Self {
        Self {
            kind: HotkeyKind::ModifierTap,
            code: code.to_string(),
            mods: Vec::new(),
        }
    }

    pub fn chord(code: &str, mods: &[&str]) -> Self {
        Self {
            kind: HotkeyKind::Chord,
            code: code.to_string(),
            mods: mods.iter().map(|s| s.to_string()).collect(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HotkeySettings {
    pub toggle: HotkeyConfig,
    pub cancel: HotkeyConfig,
    /// Switch to the next mode (TASK-96). Unbound by default: with
    /// only the built-in mode there is nothing to cycle, and a default
    /// chord would take a key combination away from other apps for
    /// nothing. Left out of the file while unbound, so a file written
    /// by this version still reads in an older one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle_mode: Option<HotkeyConfig>,
    /// Start an ask instead of a dictation (TASK-97). Unlike
    /// `cycle_mode` this slot has a default key
    /// ([`default_ask_hotkey`]), so a file written before the slot
    /// existed reads with that key, and `None` means the user cleared
    /// it: it is written as an explicit `null`, which is what tells
    /// "cleared" from "never set". The shell installs the key only
    /// while Ask is switched on ([`AskSettings::enabled`]).
    #[serde(default = "default_ask_slot")]
    pub ask: Option<HotkeyConfig>,
}

fn default_ask_slot() -> Option<HotkeyConfig> {
    Some(default_ask_hotkey())
}

/// Slot identifier — used by the shell's `settings_set_hotkey` Tauri
/// command and the capture flow to disambiguate which binding the
/// user is editing.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HotkeyTarget {
    Toggle,
    Cancel,
    /// The optional "switch to the next mode" slot.
    CycleMode,
    /// The "ask instead of dictate" slot. Has a default key and can
    /// be cleared.
    Ask,
}

impl HotkeyTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            HotkeyTarget::Toggle => "toggle",
            HotkeyTarget::Cancel => "cancel",
            HotkeyTarget::CycleMode => "cycle-mode",
            HotkeyTarget::Ask => "ask",
        }
    }

    /// The slot may be left unbound (see [`clear_hotkey_slot`]).
    /// Dictation needs its toggle and cancel keys; they can be
    /// changed, never removed.
    pub fn is_optional(self) -> bool {
        matches!(self, HotkeyTarget::CycleMode | HotkeyTarget::Ask)
    }
}

pub fn default_toggle_hotkey() -> HotkeyConfig {
    #[cfg(target_os = "macos")]
    {
        HotkeyConfig::modifier_tap("RightCommand")
    }
    #[cfg(not(target_os = "macos"))]
    {
        HotkeyConfig::chord("Space", &["Ctrl"])
    }
}

pub fn default_cancel_hotkey() -> HotkeyConfig {
    HotkeyConfig::chord("Escape", &[])
}

/// The key that starts an ask until the user picks another: a tap on
/// Right Option on macOS (the dictation key is a tap on Right Command,
/// one key over), Ctrl+Shift+Space elsewhere (dictation is
/// Ctrl+Space; chords match exactly, so the two do not collide).
pub fn default_ask_hotkey() -> HotkeyConfig {
    #[cfg(target_os = "macos")]
    {
        HotkeyConfig::modifier_tap("RightOption")
    }
    #[cfg(not(target_os = "macos"))]
    {
        HotkeyConfig::chord("Space", &["Ctrl", "Shift"])
    }
}

pub fn default_settings() -> HotkeySettings {
    HotkeySettings {
        toggle: default_toggle_hotkey(),
        cancel: default_cancel_hotkey(),
        cycle_mode: None,
        ask: Some(default_ask_hotkey()),
    }
}

/// On-disk envelope. `hotkey` is the legacy single-slot field — kept
/// for migration on first load after upgrading. `hotkeys` is the
/// current shape; we always write that. Sibling blocks (`audio`,
/// `behavior`, `pill`, `stats`) are absent on first run and on
/// upgrades from earlier schemas.
#[derive(Serialize, Deserialize, Default)]
struct SettingsFile {
    #[serde(default)]
    hotkey: Option<HotkeyConfig>,
    #[serde(default)]
    hotkeys: Option<HotkeySettings>,
    #[serde(default)]
    audio: Option<AudioSettings>,
    #[serde(default)]
    behavior: Option<BehaviorSettings>,
    #[serde(default)]
    pill: Option<PillSettings>,
    #[serde(default)]
    stats: Option<StatsSettings>,
    #[serde(default)]
    performance: Option<PerformanceSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refine: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    history: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    modes: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ask: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vision: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatsSettings {
    /// User's typing speed in words per minute. Default 40 = average
    /// adult typist baseline. Clamped to [`USER_WPM_MIN`,
    /// `USER_WPM_MAX`] on save — out-of-range writes are silently
    /// coerced (typing speed is a personal calibration, not a
    /// security boundary; clamp + helper text is friendlier than red
    /// error states).
    #[serde(default = "default_user_wpm")]
    pub user_wpm: u32,
}

pub const USER_WPM_MIN: u32 = 10;
pub const USER_WPM_MAX: u32 = 300;

pub fn default_user_wpm() -> u32 {
    40
}

impl Default for StatsSettings {
    fn default() -> Self {
        Self {
            user_wpm: default_user_wpm(),
        }
    }
}

impl StatsSettings {
    fn clamped(self) -> Self {
        Self {
            user_wpm: self.user_wpm.clamp(USER_WPM_MIN, USER_WPM_MAX),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PillSettings {
    /// Pill HUD jumps to the monitor hosting the focused app on every
    /// foreground change. ON by default — opt-out toggle, not opt-in.
    pub follow_active_screen: bool,
}

impl Default for PillSettings {
    fn default() -> Self {
        Self {
            follow_active_screen: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioSettings {
    /// Stable cpal `DeviceId` (Display form: `"host:device_id"`).
    /// `None` means "use the host default". Persisted as the cpal id
    /// rather than the device's friendly name so a saved selection
    /// rebinds across driver reinstalls and OS-level renames; we
    /// still fall back to the host default at `begin_capture` time
    /// if no enumerable device matches the saved id (mic unplugged,
    /// USB hub down, etc.).
    #[serde(default)]
    pub device_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BehaviorSettings {
    /// When true, OpenWhisper stays active over fullscreen apps —
    /// pill remains visible (best-effort), hotkey stays armed.
    /// Default false preserves the historical "step aside for games
    /// and video" behavior. Read by the fullscreen detector callback
    /// via the lock-free [`show_in_fullscreen`] cache.
    #[serde(default)]
    pub show_in_fullscreen: bool,
    /// When true, OpenWhisper pauses other apps' audio playback on
    /// PHASE_RECORDING entry and resumes on exit. Default true: most
    /// users want auto-pause; the rare user dictating intentionally
    /// over background audio toggles off once. Read via the
    /// lock-free [`pause_audio_during_dictation`] cache.
    #[serde(default = "default_pause_audio_during_dictation")]
    pub pause_audio_during_dictation: bool,
    /// Bluetooth resume delay in milliseconds. After a recording (or
    /// mic-test preview) ends, OpenWhisper waits this long before
    /// sending the SMTC `TryPlayAsync` to resume music — masks the
    /// HFP→A2DP profile-switchback window during which BT would
    /// otherwise replay music in mono. Only applies when the default
    /// render endpoint is a Bluetooth device (gated via
    /// `PKEY_Device_EnumeratorName`); wired/USB endpoints skip the
    /// wait entirely regardless of this value. See the BT mono blip
    /// gotcha in the `openwhisper-platform-gotchas` skill for the
    /// full "why blind sleep, not deterministic detection" trail.
    /// Default 5000 — empirically tuned for AirPods Pro on Win11
    /// 26200; users on faster radios can dial down.
    #[serde(default = "default_bt_resume_delay_ms")]
    pub bt_resume_delay_ms: u64,
}

fn default_pause_audio_during_dictation() -> bool {
    true
}

pub fn default_bt_resume_delay_ms() -> u64 {
    5000
}

/// Hard upper bound for `bt_resume_delay_ms`. The setter clamps to
/// this so a malformed `settings.json` doesn't ship a 60-second wait
/// to the MediaController.
pub const BT_RESUME_DELAY_MS_MAX: u64 = 10_000;

impl Default for BehaviorSettings {
    fn default() -> Self {
        Self {
            show_in_fullscreen: false,
            pause_audio_during_dictation: default_pause_audio_during_dictation(),
            bt_resume_delay_ms: default_bt_resume_delay_ms(),
        }
    }
}

/// Power-user knob: keep model handles resident between sessions.
///
/// `false` (default) → every registered `ModelHandle` honors its
/// configured idle timeout (5 min recognizer / 60 s LLM, etc.) and
/// auto-unloads after that window of `use_with` inactivity. The next
/// dictation pays the cold-load latency.
///
/// `true` → idle timers are effectively disabled (`Duration::MAX`);
/// loaded models stay in RAM until app quit. Trades RAM for
/// first-use latency. Read by `apply_keep_warm` via the lock-free
/// [`keep_models_warm`] cache.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PerformanceSettings {
    #[serde(default)]
    pub keep_models_warm: bool,
}

impl Default for PerformanceSettings {
    fn default() -> Self {
        Self {
            keep_models_warm: false,
        }
    }
}

// --- caches --------------------------------------------------------

static CURRENT: Mutex<Option<HotkeySettings>> = Mutex::new(None);
static AUDIO_CURRENT: Mutex<Option<AudioSettings>> = Mutex::new(None);
static BEHAVIOR_CURRENT: Mutex<Option<BehaviorSettings>> = Mutex::new(None);
static STATS_CURRENT: Mutex<Option<StatsSettings>> = Mutex::new(None);
static PERFORMANCE_CURRENT: Mutex<Option<PerformanceSettings>> = Mutex::new(None);
static REFINE_CURRENT: Mutex<Option<RefineSettings>> = Mutex::new(None);
static HISTORY_CURRENT: Mutex<Option<HistorySettings>> = Mutex::new(None);
static MODES_CURRENT: Mutex<Option<ModesSettings>> = Mutex::new(None);
static ASK_CURRENT: Mutex<Option<AskSettings>> = Mutex::new(None);
static VISION_CURRENT: Mutex<Option<VisionSettings>> = Mutex::new(None);

/// Process-global mirror of `pill.follow_active_screen`. The watcher
/// thread reads this lock-free on every 500 ms tick — a `Mutex` would
/// be fine but the access pattern (read-mostly, write-on-toggle)
/// makes `AtomicBool` the right primitive. Defaults to `true` so a
/// fresh checkout (no settings.json) gets follow-on behavior.
static FOLLOW_ACTIVE_SCREEN: AtomicBool = AtomicBool::new(true);

/// Lock-free behavior caches. Hot paths (fullscreen detector callback,
/// dictation phase observer, MediaController on every resume) read
/// these on every tick — taking a `Mutex` here would be allowed but
/// unnecessary, and a stuck lock on the dictation observer would
/// silently freeze the recording UI.
static SHOW_IN_FULLSCREEN: AtomicBool = AtomicBool::new(false);
static PAUSE_AUDIO: AtomicBool = AtomicBool::new(true);
/// Default matches the schema default (5000) so any read before
/// `set_bt_resume_delay_ms_cache` runs at boot still gets a sane
/// value rather than 0 (which would defeat the purpose of the wait).
static BT_RESUME_DELAY_MS: AtomicU64 = AtomicU64::new(5000);

/// Lock-free mirror of `performance.keep_models_warm`. Read by
/// `model_lifecycle::apply_keep_warm` (called from the `set` Tauri
/// command) and by every `ModelHandle::with_idle_timeout` constructor
/// so a freshly built handle inherits the current cluster-wide
/// preference without a separate sync call.
static KEEP_MODELS_WARM: AtomicBool = AtomicBool::new(false);

/// Lock-free read of `pill.follow_active_screen`. Safe to call from
/// any thread, including the focus-window watcher.
pub fn follow_active_screen() -> bool {
    FOLLOW_ACTIVE_SCREEN.load(Ordering::Relaxed)
}

/// Lock-free read of `behavior.show_in_fullscreen`. Read by the
/// fullscreen detector on every transition.
pub fn show_in_fullscreen() -> bool {
    SHOW_IN_FULLSCREEN.load(Ordering::Relaxed)
}

/// Lock-free read of `behavior.pause_audio_during_dictation`. Read
/// by `pause_audio_for_recording` on every dictation begin.
pub fn pause_audio_during_dictation() -> bool {
    PAUSE_AUDIO.load(Ordering::Relaxed)
}

/// Lock-free read of `behavior.bt_resume_delay_ms`. Read by the
/// platform MediaController on every resume.
pub fn bt_resume_delay_ms() -> u64 {
    BT_RESUME_DELAY_MS.load(Ordering::Relaxed)
}

/// Setter for the show-in-fullscreen cache. Used by the boot-time
/// hydrate path AND by the `behavior_set_show_in_fullscreen`
/// command after a successful save.
pub fn set_show_in_fullscreen_cache(value: bool) {
    SHOW_IN_FULLSCREEN.store(value, Ordering::Relaxed);
}

pub fn set_pause_audio_cache(value: bool) {
    PAUSE_AUDIO.store(value, Ordering::Relaxed);
}

pub fn set_bt_resume_delay_ms_cache(value: u64) {
    BT_RESUME_DELAY_MS.store(value, Ordering::Relaxed);
}

/// Lock-free read of `performance.keep_models_warm`. Read by
/// [`crate::model_lifecycle::apply_keep_warm`] and by
/// `ModelHandle::with_idle_timeout` at construction time so a
/// fresh handle inherits the persisted preference.
pub fn keep_models_warm() -> bool {
    KEEP_MODELS_WARM.load(Ordering::Relaxed)
}

/// Setter for the keep-models-warm cache. Called by the boot-time
/// hydrate path AND by `settings_set_keep_models_warm` after a
/// successful save. Returns the previous value so callers can detect
/// a no-op flip.
pub fn set_keep_models_warm_cache(value: bool) -> bool {
    KEEP_MODELS_WARM.swap(value, Ordering::Relaxed)
}

pub fn current_pill_settings() -> PillSettings {
    PillSettings {
        follow_active_screen: follow_active_screen(),
    }
}

// --- IO ------------------------------------------------------------

fn read_file(path: &Path) -> SettingsFile {
    let Ok(bytes) = fs::read(path) else {
        return SettingsFile::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// Held by every `save_*` fn (and the hotkey-slot helpers) from its
/// read of the file to its write. See the module docs.
static SAVE_LOCK: Mutex<()> = Mutex::new(());

/// The guard is only a turnstile: a writer that panicked left nothing
/// half-done that the next one could trip over, so poison is ignored.
fn saving() -> std::sync::MutexGuard<'static, ()> {
    SAVE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Makes the temp-file names of one process distinct.
static WRITE_SEQ: AtomicU64 = AtomicU64::new(0);

/// Replace the settings file in one step: write the new contents to a
/// temp file next to it, then rename that over it. A rename within a
/// folder is atomic on the file systems we run on, so nobody ever
/// reads a truncated or half-written `settings.json`, and a crash or
/// a full disk mid-write leaves the old file as it was.
fn write_file(path: &Path, file: &SettingsFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("create config dir: {e}"))?;
        }
    }
    let bytes = serde_json::to_vec_pretty(file).map_err(|e| format!("serialize: {e}"))?;
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(format!(
        ".{}.{}.tmp",
        std::process::id(),
        WRITE_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let tmp = path.with_file_name(tmp_name);
    let written = fs::write(&tmp, bytes)
        .map_err(|e| format!("write settings.json: {e}"))
        .and_then(|()| fs::rename(&tmp, path).map_err(|e| format!("replace settings.json: {e}")));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written
}

fn merge_loaded(file: SettingsFile) -> HotkeySettings {
    if let Some(h) = file.hotkeys {
        return h;
    }
    HotkeySettings {
        toggle: file.hotkey.unwrap_or_else(default_toggle_hotkey),
        cancel: default_cancel_hotkey(),
        cycle_mode: None,
        ask: Some(default_ask_hotkey()),
    }
}

/// Load saved hotkeys (with migration from the legacy single-slot
/// field) or fall back to platform defaults. Caches the result for
/// backends. Side-effect: hydrates [`follow_active_screen`] from the
/// same on-disk read so the pill watcher sees the persisted value on
/// its first poll tick after boot.
pub fn load_settings(path: &Path) -> HotkeySettings {
    if let Some(s) = CURRENT.lock().ok().and_then(|g| g.clone()) {
        return s;
    }
    let file = read_file(path);
    let pill = file.pill.clone().unwrap_or_default();
    FOLLOW_ACTIVE_SCREEN.store(pill.follow_active_screen, Ordering::Relaxed);
    let settings = merge_loaded(file);
    if let Ok(mut g) = CURRENT.lock() {
        *g = Some(settings.clone());
    }
    settings
}

pub fn current_settings() -> Option<HotkeySettings> {
    CURRENT.lock().ok().and_then(|g| g.clone())
}

/// Persist a new `HotkeySettings`. Re-reads the on-disk file first
/// to merge with any sibling block written by another call site
/// since boot.
pub fn save_settings(path: &Path, settings: HotkeySettings) -> Result<(), String> {
    let _saving = saving();
    save_settings_locked(path, settings)
}

/// [`save_settings`] for a caller that already holds [`SAVE_LOCK`].
fn save_settings_locked(path: &Path, settings: HotkeySettings) -> Result<(), String> {
    let file = read_file(path);
    let audio = file.audio.or_else(current_audio_settings);
    let behavior = file.behavior.or_else(current_behavior_settings);
    let stats = file.stats.or_else(current_stats_settings);
    let pill = file.pill.unwrap_or_else(current_pill_settings);
    let performance = file.performance.or_else(current_performance_settings);
    let merged = SettingsFile {
        hotkey: None,
        hotkeys: Some(settings.clone()),
        audio,
        behavior,
        pill: Some(pill),
        stats,
        performance,
        refine: file.refine.or_else(current_refine_json),
        history: file.history.or_else(current_history_json),
        modes: file.modes.or_else(current_modes_json),
        ask: file.ask.or_else(current_ask_json),
        vision: file.vision.or_else(current_vision_json),
    };
    write_file(path, &merged)?;
    if let Ok(mut g) = CURRENT.lock() {
        *g = Some(settings);
    }
    Ok(())
}

pub fn current_stats_settings() -> Option<StatsSettings> {
    STATS_CURRENT.lock().ok().and_then(|g| *g)
}

pub fn load_stats_settings(path: &Path) -> StatsSettings {
    if let Some(s) = STATS_CURRENT.lock().ok().and_then(|g| *g) {
        return s;
    }
    let file = read_file(path);
    let settings = file.stats.unwrap_or_default().clamped();
    if let Ok(mut g) = STATS_CURRENT.lock() {
        *g = Some(settings);
    }
    settings
}

pub fn save_stats_settings(path: &Path, settings: StatsSettings) -> Result<(), String> {
    let clamped = settings.clamped();
    let _saving = saving();
    let file = read_file(path);
    let hotkeys = file.hotkeys.or_else(current_settings);
    let audio = file.audio.or_else(current_audio_settings);
    let behavior = file.behavior.or_else(current_behavior_settings);
    let pill = file.pill.unwrap_or_else(current_pill_settings);
    let performance = file.performance.or_else(current_performance_settings);
    let merged = SettingsFile {
        hotkey: None,
        hotkeys,
        audio,
        behavior,
        pill: Some(pill),
        stats: Some(clamped),
        performance,
        refine: file.refine.or_else(current_refine_json),
        history: file.history.or_else(current_history_json),
        modes: file.modes.or_else(current_modes_json),
        ask: file.ask.or_else(current_ask_json),
        vision: file.vision.or_else(current_vision_json),
    };
    write_file(path, &merged)?;
    if let Ok(mut g) = STATS_CURRENT.lock() {
        *g = Some(clamped);
    }
    Ok(())
}

pub fn current_audio_settings() -> Option<AudioSettings> {
    AUDIO_CURRENT.lock().ok().and_then(|g| g.clone())
}

/// Load the audio block from disk on the first call, then cache.
/// Returns the default (no device selected) if the file is absent or
/// the audio block is missing.
pub fn load_audio_settings(path: &Path) -> AudioSettings {
    if let Some(s) = AUDIO_CURRENT.lock().ok().and_then(|g| g.clone()) {
        return s;
    }
    let file = read_file(path);
    let settings = file.audio.unwrap_or_default();
    if let Ok(mut g) = AUDIO_CURRENT.lock() {
        *g = Some(settings.clone());
    }
    settings
}

pub fn save_audio_settings(path: &Path, settings: AudioSettings) -> Result<(), String> {
    let _saving = saving();
    let file = read_file(path);
    let hotkeys = file.hotkeys.or_else(current_settings);
    let behavior = file.behavior.or_else(current_behavior_settings);
    let pill = file.pill.or_else(|| Some(current_pill_settings()));
    let stats = file.stats.or_else(current_stats_settings);
    let performance = file.performance.or_else(current_performance_settings);
    let merged = SettingsFile {
        hotkey: None,
        hotkeys,
        audio: Some(settings.clone()),
        behavior,
        pill,
        stats,
        performance,
        refine: file.refine.or_else(current_refine_json),
        history: file.history.or_else(current_history_json),
        modes: file.modes.or_else(current_modes_json),
        ask: file.ask.or_else(current_ask_json),
        vision: file.vision.or_else(current_vision_json),
    };
    write_file(path, &merged)?;
    if let Ok(mut g) = AUDIO_CURRENT.lock() {
        *g = Some(settings);
    }
    Ok(())
}

pub fn current_behavior_settings() -> Option<BehaviorSettings> {
    BEHAVIOR_CURRENT.lock().ok().and_then(|g| g.clone())
}

pub fn load_behavior_settings(path: &Path) -> BehaviorSettings {
    if let Some(s) = BEHAVIOR_CURRENT.lock().ok().and_then(|g| g.clone()) {
        return s;
    }
    let file = read_file(path);
    let settings = file.behavior.unwrap_or_default();
    if let Ok(mut g) = BEHAVIOR_CURRENT.lock() {
        *g = Some(settings.clone());
    }
    settings
}

pub fn save_behavior_settings(
    path: &Path,
    settings: BehaviorSettings,
) -> Result<(), String> {
    let _saving = saving();
    let file = read_file(path);
    let hotkeys = file.hotkeys.or_else(current_settings);
    let audio = file.audio.or_else(current_audio_settings);
    let pill = file.pill.or_else(|| Some(current_pill_settings()));
    let stats = file.stats.or_else(current_stats_settings);
    let performance = file.performance.or_else(current_performance_settings);
    let merged = SettingsFile {
        hotkey: None,
        hotkeys,
        audio,
        behavior: Some(settings.clone()),
        pill,
        stats,
        performance,
        refine: file.refine.or_else(current_refine_json),
        history: file.history.or_else(current_history_json),
        modes: file.modes.or_else(current_modes_json),
        ask: file.ask.or_else(current_ask_json),
        vision: file.vision.or_else(current_vision_json),
    };
    write_file(path, &merged)?;
    if let Ok(mut g) = BEHAVIOR_CURRENT.lock() {
        *g = Some(settings);
    }
    Ok(())
}

pub fn save_pill_settings(path: &Path, settings: PillSettings) -> Result<(), String> {
    let _saving = saving();
    let file = read_file(path);
    let hotkeys = file.hotkeys.or_else(current_settings);
    let audio = file.audio.or_else(current_audio_settings);
    let behavior = file.behavior.or_else(current_behavior_settings);
    let stats = file.stats.or_else(current_stats_settings);
    let performance = file.performance.or_else(current_performance_settings);
    let merged = SettingsFile {
        hotkey: None,
        hotkeys,
        audio,
        behavior,
        pill: Some(settings.clone()),
        stats,
        performance,
        refine: file.refine.or_else(current_refine_json),
        history: file.history.or_else(current_history_json),
        modes: file.modes.or_else(current_modes_json),
        ask: file.ask.or_else(current_ask_json),
        vision: file.vision.or_else(current_vision_json),
    };
    write_file(path, &merged)?;
    FOLLOW_ACTIVE_SCREEN.store(settings.follow_active_screen, Ordering::Relaxed);
    Ok(())
}

pub fn current_performance_settings() -> Option<PerformanceSettings> {
    PERFORMANCE_CURRENT.lock().ok().and_then(|g| *g)
}

/// Load the performance block from disk on the first call, then
/// cache. Side-effect: hydrates the `KEEP_MODELS_WARM` lock-free
/// mirror so any handle constructed via `ModelHandle::with_idle_timeout`
/// after this point sees the persisted value.
pub fn load_performance_settings(path: &Path) -> PerformanceSettings {
    if let Some(s) = PERFORMANCE_CURRENT.lock().ok().and_then(|g| *g) {
        return s;
    }
    let file = read_file(path);
    let settings = file.performance.unwrap_or_default();
    KEEP_MODELS_WARM.store(settings.keep_models_warm, Ordering::Relaxed);
    if let Ok(mut g) = PERFORMANCE_CURRENT.lock() {
        *g = Some(settings);
    }
    settings
}

pub fn save_performance_settings(
    path: &Path,
    settings: PerformanceSettings,
) -> Result<(), String> {
    let _saving = saving();
    let file = read_file(path);
    let hotkeys = file.hotkeys.or_else(current_settings);
    let audio = file.audio.or_else(current_audio_settings);
    let behavior = file.behavior.or_else(current_behavior_settings);
    let pill = file.pill.or_else(|| Some(current_pill_settings()));
    let stats = file.stats.or_else(current_stats_settings);
    let merged = SettingsFile {
        hotkey: None,
        hotkeys,
        audio,
        behavior,
        pill,
        stats,
        performance: Some(settings),
        refine: file.refine.or_else(current_refine_json),
        history: file.history.or_else(current_history_json),
        modes: file.modes.or_else(current_modes_json),
        ask: file.ask.or_else(current_ask_json),
        vision: file.vision.or_else(current_vision_json),
    };
    write_file(path, &merged)?;
    KEEP_MODELS_WARM.store(settings.keep_models_warm, Ordering::Relaxed);
    if let Ok(mut g) = PERFORMANCE_CURRENT.lock() {
        *g = Some(settings);
    }
    Ok(())
}

/// The Refine block as last loaded or saved in this process; defaults
/// (Refine off) when nothing was loaded yet. Read by the dictation
/// pipeline once per recording.
pub fn current_refine_settings() -> RefineSettings {
    REFINE_CURRENT
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default()
}

/// Seed the Refine cache without touching disk. For shells that
/// resolve settings some other way, and for tests.
pub fn set_refine_settings_cache(settings: RefineSettings) {
    if let Ok(mut g) = REFINE_CURRENT.lock() {
        *g = Some(settings.cleaned());
    }
}

fn current_refine_json() -> Option<serde_json::Value> {
    let current = REFINE_CURRENT.lock().ok().and_then(|g| g.clone())?;
    serde_json::to_value(current).ok()
}

/// Read the Refine block from disk and cache it. Unlike the other
/// `load_*` fns this always re-reads: the block is also edited by the
/// headless CLI (`openwhisper refine config`), and the shell calls
/// this when its settings pane opens. A missing or malformed block
/// yields the defaults.
pub fn load_refine_settings(path: &Path) -> RefineSettings {
    let file = read_file(path);
    let settings = file
        .refine
        .and_then(|v| serde_json::from_value::<RefineSettings>(v).ok())
        .unwrap_or_default()
        .cleaned();
    if let Ok(mut g) = REFINE_CURRENT.lock() {
        *g = Some(settings.clone());
    }
    settings
}

/// Persist the Refine block (cleaned, see [`RefineSettings::cleaned`])
/// and update the cache. Returns what was written.
pub fn save_refine_settings(path: &Path, settings: RefineSettings) -> Result<RefineSettings, String> {
    let settings = settings.cleaned();
    let _saving = saving();
    let file = read_file(path);
    let hotkeys = file.hotkeys.or_else(current_settings);
    let audio = file.audio.or_else(current_audio_settings);
    let behavior = file.behavior.or_else(current_behavior_settings);
    let pill = file.pill.or_else(|| Some(current_pill_settings()));
    let stats = file.stats.or_else(current_stats_settings);
    let performance = file.performance.or_else(current_performance_settings);
    let merged = SettingsFile {
        hotkey: None,
        hotkeys,
        audio,
        behavior,
        pill,
        stats,
        performance,
        refine: Some(serde_json::to_value(&settings).map_err(|e| format!("serialize: {e}"))?),
        history: file.history.or_else(current_history_json),
        modes: file.modes.or_else(current_modes_json),
        ask: file.ask.or_else(current_ask_json),
        vision: file.vision.or_else(current_vision_json),
    };
    write_file(path, &merged)?;
    if let Ok(mut g) = REFINE_CURRENT.lock() {
        *g = Some(settings.clone());
    }
    Ok(settings)
}

/// The history block as last loaded or saved in this process;
/// defaults (history off) when nothing was loaded yet. Read by the
/// dictation pipeline on every delivered transcript.
pub fn current_history_settings() -> HistorySettings {
    HISTORY_CURRENT.lock().ok().and_then(|g| g.clone()).unwrap_or_default()
}

/// Seed the history cache without touching disk. For shells that
/// resolve settings some other way, and for tests.
pub fn set_history_settings_cache(settings: HistorySettings) {
    if let Ok(mut g) = HISTORY_CURRENT.lock() {
        *g = Some(settings.cleaned());
    }
}

fn current_history_json() -> Option<serde_json::Value> {
    let current = HISTORY_CURRENT.lock().ok().and_then(|g| g.clone())?;
    serde_json::to_value(current).ok()
}

/// Read the history block from disk and cache it. Always re-reads,
/// like [`load_refine_settings`]: the headless CLI
/// (`openwhisper history config`) edits the same block. A missing or
/// malformed block yields the defaults (history off).
pub fn load_history_settings(path: &Path) -> HistorySettings {
    let file = read_file(path);
    let settings = file
        .history
        .and_then(|v| serde_json::from_value::<HistorySettings>(v).ok())
        .unwrap_or_default()
        .cleaned();
    if let Ok(mut g) = HISTORY_CURRENT.lock() {
        *g = Some(settings.clone());
    }
    settings
}

/// Persist the history block (cleaned) and update the cache. Returns
/// what was written.
pub fn save_history_settings(path: &Path, settings: HistorySettings) -> Result<HistorySettings, String> {
    let settings = settings.cleaned();
    let _saving = saving();
    let file = read_file(path);
    let hotkeys = file.hotkeys.or_else(current_settings);
    let audio = file.audio.or_else(current_audio_settings);
    let behavior = file.behavior.or_else(current_behavior_settings);
    let pill = file.pill.or_else(|| Some(current_pill_settings()));
    let stats = file.stats.or_else(current_stats_settings);
    let performance = file.performance.or_else(current_performance_settings);
    let merged = SettingsFile {
        hotkey: None,
        hotkeys,
        audio,
        behavior,
        pill,
        stats,
        performance,
        refine: file.refine.or_else(current_refine_json),
        history: Some(serde_json::to_value(&settings).map_err(|e| format!("serialize: {e}"))?),
        modes: file.modes.or_else(current_modes_json),
        ask: file.ask.or_else(current_ask_json),
        vision: file.vision.or_else(current_vision_json),
    };
    write_file(path, &merged)?;
    if let Ok(mut g) = HISTORY_CURRENT.lock() {
        *g = Some(settings.clone());
    }
    Ok(settings)
}

/// The modes block as last loaded or saved in this process; defaults
/// (only the built-in mode) when nothing was loaded yet. Read by the
/// dictation pipeline once per recording, so the shell loads it at
/// boot and again after every change, whoever made it.
pub fn current_modes_settings() -> ModesSettings {
    MODES_CURRENT.lock().ok().and_then(|g| g.clone()).unwrap_or_default()
}

/// Seed the modes cache without touching disk. For shells that
/// resolve settings some other way, and for tests.
pub fn set_modes_settings_cache(settings: ModesSettings) {
    if let Ok(mut g) = MODES_CURRENT.lock() {
        *g = Some(settings.cleaned());
    }
}

fn current_modes_json() -> Option<serde_json::Value> {
    let current = MODES_CURRENT.lock().ok().and_then(|g| g.clone())?;
    serde_json::to_value(current).ok()
}

/// Read the modes block from disk and cache it. Always re-reads, like
/// [`load_refine_settings`]: the headless CLI (`openwhisper modes`)
/// edits the same block. A missing or malformed block yields the
/// defaults (only the built-in mode, selected).
pub fn load_modes_settings(path: &Path) -> ModesSettings {
    let file = read_file(path);
    let settings = file
        .modes
        .and_then(|v| serde_json::from_value::<ModesSettings>(v).ok())
        .unwrap_or_default()
        .cleaned();
    if let Ok(mut g) = MODES_CURRENT.lock() {
        *g = Some(settings.clone());
    }
    settings
}

/// Persist the modes block (cleaned, see [`ModesSettings::cleaned`])
/// and update the cache. Returns what was written.
pub fn save_modes_settings(path: &Path, settings: ModesSettings) -> Result<ModesSettings, String> {
    let settings = settings.cleaned();
    let _saving = saving();
    let file = read_file(path);
    let hotkeys = file.hotkeys.or_else(current_settings);
    let audio = file.audio.or_else(current_audio_settings);
    let behavior = file.behavior.or_else(current_behavior_settings);
    let pill = file.pill.or_else(|| Some(current_pill_settings()));
    let stats = file.stats.or_else(current_stats_settings);
    let performance = file.performance.or_else(current_performance_settings);
    let merged = SettingsFile {
        hotkey: None,
        hotkeys,
        audio,
        behavior,
        pill,
        stats,
        performance,
        refine: file.refine.or_else(current_refine_json),
        history: file.history.or_else(current_history_json),
        modes: Some(serde_json::to_value(&settings).map_err(|e| format!("serialize: {e}"))?),
        ask: file.ask.or_else(current_ask_json),
        vision: file.vision.or_else(current_vision_json),
    };
    write_file(path, &merged)?;
    if let Ok(mut g) = MODES_CURRENT.lock() {
        *g = Some(settings.clone());
    }
    Ok(settings)
}

/// The Ask block as last loaded or saved in this process; defaults
/// (Ask off) when nothing was loaded yet. Read when an ask starts, so
/// the shell loads it at boot and again after every change, whoever
/// made it.
pub fn current_ask_settings() -> AskSettings {
    ASK_CURRENT.lock().ok().and_then(|g| g.clone()).unwrap_or_default()
}

/// Seed the Ask cache without touching disk. For shells that resolve
/// settings some other way, and for tests.
pub fn set_ask_settings_cache(settings: AskSettings) {
    if let Ok(mut g) = ASK_CURRENT.lock() {
        *g = Some(settings.cleaned());
    }
}

fn current_ask_json() -> Option<serde_json::Value> {
    let current = ASK_CURRENT.lock().ok().and_then(|g| g.clone())?;
    serde_json::to_value(current).ok()
}

/// Read the Ask block from disk and cache it. Always re-reads, like
/// [`load_refine_settings`]: the headless CLI (`openwhisper ask
/// config`) edits the same block. A missing or malformed block yields
/// the defaults (Ask off).
pub fn load_ask_settings(path: &Path) -> AskSettings {
    let file = read_file(path);
    let settings = file
        .ask
        .and_then(|v| serde_json::from_value::<AskSettings>(v).ok())
        .unwrap_or_default()
        .cleaned();
    if let Ok(mut g) = ASK_CURRENT.lock() {
        *g = Some(settings.clone());
    }
    settings
}

/// Persist the Ask block (cleaned, see [`AskSettings::cleaned`]) and
/// update the cache. Returns what was written.
pub fn save_ask_settings(path: &Path, settings: AskSettings) -> Result<AskSettings, String> {
    let settings = settings.cleaned();
    let _saving = saving();
    let file = read_file(path);
    let hotkeys = file.hotkeys.or_else(current_settings);
    let audio = file.audio.or_else(current_audio_settings);
    let behavior = file.behavior.or_else(current_behavior_settings);
    let pill = file.pill.or_else(|| Some(current_pill_settings()));
    let stats = file.stats.or_else(current_stats_settings);
    let performance = file.performance.or_else(current_performance_settings);
    let merged = SettingsFile {
        hotkey: None,
        hotkeys,
        audio,
        behavior,
        pill,
        stats,
        performance,
        refine: file.refine.or_else(current_refine_json),
        history: file.history.or_else(current_history_json),
        modes: file.modes.or_else(current_modes_json),
        ask: Some(serde_json::to_value(&settings).map_err(|e| format!("serialize: {e}"))?),
        vision: file.vision.or_else(current_vision_json),
    };
    write_file(path, &merged)?;
    if let Ok(mut g) = ASK_CURRENT.lock() {
        *g = Some(settings.clone());
    }
    Ok(settings)
}

/// The Vision block as last loaded or saved in this process; the
/// defaults (phrases, built-in lists) when nothing was loaded yet.
/// Read when an ask is sent, so the shell loads it at boot and again
/// after every change, whoever made it.
pub fn current_vision_settings() -> VisionSettings {
    VISION_CURRENT.lock().ok().and_then(|g| g.clone()).unwrap_or_default()
}

/// Seed the Vision cache without touching disk. For shells that
/// resolve settings some other way, and for tests.
pub fn set_vision_settings_cache(settings: VisionSettings) {
    if let Ok(mut g) = VISION_CURRENT.lock() {
        *g = Some(settings.cleaned());
    }
}

fn current_vision_json() -> Option<serde_json::Value> {
    let current = VISION_CURRENT.lock().ok().and_then(|g| g.clone())?;
    serde_json::to_value(current).ok()
}

/// Read the Vision block from disk and cache it. Always re-reads,
/// like [`load_refine_settings`]: the headless CLI (`openwhisper
/// vision config`) edits the same block. A missing block yields the
/// defaults. A block that is not an object at all reads as Vision
/// **off**: what the person chose cannot be told, so nothing is
/// captured until they choose again.
pub fn load_vision_settings(path: &Path) -> VisionSettings {
    let file = read_file(path);
    let settings = match file.vision {
        None => VisionSettings::default(),
        Some(v) => serde_json::from_value::<VisionSettings>(v)
            .unwrap_or(VisionSettings { trigger: VisionTrigger::Off, ..VisionSettings::default() }),
    }
    .cleaned();
    if let Ok(mut g) = VISION_CURRENT.lock() {
        *g = Some(settings.clone());
    }
    settings
}

/// Persist the Vision block (cleaned, see [`VisionSettings::cleaned`])
/// and update the cache. Returns what was written.
pub fn save_vision_settings(path: &Path, settings: VisionSettings) -> Result<VisionSettings, String> {
    let settings = settings.cleaned();
    let _saving = saving();
    let file = read_file(path);
    let hotkeys = file.hotkeys.or_else(current_settings);
    let audio = file.audio.or_else(current_audio_settings);
    let behavior = file.behavior.or_else(current_behavior_settings);
    let pill = file.pill.or_else(|| Some(current_pill_settings()));
    let stats = file.stats.or_else(current_stats_settings);
    let performance = file.performance.or_else(current_performance_settings);
    let merged = SettingsFile {
        hotkey: None,
        hotkeys,
        audio,
        behavior,
        pill,
        stats,
        performance,
        refine: file.refine.or_else(current_refine_json),
        history: file.history.or_else(current_history_json),
        modes: file.modes.or_else(current_modes_json),
        ask: file.ask.or_else(current_ask_json),
        vision: Some(serde_json::to_value(&settings).map_err(|e| format!("serialize: {e}"))?),
    };
    write_file(path, &merged)?;
    if let Ok(mut g) = VISION_CURRENT.lock() {
        *g = Some(settings.clone());
    }
    Ok(settings)
}

/// Mutate one slot of the hotkey block and persist. Used by the
/// shell's `settings_set_hotkey` / `settings_reset_hotkey` commands.
pub fn update_hotkey_slot(
    path: &Path,
    target: HotkeyTarget,
    config: HotkeyConfig,
) -> Result<(), String> {
    // The lock covers the read of the other slots too, so two slots
    // set from two threads both end up in the block.
    let _saving = saving();
    let mut current = current_settings().unwrap_or_else(default_settings);
    match target {
        HotkeyTarget::Toggle => current.toggle = config,
        HotkeyTarget::Cancel => current.cancel = config,
        HotkeyTarget::CycleMode => current.cycle_mode = Some(config),
        HotkeyTarget::Ask => current.ask = Some(config),
    }
    save_settings_locked(path, current)
}

/// Unbind an optional hotkey slot and persist. The toggle and cancel
/// slots cannot be unbound: without them dictation cannot be started
/// or stopped from the keyboard. A cleared ask slot stays cleared (it
/// does not fall back to its default key); set it again with
/// [`update_hotkey_slot`] and [`default_ask_hotkey`] to reset it.
pub fn clear_hotkey_slot(path: &Path, target: HotkeyTarget) -> Result<(), String> {
    let _saving = saving();
    let mut current = current_settings().unwrap_or_else(default_settings);
    match target {
        HotkeyTarget::CycleMode => current.cycle_mode = None,
        HotkeyTarget::Ask => current.ask = None,
        HotkeyTarget::Toggle | HotkeyTarget::Cancel => {
            return Err(format!(
                "The {} shortcut cannot be removed, only changed.",
                target.as_str()
            ));
        }
    }
    save_settings_locked(path, current)
}

/// Serializes the tests, in any module, that set the process-wide
/// Refine, modes, Ask or Vision cache and then rely on what is in it.
#[cfg(test)]
pub(crate) static CACHE_TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_default_user_wpm_is_40() {
        assert_eq!(StatsSettings::default().user_wpm, 40);
        assert_eq!(default_user_wpm(), 40);
    }

    #[test]
    fn stats_clamp_pulls_below_min_up_and_above_max_down() {
        assert_eq!(StatsSettings { user_wpm: 5 }.clamped().user_wpm, USER_WPM_MIN);
        assert_eq!(StatsSettings { user_wpm: 0 }.clamped().user_wpm, USER_WPM_MIN);
        assert_eq!(StatsSettings { user_wpm: 500 }.clamped().user_wpm, USER_WPM_MAX);
        assert_eq!(StatsSettings { user_wpm: 60 }.clamped().user_wpm, 60);
    }

    #[test]
    fn stats_legacy_json_without_user_wpm_defaults_to_40() {
        let legacy = r#"{"hotkeys":null}"#;
        let file: SettingsFile = serde_json::from_str(legacy).unwrap();
        let stats = file.stats.unwrap_or_default();
        assert_eq!(stats.user_wpm, 40);
    }

    #[test]
    fn behavior_default_pause_audio_is_true() {
        let s = BehaviorSettings::default();
        assert!(s.pause_audio_during_dictation);
        assert!(!s.show_in_fullscreen);
        assert_eq!(s.bt_resume_delay_ms, 5000);
    }

    #[test]
    fn behavior_serde_round_trip() {
        let original = BehaviorSettings {
            show_in_fullscreen: true,
            pause_audio_during_dictation: false,
            bt_resume_delay_ms: 2500,
        };
        let json = serde_json::to_string(&original).unwrap();
        let back: BehaviorSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(original, back);
    }

    #[test]
    fn behavior_legacy_json_without_pause_audio_defaults_true() {
        let legacy = r#"{"show_in_fullscreen":true}"#;
        let parsed: BehaviorSettings = serde_json::from_str(legacy).unwrap();
        assert!(parsed.show_in_fullscreen);
        assert!(parsed.pause_audio_during_dictation);
        assert_eq!(parsed.bt_resume_delay_ms, 5000);
    }

    #[test]
    fn behavior_legacy_json_without_bt_resume_delay_defaults_5000() {
        let legacy = r#"{"show_in_fullscreen":false,"pause_audio_during_dictation":true}"#;
        let parsed: BehaviorSettings = serde_json::from_str(legacy).unwrap();
        assert_eq!(parsed.bt_resume_delay_ms, 5000);
    }

    #[test]
    fn behavior_empty_json_uses_full_defaults() {
        let parsed: BehaviorSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed, BehaviorSettings::default());
    }

    #[test]
    fn performance_default_keep_models_warm_is_false() {
        assert!(!PerformanceSettings::default().keep_models_warm);
    }

    #[test]
    fn performance_legacy_json_without_block_defaults_false() {
        let legacy = r#"{"hotkeys":null,"behavior":null}"#;
        let file: SettingsFile = serde_json::from_str(legacy).unwrap();
        let perf = file.performance.unwrap_or_default();
        assert!(!perf.keep_models_warm);
    }

    #[test]
    fn performance_save_load_round_trip_via_disk() {
        // Use a real temp dir so we exercise the same write_file
        // codepath the Tauri shell hits. tempfile is already a
        // dev-dep used elsewhere in core::store tests.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");

        // Wipe the process-global cache so the load() doesn't short-
        // circuit on a value seeded by a sibling test running in
        // parallel. The cache is for the process lifetime; tests
        // share it.
        if let Ok(mut g) = PERFORMANCE_CURRENT.lock() {
            *g = None;
        }

        save_performance_settings(
            &path,
            PerformanceSettings {
                keep_models_warm: true,
            },
        )
        .unwrap();

        // The save populates the cache — confirm.
        let cached = current_performance_settings().unwrap();
        assert!(cached.keep_models_warm);
        assert!(keep_models_warm());

        // And the on-disk JSON contains the new value (load_ short-
        // circuits on the cache, so we read the file directly).
        let bytes = std::fs::read(&path).unwrap();
        let parsed: SettingsFile = serde_json::from_slice(&bytes).unwrap();
        assert!(parsed.performance.unwrap().keep_models_warm);
    }

    /// Serializes the tests that touch the process-wide hotkey cache.
    static HOTKEY_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn hotkeys_written_before_the_cycle_slot_read_with_it_unbound() {
        let old = r#"{"toggle":{"kind":"chord","code":"Space","mods":["Ctrl"]},"cancel":{"kind":"chord","code":"Escape","mods":[]}}"#;
        let parsed: HotkeySettings = serde_json::from_str(old).unwrap();
        assert_eq!(parsed.cycle_mode, None);
        // Unbound, the slot is not written at all: the file keeps the
        // shape an older app reads.
        let json = serde_json::to_string(&parsed).unwrap();
        assert!(!json.contains("cycle_mode"), "{json}");
        assert_eq!(default_settings().cycle_mode, None);

        let bound = HotkeySettings { cycle_mode: Some(HotkeyConfig::chord("KeyM", &["Ctrl", "Alt"])), ..parsed };
        let json = serde_json::to_string(&bound).unwrap();
        assert_eq!(serde_json::from_str::<HotkeySettings>(&json).unwrap(), bound);
    }

    #[test]
    fn legacy_single_hotkey_still_migrates() {
        let legacy = r#"{"hotkey":{"kind":"chord","code":"F5","mods":[]}}"#;
        let file: SettingsFile = serde_json::from_str(legacy).unwrap();
        let merged = merge_loaded(file);
        assert_eq!(merged.toggle, HotkeyConfig::chord("F5", &[]));
        assert_eq!(merged.cancel, default_cancel_hotkey());
        assert_eq!(merged.cycle_mode, None);
    }

    #[test]
    fn hotkey_target_ids_match_the_serialized_form() {
        for t in [HotkeyTarget::Toggle, HotkeyTarget::Cancel, HotkeyTarget::CycleMode, HotkeyTarget::Ask] {
            assert_eq!(serde_json::to_string(&t).unwrap(), format!("\"{}\"", t.as_str()));
        }
        assert_eq!(HotkeyTarget::CycleMode.as_str(), "cycle-mode");
        assert!(HotkeyTarget::CycleMode.is_optional());
        assert!(!HotkeyTarget::Toggle.is_optional());
        assert_eq!(HotkeyTarget::Ask.as_str(), "ask");
        assert!(HotkeyTarget::Ask.is_optional());
    }

    #[test]
    fn hotkeys_written_before_the_ask_slot_read_with_its_default_key() {
        let old = r#"{"toggle":{"kind":"chord","code":"Space","mods":["Ctrl"]},"cancel":{"kind":"chord","code":"Escape","mods":[]}}"#;
        let parsed: HotkeySettings = serde_json::from_str(old).unwrap();
        assert_eq!(parsed.ask, Some(default_ask_hotkey()));
        assert_eq!(default_settings().ask, Some(default_ask_hotkey()));
        #[cfg(target_os = "macos")]
        assert_eq!(default_ask_hotkey(), HotkeyConfig::modifier_tap("RightOption"));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(default_ask_hotkey(), HotkeyConfig::chord("Space", &["Ctrl", "Shift"]));

        // Cleared is written as null and reads back as cleared, not
        // as the default.
        let cleared = HotkeySettings { ask: None, ..parsed.clone() };
        let json = serde_json::to_string(&cleared).unwrap();
        assert!(json.contains("\"ask\":null"), "{json}");
        assert_eq!(serde_json::from_str::<HotkeySettings>(&json).unwrap().ask, None);

        let bound = HotkeySettings { ask: Some(HotkeyConfig::chord("KeyA", &["Ctrl", "Alt"])), ..parsed };
        let json = serde_json::to_string(&bound).unwrap();
        assert_eq!(serde_json::from_str::<HotkeySettings>(&json).unwrap(), bound);

        // The legacy single-hotkey file gets the default too.
        let legacy: SettingsFile = serde_json::from_str(r#"{"hotkey":{"kind":"chord","code":"F5","mods":[]}}"#).unwrap();
        assert_eq!(merge_loaded(legacy).ask, Some(default_ask_hotkey()));
    }

    #[test]
    fn the_ask_slot_can_be_set_and_cleared() {
        let _lock = HOTKEY_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        if let Ok(mut g) = CURRENT.lock() {
            *g = None;
        }
        let read = || -> HotkeySettings {
            let on_disk: SettingsFile = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            on_disk.hotkeys.unwrap()
        };

        let chord = HotkeyConfig::chord("KeyA", &["Ctrl", "Alt"]);
        update_hotkey_slot(&path, HotkeyTarget::Ask, chord.clone()).unwrap();
        assert_eq!(read().ask, Some(chord));
        assert_eq!(read().toggle, default_toggle_hotkey());

        clear_hotkey_slot(&path, HotkeyTarget::Ask).unwrap();
        assert_eq!(read().ask, None);
        assert_eq!(current_settings().unwrap().ask, None);
        // Another slot saved afterwards leaves it cleared.
        update_hotkey_slot(&path, HotkeyTarget::CycleMode, HotkeyConfig::chord("KeyM", &["Ctrl"])).unwrap();
        assert_eq!(read().ask, None);

        if let Ok(mut g) = CURRENT.lock() {
            *g = None;
        }
    }

    #[test]
    fn vision_block_saves_loads_and_rides_through_other_saves() {
        let _hotkeys = HOTKEY_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _lock = CACHE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        if let Ok(mut g) = CURRENT.lock() {
            *g = None;
        }
        // A file from before Vision existed: the defaults.
        std::fs::write(&path, r#"{"stats":{"user_wpm":70},"ask":{"enabled":true}}"#).unwrap();
        assert_eq!(load_vision_settings(&path), VisionSettings::default());

        let vision = VisionSettings {
            trigger: VisionTrigger::Smart,
            phrases: Some(vec!["  on my screen ".into(), "On My Screen".into()]),
            never_capture: Some(vec![NeverCaptureApp::new("Com.Example.Vault", "Vault")]),
        };
        let written = save_vision_settings(&path, vision).unwrap();
        assert_eq!(written.phrases.as_deref(), Some(&["on my screen".to_string()][..]));
        assert_eq!(written.never_capture.as_ref().unwrap()[0].id, "com.example.vault");
        assert_eq!(current_vision_settings(), written);
        assert_eq!(load_vision_settings(&path), written);

        // Saves of other blocks keep it, and it keeps theirs.
        save_stats_settings(&path, StatsSettings { user_wpm: 55 }).unwrap();
        save_modes_settings(&path, ModesSettings::default()).unwrap();
        save_refine_settings(&path, RefineSettings::default()).unwrap();
        save_ask_settings(&path, AskSettings::default()).unwrap();
        update_hotkey_slot(&path, HotkeyTarget::Cancel, default_cancel_hotkey()).unwrap();
        let on_disk: SettingsFile = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let kept: VisionSettings = serde_json::from_value(on_disk.vision.unwrap()).unwrap();
        assert_eq!(kept, written);
        assert_eq!(on_disk.stats.unwrap().user_wpm, 55);
        // And a save of the Vision block keeps the others.
        save_vision_settings(&path, VisionSettings::default()).unwrap();
        assert_eq!(read_file(&path).stats.unwrap().user_wpm, 55);
        assert!(read_file(&path).ask.is_some());

        // One broken field does not cost the block: the trigger the
        // person chose still holds, the list is the built-in one.
        std::fs::write(&path, r#"{"vision":{"trigger":"off","phrases":17},"stats":{"user_wpm":70}}"#).unwrap();
        let loaded = load_vision_settings(&path);
        assert_eq!((loaded.trigger, loaded.phrases.clone()), (VisionTrigger::Off, None));
        // A block that is not an object, or a trigger nobody knows:
        // nothing is captured, and the siblings are left alone.
        for raw in [r#"{"vision":"always","stats":{"user_wpm":70}}"#, r#"{"vision":{"trigger":"nonstop"},"stats":{"user_wpm":70}}"#] {
            std::fs::write(&path, raw).unwrap();
            assert_eq!(load_vision_settings(&path).trigger, VisionTrigger::Off, "{raw}");
            assert_eq!(read_file(&path).stats.unwrap().user_wpm, 70);
        }

        set_vision_settings_cache(VisionSettings::default());
        set_ask_settings_cache(AskSettings::default());
        set_modes_settings_cache(ModesSettings::default());
        set_refine_settings_cache(RefineSettings::default());
        if let Ok(mut g) = CURRENT.lock() {
            *g = None;
        }
    }

    #[test]
    fn ask_block_saves_loads_and_rides_through_other_saves() {
        let _hotkeys = HOTKEY_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _lock = CACHE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        if let Ok(mut g) = CURRENT.lock() {
            *g = None;
        }
        // A file from before Ask existed: the defaults, Ask off.
        std::fs::write(&path, r#"{"stats":{"user_wpm":70},"refine":{"enabled":true}}"#).unwrap();
        assert_eq!(load_ask_settings(&path), AskSettings::default());

        let ask = AskSettings {
            enabled: true,
            web_search: true,
            instructions: "  Answer in Danish.  ".into(),
            model: Some("opus".into()),
            effort: Some(RefineEffort::Medium),
            timeout_ms: 1,
        };
        let written = save_ask_settings(&path, ask).unwrap();
        assert_eq!(written.instructions, "Answer in Danish.");
        assert_eq!(written.timeout_ms, ASK_TIMEOUT_MIN_MS);
        assert_eq!(current_ask_settings(), written);
        assert_eq!(load_ask_settings(&path), written);

        // Saves of other blocks keep it, and it keeps theirs.
        save_stats_settings(&path, StatsSettings { user_wpm: 55 }).unwrap();
        save_modes_settings(&path, ModesSettings::default()).unwrap();
        save_refine_settings(&path, RefineSettings::default()).unwrap();
        update_hotkey_slot(&path, HotkeyTarget::Cancel, default_cancel_hotkey()).unwrap();
        let on_disk: SettingsFile = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let kept: AskSettings = serde_json::from_value(on_disk.ask.unwrap()).unwrap();
        assert_eq!(kept, written);
        assert_eq!(on_disk.stats.unwrap().user_wpm, 55);

        // A malformed block reads as the defaults and leaves its
        // siblings alone.
        std::fs::write(&path, r#"{"ask":{"enabled":"yes please"},"stats":{"user_wpm":70}}"#).unwrap();
        assert_eq!(load_ask_settings(&path), AskSettings::default());
        assert_eq!(read_file(&path).stats.unwrap().user_wpm, 70);

        set_ask_settings_cache(AskSettings::default());
        set_modes_settings_cache(ModesSettings::default());
        set_refine_settings_cache(RefineSettings::default());
        if let Ok(mut g) = CURRENT.lock() {
            *g = None;
        }
    }

    #[test]
    fn the_cycle_slot_can_be_set_and_cleared_and_the_others_cannot_be_cleared() {
        let _lock = HOTKEY_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        if let Ok(mut g) = CURRENT.lock() {
            *g = None;
        }

        let chord = HotkeyConfig::chord("KeyM", &["Ctrl", "Alt"]);
        update_hotkey_slot(&path, HotkeyTarget::CycleMode, chord.clone()).unwrap();
        let on_disk: SettingsFile = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let hotkeys = on_disk.hotkeys.unwrap();
        assert_eq!(hotkeys.cycle_mode, Some(chord));
        assert_eq!(hotkeys.toggle, default_toggle_hotkey());

        clear_hotkey_slot(&path, HotkeyTarget::CycleMode).unwrap();
        let on_disk: SettingsFile = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(on_disk.hotkeys.unwrap().cycle_mode, None);
        assert_eq!(current_settings().unwrap().cycle_mode, None);

        let err = clear_hotkey_slot(&path, HotkeyTarget::Toggle).unwrap_err();
        assert!(err.contains("toggle"), "{err}");
        assert!(clear_hotkey_slot(&path, HotkeyTarget::Cancel).is_err());
        assert_eq!(current_settings().unwrap().toggle, default_toggle_hotkey());

        if let Ok(mut g) = CURRENT.lock() {
            *g = None;
        }
    }

    #[test]
    fn modes_block_saves_loads_and_rides_through_other_saves() {
        let _lock = CACHE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let modes = ModesSettings {
            active: "formal".into(),
            custom: vec![Mode {
                name: "Formal".into(),
                instructions: "Rewrite it in a formal tone.".into(),
                ..Mode::default()
            }],
            app_defaults: vec![AppDefault {
                app_id: "com.example.mail".into(),
                app_name: "Mail".into(),
                mode: "formal".into(),
            }],
        };
        let written = save_modes_settings(&path, modes).unwrap();
        assert_eq!(written.custom[0].id, "formal");
        assert_eq!(written.active, "formal");
        assert_eq!(current_modes_settings(), written);
        assert_eq!(load_modes_settings(&path), written);

        // Other blocks are saved by call sites that know nothing about
        // modes: the modes block must still be in the file after.
        save_stats_settings(&path, StatsSettings { user_wpm: 55 }).unwrap();
        save_audio_settings(&path, AudioSettings::default()).unwrap();
        let on_disk: SettingsFile = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let kept: ModesSettings = serde_json::from_value(on_disk.modes.unwrap()).unwrap();
        assert_eq!(kept, written);

        // And saving modes keeps the other blocks.
        save_modes_settings(&path, ModesSettings::default()).unwrap();
        let on_disk: SettingsFile = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(on_disk.stats.unwrap().user_wpm, 55);
        assert_eq!(load_modes_settings(&path), ModesSettings::default());
    }

    /// Five writers, each saving another block of the same file at the
    /// same moment, round after round. Every `save_*` reads the file,
    /// replaces its block and writes the file back; without the save
    /// lock one of them writes back the copy it read before another
    /// one's write, and that block is back at its old value.
    #[test]
    fn concurrent_saves_of_different_blocks_all_survive() {
        let _hotkeys = HOTKEY_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _lock = CACHE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        if let Ok(mut g) = CURRENT.lock() {
            *g = None;
        }

        for round in 0..150u32 {
            let stats = StatsSettings { user_wpm: 50 + round };
            let modes = ModesSettings {
                custom: vec![Mode { name: format!("Mode {round}"), ..Mode::default() }],
                ..ModesSettings::default()
            };
            let refine = RefineSettings { instructions: format!("Round {round}."), ..RefineSettings::default() };
            let history = HistorySettings { retention_days: round + 1, ..HistorySettings::default() };
            let chord = HotkeyConfig::chord(&format!("F{}", round % 12 + 1), &["Ctrl", "Alt"]);

            let start = std::sync::Barrier::new(5);
            std::thread::scope(|s| {
                s.spawn(|| {
                    start.wait();
                    save_stats_settings(&path, stats).unwrap();
                });
                s.spawn(|| {
                    start.wait();
                    save_modes_settings(&path, modes.clone()).unwrap();
                });
                s.spawn(|| {
                    start.wait();
                    save_refine_settings(&path, refine.clone()).unwrap();
                });
                s.spawn(|| {
                    start.wait();
                    save_history_settings(&path, history.clone()).unwrap();
                });
                s.spawn(|| {
                    start.wait();
                    update_hotkey_slot(&path, HotkeyTarget::CycleMode, chord.clone()).unwrap();
                });
            });

            let on_disk: SettingsFile = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(on_disk.stats.unwrap().user_wpm, 50 + round, "stats, round {round}");
            let kept: ModesSettings = serde_json::from_value(on_disk.modes.unwrap()).unwrap();
            assert_eq!(kept.custom[0].name, format!("Mode {round}"), "modes, round {round}");
            let kept: RefineSettings = serde_json::from_value(on_disk.refine.unwrap()).unwrap();
            assert_eq!(kept.instructions, format!("Round {round}."), "refine, round {round}");
            let kept: HistorySettings = serde_json::from_value(on_disk.history.unwrap()).unwrap();
            assert_eq!(kept.retention_days, round + 1, "history, round {round}");
            assert_eq!(on_disk.hotkeys.unwrap().cycle_mode, Some(chord), "hotkeys, round {round}");
        }

        // Only the settings file is left: every temp file was renamed.
        let names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["settings.json"]);

        if let Ok(mut g) = CURRENT.lock() {
            *g = None;
        }
        set_modes_settings_cache(ModesSettings::default());
        set_refine_settings_cache(RefineSettings::default());
        set_history_settings_cache(HistorySettings::default());
    }

    /// The two hotkey-slot helpers read the other slots and save the
    /// block; from two threads, both slots must end up in it.
    #[test]
    fn hotkey_slots_set_from_two_threads_are_both_kept() {
        let _hotkeys = HOTKEY_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        if let Ok(mut g) = CURRENT.lock() {
            *g = None;
        }
        for round in 0..150u32 {
            let toggle = HotkeyConfig::chord(&format!("F{}", round % 12 + 1), &["Ctrl"]);
            let cancel = HotkeyConfig::chord(&format!("F{}", (round + 5) % 12 + 1), &["Alt"]);
            let start = std::sync::Barrier::new(2);
            std::thread::scope(|s| {
                s.spawn(|| {
                    start.wait();
                    update_hotkey_slot(&path, HotkeyTarget::Toggle, toggle.clone()).unwrap();
                });
                s.spawn(|| {
                    start.wait();
                    update_hotkey_slot(&path, HotkeyTarget::Cancel, cancel.clone()).unwrap();
                });
            });
            let saved = current_settings().unwrap();
            assert_eq!((&saved.toggle, &saved.cancel), (&toggle, &cancel), "cache, round {round}");
            let on_disk: SettingsFile = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let hotkeys = on_disk.hotkeys.unwrap();
            assert_eq!((hotkeys.toggle, hotkeys.cancel), (toggle, cancel), "file, round {round}");
        }
        if let Ok(mut g) = CURRENT.lock() {
            *g = None;
        }
    }

    /// A reader never sees a half-written file. With a plain
    /// truncate-and-write, the file is empty or cut short for a
    /// moment on every save, and a read in that moment does not parse.
    #[test]
    fn a_reader_never_sees_a_half_written_settings_file() {
        let _lock = CACHE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        // A large block, so a write takes long enough to be caught,
        // and the same size every round, so a reader can tell a whole
        // file from a partial one by its length alone.
        let block = |round: usize| ModesSettings {
            custom: (0..MODES_MAX)
                .map(|i| Mode {
                    name: format!("Mode {i:02} of round {round:03}"),
                    instructions: "Rewrite what I say in a formal tone. ".repeat(100),
                    ..Mode::default()
                })
                .collect(),
            ..ModesSettings::default()
        };
        save_modes_settings(&path, block(0)).unwrap();
        let whole_len = std::fs::read(&path).unwrap().len();

        let done = AtomicBool::new(false);
        let read_until_done = || {
            let (mut reads, mut bad) = (0u32, Vec::new());
            while !done.load(Ordering::Relaxed) {
                let bytes = std::fs::read(&path).unwrap_or_default();
                // Parsing every read would leave little time to catch
                // a write in the act; the length is checked always,
                // the contents now and then.
                let parses = reads % 25 != 0
                    || serde_json::from_slice::<SettingsFile>(&bytes)
                        .ok()
                        .and_then(|f| f.modes)
                        .and_then(|v| serde_json::from_value::<ModesSettings>(v).ok())
                        .is_some_and(|m| m.custom.len() == MODES_MAX);
                if bytes.len() != whole_len || !parses {
                    bad.push(bytes.len());
                }
                reads += 1;
            }
            (reads, bad)
        };
        let (reads, bad) = std::thread::scope(|s| {
            let readers: Vec<_> = (0..3).map(|_| s.spawn(read_until_done)).collect();
            for round in 1..=150 {
                save_modes_settings(&path, block(round)).unwrap();
            }
            done.store(true, Ordering::Relaxed);
            readers.into_iter().map(|r| r.join().unwrap()).fold((0, Vec::new()), |mut all, (reads, bad)| {
                all.0 += reads;
                all.1.extend(bad);
                all
            })
        });
        assert!(reads > 0);
        assert!(
            bad.is_empty(),
            "{} of {reads} reads saw a broken file (sizes {:?}, whole is {whole_len})",
            bad.len(),
            &bad[..bad.len().min(5)]
        );
        set_modes_settings_cache(ModesSettings::default());
    }

    #[test]
    fn a_malformed_modes_block_reads_as_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"modes":{"custom":"not a list"},"stats":{"user_wpm":70}}"#).unwrap();
        let file = read_file(&path);
        // The sibling block survives the bad one.
        assert_eq!(file.stats.unwrap().user_wpm, 70);
        let modes = file
            .modes
            .and_then(|v| serde_json::from_value::<ModesSettings>(v).ok())
            .unwrap_or_default()
            .cleaned();
        assert_eq!(modes, ModesSettings::default());
    }
}
