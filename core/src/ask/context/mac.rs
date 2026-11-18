//! macOS facts for the device context.
//!
//! Everything here is a local read: `sysctl`, the I/O registry, the
//! power source list, the display list, the routing table and the
//! user's preferences. No permission is needed, nothing asks another
//! process, and nothing touches the network. A call that fails gives
//! `None` and the fact is left out.
//!
//! The functions are declared here instead of pulled in through more
//! crates: a handful of C calls with stable signatures.

use std::ffi::{CStr, c_char, c_int, c_void};
use std::ptr;

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFAllocatorRef, CFRelease, CFType, CFTypeRef, TCFType, kCFAllocatorDefault};
use core_foundation::boolean::CFBoolean;
use core_foundation::data::CFData;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use core_foundation::url::{CFURL, CFURLRef};

use super::{LocaleFacts, Power};

type CFLocaleRef = *const c_void;

unsafe extern "C" {
    fn sysctlbyname(
        name: *const c_char,
        oldp: *mut c_void,
        oldlenp: *mut usize,
        newp: *mut c_void,
        newlen: usize,
    ) -> c_int;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFLocaleCountryCode: CFStringRef;
    static kCFLocaleLanguageCode: CFStringRef;
    static kCFLocaleUsesMetricSystem: CFStringRef;
    static kCFLocaleCurrencyCode: CFStringRef;

    fn CFLocaleCopyCurrent() -> CFLocaleRef;
    fn CFLocaleCreate(allocator: CFAllocatorRef, identifier: CFStringRef) -> CFLocaleRef;
    fn CFLocaleCopyPreferredLanguages() -> CFArrayRef;
    fn CFLocaleGetValue(locale: CFLocaleRef, key: CFStringRef) -> CFTypeRef;
    fn CFLocaleCopyDisplayNameForPropertyValue(
        display_locale: CFLocaleRef,
        key: CFStringRef,
        value: CFStringRef,
    ) -> CFStringRef;
    fn CFDateFormatterCreateDateFormatFromTemplate(
        allocator: CFAllocatorRef,
        template: CFStringRef,
        options: usize,
        locale: CFLocaleRef,
    ) -> CFStringRef;
    fn CFPreferencesCopyAppValue(key: CFStringRef, application_id: CFStringRef) -> CFTypeRef;

    static kCFURLVolumeAvailableCapacityForImportantUsageKey: CFStringRef;
    static kCFURLVolumeAvailableCapacityKey: CFStringRef;
    fn CFURLCopyResourcePropertyForKey(
        url: CFURLRef,
        key: CFStringRef,
        value: *mut CFTypeRef,
        error: *mut c_void,
    ) -> u8;
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IORegistryEntryFromPath(main_port: u32, path: *const c_char) -> u32;
    fn IORegistryEntryCreateCFProperty(
        entry: u32,
        key: CFStringRef,
        allocator: CFAllocatorRef,
        options: u32,
    ) -> CFTypeRef;
    fn IOObjectRelease(object: u32) -> c_int;
    fn IOPSCopyPowerSourcesInfo() -> CFTypeRef;
    fn IOPSCopyPowerSourcesList(blob: CFTypeRef) -> CFArrayRef;
    fn IOPSGetPowerSourceDescription(blob: CFTypeRef, source: CFTypeRef) -> CFDictionaryRef;
    fn IOPSGetProvidingPowerSourceType(blob: CFTypeRef) -> CFStringRef;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGGetActiveDisplayList(max_displays: u32, displays: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayPixelsWide(display: u32) -> usize;
    fn CGDisplayPixelsHigh(display: u32) -> usize;
}

#[link(name = "SystemConfiguration", kind = "framework")]
unsafe extern "C" {
    fn SCNetworkReachabilityCreateWithAddress(allocator: CFAllocatorRef, address: *const SockAddrIn) -> CFTypeRef;
    fn SCNetworkReachabilityGetFlags(target: CFTypeRef, flags: *mut u32) -> u8;
}

/// `struct sockaddr_in` as Darwin lays it out.
#[repr(C)]
struct SockAddrIn {
    len: u8,
    family: u8,
    port: u16,
    addr: u32,
    zero: [u8; 8],
}

/// A string `sysctl` value, e.g. `hw.model`.
fn sysctl_string(name: &CStr) -> Option<String> {
    let mut len = 0usize;
    // SAFETY: a null buffer with a length pointer asks for the size.
    if unsafe { sysctlbyname(name.as_ptr(), ptr::null_mut(), &mut len, ptr::null_mut(), 0) } != 0 || len == 0 {
        return None;
    }
    let mut buf = vec![0u8; len + 1];
    // SAFETY: `buf` holds `len` bytes and one more, which stays NUL.
    if unsafe { sysctlbyname(name.as_ptr(), buf.as_mut_ptr().cast(), &mut len, ptr::null_mut(), 0) } != 0 {
        return None;
    }
    let text = CStr::from_bytes_until_nul(&buf).ok()?.to_str().ok()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// The machine's model identifier (`hw.model`, "Mac14,9").
pub(super) fn model_id() -> Option<String> {
    sysctl_string(c"hw.model")
}

/// The name the machine is sold under ("MacBook Pro (14-inch, 2021)").
/// Apple silicon keeps it in the device tree; an Intel Mac has no such
/// entry and gets `None`.
pub(super) fn model_name() -> Option<String> {
    // SAFETY: a NUL-terminated path; 0 is the default main port. A
    // missing entry comes back as 0.
    let entry = unsafe { IORegistryEntryFromPath(0, c"IODeviceTree:/product".as_ptr()) };
    if entry == 0 {
        return None;
    }
    let key = CFString::from_static_string("product-name");
    // SAFETY: `entry` is a live registry entry; the result follows
    // the create rule and may be null.
    let value = unsafe { IORegistryEntryCreateCFProperty(entry, key.as_concrete_TypeRef(), kCFAllocatorDefault, 0) };
    // SAFETY: `entry` came from IORegistryEntryFromPath above.
    unsafe { IOObjectRelease(entry) };
    if value.is_null() {
        return None;
    }
    // SAFETY: non-null, owned by us (create rule).
    let data = unsafe { CFType::wrap_under_create_rule(value) }.downcast_into::<CFData>()?;
    let bytes = data.bytes();
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    let name = std::str::from_utf8(&bytes[..end]).ok()?.trim().to_string();
    (!name.is_empty()).then_some(name)
}

/// What the machine runs on right now.
pub(super) fn power() -> Option<Power> {
    // SAFETY: no arguments; the result follows the create rule.
    let blob = unsafe { IOPSCopyPowerSourcesInfo() };
    if blob.is_null() {
        return None;
    }
    // SAFETY: `blob` is live until the release at the end. The type
    // string follows the get rule; the list follows the create rule.
    let power = unsafe {
        let providing = IOPSGetProvidingPowerSourceType(blob);
        let on_battery =
            !providing.is_null() && CFString::wrap_under_get_rule(providing).to_string() == "Battery Power";
        let list = IOPSCopyPowerSourcesList(blob);
        let mut charge = None;
        if !list.is_null() {
            let sources = CFArray::<CFType>::wrap_under_create_rule(list);
            for source in sources.iter() {
                let description = IOPSGetPowerSourceDescription(blob, source.as_CFTypeRef());
                if description.is_null() {
                    continue;
                }
                let description = CFDictionary::<CFString, CFType>::wrap_under_get_rule(description);
                let text = |key: &'static str| {
                    description
                        .find(CFString::from_static_string(key))
                        .and_then(|v| v.downcast::<CFString>())
                        .map(|s| s.to_string())
                };
                let number = |key: &'static str| {
                    description
                        .find(CFString::from_static_string(key))
                        .and_then(|v| v.downcast::<CFNumber>())
                        .and_then(|n| n.to_i64())
                };
                if text("Type").as_deref() != Some("InternalBattery") {
                    continue;
                }
                if let (Some(now), Some(max)) = (number("Current Capacity"), number("Max Capacity"))
                    && max > 0
                {
                    charge = Some((now * 100 / max).clamp(0, 100) as u8);
                }
            }
        }
        Power { on_battery, charge }
    };
    // SAFETY: `blob` follows the create rule and is not used again.
    unsafe { CFRelease(blob) };
    Some(power)
}

