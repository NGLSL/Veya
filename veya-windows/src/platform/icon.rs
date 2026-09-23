//! Extract an application's file icon as top-down RGBA (for UI avatars).
//! Called from the UI/worker path — not from Win32 hooks.
//! Refined with Win32 GDI mask/alpha restoration and candidate resolution (inspired by Kite).

use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

/// 32-bit RGBA pixels, top-down.
#[derive(Debug, Clone)]
pub struct RgbaIcon {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Best-effort icon for `app` (exe file name or full path).
pub fn exe_icon_rgba(app: &str) -> Option<RgbaIcon> {
    let path = resolve_exe_path(app)?;
    icon_from_file(&path)
}

/// Resolve `chrome.exe` / `C:\\...\\app.exe` to a filesystem path.
pub fn resolve_exe_path(app: &str) -> Option<PathBuf> {
    let t = app.trim();
    if t.is_empty() || t == "unknown" {
        return None;
    }
    let t = t.split(" (").next().unwrap_or(t).trim();
    let p = Path::new(t);
    if p.is_absolute() && p.is_file() {
        return Some(p.to_path_buf());
    }
    if t.contains('\\') || t.contains('/') {
        let p = PathBuf::from(t);
        if p.is_file() {
            return Some(p);
        }
    }

    let file_name = Path::new(t)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| t.to_string());

    if let Some(found) = search_path(&file_name) {
        return Some(found);
    }
    if let Some(found) = app_paths_lookup(&file_name) {
        return Some(found);
    }

    // Try alternative names (e.g. Weixin.exe <-> WeChat.exe)
    let alt_name = match file_name.to_ascii_lowercase().as_str() {
        "weixin.exe" => Some("WeChat.exe"),
        "wechat.exe" => Some("Weixin.exe"),
        _ => None,
    };
    if let Some(alt) = alt_name {
        if let Some(found) = search_path(alt) {
            return Some(found);
        }
        if let Some(found) = app_paths_lookup(alt) {
            return Some(found);
        }
    }

    // Check system directories
    for dir in [
        std::env::var("SystemRoot")
            .ok()
            .map(|r| PathBuf::from(r).join("System32")),
        std::env::var("SystemRoot")
            .ok()
            .map(|r| PathBuf::from(r).join("SysWOW64")),
        std::env::var("windir").ok().map(PathBuf::from),
    ]
    .into_iter()
    .flatten()
    {
        let cand = dir.join(&file_name);
        if cand.is_file() {
            return Some(cand);
        }
        if let Some(alt) = alt_name {
            let cand_alt = dir.join(alt);
            if cand_alt.is_file() {
                return Some(cand_alt);
            }
        }
    }

    // Check common ProgramFiles and LocalAppData locations for typical desktop apps
    let search_roots: Vec<PathBuf> = [
        std::env::var("ProgramFiles").ok().map(PathBuf::from),
        std::env::var("ProgramFiles(x86)").ok().map(PathBuf::from),
        std::env::var("LOCALAPPDATA").ok().map(PathBuf::from),
        std::env::var("APPDATA").ok().map(PathBuf::from),
    ]
    .into_iter()
    .flatten()
    .collect();

    let lower = file_name.to_ascii_lowercase();
    let relative_hints: &[&str] = match lower.as_str() {
        "weixin.exe" | "wechat.exe" => &[
            r"Tencent\WeChat\WeChat.exe",
            r"Tencent\Weixin\Weixin.exe",
            r"Tencent\WeChatApp\WeChat.exe",
            r"Programs\Tencent\WeChat\WeChat.exe",
        ],
        "chrome.exe" => &[r"Google\Chrome\Application\chrome.exe"],
        "msedge.exe" => &[r"Microsoft\Edge\Application\msedge.exe"],
        "code.exe" => &[
            r"Programs\Microsoft VS Code\Code.exe",
            r"Microsoft VS Code\Code.exe",
        ],
        "snipaste.exe" => &[r"Snipaste\Snipaste.exe"],
        _ => &[],
    };

    for root in &search_roots {
        for hint in relative_hints {
            let cand = root.join(hint);
            if cand.is_file() {
                return Some(cand);
            }
        }
    }

    None
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn path_wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn search_path(file_name: &str) -> Option<PathBuf> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::SearchPathW;

    let name = to_wide(file_name);
    let ext: Vec<u16> = if file_name.to_lowercase().ends_with(".exe") {
        Vec::new()
    } else {
        to_wide(".exe")
    };
    let ext_pcw = if ext.is_empty() {
        PCWSTR::null()
    } else {
        PCWSTR(ext.as_ptr())
    };
    let mut buf = [0u16; 1024];
    let n = unsafe {
        SearchPathW(
            PCWSTR::null(),
            PCWSTR(name.as_ptr()),
            ext_pcw,
            Some(&mut buf),
            None,
        )
    };
    if n == 0 || n as usize >= buf.len() {
        return None;
    }
    let path = PathBuf::from(String::from_utf16_lossy(&buf[..n as usize]));
    path.is_file().then_some(path)
}

