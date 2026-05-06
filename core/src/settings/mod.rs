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
//!   default Right ⌘) or `Chord` (Win default Ctrl+Space).
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
//!
//! Migration: the legacy `hotkey` (single-slot) field is read on first
//!  load after upgrade and folded into the current `hotkeys` shape;
//! we always write the new shape on save. Sibling-block re-read on
//! every save protects against clobbering when multiple call sites
//! edit different blocks concurrently.
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
}

/// Slot identifier — used by the shell's `settings_set_hotkey` Tauri
/// command and the capture flow to disambiguate which binding the
/// user is editing.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HotkeyTarget {
    Toggle,
    Cancel,
}

impl HotkeyTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            HotkeyTarget::Toggle => "toggle",
            HotkeyTarget::Cancel => "cancel",
        }
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

pub fn default_settings() -> HotkeySettings {
    HotkeySettings {
        toggle: default_toggle_hotkey(),
        cancel: default_cancel_hotkey(),
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

// --- caches --------------------------------------------------------

static CURRENT: Mutex<Option<HotkeySettings>> = Mutex::new(None);
static AUDIO_CURRENT: Mutex<Option<AudioSettings>> = Mutex::new(None);
static BEHAVIOR_CURRENT: Mutex<Option<BehaviorSettings>> = Mutex::new(None);
static STATS_CURRENT: Mutex<Option<StatsSettings>> = Mutex::new(None);

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

fn write_file(path: &Path, file: &SettingsFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("create config dir: {e}"))?;
        }
    }
    let bytes = serde_json::to_vec_pretty(file).map_err(|e| format!("serialize: {e}"))?;
    fs::write(path, bytes).map_err(|e| format!("write settings.json: {e}"))?;
    Ok(())
}

fn merge_loaded(file: SettingsFile) -> HotkeySettings {
    if let Some(h) = file.hotkeys {
        return h;
    }
    HotkeySettings {
        toggle: file.hotkey.unwrap_or_else(default_toggle_hotkey),
        cancel: default_cancel_hotkey(),
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
    let file = read_file(path);
    let audio = file.audio.or_else(current_audio_settings);
    let behavior = file.behavior.or_else(current_behavior_settings);
    let stats = file.stats.or_else(current_stats_settings);
    let pill = file.pill.unwrap_or_else(current_pill_settings);
    let merged = SettingsFile {
        hotkey: None,
        hotkeys: Some(settings.clone()),
        audio,
        behavior,
        pill: Some(pill),
        stats,
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
    let file = read_file(path);
    let hotkeys = file.hotkeys.or_else(current_settings);
    let audio = file.audio.or_else(current_audio_settings);
    let behavior = file.behavior.or_else(current_behavior_settings);
    let pill = file.pill.unwrap_or_else(current_pill_settings);
    let merged = SettingsFile {
        hotkey: None,
        hotkeys,
        audio,
        behavior,
        pill: Some(pill),
        stats: Some(clamped),
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
    let file = read_file(path);
    let hotkeys = file.hotkeys.or_else(current_settings);
    let behavior = file.behavior.or_else(current_behavior_settings);
    let pill = file.pill.or_else(|| Some(current_pill_settings()));
    let stats = file.stats.or_else(current_stats_settings);
    let merged = SettingsFile {
        hotkey: None,
        hotkeys,
        audio: Some(settings.clone()),
        behavior,
        pill,
        stats,
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
    let file = read_file(path);
    let hotkeys = file.hotkeys.or_else(current_settings);
    let audio = file.audio.or_else(current_audio_settings);
    let pill = file.pill.or_else(|| Some(current_pill_settings()));
    let stats = file.stats.or_else(current_stats_settings);
    let merged = SettingsFile {
        hotkey: None,
        hotkeys,
        audio,
        behavior: Some(settings.clone()),
        pill,
        stats,
    };
    write_file(path, &merged)?;
    if let Ok(mut g) = BEHAVIOR_CURRENT.lock() {
        *g = Some(settings);
    }
    Ok(())
}

pub fn save_pill_settings(path: &Path, settings: PillSettings) -> Result<(), String> {
    let file = read_file(path);
    let hotkeys = file.hotkeys.or_else(current_settings);
    let audio = file.audio.or_else(current_audio_settings);
    let behavior = file.behavior.or_else(current_behavior_settings);
    let stats = file.stats.or_else(current_stats_settings);
    let merged = SettingsFile {
        hotkey: None,
        hotkeys,
        audio,
        behavior,
        pill: Some(settings.clone()),
        stats,
    };
    write_file(path, &merged)?;
    FOLLOW_ACTIVE_SCREEN.store(settings.follow_active_screen, Ordering::Relaxed);
    Ok(())
}

/// Mutate one slot of the hotkey block (toggle or cancel) and
/// persist. Used by the shell's `settings_set_hotkey` /
/// `settings_reset_hotkey` commands.
pub fn update_hotkey_slot(
    path: &Path,
    target: HotkeyTarget,
    config: HotkeyConfig,
) -> Result<(), String> {
    let mut current = current_settings().unwrap_or_else(default_settings);
    match target {
        HotkeyTarget::Toggle => current.toggle = config,
        HotkeyTarget::Cancel => current.cancel = config,
    }
    save_settings(path, current)
}

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
}