/// Width and height of every active display, in the pixels of its
/// current mode.
pub(super) fn displays() -> Vec<(u32, u32)> {
    let mut ids = [0u32; 16];
    let mut count = 0u32;
    // SAFETY: `ids` has room for the 16 displays asked for.
    if unsafe { CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) } != 0 {
        return Vec::new();
    }
    ids.iter()
        .take(count as usize)
        // SAFETY: ids the call above just handed out.
        .map(|id| unsafe { (CGDisplayPixelsWide(*id) as u32, CGDisplayPixelsHigh(*id) as u32) })
        .filter(|(w, h)| *w > 0 && *h > 0)
        .collect()
}

/// Free space on the system volume, in bytes: the figure the Finder
/// shows (space the system can free up counts), or the plain free
/// space where that is not on offer.
pub(super) fn disk_free() -> Option<u64> {
    let root = CFURL::from_path("/", true)?;
    // SAFETY: the keys are constants of CoreFoundation.
    let keys = unsafe { [kCFURLVolumeAvailableCapacityForImportantUsageKey, kCFURLVolumeAvailableCapacityKey] };
    for key in keys {
        let mut value: CFTypeRef = ptr::null();
        // SAFETY: `root` is a live URL and `value` a valid out
        // pointer; what it receives follows the create rule.
        let read =
            unsafe { CFURLCopyResourcePropertyForKey(root.as_concrete_TypeRef(), key, &mut value, ptr::null_mut()) };
        if read == 0 || value.is_null() {
            continue;
        }
        // SAFETY: non-null, owned by us.
        let bytes = unsafe { CFType::wrap_under_create_rule(value) }.downcast_into::<CFNumber>().and_then(|n| n.to_i64());
        if let Some(bytes) = bytes.filter(|b| *b > 0) {
            return Some(bytes as u64);
        }
    }
    None
}

