//! Capture worker: platform events → enrich → FlowEngine → SQLite + UI snapshot.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};

use veya_core::{
    aggregate, payload_hash, ClipboardPayload, ClipboardRecord, FlowEngine, InternalClipboardWrite,
    PasteConfidence,
};
use veya_storage::Store;
use veya_windows::hotkey::Hotkey;
use veya_windows::platform::{enrich_clipboard, enrich_paste, PlatformEvent, WindowSignal};
use veya_windows::write_payload;

use crate::capture::{
    card_view_with_cached_image, CardPayloadView, CardView, HistoryQuery, Retention, SharedUi,
    UiState,
};
use crate::format::{self, now_ms};

const HISTORY_PAGE_SIZE: usize = 25;
const HISTORY_READ_BATCH: usize = 16;

pub enum WorkerCmd {
    Recopy { sequence: u32 },
    RecopyText { text: String },
    ClearHistory,
    SetTracking { on: bool, request_id: Option<u64> },
    SetRetention(Retention),
    ExcludeApp { exe: String },
    UnexcludeApp { exe: String },
    DeleteRecord { sequences: Vec<u32> },
    SetPinned { sequences: Vec<u32>, pinned: bool },
    SetExpandedRaw(bool),
    SetHotkey(Hotkey),
    QueryHistory(HistoryQuery),
    SetHistoryActive(bool),
    LoadFullImage(u32),
    UnloadFullImage,
}

pub struct WorkerHandle {
    pub cmd_tx: Sender<WorkerCmd>,
}

pub fn spawn_worker(shared: SharedUi, activate_tx: Sender<WindowSignal>) -> WorkerHandle {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<WorkerCmd>();
    std::thread::Builder::new()
        .name("veya-capture".into())
        .spawn(move || run_worker(shared, cmd_rx, activate_tx))
        .expect("spawn capture worker");
    WorkerHandle { cmd_tx }
}

fn db_path() -> std::path::PathBuf {
    std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("Veya")
        .join("veya.db")
}

fn write_internal_payload(
    flow: &mut FlowEngine,
    payload: &ClipboardPayload,
    writer: impl FnOnce(&ClipboardPayload) -> Result<u32, String>,
) -> Result<(), String> {
    let hash = payload_hash(payload);
    // Arm before touching the clipboard. Windows may publish the update with
    // a sequence newer than the value observed inside write_unicode_text.
    flow.begin_internal_write(InternalClipboardWrite {
        expected_sequence: None,
        hash,
    });
    match writer(payload) {
        Ok(sequence) => {
            flow.confirm_internal_write(sequence, std::process::id());
            Ok(())
        }
        Err(error) => {
            flow.cancel_internal_write();
            Err(error)
        }
    }
}

fn same_history_group(left: &ClipboardRecord, right: &ClipboardRecord) -> bool {
    left.content_hash == right.content_hash
        && left.pinned == right.pinned
        && left.source_app == right.source_app
        && left.created_at_ms.abs_diff(right.created_at_ms)
            <= aggregate::AGGREGATION_WINDOW_MS as u64
}

fn load_history_page(
    store: &Store,
    query: &HistoryQuery,
    cached_images: &HashMap<u32, iced::widget::image::Handle>,
) -> Result<(Vec<CardView>, bool), String> {
    let mut cursor = None;
    let mut group = Vec::<ClipboardRecord>::new();
    let mut cards = Vec::with_capacity(HISTORY_PAGE_SIZE);
    let mut matched = 0usize;
    let skip = query.page.saturating_mul(HISTORY_PAGE_SIZE);
    let now = now_ms();

    let mut finish_group = |group: &mut Vec<ClipboardRecord>| -> bool {
        if group.is_empty() {
            return false;
        }
        let aggregated = aggregate::history_cards(group.iter());
        let card = &aggregated[0];
        let matches = query.filter.matches(format::payload_kind(card.payload))
            && (!query.pinned_only || card.pinned)
            && veya_core::match_field(
                card.content,
                card.source_app,
                card.used_in
                    .iter()
                    .map(|use_record| use_record.target_app.as_str()),
                &query.search,
            )
            .is_some();
        if matches {
            if matched >= skip.saturating_add(HISTORY_PAGE_SIZE) {
                return true;
            }
            if matched >= skip {
                cards.push(card_view_with_cached_image(
                    card,
                    now,
                    cached_images.get(&card.representative_sequence),
                ));
            }
            matched += 1;
        }
        group.clear();
        false
    };

    loop {
        let batch = store
            .load_records_page(cursor, HISTORY_READ_BATCH, query.newest_first)
            .map_err(|error| format!("读取历史失败：{error}"))?;
        if batch.is_empty() {
            break;
        }
        let count = batch.len();
        for record in batch {
            cursor = Some((record.created_at_ms, record.sequence));
            if group
                .last()
                .is_some_and(|last| !same_history_group(last, &record))
                && finish_group(&mut group)
            {
                return Ok((cards, true));
            }
            group.push(record);
        }
        if count < HISTORY_READ_BATCH {
            break;
        }
    }
    let has_next = finish_group(&mut group);
    Ok((cards, has_next))
}

