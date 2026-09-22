//! Veya domain core: Clipboard Flow Tracker.
//!
//! Semantics:
//! - A paste observation is a **PasteTrigger** (hotkey intent + foreground app),
//!   not verified insertion into that app.
//! - Source attribution carries **SourceConfidence** (`Exact` / `Likely` / `Unknown`);
//!   inferred sources are never presented as Exact.
//! - Raw clipboard events are never merged in storage; aggregation is a **view**.

pub mod aggregate;
pub mod events;
pub mod flow;
pub mod model;
pub mod search;

pub use aggregate::{HistoryCard, UsedInSummary};
pub use events::{
    ClipboardChange, InternalClipboardWrite, PasteMethod, PasteTrigger, SourceConfidence,
};
pub use flow::{FlowEngine, FlowOutcome};
pub use model::{ClipboardRecord, PasteTriggerRecord};
pub use search::SearchHit;
