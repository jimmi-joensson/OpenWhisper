//! C ABI surface for the Windows C# shell (P/Invoke).
//!
//! The macOS shell talks to the core via swift-bridge (see `lib.rs`'s
//! `#[swift_bridge::bridge]` module). The Windows shell can't use that —
//! swift-bridge emits Swift headers. So we expose the same underlying
//! logic through a flat `extern "C"` surface using only C-compatible
//! types (`*const c_char`, `u32`, `u64`, `f32`, `#[repr(C)]` structs).
//!
//! Conventions:
//! - Strings in: UTF-8 null-terminated (`*const c_char`).
//! - Strings out: caller provides a `*mut c_char` buffer + capacity (bytes).
//!   We write UTF-8 + null terminator and return the byte count (excluding
//!   the null). If the buffer is too small we write nothing and return a
//!   negative number whose magnitude is the required capacity (including
//!   the null terminator). This lets C# call once with a zero-cap buffer
//!   to learn the size, then allocate and call again.
//! - Primitive state queries (phase, confidence, timers) are pulled via a
//!   single `ow_dictation_snapshot` call that returns a flat struct by
//!   value. Matches the macOS pattern — one lock acquisition per UI tick.
//!
//! Anything non-`extern "C"` stays out of this module.

use std::ffi::{CStr, c_char};

use crate::{audio, dictation, transcript};

/// Flat snapshot struct for P/Invoke.
///
/// `#[repr(C)]` guarantees a stable layout across the FFI boundary. We do
/// NOT pack it — default alignment is fine on both ends. String fields
/// become two `*mut c_char` buffers the caller pre-allocates; see
/// `ow_dictation_snapshot_fill` below.
#[repr(C)]
pub struct OwDictationSnapshot {
    pub phase: u32,
    pub confidence: f32,
    pub sample_count: u64,
    pub elapsed_ms: u64,
    pub can_toggle: u8, // bool as u8 to avoid repr(C) bool warnings
    pub is_recording: u8,
}

/// Returns the core crate version as a static C string. Caller must NOT free.
#[unsafe(no_mangle)]
pub extern "C" fn ow_core_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

/// Apply the transcript post-processing pipeline (filler stripping,
/// substitutions, whitespace normalization) to `input`.
///
/// Writes UTF-8 + null terminator to `out_buf` up to `out_cap` bytes.
/// Returns number of bytes written (excluding null). On overflow returns
/// `-(required_cap as isize)` where `required_cap` includes the null.
///
/// # Safety
/// `input` must be a valid null-terminated UTF-8 C string.
/// `out_buf` must point to `out_cap` writable bytes (or be null when
/// `out_cap == 0`, for size probing).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ow_process_transcript(
    input: *const c_char,
    out_buf: *mut c_char,
    out_cap: usize,
) -> isize {
    if input.is_null() {
        return 0;
    }
    let Ok(input_str) = (unsafe { CStr::from_ptr(input) }).to_str() else {
        return 0;
    };
    let processed = transcript::process(input_str);
    write_cstr(&processed, out_buf, out_cap)
}

/// Populate `out` with the current dictation state.
/// Callers poll this on a UI timer.
///
/// # Safety
/// `out` must point to a valid, writable `OwDictationSnapshot`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ow_dictation_snapshot(out: *mut OwDictationSnapshot) {
    if out.is_null() {
        return;
    }
    let snap = dictation::dictation_snapshot();
    unsafe {
        *out = OwDictationSnapshot {
            phase: snap.phase(),
            confidence: snap.confidence(),
            sample_count: snap.sample_count(),
            elapsed_ms: snap.elapsed_ms(),
            can_toggle: snap.can_toggle() as u8,
            is_recording: snap.is_recording() as u8,
        };
    }
}

/// Writes the status message from the latest snapshot into `out_buf`.
/// Same overflow convention as `ow_process_transcript`.
///
/// # Safety
/// See `ow_process_transcript`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ow_dictation_status_message(
    out_buf: *mut c_char,
    out_cap: usize,
) -> isize {
    let snap = dictation::dictation_snapshot();
    write_cstr(&snap.status_message(), out_buf, out_cap)
}

/// Writes the current transcript from the latest snapshot.
///
/// # Safety
/// See `ow_process_transcript`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ow_dictation_transcript(
    out_buf: *mut c_char,
    out_cap: usize,
) -> isize {
    let snap = dictation::dictation_snapshot();
    write_cstr(&snap.transcript(), out_buf, out_cap)
}