/// Whether the machine has a route to the internet. Read off the
/// routing table (reachability of the zero address): no packet is
/// sent and no name is looked up.
pub(super) fn online() -> Option<bool> {
    const AF_INET: u8 = 2;
    const REACHABLE: u32 = 1 << 1;
    const CONNECTION_REQUIRED: u32 = 1 << 2;
    let address = SockAddrIn { len: size_of::<SockAddrIn>() as u8, family: AF_INET, port: 0, addr: 0, zero: [0; 8] };
    // SAFETY: a valid sockaddr_in; the result follows the create rule.
    let target = unsafe { SCNetworkReachabilityCreateWithAddress(kCFAllocatorDefault, &address) };
    if target.is_null() {
        return None;
    }
    let mut flags = 0u32;
    // SAFETY: `target` is live; `flags` is a valid out pointer.
    let known = unsafe { SCNetworkReachabilityGetFlags(target, &mut flags) } != 0;
    // SAFETY: created above, not used again.
    unsafe { CFRelease(target) };
    known.then_some(flags & REACHABLE != 0 && flags & CONNECTION_REQUIRED == 0)
}

/// A string value of `locale`.
///
/// # Safety
/// `locale` is a live CFLocale and `key` one of its string keys.
unsafe fn locale_string(locale: CFLocaleRef, key: CFStringRef) -> Option<String> {
    // SAFETY: per the contract; the value follows the get rule.
    let value = unsafe { CFLocaleGetValue(locale, key) };
    if value.is_null() {
        return None;
    }
    // SAFETY: non-null, retained by the wrapper.
    let text = unsafe { CFType::wrap_under_get_rule(value) }.downcast_into::<CFString>()?.to_string();
    (!text.is_empty()).then_some(text)
}

/// The English name of a language or region code ("da" is "Danish").
///
/// # Safety
/// `english` is a live CFLocale and `key` the key `code` belongs to.
unsafe fn display_name(english: CFLocaleRef, key: CFStringRef, code: &str) -> Option<String> {
    let code = CFString::new(code);
    // SAFETY: per the contract; the result follows the create rule.
    let name = unsafe { CFLocaleCopyDisplayNameForPropertyValue(english, key, code.as_concrete_TypeRef()) };
    if name.is_null() {
        return None;
    }
    // SAFETY: non-null, owned by us.
    let name = unsafe { CFString::wrap_under_create_rule(name) }.to_string();
    (!name.is_empty()).then_some(name)
}

/// "Danish (da)", or the code alone when it has no English name.
fn named(name: Option<String>, code: &str) -> String {
    match name {
        Some(name) if !name.eq_ignore_ascii_case(code) => format!("{name} ({code})"),
        _ => code.to_string(),
    }
}

