//! Clipboard change listener: AddClipboardFormatListener + WM_CLIPBOARDUPDATE.
//!
//! Only CF_UNICODETEXT is handled in v0.1. Callback path stays cheap: read text
//! + owner HWND + PID + sequence + time; exe/title resolved on the worker.

use windows::Win32::Foundation::{HGLOBAL, HANDLE, HWND};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, GetClipboardData, GetClipboardOwner,
    GetClipboardSequenceNumber, GetOpenClipboardWindow, IsClipboardFormatAvailable,
    OpenClipboard, RemoveClipboardFormatListener,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use super::time::now_ms;
use super::{ClipboardChangeRaw, PlatformEvent};
use super::win::emit;
use veya_core::SourceConfidence;

/// Sent to the window when clipboard content changes.
pub const WM_CLIPBOARDUPDATE: u32 = 0x031D;

/// Win32 CF_UNICODETEXT (value 13). Declared locally to avoid the Ole feature.
const CF_UNICODETEXT: u32 = 13;

pub fn install_listener(hwnd: HWND) -> windows::core::Result<()> {
    unsafe {
        AddClipboardFormatListener(hwnd)?;
    }
    Ok(())
}

pub fn uninstall_listener(hwnd: HWND) {
    unsafe {
        let _ = RemoveClipboardFormatListener(hwnd);
    }
}

fn pid_of(hwnd: HWND) -> u32 {
    if hwnd.0.is_null() {
        return 0;
    }
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    pid
}

/// Resolve the likely source HWND/PID for this clipboard change.
///
/// Prefer `GetClipboardOwner`. QQ/OLE-style writers often leave owner NULL —
/// fall back to the open-clipboard window, then the foreground window (heuristic).
///
/// REGRESSION GUARD: call this **before** `OpenClipboard`. Opening the clipboard
/// can re-associate owner with this task and mis-attribute source to Veya.
fn resolve_source() -> (isize, u32, SourceConfidence) {
    let owner = unsafe { GetClipboardOwner() }.unwrap_or_default();
    if !owner.0.is_null() {
        return (owner.0 as isize, pid_of(owner), SourceConfidence::Exact);
    }

    let open = unsafe { GetOpenClipboardWindow() }.unwrap_or_default();
    if !open.0.is_null() {
        return (open.0 as isize, pid_of(open), SourceConfidence::Likely);
    }

    let fg = unsafe { GetForegroundWindow() };
    if !fg.0.is_null() {
        return (fg.0 as isize, pid_of(fg), SourceConfidence::Likely);
    }

    (0, 0, SourceConfidence::Unknown)
}

pub fn handle_clipboard_update(_hwnd: HWND) {
    let sequence = unsafe { GetClipboardSequenceNumber() };

    // REGRESSION GUARD: capture source BEFORE OpenClipboard (see resolve_source).
    let (owner_hwnd, source_pid, source_confidence) = resolve_source();

    let has_text = unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) }.is_ok();
    if !has_text {
        return;
    }

    let Some(text) = read_unicode_text() else {
        return;
    };

    emit(PlatformEvent::ClipboardChange(ClipboardChangeRaw {
        sequence,
        text,
        owner_hwnd,
        source_pid,
        source_confidence,
        timestamp_ms: now_ms(),
    }));
}

fn read_unicode_text() -> Option<String> {
    unsafe {
        if OpenClipboard(None).is_err() {
            return None;
        }

        let result = (|| {
            if IsClipboardFormatAvailable(CF_UNICODETEXT).is_err() {
                return None;
            }
            let handle = GetClipboardData(CF_UNICODETEXT).ok()?;
            let hglobal = HGLOBAL(handle.0);
            let ptr = GlobalLock(hglobal);
            if ptr.is_null() {
                return None;
            }

            let mut len = 0usize;
            let base = ptr as *const u16;
            // CF_UNICODETEXT is a null-terminated UTF-16 string. Cap length
            // against malformed clipboard payloads.
            while len < 1_000_000 {
                if *base.add(len) == 0 {
                    break;
                }
                len += 1;
            }

            let text = String::from_utf16_lossy(std::slice::from_raw_parts(base, len));
            let _ = GlobalUnlock(hglobal);
            Some(text)
        })();

        let _ = CloseClipboard();
        result
    }
}

/// Write text to the system clipboard (Veya-initiated). Returns the new sequence number.
///
/// Caller should arm `InternalClipboardWrite` with this sequence + content hash
/// so the resulting WM_CLIPBOARDUPDATE is suppressed as a history row.
pub fn write_unicode_text(text: &str) -> Option<u32> {
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc};
    use windows::Win32::System::DataExchange::{EmptyClipboard, SetClipboardData};

    unsafe {
        if OpenClipboard(None).is_err() {
            return None;
        }
        let result = (|| {
            let _ = EmptyClipboard();
            let mut utf16: Vec<u16> = text.encode_utf16().collect();
            utf16.push(0);
            let bytes = utf16.len() * std::mem::size_of::<u16>();
            let hglobal = GlobalAlloc(GMEM_MOVEABLE, bytes).ok()?;
            let ptr = GlobalLock(hglobal);
            if ptr.is_null() {
                return None;
            }
            std::ptr::copy_nonoverlapping(utf16.as_ptr(), ptr as *mut u16, utf16.len());
            let _ = GlobalUnlock(hglobal);
            // Keep hglobal ownership with the system after SetClipboardData.
            if SetClipboardData(CF_UNICODETEXT, Some(HANDLE(hglobal.0 as *mut _))).is_err() {
                return None;
            }
            Some(GetClipboardSequenceNumber())
        })();
        let _ = CloseClipboard();
        result
    }
}
