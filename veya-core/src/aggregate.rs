//! UI aggregation view. Never merges raw storage events.

use crate::events::SourceConfidence;
use crate::model::{ClipboardPayload, ClipboardRecord, PasteTriggerRecord};

/// Group identical short-window copies for display only.
pub const AGGREGATION_WINDOW_MS: i64 = 5_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsedInSummary {
    pub target_app: String,
    pub target_window: String,
    pub method_label: String,
    pub triggered_at_ms: i64,
}

/// Search/list payload metadata. Original image bytes stay in storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryPayload {
    Text,
    Files(Vec<String>),
    Image {
        width: u32,
        height: u32,
        encoded_bytes: usize,
    },
}

/// A transient searchable projection of one raw record, without replay bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRecord {
    pub sequence: u32,
    pub content: String,
    pub payload: HistoryPayload,
    pub content_hash: String,
    pub source_app: String,
    pub source_window: String,
    pub source_confidence: SourceConfidence,
    pub created_at_ms: i64,
    pub pinned: bool,
    pub pastes: Vec<PasteTriggerRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistorySummary {
    pub representative: HistoryRecord,
    /// Raw IDs in chronological (timestamp, sequence) order.
    pub raw_sequences: Vec<u32>,
    /// Reverse chronological record order, then reverse paste order per record.
    pub used_in: Vec<UsedInSummary>,
    pub last_created_at_ms: i64,
}

impl HistorySummary {
    pub fn copy_count(&self) -> usize {
        self.raw_sequences.len()
    }
}

/// Aggregates input ordered by (created_at_ms, sequence), in either direction.
/// Keeps one representative's content plus raw IDs and paste summaries per group.
/// Emitted groups follow the input direction; the representative is always oldest.
pub struct HistoryAggregator {
    newest_first: bool,
    current: Option<HistorySummary>,
    adjacent_created_at_ms: i64,
}

impl HistoryAggregator {
    pub fn new(newest_first: bool) -> Self {
        Self {
            newest_first,
            current: None,
            adjacent_created_at_ms: 0,
        }
    }

    pub fn push(&mut self, record: HistoryRecord) -> Option<HistorySummary> {
        let joins = self.current.as_ref().is_some_and(|group| {
            group.representative.content_hash == record.content_hash
                && group.representative.pinned == record.pinned
                && group.representative.source_app == record.source_app
                && self.adjacent_created_at_ms.abs_diff(record.created_at_ms)
                    <= AGGREGATION_WINDOW_MS as u64
        });
        let completed = if joins { None } else { self.finish() };
        self.adjacent_created_at_ms = record.created_at_ms;
        let mut used_in: Vec<_> = record
            .pastes
            .iter()
            .map(|paste| UsedInSummary {
                target_app: paste.target_app.clone(),
                target_window: paste.target_window.clone(),
                method_label: paste.method.label().to_string(),
                triggered_at_ms: paste.triggered_at_ms,
            })
            .collect();
        if self.newest_first {
            used_in.reverse();
        }
        if let Some(group) = &mut self.current {
            group.raw_sequences.push(record.sequence);
            group.used_in.extend(used_in);
            if self.newest_first {
                group.representative = record;
            } else {
                group.last_created_at_ms = record.created_at_ms;
            }
        } else {
            self.current = Some(HistorySummary {
                raw_sequences: vec![record.sequence],
                used_in,
                last_created_at_ms: record.created_at_ms,
                representative: record,
            });
        }
        completed
    }

    pub fn finish(&mut self) -> Option<HistorySummary> {
        self.current.take().map(|mut group| {
            if self.newest_first {
                group.raw_sequences.reverse();
            } else {
                group.used_in.reverse();
            }
            group
        })
    }
}

/// One left-column History card (may represent several raw records).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryCard<'a> {
    pub representative_sequence: u32,
    pub raw_sequences: Vec<u32>,
    pub copy_count: usize,
    pub content: &'a str,
    pub payload: &'a ClipboardPayload,
    pub content_preview: &'a str,
    pub content_hash: &'a str,
    pub pinned: bool,
    pub source_app: &'a str,
    pub source_window: &'a str,
    pub source_confidence: SourceConfidence,
    pub first_created_at_ms: i64,
    pub last_created_at_ms: i64,
    pub used_in: Vec<UsedInSummary>,
    pub has_paste_activity: bool,
}

