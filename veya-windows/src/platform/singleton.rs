//! Windows process singleton and activation bridge.
//!
//! The primary process owns a session-local named mutex and an auto-reset
//! event. A later launch signals the event and exits; the primary process
//! forwards the activation request to the Iced window owner.

use std::sync::mpsc::Sender;
use std::sync::OnceLock;
use std::time::Duration;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject, INFINITE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, FindWindowW, GetWindowRect, IsIconic, IsWindowVisible, IsZoomed,
    SetForegroundWindow, SetWindowPos, ShowWindow, SWP_NOACTIVATE, SWP_NOZORDER, SW_RESTORE,
};

use super::WindowSignal;

const MUTEX_NAME: &str = r"Local\Veya.desktop.instance";
const EVENT_NAME: &str = r"Local\Veya.desktop.activate";
const WINDOW_TITLE: &str = "Veya";

/// Kernel handles held by the primary process for the lifetime of the app.
struct Instance {
    mutex: HANDLE,
    event: HANDLE,
}

// The handles are owned by this module and are only used by Win32's thread-safe
// wait/signal APIs. The listener receives a raw value because HANDLE is not Send.
unsafe impl Send for Instance {}
unsafe impl Sync for Instance {}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.event);
            let _ = CloseHandle(self.mutex);
        }
    }
}

static PRIMARY: OnceLock<Instance> = OnceLock::new();

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Public startup error that distinguishes a normal secondary launch from a
/// real failure to create the guard.
#[derive(Debug)]
pub enum ClaimError {
    AlreadyRunning,
    CreateFailed(String),
}

/// Claim the Windows session-local singleton.
///
/// The event is created before the mutex so a secondary process can always
/// signal activation after it observes an existing mutex. Kernel creation
/// errors are returned to the caller instead of silently starting an
/// unguarded second process.
pub fn claim() -> Result<(), ClaimError> {
    if PRIMARY.get().is_some() {
        return Ok(());
    }

    match claim_with(MUTEX_NAME, EVENT_NAME) {
        Ok(instance) => {
            // A second claim from this process is harmless; keep the first
            // handle pair alive until process shutdown.
            let _ = PRIMARY.set(instance);
            Ok(())
        }
        Err(ClaimError::AlreadyRunning) => {
            // The secondary process is commonly the foreground process that
            // the user just launched. Attempt activation here as well as via
            // the primary listener; this avoids Windows foreground-lock
            // restrictions in the common case. The listener covers launches
            // where the first window is still being created.
            activate_main_window();
            Err(ClaimError::AlreadyRunning)
        }
        Err(ClaimError::CreateFailed(message)) => Err(ClaimError::CreateFailed(message)),
    }
}

fn claim_with(mutex_name: &str, event_name: &str) -> Result<Instance, ClaimError> {
    let mutex_w = wide(mutex_name);
    let event_w = wide(event_name);

    unsafe {
        // CreateEventW opens the existing event when the primary process has
        // already created it. It also makes the mutex/event creation order
        // race-safe for two launches that begin nearly simultaneously.
        let event =
            CreateEventW(None, false, false, PCWSTR(event_w.as_ptr())).map_err(|error| {
                ClaimError::CreateFailed(format!("activation event creation failed: {error}"))
            })?;

        let mutex = match CreateMutexW(None, true, PCWSTR(mutex_w.as_ptr())) {
            Ok(handle) => handle,
            Err(error) => {
                let _ = CloseHandle(event);
                return Err(ClaimError::CreateFailed(format!(
                    "singleton mutex creation failed: {error}"
                )));
            }
        };

        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(mutex);
            // This handle was opened with modify access by CreateEventW, so it
            // can signal the primary listener without another name lookup.
            let _ = SetEvent(event);
            let _ = CloseHandle(event);
            return Err(ClaimError::AlreadyRunning);
        }

        Ok(Instance { mutex, event })
    }
}

