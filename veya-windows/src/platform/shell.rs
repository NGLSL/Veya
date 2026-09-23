//! Small Windows shell actions used by the desktop view.
//!
//! The UI supplies an explicit target, while this module owns the platform
//! process launch and URL validation. Keeping the command construction here
//! prevents shell details from leaking into Iced message handling.

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::core::{w, PCWSTR};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// Open an application or executable path with the Windows shell.
pub fn open_source(executable: &str) {
    shell_open(executable.trim());
}

/// Open an HTTP(S) URL with the Windows shell.
pub fn open_link(url: &str) {
    let mut target = url.trim();
    if !(target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("www."))
    {
        return;
    }

    let normalized;
    if target.starts_with("www.") {
        normalized = format!("https://{target}");
        target = &normalized;
    }
    shell_open(target);
}

/// Open a Bing search for the supplied text.
pub fn web_search(query: &str) {
    let url = format!("https://www.bing.com/search?q={}", url_encode(query.trim()));
    open_link(&url);
}

/// Ask Windows to elevate and start a verified installer. The caller owns
/// download and digest verification before this function is invoked.
pub fn launch_installer(path: &Path) -> Result<(), String> {
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("runas"),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        return Err("无法启动安装器，可能已取消管理员授权".into());
    }
    Ok(())
}

fn url_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn shell_open(target: &str) {
    if target.is_empty() {
        return;
    }
    let wide: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let _ = ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}
