//! Message pump + hook lifecycle (Windows).

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Mutex;

use windows::core::BOOL;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Console::{
    SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_C_EVENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, PostMessageW, PostQuitMessage,
    TranslateMessage, MSG, WM_APP, WM_CLOSE, WM_DESTROY, WM_HOTKEY,
};

use super::clipboard;
use super::hotkey;
use super::keyboard;
use super::singleton;
use super::window;
use super::{ClipboardChangeRaw, PasteTriggerRaw};
use crate::hotkey::Hotkey;

#[derive(Debug, Clone)]
pub enum PlatformEvent {
    ClipboardChange(ClipboardChangeRaw),
    ClipboardSkipped {
        sequence: u32,
    },
    PasteTrigger(PasteTriggerRaw),
    ToggleWindow {
        visible: bool,
    },
    HotkeyStatus {
        requested: Hotkey,
        active: Hotkey,
        error: Option<String>,
    },
}

/// User message: request the message-loop thread to shut down cleanly.
pub const WM_VEYA_SHUTDOWN: u32 = WM_APP + 1;
const WM_VEYA_SET_HOTKEY: u32 = WM_APP + 2;
const WM_VEYA_RECORD_HOTKEY: u32 = WM_APP + 3;

static EVENT_TX: Mutex<Option<Sender<PlatformEvent>>> = Mutex::new(None);
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn emit(ev: PlatformEvent) {
    if let Ok(guard) = EVENT_TX.lock() {
        if let Some(tx) = guard.as_ref() {
            let _ = tx.send(ev);
        }
    }
}

fn install_sender(tx: Sender<PlatformEvent>) {
    if let Ok(mut guard) = EVENT_TX.lock() {
        *guard = Some(tx);
    }
}

/// Drop the only Sender so the worker thread can finish and print its summary.
pub fn release_sender() {
    if let Ok(mut guard) = EVENT_TX.lock() {
        *guard = None;
    }
}

pub(crate) fn instance() -> HINSTANCE {
    unsafe {
        let hmod = GetModuleHandleW(None).unwrap_or_default();
        HINSTANCE(hmod.0)
    }
}

/// HWND used to own clipboard writes. `EmptyClipboard` requires a real owner
/// window or the following `SetClipboardData` calls fail.
pub(crate) fn clipboard_owner_hwnd() -> Option<HWND> {
    let hwnd = MAIN_HWND.load(Ordering::SeqCst);
    (hwnd != 0).then_some(HWND(hwnd as *mut _))
}

/// Create the hidden window, install clipboard listener + LL keyboard hook, pump messages.
///
/// Blocks until shutdown. Returns after hooks/listener are cleaned up.
pub fn run(tx: Sender<PlatformEvent>, initial_hotkey: Hotkey) -> windows::core::Result<()> {
    install_sender(tx);
    SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);

    unsafe {
        SetConsoleCtrlHandler(Some(console_ctrl_handler), true)?;

        let hwnd = window::create_message_window()?;
        MAIN_HWND.store(hwnd.0 as isize, Ordering::SeqCst);

        clipboard::install_listener(hwnd)?;
        keyboard::install_hook()?;
        apply_hotkey(hwnd, initial_hotkey);

        let mut msg = MSG::default();
        loop {
            let r = GetMessageW(&mut msg, None, 0, 0);
            if r.0 == 0 || r.0 == -1 {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        keyboard::uninstall_hook();
        clipboard::uninstall_listener(hwnd);
        hotkey::release(hwnd);
        let _ = DestroyWindow(hwnd);
        MAIN_HWND.store(0, Ordering::SeqCst);
        let _ = SetConsoleCtrlHandler(Some(console_ctrl_handler), false);
    }

    release_sender();
    Ok(())
}

/// Queue a shortcut change on the window thread that owns RegisterHotKey.
pub fn request_hotkey_change(selection: Hotkey) -> bool {
    let hwnd = MAIN_HWND.load(Ordering::SeqCst);
    hwnd != 0
        && unsafe {
            PostMessageW(
                Some(HWND(hwnd as *mut _)),
                WM_VEYA_SET_HOTKEY,
                WPARAM(selection.code()),
                LPARAM(0),
            )
        }
        .is_ok()
}

/// Temporarily release the active shortcut while the UI records a new one.
pub fn request_hotkey_recording(recording: bool) -> bool {
    let hwnd = MAIN_HWND.load(Ordering::SeqCst);
    hwnd != 0
        && unsafe {
            PostMessageW(
                Some(HWND(hwnd as *mut _)),
                WM_VEYA_RECORD_HOTKEY,
                WPARAM(recording as usize),
                LPARAM(0),
            )
        }
        .is_ok()
}

fn apply_hotkey(hwnd: HWND, requested: Hotkey) {
    let (active, error) = hotkey::apply(hwnd, requested);
    emit(PlatformEvent::HotkeyStatus {
        requested,
        active,
        error,
    });
}

pub fn request_shutdown() {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
    let hwnd = MAIN_HWND.load(Ordering::SeqCst);
    if hwnd != 0 {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd as *mut _)),
                WM_VEYA_SHUTDOWN,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

unsafe extern "system" fn console_ctrl_handler(ctrl_type: u32) -> BOOL {
    match ctrl_type {
        CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_CLOSE_EVENT => {
            request_shutdown();
            BOOL(1)
        }
        _ => BOOL(0),
    }
}

/// Shared WndProc for the message-only window (clipboard updates + shutdown).
pub(crate) unsafe extern "system" fn message_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        clipboard::WM_CLIPBOARDUPDATE => {
            clipboard::handle_clipboard_update(hwnd);
            LRESULT(0)
        }
        WM_HOTKEY => {
            if hotkey::is_active(wparam.0) {
                emit(PlatformEvent::ToggleWindow {
                    visible: singleton::main_window_is_showing(),
                });
            }
            LRESULT(0)
        }
        WM_VEYA_SET_HOTKEY => {
            if let Some(selection) = Hotkey::from_code(wparam.0) {
                apply_hotkey(hwnd, selection);
            }
            LRESULT(0)
        }
        WM_VEYA_RECORD_HOTKEY => {
            if wparam.0 != 0 {
                if let Err(error) = hotkey::suspend(hwnd) {
                    let current = hotkey::current();
                    emit(PlatformEvent::HotkeyStatus {
                        requested: current,
                        active: current,
                        error: Some(format!("快捷键录制准备失败：{error}")),
                    });
                }
            } else if let Some((active, error)) = hotkey::resume(hwnd) {
                emit(PlatformEvent::HotkeyStatus {
                    requested: active,
                    active,
                    error,
                });
            }
            LRESULT(0)
        }
        WM_VEYA_SHUTDOWN | WM_CLOSE | WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
