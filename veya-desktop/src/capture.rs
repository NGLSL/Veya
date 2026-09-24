//! Shared UI state fed by the capture worker.

use std::sync::{Arc, Mutex};

use image::ImageFormat;
use veya_core::{ClipboardPayload, HistoryCard, PasteConfidence, SourceConfidence};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HistoryFilter {
    #[default]
    All,
    Text,
    Link,
    Code,
    File,
    Image,
}

impl HistoryFilter {
    pub fn matches(self, kind: ContentKind) -> bool {
        match self {
            Self::All => true,
            Self::Text => kind == ContentKind::Text,
            Self::Link => kind == ContentKind::Link,
            Self::Code => kind == ContentKind::Code,
            Self::File => kind == ContentKind::File,
            Self::Image => kind == ContentKind::Image,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryQuery {
    pub search: String,
    pub filter: HistoryFilter,
    pub pinned_only: bool,
    pub newest_first: bool,
    pub page: usize,
}

impl Default for HistoryQuery {
    fn default() -> Self {
        Self {
            search: String::new(),
            filter: HistoryFilter::All,
            pinned_only: false,
            newest_first: true,
            page: 0,
        }
    }
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
    /// Advances only when the worker publishes a changed snapshot.
    pub revision: u64,
    pub cards: Vec<CardView>,
    pub history_query: HistoryQuery,
    pub history_active: bool,
    pub history_has_next: bool,
    pub modal_image: Option<(u32, iced::widget::image::Handle)>,
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

pub fn card_view_with_cached_image(
    c: &HistoryCard<'_>,
    now_ms: i64,
    cached_image: Option<&iced::widget::image::Handle>,
) -> CardView {
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
            handle: cached_image
                .cloned()
                .unwrap_or_else(|| image_thumbnail_handle(png)),
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
        time_full: crate::format::full_time_label(c.first_created_at_ms),
        relative_time: crate::format::relative_time(c.first_created_at_ms, now_ms),
        time_range: crate::format::time_range_label(c.first_created_at_ms, c.last_created_at_ms),
        used_in,
        used_in_apps,
        has_paste_activity: c.has_paste_activity,
        paste_detail: PasteConfidence::HotkeyObserved.detail_copy(),
    }
}

fn image_thumbnail_handle(png: &[u8]) -> iced::widget::image::Handle {
    const THUMBNAIL_SIDE: u32 = 128;
    match image::load_from_memory_with_format(png, ImageFormat::Png) {
        Ok(decoded) => {
            let thumbnail = decoded
                .thumbnail(THUMBNAIL_SIDE, THUMBNAIL_SIDE)
                .into_rgba8();
            iced::widget::image::Handle::from_rgba(
                thumbnail.width(),
                thumbnail.height(),
                thumbnail.into_raw(),
            )
        }
        Err(_) => iced::widget::image::Handle::from_rgba(1, 1, vec![0, 0, 0, 0]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_image_handle_is_bounded_by_thumbnail_size() {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            400,
            200,
            image::Rgba([20, 40, 60, 255]),
        ));
        let mut png = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
            .unwrap();

        match image_thumbnail_handle(&png) {
            iced::widget::image::Handle::Rgba { width, height, .. } => {
                assert_eq!((width, height), (128, 64));
            }
            _ => panic!("history image should use decoded thumbnail pixels"),
        }
    }
}
