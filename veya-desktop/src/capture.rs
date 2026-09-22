//! Shared UI state fed by the capture worker.

use std::sync::{Arc, Mutex};

use veya_core::{FlowEngine, HistoryCard, PasteConfidence, SourceConfidence};

#[derive(Debug, Clone)]
pub struct UsedInView {
    pub target_app: String,
    pub method_label: String,
    pub time_label: String,
}

#[derive(Debug, Clone)]
pub struct CardView {
    pub sequence: u32,
    pub raw_count: usize,
    pub raw_sequences: Vec<u32>,
    pub content_preview: String,
    pub full_content: String,
    pub source_app: String,
    pub source_confidence: SourceConfidence,
    pub time_label: String,
    pub used_in: Vec<UsedInView>,
    pub used_in_apps: Vec<String>,
    pub has_paste_activity: bool,
    pub paste_detail: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Retention {
    Day1,
    Day7,
    #[default]
    Day30,
    Never,
}

impl Retention {
    pub fn label(self) -> &'static str {
        match self {
            Retention::Day1 => "1 day",
            Retention::Day7 => "7 days",
            Retention::Day30 => "30 days",
            Retention::Never => "Never",
        }
    }

    pub fn as_key(self) -> &'static str {
        match self {
            Retention::Day1 => "day1",
            Retention::Day7 => "day7",
            Retention::Day30 => "day30",
            Retention::Never => "never",
        }
    }

    pub fn from_key(s: &str) -> Self {
        match s {
            "day1" => Retention::Day1,
            "day7" => Retention::Day7,
            "never" => Retention::Never,
            _ => Retention::Day30,
        }
    }

    pub fn cutoff_ms(self, now_ms: i64) -> Option<i64> {
        match self {
            Retention::Day1 => Some(now_ms - 24 * 60 * 60 * 1000),
            Retention::Day7 => Some(now_ms - 7 * 24 * 60 * 60 * 1000),
            Retention::Day30 => Some(now_ms - 30 * 24 * 60 * 60 * 1000),
            Retention::Never => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct UiState {
    pub cards: Vec<CardView>,
    pub selected: Option<u32>,
    pub record_count: usize,
    pub retention: Retention,
    pub tracking: bool,
    pub excluded_apps: Vec<String>,
    pub expanded_raw: bool,
    pub status_note: String,
}

pub type SharedUi = Arc<Mutex<UiState>>;

pub fn snapshot_from_flow(
    flow: &FlowEngine,
    retention: Retention,
    tracking: bool,
    excluded_apps: Vec<String>,
    expanded_raw: bool,
    status_note: String,
) -> UiState {
    let cards: Vec<CardView> = flow
        .history_cards()
        .into_iter()
        .map(|c| card_view(&c))
        .collect();
    UiState {
        cards,
        selected: None,
        record_count: flow.len(),
        retention,
        tracking,
        excluded_apps,
        expanded_raw,
        status_note,
    }
}

pub fn card_view(c: &HistoryCard<'_>) -> CardView {
    let used_in: Vec<UsedInView> = c
        .used_in
        .iter()
        .map(|u| UsedInView {
            target_app: u.target_app.clone(),
            method_label: u.method_label.clone(),
            time_label: crate::format::time_label(u.triggered_at_ms),
        })
        .collect();
    let mut used_in_apps = Vec::new();
    for u in &used_in {
        if !used_in_apps.contains(&u.target_app) {
            used_in_apps.push(u.target_app.clone());
        }
    }
    CardView {
        sequence: c.representative_sequence,
        raw_count: c.copy_count,
        raw_sequences: c.raw_sequences.clone(),
        content_preview: crate::format::preview_line(c.content_preview),
        full_content: c.content.to_string(),
        source_app: c.source_app.to_string(),
        source_confidence: c.source_confidence,
        time_label: crate::format::time_label(c.first_created_at_ms),
        used_in,
        used_in_apps,
        has_paste_activity: c.has_paste_activity,
        paste_detail: PasteConfidence::HotkeyObserved.detail_copy(),
    }
}
