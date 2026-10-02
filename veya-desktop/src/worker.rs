//! Capture worker: platform events → enrich → FlowEngine → SQLite + UI snapshot.

use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use veya_core::{
    payload_hash, ClipboardPayload, ClipboardRecord, FlowEngine, InternalClipboardWrite,
    PasteConfidence, PasteTrigger, PasteTriggerRecord,
};
use veya_storage::Store;
use veya_windows::hotkey::Hotkey;
use veya_windows::platform::{enrich_clipboard, enrich_paste, PlatformEvent, WindowSignal};
use veya_windows::{write_payload, ClipboardWriteError};

use crate::capture::{CardPayloadView, CardView, HistoryQuery, Retention, SharedUi, UiState};
use crate::format::now_ms;

#[path = "history_loader.rs"]
mod history_loader;

const HISTORY_PAGE_SIZE: usize = 25;
const HISTORY_READ_BATCH: usize = 16;

pub enum WorkerCmd {
    Recopy { sequence: u32 },
    PreparePaste { sequence: u32, request_id: u64 },
    RecopyPlainText { sequence: u32 },
    LoadContent(u32),
    UnloadContent,
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

pub enum WorkerEvent {
    PastePrepared {
        request_id: u64,
        result: Result<u32, String>,
    },
}

pub struct WorkerHandle {
    pub cmd_tx: Sender<WorkerCmd>,
    pub event_rx: Receiver<WorkerEvent>,
}

pub fn spawn_worker(shared: SharedUi, activate_tx: Sender<WindowSignal>) -> WorkerHandle {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<WorkerCmd>();
    let (event_tx, event_rx) = std::sync::mpsc::channel::<WorkerEvent>();
    std::thread::Builder::new()
        .name("veya-capture".into())
        .spawn(move || run_worker(shared, cmd_rx, activate_tx, event_tx))
        .expect("spawn capture worker");
    WorkerHandle { cmd_tx, event_rx }
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
    writer: impl FnOnce(&ClipboardPayload) -> Result<u32, ClipboardWriteError>,
) -> Result<u32, String> {
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
            Ok(sequence)
        }
        Err(error) => {
            if error.changed {
                flow.fail_internal_write_after_change(std::process::id());
            } else {
                flow.cancel_internal_write();
            }
            Err(error.message)
        }
    }
}

fn load_replay_record(
    flow: &FlowEngine,
    store: &Store,
    sequence: u32,
) -> Result<ClipboardRecord, String> {
    if let Some(record) = flow.record(sequence) {
        return Ok(record.clone());
    }
    store
        .load_record(sequence)
        .map_err(|error| format!("读取记录失败：{error}"))?
        .ok_or_else(|| "记录已不存在".into())
}

fn recopy_plain_text(
    flow: &mut FlowEngine,
    store: &Store,
    sequence: u32,
    writer: impl FnOnce(&ClipboardPayload) -> Result<u32, ClipboardWriteError>,
) -> Result<u32, String> {
    let text = store
        .load_content(sequence)
        .map_err(|error| format!("读取记录失败：{error}"))?
        .ok_or_else(|| "记录已不存在".to_string())?;
    write_internal_payload(flow, &ClipboardPayload::Text(text), writer)
}

fn prepare_record_paste(
    flow: &mut FlowEngine,
    store: &Store,
    sequence: u32,
    writer: impl FnOnce(&ClipboardPayload) -> Result<u32, ClipboardWriteError>,
) -> Result<u32, String> {
    let record = load_replay_record(flow, store, sequence)?;
    let written_sequence = write_internal_payload(flow, &record.payload, writer)?;
    flow.activate_replayed_record(record);
    discard_inactive_records(flow, Some(sequence));
    Ok(written_sequence)
}

#[cfg(test)]
fn load_history_page(store: &Store, query: &HistoryQuery) -> Result<(Vec<CardView>, bool), String> {
    history_loader::load_page(store, query, &|| true)?.ok_or_else(|| "cancelled".into())
}

