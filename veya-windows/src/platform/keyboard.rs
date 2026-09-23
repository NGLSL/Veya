//! Global low-level keyboard hook — detects Ctrl+V and Shift+Insert only.
//!
//! Normal keystrokes are never recorded. Hook proc only sends a tiny event.
//! Modifier state is tracked from hook messages; GetAsyncKeyState is used as a
//! fallback because some synthetic/fast sequences surface modifiers late.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetForegroundWindow, GetWindowThreadProcessId, SetWindowsHookExW,
    UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use super::time::now_ms;
use super::win::emit;
use super::{PasteTriggerRaw, PlatformEvent};
use veya_core::PasteMethod;

static HOOK: AtomicIsize = AtomicIsize::new(0);
static CTRL_DOWN: AtomicBool = AtomicBool::new(false);
static SHIFT_DOWN: AtomicBool = AtomicBool::new(false);

const VK_SHIFT: u32 = 0x10;
const VK_CONTROL: u32 = 0x11;
const VK_LSHIFT: u32 = 0xA0;
const VK_RSHIFT: u32 = 0xA1;
const VK_LCONTROL: u32 = 0xA2;
const VK_RCONTROL: u32 = 0xA3;
const VK_INSERT: u32 = 0x2D;
const VK_V: u32 = 0x56;

pub fn install_hook() -> windows::core::Result<()> {
    unsafe {
        let hmod = Some(super::win::instance());
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), hmod, 0)?;
        HOOK.store(hook.0 as isize, Ordering::SeqCst);
        CTRL_DOWN.store(false, Ordering::SeqCst);
        SHIFT_DOWN.store(false, Ordering::SeqCst);
    }
    Ok(())
}

pub fn uninstall_hook() {
    let h = HHOOK(HOOK.swap(0, Ordering::SeqCst) as *mut _);
    if !h.0.is_null() {
        unsafe {
            let _ = UnhookWindowsHookEx(h);
        }
    }
}

fn async_down(vk: u32) -> bool {
    unsafe { (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0 }
}

fn ctrl_down() -> bool {
    CTRL_DOWN.load(Ordering::Relaxed)
        || async_down(VK_CONTROL)
        || async_down(VK_LCONTROL)
        || async_down(VK_RCONTROL)
}

fn shift_down() -> bool {
    SHIFT_DOWN.load(Ordering::Relaxed)
        || async_down(VK_SHIFT)
        || async_down(VK_LSHIFT)
        || async_down(VK_RSHIFT)
}

unsafe extern "system" fn low_level_keyboard_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    // HC_ACTION == 0
    if n_code == 0 && l_param.0 != 0 {
        let msg = w_param.0 as u32;
        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

        if is_down || is_up {
            let kb = &*(l_param.0 as *const KBDLLHOOKSTRUCT);
            let vk = kb.vkCode;

            match vk {
                VK_SHIFT | VK_LSHIFT | VK_RSHIFT => SHIFT_DOWN.store(is_down, Ordering::Relaxed),
                VK_CONTROL | VK_LCONTROL | VK_RCONTROL => {
                    CTRL_DOWN.store(is_down, Ordering::Relaxed)
                }
                _ => {}
            }

            // Paste triggers only on key-down (ignore key-up and raw typing).
            // This is paste *intent* (hotkey + foreground app), not verified insertion.
            if is_down {
                if vk == VK_V && ctrl_down() {
                    emit_paste(PasteMethod::CtrlV);
                } else if vk == VK_INSERT && shift_down() {
                    emit_paste(PasteMethod::ShiftInsert);
                }
            }
        }
    }

    CallNextHookEx(None, n_code, w_param, l_param)
}

fn emit_paste(method: PasteMethod) {
    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        let mut pid = 0u32;
        if !hwnd.0.is_null() {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        emit(PlatformEvent::PasteTrigger(PasteTriggerRaw {
            target_hwnd: hwnd.0 as isize,
            target_pid: pid,
            method,
            timestamp_ms: now_ms(),
        }));
    }
}
