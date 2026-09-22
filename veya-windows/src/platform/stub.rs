//! Non-Windows stub so workspace typechecks off-box.

use std::sync::mpsc::Sender;

use super::{ClipboardChangeRaw, PasteTriggerRaw};

#[derive(Debug, Clone)]
pub enum PlatformEvent {
    ClipboardChange(ClipboardChangeRaw),
    PasteTrigger(PasteTriggerRaw),
}

pub fn run(_tx: Sender<PlatformEvent>) -> Result<(), String> {
    Err("veya-windows capture is Windows-only".to_string())
}

pub fn write_text(_text: &str) -> Option<u32> {
    None
}
