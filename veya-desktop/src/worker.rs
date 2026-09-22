//! Capture worker: platform events → enrich → FlowEngine → SQLite + UI snapshot.

use std::sync::mpsc::{Receiver, Sender};

use veya_core::{FlowEngine, InternalClipboardWrite, PasteConfidence};
use veya_storage::Store;
use veya_windows::platform::{enrich_clipboard, enrich_paste, PlatformEvent};
use veya_windows::{content_hash, write_text};

use crate::capture::{snapshot_from_flow, Retention, SharedUi};
use crate::format::now_ms;

pub enum WorkerCmd {
    Recopy { text: String },
    ClearHistory,
    SetTracking(bool),
    SetRetention(Retention),
    ExcludeApp { exe: String },
    UnexcludeApp { exe: String },
    DeleteRecord { sequence: u32 },
    SetExpandedRaw(bool),
}

pub struct WorkerHandle {
    pub cmd_tx: Sender<WorkerCmd>,
}

pub fn spawn_worker(shared: SharedUi) -> WorkerHandle {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<WorkerCmd>();
    std::thread::Builder::new()
        .name("veya-capture".into())
        .spawn(move || run_worker(shared, cmd_rx))
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

fn load_into_flow(store: &Store, flow: &mut FlowEngine) {
    if let Ok(existing) = store.load_all() {
        for rec in existing {
            let _ = flow.on_clipboard_change(veya_core::ClipboardChange {
                sequence: rec.sequence,
                text: rec.content.clone(),
                content_hash: rec.content_hash.clone(),
                source_pid: rec.source_pid,
                source_exe: rec.source_app.clone(),
                source_window: rec.source_window.clone(),
                source_confidence: rec.source_confidence,
                timestamp_ms: rec.created_at_ms,
            });
            for p in &rec.pastes {
                let _ = flow.on_paste_trigger(veya_core::PasteTrigger {
                    target_pid: p.target_pid,
                    target_exe: p.target_app.clone(),
                    target_window: p.target_window.clone(),
                    method: p.method,
                    timestamp_ms: p.triggered_at_ms,
                });
            }
        }
    }
}

fn publish(
    shared: &SharedUi,
    flow: &FlowEngine,
    tracking: bool,
    retention: Retention,
    excluded_apps: Vec<String>,
    expanded_raw: bool,
    status_note: String,
) {
    let mut snap = snapshot_from_flow(flow, retention, tracking, excluded_apps, expanded_raw, status_note);
    if let Ok(mut guard) = shared.lock() {
        snap.selected = guard.selected;
        *guard = snap;
    }
}

fn base_exe(name: &str) -> &str {
    name.split(" (").next().unwrap_or(name)
}

fn run_worker(shared: SharedUi, cmd_rx: Receiver<WorkerCmd>) {
    #[cfg(not(windows))]
    {
        let _ = (shared, cmd_rx);
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
        let mut retention = store
            .get_setting("retention")
            .ok()
            .flatten()
            .map(|s| Retention::from_key(&s))
            .unwrap_or_default();
        let mut expanded_raw = false;
        let mut status_note = String::new();
        let mut excluded_apps = store.list_excluded().unwrap_or_default();
        let mut last_purge_ms = 0i64;

        let pump = std::thread::Builder::new()
            .name("veya-win-pump".into())
            .spawn(move || {
                if let Err(e) = veya_windows::run(tx) {
                    eprintln!("platform error: {e}");
                }
            })
            .expect("spawn win pump");

        loop {
            let mut dirty = false;
            while let Ok(cmd) = cmd_rx.try_recv() {
                dirty = true;
                match cmd {
                    WorkerCmd::Recopy { text } => {
                        let hash = content_hash(&text);
                        if let Some(seq) = write_text(&text) {
                            flow.begin_internal_write(InternalClipboardWrite {
                                expected_sequence: Some(seq),
                                hash,
                            });
                            status_note = "Copied to clipboard".into();
                        }
                    }
                    WorkerCmd::ClearHistory => {
                        flow.clear();
                        let _ = store.clear();
                        status_note = "History cleared".into();
                    }
                    WorkerCmd::SetTracking(on) => {
                        tracking = on;
                        veya_windows::set_tray_paused(!on);
                        status_note = if on {
                            "Tracking resumed".into()
                        } else {
                            "Tracking paused".into()
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
                        status_note = format!("Retention set to {}", r.label());
                    }
                    WorkerCmd::ExcludeApp { exe } => {
                        let exe = base_exe(&exe).to_string();
                        let _ = store.upsert_application(&exe, &exe, "", true);
                        if !excluded_apps.contains(&exe) {
                            excluded_apps.push(exe.clone());
                        }
                        status_note = format!("Excluded {exe}");
                    }
                    WorkerCmd::UnexcludeApp { exe } => {
                        let exe = base_exe(&exe).to_string();
                        let _ = store.upsert_application(&exe, &exe, "", false);
                        excluded_apps.retain(|e| e != &exe);
                        status_note = format!("Stopped excluding {exe}");
                    }
                    WorkerCmd::DeleteRecord { sequence } => {
                        flow.delete_record(sequence);
                        let _ = store.delete_record(sequence);
                        status_note = "Record deleted".into();
                    }
                    WorkerCmd::SetExpandedRaw(on) => {
                        expanded_raw = on;
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
                    if tracking {
                        match ev {
                            PlatformEvent::ClipboardChange(raw) => {
                                let change = enrich_clipboard(raw);
                                let exe = base_exe(&change.source_exe).to_string();
                                if excluded_apps.iter().any(|e| e == &exe) {
                                    // never track excluded apps
                                } else {
                                    let seq = change.sequence;
                                    let outcome = flow.on_clipboard_change(change);
                                    if matches!(outcome, veya_core::FlowOutcome::Recorded { .. }) {
                                        if let Some(rec) = flow.record(seq) {
                                            let _ = store.insert_record(rec);
                                        }
                                    }
                                }
                            }
                            PlatformEvent::PasteTrigger(raw) => {
                                let trigger = enrich_paste(raw);
                                let outcome = flow.on_paste_trigger(trigger);
                                if let veya_core::FlowOutcome::PasteAttached { sequence } = outcome {
                                    if let Some(rec) = flow.record(sequence) {
                                        let _ = store.insert_record(rec);
                                    }
                                }
                            }
                        }
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
                    retention,
                    excluded_apps.clone(),
                    expanded_raw,
                    status_note.clone(),
                );
            }
        }

        let _ = pump.join();
        let _ = PasteConfidence::HotkeyObserved;
    }
}
