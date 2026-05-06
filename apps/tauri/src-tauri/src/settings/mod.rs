//! Persistent user settings — currently the global toggle + cancel hotkey
//! descriptors.
//!
//! Stored as JSON at `<app_config_dir>/settings.json`. Hand-rolled (no
//! tauri-plugin-store) — small surface, single read at boot.
//!
//! Two-slot model: `toggle` (start/stop dictation) and `cancel` (discard
//! audio while recording). Both rebindable; defaults Right ⌘ / Esc on
//! mac, Ctrl+Space / Esc on Windows.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

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
    // ModifierTap is only used for the Mac default (RightCommand). The
    // Windows hotkey backend doesn't accept it, so this constructor is
    // dead on the Windows build — silence the warning rather than
    // sprinkle cfg gates at every call site.
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

/// Slot identifier — used by `settings_set_hotkey` and the capture flow
/// to disambiguate which binding the user is editing.
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

/// On-disk schema. `hotkey` is the legacy single-slot field — kept for
/// migration on first load after upgrading. `hotkeys` is the current
/// shape; we always write that. `audio`, `behavior`, and `pill` are
/// sibling blocks that hold non-hotkey settings; absent on first run
/// and on upgrades from the hotkey-only schema.
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

/// Calibration values for the Home pane stats. Today: typing speed
/// (used by the Time Saved formula). Lives in JSON next to the other
/// settings blocks rather than in SQLite — settings are preferences
/// (different shape, different lifecycle), and a stats reset
/// shouldn't lose the user's WPM calibration.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatsSettings {
    /// User's typing speed in words per minute. Default 40 = average
    /// adult typist baseline. Clamped to [10, 300] on save — out-of-
    /// range writes are silently coerced (typing speed is a personal
    /// calibration, not a security boundary; clamp + helper text is
    /// friendlier than red error states).
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
        Self { follow_active_screen: true }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioSettings {
    /// Stable cpal `DeviceId` (Display form: "host:device_id"). `None`
    /// means "use the host default". Persisted as the cpal id rather than
    /// the device's friendly name so a saved selection rebinds across
    /// driver reinstalls and OS-level renames; we still fall back to the
    /// host default at `begin_capture` time if no enumerable device
    /// matches the saved id (mic unplugged, USB hub down, etc.).
    #[serde(default)]
    pub device_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BehaviorSettings {
    /// When true, OpenWhisper stays active over fullscreen apps — pill
    /// remains visible (best-effort), hotkey stays armed. Default false
    /// preserves the historical "step aside for games and video"
    /// behavior. Read by the fullscreen detector callback via the
    /// `behavior::SHOW_IN_FULLSCREEN` AtomicBool cache.
    #[serde(default)]
    pub show_in_fullscreen: bool,
    /// When true, OpenWhisper pauses other apps' audio playback on
    /// PHASE_RECORDING entry and resumes on exit. Default true: most
    /// users want auto-pause; the rare user dictating intentionally
    /// over background audio toggles off once. Read by the phase
    /// observer in `spawn_dictation_emitter` via the
    /// `behavior::PAUSE_AUDIO` AtomicBool cache.
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
    /// gotcha in the `openwhisper-platform-gotchas` skill for the full
    /// "why blind sleep, not deterministic detection" trail. Default
    /// 5000 — empirically tuned for AirPods Pro on Win11 26200; users
    /// on faster radios can dial down.
    #[serde(default = "default_bt_resume_delay_ms")]
    pub bt_resume_delay_ms: u64,
}

fn default_pause_audio_during_dictation() -> bool {
    true
}

pub fn default_bt_resume_delay_ms() -> u64 {
    5000
}

impl Default for BehaviorSettings {
    fn default() -> Self {
        Self {
            show_in_fullscreen: false,
            pause_audio_during_dictation: default_pause_audio_during_dictation(),
            bt_resume_delay_ms: default_bt_resume_delay_ms(),
        }
    }
}

static CURRENT: Mutex<Option<HotkeySettings>> = Mutex::new(None);
static AUDIO_CURRENT: Mutex<Option<AudioSettings>> = Mutex::new(None);
static BEHAVIOR_CURRENT: Mutex<Option<BehaviorSettings>> = Mutex::new(None);
static STATS_CURRENT: Mutex<Option<StatsSettings>> = Mutex::new(None);
/// Process-global mirror of `pill.follow_active_screen`. The watcher
/// thread reads this lock-free on every 500 ms tick — a `Mutex` would
/// be fine but the access pattern (read-mostly, write-on-toggle) makes
/// `AtomicBool` the right primitive. Defaults to `true` so a fresh
/// checkout (no settings.json) gets follow-on behavior.
static FOLLOW_ACTIVE_SCREEN: AtomicBool = AtomicBool::new(true);

pub fn follow_active_screen() -> bool {
    FOLLOW_ACTIVE_SCREEN.load(Ordering::Relaxed)
}

pub fn current_pill_settings() -> PillSettings {
    PillSettings { follow_active_screen: follow_active_screen() }
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("app_config_dir: {e}"))?;
    Ok(dir.join("settings.json"))
}

