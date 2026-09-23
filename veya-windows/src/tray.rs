//! System tray (Shell_NotifyIcon + popup menu). Windows only.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    OpenWindow,
    Pause10Min,
    PauseTracking,
    ResumeTracking,
    ClearHistory,
    OpenSettings,
    Exit,
}

/// Reflect pause state on the tray icon. Safe from any thread after `spawn`.
pub fn set_paused(paused: bool) {
    set_paused_impl(paused);
}

#[cfg(windows)]
static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);
#[cfg(windows)]
static PAUSED: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static TX: Mutex<Option<Sender<TrayEvent>>> = Mutex::new(None);

#[cfg(not(windows))]
fn set_paused_impl(_paused: bool) {}

#[cfg(windows)]
fn set_paused_impl(paused: bool) {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

    const WM_TRAY_PAUSE: u32 = WM_APP + 3;
    PAUSED.store(paused, Ordering::SeqCst);
    let hwnd_val = TRAY_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        let _ = unsafe {
            PostMessageW(
                Some(HWND(hwnd_val as *mut _)),
                WM_TRAY_PAUSE,
                WPARAM(paused as usize),
                LPARAM(0),
            )
        };
    }
}

#[cfg(windows)]
pub fn spawn(tx: Sender<TrayEvent>) -> Result<(), String> {
    *TX.lock().unwrap() = Some(tx);
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("veya-tray".into())
        .spawn(move || {
            if let Err(e) = run_tray(&ready_tx) {
                let _ = ready_tx.send(Err(e.clone()));
                eprintln!("tray: {e}");
            }
            *TX.lock().unwrap() = None;
            TRAY_HWND.store(0, Ordering::SeqCst);
        })
        .map_err(|e| e.to_string())?;
    ready_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| format!("tray startup status unavailable: {e}"))?
}

#[cfg(not(windows))]
pub fn spawn(_tx: Sender<TrayEvent>) -> Result<(), String> {
    Err("tray is Windows-only".into())
}

/// Create a tray HICON from exact RGBA pixels (Kite-style 32×32).
/// Do not let the shell resample a larger icon — that is what made the
/// tray glyph look softer/different from Kite.
#[cfg(windows)]
fn create_hicon_from_rgba(
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
    use std::ptr;
    use windows::core::BOOL;
    use windows::Win32::Graphics::Gdi::{
        CreateBitmap, CreateDIBSection, DeleteObject, GetDC, ReleaseDC, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, ICONINFO};

    if rgba.len() != (width * height * 4) as usize {
        return None;
    }

    // Windows DIBs are BGRA top-down.
    let mut bgra = rgba.to_vec();
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
    }

    unsafe {
        let hdc = GetDC(None);
        let mut bits: *mut core::ffi::c_void = ptr::null_mut();
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                biSizeImage: (width * height * 4) as u32,
                ..Default::default()
            },
            bmiColors: [Default::default()],
        };
        let color = match CreateDIBSection(Some(hdc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(bmp) => bmp,
            Err(_) => {
                let _ = ReleaseDC(None, hdc);
                return None;
            }
        };
        if bits.is_null() {
            let _ = DeleteObject(color.into());
            let _ = ReleaseDC(None, hdc);
            return None;
        }
        ptr::copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, bgra.len());

        // 32bpp + alpha: mask is unused but CreateIconIndirect requires one.
        let mask = CreateBitmap(width as i32, height as i32, 1, 1, None);
        if mask.is_invalid() {
            let _ = DeleteObject(color.into());
            let _ = ReleaseDC(None, hdc);
            return None;
        }

        let icon_info = ICONINFO {
            fIcon: BOOL(1),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let hicon = CreateIconIndirect(&icon_info).ok();
        let _ = DeleteObject(mask.into());
        let _ = DeleteObject(color.into());
        let _ = ReleaseDC(None, hdc);
        hicon.filter(|h| !h.is_invalid())
    }
}

/// Product logo at tray-native 32×32. The source artwork has transparent
/// padding, so crop that padding before downsampling to the shell icon size.
#[cfg(windows)]
fn product_icon() -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
    create_hicon_from_rgba(&tray_pixels()?, 32, 32)
}

