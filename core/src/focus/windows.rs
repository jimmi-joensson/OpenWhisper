//! Windows focus read: the foreground window, its owning process and
//! that process's exe. The list of running apps is the same read for
//! every visible top-level window.
//!
//! Known limit: a packaged (UWP) app's window belongs to
//! `ApplicationFrameHost.exe`, so such apps all report that host
//! instead of themselves.

use std::ffi::c_void;

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GWL_EXSTYLE, GetForegroundWindow, GetWindowLongW, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, PCWSTR, PWSTR};

use super::{FocusContext, exe_file_name, exe_stem};

pub(super) fn read(with_title: bool) -> FocusContext {
    // SAFETY: Win32 calls with buffers we own; every handle opened
    // here is closed before returning.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return FocusContext::default();
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));

        let window_title = if with_title {
            let mut title = [0u16; 512];
            let len = GetWindowTextW(hwnd, &mut title).max(0) as usize;
            String::from_utf16_lossy(&title[..len.min(title.len())])
        } else {
            String::new()
        };

        let (app_id, app_name) = match exe_path(pid) {
            Some(path) => app_of_exe(&path),
            None => (String::new(), String::new()),
        };
        FocusContext { app_id, app_name, window_title, pid, is_self: false }
    }
}

/// Id and name of the app whose exe is at `path`: the file name in
/// lower case, and the file's description (the stem of the file name
/// when it has none).
unsafe fn app_of_exe(path: &str) -> (String, String) {
    let name = unsafe { file_description(path) }
        .filter(|d| !d.trim().is_empty())
        .unwrap_or_else(|| exe_stem(path));
    (exe_file_name(path), name)
}

/// The processes that own a visible top-level window with a title
/// (tool windows left out), each exe once, without this process. Same
/// ids and names as [`read`]. A process we may not query (elevated)
/// is skipped.
pub(super) fn running() -> Vec<(String, String)> {
    let own_pid = std::process::id();
    let mut pids: Vec<u32> = Vec::new();
    // SAFETY: `pids` outlives the call, and the callback is the only
    // user of the pointer while `EnumWindows` runs.
    unsafe {
        let _ = EnumWindows(Some(collect_window_pid), LPARAM(&mut pids as *mut Vec<u32> as isize));
    }
    let mut paths: Vec<String> = Vec::new();
    let mut apps = Vec::new();
    for pid in pids {
        if pid == own_pid {
            continue;
        }
        // SAFETY: see `exe_path` and `file_description`.
        let Some(path) = (unsafe { exe_path(pid) }) else {
            continue;
        };
        // One read of the version info per exe, however many
        // processes and windows it has.
        if paths.iter().any(|known| known.eq_ignore_ascii_case(&path)) {
            continue;
        }
        apps.push(unsafe { app_of_exe(&path) });
        paths.push(path);
    }
    apps
}

/// `EnumWindows` callback: note the owning process of every window a
/// person would call a window. `lparam` points at a `Vec<u32>`.
unsafe extern "system" fn collect_window_pid(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `Vec<u32>` passed by `running`, alive
    // for the whole enumeration and not touched by anyone else.
    unsafe {
        let pids = &mut *(lparam.0 as *mut Vec<u32>);
        let tool_window = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0;
        if IsWindowVisible(hwnd).as_bool() && !tool_window && GetWindowTextLengthW(hwnd) > 0 {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid != 0 && !pids.contains(&pid) {
                pids.push(pid);
            }
        }
    }
    // Keep going.
    BOOL(1)
}

/// Full path of the process's exe. `None` for a process we may not
/// query (elevated or protected) or one that has exited.
unsafe fn exe_path(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let result =
            QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(handle);
        result.ok()?;
        Some(String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())]))
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The exe's `FileDescription` ("Slack", "Google Chrome"): the name
/// Task Manager shows.
unsafe fn file_description(path: &str) -> Option<String> {
    unsafe {
        let path_w = wide(path);
        let size = GetFileVersionInfoSizeW(PCWSTR(path_w.as_ptr()), None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(PCWSTR(path_w.as_ptr()), None, size, data.as_mut_ptr() as *mut c_void)
            .ok()?;

        // First language + code page the file declares.
        let mut ptr: *mut c_void = std::ptr::null_mut();
        let mut len = 0u32;
        let key = wide("\\VarFileInfo\\Translation");
        if !VerQueryValueW(data.as_ptr() as *const c_void, PCWSTR(key.as_ptr()), &mut ptr, &mut len)
            .as_bool()
            || ptr.is_null()
            || len < 4
        {
            return None;
        }
        let lang = *(ptr as *const u16);
        let code_page = *(ptr as *const u16).add(1);

        let key = wide(&format!("\\StringFileInfo\\{lang:04x}{code_page:04x}\\FileDescription"));
        let mut ptr: *mut c_void = std::ptr::null_mut();
        let mut len = 0u32;
        if !VerQueryValueW(data.as_ptr() as *const c_void, PCWSTR(key.as_ptr()), &mut ptr, &mut len)
            .as_bool()
            || ptr.is_null()
            || len == 0
        {
            return None;
        }
        // `len` counts UTF-16 units including the terminator.
        let chars = std::slice::from_raw_parts(ptr as *const u16, len as usize);
        let end = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
        Some(String::from_utf16_lossy(&chars[..end]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_own_exe_path() {
        let path = unsafe { exe_path(std::process::id()) }.expect("own exe path");
        assert!(path.to_lowercase().ends_with(".exe"), "{path}");
    }

    #[test]
    fn running_apps_have_exe_ids_and_leave_this_process_out() {
        let own = exe_file_name(&unsafe { exe_path(std::process::id()) }.expect("own exe path"));
        for (id, name) in running() {
            assert_eq!(id, id.to_lowercase(), "{id}");
            assert!(!name.is_empty(), "{id}");
            assert_ne!(id, own);
        }
    }

    #[test]
    fn unknown_process_has_no_path() {
        assert_eq!(unsafe { exe_path(0) }, None);
    }

    #[test]
    fn missing_file_has_no_description() {
        assert_eq!(unsafe { file_description(r"C:\no\such\file.exe") }, None);
    }
}
