//! Epoch milliseconds for core timestamps + local clock formatting for UI.

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Local `HH:MM` for a unix-epoch millisecond timestamp.
/// Falls back to UTC `HH:MM` only if the OS timezone conversion fails — never invents a zone.
pub fn local_hhmm(ms: i64) -> String {
    #[cfg(windows)]
    {
        if let Some((y, mo, d, h, mi, _s)) = windows_local_parts(ms) {
            let _ = (y, mo, d);
            return format!("{h:02}:{mi:02}");
        }
    }
    utc_hhmm(ms)
}

/// Local `YYYY-MM-DD HH:MM:SS`.
pub fn local_datetime(ms: i64) -> String {
    #[cfg(windows)]
    {
        if let Some((y, mo, d, h, mi, s)) = windows_local_parts(ms) {
            return format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}");
        }
    }
    format!("{} {}", "1970-01-01", utc_hhmm(ms))
}

fn utc_hhmm(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let mins = secs.div_euclid(60);
    let hours = mins.div_euclid(24).rem_euclid(24);
    let minutes = mins.rem_euclid(60);
    format!("{hours:02}:{minutes:02}")
}

#[cfg(windows)]
fn windows_local_parts(ms: i64) -> Option<(u16, u16, u16, u16, u16, u16)> {
    use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};

    // FILETIME is 100ns ticks since 1601-01-01; unix epoch is 1970-01-01.
    const EPOCH_DIFF_100NS: i64 = 116_444_736_000_000_000;
    let ticks = ms.max(0) * 10_000 + EPOCH_DIFF_100NS;
    let ft = FILETIME {
        dwLowDateTime: (ticks as u64 & 0xFFFF_FFFF) as u32,
        dwHighDateTime: ((ticks as u64 >> 32) & 0xFFFF_FFFF) as u32,
    };
    let mut utc = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();
    unsafe {
        if FileTimeToSystemTime(&ft, &mut utc).is_ok()
            && SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).is_ok()
        {
            return Some((
                local.wYear,
                local.wMonth,
                local.wDay,
                local.wHour,
                local.wMinute,
                local.wSecond,
            ));
        }
    }
    None
}
