//! Clipboard capture and replay. Win32 clipboard ownership stays in this module.

use std::path::Path;

use image::ImageFormat;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData,
    GetClipboardOwner, GetClipboardSequenceNumber, GetOpenClipboardWindow,
    IsClipboardFormatAvailable, OpenClipboard, RemoveClipboardFormatListener, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use super::time::now_ms;
use super::win::{clipboard_owner_hwnd, emit};
use super::{ClipboardChangeRaw, ClipboardPayloadRaw, PlatformEvent};
use veya_core::{ClipboardPayload, SourceConfidence};

/// Sent to the window when clipboard content changes.
pub const WM_CLIPBOARDUPDATE: u32 = 0x031D;

const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;
const CF_DIBV5: u32 = 17;

const BI_BITFIELDS: u32 = 3;
const BI_JPEG: u32 = 4;
const BI_PNG: u32 = 5;
const BI_ALPHABITFIELDS: u32 = 6;
const LCS_SRGB: u32 = 0x7352_4742;

const MAX_DIB_BYTES: usize = 64 * 1024 * 1024;
const MAX_PNG_BYTES: usize = 32 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u64 = 16 * 1024 * 1024;
const MAX_FILE_COUNT: u32 = 4096;
const MAX_PATH_UNITS: usize = 32_767;
const MAX_FILE_LIST_BYTES: usize = 4 * 1024 * 1024;
const MAX_TEXT_UNITS: usize = 1_000_000;

pub fn install_listener(hwnd: HWND) -> windows::core::Result<()> {
    unsafe {
        AddClipboardFormatListener(hwnd)?;
    }
    Ok(())
}

pub fn uninstall_listener(hwnd: HWND) {
    unsafe {
        let _ = RemoveClipboardFormatListener(hwnd);
    }
}

fn pid_of(hwnd: HWND) -> u32 {
    if hwnd.0.is_null() {
        return 0;
    }
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    pid
}

/// Resolve the source before opening the clipboard. Opening it can change the
/// open-clipboard window and make a NULL owner look like Veya.
fn resolve_source() -> (isize, u32, SourceConfidence) {
    let owner = unsafe { GetClipboardOwner() }.unwrap_or_default();
    if !owner.0.is_null() {
        return (owner.0 as isize, pid_of(owner), SourceConfidence::Exact);
    }

    let open = unsafe { GetOpenClipboardWindow() }.unwrap_or_default();
    if !open.0.is_null() {
        return (open.0 as isize, pid_of(open), SourceConfidence::Likely);
    }

    let foreground = unsafe { GetForegroundWindow() };
    if !foreground.0.is_null() {
        return (
            foreground.0 as isize,
            pid_of(foreground),
            SourceConfidence::Likely,
        );
    }

    (0, 0, SourceConfidence::Unknown)
}

pub fn handle_clipboard_update(hwnd: HWND) {
    let initial_sequence = unsafe { GetClipboardSequenceNumber() };
    let (mut owner_hwnd, mut source_pid, mut source_confidence) = resolve_source();
    let Some((sequence, payload)) = read_clipboard_payload(hwnd) else {
        emit(PlatformEvent::ClipboardSkipped {
            sequence: initial_sequence,
        });
        return;
    };

    // The owner was observed before opening the clipboard. If a newer write
    // raced with that observation, its source cannot be claimed as exact.
    if sequence != initial_sequence {
        owner_hwnd = 0;
        source_pid = 0;
        source_confidence = SourceConfidence::Unknown;
    }

    emit(PlatformEvent::ClipboardChange(ClipboardChangeRaw {
        sequence,
        payload,
        owner_hwnd,
        source_pid,
        source_confidence,
        timestamp_ms: now_ms(),
    }));
}

