//! Windows focus read: the foreground window, its owning process and
//! that process's exe.
//!
//! Known limit: a packaged (UWP) app's window belongs to
//! `ApplicationFrameHost.exe`, so such apps all report that host
//! instead of themselves.

use std::ffi::c_void;

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
};
use windows::core::{PCWSTR, PWSTR};

use super::{FocusContext, exe_file_name, exe_stem};

pub(super) fn read() -> FocusContext {
    // SAFETY: Win32 calls with buffers we own; every handle opened
    // here is closed before returning.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return FocusContext::default();
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));

        let mut title = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut title).max(0) as usize;
        let window_title = String::from_utf16_lossy(&title[..len.min(title.len())]);

        let (app_id, app_name) = match exe_path(pid) {
            Some(path) => {
                let name = file_description(&path)
                    .filter(|d| !d.trim().is_empty())
                    .unwrap_or_else(|| exe_stem(&path));
                (exe_file_name(&path), name)
            }
            None => (String::new(), String::new()),
        };
        FocusContext { app_id, app_name, window_title, pid, is_self: false }
    }
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
    fn unknown_process_has_no_path() {
        assert_eq!(unsafe { exe_path(0) }, None);
    }

    #[test]
    fn missing_file_has_no_description() {
        assert_eq!(unsafe { file_description(r"C:\no\such\file.exe") }, None);
    }
}
