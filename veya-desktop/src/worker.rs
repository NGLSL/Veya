//! Capture worker: platform events → enrich → FlowEngine → SQLite + UI snapshot.

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use veya_core::{FlowEngine, InternalClipboardWrite};
use veya_storage::Store;
use veya_windows::platform::{enrich_clipboard, enrich_paste, PlatformEvent};
use veya_windows::{content_hash, write_text};

use crate::capture::{snapshot_from_flow, SharedUi, UiState};

pub enum WorkerCmd {
    Recopy { text: String },
    ClearHistory,
    SetTracking(bool),
    PurgeOlderThan(i64),
    ExcludeApp { exe: String },
    DeleteRecord { sequence: u32 },
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

fn publish(shared: &SharedUi, flow: &FlowEngine, tracking: bool) {
    let mut snap = snapshot_from_flow(flow);
    snap.tracking = tracking;
    if let Ok(mut guard) = shared.lock() {
        snap.selected = guard.selected;
        *guard = snap;
    }
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

        let pump = std::thread::Builder::new()
            .name("veya-win-pump".into())
            .spawn(move || {
                if let Err(e) = veya_windows::run(tx) {
                    eprintln!("platform error: {e}");
                }
            })
            .expect("spawn win pump");

        loop {
            while let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    WorkerCmd::Recopy { text } => {
                        let hash = content_hash(&text);
                        if let Some(seq) = write_text(&text) {
                            flow.begin_internal_write(InternalClipboardWrite {
                                expected_sequence: Some(seq),
                                hash,
                            });
                        }
                    }
                    WorkerCmd::ClearHistory => {
                        flow.clear();
                        let _ = store.clear();
                    }
                    WorkerCmd::SetTracking(on) => {
                        tracking = on;
                    }
                    WorkerCmd::PurgeOlderThan(cutoff) => {
                        let _ = store.purge_older_than(cutoff);
                        flow.clear();
                        load_into_flow(&store, &mut flow);
                    }
                    WorkerCmd::ExcludeApp { exe } => {
                        let _ = store.upsert_application(&exe, &exe, "", true);
                    }
                    WorkerCmd::DeleteRecord { sequence } => {
                        flow.delete_record(sequence);
                        let _ = store.delete_record(sequence);
                    }
                }
            }

            match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(ev) => {
                    if tracking {
                        match ev {
                            PlatformEvent::ClipboardChange(raw) => {
                                let change = enrich_clipboard(raw);
                                // Honor Excluded Apps: never track these sources.
                                let base_exe = change
                                    .source_exe
                                    .split(" (")
                                    .next()
                                    .unwrap_or(&change.source_exe)
                                    .to_string();
                                if store.is_excluded(&base_exe).unwrap_or(false)
                                    || store.is_excluded(&change.source_exe).unwrap_or(false)
                                {
                                    publish(&shared, &flow, tracking);
                                    continue;
                                }
                                let seq = change.sequence;
                                let outcome = flow.on_clipboard_change(change);
                                if matches!(outcome, veya_core::FlowOutcome::Recorded { .. }) {
                                    if let Some(rec) = flow.record(seq) {
                                        let _ = store.insert_record(rec);
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
                    publish(&shared, &flow, tracking);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    publish(&shared, &flow, tracking);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        let _ = pump.join();
    }
}

// Silence unused on non-windows builds.
#[allow(dead_code)]
fn _types(_: Arc<Mutex<UiState>>) {}