pub fn history_cards<'a>(
    records: impl Iterator<Item = &'a ClipboardRecord>,
) -> Vec<HistoryCard<'a>> {
    let mut sorted: Vec<&ClipboardRecord> = records.collect();
    sorted.sort_by_key(|r| (r.created_at_ms, r.sequence));

    let mut cards: Vec<HistoryCard<'_>> = Vec::new();
    for rec in sorted {
        match cards.last_mut() {
            Some(card)
                if card.content_hash == rec.content_hash
                    && card.pinned == rec.pinned
                    && card.source_app == rec.source_app
                    && rec.created_at_ms - card.last_created_at_ms <= AGGREGATION_WINDOW_MS =>
            {
                card.raw_sequences.push(rec.sequence);
                card.copy_count += 1;
                card.last_created_at_ms = rec.created_at_ms;
                for p in &rec.pastes {
                    card.used_in.push(UsedInSummary {
                        target_app: p.target_app.clone(),
                        target_window: p.target_window.clone(),
                        method_label: p.method.label().to_string(),
                        triggered_at_ms: p.triggered_at_ms,
                    });
                    card.has_paste_activity = true;
                }
            }
            _ => {
                let mut used_in = Vec::new();
                for p in &rec.pastes {
                    used_in.push(UsedInSummary {
                        target_app: p.target_app.clone(),
                        target_window: p.target_window.clone(),
                        method_label: p.method.label().to_string(),
                        triggered_at_ms: p.triggered_at_ms,
                    });
                }
                cards.push(HistoryCard {
                    representative_sequence: rec.sequence,
                    raw_sequences: vec![rec.sequence],
                    copy_count: 1,
                    content: &rec.content,
                    payload: &rec.payload,
                    content_preview: preview(&rec.content, 80),
                    content_hash: &rec.content_hash,
                    pinned: rec.pinned,
                    source_app: &rec.source_app,
                    source_window: &rec.source_window,
                    source_confidence: rec.source_confidence,
                    first_created_at_ms: rec.created_at_ms,
                    last_created_at_ms: rec.created_at_ms,
                    has_paste_activity: !used_in.is_empty(),
                    used_in,
                });
            }
        }
    }
    // UI 是「最新优先」：聚合按时间正序完成后再反转卡片与使用记录。
    for card in &mut cards {
        card.used_in.reverse();
    }
    cards.reverse();
    cards
}