/// Copy exactly one prioritized payload while holding the clipboard lock.
/// DIB decoding and PNG encoding happen later, on the worker.
fn read_clipboard_payload(hwnd: HWND) -> Option<(u32, ClipboardPayloadRaw)> {
    unsafe {
        OpenClipboard(Some(hwnd)).ok()?;
        let result = (|| {
            let sequence = GetClipboardSequenceNumber();
            let payload = if format_available(CF_HDROP) {
                ClipboardPayloadRaw::Files(read_hdrop()?)
            } else if format_available(CF_DIBV5) {
                ClipboardPayloadRaw::Dib {
                    format: CF_DIBV5,
                    bytes: read_global_data(CF_DIBV5, MAX_DIB_BYTES)?,
                }
            } else if format_available(CF_DIB) {
                ClipboardPayloadRaw::Dib {
                    format: CF_DIB,
                    bytes: read_global_data(CF_DIB, MAX_DIB_BYTES)?,
                }
            } else if format_available(CF_UNICODETEXT) {
                ClipboardPayloadRaw::Text(read_unicode_text()?)
            } else {
                return None;
            };
            Some((sequence, payload))
        })();
        let _ = CloseClipboard();
        result
    }
}

fn format_available(format: u32) -> bool {
    unsafe { IsClipboardFormatAvailable(format).is_ok() }
}

unsafe fn read_global_data(format: u32, max_bytes: usize) -> Option<Vec<u8>> {
    let handle = GetClipboardData(format).ok()?;
    let hglobal = HGLOBAL(handle.0);
    let size = GlobalSize(hglobal);
    if size == 0 || size > max_bytes {
        return None;
    }
    let locked = GlobalLock(hglobal);
    if locked.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(locked.cast::<u8>(), size).to_vec();
    let _ = GlobalUnlock(hglobal);
    Some(bytes)
}

unsafe fn read_hdrop() -> Option<Vec<String>> {
    let handle = GetClipboardData(CF_HDROP).ok()?;
    let hdrop = HDROP(handle.0);
    let count = DragQueryFileW(hdrop, u32::MAX, None);
    if count == 0 || count > MAX_FILE_COUNT {
        return None;
    }

    let mut paths = Vec::with_capacity(count as usize);
    let mut total_units = 0usize;
    for index in 0..count {
        let length = DragQueryFileW(hdrop, index, None) as usize;
        if length == 0 || length > MAX_PATH_UNITS {
            return None;
        }
        total_units = total_units.checked_add(length.checked_add(1)?)?;
        if total_units.checked_mul(2)? > MAX_FILE_LIST_BYTES {
            return None;
        }

        let mut buffer = vec![0u16; length + 1];
        let copied = DragQueryFileW(hdrop, index, Some(&mut buffer));
        if copied as usize != length {
            return None;
        }
        buffer.truncate(copied as usize);
        paths.push(String::from_utf16(&buffer).ok()?);
    }
    Some(paths)
}

unsafe fn read_unicode_text() -> Option<String> {
    let handle = GetClipboardData(CF_UNICODETEXT).ok()?;
    let hglobal = HGLOBAL(handle.0);
    let size = GlobalSize(hglobal);
    if size < 2 || size > MAX_TEXT_UNITS * 2 {
        return None;
    }
    let locked = GlobalLock(hglobal);
    if locked.is_null() {
        return None;
    }
    let units = std::slice::from_raw_parts(locked.cast::<u16>(), size / 2);
    let text = units
        .iter()
        .position(|unit| *unit == 0)
        .map(|length| String::from_utf16_lossy(&units[..length]));
    let _ = GlobalUnlock(hglobal);
    text
}

