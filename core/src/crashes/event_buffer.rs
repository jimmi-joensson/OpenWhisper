//! Bounded ring buffer for dictation transitions.
//!
//! The panic hook drains this into the crash file's `events` array
//! (oldest first). Capacity stays small per spec — we only need recent
//! history, not a full trace log.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::crashes::{EVENT_BUFFER_CAPACITY, Event};

static BUFFER: Mutex<VecDeque<Event>> = Mutex::new(VecDeque::new());

/// Append an event. Evicts the oldest entry once capacity is reached.
/// Silently drops on lock poisoning — telemetry loss beats double-fault.
pub fn push_event(kind: &str, data: serde_json::Value) {
    let ts_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let ev = Event {
        ts_unix_ms,
        kind: kind.to_string(),
        data,
    };
    if let Ok(mut buf) = BUFFER.lock() {
        if buf.len() >= EVENT_BUFFER_CAPACITY {
            buf.pop_front();
        }
        buf.push_back(ev);
    }
}

/// Drain all events. Caller takes ownership; subsequent pushes start in
/// an empty buffer. Used exclusively by the panic hook.
pub fn drain() -> Vec<Event> {
    BUFFER
        .lock()
        .map(|mut buf| buf.drain(..).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_evicts_oldest_at_capacity() {
        // Other tests in this file would race on BUFFER; keep the
        // assertion logic single-test and drain at entry.
        let _ = drain();
        for i in 0..(EVENT_BUFFER_CAPACITY + 5) {
            push_event("Tick", serde_json::json!({ "i": i }));
        }
        let events = drain();
        assert_eq!(events.len(), EVENT_BUFFER_CAPACITY);
        let first_i = events
            .first()
            .unwrap()
            .data
            .get("i")
            .and_then(|v| v.as_u64())
            .unwrap();
        assert_eq!(first_i, 5);
    }
}
