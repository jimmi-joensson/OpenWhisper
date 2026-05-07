// C-ABI wrapper around FluidAudio for the Rust core's recognizer module.
//
// The Rust side calls these via extern "C" from
// core/src/recognizer/fluidaudio.rs. Mirror of
// archive/macos/App/DictationService.swift's load + transcribe flow without
// the AppKit / @MainActor wrappers — this is shell-agnostic.
//
// Memory contract:
//   - Strings returned by `fab_*` fns are heap-allocated via `strdup` and
//     MUST be freed by the caller using `fab_free_string`.
//   - `fab_last_error` returns a borrowed pointer into a thread-local
//     buffer — do not free, do not store across calls.
//
// Concurrency contract:
//   - `fab_load` and `fab_transcribe` block the calling thread until the
//     underlying async work completes (semaphore wait). Caller is
//     expected to be a worker thread, not the UI thread.
//   - The global `state` lock serializes calls — the Rust trait already
//     enforces single-threaded access via `&mut self`, so contention is
//     not expected in normal use.

import Foundation
import FluidAudio

// MARK: - Global state

// Plain class + lock instead of @MainActor: the Rust trait already
// serializes calls (`&mut self`), so we don't need actor isolation —
// just enough Sendable plumbing to keep Swift 6 happy.
private final class BridgeState: @unchecked Sendable {
    let lock = NSLock()
    var asr: AsrManager?
    var loaded = false
}

private let state = BridgeState()

// Per-thread last-error buffer. C-side reads via `fab_last_error`.
private final class LastError {
    static let key = "fluidaudiobridge.lasterror"

    static func set(_ message: String) {
        Thread.current.threadDictionary[key] = strdup(message)
    }

    static func get() -> UnsafePointer<CChar>? {
        guard let raw = Thread.current.threadDictionary[key] else { return nil }
        // strdup'd pointer stored as Any — bridge back via unsafeBitCast
        // is fragile; round-trip via NSValue keeps the pointer intact.
        if let ptr = raw as? OpaquePointer {
            return UnsafePointer<CChar>(ptr)
        }
        return nil
    }

    static func clear() {
        if let raw = Thread.current.threadDictionary[key], let ptr = raw as? OpaquePointer {
            free(UnsafeMutableRawPointer(ptr))
        }
        Thread.current.threadDictionary.removeObject(forKey: key)
    }
}

private func setError(_ message: String) {
    LastError.clear()
    let dup = strdup(message)
    Thread.current.threadDictionary[LastError.key] = OpaquePointer(dup!)
}

// MARK: - C ABI

/// Returns a null-terminated, borrowed pointer to the last error message
/// recorded on the calling thread. nil if no error since last clear.
@_cdecl("fab_last_error")
public func fab_last_error() -> UnsafePointer<CChar>? {
    return LastError.get()
}

/// Free a string returned by `fab_transcribe`. No-op on nil.
@_cdecl("fab_free_string")
public func fab_free_string(_ ptr: UnsafeMutablePointer<CChar>?) {
    if let p = ptr { free(UnsafeMutableRawPointer(p)) }
}

/// Release the AsrManager + .mlmodelc held by FluidAudio so the
/// Apple Neural Engine drops its weights and the process RSS
/// returns to baseline. Idempotent — calling on an already-unloaded
/// state is a no-op. Triggered by Rust's `FluidAudioBridge::Drop`
/// which fires when `ModelHandle::unload()` releases the handle
/// after the configured idle timeout (TASK-62.5).
///
/// Returns 0 unconditionally — there's no failure path that's
/// useful to surface; nilling the references hands the resources
/// to ARC and lets CoreML reclaim them on the next autorelease pool
/// drain.
@_cdecl("fab_unload")
public func fab_unload() -> Int32 {
    state.lock.lock()
    state.asr = nil
    state.loaded = false
    state.lock.unlock()
    return 0
}

/// Idempotent: download Parakeet v3 (first call) and load AsrManager.
/// Returns 0 on success, nonzero on error (call `fab_last_error`).
@_cdecl("fab_load")
public func fab_load() -> Int32 {
    let semaphore = DispatchSemaphore(value: 0)
    var status: Int32 = 0

    Task {
        defer { semaphore.signal() }
        state.lock.lock()
        let already = state.loaded
        state.lock.unlock()
        if already { return }
        do {
            let models = try await AsrModels.downloadAndLoad(version: .v3)
            let manager = AsrManager(config: .default)
            try await manager.loadModels(models)
            state.lock.lock()
            state.asr = manager
            state.loaded = true
            state.lock.unlock()
        } catch {
            setError("FluidAudio load failed: \(error.localizedDescription)")
            status = 1
        }
    }
    semaphore.wait()
    return status
}

/// Decode a 16 kHz mono f32 buffer. Returns a heap-allocated, null-
/// terminated UTF-8 string (caller frees via `fab_free_string`) and
/// writes the confidence into `out_confidence`. On error returns nil
/// and records a message accessible via `fab_last_error`.
@_cdecl("fab_transcribe")
public func fab_transcribe(
    _ samples: UnsafePointer<Float>?,
    _ count: UInt64,
    _ out_confidence: UnsafeMutablePointer<Float>?
) -> UnsafeMutablePointer<CChar>? {
    guard let samples = samples else {
        setError("fab_transcribe: null samples pointer")
        return nil
    }
    let buffer = Array(UnsafeBufferPointer(start: samples, count: Int(count)))

    let semaphore = DispatchSemaphore(value: 0)
    var resultText: String?
    var resultConf: Float = 0
    var failure: String?

    Task {
        defer { semaphore.signal() }
        state.lock.lock()
        let manager = state.asr
        state.lock.unlock()
        guard let manager = manager else {
            failure = "FluidAudio not loaded — call fab_load first"
            return
        }
        do {
            let result = try await manager.transcribe(buffer, source: .microphone)
            resultText = result.text
            resultConf = result.confidence
        } catch {
            failure = "FluidAudio transcribe failed: \(error.localizedDescription)"
        }
    }
    semaphore.wait()

    if let failure = failure {
        setError(failure)
        return nil
    }
    if let out = out_confidence { out.pointee = resultConf }
    return strdup(resultText ?? "")
}
