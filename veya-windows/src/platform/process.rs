//! Resolve process exe name and window title from PID / HWND.
//! Called from the worker thread — never from Win32 callbacks.
//!
//! REGRESSION GUARD: do **not** use plain GetWindowTextW / WM_GETTEXT here.
//! Cross-integrity (Medium→High) titles can block forever and stall the worker
//! so [PASTE] lines never print — that looked like "LL hook missed elevated paste".
//! Use SendMessageTimeoutW + short timeout instead.

use std::path::Path;

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SendMessageTimeoutW, SMTO_ABORTIFHUNG, SMTO_BLOCK, WM_GETTEXT,
};

use veya_core::SourceConfidence;

/// Cap on cross-process WM_GETTEXT wait. Elevated/unresponsive targets must
/// not freeze flow tracking.
const TITLE_TIMEOUT_MS: u32 = 80;

/// Best-effort exe file name for `pid`. `"unknown"` on failure.
pub fn exe_name(pid: u32) -> String {
    if pid == 0 {
        return "unknown".to_string();
    }
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return format!("pid:{pid}(inaccessible)");
        };

        let result = (|| {
            let mut buf = vec![0u16; 32768];
            let mut size = buf.len() as u32;
            QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                PWSTR(buf.as_mut_ptr()),
                &mut size,
            )
            .ok()?;
            let path = String::from_utf16_lossy(&buf[..size as usize]);
            let name = Path::new(&path)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or(path);
            Some(name)
        })();

        let _ = CloseHandle(handle);
        result.unwrap_or_else(|| format!("pid:{pid}(query-failed)"))
    }
}

/// Source exe file name only (no confidence marks — UI formats those).
pub fn display_exe(pid: u32, _confidence: SourceConfidence) -> String {
    exe_name(pid)
}

/// Best-effort window title. Empty string on null hwnd / timeout / UIPI deny.
pub fn window_title(hwnd: isize) -> String {
    if hwnd == 0 {
        return String::new();
    }
    unsafe {
        let hw = HWND(hwnd as *mut _);
        let mut buf = [0u16; 512];
        let mut result: usize = 0;
        let ok = SendMessageTimeoutW(
            hw,
            WM_GETTEXT,
            WPARAM(buf.len()),
            LPARAM(buf.as_mut_ptr() as isize),
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            TITLE_TIMEOUT_MS,
            Some(&mut result),
        );
        // LRESULT 0 => timeout / failure / UIPI. Treat as no title.
        if ok == LRESULT(0) {
            return String::new();
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len])
    }
}