/// Convert CF_DIB/CF_DIBV5 bytes to a bounded PNG. Unsupported or malformed
/// DIB variants are rejected without allocating from untrusted dimensions.
pub fn dib_to_png(format: u32, bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    if !matches!(format, CF_DIB | CF_DIBV5) || bytes.len() < 40 || bytes.len() > MAX_DIB_BYTES {
        return None;
    }

    let header_size = read_u32(bytes, 0)? as usize;
    if !(40..=124).contains(&header_size) || header_size > bytes.len() {
        return None;
    }
    let width = read_i32(bytes, 4)?;
    let signed_height = read_i32(bytes, 8)?;
    let bit_count = read_u16(bytes, 14)?;
    let compression = read_u32(bytes, 16)?;
    let size_image = read_u32(bytes, 20)? as usize;
    let colors_used = read_u32(bytes, 32)? as usize;
    let height = signed_height.checked_abs()? as u32;
    let width = u32::try_from(width).ok()?;
    if width == 0
        || height == 0
        || u64::from(width).checked_mul(u64::from(height))? > MAX_IMAGE_PIXELS
    {
        return None;
    }

    // V5 may carry an embedded or linked profile. The canonical PNG conversion
    // does not preserve that profile, so reject rather than silently mislabel it.
    if header_size >= 124 && (read_u32(bytes, 112)? != 0 || read_u32(bytes, 116)? != 0) {
        return None;
    }

    let decoded = if matches!(compression, BI_PNG | BI_JPEG) {
        if header_size < 124 || size_image == 0 {
            return None;
        }
        let end = header_size.checked_add(size_image)?;
        let encoded = bytes.get(header_size..end)?;
        image::load_from_memory(encoded).ok()?
    } else {
        let bitfield_bytes = match (header_size, compression) {
            (40, BI_BITFIELDS) => 12usize,
            (40, BI_ALPHABITFIELDS) => 16usize,
            (_, BI_BITFIELDS | BI_ALPHABITFIELDS | 0 | 1 | 2) => 0,
            _ => return None,
        };
        let palette_entries = if bit_count <= 8 {
            if colors_used == 0 {
                1usize.checked_shl(u32::from(bit_count))?
            } else {
                colors_used
            }
        } else {
            0
        };
        let palette_bytes = palette_entries.checked_mul(4)?;
        let dib_pixel_offset = header_size
            .checked_add(bitfield_bytes)?
            .checked_add(palette_bytes)?;
        let dib_pixels = bytes.get(dib_pixel_offset..)?;

        let bits_per_row = u64::from(width).checked_mul(u64::from(bit_count))?;
        let stride = bits_per_row
            .checked_add(31)?
            .checked_div(32)?
            .checked_mul(4)?;
        let expected_pixel_bytes = usize::try_from(stride.checked_mul(u64::from(height))?).ok()?;
        if expected_pixel_bytes == 0
            || dib_pixels.len() < expected_pixel_bytes
            || (size_image != 0 && size_image < expected_pixel_bytes)
        {
            return None;
        }

        let file_offset = 14usize.checked_add(dib_pixel_offset)?;
        let file_size = 14usize.checked_add(bytes.len())?;
        let file_offset = u32::try_from(file_offset).ok()?;
        let file_size = u32::try_from(file_size).ok()?;
        let mut bmp = Vec::with_capacity(file_size as usize);
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&file_size.to_le_bytes());
        bmp.extend_from_slice(&[0; 4]);
        bmp.extend_from_slice(&file_offset.to_le_bytes());
        bmp.extend_from_slice(bytes);
        image::load_from_memory_with_format(&bmp, ImageFormat::Bmp).ok()?
    };

    let rgba = decoded.to_rgba8();
    if rgba.width() != width || rgba.height() != height {
        return None;
    }
    let mut png = Vec::new();
    decoded
        .write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
        .ok()?;
    if png.is_empty() || png.len() > MAX_PNG_BYTES {
        return None;
    }
    Some((png, width, height))
}

