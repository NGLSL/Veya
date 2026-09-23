//! Capture worker: platform events → enrich → FlowEngine → SQLite + UI snapshot.

use std::sync::mpsc::{Receiver, Sender};

use veya_core::{
    payload_hash, ClipboardPayload, FlowEngine, InternalClipboardWrite, PasteConfidence,
};
use veya_storage::Store;
use veya_windows::hotkey::Hotkey;
use veya_windows::platform::{enrich_clipboard, enrich_paste, PlatformEvent, WindowSignal};
use veya_windows::write_payload;

use crate::capture::{snapshot_from_flow, Retention, SharedUi};
use crate::format::now_ms;

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

fn load_into_flow(store: &Store, flow: &mut FlowEngine) {
    if let Ok(existing) = store.load_all() {
        for rec in existing {
            flow.restore_record(rec);
        }
    }
}

fn publish(
    shared: &SharedUi,
    flow: &FlowEngine,
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
    let mut snap = snapshot_from_flow(
        flow,
        retention,
        tracking,
        excluded_apps,
        expanded_raw,
        status_note,
    );
    snap.tracking_ack = tracking_ack;
    snap.hotkey_selected = hotkey_selected;
    snap.hotkey_active = hotkey_active;
    snap.hotkey_error = hotkey_error;
    if let Ok(mut guard) = shared.lock() {
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
        load_into_flow(&store, &mut flow);
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
            &flow,
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
                        let payload = flow.record(sequence).map(|record| record.payload.clone());
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
                            flow.clear();
                            load_into_flow(&store, &mut flow);
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
                        flow.clear();
                        load_into_flow(&store, &mut flow);
                        dirty = true;
                    }
                }
            }

            match rx.recv_timeout(std::time::Duration::from_millis(100)) {
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
                                    }
                                }
                                None => {
                                    flow.on_untracked_clipboard_change(sequence);
                                    status_note = "剪贴板内容无法读取或超出大小上限，已跳过".into();
                                }
                            }
                        }
                        PlatformEvent::ClipboardSkipped { sequence } if tracking => {
                            flow.on_untracked_clipboard_change(sequence);
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
                    dirty = true;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }

            if dirty {
                publish(
                    &shared,
                    &flow,
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
    use veya_core::{ClipboardChange, FlowOutcome, SourceConfidence};

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