fn persist_paste_trigger(
    store: &mut Store,
    flow: &mut FlowEngine,
    trigger: PasteTrigger,
) -> Result<bool, String> {
    let Some(sequence) = flow
        .current_sequence()
        .filter(|seq| flow.record(*seq).is_some())
    else {
        return Ok(false);
    };
    let paste = PasteTriggerRecord {
        target_app: trigger.target_exe.clone(),
        target_pid: trigger.target_pid,
        target_window: trigger.target_window.clone(),
        method: trigger.method,
        confidence: PasteConfidence::HotkeyObserved,
        triggered_at_ms: trigger.timestamp_ms,
    };
    // Keep core and persisted history in agreement if this write fails.
    store
        .append_paste(sequence, &paste)
        .map_err(|error| format!("粘贴记录保存失败：{error}"))?;
    let outcome = flow.on_paste_trigger(trigger);
    debug_assert_eq!(outcome, veya_core::FlowOutcome::PasteAttached { sequence });
    Ok(true)
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

/// Owns only the current immutable page; status updates can share it without
/// rereading payloads or cloning each card's full text.
#[derive(Default)]
struct HistoryPage {
    loader: Option<history_loader::Loader>,
    generation: u64,
    query: Option<HistoryQuery>,
    cards: Arc<[CardView]>,
    has_next: bool,
    error: Option<String>,
    invalidated: bool,
    loading: bool,
    detail_content: Option<(u32, Arc<str>)>,
    detail_error: Option<(u32, String)>,
}

impl HistoryPage {
    fn invalidate(&mut self) {
        self.invalidated = true;
        if let Some(loader) = &self.loader {
            self.generation = loader.cancel();
        }
    }

    fn refresh(&mut self, _store: &Store, query: &HistoryQuery, active: bool) {
        if !active {
            if self.query.is_some() || self.loading {
                if let Some(loader) = &self.loader {
                    self.generation = loader.cancel();
                }
            }
            self.query = None;
            self.cards = Arc::default();
            self.has_next = false;
            self.error = None;
            self.loading = false;
            self.detail_content = None;
            self.detail_error = None;
            return;
        }
        if self.invalidated || self.query.as_ref() != Some(query) {
            self.cards = Arc::default();
            self.has_next = false;
            self.error = None;
            self.loading = true;
            if let Some(loader) = &self.loader {
                self.generation = loader.request(query);
            }
            self.query = Some(query.clone());
            self.invalidated = false;
        }
    }

    fn receive(&mut self) -> bool {
        let mut changed = false;
        loop {
            let Some(result) = self
                .loader
                .as_ref()
                .and_then(|loader| loader.results.try_recv().ok())
            else {
                break;
            };
            let initial_cards = matches!(result, history_loader::ResultEvent::Cards { .. });
            changed |= self.apply(result);
            // Publish text cards independently before consuming decoded images.
            if changed && initial_cards {
                break;
            }
        }
        changed
    }

    fn apply(&mut self, result: history_loader::ResultEvent) -> bool {
        if self.query.is_none() || self.invalidated {
            return false;
        }
        match result {
            history_loader::ResultEvent::Cards {
                generation,
                cards,
                has_next,
            } if generation == self.generation => {
                self.cards = cards.into();
                self.has_next = has_next;
                self.loading = false;
                self.error = None;
                true
            }
            history_loader::ResultEvent::Thumbnail {
                generation,
                sequence,
                hash,
                handle,
            } if generation == self.generation => {
                if let Some(index) = self
                    .cards
                    .iter()
                    .position(|c| c.sequence == sequence && c.content_hash == hash)
                {
                    if let CardPayloadView::Image { handle: slot, .. } =
                        &mut Arc::make_mut(&mut self.cards)[index].payload
                    {
                        *slot = Some(handle);
                        return true;
                    }
                }
                false
            }
            history_loader::ResultEvent::Error {
                generation,
                message,
            } if generation == self.generation => {
                self.loading = false;
                self.error = Some(message);
                true
            }
            history_loader::ResultEvent::ThumbnailUnavailable {
                generation,
                sequence,
                hash,
            } if generation == self.generation => {
                if let Some(index) = self
                    .cards
                    .iter()
                    .position(|c| c.sequence == sequence && c.content_hash == hash)
                {
                    if let CardPayloadView::Image {
                        thumbnail_failed, ..
                    } = &mut Arc::make_mut(&mut self.cards)[index].payload
                    {
                        *thumbnail_failed = true;
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }
}

fn publish(
    shared: &SharedUi,
    store: &Store,
    flow: &FlowEngine,
    history_query: &HistoryQuery,
    history_active: bool,
    history_page: &mut HistoryPage,
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
    history_page.refresh(store, history_query, history_active);
    let status_note = history_page.error.clone().unwrap_or(status_note);
    let mut snap = UiState {
        revision: 0,
        cards: Arc::clone(&history_page.cards),
        history_query: history_query.clone(),
        history_active,
        history_has_next: history_page.has_next,
        history_loading: history_page.loading,
        detail_content: history_page.detail_content.clone(),
        detail_error: history_page.detail_error.clone(),
        modal_image: if history_active {
            modal_image.clone()
        } else {
            None
        },
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
        if !history_page.loading {
            if let Some(loader) = &history_page.loader {
                loader.acknowledge(history_page.generation);
            }
        }
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

fn run_worker(
    shared: SharedUi,
    cmd_rx: Receiver<WorkerCmd>,
    activate_tx: Sender<WindowSignal>,
    event_tx: Sender<WorkerEvent>,
) {
    #[cfg(not(windows))]
    {
        let _ = (shared, cmd_rx, activate_tx, event_tx);
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
        let mut history_page = HistoryPage {
            loader: Some(history_loader::Loader::spawn(store_path.clone())),
            ..HistoryPage::default()
        };
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
            &mut history_page,
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
                        status_note = match prepare_record_paste(
                            &mut flow,
                            &store,
                            sequence,
                            write_payload,
                        ) {
                            Ok(_) => "已复制到剪贴板".into(),
                            Err(error) => format!("复制失败：{error}"),
                        };
                    }
                    WorkerCmd::PreparePaste {
                        sequence,
                        request_id,
                    } => {
                        let result =
                            prepare_record_paste(&mut flow, &store, sequence, write_payload);
                        status_note = match &result {
                            Ok(_) => "已准备粘贴".into(),
                            Err(error) => format!("粘贴准备失败：{error}"),
                        };
                        let _ = event_tx.send(WorkerEvent::PastePrepared { request_id, result });
                    }
                    WorkerCmd::RecopyPlainText { sequence } => {
                        status_note =
                            match recopy_plain_text(&mut flow, &store, sequence, write_payload) {
                                Ok(_) => "已复制为纯文本".into(),
                                Err(error) => format!("复制失败：{error}"),
                            };
                    }
                    WorkerCmd::LoadContent(sequence) => {
                        history_page.detail_content = None;
                        history_page.detail_error = None;
                        if history_active {
                            match store.load_content(sequence) {
                                Ok(Some(text)) => {
                                    history_page.detail_content = Some((sequence, Arc::from(text)))
                                }
                                Ok(None) => {
                                    history_page.detail_error =
                                        Some((sequence, "记录已不存在".into()))
                                }
                                Err(error) => {
                                    history_page.detail_error =
                                        Some((sequence, format!("读取内容失败：{error}")))
                                }
                            }
                        }
                    }
                    WorkerCmd::UnloadContent => {
                        history_page.detail_content = None;
                        history_page.detail_error = None;
                    }
                    WorkerCmd::ClearHistory => {
                        status_note = match store.clear() {
                            Ok(()) => {
                                history_page.invalidate();
                                flow.clear();
                                history_page.detail_content = None;
                                history_page.detail_error = None;
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
                            if store.purge_older_than(cutoff).unwrap_or(0) > 0 {
                                history_page.invalidate();
                            }
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
                        if history_page
                            .detail_content
                            .as_ref()
                            .is_some_and(|(sequence, _)| sequences.contains(sequence))
                        {
                            history_page.detail_content = None;
                        }
                        if history_page
                            .detail_error
                            .as_ref()
                            .is_some_and(|(sequence, _)| sequences.contains(sequence))
                        {
                            history_page.detail_error = None;
                        }
                        if modal_image
                            .as_ref()
                            .is_some_and(|(sequence, _)| sequences.contains(sequence))
                        {
                            modal_image = None;
                        }
                        status_note = match store.delete_records(&sequences) {
                            Ok(removed) => {
                                if removed > 0 {
                                    history_page.invalidate();
                                }
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
                            Ok(changed) => {
                                if changed > 0 {
                                    history_page.invalidate();
                                }
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
                            history_page.refresh(&store, &history_query, false);
                            modal_image = None;
                        }
                    }
                    WorkerCmd::LoadFullImage(sequence) => {
                        if !history_active {
                            continue;
                        }
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

            dirty |= history_page.receive();

            // Periodic retention purge (default 30 days; honors setting).
            let now = now_ms();
            if now - last_purge_ms > 60_000 {
                last_purge_ms = now;
                if let Some(cutoff) = retention.cutoff_ms(now) {
                    let removed = store.purge_older_than(cutoff).unwrap_or(0);
                    if removed > 0 {
                        history_page.invalidate();
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
                    &mut history_page,
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
                        PlatformEvent::ToggleWindow { visible, target } => {
                            let _ = activate_tx.send(WindowSignal::Hotkey { visible, target });
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
                                                } else {
                                                    history_page.invalidate();
                                                }
                                            }
                                        }
                                        let active = flow.current_sequence();
                                        discard_inactive_records(&mut flow, active);
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
                            match persist_paste_trigger(&mut store, &mut flow, trigger) {
                                Ok(true) => history_page.invalidate(),
                                Ok(false) => {}
                                Err(error) => status_note = error,
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
                    &mut history_page,
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

    pub(super) fn text_record(sequence: u32, text: &str, created_at_ms: i64) -> ClipboardRecord {
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
    fn persisted_pastes_match_core_including_repeated_timestamps_and_methods() {
        let mut store = Store::open_in_memory().unwrap();
        let mut flow = FlowEngine::new();
        let record = text_record(1, "paste source", 1000);
        store.insert_record(&record).unwrap();
        let trigger = PasteTrigger {
            target_pid: 42,
            target_exe: "notepad.exe".into(),
            target_window: "目标窗口".into(),
            method: PasteMethod::CtrlV,
            timestamp_ms: 2000,
        };
        assert_eq!(
            persist_paste_trigger(&mut store, &mut flow, trigger.clone()),
            Ok(false)
        );
        flow.activate_replayed_record(record);
        for method in [
            PasteMethod::CtrlV,
            PasteMethod::ShiftInsert,
            PasteMethod::CtrlV,
        ] {
            let mut trigger = trigger.clone();
            trigger.method = method;
            assert_eq!(
                persist_paste_trigger(&mut store, &mut flow, trigger),
                Ok(true)
            );
        }
        assert_eq!(flow.record(1), store.load_record(1).unwrap().as_ref());
        assert_eq!(flow.record(1).unwrap().pastes.len(), 3);
        assert!(flow
            .record(1)
            .unwrap()
            .pastes
            .iter()
            .all(|p| p.confidence == PasteConfidence::HotkeyObserved && p.triggered_at_ms == 2000));
    }

    #[test]
    fn failed_paste_persistence_does_not_mutate_core_or_saved_history() {
        let path = std::env::temp_dir().join(format!(
            "veya-worker-paste-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        {
            let mut store = Store::open(&path).unwrap();
            let mut flow = FlowEngine::new();
            let record = text_record(1, "paste source", 1000);
            flow.activate_replayed_record(record.clone());
            let trigger = PasteTrigger {
                target_pid: 42,
                target_exe: "notepad.exe".into(),
                target_window: "target".into(),
                method: PasteMethod::ShiftInsert,
                timestamp_ms: 2000,
            };
            let error = persist_paste_trigger(&mut store, &mut flow, trigger.clone()).unwrap_err();
            assert!(error.starts_with("粘贴记录保存失败："));
            assert_eq!(flow.record(1), Some(&record));
            assert!(store.load_record(1).unwrap().is_none());
            store.insert_record(&record).unwrap();
            // Establish an older saved association before injecting a real SQL failure.
            assert_eq!(
                persist_paste_trigger(&mut store, &mut flow, trigger.clone()),
                Ok(true)
            );
            let before = store.load_record(1).unwrap().unwrap();
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch("CREATE TRIGGER reject_paste BEFORE INSERT ON paste_trigger BEGIN SELECT RAISE(ABORT, 'injected write failure'); END;").unwrap();
            let error = persist_paste_trigger(&mut store, &mut flow, trigger).unwrap_err();
            assert!(error.contains("injected write failure"));
            assert_eq!(flow.record(1), Some(&before));
            assert_eq!(store.load_record(1).unwrap(), Some(before.clone()));
            drop(store);
            assert_eq!(
                Store::open(&path).unwrap().load_record(1).unwrap(),
                Some(before)
            );
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn plain_copy_preserves_full_text_beyond_preview() {
        let mut store = Store::open_in_memory().unwrap();
        let full = format!("{}END", "long unicode 内容".repeat(1000));
        store.insert_record(&text_record(7, &full, 10_000)).unwrap();
        let mut flow = FlowEngine::new();
        assert_eq!(
            recopy_plain_text(&mut flow, &store, 7, |payload| {
                assert_eq!(payload, &ClipboardPayload::Text(full.clone()));
                Ok(42)
            }),
            Ok(42)
        );
        assert!(recopy_plain_text(&mut flow, &store, 99, |_| panic!(
            "missing record cannot write"
        ))
        .is_err());
    }

    #[test]
    fn prepare_paste_restores_typed_payloads_and_suppresses_own_clipboard_update() {
        let payloads = [
            ClipboardPayload::Text("original text".into()),
            ClipboardPayload::Files(vec![r"C:\Pictures\one.png".into(), r"C:\two.txt".into()]),
            ClipboardPayload::Image {
                png: vec![137, 80, 78, 71],
                width: 3,
                height: 2,
            },
        ];
        for payload in payloads {
            let mut store = Store::open_in_memory().unwrap();
            let mut record = text_record(7, "display summary must not be replayed", 10_000);
            record.payload = payload.clone();
            record.content_hash = payload_hash(&payload);
            store.insert_record(&record).unwrap();
            let mut flow = FlowEngine::new();

            let result = prepare_record_paste(&mut flow, &store, 7, |written| {
                assert_eq!(written, &payload);
                Ok(41)
            });
            assert_eq!(result, Ok(41));
            let outcome = flow.on_clipboard_change(ClipboardChange {
                sequence: 41,
                payload: payload.clone(),
                content_hash: payload_hash(&payload),
                source_pid: std::process::id(),
                source_exe: "veya.exe".into(),
                source_window: "Veya".into(),
                source_confidence: SourceConfidence::Likely,
                timestamp_ms: 11_000,
            });
            assert_eq!(
                outcome,
                FlowOutcome::SuppressedInternalWrite { sequence: 41 }
            );
            assert_eq!(flow.len(), 1);
            assert_eq!(flow.current_sequence(), Some(7));
        }
    }

    #[test]
    fn prepare_paste_reads_the_active_flow_record_before_storage() {
        let store = Store::open_in_memory().unwrap();
        let mut flow = FlowEngine::new();
        let payload = ClipboardPayload::Text("active clipboard".into());
        flow.on_clipboard_change(ClipboardChange {
            sequence: 7,
            payload: payload.clone(),
            content_hash: payload_hash(&payload),
            source_pid: 200,
            source_exe: "Code.exe".into(),
            source_window: "VS Code".into(),
            source_confidence: SourceConfidence::Exact,
            timestamp_ms: 10_000,
        });
        assert_eq!(
            prepare_record_paste(&mut flow, &store, 7, |written| {
                assert_eq!(written, &payload);
                Ok(41)
            }),
            Ok(41)
        );
    }

    #[test]
    fn prepare_paste_missing_record_does_not_touch_clipboard() {
        let store = Store::open_in_memory().unwrap();
        let mut flow = FlowEngine::new();
        assert_eq!(
            prepare_record_paste(&mut flow, &store, 7, |_| {
                panic!("missing record must not reach the clipboard writer")
            }),
            Err("记录已不存在".into())
        );
    }

    #[test]
    fn prepare_paste_storage_decode_error_does_not_touch_clipboard() {
        let mut store = Store::open_in_memory().unwrap();
        let mut record = text_record(7, "invalid persisted image", 10_000);
        record.payload = ClipboardPayload::Image {
            png: Vec::new(),
            width: 1,
            height: 1,
        };
        store.insert_record(&record).unwrap();
        let mut flow = FlowEngine::new();
        let result = prepare_record_paste(&mut flow, &store, 7, |_| {
            panic!("unreadable record must not reach the clipboard writer")
        });
        assert!(result.unwrap_err().starts_with("读取记录失败："));
    }

    #[test]
    fn prepare_paste_write_failure_is_returned_and_does_not_suppress_user_copy() {
        let mut store = Store::open_in_memory().unwrap();
        let record = text_record(7, "saved text", 10_000);
        store.insert_record(&record).unwrap();
        let mut flow = FlowEngine::new();
        assert_eq!(
            prepare_record_paste(
                &mut flow,
                &store,
                7,
                |_| Err("clipboard unavailable".into())
            ),
            Err("clipboard unavailable".into())
        );
        let outcome = flow.on_clipboard_change(ClipboardChange {
            sequence: 50,
            payload: record.payload,
            content_hash: record.content_hash,
            source_pid: 200,
            source_exe: "Code.exe".into(),
            source_window: "VS Code".into(),
            source_confidence: SourceConfidence::Exact,
            timestamp_ms: 11_000,
        });
        assert_eq!(outcome, FlowOutcome::Recorded { sequence: 50 });
    }

    #[test]
    fn prepare_paste_unconfirmed_write_preserves_only_own_update_suppression() {
        let mut store = Store::open_in_memory().unwrap();
        let record = text_record(7, "saved text", 10_000);
        store.insert_record(&record).unwrap();
        let mut flow = FlowEngine::new();
        let result = prepare_record_paste(&mut flow, &store, 7, |_| {
            Err(ClipboardWriteError {
                message: "confirmation busy".into(),
                changed: true,
            })
        });
        assert_eq!(result, Err("confirmation busy".into()));
        assert_eq!(flow.current_sequence(), None);
        let change = ClipboardChange {
            sequence: 50,
            payload: record.payload.clone(),
            content_hash: record.content_hash.clone(),
            source_pid: std::process::id(),
            source_exe: "veya.exe".into(),
            source_window: "Veya".into(),
            source_confidence: SourceConfidence::Exact,
            timestamp_ms: 11_000,
        };
        assert_eq!(
            flow.on_clipboard_change(change.clone()),
            FlowOutcome::SuppressedInternalWrite { sequence: 50 }
        );
        assert!(flow.is_empty());
        let mut user_change = change;
        user_change.sequence = 51;
        user_change.source_pid = 12345;
        user_change.source_exe = "Code.exe".into();
        assert_eq!(
            flow.on_clipboard_change(user_change),
            FlowOutcome::Recorded { sequence: 51 }
        );
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
        let (first, has_next) = load_history_page(&store, &query).unwrap();
        assert_eq!(first.len(), HISTORY_PAGE_SIZE);
        assert_eq!(first.first().unwrap().sequence, 30);
        assert_eq!(first.last().unwrap().sequence, 6);
        assert!(has_next);

        query.page = 1;
        let (second, has_next) = load_history_page(&store, &query).unwrap();
        assert_eq!(second.len(), 5);
        assert_eq!(second.first().unwrap().sequence, 5);
        assert_eq!(second.last().unwrap().sequence, 1);
        assert!(!has_next);

        query.page = 0;
        query.search = "entry-03-X".into();
        let (matches, has_next) = load_history_page(&store, &query).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].sequence, 3);
        assert!(!has_next);

        query.search.clear();
        query.newest_first = false;
        let (oldest, has_next) = load_history_page(&store, &query).unwrap();
        assert_eq!(oldest.first().unwrap().sequence, 1);
        assert_eq!(oldest.last().unwrap().sequence, 25);
        assert!(has_next);
    }

    #[test]
    fn loading_page_rejects_old_results_and_releases_inactive_cards() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .insert_record(&text_record(1, "saved", 10_000))
            .unwrap();
        let query = HistoryQuery::default();
        let (cards, has_next) = load_history_page(&store, &query).unwrap();
        let mut page = HistoryPage {
            generation: 2,
            query: Some(query.clone()),
            loading: true,
            ..HistoryPage::default()
        };
        assert!(!page.apply(history_loader::ResultEvent::Cards {
            generation: 1,
            cards: cards.clone(),
            has_next
        }));
        assert!(page.loading);
        assert!(page.apply(history_loader::ResultEvent::Cards {
            generation: 2,
            cards,
            has_next
        }));
        assert!(!page.loading);
        let old = Arc::downgrade(&page.cards);
        page.refresh(&store, &query, false);
        assert!(old.upgrade().is_none());
        assert!(!page.apply(history_loader::ResultEvent::Error {
            generation: 2,
            message: "stale".into()
        }));
        assert!(page.error.is_none());
    }

    #[test]
    fn status_only_publication_shares_bounded_cards() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .insert_record(&text_record(1, &"large text ".repeat(100_000), 10_000))
            .unwrap();
        let query = HistoryQuery::default();
        let (cards, _) = load_history_page(&store, &query).unwrap();
        assert!(cards[0].content_excerpt.chars().count() <= 512);
        let mut page = HistoryPage {
            query: Some(query.clone()),
            cards: cards.into(),
            ..HistoryPage::default()
        };
        let old = page.cards.clone();
        page.refresh(&store, &query, true);
        assert!(Arc::ptr_eq(&old, &page.cards));
        page.invalidate();
        assert!(!page.apply(history_loader::ResultEvent::Error {
            generation: 0,
            message: "stale".into()
        }));
        page.refresh(&store, &query, true);
        assert!(page.loading);
        assert!(page.cards.is_empty());
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

        let (cards, has_next) = load_history_page(&store, &HistoryQuery::default()).unwrap();
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
            let (cards, _) = load_history_page(&store, &query).unwrap();
            assert_eq!(
                cards.iter().map(|card| card.sequence).collect::<Vec<_>>(),
                [expected]
            );
        }
        query.filter = HistoryFilter::All;
        query.pinned_only = true;
        let (cards, _) = load_history_page(&store, &query).unwrap();
        assert_eq!(
            cards.iter().map(|card| card.sequence).collect::<Vec<_>>(),
            [2]
        );
        query.pinned_only = false;
        query.search = "target-app".into();
        let (cards, _) = load_history_page(&store, &query).unwrap();
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
