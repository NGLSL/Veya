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
    std::thread::Builder::new()
        .name("veya-tray".into())
        .spawn(move || {
            if let Err(e) = run_tray() {
                eprintln!("tray: {e}");
            }
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
pub fn spawn(_tx: Sender<TrayEvent>) -> Result<(), String> {
    Err("tray is Windows-only".into())
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
fn run_tray() -> Result<(), String> {
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
            "Veya (paused)\0".encode_utf16().collect()
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
        let open: Vec<u16> = "Open Veya\0".encode_utf16().collect();
        let p10: Vec<u16> = "Pause for 10 minutes\0".encode_utf16().collect();
        let pause: Vec<u16> = if PAUSED.load(Ordering::SeqCst) {
            "Resume tracking\0".encode_utf16().collect()
        } else {
            "Pause tracking\0".encode_utf16().collect()
        };
        let clear: Vec<u16> = "Clear history\0".encode_utf16().collect();
        let settings: Vec<u16> = "Settings\0".encode_utf16().collect();
        let exit: Vec<u16> = "Exit\0".encode_utf16().collect();
        let _ = InsertMenuW(menu, 0, MF_BYPOSITION | MF_STRING, ID_OPEN, PCWSTR(open.as_ptr()));
        let _ = InsertMenuW(menu, 1, MF_BYPOSITION | MF_STRING, ID_PAUSE10, PCWSTR(p10.as_ptr()));
        let _ = InsertMenuW(menu, 2, MF_BYPOSITION | MF_STRING, ID_PAUSE, PCWSTR(pause.as_ptr()));
        let _ = InsertMenuW(menu, 3, MF_BYPOSITION | MF_STRING, ID_CLEAR, PCWSTR(clear.as_ptr()));
        let _ = InsertMenuW(
            menu,
            4,
            MF_BYPOSITION | MF_STRING,
            ID_SETTINGS,
            PCWSTR(settings.as_ptr()),
        );
        let _ = InsertMenuW(menu, 5, MF_BYPOSITION | MF_STRING, ID_EXIT, PCWSTR(exit.as_ptr()));

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

        let tracking = LoadIconW(None, IDI_APPLICATION).unwrap_or_default();
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
