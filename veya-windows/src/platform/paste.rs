//! Restore a captured external destination and send a paste shortcut only there.

use super::PasteTarget;

#[cfg(windows)]
mod native {
    use std::thread;
    use std::time::{Duration, Instant};

    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
        VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RETURN, VK_RWIN, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, IsWindow, SetForegroundWindow,
    };

    use super::PasteTarget;

    fn identity(hwnd: HWND) -> Option<PasteTarget> {
        if hwnd.0.is_null() || !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            return None;
        }
        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        (thread_id != 0 && process_id != 0).then_some(PasteTarget {
            hwnd: hwnd.0 as isize,
            process_id,
            thread_id,
        })
    }

    pub fn capture_target() -> Option<PasteTarget> {
        let target = identity(unsafe { GetForegroundWindow() })?;
        (target.process_id != unsafe { GetCurrentProcessId() }).then_some(target)
    }

    fn validate(target: PasteTarget) -> Result<HWND, String> {
        let hwnd = HWND(target.hwnd as *mut _);
        if !super::same_external_target(target, identity(hwnd), unsafe { GetCurrentProcessId() }) {
            return Err("原输入窗口已关闭或身份发生变化，请重新唤起快捷粘贴。".into());
        }
        Ok(hwnd)
    }

    fn require_foreground(target: PasteTarget) -> Result<(), String> {
        validate(target)?;
        if identity(unsafe { GetForegroundWindow() }) != Some(target) {
            return Err("原输入窗口未获得焦点，已取消粘贴，避免发送到其他窗口。".into());
        }
        Ok(())
    }

    pub fn restore_target(target: PasteTarget) -> Result<(), String> {
        restore_target_checked(target, None)
    }

    fn check_clipboard(expected_sequence: Option<u32>) -> Result<(), String> {
        if let Some(expected) = expected_sequence {
            super::require_clipboard_sequence(expected, unsafe { GetClipboardSequenceNumber() })?;
        }
        Ok(())
    }

    fn restore_target_checked(
        target: PasteTarget,
        expected_sequence: Option<u32>,
    ) -> Result<(), String> {
        let hwnd = validate(target)?;
        check_clipboard(expected_sequence)?;
        let deadline = Instant::now() + Duration::from_millis(250);
        let mut restore_requested = false;
        loop {
            check_clipboard(expected_sequence)?;
            validate(target)?;
            let foreground = unsafe { GetForegroundWindow() };
            let foreground_identity = identity(foreground);
            match super::restore_permission(target, foreground_identity, unsafe {
                GetCurrentProcessId()
            }) {
                super::RestorePermission::AlreadyTarget => return require_foreground(target),
                super::RestorePermission::OwnWindow if !restore_requested => {
                    if !unsafe { SetForegroundWindow(hwnd) }.as_bool() {
                        return Err("Windows 未允许恢复原输入窗口，请重新唤起快捷粘贴。".into());
                    }
                    restore_requested = true;
                }
                super::RestorePermission::WaitForForeground if foreground.0.is_null() => {}
                super::RestorePermission::OwnWindow => {}
                _ => return Err("前台窗口已变化，已取消粘贴，避免发送到其他窗口。".into()),
            }
            if Instant::now() >= deadline {
                return Err("原输入窗口未获得焦点，已取消粘贴。".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn activation_keys_down() -> bool {
        [
            VK_CONTROL,
            VK_MENU,
            VK_SHIFT,
            VK_LWIN,
            VK_RWIN,
            VK_RETURN,
            VIRTUAL_KEY(0x56),
        ]
        .iter()
        .any(|key| unsafe { GetAsyncKeyState(i32::from(key.0)) } < 0)
    }

    fn key_input(key: VIRTUAL_KEY, release: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    dwFlags: if release {
                        KEYEVENTF_KEYUP
                    } else {
                        Default::default()
                    },
                    ..Default::default()
                },
            },
        }
    }

    pub fn paste_to_target(target: PasteTarget, expected_sequence: u32) -> Result<(), String> {
        restore_target_checked(target, Some(expected_sequence))?;
        let deadline = Instant::now() + Duration::from_millis(1200);
        while activation_keys_down() {
            check_clipboard(Some(expected_sequence))?;
            require_foreground(target)?;
            if Instant::now() >= deadline {
                return Err("快捷键或修饰键仍按住，已取消粘贴；请松开按键后重试。".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
        require_foreground(target)?;
        let inputs = [
            key_input(VK_CONTROL, false),
            key_input(VIRTUAL_KEY(0x56), false),
            key_input(VIRTUAL_KEY(0x56), true),
            key_input(VK_CONTROL, true),
        ];
        require_foreground(target)?;
        check_clipboard(Some(expected_sequence))?;
        let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
        if sent != inputs.len() as u32 {
            return Err("Windows 未完整接受粘贴按键，目标可能以管理员权限运行（UIPI 限制）。内容已复制，可手动粘贴。".into());
        }
        Ok(())
    }
}

#[cfg(windows)]
pub use native::{capture_target, paste_to_target, restore_target};

#[cfg(not(windows))]
pub fn capture_target() -> Option<PasteTarget> {
    None
}

#[cfg(not(windows))]
pub fn restore_target(_target: PasteTarget) -> Result<(), String> {
    Err("快捷粘贴仅支持 Windows。".into())
}

#[cfg(not(windows))]
pub fn paste_to_target(_target: PasteTarget, _expected_sequence: u32) -> Result<(), String> {
    Err("快捷粘贴仅支持 Windows。".into())
}

#[cfg(any(windows, test))]
fn same_external_target(target: PasteTarget, current: Option<PasteTarget>, own_pid: u32) -> bool {
    target.hwnd != 0 && target.process_id != own_pid && current == Some(target)
}

#[cfg(any(windows, test))]
fn require_clipboard_sequence(expected: u32, current: u32) -> Result<(), String> {
    if expected != current {
        return Err("剪贴板已变化，请重新选择。".into());
    }
    Ok(())
}

#[cfg(any(windows, test))]
#[derive(Debug, PartialEq, Eq)]
enum RestorePermission {
    AlreadyTarget,
    OwnWindow,
    WaitForForeground,
    Denied,
}

#[cfg(any(windows, test))]
fn restore_permission(
    target: PasteTarget,
    foreground: Option<PasteTarget>,
    own_pid: u32,
) -> RestorePermission {
    match foreground {
        Some(current) if current == target => RestorePermission::AlreadyTarget,
        Some(current) if current.process_id == own_pid => RestorePermission::OwnWindow,
        None => RestorePermission::WaitForForeground,
        Some(_) => RestorePermission::Denied,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_live_matching_external_identity_is_accepted() {
        let target = PasteTarget {
            hwnd: 123,
            process_id: 5,
            thread_id: 8,
        };
        assert!(same_external_target(target, Some(target), 1));
        assert!(!same_external_target(target, None, 1));
        assert!(!same_external_target(target, Some(target), 5));
        assert!(!same_external_target(
            target,
            Some(PasteTarget {
                process_id: 6,
                ..target
            }),
            1
        ));
        assert!(!same_external_target(
            target,
            Some(PasteTarget {
                thread_id: 9,
                ..target
            }),
            1
        ));
        assert!(!same_external_target(
            target,
            Some(PasteTarget {
                hwnd: 124,
                ..target
            }),
            1
        ));
        let null = PasteTarget { hwnd: 0, ..target };
        assert!(!same_external_target(null, Some(null), 1));
    }

    #[test]
    fn clipboard_change_cancels_prepared_paste() {
        assert!(require_clipboard_sequence(42, 42).is_ok());
        assert_eq!(
            require_clipboard_sequence(42, 43).unwrap_err(),
            "剪贴板已变化，请重新选择。"
        );
        assert!(require_clipboard_sequence(u32::MAX, 0).is_err());
    }

    #[test]
    fn restoring_only_uses_veya_focus_or_waits_for_null_foreground() {
        let target = PasteTarget {
            hwnd: 123,
            process_id: 5,
            thread_id: 8,
        };
        assert_eq!(
            restore_permission(target, Some(target), 1),
            RestorePermission::AlreadyTarget
        );
        let own = PasteTarget {
            hwnd: 456,
            process_id: 1,
            thread_id: 9,
        };
        assert_eq!(
            restore_permission(target, Some(own), 1),
            RestorePermission::OwnWindow
        );
        assert_eq!(
            restore_permission(target, None, 1),
            RestorePermission::WaitForForeground
        );
        let third_party = PasteTarget {
            process_id: 6,
            ..own
        };
        assert_eq!(
            restore_permission(target, Some(third_party), 1),
            RestorePermission::Denied
        );
        let other_target_window = PasteTarget {
            hwnd: 789,
            ..target
        };
        assert_eq!(
            restore_permission(target, Some(other_target_window), 1),
            RestorePermission::Denied
        );
    }

    #[cfg(windows)]
    #[test]
    fn invalid_target_cannot_restore_or_send_input() {
        let invalid = PasteTarget {
            hwnd: 0,
            process_id: u32::MAX,
            thread_id: u32::MAX,
        };
        assert!(restore_target(invalid).is_err());
        assert!(paste_to_target(invalid, 0).is_err());
    }
}
