//! Stored flow records. Raw events stay separate; UI aggregation is a view.

use crate::events::{PasteConfidence, PasteMethod, SourceConfidence};

/// The actual clipboard payload retained for replay. A path-like text string
/// stays Text; Files is reserved for a real file-list clipboard format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardPayload {
    Text(String),
    Files(Vec<String>),
    Image {
        png: Vec<u8>,
        width: u32,
        height: u32,
    },
}

impl ClipboardPayload {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Text(_) => "text",
            Self::Files(_) => "files",
            Self::Image { .. } => "image",
        }
    }

    /// Text projection for search and history labels, never used for replay.
    pub fn display_text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Files(paths) => paths.join("\n"),
            Self::Image { width, height, .. } => format!("图片 {width} × {height}"),
        }
    }
}

/// Stable, type-separated payload identity for aggregation and internal write suppression.
pub fn payload_hash(payload: &ClipboardPayload) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    fn feed(mut hash: u64, bytes: &[u8]) -> u64 {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(PRIME);
        }
        hash
    }
    let mut hash = feed(OFFSET, payload.kind().as_bytes());
    match payload {
        ClipboardPayload::Text(text) => hash = feed(hash, text.as_bytes()),
        ClipboardPayload::Files(paths) => {
            for path in paths {
                hash = feed(hash, &(path.len() as u64).to_le_bytes());
                hash = feed(hash, path.as_bytes());
            }
        }
        ClipboardPayload::Image { png, width, height } => {
            hash = feed(hash, &width.to_le_bytes());
            hash = feed(hash, &height.to_le_bytes());
            hash = feed(hash, png);
        }
    }
    format!("{hash:016x}")
}

/// One observed paste trigger attached to a clipboard record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteTriggerRecord {
    pub target_app: String,
    pub target_pid: u32,
    pub target_window: String,
    pub method: PasteMethod,
    pub confidence: PasteConfidence,
    /// Unix epoch milliseconds.
    pub triggered_at_ms: i64,
}

/// One clipboard write. Keyed by clipboard sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardRecord {
    pub sequence: u32,
    pub content_type: String,
    pub content: String,
    /// Authoritative data for replay. `content` is only a searchable projection.
    pub payload: ClipboardPayload,
    pub content_hash: String,
    pub source_app: String,
    pub source_pid: u32,
    pub source_window: String,
    pub source_confidence: SourceConfidence,
    pub created_at_ms: i64,
    /// Retention state of this raw clipboard event, independent of its payload.
    pub pinned: bool,
    pub pastes: Vec<PasteTriggerRecord>,
}

impl ClipboardRecord {
    pub fn has_paste_activity(&self) -> bool {
        !self.pastes.is_empty()
    }
}
