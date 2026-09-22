//! Hidden message-only window used for clipboard listener messages.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, RegisterClassExW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, HCURSOR, HICON,
    HMENU, HWND_MESSAGE, WINDOW_EX_STYLE, WNDCLASSEXW, WS_OVERLAPPED,
};

use super::win::{instance, message_wnd_proc};

const CLASS_NAME: PCWSTR = w!("VeyaMsgWindow");

pub fn create_message_window() -> windows::core::Result<HWND> {
    unsafe {
        let hinstance: HINSTANCE = instance();

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(message_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: HICON::default(),
            hCursor: HCURSOR::default(),
            hbrBackground: HBRUSH::default(),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: CLASS_NAME,
            hIconSm: HICON::default(),
        };
        // Re-registration is fine if class already exists.
        let _ = RegisterClassExW(&wc);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS_NAME,
            w!("Veya Clipboard Flow"),
            WS_OVERLAPPED,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            0,
            Some(HWND_MESSAGE),
            Some(HMENU::default()),
            Some(hinstance),
            None,
        )?;

        Ok(hwnd)
    }
}
