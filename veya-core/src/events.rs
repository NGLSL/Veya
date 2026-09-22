//! Platform-facing event shapes (already enriched; core does not call Win32).

/// How we learned about the clipboard source window/process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceConfidence {
    /// Clipboard owner HWND gave a real source.
    Exact,
    /// Owner unavailable; approximated via open-clipboard / foreground HWND.
    Likely,
    /// Nothing usable to attribute.
    Unknown,
}

impl SourceConfidence {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceConfidence::Exact => "exact",
            SourceConfidence::Likely => "likely",
            SourceConfidence::Unknown => "unknown",
        }
    }
}

/// Paste shortcut that fired the trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteMethod {
    CtrlV,
    ShiftInsert,
}

impl PasteMethod {
    pub fn label(self) -> &'static str {
        match self {
            PasteMethod::CtrlV => "Ctrl+V",
            PasteMethod::ShiftInsert => "Shift+Insert",
        }
    }
}

/// New CF_UNICODETEXT on the clipboard (enriched by the platform layer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardChange {
    pub sequence: u32,
    pub text: String,
    pub content_hash: String,
    pub source_pid: u32,
    pub source_exe: String,
    pub source_window: String,
    pub source_confidence: SourceConfidence,
    /// Unix epoch milliseconds.
    pub timestamp_ms: i64,
}

/// Paste **intent** observed via keyboard hook — not a verified insertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteTrigger {
    pub target_pid: u32,
    pub target_exe: String,
    pub target_window: String,
    pub method: PasteMethod,
    /// Unix epoch milliseconds.
    pub timestamp_ms: i64,
}

/// Token for a Veya-initiated clipboard write (user re-copied from the UI).
///
/// Suppress only the matching write; never ignore all `source == veya` traffic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalClipboardWrite {
    pub expected_sequence: Option<u32>,
    pub hash: String,
}