fn app_paths_lookup(file_name: &str) -> Option<PathBuf> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
        KEY_READ, REG_SZ,
    };

    let sub = to_wide(&format!(
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\{file_name}"
    ));

    // Try HKCU first (modern per-user installs like WeChat, VSCode), then HKLM
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let mut hkey = Default::default();
        let ok = unsafe { RegOpenKeyExW(root, PCWSTR(sub.as_ptr()), Some(0), KEY_READ, &mut hkey) };
        if ok.is_err() {
            continue;
        }

        let mut buf = [0u8; 1024];
        let mut size = buf.len() as u32;
        let mut kind = REG_SZ;
        let empty = [0u16];
        let q = unsafe {
            RegQueryValueExW(
                hkey,
                PCWSTR(empty.as_ptr()),
                None,
                Some(&mut kind),
                Some(buf.as_mut_ptr()),
                Some(&mut size),
            )
        };
        unsafe {
            let _ = RegCloseKey(hkey);
        }
        if q.is_ok() && size >= 2 {
            let units: Vec<u16> = buf
                .chunks_exact(2)
                .take((size as usize / 2).saturating_sub(1))
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .take_while(|&u| u != 0)
                .collect();
            let path = PathBuf::from(String::from_utf16_lossy(&units).trim_matches('"'));
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

fn icon_from_file(path: &Path) -> Option<RgbaIcon> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::ExtractIconExW;
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON};

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    // 1) Shell 大图标优先（多数快捷方式/exe 能拿到高清图标）
    if let Some(icon) = shgfi_icon(path) {
        return Some(icon);
    }

    // 2) ExtractIconExW 主大图标
    let wide = path_wide(path);
    unsafe {
        let mut large = HICON::default();
        let mut small = HICON::default();
        let n = ExtractIconExW(
            PCWSTR(wide.as_ptr()),
            0,
            Some(&mut large),
            Some(&mut small),
            1,
        );
        if n > 0 && !large.is_invalid() {
            let img = hicon_to_rgba(large);
            let _ = DestroyIcon(large);
            if !small.is_invalid() {
                let _ = DestroyIcon(small);
            }
            if img.is_some() {
                return img;
            }
        }
        if n > 0 && !small.is_invalid() {
            let img = hicon_to_rgba(small);
            let _ = DestroyIcon(small);
            return img;
        }
    }
    None
}

fn shgfi_icon(path: &Path) -> Option<RgbaIcon> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
    use windows::Win32::UI::WindowsAndMessaging::DestroyIcon;

    let wide = path_wide(path);
    unsafe {
        let mut shfi = SHFILEINFOW::default();
        let ok = SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            Default::default(),
            Some(&mut shfi),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        if ok == 0 || shfi.hIcon.is_invalid() {
            return None;
        }
        let img = hicon_to_rgba(shfi.hIcon);
        let _ = DestroyIcon(shfi.hIcon);
        img
    }
}

