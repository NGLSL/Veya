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

    /// Display label for a source app. Inference marks live here only — never in stored exe names.
    pub fn display_source(self, source_app: &str) -> String {
        match self {
            SourceConfidence::Exact => source_app.to_string(),
            SourceConfidence::Likely => format!("{source_app}（疑似）"),
            SourceConfidence::Unknown => {
                if source_app == "unknown" {
                    "未知来源".to_string()
                } else {
                    format!("{source_app}（不确定）")
                }
            }
        }
    }

    /// User-facing explanation when attribution is not Exact.
    pub fn hint(self) -> &'static str {
        match self {
            SourceConfidence::Exact => "",
            SourceConfidence::Likely => "来源根据前台应用推断，剪贴板所有者不可用。",
            SourceConfidence::Unknown => "无法确定来源应用。",
        }
    }
}

/// What a paste observation claims. Insertion into the target is never verified in v0.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteConfidence {
    /// Hotkey + foreground app observed; insertion not verified.
    HotkeyObserved,
}

impl PasteConfidence {
    pub fn as_str(self) -> &'static str {
        match self {
            PasteConfidence::HotkeyObserved => "hotkey-observed",
        }
    }

    pub fn detail_copy(self) -> &'static str {
        match self {
            PasteConfidence::HotkeyObserved => "检测到粘贴触发 — 未验证是否已插入目标应用",
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

/// One supported clipboard payload (enriched by the platform layer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardChange {
    pub sequence: u32,
    pub payload: crate::model::ClipboardPayload,
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
/// `expected_sequence: None` means the next event with the same content hash;
/// the token expires when a different clipboard event arrives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalClipboardWrite {
    pub expected_sequence: Option<u32>,
    pub hash: String,
}
