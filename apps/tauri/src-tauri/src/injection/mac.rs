//! macOS Cmd+V via CGEventPost. Direct port of
//! `apps/macos/App/TextInjector.swift::simulatePaste`.
//!
//! Posts to `AnnotatedSession` (the Swift `cgAnnotatedSessionEventTap`
//! constant) so the synthetic event runs through the same per-session tap
//! chain a real keystroke would. AX grant required — already prompted by
//! `hotkey::install`, so paste piggybacks on that grant.

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

/// kVK_ANSI_V from Carbon/HIToolbox. Inlined so we don't pull in Carbon for
/// a single constant.
const VK_V: u16 = 0x09;

pub fn synthesize_paste() {
    let source = match CGEventSource::new(CGEventSourceStateID::CombinedSessionState) {
        Ok(s) => s,
        Err(()) => {
            eprintln!("inject: CGEventSource creation failed");
            return;
        }
    };

    let down = match CGEvent::new_keyboard_event(source.clone(), VK_V, true) {
        Ok(e) => e,
        Err(()) => {
            eprintln!("inject: keyDown event creation failed");
            return;
        }
    };
    let up = match CGEvent::new_keyboard_event(source, VK_V, false) {
        Ok(e) => e,
        Err(()) => {
            eprintln!("inject: keyUp event creation failed");
            return;
        }
    };

    down.set_flags(CGEventFlags::CGEventFlagCommand);
    up.set_flags(CGEventFlags::CGEventFlagCommand);

    down.post(CGEventTapLocation::AnnotatedSession);
    up.post(CGEventTapLocation::AnnotatedSession);
}