/// Language, region, units, clock, currency and keyboard layout, from
/// the user's settings.
pub(super) fn locale() -> LocaleFacts {
    let mut facts = LocaleFacts::default();
    // SAFETY: every reference made here is released before the block
    // ends; values read with the get rule are copied out at once.
    unsafe {
        let locale = CFLocaleCopyCurrent();
        if locale.is_null() {
            return facts;
        }
        let english = CFLocaleCreate(kCFAllocatorDefault, CFString::from_static_string("en_US").as_concrete_TypeRef());

        // The language the system is shown in is the first of the
        // preferred languages ("en-DK"); the locale's own language
        // code follows the app's translations instead.
        let preferred = CFLocaleCopyPreferredLanguages();
        let mut language = None;
        if !preferred.is_null() {
            let preferred = CFArray::<CFString>::wrap_under_create_rule(preferred);
            language = preferred.get(0).map(|l| l.to_string());
        }
        let language = language
            .and_then(|tag| tag.split(['-', '_']).next().map(str::to_string))
            .filter(|code| !code.is_empty())
            .or_else(|| locale_string(locale, kCFLocaleLanguageCode));
        if let Some(code) = language {
            let name = if english.is_null() { None } else { display_name(english, kCFLocaleLanguageCode, &code) };
            facts.language = named(name, &code);
        }
        if let Some(code) = locale_string(locale, kCFLocaleCountryCode) {
            let name = if english.is_null() { None } else { display_name(english, kCFLocaleCountryCode, &code) };
            facts.region = named(name, &code);
        }
        let metric = CFLocaleGetValue(locale, kCFLocaleUsesMetricSystem);
        if !metric.is_null()
            && let Some(metric) = CFType::wrap_under_get_rule(metric).downcast_into::<CFBoolean>()
        {
            facts.units = if bool::from(metric) { "metric" } else { "imperial" }.to_string();
        }
        if let Some(code) = locale_string(locale, kCFLocaleCurrencyCode) {
            facts.currency = code;
        }
        // The hour pattern the locale uses: one with an am/pm marker
        // ("a") is a 12-hour clock.
        let pattern = CFDateFormatterCreateDateFormatFromTemplate(
            kCFAllocatorDefault,
            CFString::from_static_string("j").as_concrete_TypeRef(),
            0,
            locale,
        );
        if !pattern.is_null() {
            let pattern = CFString::wrap_under_create_rule(pattern).to_string();
            facts.clock = if pattern.contains('a') { "12-hour" } else { "24-hour" }.to_string();
        }
        if !english.is_null() {
            CFRelease(english);
        }
        CFRelease(locale);
    }
    facts.keyboard = keyboard_layout().unwrap_or_default();
    facts
}

/// The keyboard layout in use ("Danish"), from the input source
/// preferences. Read from the preferences and not from the Text Input
/// Source API, which may only be called on the main thread.
fn keyboard_layout() -> Option<String> {
    let key = CFString::from_static_string("AppleCurrentKeyboardLayoutInputSourceID");
    let app = CFString::from_static_string("com.apple.HIToolbox");
    // SAFETY: two live strings; the result follows the create rule
    // and may be null.
    let value = unsafe { CFPreferencesCopyAppValue(key.as_concrete_TypeRef(), app.as_concrete_TypeRef()) };
    if value.is_null() {
        return None;
    }
    // SAFETY: non-null, owned by us.
    let id = unsafe { CFType::wrap_under_create_rule(value) }.downcast_into::<CFString>()?.to_string();
    layout_name(&id)
}

/// `com.apple.keylayout.Danish` is "Danish", and
/// `com.apple.keylayout.US-International` is "US International".
fn layout_name(id: &str) -> Option<String> {
    let name = id.rsplit('.').next().unwrap_or_default().replace(['-', '_'], " ");
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_ids_become_names() {
        assert_eq!(layout_name("com.apple.keylayout.Danish").as_deref(), Some("Danish"));
        assert_eq!(layout_name("com.apple.keylayout.US-International").as_deref(), Some("US International"));
        assert_eq!(layout_name("ABC").as_deref(), Some("ABC"));
        assert_eq!(layout_name(""), None);
    }

    #[test]
    fn codes_get_their_english_name() {
        assert_eq!(named(Some("Danish".into()), "da"), "Danish (da)");
        assert_eq!(named(None, "da"), "da");
        assert_eq!(named(Some("DA".into()), "da"), "da");
    }

    #[test]
    fn the_reads_do_not_fail_on_this_machine() {
        // Whatever the machine is, the calls come back and what they
        // return is well formed.
        let model = model_id().expect("hw.model is on every Mac");
        assert!(!model.contains('\n') && !model.is_empty());
        if let Some(name) = model_name() {
            assert!(!name.is_empty());
        }
        if let Some(power) = power() {
            assert!(power.charge.is_none_or(|c| c <= 100));
        }
        for (w, h) in displays() {
            assert!(w > 0 && h > 0);
        }
        let _ = online();
        assert!(disk_free().is_some_and(|free| free > 0));
        let facts = locale();
        assert!(facts.units.is_empty() || facts.units == "metric" || facts.units == "imperial");
        assert!(facts.clock.is_empty() || facts.clock == "12-hour" || facts.clock == "24-hour");
    }
}