/// Re-copy a real clipboard payload. Files are always replayed as a copy;
/// stale paths fail before the current clipboard is emptied.
pub fn write_payload(payload: &ClipboardPayload) -> Result<u32, String> {
    match payload {
        ClipboardPayload::Text(text) => {
            let mut bytes = Vec::with_capacity((text.len() + 1) * 2);
            for unit in text.encode_utf16().chain(std::iter::once(0)) {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
            write_global_payload(CF_UNICODETEXT, &bytes)
        }
        ClipboardPayload::Files(paths) => {
            let bytes = encode_hdrop(paths)?;
            write_global_payload(CF_HDROP, &bytes)
        }
        ClipboardPayload::Image { png, width, height } => {
            let bytes = encode_dibv5(png, *width, *height)?;
            write_global_payload(CF_DIBV5, &bytes)
        }
    }
}

/// Compatibility wrapper for existing text-only callers.
pub fn write_unicode_text(text: &str) -> Option<u32> {
    write_payload(&ClipboardPayload::Text(text.to_string())).ok()
}

fn encode_hdrop(paths: &[String]) -> Result<Vec<u8>, String> {
    if paths.is_empty() || paths.len() > MAX_FILE_COUNT as usize {
        return Err("文件列表为空或项目过多".to_string());
    }
    let mut encoded_paths = Vec::with_capacity(paths.len());
    for value in paths {
        let path = Path::new(value);
        if !path.is_absolute() || !path.exists() {
            return Err(format!("文件或文件夹已不存在：{value}"));
        }
        if value.contains('\0') {
            return Err("文件路径包含无效字符".to_string());
        }
        let encoded: Vec<u16> = value.encode_utf16().collect();
        if encoded.is_empty() || encoded.len() > MAX_PATH_UNITS {
            return Err("文件路径长度超出支持范围".to_string());
        }
        encoded_paths.push(encoded);
    }
    encode_hdrop_wide(&encoded_paths)
}

fn encode_hdrop_wide(paths: &[Vec<u16>]) -> Result<Vec<u8>, String> {
    if paths.is_empty() || paths.len() > MAX_FILE_COUNT as usize {
        return Err("文件列表为空或项目过多".to_string());
    }
    let mut name_list = Vec::<u16>::new();
    for path in paths {
        if path.is_empty() || path.len() > MAX_PATH_UNITS || path.contains(&0) {
            return Err("文件路径无效或超出支持范围".to_string());
        }
        name_list.extend(path);
        name_list.push(0);
        if name_list
            .len()
            .checked_mul(2)
            .is_none_or(|size| size > MAX_FILE_LIST_BYTES)
        {
            return Err("文件列表超出支持大小".to_string());
        }
    }
    name_list.push(0);

    let header_size = 20usize;
    let total_size = header_size
        .checked_add(name_list.len().checked_mul(2).ok_or("文件列表过大")?)
        .ok_or("文件列表过大")?;
    let mut bytes = Vec::with_capacity(total_size);
    bytes.extend_from_slice(&(header_size as u32).to_le_bytes()); // DROPFILES.pFiles
    bytes.extend_from_slice(&0i32.to_le_bytes()); // DROPFILES.pt.x
    bytes.extend_from_slice(&0i32.to_le_bytes()); // DROPFILES.pt.y
    bytes.extend_from_slice(&0u32.to_le_bytes()); // fNC = FALSE
    bytes.extend_from_slice(&1u32.to_le_bytes()); // fWide = TRUE
    for unit in name_list {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    Ok(bytes)
}

fn encode_dibv5(png: &[u8], declared_width: u32, declared_height: u32) -> Result<Vec<u8>, String> {
    if png.is_empty() || png.len() > MAX_PNG_BYTES {
        return Err("图片数据为空或超出支持大小".to_string());
    }
    let decoded = image::load_from_memory_with_format(png, ImageFormat::Png)
        .map_err(|_| "图片数据无效".to_string())?;
    let width = decoded.width();
    let height = decoded.height();
    if width != declared_width
        || height != declared_height
        || width == 0
        || height == 0
        || u64::from(width)
            .checked_mul(u64::from(height))
            .is_none_or(|pixels| pixels > MAX_IMAGE_PIXELS)
    {
        return Err("图片尺寸无效或超出支持范围".to_string());
    }
    let pixel_bytes = usize::try_from(u64::from(width) * u64::from(height) * 4)
        .map_err(|_| "图片尺寸超出支持范围".to_string())?;
    let total = 124usize
        .checked_add(pixel_bytes)
        .ok_or_else(|| "图片尺寸超出支持范围".to_string())?;
    let image_size = u32::try_from(pixel_bytes).map_err(|_| "图片数据超出支持大小".to_string())?;

    let mut dib = vec![0u8; total];
    write_u32(&mut dib, 0, 124);
    write_i32(&mut dib, 4, width as i32);
    write_i32(&mut dib, 8, -(height as i32)); // top-down rows
    write_u16(&mut dib, 12, 1);
    write_u16(&mut dib, 14, 32);
    write_u32(&mut dib, 16, BI_BITFIELDS);
    write_u32(&mut dib, 20, image_size);
    write_u32(&mut dib, 40, 0x00ff_0000); // red mask
    write_u32(&mut dib, 44, 0x0000_ff00); // green mask
    write_u32(&mut dib, 48, 0x0000_00ff); // blue mask
    write_u32(&mut dib, 52, 0xff00_0000); // alpha mask
    write_u32(&mut dib, 56, LCS_SRGB);

    let mut bgra = decoded.to_rgba8().into_raw();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    dib[124..].copy_from_slice(&bgra);
    Ok(dib)
}

fn write_global_payload(format: u32, bytes: &[u8]) -> Result<u32, String> {
    if bytes.is_empty() || bytes.len() > MAX_DIB_BYTES.max(MAX_TEXT_UNITS * 2) {
        return Err("剪贴板数据超出支持大小".to_string());
    }
    unsafe {
        let owner = clipboard_owner_hwnd().ok_or_else(|| "剪贴板窗口尚未启动".to_string())?;
        let hglobal = GlobalAlloc(GMEM_MOVEABLE, bytes.len())
            .map_err(|error| format!("无法分配剪贴板内存：{error}"))?;
        let pointer = GlobalLock(hglobal);
        if pointer.is_null() {
            let _ = GlobalFree(Some(hglobal));
            return Err("无法锁定剪贴板内存".to_string());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.cast::<u8>(), bytes.len());
        let _ = GlobalUnlock(hglobal);

        if let Err(error) = OpenClipboard(Some(owner)) {
            let _ = GlobalFree(Some(hglobal));
            return Err(format!("无法打开剪贴板：{error}"));
        }
        if let Err(error) = EmptyClipboard() {
            let _ = CloseClipboard();
            let _ = GlobalFree(Some(hglobal));
            return Err(format!("无法清空剪贴板：{error}"));
        }
        if let Err(error) = SetClipboardData(format, Some(HANDLE(hglobal.0 as *mut _))) {
            let _ = GlobalFree(Some(hglobal));
            let _ = CloseClipboard();
            return Err(format!("无法写入剪贴板：{error}"));
        }
        let _ = CloseClipboard();
        Ok(GetClipboardSequenceNumber())
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn read_i32(bytes: &[u8], offset: usize) -> Option<i32> {
    Some(i32::from_le_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_i32(bytes: &mut [u8], offset: usize, value: i32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hdrop_encodes_unicode_paths_as_double_nul_terminated_wide_list() {
        let encoded = encode_hdrop_wide(&[
            "R:\\项目".encode_utf16().collect(),
            "R:\\项目_二".encode_utf16().collect(),
        ])
        .unwrap();
        let header = &encoded[..20];
        assert_eq!(&header[0..4], &20u32.to_le_bytes());
        assert_eq!(&header[4..12], &[0; 8]);
        assert_eq!(&header[12..16], &0u32.to_le_bytes());
        assert_eq!(&header[16..20], &1u32.to_le_bytes());
        let units: Vec<u16> = encoded[20..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        assert_eq!(units.last(), Some(&0));
        assert_eq!(units.iter().rev().take_while(|unit| **unit == 0).count(), 2);
        assert!(String::from_utf16_lossy(&units).contains("项目"));
    }

    #[test]
    fn png_round_trips_through_dibv5_and_preserves_dimensions() {
        let mut png = Vec::new();
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            1,
            image::Rgba([12, 34, 56, 78]),
        ));
        image
            .write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
            .unwrap();
        let dib = encode_dibv5(&png, 2, 1).unwrap();
        let (round_trip, width, height) = dib_to_png(CF_DIBV5, &dib).unwrap();
        let decoded = image::load_from_memory_with_format(&round_trip, ImageFormat::Png)
            .unwrap()
            .to_rgba8();
        assert_eq!((width, height), (2, 1));
        assert_eq!(decoded.get_pixel(0, 0).0, [12, 34, 56, 78]);
        assert_eq!(round_trip, png);
        assert_eq!(
            veya_core::payload_hash(&ClipboardPayload::Image {
                png: png.clone(),
                width,
                height,
            }),
            veya_core::payload_hash(&ClipboardPayload::Image {
                png: round_trip,
                width,
                height,
            })
        );
    }

    #[test]
    fn dib_rejects_excessive_or_truncated_dimensions() {
        let mut dib = vec![0; 124];
        write_u32(&mut dib, 0, 124);
        write_i32(&mut dib, 4, 4096);
        write_i32(&mut dib, 8, 4096);
        write_u16(&mut dib, 12, 1);
        write_u16(&mut dib, 14, 32);
        assert!(dib_to_png(CF_DIBV5, &dib).is_none());

        write_i32(&mut dib, 4, 1);
        write_i32(&mut dib, 8, 1);
        assert!(dib_to_png(CF_DIBV5, &dib).is_none());
    }
}
