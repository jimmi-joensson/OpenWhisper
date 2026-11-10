//! Only one OpenWhisper runs at a time (TASK-102).
//!
//! The production app and the Dev build are two different apps as far
//! as the OS is concerned (different identifiers), so the
//! single-instance plugin does not keep them apart. Left running side
//! by side they both listen for the same hotkey: every dictation is
//! recorded, pasted and saved twice.
//!
//! So the build that starts closes the other one. The newest launch
//! wins: open the Dev build and production quits, open production and
//! the Dev build quits.
//!
//! - macOS: the other build is found by its bundle identifier and
//!   asked to quit the normal way (what the Dock's Quit does; no
//!   permission needed). If it is still there after a few seconds it
//!   is force-quit.
//! - Windows: both builds run from an exe with the same file name, in
//!   different folders, so any other process with our exe name is the
//!   other build (a second copy of the same build never gets this far:
//!   the single-instance plugin stops it). It is ended with
//!   `TerminateProcess`. That is abrupt, but nothing is lost: settings
//!   and history are written in one step, and the stats database
//!   recovers on its own.
//!
//! Runs on its own thread so a slow quit never delays startup.

use tauri::AppHandle;

/// Identifier of the released app (`tauri.conf.json`).
pub const PRODUCTION_ID: &str = "com.openwhisper.app";
/// Identifier of the Dev build (`tauri.dev.conf.json`).
pub const DEV_ID: &str = "com.openwhisper.app.dev";

/// The identifiers of the other OpenWhisper builds, seen from `own`.
/// Empty for an identifier we do not know (a fork or a renamed
/// build): then nothing is closed.
fn other_builds(own: &str) -> &'static [&'static str] {
    match own {
        PRODUCTION_ID => &[DEV_ID],
        DEV_ID => &[PRODUCTION_ID],
        _ => &[],
    }
}

/// Close the other OpenWhisper build if it is running. Call once at
/// startup.
pub fn close_other_builds(app: &AppHandle) {
    let others = other_builds(&app.config().identifier);
    if others.is_empty() {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("openwhisper-sibling".into())
        .spawn(move || platform::close(others));
    if let Err(e) = spawned {
        eprintln!("[sibling] could not start the check: {e}");
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::time::{Duration, Instant};

    use objc2::rc::autoreleasepool;
    use objc2_app_kit::NSRunningApplication;
    use objc2_foundation::NSString;

    /// How long the other build gets to quit on its own.
    const GRACE: Duration = Duration::from_secs(4);

    extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }

    /// The process still exists. `kill` with signal 0 sends nothing;
    /// it only reports whether the pid is there.
    fn alive(pid: i32) -> bool {
        // SAFETY: signal 0 has no effect on the target.
        pid > 0 && unsafe { kill(pid, 0) } == 0
    }

    pub(super) fn close(others: &[&str]) {
        autoreleasepool(|_| {
            for id in others {
                let running =
                    NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(id));
                for app in running.iter() {
                    let pid = app.processIdentifier();
                    eprintln!("[sibling] {id} is running (pid {pid}); asking it to quit");
                    app.terminate();
                    let asked = Instant::now();
                    while alive(pid) && asked.elapsed() < GRACE {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    if alive(pid) {
                        eprintln!("[sibling] {id} did not quit in time; forcing it");
                        app.forceTerminate();
                    }
                }
            }
        });
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};

    /// Pids of every other process whose exe has the same file name
    /// as ours.
    fn other_pids() -> Vec<u32> {
        let own_pid = std::process::id();
        let Some(own_name) = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()))
        else {
            return Vec::new();
        };
        let mut pids = Vec::new();
        // SAFETY: plain Win32 calls; the snapshot handle is closed
        // before returning and `entry` has its size set as required.
        unsafe {
            let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return pids;
            };
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
            while more {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase();
                if name == own_name && entry.th32ProcessID != own_pid {
                    pids.push(entry.th32ProcessID);
                }
                more = Process32NextW(snapshot, &mut entry).is_ok();
            }
            let _ = CloseHandle(snapshot);
        }
        pids
    }

    pub(super) fn close(_others: &[&str]) {
        for pid in other_pids() {
            eprintln!("[sibling] another OpenWhisper build is running (pid {pid}); ending it");
            // SAFETY: the handle is closed after use.
            unsafe {
                match OpenProcess(PROCESS_TERMINATE, false, pid) {
                    Ok(handle) => {
                        if let Err(e) = TerminateProcess(handle, 0) {
                            eprintln!("[sibling] could not end pid {pid}: {e}");
                        }
                        let _ = CloseHandle(handle);
                    }
                    Err(e) => eprintln!("[sibling] could not open pid {pid}: {e}"),
                }
            }
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    pub(super) fn close(_others: &[&str]) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_build_names_the_other() {
        assert_eq!(other_builds(PRODUCTION_ID), [DEV_ID]);
        assert_eq!(other_builds(DEV_ID), [PRODUCTION_ID]);
    }

    #[test]
    fn an_unknown_build_closes_nothing() {
        assert!(other_builds("com.example.fork").is_empty());
        assert!(other_builds("").is_empty());
    }

    #[test]
    fn identifiers_match_the_tauri_configs() {
        let config = |file: &str| -> serde_json::Value {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(file);
            serde_json::from_str(&std::fs::read_to_string(path).expect("read config")).expect("parse config")
        };
        assert_eq!(config("tauri.conf.json")["identifier"], PRODUCTION_ID);
        assert_eq!(config("tauri.dev.conf.json")["identifier"], DEV_ID);
    }
}