fn discard_inactive_records(flow: &mut FlowEngine, keep: Option<u32>) {
    let stale: Vec<_> = flow
        .records()
        .map(|record| record.sequence)
        .filter(|sequence| Some(*sequence) != keep)
        .collect();
    for sequence in stale {
        flow.delete_record(sequence);
    }
}

fn discard_purged_record(store: &Store, flow: &mut FlowEngine) {
    let current = flow.records().next().map(|record| record.sequence);
    if let Some(sequence) = current {
        if matches!(store.load_record(sequence), Ok(None)) {
            flow.delete_record(sequence);
        }
    }
}

fn publish(
    shared: &SharedUi,
    store: &Store,
    flow: &FlowEngine,
    history_query: &HistoryQuery,
    history_active: bool,
    modal_image: &Option<(u32, iced::widget::image::Handle)>,
    tracking: bool,
    tracking_ack: u64,
    retention: Retention,
    excluded_apps: Vec<String>,
    expanded_raw: bool,
    status_note: String,
    hotkey_selected: Hotkey,
    hotkey_active: Hotkey,
    hotkey_error: Option<String>,
) {
    let (cards, history_has_next, status_note) = if history_active {
        let cached_images = shared
            .lock()
            .map(|guard| {
                guard
                    .cards
                    .iter()
                    .filter_map(|card| match &card.payload {
                        CardPayloadView::Image { handle, .. } => {
                            Some((card.sequence, handle.clone()))
                        }
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        match load_history_page(store, history_query, &cached_images) {
            Ok((cards, has_next)) => (cards, has_next, status_note),
            Err(error) => (Vec::new(), false, error),
        }
    } else {
        (Vec::new(), false, status_note)
    };
    let mut snap = UiState {
        revision: 0,
        cards,
        history_query: history_query.clone(),
        history_active,
        history_has_next,
        modal_image: modal_image.clone(),
        selected: None,
        record_count: store.count_records().unwrap_or(flow.len()),
        retention,
        tracking,
        tracking_ack,
        excluded_apps,
        expanded_raw,
        status_note,
        hotkey_selected,
        hotkey_active,
        hotkey_error,
    };
    if let Ok(mut guard) = shared.lock() {
        snap.revision = guard.revision.wrapping_add(1);
        snap.selected = guard.selected;
        *guard = snap;
    }
}

fn base_exe(name: &str) -> &str {
    name.split(" (").next().unwrap_or(name)
}

fn exe_key(name: &str) -> String {
    base_exe(name).trim().to_lowercase()
}

fn is_excluded_source(excluded_apps: &[String], source_exe: &str) -> bool {
    let key = exe_key(source_exe);
    !key.is_empty() && excluded_apps.iter().any(|exe| exe_key(exe) == key)
}

fn exclude_app(
    store: &mut Store,
    excluded_apps: &mut Vec<String>,
    name: &str,
) -> Result<String, String> {
    let exe = exe_key(name);
    if exe.is_empty() {
        return Err("应用文件名不能为空".into());
    }
    if !is_excluded_source(excluded_apps, &exe) {
        store
            .upsert_application(&exe, &exe, "", true)
            .map_err(|error| format!("保存排除设置失败：{error}"))?;
        excluded_apps.push(exe.clone());
    }
    Ok(exe)
}

fn unexclude_app(
    store: &mut Store,
    excluded_apps: &mut Vec<String>,
    name: &str,
) -> Result<String, String> {
    let exe = exe_key(name);
    if exe.is_empty() {
        return Err("应用文件名不能为空".into());
    }
    // Older databases can contain mixed-case keys, including several spellings
    // of the same Windows executable. Clear every persisted matching entry.
    let matching: Vec<_> = excluded_apps
        .iter()
        .filter(|existing| exe_key(existing) == exe)
        .cloned()
        .collect();
    for existing in matching {
        store
            .upsert_application(&existing, &existing, "", false)
            .map_err(|error| format!("取消排除失败：{error}"))?;
    }
    excluded_apps.retain(|existing| exe_key(existing) != exe);
    Ok(exe)
}

fn run_worker(shared: SharedUi, cmd_rx: Receiver<WorkerCmd>, activate_tx: Sender<WindowSignal>) {
    #[cfg(not(windows))]
    {
        let _ = (shared, cmd_rx, activate_tx);
        return;
    }

    #[cfg(windows)]
    {
        let (tx, rx) = std::sync::mpsc::channel::<PlatformEvent>();

        let store_path = db_path();
        if let Some(parent) = store_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut store = Store::open(&store_path).expect("open sqlite store");
        let mut flow = FlowEngine::new();
        let mut history_query = HistoryQuery::default();
        let mut history_active = false;
        let mut modal_image = None;
        let mut tracking = true;
        let mut tracking_ack = 0;
        let mut retention = store
            .get_setting("retention")
            .ok()
            .flatten()
            .map(|s| Retention::from_key(&s))
            .unwrap_or_default();
        let mut expanded_raw = false;
        let mut status_note = String::new();
        let mut excluded_apps = store.list_excluded().unwrap_or_default();
        let stored_hotkey = store.get_setting("window_hotkey").ok().flatten();
        let mut invalid_hotkey_setting = stored_hotkey
            .as_deref()
            .is_some_and(|value| Hotkey::parse(value).is_none());
        let mut hotkey_selected = stored_hotkey
            .as_deref()
            .and_then(Hotkey::parse)
            .unwrap_or_default();
        let mut hotkey_active = Hotkey::Disabled;
        let mut hotkey_error = None;
        let mut last_purge_ms = 0i64;

        publish(
            &shared,
            &store,
            &flow,
            &history_query,
            history_active,
            &modal_image,
            tracking,
            tracking_ack,
            retention,
            excluded_apps.clone(),
            expanded_raw,
            status_note.clone(),
            hotkey_selected,
            hotkey_active,
            hotkey_error.clone(),
        );

        let pump = std::thread::Builder::new()
            .name("veya-win-pump".into())
            .spawn(move || {
                if let Err(e) = veya_windows::run(tx, hotkey_selected) {
                    eprintln!("platform error: {e}");
                }
            })
            .expect("spawn win pump");

        loop {
            let mut dirty = false;
            while let Ok(cmd) = cmd_rx.try_recv() {
                dirty = true;
                match cmd {
                    WorkerCmd::Recopy { sequence } => {
                        let payload = flow
                            .record(sequence)
                            .map(|record| record.payload.clone())
                            .or_else(|| {
                                store
                                    .load_record(sequence)
                                    .ok()
                                    .flatten()
                                    .map(|record| record.payload)
                            });
                        status_note = match payload {
                            Some(payload) => {
                                match write_internal_payload(&mut flow, &payload, write_payload) {
                                    Ok(()) => "已复制到剪贴板".into(),
                                    Err(error) => format!("复制失败：{error}"),
                                }
                            }
                            None => "记录已不存在".into(),
                        };
                    }
                    WorkerCmd::RecopyText { text } => {
                        let payload = ClipboardPayload::Text(text);
                        status_note =
                            match write_internal_payload(&mut flow, &payload, write_payload) {
                                Ok(()) => "已复制为纯文本".into(),
                                Err(error) => format!("复制失败：{error}"),
                            };
                    }
                    WorkerCmd::ClearHistory => {
                        status_note = match store.clear() {
                            Ok(()) => {
                                flow.clear();
                                modal_image = None;
                                history_query.page = 0;
                                "历史已清空".into()
                            }
                            Err(error) => format!("清空历史失败：{error}"),
                        };
                    }
                    WorkerCmd::SetTracking { on, request_id } => {
                        tracking = on;
                        if let Some(request_id) = request_id {
                            tracking_ack = request_id;
                        }
                        veya_windows::set_tray_paused(!on);
                        status_note = if on {
                            "已恢复记录".into()
                        } else {
                            "已暂停记录".into()
                        };
                    }
                    WorkerCmd::SetRetention(r) => {
                        retention = r;
                        let _ = store.set_setting("retention", r.as_key());
                        if let Some(cutoff) = retention.cutoff_ms(now_ms()) {
                            let _ = store.purge_older_than(cutoff);
                            discard_purged_record(&store, &mut flow);
                        }
                        status_note = format!("保留期已设为 {}", r.label());
                    }
                    WorkerCmd::ExcludeApp { exe } => {
                        status_note = match exclude_app(&mut store, &mut excluded_apps, &exe) {
                            Ok(exe) => format!("已排除 {exe} 的后续复制"),
                            Err(error) => error,
                        };
                    }
                    WorkerCmd::UnexcludeApp { exe } => {
                        status_note = match unexclude_app(&mut store, &mut excluded_apps, &exe) {
                            Ok(exe) => format!("已取消排除 {exe}"),
                            Err(error) => error,
                        };
                    }
                    WorkerCmd::DeleteRecord { sequences } => {
                        if modal_image
                            .as_ref()
                            .is_some_and(|(sequence, _)| sequences.contains(sequence))
                        {
                            modal_image = None;
                        }
                        status_note = match store.delete_records(&sequences) {
                            Ok(_) => {
                                for sequence in sequences {
                                    flow.delete_record(sequence);
                                }
                                "记录已删除".into()
                            }
                            Err(error) => format!("删除失败：{error}"),
                        };
                    }
                    WorkerCmd::SetPinned { sequences, pinned } => {
                        status_note = match store.set_pinned(&sequences, pinned) {
                            Ok(_) => {
                                flow.set_pinned(&sequences, pinned);
                                if pinned {
                                    "已固定记录"
                                } else {
                                    "已取消固定"
                                }
                                .into()
                            }
                            Err(error) => format!("固定状态保存失败：{error}"),
                        };
                    }
                    WorkerCmd::SetExpandedRaw(on) => {
                        expanded_raw = on;
                    }
                    WorkerCmd::QueryHistory(query) => {
                        history_query = query;
                    }
                    WorkerCmd::SetHistoryActive(active) => {
                        history_active = active;
                        if !active {
                            modal_image = None;
                        }
                    }
                    WorkerCmd::LoadFullImage(sequence) => {
                        modal_image =
                            store
                                .load_record(sequence)
                                .ok()
                                .flatten()
                                .and_then(|record| match record.payload {
                                    ClipboardPayload::Image { png, .. } => Some((
                                        sequence,
                                        iced::widget::image::Handle::from_bytes(png),
                                    )),
                                    _ => None,
                                });
                    }
                    WorkerCmd::UnloadFullImage => {
                        modal_image = None;
                    }
                    WorkerCmd::SetHotkey(selection) => {
                        if !veya_windows::platform::request_hotkey_change(selection) {
                            hotkey_error = Some("快捷键服务尚未就绪，请稍后重试".into());
                            veya_windows::platform::request_hotkey_recording(false);
                        }
                    }
                }
            }

            // Periodic retention purge (default 30 days; honors setting).
            let now = now_ms();
            if now - last_purge_ms > 60_000 {
                last_purge_ms = now;
                if let Some(cutoff) = retention.cutoff_ms(now) {
                    let removed = store.purge_older_than(cutoff).unwrap_or(0);
                    if removed > 0 {
                        discard_purged_record(&store, &mut flow);
                        dirty = true;
                    }
                }
            }

            if dirty {
                publish(
                    &shared,
                    &store,
                    &flow,
                    &history_query,
                    history_active,
                    &modal_image,
                    tracking,
                    tracking_ack,
                    retention,
                    excluded_apps.clone(),
                    expanded_raw,
                    status_note.clone(),
                    hotkey_selected,
                    hotkey_active,
                    hotkey_error.clone(),
                );
            }

            let event_received = match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(ev) => {
                    match ev {
                        PlatformEvent::ToggleWindow { visible } => {
                            let _ = activate_tx.send(WindowSignal::Hotkey { visible });
                        }
                        PlatformEvent::HotkeyStatus {
                            requested,
                            active,
                            error,
                        } => {
                            hotkey_active = active;
                            hotkey_error = error;
                            if hotkey_error.is_none() {
                                hotkey_selected = requested;
                                status_note = if requested == Hotkey::Disabled {
                                    "窗口快捷键已关闭".into()
                                } else {
                                    format!("窗口快捷键已设为 {requested}")
                                };
                                if let Err(error) =
                                    store.set_setting("window_hotkey", &requested.as_key())
                                {
                                    hotkey_error = Some(format!(
                                        "{requested} 已生效，但保存失败；重启后可能恢复旧设置：{error}"
                                    ));
                                } else if invalid_hotkey_setting {
                                    hotkey_error =
                                        Some("保存的窗口快捷键无效，已恢复默认 Alt+V".into());
                                }
                                invalid_hotkey_setting = false;
                            }
                            if let Some(message) = &hotkey_error {
                                status_note = message.clone();
                            }
                        }
                        PlatformEvent::ClipboardChange(raw) if tracking => {
                            let sequence = raw.sequence;
                            match enrich_clipboard(raw) {
                                Some(change) => {
                                    if is_excluded_source(&excluded_apps, &change.source_exe) {
                                        flow.on_untracked_clipboard_change(change.sequence);
                                        discard_inactive_records(&mut flow, None);
                                    } else {
                                        let seq = change.sequence;
                                        let outcome = flow.on_clipboard_change(change);
                                        if matches!(
                                            outcome,
                                            veya_core::FlowOutcome::Recorded { .. }
                                        ) {
                                            if let Some(rec) = flow.record(seq) {
                                                if let Err(error) = store.insert_record(rec) {
                                                    flow.delete_record(seq);
                                                    status_note = format!("记录保存失败：{error}");
                                                }
                                            }
                                        }
                                        discard_inactive_records(&mut flow, Some(seq));
                                    }
                                }
                                None => {
                                    flow.on_untracked_clipboard_change(sequence);
                                    discard_inactive_records(&mut flow, None);
                                    status_note = "剪贴板内容无法读取或超出大小上限，已跳过".into();
                                }
                            }
                        }
                        PlatformEvent::ClipboardSkipped { sequence } if tracking => {
                            flow.on_untracked_clipboard_change(sequence);
                            discard_inactive_records(&mut flow, None);
                            status_note = "该剪贴板格式暂不支持或无法读取，已跳过".into();
                        }
                        PlatformEvent::PasteTrigger(raw) if tracking => {
                            let trigger = enrich_paste(raw);
                            let outcome = flow.on_paste_trigger(trigger);
                            if let veya_core::FlowOutcome::PasteAttached { sequence } = outcome {
                                if let Some(rec) = flow.record(sequence) {
                                    if let Err(error) = store.insert_record(rec) {
                                        status_note = format!("粘贴记录保存失败：{error}");
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    true
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => false,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };

            if event_received {
                publish(
                    &shared,
                    &store,
                    &flow,
                    &history_query,
                    history_active,
                    &modal_image,
                    tracking,
                    tracking_ack,
                    retention,
                    excluded_apps.clone(),
                    expanded_raw,
                    status_note.clone(),
                    hotkey_selected,
                    hotkey_active,
                    hotkey_error.clone(),
                );
            }
        }

        let _ = pump.join();
        let _ = PasteConfidence::HotkeyObserved;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::HistoryFilter;
    use veya_core::{
        ClipboardChange, FlowOutcome, PasteConfidence, PasteMethod, PasteTriggerRecord,
        SourceConfidence,
    };

    fn text_record(sequence: u32, text: &str, created_at_ms: i64) -> ClipboardRecord {
        let payload = ClipboardPayload::Text(text.to_string());
        ClipboardRecord {
            sequence,
            content_type: "text".into(),
            content: text.into(),
            content_hash: payload_hash(&payload),
            payload,
            source_app: "test.exe".into(),
            source_pid: 1,
            source_window: String::new(),
            source_confidence: SourceConfidence::Exact,
            created_at_ms,
            pinned: false,
            pastes: Vec::new(),
        }
    }

    #[test]
    fn history_paging_searches_all_records_without_retaining_all_cards() {
        let mut store = Store::open_in_memory().unwrap();
        for sequence in 1..=30 {
            store
                .insert_record(&text_record(
                    sequence,
                    &format!("entry-{sequence:02}-X"),
                    i64::from(sequence) * 10_000,
                ))
                .unwrap();
        }

        let mut query = HistoryQuery::default();
        let (first, has_next) = load_history_page(&store, &query, &HashMap::new()).unwrap();
        assert_eq!(first.len(), HISTORY_PAGE_SIZE);
        assert_eq!(first.first().unwrap().sequence, 30);
        assert_eq!(first.last().unwrap().sequence, 6);
        assert!(has_next);

        query.page = 1;
        let (second, has_next) = load_history_page(&store, &query, &HashMap::new()).unwrap();
        assert_eq!(second.len(), 5);
        assert_eq!(second.first().unwrap().sequence, 5);
        assert_eq!(second.last().unwrap().sequence, 1);
        assert!(!has_next);

        query.page = 0;
        query.search = "entry-03-X".into();
        let (matches, has_next) = load_history_page(&store, &query, &HashMap::new()).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].sequence, 3);
        assert!(!has_next);

        query.search.clear();
        query.newest_first = false;
        let (oldest, has_next) = load_history_page(&store, &query, &HashMap::new()).unwrap();
        assert_eq!(oldest.first().unwrap().sequence, 1);
        assert_eq!(oldest.last().unwrap().sequence, 25);
        assert!(has_next);
    }

    #[test]
    fn inactive_history_releases_the_published_page() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .insert_record(&text_record(1, "saved", 10_000))
            .unwrap();
        let shared = SharedUi::default();
        let flow = FlowEngine::new();
        let query = HistoryQuery::default();
        let publish_with_active = |active| {
            publish(
                &shared,
                &store,
                &flow,
                &query,
                active,
                &None,
                true,
                0,
                Retention::default(),
                Vec::new(),
                false,
                String::new(),
                Hotkey::default(),
                Hotkey::default(),
                None,
            );
        };

        publish_with_active(true);
        assert_eq!(shared.lock().unwrap().cards.len(), 1);
        publish_with_active(false);
        let snap = shared.lock().unwrap();
        assert!(!snap.history_active);
        assert!(snap.cards.is_empty());
        assert_eq!(snap.record_count, 1);
    }

    #[test]
    fn history_group_survives_storage_batch_boundary() {
        let mut store = Store::open_in_memory().unwrap();
        for sequence in 1..=18 {
            let grouped = (2..=4).contains(&sequence);
            let mut record = text_record(
                sequence,
                if grouped { "repeated" } else { "other" },
                if grouped {
                    20_000 + i64::from(sequence) * 100
                } else {
                    i64::from(sequence) * 10_000
                },
            );
            if !grouped {
                record.content_hash = format!("unique-{sequence}");
            }
            store.insert_record(&record).unwrap();
        }

        let (cards, has_next) =
            load_history_page(&store, &HistoryQuery::default(), &HashMap::new()).unwrap();
        let grouped = cards.iter().find(|card| card.sequence == 2).unwrap();
        assert_eq!(grouped.raw_count, 3);
        assert_eq!(grouped.raw_sequences, vec![2, 3, 4]);
        assert!(!has_next);
    }

    #[test]
    fn history_filters_use_payload_kind_pin_and_paste_target_across_storage() {
        let mut store = Store::open_in_memory().unwrap();
        let mut link = text_record(1, "https://example.com", 10_000);
        link.pastes.push(PasteTriggerRecord {
            target_app: "target-app.exe".into(),
            target_pid: 2,
            target_window: String::new(),
            method: PasteMethod::CtrlV,
            confidence: PasteConfidence::HotkeyObserved,
            triggered_at_ms: 11_000,
        });
        let mut code = text_record(2, "SELECT * FROM notes", 20_000);
        code.pinned = true;
        let path_text = text_record(3, r"C:\Pictures\image.png", 30_000);
        let mut files = text_record(4, "", 40_000);
        files.payload = ClipboardPayload::Files(vec![r"C:\Pictures\image.png".into()]);
        files.content_hash = payload_hash(&files.payload);
        let mut image = text_record(5, "", 50_000);
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            1,
            1,
            image::Rgba([1, 2, 3, 255]),
        ))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
        image.payload = ClipboardPayload::Image {
            png,
            width: 1,
            height: 1,
        };
        image.content_hash = payload_hash(&image.payload);
        for record in [&link, &code, &path_text, &files, &image] {
            store.insert_record(record).unwrap();
        }

        let mut query = HistoryQuery::default();
        for (filter, expected) in [
            (HistoryFilter::Link, 1),
            (HistoryFilter::Code, 2),
            (HistoryFilter::Text, 3),
            (HistoryFilter::File, 4),
            (HistoryFilter::Image, 5),
        ] {
            query.filter = filter;
            let (cards, _) = load_history_page(&store, &query, &HashMap::new()).unwrap();
            assert_eq!(
                cards.iter().map(|card| card.sequence).collect::<Vec<_>>(),
                [expected]
            );
        }
        query.filter = HistoryFilter::All;
        query.pinned_only = true;
        let (cards, _) = load_history_page(&store, &query, &HashMap::new()).unwrap();
        assert_eq!(
            cards.iter().map(|card| card.sequence).collect::<Vec<_>>(),
            [2]
        );
        query.pinned_only = false;
        query.search = "target-app".into();
        let (cards, _) = load_history_page(&store, &query, &HashMap::new()).unwrap();
        assert_eq!(
            cards.iter().map(|card| card.sequence).collect::<Vec<_>>(),
            [1]
        );
    }

    #[test]
    fn excluded_app_matches_win32_exe_name_regardless_of_case() {
        let excluded_apps = ["chatgpt.exe".to_string()];
        let observed_source = "ChatGPT.exe";

        assert!(is_excluded_source(&excluded_apps, observed_source));
        assert!(is_excluded_source(&excluded_apps, "CHATGPT.EXE (likely)"));
        assert!(!is_excluded_source(&excluded_apps, "ChatGPT-helper.exe"));
    }

    #[test]
    fn exclude_and_unexclude_round_trip_with_mixed_case_legacy_keys() {
        let mut store = Store::open_in_memory().unwrap();
        let mut excluded_apps = Vec::new();

        assert_eq!(
            exclude_app(&mut store, &mut excluded_apps, " chatgpt.exe ").unwrap(),
            "chatgpt.exe"
        );
        assert!(is_excluded_source(&excluded_apps, "ChatGPT.exe"));
        assert_eq!(store.list_excluded().unwrap(), vec!["chatgpt.exe"]);

        store
            .upsert_application("ChatGPT.exe", "ChatGPT.exe", "", true)
            .unwrap();
        excluded_apps = store.list_excluded().unwrap();
        unexclude_app(&mut store, &mut excluded_apps, "CHATGPT.EXE").unwrap();
        assert!(excluded_apps.is_empty());
        assert!(store.list_excluded().unwrap().is_empty());
    }

    #[test]
    fn internal_copy_is_suppressed_when_platform_event_has_the_written_sequence() {
        let mut flow = FlowEngine::new();
        let text = "copied from Veya";

        let payload = ClipboardPayload::Text(text.to_string());
        assert!(write_internal_payload(&mut flow, &payload, |_| Ok(41)).is_ok());
        let outcome = flow.on_clipboard_change(ClipboardChange {
            sequence: 41,
            payload: veya_core::ClipboardPayload::Text(text.to_string()),
            content_hash: payload_hash(&payload),
            source_pid: std::process::id(),
            source_exe: "veya.exe".to_string(),
            source_window: "Veya".to_string(),
            source_confidence: SourceConfidence::Likely,
            timestamp_ms: 1_000,
        });

        assert_eq!(
            outcome,
            FlowOutcome::SuppressedInternalWrite { sequence: 41 }
        );
        assert!(flow.is_empty());
    }

    #[test]
    fn failed_internal_copy_does_not_suppress_a_later_user_copy() {
        let mut flow = FlowEngine::new();
        let text = "copy that failed";

        let payload = ClipboardPayload::Text(text.to_string());
        assert!(write_internal_payload(&mut flow, &payload, |_| Err(
            "clipboard unavailable".into()
        ))
        .is_err());
        let outcome = flow.on_clipboard_change(ClipboardChange {
            sequence: 50,
            payload: veya_core::ClipboardPayload::Text(text.to_string()),
            content_hash: payload_hash(&payload),
            source_pid: 200,
            source_exe: "Code.exe".to_string(),
            source_window: "VS Code".to_string(),
            source_confidence: SourceConfidence::Exact,
            timestamp_ms: 2_000,
        });

        assert_eq!(outcome, FlowOutcome::Recorded { sequence: 50 });
        assert_eq!(flow.len(), 1);
    }
}
