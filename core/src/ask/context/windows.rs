//! Windows facts for the device context.
//!
//! Everything here is a local read: the BIOS keys of the registry,
//! the power status, the monitor list, the connection state and the
//! user's locale. No permission is needed and nothing touches the
//! network. A call that fails gives `None` and the fact is left out.

use windows::Win32::Foundation::{ERROR_SUCCESS, LPARAM, RECT};
use windows::Win32::Globalization::{
    GetLocaleInfoEx, GetUserDefaultLocaleName, LCIDToLocaleName, LOCALE_IMEASURE, LOCALE_SENGLISHCOUNTRYNAME,
    LOCALE_SENGLISHDISPLAYNAME, LOCALE_SENGLISHLANGUAGENAME, LOCALE_SINTLSYMBOL, LOCALE_SISO639LANGNAME,
    LOCALE_SISO3166CTRYNAME, LOCALE_STIMEFORMAT,
};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
use windows::Win32::Networking::WinInet::{INTERNET_CONNECTION, InternetGetConnectedState};
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout;
use windows::core::{BOOL, PCWSTR, w};

use super::{LocaleFacts, Power};

/// `LOCALE_NAME_MAX_LENGTH`.
const LOCALE_NAME_LEN: usize = 85;

/// The text in a NUL-terminated UTF-16 buffer.
fn wide_text(buf: &[u16]) -> Option<String> {
    let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    let text = String::from_utf16_lossy(&buf[..end]).trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// A string value under `HKLM\HARDWARE\DESCRIPTION\System\BIOS`.
fn bios_value(name: PCWSTR) -> Option<String> {
    let mut buf = [0u16; 256];
    let mut bytes = (buf.len() * 2) as u32;
    // SAFETY: `buf` holds `bytes` bytes; the key and value names are
    // NUL-terminated literals.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!("HARDWARE\\DESCRIPTION\\System\\BIOS"),
            name,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    wide_text(&buf)
}

/// Placeholders a board maker left in the BIOS instead of a name.
fn is_placeholder(value: &str) -> bool {
    let v = value.to_ascii_lowercase();
    v == "system manufacturer"
        || v == "system product name"
        || v == "default string"
        || v.starts_with("to be filled")
        || v == "not applicable"
}

/// Manufacturer and model of the system ("Contoso Book 14").
pub(super) fn model_name() -> Option<String> {
    let maker = bios_value(w!("SystemManufacturer")).filter(|v| !is_placeholder(v));
    let product = bios_value(w!("SystemProductName")).filter(|v| !is_placeholder(v));
    match (maker, product) {
        (Some(maker), Some(product)) if product.to_lowercase().starts_with(&maker.to_lowercase()) => Some(product),
        (Some(maker), Some(product)) => Some(format!("{maker} {product}")),
        (maker, product) => product.or(maker),
    }
}

/// Windows has no second identifier next to the name.
pub(super) fn model_id() -> Option<String> {
    None
}

/// What the machine runs on right now.
pub(super) fn power() -> Option<Power> {
    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: a valid out pointer.
    unsafe { GetSystemPowerStatus(&mut status) }.ok()?;
    // ACLineStatus: 0 offline, 1 online, 255 unknown.
    let on_battery = match status.ACLineStatus {
        0 => true,
        1 => false,
        _ => return None,
    };
    // BatteryFlag 128 = no system battery; BatteryLifePercent 255 =
    // unknown.
    let has_battery = status.BatteryFlag & 128 == 0 && status.BatteryFlag != 255;
    let charge = (has_battery && status.BatteryLifePercent <= 100).then_some(status.BatteryLifePercent);
    Some(Power { on_battery, charge })
}

unsafe extern "system" fn collect_monitor(monitor: HMONITOR, _hdc: HDC, _clip: *mut RECT, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the `Vec` handed to EnumDisplayMonitors below,
    // which outlives the enumeration.
    let sizes = unsafe { &mut *(data.0 as *mut Vec<(u32, u32)>) };
    let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..MONITORINFO::default() };
    // SAFETY: `monitor` is the handle the enumeration passed in.
    if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        let width = (info.rcMonitor.right - info.rcMonitor.left).max(0) as u32;
        let height = (info.rcMonitor.bottom - info.rcMonitor.top).max(0) as u32;
        if width > 0 && height > 0 {
            sizes.push((width, height));
        }
    }
    BOOL(1)
}

/// Width and height of every monitor, in the pixels this process
/// sees them in.
pub(super) fn displays() -> Vec<(u32, u32)> {
    let mut sizes: Vec<(u32, u32)> = Vec::new();
    // SAFETY: `sizes` outlives the call, and the callback is the only
    // user of the pointer while the enumeration runs.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect_monitor),
            LPARAM(&mut sizes as *mut Vec<(u32, u32)> as isize),
        );
    }
    sizes
}

