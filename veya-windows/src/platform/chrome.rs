//! Rounded borderless window chrome.

/// Keep the native window outline in sync with its size. A borderless window
/// does not reliably honor DWM's corner hint, so clip its actual Win32 region.
/// Apply rounded chrome if the window geometry has changed.
///
/// Returns `true` when the current geometry is known and either already had
/// the requested region or was updated successfully. Callers can therefore
/// stop polling once the native window is ready instead of querying it on
/// every UI tick.
pub fn apply_rounded_corners(title: &str) -> bool {
    use std::sync::Mutex;

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };
    use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn};
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowRect, IsZoomed};

    static LAST: Mutex<Option<(usize, i32, i32, bool)>> = Mutex::new(None);

    let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(wide.as_ptr())) else {
            return false;
        };
        if hwnd.is_invalid() {
            return false;
        }
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return false;
        }
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let maximized = IsZoomed(hwnd).as_bool();
        let key = (hwnd.0 as usize, width, height, maximized);
        let Ok(mut last) = LAST.lock() else {
            return false;
        };
        if *last == Some(key) {
            return true;
        }

        let pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as *const core::ffi::c_void,
            std::mem::size_of_val(&pref) as u32,
        );

        let applied = if maximized {
            SetWindowRgn(hwnd, None, true) != 0
        } else {
            let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, 20, 20);
            if region.is_invalid() {
                false
            } else if SetWindowRgn(hwnd, Some(region), true) != 0 {
                // Windows owns the region after a successful SetWindowRgn.
                true
            } else {
                let _ = DeleteObject(region.into());
                false
            }
        };
        if applied {
            *last = Some(key);
        }
        applied
    }
}
