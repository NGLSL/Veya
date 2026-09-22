//! Stored flow records. Raw events stay separate; UI aggregation is a view.

use crate::events::{PasteMethod, SourceConfidence};

/// One observed paste trigger attached to a clipboard record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteTriggerRecord {
    pub target_app: String,
    pub target_pid: u32,
    pub target_window: String,
    pub method: PasteMethod,
    /// Hotkey observed; insertion into the target is NOT claimed.
    pub triggered_at_ms: i64,
}

/// One clipboard write. Keyed by clipboard sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardRecord {
    pub sequence: u32,
    pub content_type: String,
    pub content: String,
    pub content_hash: String,
    pub source_app: String,
    pub source_pid: u32,
    pub source_window: String,
    pub source_confidence: SourceConfidence,
    pub created_at_ms: i64,
    pub pastes: Vec<PasteTriggerRecord>,
}

impl ClipboardRecord {
    pub fn has_paste_activity(&self) -> bool {
        !self.pastes.is_empty()
    }
}
