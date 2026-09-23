//! RegisterHotKey belongs to the hidden message window's thread.

use std::sync::Mutex;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_NOREPEAT,
};

use crate::hotkey::Hotkey;

const HOTKEY_ID_A: i32 = 1;
const HOTKEY_ID_B: i32 = 2;

#[derive(Clone, Copy)]
struct Registered {
    selection: Hotkey,
    id: i32,
    suspended: bool,
}

static REGISTERED: Mutex<Option<Registered>> = Mutex::new(None);

fn register(hwnd: HWND, id: i32, selection: Hotkey) -> windows::core::Result<()> {
    let (modifiers, key) = selection.registration().expect("enabled hotkey");
    unsafe {
        RegisterHotKey(
            Some(hwnd),
            id,
            HOT_KEY_MODIFIERS(modifiers | MOD_NOREPEAT.0),
            key,
        )
    }
}

fn resume_locked(hwnd: HWND, registered: &mut Option<Registered>) -> (Hotkey, Option<String>) {
    let Some(mut entry) = *registered else {
        return (Hotkey::Disabled, None);
    };
    if !entry.suspended {
        return (entry.selection, None);
    }
    match register(hwnd, entry.id, entry.selection) {
        Ok(()) => {
            entry.suspended = false;
            *registered = Some(entry);
            (entry.selection, None)
        }
        Err(error) => {
            *registered = None;
            (
                Hotkey::Disabled,
                Some(format!("{} 恢复失败：{error}", entry.selection)),
            )
        }
    }
}

/// Register a replacement before releasing the old shortcut. On failure the
/// old shortcut remains active, so a failed settings change is reversible.
pub(super) fn apply(hwnd: HWND, requested: Hotkey) -> (Hotkey, Option<String>) {
    let mut registered = REGISTERED.lock().expect("hotkey registration lock");
    let old = *registered;
    if old.map(|entry| entry.selection) == Some(requested) {
        return resume_locked(hwnd, &mut registered);
    }

    if requested == Hotkey::Disabled {
        if let Some(entry) = old {
            if !entry.suspended {
                let _ = unsafe { UnregisterHotKey(Some(hwnd), entry.id) };
            }
        }
        *registered = None;
        return (Hotkey::Disabled, None);
    }

    let next_id = if old.map(|entry| entry.id) == Some(HOTKEY_ID_A) {
        HOTKEY_ID_B
    } else {
        HOTKEY_ID_A
    };
    match register(hwnd, next_id, requested) {
        Ok(()) => {
            if let Some(entry) = old {
                if !entry.suspended {
                    let _ = unsafe { UnregisterHotKey(Some(hwnd), entry.id) };
                }
            }
            *registered = Some(Registered {
                selection: requested,
                id: next_id,
                suspended: false,
            });
            (requested, None)
        }
        Err(error) => {
            let (active, restore_error) = resume_locked(hwnd, &mut registered);
            let message = if let Some(restore_error) = restore_error {
                format!("{requested} 注册失败：{error}；{restore_error}")
            } else {
                format!("{requested} 注册失败：{error}")
            };
            (active, Some(message))
        }
    }
}

pub(super) fn suspend(hwnd: HWND) -> Result<(), String> {
    let mut registered = REGISTERED.lock().map_err(|error| error.to_string())?;
    if let Some(mut entry) = *registered {
        if !entry.suspended {
            unsafe { UnregisterHotKey(Some(hwnd), entry.id) }.map_err(|e| e.to_string())?;
            entry.suspended = true;
            *registered = Some(entry);
        }
    }
    Ok(())
}

pub(super) fn resume(hwnd: HWND) -> Option<(Hotkey, Option<String>)> {
    let mut registered = REGISTERED.lock().expect("hotkey registration lock");
    registered
        .is_some()
        .then(|| resume_locked(hwnd, &mut registered))
}

pub(super) fn current() -> Hotkey {
    REGISTERED
        .lock()
        .ok()
        .and_then(|guard| guard.map(|entry| entry.selection))
        .unwrap_or(Hotkey::Disabled)
}

pub(super) fn is_active(id: usize) -> bool {
    REGISTERED
        .lock()
        .ok()
        .and_then(|guard| guard.map(|entry| !entry.suspended && entry.id as usize == id))
        .unwrap_or(false)
}

pub(super) fn release(hwnd: HWND) {
    if let Ok(mut registered) = REGISTERED.lock() {
        if let Some(entry) = registered.take() {
            if !entry.suspended {
                let _ = unsafe { UnregisterHotKey(Some(hwnd), entry.id) };
            }
        }
    }
}
