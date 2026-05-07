//! Process telemetry primitives — read-side facts about the running
//! OpenWhisper process for the in-app Diagnostics pane and the
//! headless `cli` smoke harness.
//!
//! Today: `memory::query_process_memory` returns the current process
//! RSS + peak RSS via the `sysinfo` crate (cross-platform). Future
//! tasks under TASK-62 layer per-model attribution on top via the
//! `ModelHandle` registry; that aggregation lives here too once the
//! handle abstraction lands (TASK-62.2 → 62.7).

pub mod memory;

pub use memory::{query_process_memory, ProcessMemory};