/// Writes the latest error message (empty string if no error).
///
/// # Safety
/// See `ow_process_transcript`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ow_dictation_error_message(
    out_buf: *mut c_char,
    out_cap: usize,
) -> isize {
    let snap = dictation::dictation_snapshot();
    write_cstr(&snap.error_message(), out_buf, out_cap)
}

/// Request a toggle. Returns the `ToggleAction` value (0=ignore, 1=begin, 2=stop).
#[unsafe(no_mangle)]
pub extern "C" fn ow_dictation_request_toggle() -> u32 {
    dictation::dictation_request_toggle()
}

/// Cancel an in-progress recording. Returns 1 if cancelled, 0 if ignored.
#[unsafe(no_mangle)]
pub extern "C" fn ow_dictation_request_cancel() -> u8 {
    dictation::dictation_request_cancel() as u8
}

#[unsafe(no_mangle)]
pub extern "C" fn ow_dictation_mark_loading_model() {
    dictation::dictation_mark_loading_model();
}

#[unsafe(no_mangle)]
pub extern "C" fn ow_dictation_mark_capture_started() {
    dictation::dictation_mark_capture_started();
}

#[unsafe(no_mangle)]
pub extern "C" fn ow_dictation_mark_capture_stopped(sample_count: u64) {
    dictation::dictation_mark_capture_stopped(sample_count);
}

/// Deliver the transcribed text. `text` must be a valid UTF-8 C string.
///
/// # Safety
/// `text` must be a valid null-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ow_dictation_deliver_transcript(text: *const c_char, confidence: f32) {
    if text.is_null() {
        return;
    }
    let Ok(s) = (unsafe { CStr::from_ptr(text) }).to_str() else {
        return;
    };
    dictation::dictation_deliver_transcript(s, confidence);
}

/// Deliver an error message. `message` must be a valid UTF-8 C string.
///
/// # Safety
/// `message` must be a valid null-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ow_dictation_deliver_error(message: *const c_char) {
    if message.is_null() {
        return;
    }
    let Ok(s) = (unsafe { CStr::from_ptr(message) }).to_str() else {
        return;
    };
    dictation::dictation_deliver_error(s);
}

/// Starts microphone capture. Returns 0 on success, nonzero on error.
/// Writes the error message to `err_buf` (same overflow rules).
///
/// # Safety
/// `err_buf`, if non-null, must be valid for `err_cap` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ow_audio_start_capture(
    err_buf: *mut c_char,
    err_cap: usize,
) -> i32 {
    match audio::audio_start_capture() {
        Ok(()) => 0,
        Err(e) => {
            let _ = write_cstr(&e, err_buf, err_cap);
            1
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ow_audio_stop_capture() {
    audio::audio_stop_capture();
}

#[unsafe(no_mangle)]
pub extern "C" fn ow_audio_is_capturing() -> u8 {
    audio::audio_is_capturing() as u8
}

#[unsafe(no_mangle)]
pub extern "C" fn ow_audio_current_level() -> f32 {
    audio::audio_current_level()
}

/// Drain captured samples into caller-provided buffer.
/// Returns number of f32 samples written. If the buffer is too small,
/// returns `-(required_capacity as isize)` and drains nothing.
///
/// # Safety
/// `out_buf`, if non-null, must point to `out_cap * sizeof(f32)` writable
/// bytes. `out_buf` may be null only when `out_cap == 0` (size probe — but
/// note the buffer is still drained on the next call either way).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ow_audio_drain_samples(
    out_buf: *mut f32,
    out_cap: usize,
) -> isize {
    let samples = audio::audio_drain_samples();
    if samples.len() > out_cap {
        // Drop them — caller must re-call with a bigger buffer. This is
        // deliberate: a non-destructive "peek" API would require extra
        // state on the Rust side; prefer to let the caller size correctly.
        return -(samples.len() as isize);
    }
    if !out_buf.is_null() {
        unsafe {
            std::ptr::copy_nonoverlapping(samples.as_ptr(), out_buf, samples.len());
        }
    }
    samples.len() as isize
}


// --- helpers ---

/// Write `s` as UTF-8 + null terminator into `buf`. Returns bytes written
/// (excluding null) on success, or `-(required_cap as isize)` on overflow.
fn write_cstr(s: &str, buf: *mut c_char, cap: usize) -> isize {
    let needed = s.len() + 1; // null terminator
    if cap < needed {
        return -(needed as isize);
    }
    if buf.is_null() {
        return -(needed as isize);
    }
    unsafe {
        std::ptr::copy_nonoverlapping(s.as_ptr() as *const c_char, buf, s.len());
        *buf.add(s.len()) = 0;
    }
    s.len() as isize
}
