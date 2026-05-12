//! Verbose logging gated by the `OPENWHISPER_VERBOSE` env var.
//!
//! Enable by either:
//!   - `pnpm dev:tauri --verbose` (the dev script wires the env var)
//!   - `OPENWHISPER_VERBOSE=1` set before launching the binary directly
//!
//! Use the `verbose_log!` macro at any pipeline checkpoint that should
//! emit timing or state info for the automated-feedback-loop scripts.
//! Output goes to stderr, prefixed with `[ow.<area>]` so consumers can
//! grep / parse without sifting through unrelated chatter from cpal,
//! tauri, etc.
//!
//! Intentionally one OnceLock + env read, no `log` / `tracing` crate —
//! this is a perf-tuning aid, not structured logging. Cost when off is
//! one atomic load + branch per call site.

use std::sync::OnceLock;

static ENABLED: OnceLock<bool> = OnceLock::new();

/// True when `OPENWHISPER_VERBOSE` was set in the process environment at
/// first call. Cached for the lifetime of the process — env mutations
/// after the first call don't take effect, which is fine for a dev flag.
pub fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var_os("OPENWHISPER_VERBOSE").is_some())
}

/// Print to stderr only when verbose mode is on. Same args as
/// [`eprintln!`]. Prefix lines with a stable `[ow.area]` tag so log
/// consumers can grep deterministically.
#[macro_export]
macro_rules! verbose_log {
    ($($t:tt)*) => {
        if $crate::verbose::enabled() {
            eprintln!($($t)*);
        }
    };
}