/// HICON → top-down RGBA with mask and alpha restoration.
fn hicon_to_rgba(hicon: windows::Win32::UI::WindowsAndMessaging::HICON) -> Option<RgbaIcon> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC,
        BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetIconInfo, ICONINFO};

    unsafe {
        if hicon.is_invalid() {
            return None;
        }
        let mut info = ICONINFO::default();
        if GetIconInfo(hicon, &mut info).is_err() {
            return None;
        }

        let color_bmp = info.hbmColor;
        let mask_bmp = info.hbmMask;
        let use_mask = color_bmp.is_invalid();
        let src_bmp = if use_mask { mask_bmp } else { color_bmp };

        if src_bmp.is_invalid() {
            if !color_bmp.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(color_bmp.0));
            }
            if !mask_bmp.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(mask_bmp.0));
            }
            return None;
        }

        let mut bmp = BITMAP::default();
        let got = GetObjectW(
            HGDIOBJ(src_bmp.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bmp as *mut BITMAP as *mut _),
        );
        if got == 0 {
            if !color_bmp.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(color_bmp.0));
            }
            if !mask_bmp.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(mask_bmp.0));
            }
            return None;
        }

        let width = bmp.bmWidth.max(0) as u32;
        let mut height = bmp.bmHeight.unsigned_abs();
        if use_mask && height > 0 {
            height /= 2;
        }
        if width == 0 || height == 0 || width > 512 || height > 512 {
            if !color_bmp.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(color_bmp.0));
            }
            if !mask_bmp.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(mask_bmp.0));
            }
            return None;
        }

        let hdc_screen = GetDC(Some(HWND::default()));
        let mem_dc = CreateCompatibleDC(Some(hdc_screen));
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut pixels = vec![0u8; (width * 4 * height) as usize];
        let lines = GetDIBits(
            mem_dc,
            src_bmp,
            0,
            height,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        // Alpha channel restoration from AND mask (critical for legacy Win32 / WeChat icons)
        if lines != 0
            && !use_mask
            && pixels.chunks_exact(4).all(|px| px[3] <= 8)
            && !mask_bmp.is_invalid()
        {
            let mut mask_bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0 as u32,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut mask_pixels = vec![0u8; (width * 4 * height) as usize];
            let got_mask = GetDIBits(
                mem_dc,
                mask_bmp,
                0,
                height,
                Some(mask_pixels.as_mut_ptr() as *mut _),
                &mut mask_bmi,
                DIB_RGB_COLORS,
            );
            if got_mask != 0 {
                for (color, mask) in pixels.chunks_exact_mut(4).zip(mask_pixels.chunks_exact(4)) {
                    let mask_val = mask[0].max(mask[1]).max(mask[2]);
                    color[3] = if mask_val < 128 { 255 } else { 0 };
                }
            }
        }

        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(Some(HWND::default()), hdc_screen);
        if !color_bmp.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(color_bmp.0));
        }
        if !mask_bmp.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(mask_bmp.0));
        }

        if lines == 0 {
            return None;
        }

        // BGRA → RGBA
        for px in pixels.chunks_exact_mut(4) {
            px.swap(0, 2);
        }

        // Monochrome icon inversion
        if use_mask {
            for px in pixels.chunks_exact_mut(4) {
                let v = px[0];
                px[0] = 255 - v;
                px[1] = 255 - v;
                px[2] = 255 - v;
                px[3] = 255;
            }
        }

        // Fallback for all-transparent with non-black RGB
        if pixels.chunks_exact(4).all(|px| px[3] <= 8) {
            if pixels
                .chunks_exact(4)
                .all(|px| px[0] <= 8 && px[1] <= 8 && px[2] <= 8)
            {
                return None;
            }
            for px in pixels.chunks_exact_mut(4) {
                px[3] = 255;
            }
        }

        // Auto crop transparent borders and center
        let (cw, ch, cropped_pixels) = crop_and_center_rgba(&pixels, width, height, 48);

        Some(RgbaIcon {
            width: cw,
            height: ch,
            rgba: cropped_pixels,
        })
    }
}

/// Crop transparent margins and center on a square canvas (matches Kite crop_and_fill logic).
fn crop_and_center_rgba(src: &[u8], w: u32, h: u32, target_size: u32) -> (u32, u32, Vec<u8>) {
    let mut min_x = w;
    let mut max_x = 0;
    let mut min_y = h;
    let mut max_y = 0;

    for y in 0..h {
        for x in 0..w {
            let idx = ((y * w + x) * 4) as usize;
            if idx + 3 < src.len() && src[idx + 3] > 8 {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }

    if min_x > max_x || min_y > max_y {
        return (w, h, src.to_vec());
    }

    let cw = max_x - min_x + 1;
    let ch = max_y - min_y + 1;

    // Scale so that max(cw, ch) fits ~88% of target_size
    let inner_target = ((target_size as f32) * 0.88).round() as u32;
    let scale = (inner_target as f32 / cw.max(ch) as f32).min(1.0);
    let nw = ((cw as f32) * scale).round().max(1.0) as u32;
    let nh = ((ch as f32) * scale).round().max(1.0) as u32;

    let ox = (target_size.saturating_sub(nw)) / 2;
    let oy = (target_size.saturating_sub(nh)) / 2;

    let mut canvas = vec![0u8; (target_size * target_size * 4) as usize];

    for dy in 0..nh {
        let sy = min_y + ((dy as f32 / scale) as u32).min(ch - 1);
        let dst_y = oy + dy;
        if dst_y >= target_size {
            continue;
        }
        for dx in 0..nw {
            let sx = min_x + ((dx as f32 / scale) as u32).min(cw - 1);
            let dst_x = ox + dx;
            if dst_x >= target_size {
                continue;
            }

            let s_idx = ((sy * w + sx) * 4) as usize;
            let d_idx = ((dst_y * target_size + dst_x) * 4) as usize;

            if s_idx + 3 < src.len() && d_idx + 3 < canvas.len() {
                canvas[d_idx..d_idx + 4].copy_from_slice(&src[s_idx..s_idx + 4]);
            }
        }
    }

    (target_size, target_size, canvas)
}
