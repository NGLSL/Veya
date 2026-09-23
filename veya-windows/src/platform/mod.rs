//! Windows platform layer — all Win32 unsafe stays in this module tree.

#[cfg(windows)]
pub mod chrome;
#[cfg(windows)]
pub mod clipboard;
#[cfg(windows)]
mod hotkey;
#[cfg(windows)]
pub mod icon;
#[cfg(windows)]
pub mod keyboard;
#[cfg(windows)]
pub mod process;
#[cfg(windows)]
pub mod shell;
#[cfg(windows)]
pub mod singleton;
#[cfg(windows)]
pub mod time;
pub mod update;
#[cfg(windows)]
mod win;
#[cfg(windows)]
pub mod window;
#[cfg(windows)]
pub mod window_place;

#[cfg(windows)]
pub use clipboard::{write_payload, write_unicode_text as write_text};
#[cfg(windows)]
pub use win::{
    emit, request_hotkey_change, request_hotkey_recording, request_shutdown, run, PlatformEvent,
};

#[cfg(not(windows))]
mod stub;
#[cfg(not(windows))]
pub use stub::{run, write_text, PlatformEvent};
#[cfg(not(windows))]
mod shell_stub;
#[cfg(not(windows))]
pub mod shell {
    pub use super::shell_stub::{launch_installer, open_link, open_source, web_search};
}

use veya_core::ClipboardPayload;
use veya_core::{ClipboardChange, PasteTrigger, SourceConfidence};

/// Signal sent to the Iced window owner by a second launch or a global hotkey.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowSignal {
    Activate,
    Hotkey { visible: bool },
}

/// Cheap clipboard signal from the Win32 callback (names resolved later).
#[derive(Debug, Clone)]
pub struct ClipboardChangeRaw {
    pub sequence: u32,
    pub payload: ClipboardPayloadRaw,
    pub owner_hwnd: isize,
    pub source_pid: u32,
    pub source_confidence: SourceConfidence,
    pub timestamp_ms: i64,
}

/// Clipboard bytes copied while the Windows clipboard is open. Image DIBs are
/// normalized to the core's PNG payload after the clipboard lock is released.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardPayloadRaw {
    Text(String),
    Files(Vec<String>),
    Dib { format: u32, bytes: Vec<u8> },
}

/// Cheap paste signal from the keyboard hook (names resolved later).
#[derive(Debug, Clone)]
pub struct PasteTriggerRaw {
    pub target_hwnd: isize,
    pub target_pid: u32,
    pub method: veya_core::PasteMethod,
    pub timestamp_ms: i64,
}

/// Resolve exe/window and normalize platform data into the core payload.
pub fn enrich_clipboard(raw: ClipboardChangeRaw) -> Option<ClipboardChange> {
    #[cfg(windows)]
    let (source_exe, source_window) = {
        let exe = process::display_exe(raw.source_pid, raw.source_confidence);
        let window = process::window_title(raw.owner_hwnd);
        (exe, window)
    };
    #[cfg(not(windows))]
    let (source_exe, source_window) = ("unknown".to_string(), String::new());

    let payload = match raw.payload {
        ClipboardPayloadRaw::Text(text) => ClipboardPayload::Text(text),
        ClipboardPayloadRaw::Files(paths) => ClipboardPayload::Files(paths),
        ClipboardPayloadRaw::Dib { format, bytes } => {
            #[cfg(windows)]
            {
                let (png, width, height) = clipboard::dib_to_png(format, &bytes)?;
                ClipboardPayload::Image { png, width, height }
            }
            #[cfg(not(windows))]
            {
                let _ = (format, bytes);
                return None;
            }
        }
    };

    Some(ClipboardChange {
        sequence: raw.sequence,
        content_hash: veya_core::payload_hash(&payload),
        payload,
        source_pid: raw.source_pid,
        source_exe,
        source_window,
        source_confidence: raw.source_confidence,
        timestamp_ms: raw.timestamp_ms,
    })
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
