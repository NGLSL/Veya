//! Non-Windows stub so workspace typechecks off-box.

use std::sync::mpsc::Sender;

use super::{ClipboardChangeRaw, PasteTriggerRaw};
use crate::hotkey::Hotkey;
use veya_core::ClipboardPayload;

#[derive(Debug, Clone)]
pub enum PlatformEvent {
    ClipboardChange(ClipboardChangeRaw),
    ClipboardSkipped {
        sequence: u32,
    },
    PasteTrigger(PasteTriggerRaw),
    ToggleWindow {
        visible: bool,
    },
    HotkeyStatus {
        requested: Hotkey,
        active: Hotkey,
        error: Option<String>,
    },
}

pub fn run(_tx: Sender<PlatformEvent>, _hotkey: Hotkey) -> Result<(), String> {
    Err("veya-windows capture is Windows-only".to_string())
}

pub fn write_text(_text: &str) -> Option<u32> {
    None
}

pub fn write_payload(_payload: &ClipboardPayload) -> Result<u32, String> {
    Err("veya-windows clipboard support is Windows-only".to_string())
}