fn preview(text: &str, max_chars: usize) -> &str {
    let mut end = 0;
    let mut count = 0;
    for (idx, ch) in text.char_indices() {
        if count == max_chars {
            return &text[..idx];
        }
        count += 1;
        end = idx + ch.len_utf8();
    }
    let _ = end;
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{PasteConfidence, PasteMethod};

    fn record(sequence: u32, created_at_ms: i64) -> ClipboardRecord {
        ClipboardRecord {
            sequence,
            content_type: "text".into(),
            content: "same content".into(),
            payload: ClipboardPayload::Text("same content".into()),
            content_hash: "same hash".into(),
            source_app: "editor.exe".into(),
            source_pid: 1,
            source_window: format!("window {sequence}"),
            source_confidence: if sequence == 1 {
                SourceConfidence::Unknown
            } else {
                SourceConfidence::Exact
            },
            created_at_ms,
            pinned: false,
            pastes: vec![
                PasteTriggerRecord {
                    target_app: format!("target {sequence}"),
                    target_pid: 2,
                    target_window: "first".into(),
                    method: PasteMethod::CtrlV,
                    confidence: PasteConfidence::HotkeyObserved,
                    // Deliberately not sorted: legacy order is observation order.
                    triggered_at_ms: 999_999,
                },
                PasteTriggerRecord {
                    target_app: format!("target {sequence}"),
                    target_pid: 2,
                    target_window: "second".into(),
                    method: PasteMethod::ShiftInsert,
                    confidence: PasteConfidence::HotkeyObserved,
                    triggered_at_ms: 1,
                },
            ],
        }
    }

    fn projection(record: &ClipboardRecord) -> HistoryRecord {
        HistoryRecord {
            sequence: record.sequence,
            content: record.content.clone(),
            payload: match &record.payload {
                ClipboardPayload::Text(_) => HistoryPayload::Text,
                ClipboardPayload::Files(paths) => HistoryPayload::Files(paths.clone()),
                ClipboardPayload::Image { png, width, height } => HistoryPayload::Image {
                    width: *width,
                    height: *height,
                    encoded_bytes: png.len(),
                },
            },
            content_hash: record.content_hash.clone(),
            source_app: record.source_app.clone(),
            source_window: record.source_window.clone(),
            source_confidence: record.source_confidence,
            created_at_ms: record.created_at_ms,
            pinned: record.pinned,
            pastes: record.pastes.clone(),
        }
    }

    fn assert_matches_legacy(records: &[ClipboardRecord], newest_first: bool) {
        let mut ordered: Vec<_> = records.iter().collect();
        ordered.sort_by_key(|record| (record.created_at_ms, record.sequence));
        if newest_first {
            ordered.reverse();
        }
        let mut aggregator = HistoryAggregator::new(newest_first);
        let mut summaries: Vec<_> = ordered
            .into_iter()
            .filter_map(|record| aggregator.push(projection(record)))
            .collect();
        summaries.extend(aggregator.finish());
        assert!(aggregator.finish().is_none());
        if !newest_first {
            summaries.reverse();
        }
        let legacy = history_cards(records.iter());
        assert_eq!(summaries.len(), legacy.len());
        for (summary, card) in summaries.iter().zip(legacy) {
            assert_eq!(
                summary.representative.sequence,
                card.representative_sequence
            );
            assert_eq!(summary.raw_sequences, card.raw_sequences);
            assert_eq!(summary.copy_count(), card.copy_count);
            assert_eq!(summary.representative.content, card.content);
            assert_eq!(summary.representative.content_hash, card.content_hash);
            assert_eq!(summary.representative.pinned, card.pinned);
            assert_eq!(summary.representative.source_app, card.source_app);
            assert_eq!(summary.representative.source_window, card.source_window);
            assert_eq!(
                summary.representative.source_confidence,
                card.source_confidence
            );
            assert_eq!(
                summary.representative.created_at_ms,
                card.first_created_at_ms
            );
            assert_eq!(summary.last_created_at_ms, card.last_created_at_ms);
            assert_eq!(summary.used_in, card.used_in);
        }
    }

    #[test]
    fn streaming_preserves_legacy_order_confidence_and_adjacent_gap_in_both_directions() {
        // A group may span more than 5 seconds, provided every adjacent gap fits.
        let records = vec![record(3, 10_000), record(1, 0), record(2, 5_000)];
        assert_matches_legacy(&records, true);
        assert_matches_legacy(&records, false);
        assert_eq!(history_cards(records.iter())[0].raw_sequences, [1, 2, 3]);
    }

    #[test]
    fn streaming_preserves_equal_timestamp_sequence_order_and_group_boundaries() {
        let mut records = vec![record(2, 0), record(1, 0), record(3, 5_001)];
        let mut pinned = record(4, 5_002);
        pinned.pinned = true;
        records.push(pinned);
        let mut source = record(5, 5_003);
        source.source_app = "other.exe".into();
        records.push(source);
        let mut hash = record(6, 5_004);
        hash.content_hash = "other hash".into();
        records.push(hash);
        let mut image = record(7, 5_005);
        image.content_hash = "image hash".into();
        image.payload = ClipboardPayload::Image {
            png: vec![1; 16],
            width: 2,
            height: 2,
        };
        records.push(image);
        let mut files = record(8, 5_006);
        files.content_hash = "file hash".into();
        files.payload = ClipboardPayload::Files(vec!["C:\\example.txt".into()]);
        records.push(files);
        assert_matches_legacy(&records, true);
        assert_matches_legacy(&records, false);
        assert_eq!(history_cards(records.iter()).len(), 7);
    }

    #[test]
    fn long_group_retains_only_the_oldest_representative_content() {
        let mut aggregator = HistoryAggregator::new(true);
        for sequence in (1..=2_000).rev() {
            let mut projected = projection(&record(sequence, i64::from(sequence)));
            projected.content = format!("{sequence}:{}", "x".repeat(8_192));
            projected.pastes.clear();
            assert!(aggregator.push(projected).is_none());
        }
        let summary = aggregator.finish().unwrap();
        assert_eq!(summary.copy_count(), 2_000);
        assert_eq!(summary.raw_sequences, (1..=2_000).collect::<Vec<_>>());
        assert!(summary.representative.content.starts_with("1:"));
        assert_eq!(summary.last_created_at_ms, 2_000);
        assert!(summary.used_in.is_empty());
    }

    #[test]
    fn empty_stream_and_timestamp_extremes_do_not_overflow() {
        let mut aggregator = HistoryAggregator::new(false);
        assert!(aggregator.finish().is_none());
        assert!(aggregator.push(projection(&record(1, i64::MIN))).is_none());
        assert!(aggregator.push(projection(&record(2, i64::MAX))).is_some());
        assert_eq!(aggregator.finish().unwrap().representative.sequence, 2);
    }
}