/// Start the primary process's activation listener.
pub fn spawn_activation_listener(activate_tx: Sender<WindowSignal>) -> Result<(), &'static str> {
    let Some(primary) = PRIMARY.get() else {
        return Err("singleton has not been claimed");
    };

    let event = primary.event.0 as isize;
    std::thread::spawn(move || loop {
        let handle = HANDLE(event as *mut core::ffi::c_void);
        let wait = unsafe { WaitForSingleObject(handle, INFINITE) };
        if wait.0 != 0 {
            break;
        }
        if activate_tx.send(WindowSignal::Activate).is_err() {
            break;
        }
    });
    Ok(())
}

/// Sample visibility when WM_HOTKEY arrives, before the queued UI action runs.
/// A minimized window should be restored rather than hidden again.
pub fn main_window_is_showing() -> bool {
    let title = wide(WINDOW_TITLE);
    unsafe {
        FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr()))
            .ok()
            .is_some_and(|hwnd| {
                !hwnd.is_invalid() && IsWindowVisible(hwnd).as_bool() && !IsIconic(hwnd).as_bool()
            })
    }
}

/// Restore and focus the primary Veya window, waiting briefly for the native
/// window to be created during startup. This also handles a minimized window.
fn activate_main_window() {
    let title = wide(WINDOW_TITLE);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);

    loop {
        unsafe {
            if let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())) {
                if !hwnd.is_invalid() {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                    ensure_visible(hwnd);
                    let _ = BringWindowToTop(hwnd);
                    let _ = SetForegroundWindow(hwnd);
                    return;
                }
            }
        }

        if std::time::Instant::now() >= deadline {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Keep a restored main window inside its nearest active monitor's work area.
/// An already visible, fully contained window retains its user-chosen position.
pub fn ensure_main_window_visible() {
    let title = wide(WINDOW_TITLE);
    unsafe {
        if let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())) {
            if !hwnd.is_invalid() {
                ensure_visible(hwnd);
            }
        }
    }
}

fn ensure_visible(hwnd: windows::Win32::Foundation::HWND) {
    unsafe {
        if IsZoomed(hwnd).as_bool() {
            return;
        }
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return;
        }
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return;
        }
        let work = info.rcWork;
        let width = (rect.right - rect.left).min(work.right - work.left).max(1);
        let height = (rect.bottom - rect.top).min(work.bottom - work.top).max(1);
        let x = rect.left.clamp(work.left, work.right - width);
        let y = rect.top.clamp(work.top, work.bottom - height);
        if x != rect.left
            || y != rect.top
            || width != rect.right - rect.left
            || height != rect.bottom - rect.top
        {
            let _ = SetWindowPos(
                hwnd,
                None,
                x,
                y,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_names(tag: &str) -> (String, String) {
        let tick = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let suffix = format!("{}.{}.{}", std::process::id(), tag, tick);
        (
            format!(r"Local\Veya.test.instance.{suffix}"),
            format!(r"Local\Veya.test.activate.{suffix}"),
        )
    }

    #[test]
    fn names_are_nul_terminated() {
        assert_eq!(wide(MUTEX_NAME).last(), Some(&0));
        assert_eq!(wide(EVENT_NAME).last(), Some(&0));
        assert_eq!(wide(WINDOW_TITLE).last(), Some(&0));
    }

    #[test]
    fn secondary_claim_signals_primary_event() {
        let (mutex_name, event_name) = unique_names("activation");
        let primary = claim_with(&mutex_name, &event_name).expect("primary claim");
        let secondary = claim_with(&mutex_name, &event_name);
        assert!(
            matches!(secondary, Err(ClaimError::AlreadyRunning)),
            "expected secondary claim"
        );

        let wait = unsafe { WaitForSingleObject(primary.event, 1000) };
        assert_eq!(wait.0, 0, "secondary did not signal activation event");
    }

    #[test]
    fn dropping_primary_releases_mutex() {
        let (mutex_name, event_name) = unique_names("release");
        let primary = claim_with(&mutex_name, &event_name).expect("primary claim");
        drop(primary);
        let next = claim_with(&mutex_name, &event_name).expect("mutex should be released");
        drop(next);
    }
}
