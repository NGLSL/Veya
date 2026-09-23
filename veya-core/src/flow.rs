//! Flow use-cases: record clipboard changes and paste triggers.
//!
//! External behavior only — no Win32, no persistence, no UI.

use std::collections::HashMap;

use crate::aggregate::{self, HistoryCard};
use crate::events::{ClipboardChange, InternalClipboardWrite, PasteTrigger};
use crate::model::{ClipboardRecord, PasteTriggerRecord};
use crate::search::{self, SearchHit};

/// What happened after ingesting a platform event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowOutcome {
    Recorded { sequence: u32 },
    DuplicateSequenceIgnored { sequence: u32 },
    SuppressedInternalWrite { sequence: u32 },
    PasteAttached { sequence: u32 },
    PasteWithoutRecord,
}

/// In-memory clipboard flow engine (the single automated test seam).
pub struct FlowEngine {
    records: HashMap<u32, ClipboardRecord>,
    order: Vec<u32>,
    current_seq: Option<u32>,
    last_seen_seq: Option<u32>,
    pending_internal: Option<InternalClipboardWrite>,
    pending_internal_owner_pid: Option<u32>,
}

impl Default for FlowEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl FlowEngine {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
            order: Vec::new(),
            current_seq: None,
            last_seen_seq: None,
            pending_internal: None,
            pending_internal_owner_pid: None,
        }
    }

    /// Arm suppression for the next Veya-initiated clipboard write.
    pub fn begin_internal_write(&mut self, write: InternalClipboardWrite) {
        self.pending_internal = Some(write);
        self.pending_internal_owner_pid = None;
    }

    /// After a successful Windows write, bind suppression to the observed
    /// sequence and our exact clipboard owner. DIB re-encoding can change PNG
    /// bytes without changing the user-visible image.
    pub fn confirm_internal_write(&mut self, sequence: u32, owner_pid: u32) {
        if let Some(pending) = &mut self.pending_internal {
            pending.expected_sequence = Some(sequence);
            self.pending_internal_owner_pid = Some(owner_pid);
        }
    }

    pub fn cancel_internal_write(&mut self) {
        self.pending_internal = None;
        self.pending_internal_owner_pid = None;
    }

    pub fn on_clipboard_change(&mut self, ev: ClipboardChange) -> FlowOutcome {
        if self.last_seen_seq == Some(ev.sequence) {
            return FlowOutcome::DuplicateSequenceIgnored {
                sequence: ev.sequence,
            };
        }
        self.last_seen_seq = Some(ev.sequence);

        if self.matches_pending_internal(&ev) {
            self.pending_internal = None;
            self.pending_internal_owner_pid = None;
            return FlowOutcome::SuppressedInternalWrite {
                sequence: ev.sequence,
            };
        }
        if self
            .pending_internal
            .as_ref()
            .is_some_and(|pending| pending.expected_sequence.is_none())
        {
            // An unsequenced token means "the next matching event". Once a
            // different event arrives it must not suppress a future user copy.
            self.pending_internal = None;
            self.pending_internal_owner_pid = None;
        } else {
            self.expire_stale_pending(ev.sequence);
        }

        let record = ClipboardRecord {
            sequence: ev.sequence,
            content_type: ev.payload.kind().to_string(),
            content: ev.payload.display_text(),
            payload: ev.payload,
            content_hash: ev.content_hash,
            source_app: ev.source_exe,
            source_pid: ev.source_pid,
            source_window: ev.source_window,
            source_confidence: ev.source_confidence,
            created_at_ms: ev.timestamp_ms,
            pinned: false,
            pastes: Vec::new(),
        };
        self.records.insert(ev.sequence, record);
        self.order.push(ev.sequence);
        self.current_seq = Some(ev.sequence);
        FlowOutcome::Recorded {
            sequence: ev.sequence,
        }
    }

    /// A clipboard update that policy deliberately does not retain. Drop the
    /// current paste association so its content cannot be attributed to the
    /// previous recorded clipboard value.
    pub fn on_untracked_clipboard_change(&mut self, sequence: u32) {
        self.last_seen_seq = Some(sequence);
        self.current_seq = None;
        self.pending_internal = None;
        self.pending_internal_owner_pid = None;
    }

    pub fn on_paste_trigger(&mut self, ev: PasteTrigger) -> FlowOutcome {
        let Some(seq) = self.current_seq else {
            return FlowOutcome::PasteWithoutRecord;
        };
        let Some(rec) = self.records.get_mut(&seq) else {
            return FlowOutcome::PasteWithoutRecord;
        };
        rec.pastes.push(PasteTriggerRecord {
            target_app: ev.target_exe,
            target_pid: ev.target_pid,
            target_window: ev.target_window,
            method: ev.method,
            confidence: crate::events::PasteConfidence::HotkeyObserved,
            triggered_at_ms: ev.timestamp_ms,
        });
        FlowOutcome::PasteAttached { sequence: seq }
    }

    pub fn record(&self, sequence: u32) -> Option<&ClipboardRecord> {
        self.records.get(&sequence)
    }

    /// Restore a persisted raw event without recapturing or reinterpreting its payload.
    pub fn restore_record(&mut self, record: ClipboardRecord) {
        if self.records.contains_key(&record.sequence) {
            return;
        }
        self.last_seen_seq = Some(record.sequence);
        // Persisted history alone does not prove which payload is currently on
        // the OS clipboard after a restart or a retention reload.
        self.order.push(record.sequence);
        self.records.insert(record.sequence, record);
    }

    /// Apply one history-card pin action to its raw events. New copies remain unpinned.
    pub fn set_pinned(&mut self, sequences: &[u32], pinned: bool) -> usize {
        let mut changed = 0;
        for sequence in sequences {
            if let Some(record) = self.records.get_mut(sequence) {
                if record.pinned != pinned {
                    record.pinned = pinned;
                    changed += 1;
                }
            }
        }
        changed
    }

    /// Records in insertion order (raw events, never merged).
    pub fn records(&self) -> impl Iterator<Item = &ClipboardRecord> {
        self.order.iter().filter_map(|seq| self.records.get(seq))
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Delete one record and its dependent paste triggers.
    pub fn delete_record(&mut self, sequence: u32) -> bool {
        let removed = self.records.remove(&sequence).is_some();
        if removed {
            self.order.retain(|s| *s != sequence);
            if self.current_seq == Some(sequence) {
                self.current_seq = None;
            }
        }
        removed
    }

    pub fn clear(&mut self) {
        self.records.clear();
        self.order.clear();
        self.current_seq = None;
        self.last_seen_seq = None;
        self.pending_internal = None;
        self.pending_internal_owner_pid = None;
    }

    pub fn search(&self, query: &str) -> Vec<SearchHit<'_>> {
        search::search(self.records(), query)
    }

    /// UI-facing list: raw events grouped for display only.
    pub fn history_cards(&self) -> Vec<HistoryCard<'_>> {
        aggregate::history_cards(self.records())
    }

    fn matches_pending_internal(&self, ev: &ClipboardChange) -> bool {
        let Some(pending) = &self.pending_internal else {
            return false;
        };
        if pending.expected_sequence == Some(ev.sequence)
            && self.pending_internal_owner_pid == Some(ev.source_pid)
            && ev.source_confidence == crate::events::SourceConfidence::Exact
        {
            return true;
        }
        if pending.hash != ev.content_hash {
            return false;
        }
        match pending.expected_sequence {
            None => true,
            Some(expected) => expected == ev.sequence,
        }
    }

    fn expire_stale_pending(&mut self, sequence: u32) {
        if let Some(pending) = &self.pending_internal {
            if let Some(expected) = pending.expected_sequence {
                if sequence > expected {
                    self.pending_internal = None;
                    self.pending_internal_owner_pid = None;
                }
            }
        }
    }
}