fn read_file(app: &AppHandle) -> SettingsFile {
    let Ok(path) = settings_path(app) else {
        return SettingsFile::default();
    };
    let Ok(bytes) = fs::read(&path) else {
        return SettingsFile::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn write_file(app: &AppHandle, file: &SettingsFile) -> Result<(), String> {
    let path = settings_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create config dir: {e}"))?;
    }
    let bytes = serde_json::to_vec_pretty(file).map_err(|e| format!("serialize: {e}"))?;
    fs::write(&path, bytes).map_err(|e| format!("write settings.json: {e}"))?;
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

/// Load saved hotkeys (with migration from the legacy single-slot field)
/// or fall back to platform defaults. Caches the result for backends.
/// Side-effect: hydrates `FOLLOW_ACTIVE_SCREEN` from the same on-disk
/// read, so the pill watcher sees the persisted value on its first
/// poll tick after boot.
pub fn load_settings(app: &AppHandle) -> HotkeySettings {
    if let Some(s) = CURRENT.lock().ok().and_then(|g| g.clone()) {
        return s;
    }
    let file = read_file(app);
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

fn save_settings(app: &AppHandle, settings: HotkeySettings) -> Result<(), String> {
    let audio = current_audio_settings();
    let behavior = current_behavior_settings();
    let stats = current_stats_settings();
    let file = SettingsFile {
        hotkey: None,
        hotkeys: Some(settings.clone()),
        audio,
        behavior,
        pill: Some(current_pill_settings()),
        stats,
    };
    write_file(app, &file)?;
    if let Ok(mut g) = CURRENT.lock() {
        *g = Some(settings);
    }
    Ok(())
}

pub fn current_stats_settings() -> Option<StatsSettings> {
    STATS_CURRENT.lock().ok().and_then(|g| *g)
}

/// Load the stats settings block from disk on first call, cache thereafter.
/// Missing field → default (40 wpm). Mirrors `load_audio_settings`.
pub fn load_stats_settings(app: &AppHandle) -> StatsSettings {
    if let Some(s) = STATS_CURRENT.lock().ok().and_then(|g| *g) {
        return s;
    }
    let file = read_file(app);
    let settings = file.stats.unwrap_or_default().clamped();
    if let Ok(mut g) = STATS_CURRENT.lock() {
        *g = Some(settings);
    }
    settings
}

fn save_stats_settings(app: &AppHandle, settings: StatsSettings) -> Result<(), String> {
    let clamped = settings.clamped();
    let file = read_file(app);
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
    write_file(app, &merged)?;
    if let Ok(mut g) = STATS_CURRENT.lock() {
        *g = Some(clamped);
    }
    Ok(())
}

pub fn current_audio_settings() -> Option<AudioSettings> {
    AUDIO_CURRENT.lock().ok().and_then(|g| g.clone())
}

/// Load the audio block from disk on the first call, then cache. Returns
/// the default (no device selected) if the file is absent or the audio
/// block is missing — matches the migration path for a user upgrading
/// from a hotkey-only `settings.json`.
pub fn load_audio_settings(app: &AppHandle) -> AudioSettings {
    if let Some(s) = AUDIO_CURRENT.lock().ok().and_then(|g| g.clone()) {
        return s;
    }
    let file = read_file(app);
    let settings = file.audio.unwrap_or_default();
    if let Ok(mut g) = AUDIO_CURRENT.lock() {
        *g = Some(settings.clone());
    }
    settings
}

fn save_audio_settings(app: &AppHandle, settings: AudioSettings) -> Result<(), String> {
    // Re-read the on-disk file so we don't clobber a sibling block that
    // is newer than our cache. Hotkeys / behavior / pill may all have
    // been written by the user since boot.
    let file = read_file(app);
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
    write_file(app, &merged)?;
    if let Ok(mut g) = AUDIO_CURRENT.lock() {
        *g = Some(settings);
    }
    Ok(())
}

pub fn current_behavior_settings() -> Option<BehaviorSettings> {
    BEHAVIOR_CURRENT.lock().ok().and_then(|g| g.clone())
}

/// Load the behavior block from disk on the first call, then cache.
/// Returns the default (show_in_fullscreen=false) when the file is
/// absent or the behavior block is missing — same migration shape as
/// `load_audio_settings`.
pub fn load_behavior_settings(app: &AppHandle) -> BehaviorSettings {
    if let Some(s) = BEHAVIOR_CURRENT.lock().ok().and_then(|g| g.clone()) {
        return s;
    }
    let file = read_file(app);
    let settings = file.behavior.unwrap_or_default();
    if let Ok(mut g) = BEHAVIOR_CURRENT.lock() {
        *g = Some(settings.clone());
    }
    settings
}

pub fn save_behavior_settings(
    app: &AppHandle,
    settings: BehaviorSettings,
) -> Result<(), String> {
    // Re-read for the same reason as `save_audio_settings`: avoid
    // clobbering hotkey/audio/pill blocks that may be newer than our
    // cache.
    let file = read_file(app);
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
    write_file(app, &merged)?;
    if let Ok(mut g) = BEHAVIOR_CURRENT.lock() {
        *g = Some(settings);
    }
    Ok(())
}

fn update_slot(
    app: &AppHandle,
    target: HotkeyTarget,
    config: HotkeyConfig,
) -> Result<(), String> {
    let mut current = current_settings().unwrap_or_else(default_settings);
    match target {
        HotkeyTarget::Toggle => current.toggle = config,
        HotkeyTarget::Cancel => current.cancel = config,
    }
    save_settings(app, current)
}

#[tauri::command]
pub fn settings_get_hotkeys(app: AppHandle) -> HotkeySettings {
    load_settings(&app)
}

#[tauri::command]
pub fn settings_set_hotkey(
    app: AppHandle,
    target: HotkeyTarget,
    config: HotkeyConfig,
) -> Result<(), String> {
    update_slot(&app, target, config)?;
    crate::hotkey::install(&app);
    Ok(())
}

#[tauri::command]
pub fn settings_reset_hotkey(
    app: AppHandle,
    target: HotkeyTarget,
) -> Result<HotkeyConfig, String> {
    let cfg = match target {
        HotkeyTarget::Toggle => default_toggle_hotkey(),
        HotkeyTarget::Cancel => default_cancel_hotkey(),
    };
    update_slot(&app, target, cfg.clone())?;
    crate::hotkey::install(&app);
    Ok(cfg)
}

#[tauri::command]
pub fn settings_capture_hotkey_start(target: HotkeyTarget) {
    crate::hotkey::set_capture_active(true, Some(target));
}

#[tauri::command]
pub fn settings_capture_hotkey_cancel() {
    crate::hotkey::set_capture_active(false, None);
}

#[tauri::command]
pub fn audio_set_device(app: AppHandle, id: Option<String>) -> Result<(), String> {
    let normalized = id.and_then(|s| {
        let trimmed = s.trim().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    });
    let settings = AudioSettings { device_id: normalized.clone() };
    save_audio_settings(&app, settings)?;
    openwhisper_core::audio::audio_set_selected_device_id(normalized);
    Ok(())
}

fn save_pill_settings(app: &AppHandle, settings: PillSettings) -> Result<(), String> {
    // Re-read disk so a hotkey/audio/behavior change written between
    // boot and this call survives. Same shape as save_audio_settings.
    let file = read_file(app);
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
    write_file(app, &merged)?;
    FOLLOW_ACTIVE_SCREEN.store(settings.follow_active_screen, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub fn settings_get_pill(_app: AppHandle) -> PillSettings {
    current_pill_settings()
}

#[tauri::command]
pub fn settings_set_pill_follow(app: AppHandle, follow: bool) -> Result<(), String> {
    save_pill_settings(&app, PillSettings { follow_active_screen: follow })
}

#[tauri::command]
pub fn settings_get_stats(app: AppHandle) -> StatsSettings {
    load_stats_settings(&app)
}

/// Persist a new typing-speed calibration. Out-of-range writes are
/// silently clamped to [USER_WPM_MIN, USER_WPM_MAX] inside
/// `save_stats_settings` — the React side mirrors this with helper
/// text so the UI doesn't surprise the user, but the backend is the
/// authoritative bound. Emits `settings_stats_changed` so the React
/// `useUserWpm` hook can refresh without a manual round-trip.
#[tauri::command]
pub fn settings_set_user_wpm(app: AppHandle, wpm: u32) -> Result<u32, String> {
    save_stats_settings(&app, StatsSettings { user_wpm: wpm })?;
    let stored = current_stats_settings().unwrap_or_default().user_wpm;
    let _ = app.emit("settings_stats_changed", stored);
    Ok(stored)
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
        // In-range values are passed through untouched.
        assert_eq!(StatsSettings { user_wpm: 60 }.clamped().user_wpm, 60);
    }

    #[test]
    fn stats_legacy_json_without_user_wpm_defaults_to_40() {
        // settings.json from a build before TASK-88.3 lacks a `stats`
        // block. Round-trip through the SettingsFile envelope, then
        // pull the stats default; it must be 40 wpm.
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
        // settings.json from a build that predates pause_audio_during_dictation
        // must round-trip through serde with the new field defaulting to true.
        let legacy = r#"{"show_in_fullscreen":true}"#;
        let parsed: BehaviorSettings = serde_json::from_str(legacy).unwrap();
        assert!(parsed.show_in_fullscreen);
        assert!(parsed.pause_audio_during_dictation);
        assert_eq!(parsed.bt_resume_delay_ms, 5000);
    }

    #[test]
    fn behavior_legacy_json_without_bt_resume_delay_defaults_5000() {
        // settings.json from a build that has pause_audio_during_dictation
        // but predates bt_resume_delay_ms must round-trip with the new
        // field at its default.
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
