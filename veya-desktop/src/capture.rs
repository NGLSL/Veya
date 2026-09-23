//! Shared UI state fed by the capture worker.

use std::sync::{Arc, Mutex};

use veya_core::{ClipboardPayload, FlowEngine, HistoryCard, PasteConfidence, SourceConfidence};
use veya_windows::hotkey::Hotkey;

use crate::format::{self, ContentKind};

#[derive(Debug, Clone)]
pub struct UsedInView {
    pub target_app: String,
    pub target_window: String,
    pub method_label: String,
    pub time_label: String,
}

#[derive(Debug, Clone)]
pub enum CardPayloadView {
    Text,
    Files(Vec<String>),
    Image {
        handle: iced::widget::image::Handle,
        width: u32,
        height: u32,
        encoded_bytes: usize,
    },
}

#[derive(Debug, Clone)]
pub struct CardView {
    pub sequence: u32,
    pub raw_count: usize,
    pub raw_sequences: Vec<u32>,
    pub content_preview: String,
    pub full_content: String,
    pub payload: CardPayloadView,
    pub kind: ContentKind,
    pub pinned: bool,
    pub source_app: String,
    pub source_confidence: SourceConfidence,
    pub source_window: String,
    pub first_ms: i64,
    pub time_full: String,
    pub relative_time: String,
    pub time_range: String,
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
            Retention::Day1 => "1 天",
            Retention::Day7 => "7 天",
            Retention::Day30 => "30 天",
            Retention::Never => "永不",
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
    /// Last tracking toggle requested by the UI and applied by the worker.
    pub tracking_ack: u64,
    pub excluded_apps: Vec<String>,
    pub expanded_raw: bool,
    pub status_note: String,
    pub hotkey_selected: Hotkey,
    pub hotkey_active: Hotkey,
    pub hotkey_error: Option<String>,
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
    let now = format::now_ms();
    let cards: Vec<CardView> = flow
        .history_cards()
        .into_iter()
        .map(|c| card_view(&c, now))
        .collect();
    UiState {
        cards,
        selected: None,
        record_count: flow.len(),
        retention,
        tracking,
        tracking_ack: 0,
        excluded_apps,
        expanded_raw,
        status_note,
        hotkey_selected: Hotkey::default(),
        hotkey_active: Hotkey::Disabled,
        hotkey_error: None,
    }
}

pub fn card_view(c: &HistoryCard<'_>, now_ms: i64) -> CardView {
    let used_in: Vec<UsedInView> = c
        .used_in
        .iter()
        .map(|u| UsedInView {
            target_app: u.target_app.clone(),
            target_window: u.target_window.clone(),
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
    let kind = format::payload_kind(c.payload);
    let payload = match c.payload {
        ClipboardPayload::Text(_) => CardPayloadView::Text,
        ClipboardPayload::Files(paths) => CardPayloadView::Files(paths.clone()),
        ClipboardPayload::Image { png, width, height } => CardPayloadView::Image {
            handle: iced::widget::image::Handle::from_bytes(png.clone()),
            width: *width,
            height: *height,
            encoded_bytes: png.len(),
        },
    };
    CardView {
        sequence: c.representative_sequence,
        raw_count: c.copy_count,
        raw_sequences: c.raw_sequences.clone(),
        content_preview: format::payload_preview(c.payload, c.content_preview),
        full_content: c.content.to_string(),
        payload,
        kind,
        pinned: c.pinned,
        source_app: c.source_app.to_string(),
        source_confidence: c.source_confidence,
        source_window: c.source_window.to_string(),
        first_ms: c.first_created_at_ms,
        time_full: crate::format::full_time_label(c.first_created_at_ms),
        relative_time: crate::format::relative_time(c.first_created_at_ms, now_ms),
        time_range: crate::format::time_range_label(c.first_created_at_ms, c.last_created_at_ms),
        used_in,
        used_in_apps,
        has_paste_activity: c.has_paste_activity,
        paste_detail: PasteConfidence::HotkeyObserved.detail_copy(),
    }
}
