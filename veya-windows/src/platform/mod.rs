//! Windows platform layer — all Win32 unsafe stays in this module tree.

#[cfg(windows)]
pub mod clipboard;
#[cfg(windows)]
pub mod keyboard;
#[cfg(windows)]
pub mod process;
#[cfg(windows)]
pub mod time;
#[cfg(windows)]
pub mod window;
#[cfg(windows)]
mod win;

#[cfg(windows)]
pub use win::{emit, request_shutdown, run, PlatformEvent};
#[cfg(windows)]
pub use clipboard::write_unicode_text as write_text;

#[cfg(not(windows))]
mod stub;
#[cfg(not(windows))]
pub use stub::{run, write_text, PlatformEvent};

use veya_core::{ClipboardChange, PasteTrigger, SourceConfidence};

/// Cheap clipboard signal from the Win32 callback (names resolved later).
#[derive(Debug, Clone)]
pub struct ClipboardChangeRaw {
    pub sequence: u32,
    pub text: String,
    pub owner_hwnd: isize,
    pub source_pid: u32,
    pub source_confidence: SourceConfidence,
    pub timestamp_ms: i64,
}

/// Cheap paste signal from the keyboard hook (names resolved later).
#[derive(Debug, Clone)]
pub struct PasteTriggerRaw {
    pub target_hwnd: isize,
    pub target_pid: u32,
    pub method: veya_core::PasteMethod,
    pub timestamp_ms: i64,
}

/// Resolve exe/window and hash content → core `ClipboardChange`.
pub fn enrich_clipboard(raw: ClipboardChangeRaw) -> ClipboardChange {
    #[cfg(windows)]
    let (source_exe, source_window) = {
        let exe = process::display_exe(raw.source_pid, raw.source_confidence);
        let window = process::window_title(raw.owner_hwnd);
        (exe, window)
    };
    #[cfg(not(windows))]
    let (source_exe, source_window) = ("unknown".to_string(), String::new());

    ClipboardChange {
        sequence: raw.sequence,
        content_hash: crate::hash::content_hash(&raw.text),
        text: raw.text,
        source_pid: raw.source_pid,
        source_exe,
        source_window,
        source_confidence: raw.source_confidence,
        timestamp_ms: raw.timestamp_ms,
    }
}

/// Resolve target names → core `PasteTrigger`.
pub fn enrich_paste(raw: PasteTriggerRaw) -> PasteTrigger {
    #[cfg(windows)]
    let (target_exe, target_window) = {
        let exe = process::exe_name(raw.target_pid);
        let window = process::window_title(raw.target_hwnd);
        (exe, window)
    };
    #[cfg(not(windows))]
    let (target_exe, target_window) = ("unknown".to_string(), String::new());

    PasteTrigger {
        target_pid: raw.target_pid,
        target_exe,
        target_window,
        method: raw.method,
        timestamp_ms: raw.timestamp_ms,
    }
}
