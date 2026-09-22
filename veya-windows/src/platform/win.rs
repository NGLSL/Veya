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
    TranslateMessage, MSG, WM_APP, WM_CLOSE, WM_DESTROY,
};

use super::clipboard;
use super::keyboard;
use super::window;
use super::{ClipboardChangeRaw, PasteTriggerRaw};

#[derive(Debug, Clone)]
pub enum PlatformEvent {
    ClipboardChange(ClipboardChangeRaw),
    PasteTrigger(PasteTriggerRaw),
}

/// User message: request the message-loop thread to shut down cleanly.
pub const WM_VEYA_SHUTDOWN: u32 = WM_APP + 1;

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

/// Create the hidden window, install clipboard listener + LL keyboard hook, pump messages.
///
/// Blocks until shutdown. Returns after hooks/listener are cleaned up.
pub fn run(tx: Sender<PlatformEvent>) -> windows::core::Result<()> {
    install_sender(tx);
    SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);

    unsafe {
        SetConsoleCtrlHandler(Some(console_ctrl_handler), true)?;

        let hwnd = window::create_message_window()?;
        MAIN_HWND.store(hwnd.0 as isize, Ordering::SeqCst);

        clipboard::install_listener(hwnd)?;
        keyboard::install_hook()?;

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
        let _ = DestroyWindow(hwnd);
        MAIN_HWND.store(0, Ordering::SeqCst);
        let _ = SetConsoleCtrlHandler(Some(console_ctrl_handler), false);
    }

    release_sender();
    Ok(())
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
        WM_VEYA_SHUTDOWN | WM_CLOSE | WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}


