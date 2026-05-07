//! End-to-end exercise of `crashes::install_panic_hook`.
//!
//! Lives in a dedicated integration-test binary so the panic hook is
//! installed once for the whole test process — `install_panic_hook` is
//! idempotent (first call wins) so a second test elsewhere wouldn't
//! observe a freshly-installed closure.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use openwhisper_core::crashes::{self, CrashFile};

#[test]
fn panic_on_thread_writes_chained_crash_file() {
    let dir = tempfile::tempdir().expect("tmpdir");

    // Install our own "previous hook" via set_hook BEFORE
    // install_panic_hook so we can assert the chained call still fires.
    let chain_calls = Arc::new(AtomicUsize::new(0));
    let chain_calls_for_hook = chain_calls.clone();
    std::panic::set_hook(Box::new(move |_info| {
        chain_calls_for_hook.fetch_add(1, Ordering::SeqCst);
    }));

    crashes::install_panic_hook(dir.path().to_path_buf(), "0.0.0-test".into());

    // Panic from a worker thread — exercises tokio-style off-main-thread
    // panic capture without needing the runtime.
    let handle = std::thread::Builder::new()
        .name("crash-test-worker".into())
        .spawn(|| {
            panic!("kaboom from worker thread");
        })
        .expect("spawn");
    let _ = handle.join();

    // The chained hook (the one we installed before install_panic_hook)
    // must have been invoked — proves AC #4.
    assert!(
        chain_calls.load(Ordering::SeqCst) >= 1,
        "previous hook was not chained; default panic stderr would be lost"
    );

    // Exactly one crash file should exist; it must deserialize cleanly.
    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .expect("read tmpdir")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().and_then(|s| s.to_str()) == Some("json")
        })
        .collect();
    assert_eq!(
        entries.len(),
        1,
        "expected exactly one crash file, found {}",
        entries.len()
    );

    let raw = std::fs::read_to_string(entries[0].path()).expect("read crash file");
    let parsed: CrashFile = serde_json::from_str(&raw).expect("deserialize crash file");

    assert_eq!(parsed.schema_version, crashes::SCHEMA_VERSION);
    assert_eq!(parsed.rust_panic.thread_name, "crash-test-worker");
    assert!(
        parsed.rust_panic.message.contains("kaboom from worker thread"),
        "missing panic message: {}",
        parsed.rust_panic.message
    );
    assert!(
        !parsed.rust_panic.backtrace.is_empty(),
        "backtrace should be captured"
    );
    assert!(
        parsed.rust_panic.location.contains("panic_hook.rs"),
        "location should point at this test file: {}",
        parsed.rust_panic.location
    );
}