#[cfg(windows)]
fn tray_pixels() -> Option<Vec<u8>> {
    use image::imageops::{crop_imm, overlay, resize, FilterType};
    use image::RgbaImage;

    let source = image::load_from_memory(include_bytes!("../../icons/256x256.png"))
        .ok()?
        .to_rgba8();
    let mut min_x = source.width();
    let mut min_y = source.height();
    let mut max_x = 0;
    let mut max_y = 0;
    for (x, y, pixel) in source.enumerate_pixels() {
        if pixel[3] >= 32 {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if min_x > max_x || min_y > max_y {
        return None;
    }
    let crop_width = max_x - min_x + 1;
    let crop_height = max_y - min_y + 1;
    let cropped = crop_imm(&source, min_x, min_y, crop_width, crop_height).to_image();
    let scale = 32.0 / crop_width.max(crop_height) as f32;
    let width = ((crop_width as f32 * scale).round() as u32).clamp(1, 32);
    let height = ((crop_height as f32 * scale).round() as u32).clamp(1, 32);
    let scaled = resize(&cropped, width, height, FilterType::Lanczos3);
    let mut canvas = RgbaImage::new(32, 32);
    overlay(
        &mut canvas,
        &scaled,
        ((32 - width) / 2) as i64,
        ((32 - height) / 2) as i64,
    );
    Some(canvas.into_raw())
}

#[cfg(windows)]
fn load_app_icon() -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
    product_icon().or_else(|| {
        use windows::core::PCWSTR;
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::{LoadImageW, HICON, IMAGE_ICON};

        let module = unsafe { GetModuleHandleW(None) }.ok()?;
        // Fallback: exe RT_GROUP_ICON at exact 32×32 (no LR_DEFAULTSIZE resample).
        let img = unsafe {
            LoadImageW(
                Some(module.into()),
                PCWSTR(1 as *const u16),
                IMAGE_ICON,
                32,
                32,
                Default::default(),
            )
            .ok()?
        };
        Some(HICON(img.0))
    })
}

#[cfg(windows)]
fn send(cmd: TrayEvent) {
    if let Ok(guard) = TX.lock() {
        if let Some(tx) = guard.as_ref() {
            let _ = tx.send(cmd);
        }
    }
}

#[cfg(windows)]
fn run_tray(ready: &Sender<Result<(), String>>) -> Result<(), String> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows::Win32::Graphics::Gdi::HBRUSH;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
        NOTIFYICONDATAW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW,
        GetCursorPos, GetMessageW, InsertMenuW, LoadIconW, PostQuitMessage, RegisterClassExW,
        SetForegroundWindow, SetTimer, TrackPopupMenu, TranslateMessage, CS_HREDRAW, CS_VREDRAW,
        CW_USEDEFAULT, HICON, HMENU, HWND_MESSAGE, IDI_APPLICATION, IDI_WARNING, MF_BYPOSITION,
        MF_STRING, MSG, TPM_BOTTOMALIGN, TPM_LEFTALIGN, WM_APP, WM_COMMAND, WM_DESTROY,
        WM_LBUTTONUP, WM_RBUTTONUP, WM_TIMER, WNDCLASSEXW, WS_OVERLAPPED,
    };

    const WM_TRAY: u32 = WM_APP + 2;
    const WM_TRAY_PAUSE: u32 = WM_APP + 3;
    const ID_OPEN: usize = 1001;
    const ID_PAUSE10: usize = 1002;
    const ID_PAUSE: usize = 1003;
    const ID_CLEAR: usize = 1004;
    const ID_SETTINGS: usize = 1005;
    const ID_EXIT: usize = 1006;
    const TIMER_RESUME: usize = 50;

    static ICON_TRACKING: AtomicIsize = AtomicIsize::new(0);
    static ICON_PAUSED: AtomicIsize = AtomicIsize::new(0);

    fn tip_for(paused: bool) -> [u16; 128] {
        let src: Vec<u16> = if paused {
            "Veya（已暂停）\0".encode_utf16().collect()
        } else {
            "Veya\0".encode_utf16().collect()
        };
        let mut tip = [0u16; 128];
        let n = src.len().min(127);
        tip[..n].copy_from_slice(&src[..n]);
        tip
    }

    unsafe fn apply_icon_state(hwnd: HWND, paused: bool) {
        let tracking = HICON(ICON_TRACKING.load(Ordering::SeqCst) as *mut _);
        let paused_icon = HICON(ICON_PAUSED.load(Ordering::SeqCst) as *mut _);
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_TIP,
            hIcon: if paused { paused_icon } else { tracking },
            szTip: tip_for(paused),
            ..Default::default()
        };
        let _ = Shell_NotifyIconW(NIM_MODIFY, &mut nid).ok();
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_TRAY => {
                let mouse = (lparam.0 as u32) & 0xFFFF;
                if mouse == WM_RBUTTONUP || mouse == WM_LBUTTONUP {
                    show_menu(hwnd);
                }
                LRESULT(0)
            }
            WM_TRAY_PAUSE => {
                let paused = wparam.0 != 0;
                PAUSED.store(paused, Ordering::SeqCst);
                apply_icon_state(hwnd, paused);
                LRESULT(0)
            }
            WM_COMMAND => {
                match wparam.0 & 0xFFFF {
                    ID_OPEN => send(TrayEvent::OpenWindow),
                    ID_PAUSE10 => {
                        send(TrayEvent::Pause10Min);
                        PAUSED.store(true, Ordering::SeqCst);
                        apply_icon_state(hwnd, true);
                        SetTimer(Some(hwnd), TIMER_RESUME, 10 * 60 * 1000, None);
                    }
                    ID_PAUSE => {
                        if PAUSED.load(Ordering::SeqCst) {
                            send(TrayEvent::ResumeTracking);
                            PAUSED.store(false, Ordering::SeqCst);
                            apply_icon_state(hwnd, false);
                        } else {
                            send(TrayEvent::PauseTracking);
                            PAUSED.store(true, Ordering::SeqCst);
                            apply_icon_state(hwnd, true);
                        }
                    }
                    ID_CLEAR => send(TrayEvent::ClearHistory),
                    ID_SETTINGS => send(TrayEvent::OpenSettings),
                    ID_EXIT => {
                        send(TrayEvent::Exit);
                        PostQuitMessage(0);
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_TIMER => {
                if wparam.0 == TIMER_RESUME {
                    send(TrayEvent::ResumeTracking);
                    PAUSED.store(false, Ordering::SeqCst);
                    apply_icon_state(hwnd, false);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    unsafe fn show_menu(hwnd: HWND) {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        let open: Vec<u16> = "打开 Veya\0".encode_utf16().collect();
        let p10: Vec<u16> = "暂停 10 分钟\0".encode_utf16().collect();
        let pause: Vec<u16> = if PAUSED.load(Ordering::SeqCst) {
            "恢复记录\0".encode_utf16().collect()
        } else {
            "暂停记录\0".encode_utf16().collect()
        };
        let clear: Vec<u16> = "清空历史\0".encode_utf16().collect();
        let settings: Vec<u16> = "设置\0".encode_utf16().collect();
        let exit: Vec<u16> = "退出\0".encode_utf16().collect();
        let _ = InsertMenuW(
            menu,
            0,
            MF_BYPOSITION | MF_STRING,
            ID_OPEN,
            PCWSTR(open.as_ptr()),
        );
        let _ = InsertMenuW(
            menu,
            1,
            MF_BYPOSITION | MF_STRING,
            ID_PAUSE10,
            PCWSTR(p10.as_ptr()),
        );
        let _ = InsertMenuW(
            menu,
            2,
            MF_BYPOSITION | MF_STRING,
            ID_PAUSE,
            PCWSTR(pause.as_ptr()),
        );
        let _ = InsertMenuW(
            menu,
            3,
            MF_BYPOSITION | MF_STRING,
            ID_CLEAR,
            PCWSTR(clear.as_ptr()),
        );
        let _ = InsertMenuW(
            menu,
            4,
            MF_BYPOSITION | MF_STRING,
            ID_SETTINGS,
            PCWSTR(settings.as_ptr()),
        );
        let _ = InsertMenuW(
            menu,
            5,
            MF_BYPOSITION | MF_STRING,
            ID_EXIT,
            PCWSTR(exit.as_ptr()),
        );

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | TPM_BOTTOMALIGN,
            pt.x,
            pt.y,
            Some(0),
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
    }

    unsafe {
        let class = w!("VeyaTrayWindow");
        let hmod = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: HINSTANCE(hmod.0),
            hIcon: HICON::default(),
            hCursor: Default::default(),
            hbrBackground: HBRUSH::default(),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: class,
            hIconSm: HICON::default(),
            cbClsExtra: 0,
            cbWndExtra: 0,
        };
        let _ = RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(
            Default::default(),
            class,
            w!("Veya Tray"),
            WS_OVERLAPPED,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            0,
            Some(HWND_MESSAGE),
            Some(HMENU::default()),
            Some(HINSTANCE(hmod.0)),
            None,
        )
        .map_err(|e| e.to_string())?;

        // Prefer the embedded app icon (winres / icons/icon.ico) over the generic
        // stock IDI_APPLICATION glyph so the tray matches the product logo.
        let tracking = load_app_icon()
            .or_else(|| LoadIconW(None, IDI_APPLICATION).ok())
            .unwrap_or_default();
        let paused_icon = LoadIconW(None, IDI_WARNING).unwrap_or(tracking);
        ICON_TRACKING.store(tracking.0 as isize, Ordering::SeqCst);
        ICON_PAUSED.store(paused_icon.0 as isize, Ordering::SeqCst);

        let paused = PAUSED.load(Ordering::SeqCst);
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            hIcon: if paused { paused_icon } else { tracking },
            szTip: tip_for(paused),
            ..Default::default()
        };
        if !Shell_NotifyIconW(NIM_ADD, &mut nid).as_bool() {
            return Err("Shell_NotifyIconW NIM_ADD failed".into());
        }
        TRAY_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
        let _ = ready.send(Ok(()));

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 != 0 {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }

        let _ = Shell_NotifyIconW(NIM_DELETE, &mut nid).ok();
        TRAY_HWND.store(0, Ordering::SeqCst);
    }

    Ok(())
}

#[cfg(all(test, windows))]
mod icon_tests {
    use super::{product_icon, tray_pixels};

    #[test]
    fn visible_tray_artwork_fills_native_canvas() {
        let rgba = tray_pixels().expect("tray icon pixels");
        assert_eq!(rgba.len(), 32 * 32 * 4);
        let visible: Vec<(usize, usize)> = (0..32 * 32)
            .filter(|&pixel| rgba[pixel * 4 + 3] >= 128)
            .map(|pixel| (pixel % 32, pixel / 32))
            .collect();
        let min_x = visible.iter().map(|&(x, _)| x).min().unwrap();
        let max_x = visible.iter().map(|&(x, _)| x).max().unwrap();
        let min_y = visible.iter().map(|&(_, y)| y).min().unwrap();
        let max_y = visible.iter().map(|&(_, y)| y).max().unwrap();
        assert!(max_x - min_x + 1 >= 30, "tray logo is too narrow");
        assert!(max_y - min_y + 1 >= 30, "tray logo is too short");
    }

    #[test]
    fn win32_tray_icon_has_native_dimensions() {
        use windows::Win32::Graphics::Gdi::{DeleteObject, GetObjectW, BITMAP, HGDIOBJ};
        use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};

        let icon = product_icon().expect("CreateIconIndirect should accept tray pixels");
        let mut info = ICONINFO::default();
        unsafe {
            let got_info = GetIconInfo(icon, &mut info);
            let mut bitmap = BITMAP::default();
            let got_bitmap = if got_info.is_ok() {
                GetObjectW(
                    HGDIOBJ(info.hbmColor.0),
                    std::mem::size_of::<BITMAP>() as i32,
                    Some(&mut bitmap as *mut BITMAP as *mut _),
                )
            } else {
                0
            };
            if !info.hbmColor.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
            }
            if !info.hbmMask.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
            }
            let _ = DestroyIcon(icon);
            assert!(got_bitmap > 0, "GetIconInfo/GetObjectW failed");
            assert_eq!((bitmap.bmWidth, bitmap.bmHeight), (32, 32));
        }
    }
}