/// Free space on the system drive, in bytes.
pub(super) fn disk_free() -> Option<u64> {
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    let root: Vec<u16> =
        format!("{}\\", drive.trim_end_matches('\\')).encode_utf16().chain(std::iter::once(0)).collect();
    let mut free = 0u64;
    // SAFETY: `root` is NUL-terminated and `free` a valid out pointer.
    unsafe { GetDiskFreeSpaceExW(PCWSTR(root.as_ptr()), Some(&mut free), None, None) }.ok()?;
    (free > 0).then_some(free)
}

/// Whether the machine has a connection to the internet, as Windows
/// itself judges it. No packet is sent.
pub(super) fn online() -> Option<bool> {
    let mut flags = INTERNET_CONNECTION::default();
    // SAFETY: a valid out pointer.
    Some(unsafe { InternetGetConnectedState(&mut flags, None) }.is_ok())
}

/// A string of the user's locale `name`.
fn locale_text(name: PCWSTR, what: u32) -> Option<String> {
    let mut buf = [0u16; 128];
    // SAFETY: `name` is NUL-terminated and `buf` is ours.
    let len = unsafe { GetLocaleInfoEx(name, what, Some(&mut buf)) };
    if len <= 0 {
        return None;
    }
    wide_text(&buf)
}

/// "Danish (da)", or whichever of the two there is.
fn named(name: Option<String>, code: Option<String>) -> String {
    match (name, code) {
        (Some(name), Some(code)) => format!("{name} ({code})"),
        (name, code) => name.or(code).unwrap_or_default(),
    }
}

/// Language, region, units, clock, currency and input language, from
/// the user's settings.
pub(super) fn locale() -> LocaleFacts {
    let mut facts = LocaleFacts::default();
    let mut name = [0u16; LOCALE_NAME_LEN];
    // SAFETY: a buffer of the documented maximum length.
    if unsafe { GetUserDefaultLocaleName(&mut name) } <= 0 {
        return facts;
    }
    let locale = PCWSTR(name.as_ptr());
    facts.language =
        named(locale_text(locale, LOCALE_SENGLISHLANGUAGENAME), locale_text(locale, LOCALE_SISO639LANGNAME));
    facts.region = named(locale_text(locale, LOCALE_SENGLISHCOUNTRYNAME), locale_text(locale, LOCALE_SISO3166CTRYNAME));
    // "0" metric, "1" U.S.
    facts.units = match locale_text(locale, LOCALE_IMEASURE).as_deref() {
        Some("0") => "metric".to_string(),
        Some("1") => "imperial".to_string(),
        _ => String::new(),
    };
    facts.currency = locale_text(locale, LOCALE_SINTLSYMBOL).unwrap_or_default();
    // The time pattern: "H" is the 24-hour hour, "h" the 12-hour one.
    if let Some(pattern) = locale_text(locale, LOCALE_STIMEFORMAT) {
        facts.clock = clock_of(&pattern).to_string();
    }
    facts.keyboard = input_language().unwrap_or_default();
    facts
}

/// Which clock a Windows time pattern ("HH:mm:ss", "h:mm tt") shows.
fn clock_of(pattern: &str) -> &'static str {
    if pattern.contains('H') {
        "24-hour"
    } else if pattern.contains('h') {
        "12-hour"
    } else {
        ""
    }
}

/// The input language of the keyboard layout in use ("Danish
/// (Denmark)").
fn input_language() -> Option<String> {
    // SAFETY: 0 asks for the calling thread's layout. The low word of
    // the handle is the language id.
    let layout = unsafe { GetKeyboardLayout(0) };
    let language = (layout.0 as usize & 0xFFFF) as u32;
    if language == 0 {
        return None;
    }
    let mut name = [0u16; LOCALE_NAME_LEN];
    // SAFETY: a buffer of the documented maximum length.
    if unsafe { LCIDToLocaleName(language, Some(&mut name), 0) } <= 0 {
        return None;
    }
    locale_text(PCWSTR(name.as_ptr()), LOCALE_SENGLISHDISPLAYNAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_not_model_names() {
        assert!(is_placeholder("System Product Name"));
        assert!(is_placeholder("To Be Filled By O.E.M."));
        assert!(!is_placeholder("Contoso Book 14"));
    }

    #[test]
    fn the_time_pattern_tells_the_clock() {
        assert_eq!(clock_of("HH:mm:ss"), "24-hour");
        assert_eq!(clock_of("h:mm:ss tt"), "12-hour");
        assert_eq!(clock_of(""), "");
    }
}
