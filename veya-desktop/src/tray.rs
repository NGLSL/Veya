//! System tray command vocabulary (UI-level). Implementation lives in `veya-windows`.

use crate::worker::WorkerCmd;

#[derive(Debug, Clone)]
pub enum TrayCmd {
    OpenWindow,
    Pause10Min,
    PauseTracking,
    ResumeTracking,
    ClearHistory,
    OpenSettings,
    Exit,
}

/// Spawn a tray icon + menu. Menu events are sent on `tx`.
pub fn spawn_tray(tx: std::sync::mpsc::Sender<TrayCmd>) -> Result<(), String> {
    #[cfg(windows)]
    {
        let (win_tx, win_rx) = std::sync::mpsc::channel::<veya_windows::TrayEvent>();
        std::thread::Builder::new()
            .name("veya-tray-bridge".into())
            .spawn(move || {
                while let Ok(ev) = win_rx.recv() {
                    let cmd = match ev {
                        veya_windows::TrayEvent::OpenWindow => TrayCmd::OpenWindow,
                        veya_windows::TrayEvent::Pause10Min => TrayCmd::Pause10Min,
                        veya_windows::TrayEvent::PauseTracking => TrayCmd::PauseTracking,
                        veya_windows::TrayEvent::ResumeTracking => TrayCmd::ResumeTracking,
                        veya_windows::TrayEvent::ClearHistory => TrayCmd::ClearHistory,
                        veya_windows::TrayEvent::OpenSettings => TrayCmd::OpenSettings,
                        veya_windows::TrayEvent::Exit => TrayCmd::Exit,
                    };
                    if tx.send(cmd).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        veya_windows::tray::spawn(win_tx)
    }
    #[cfg(not(windows))]
    {
        let _ = tx;
        Err("tray is Windows-only".into())
    }
}

pub fn apply_tray_action(cmd: &TrayCmd, worker: &crate::worker::WorkerHandle) -> bool {
    match cmd {
        TrayCmd::ClearHistory => {
            let _ = worker.cmd_tx.send(WorkerCmd::ClearHistory);
            true
        }
        TrayCmd::PauseTracking => {
            let _ = worker.cmd_tx.send(WorkerCmd::SetTracking {
                on: false,
                request_id: None,
            });
            true
        }
        TrayCmd::ResumeTracking => {
            let _ = worker.cmd_tx.send(WorkerCmd::SetTracking {
                on: true,
                request_id: None,
            });
            true
        }
        TrayCmd::Pause10Min => {
            let _ = worker.cmd_tx.send(WorkerCmd::SetTracking {
                on: false,
                request_id: None,
            });
            true
        }
        TrayCmd::OpenWindow | TrayCmd::OpenSettings | TrayCmd::Exit => false,
    }
}
