//! UI aggregation view. Never merges raw storage events.

use crate::events::SourceConfidence;
use crate::model::ClipboardRecord;

/// Group identical short-window copies for display only.
pub const AGGREGATION_WINDOW_MS: i64 = 5_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsedInSummary {
    pub target_app: String,
    pub method_label: String,
    pub triggered_at_ms: i64,
}

/// One left-column History card (may represent several raw records).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryCard<'a> {
    pub representative_sequence: u32,
    pub raw_sequences: Vec<u32>,
    pub copy_count: usize,
    pub content: &'a str,
    pub content_preview: &'a str,
    pub content_hash: &'a str,
    pub source_app: &'a str,
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
                    && card.source_app == rec.source_app
                    && rec.created_at_ms - card.last_created_at_ms <= AGGREGATION_WINDOW_MS =>
            {
                card.raw_sequences.push(rec.sequence);
                card.copy_count += 1;
                card.last_created_at_ms = rec.created_at_ms;
                for p in &rec.pastes {
                    card.used_in.push(UsedInSummary {
                        target_app: p.target_app.clone(),
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
                        method_label: p.method.label().to_string(),
                        triggered_at_ms: p.triggered_at_ms,
                    });
                }
                cards.push(HistoryCard {
                    representative_sequence: rec.sequence,
                    raw_sequences: vec![rec.sequence],
                    copy_count: 1,
                    content: &rec.content,
                    content_preview: preview(&rec.content, 80),
                    content_hash: &rec.content_hash,
                    source_app: &rec.source_app,
                    source_confidence: rec.source_confidence,
                    first_created_at_ms: rec.created_at_ms,
                    last_created_at_ms: rec.created_at_ms,
                    has_paste_activity: !used_in.is_empty(),
                    used_in,
                });
            }
        }
    }
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
    // Behavior tests live in tests/ against the public FlowEngine API.
}
