//! Shared UI state fed by the capture worker.

use std::sync::{Arc, Mutex};

use veya_core::{FlowEngine, HistoryCard, SourceConfidence};

#[derive(Debug, Clone)]
pub struct CardView {
    pub sequence: u32,
    pub raw_count: usize,
    pub content_preview: String,
    pub full_content: String,
    pub source_app: String,
    pub source_confidence: SourceConfidence,
    pub time_label: String,
    pub used_in: Vec<String>,
    pub has_paste_activity: bool,
}

#[derive(Debug, Clone, Default)]
pub struct UiState {
    pub cards: Vec<CardView>,
    pub selected: Option<u32>,
    pub record_count: usize,
    pub retention_label: String,
    pub tracking: bool,
}

pub type SharedUi = Arc<Mutex<UiState>>;

pub fn snapshot_from_flow(flow: &FlowEngine) -> UiState {
    let cards: Vec<CardView> = flow
        .history_cards()
        .into_iter()
        .map(|c| card_view(&c))
        .collect();
    UiState {
        cards,
        selected: None,
        record_count: flow.len(),
        retention_label: "Local only · 30 day history".to_string(),
        tracking: true,
    }
}

pub fn card_view(c: &HistoryCard<'_>) -> CardView {
    let mut used_in = Vec::new();
    for u in &c.used_in {
        if !used_in.contains(&u.target_app) {
            used_in.push(u.target_app.clone());
        }
    }
    CardView {
        sequence: c.representative_sequence,
        raw_count: c.copy_count,
        content_preview: crate::format::preview_line(c.content_preview),
        full_content: c.content.to_string(),
        source_app: c.source_app.to_string(),
        source_confidence: c.source_confidence,
        time_label: crate::format::time_label(c.first_created_at_ms),
        used_in,
        has_paste_activity: c.has_paste_activity,
    }
}
