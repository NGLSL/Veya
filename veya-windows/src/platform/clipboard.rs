//! Clipboard capture and replay. Win32 clipboard ownership stays in this module.

use std::cell::RefCell;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use image::{ImageDecoder, ImageFormat};
use sha2::{Digest, Sha256};
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData,
    GetClipboardOwner, GetClipboardSequenceNumber, GetOpenClipboardWindow,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW,
    RemoveClipboardFormatListener, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, KillTimer, SetTimer,
};

use super::time::now_ms;
use super::win::{clipboard_owner_hwnd, emit};
use super::{
    ClipboardChangeRaw, ClipboardImageFormat, ClipboardImageRaw, ClipboardPayloadRaw,
    ClipboardWriteError, PasteTriggerRaw, PlatformEvent,
};
use veya_core::{ClipboardPayload, SourceConfidence};

/// Sent to the window when clipboard content changes.
pub const WM_CLIPBOARDUPDATE: u32 = 0x031D;

pub const RETRY_TIMER_ID: usize = 0x5643;
const RETRY_INTERVAL_MS: u32 = 25;
const RETRY_TIMEOUT: Duration = Duration::from_secs(1);
const MAX_DEFERRED_PASTES: usize = 32;

struct PendingCapture {
    sequence: u32,
    owner_hwnd: isize,
    source_pid: u32,
    source_confidence: SourceConfidence,
    timestamp_ms: i64,
    started: Instant,
}

#[derive(Default)]
struct CaptureState {
    pending: Option<PendingCapture>,
    reading: bool,
    completed: Option<u32>,
    deferred_pastes: Vec<(u32, PasteTriggerRaw)>,
}

thread_local! {
    // Both the window callback and low-level keyboard hook run on the pump thread.
    static CAPTURE: RefCell<CaptureState> = RefCell::new(CaptureState::default());
}

const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;
const CF_DIBV5: u32 = 17;

const BI_BITFIELDS: u32 = 3;
const BI_JPEG: u32 = 4;
const BI_PNG: u32 = 5;
const BI_ALPHABITFIELDS: u32 = 6;
const LCS_SRGB: u32 = 0x7352_4742;
const LCS_GM_IMAGES: u32 = 4;

const MAX_DIB_BYTES: usize = 64 * 1024 * 1024;
const MAX_PNG_BYTES: usize = 32 * 1024 * 1024;
const MAX_IMAGE_CANDIDATE_BYTES: usize = MAX_DIB_BYTES + MAX_PNG_BYTES;
const MAX_IMAGE_PIXELS: u64 = 16 * 1024 * 1024;
const MAX_FILE_COUNT: u32 = 4096;
const MAX_PATH_UNITS: usize = 32_767;
const MAX_FILE_LIST_BYTES: usize = 4 * 1024 * 1024;
const MAX_TEXT_UNITS: usize = 1_000_000;

// The listener and replay worker use the same owner HWND. Windows can accept
// nested opens from those threads, so their Open/Close pairs must not overlap.
static CLIPBOARD_ACCESS: Mutex<()> = Mutex::new(());

pub fn install_listener(hwnd: HWND) -> windows::core::Result<()> {
    unsafe {
        AddClipboardFormatListener(hwnd)?;
    }
    Ok(())
}

pub fn uninstall_listener(hwnd: HWND) {
    CAPTURE.with(|state| *state.borrow_mut() = CaptureState::default());
    unsafe {
        let _ = KillTimer(Some(hwnd), RETRY_TIMER_ID);
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

/// Hold paste observations until their clipboard sequence has a final result.
/// A newer write invalidates observations waiting for the previous payload.
fn defer_paste(raw: &PasteTriggerRaw) -> bool {
    let sequence = unsafe { GetClipboardSequenceNumber() };
    CAPTURE.with(|state| {
        let mut state = state.borrow_mut();
        if state.completed == Some(sequence) && state.pending.is_none() {
            return false;
        }
        state.deferred_pastes.retain(|(seq, _)| *seq == sequence);
        if state.deferred_pastes.len() < MAX_DEFERRED_PASTES {
            state.deferred_pastes.push((sequence, raw.clone()));
        }
        true
    })
}

pub fn emit_paste(raw: PasteTriggerRaw) {
    if !defer_paste(&raw) {
        emit(PlatformEvent::PasteTrigger(raw));
    }
}

pub fn handle_clipboard_update(hwnd: HWND) {
    begin_capture(hwnd, false);
}

fn begin_capture(hwnd: HWND, from_timer: bool) {
    let sequence = unsafe { GetClipboardSequenceNumber() };
    let already_seen = CAPTURE.with(|state| {
        let state = state.borrow();
        state.reading
            || state.completed == Some(sequence)
            || state
                .pending
                .as_ref()
                .is_some_and(|pending| pending.sequence == sequence)
    });
    if already_seen {
        return;
    }
    let (mut owner_hwnd, mut source_pid, mut source_confidence) = resolve_source();
    // A timer can discover a new write before its notification is dispatched.
    // Only an actual owner proves its source; the current foreground may have changed.
    if from_timer && source_confidence != SourceConfidence::Exact {
        owner_hwnd = 0;
        source_pid = 0;
        source_confidence = SourceConfidence::Unknown;
    }
    CAPTURE.with(|state| {
        let mut state = state.borrow_mut();
        state.deferred_pastes.retain(|(seq, _)| *seq == sequence);
        state.pending = Some(PendingCapture {
            sequence,
            owner_hwnd,
            source_pid,
            source_confidence,
            timestamp_ms: now_ms(),
            started: Instant::now(),
        });
    });
    attempt_capture(hwnd);
}

pub fn handle_retry_timer(hwnd: HWND) {
    let sequence = unsafe { GetClipboardSequenceNumber() };
    let changed = CAPTURE.with(|state| {
        state
            .borrow()
            .pending
            .as_ref()
            .is_some_and(|pending| pending.sequence != sequence)
    });
    if changed {
        begin_capture(hwnd, true);
    } else {
        attempt_capture(hwnd);
    }
}

fn attempt_capture(hwnd: HWND) {
    let reading = CAPTURE.with(|state| state.borrow().reading);
    if reading {
        return;
    }
    let Some(sequence) = CAPTURE.with(|state| {
        state
            .borrow()
            .pending
            .as_ref()
            .map(|pending| pending.sequence)
    }) else {
        return;
    };
    // Replay owns the same HWND on the worker. Never wait for its mutex in the
    // message pump: hotkeys and shutdown must remain responsive while retrying.
    // Delayed rendering can pump nested window messages. The nested update is
    // handled by the next timer tick after the current read releases its locks.
    CAPTURE.with(|state| state.borrow_mut().reading = true);
    let result = match CLIPBOARD_ACCESS.try_lock() {
        Ok(_access) => read_clipboard_payload(hwnd, sequence),
        Err(std::sync::TryLockError::WouldBlock) => CaptureRead::Retry,
        Err(std::sync::TryLockError::Poisoned(_)) => CaptureRead::Complete(None),
    };
    CAPTURE.with(|state| state.borrow_mut().reading = false);
    let payload = match result {
        CaptureRead::Changed => {
            // Do not publish bytes from a newer write under the pending identity.
            // Its queued notification (or next timer tick) starts a fresh capture.
            if unsafe { SetTimer(Some(hwnd), RETRY_TIMER_ID, RETRY_INTERVAL_MS, None) } != 0 {
                return;
            }
            None
        }
        CaptureRead::Retry => {
            let expired = CAPTURE.with(|state| {
                state
                    .borrow()
                    .pending
                    .as_ref()
                    .is_none_or(|pending| pending.started.elapsed() >= RETRY_TIMEOUT)
            });
            if !expired
                && unsafe { SetTimer(Some(hwnd), RETRY_TIMER_ID, RETRY_INTERVAL_MS, None) } != 0
            {
                return;
            }
            None
        }
        CaptureRead::Complete(payload) => payload,
    };
    unsafe {
        let _ = KillTimer(Some(hwnd), RETRY_TIMER_ID);
    }
    let (pending, pastes) = CAPTURE.with(|state| {
        let mut state = state.borrow_mut();
        let pending = state.pending.take().unwrap();
        state.completed = Some(pending.sequence);
        (pending, std::mem::take(&mut state.deferred_pastes))
    });
    match payload {
        Some(payload) => emit(PlatformEvent::ClipboardChange(ClipboardChangeRaw {
            sequence: pending.sequence,
            payload,
            owner_hwnd: pending.owner_hwnd,
            source_pid: pending.source_pid,
            source_confidence: pending.source_confidence,
            timestamp_ms: pending.timestamp_ms,
        })),
        None => emit(PlatformEvent::ClipboardSkipped {
            sequence: pending.sequence,
        }),
    }
    for (sequence, paste) in pastes {
        if sequence == pending.sequence {
            emit(PlatformEvent::PasteTrigger(paste));
        }
    }
}

enum CaptureRead {
    Retry,
    Changed,
    Complete(Option<ClipboardPayloadRaw>),
}

/// Copy one prioritized payload; decoding happens later on the worker.
/// The caller holds `CLIPBOARD_ACCESS` without blocking the message pump.
fn read_clipboard_payload(hwnd: HWND, expected_sequence: u32) -> CaptureRead {
    unsafe {
        if OpenClipboard(Some(hwnd)).is_err() {
            return CaptureRead::Retry;
        }
        let result = if GetClipboardSequenceNumber() != expected_sequence {
            CaptureRead::Changed
        } else {
            let payload = if format_available(CF_HDROP) {
                read_hdrop().map(|mut paths| {
                    // Messaging apps can publish one temporary file alongside
                    // image pixels. Prefer a valid image, retaining the file as
                    // fallback; multi-file copies keep their original meaning.
                    if paths.len() == 1 {
                        let (_, images) = read_image_candidates();
                        if !images.is_empty() {
                            return ClipboardPayloadRaw::FileWithImages {
                                path: paths.remove(0),
                                images,
                            };
                        }
                    }
                    ClipboardPayloadRaw::Files(paths)
                })
            } else {
                let (has_images, images) = read_image_candidates();
                if has_images {
                    // Never turn a failed image into a browser's accompanying URL text.
                    (!images.is_empty()).then_some(ClipboardPayloadRaw::Images(images))
                } else if format_available(CF_UNICODETEXT) {
                    read_unicode_text().map(ClipboardPayloadRaw::Text)
                } else {
                    let _ = CloseClipboard();
                    return CaptureRead::Complete(None);
                }
            };
            // Delayed-rendering owners may advance the sequence while serving GetClipboardData.
            if GetClipboardSequenceNumber() != expected_sequence {
                CaptureRead::Changed
            } else {
                payload.map_or(CaptureRead::Retry, |payload| {
                    CaptureRead::Complete(Some(payload))
                })
            }
        };
        let _ = CloseClipboard();
        result
    }
}

/// Retain bounded alternatives so one malformed representation cannot mask another.
/// Each DIB keeps its existing 64 MiB allowance; each encoded-image family shares
/// 32 MiB across aliases, within a 96 MiB total including the next read.
unsafe fn read_image_candidates() -> (bool, Vec<ClipboardImageRaw>) {
    let mut available = false;
    let mut candidates = Vec::new();
    let mut budget = MAX_IMAGE_CANDIDATE_BYTES;
    for format in [CF_DIBV5, CF_DIB] {
        if format_available(format) {
            available = true;
            if let Some(bytes) = read_image_data(
                format,
                MAX_DIB_BYTES.min(budget),
                ClipboardImageFormat::Dib(format),
                &candidates,
            ) {
                budget -= bytes.len();
                candidates.push(ClipboardImageRaw {
                    format: ClipboardImageFormat::Dib(format),
                    bytes,
                });
            }
        }
    }
    for (kind, names) in [
        (
            ClipboardImageFormat::Png,
            &["PNG", "image/png", "image/x-png"][..],
        ),
        (
            ClipboardImageFormat::Jpeg,
            &[
                "JFIF",
                "JPEG",
                "JPG",
                "image/jpeg",
                "image/jpg",
                "image/pjpeg",
            ][..],
        ),
        (ClipboardImageFormat::WebP, &["WebP", "image/webp"][..]),
        (ClipboardImageFormat::Gif, &["GIF", "image/gif"][..]),
    ] {
        let mut remaining = MAX_PNG_BYTES;
        let mut formats = Vec::new();
        for name in names {
            let name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            let format = RegisterClipboardFormatW(windows::core::PCWSTR(name.as_ptr()));
            if format == 0 || formats.contains(&format) {
                continue;
            }
            formats.push(format);
            if !format_available(format) {
                continue;
            }
            available = true;
            if let Some(bytes) = read_image_data(format, remaining.min(budget), kind, &candidates) {
                remaining -= bytes.len();
                budget -= bytes.len();
                candidates.push(ClipboardImageRaw {
                    format: kind,
                    bytes,
                });
            }
        }
    }
    (available, candidates)
}

/// Compare aliases while borrowing the OS allocation, before allocating a copy.
unsafe fn read_image_data(
    format: u32,
    max_bytes: usize,
    kind: ClipboardImageFormat,
    candidates: &[ClipboardImageRaw],
) -> Option<Vec<u8>> {
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
    let bytes = std::slice::from_raw_parts(locked.cast::<u8>(), size);
    let duplicate = candidates
        .iter()
        .any(|candidate| candidate.format == kind && candidate.bytes == bytes);
    let result = (!duplicate).then(|| bytes.to_vec());
    let _ = GlobalUnlock(hglobal);
    result
}

pub fn normalize_image(candidate: ClipboardImageRaw) -> Option<ClipboardPayload> {
    let ClipboardImageRaw { format, bytes } = candidate;
    let (png, width, height) = match format {
        ClipboardImageFormat::Dib(format) => dib_to_png(format, &bytes)?,
        ClipboardImageFormat::Png => {
            let (width, height) = png_dimensions(&bytes)?;
            (bytes, width, height)
        }
        format => {
            let format = match format {
                ClipboardImageFormat::Jpeg => ImageFormat::Jpeg,
                ClipboardImageFormat::WebP => ImageFormat::WebP,
                ClipboardImageFormat::Gif => ImageFormat::Gif,
                _ => unreachable!(),
            };
            let decoded = decode_bounded(&bytes, format, MAX_PNG_BYTES)?;
            let (width, height) = (decoded.width(), decoded.height());
            let png = encode_png(decoded)?;
            (png, width, height)
        }
    };
    Some(ClipboardPayload::Image {
        png: png.into(),
        width,
        height,
    })
}

/// ImageDecoder::read_image reads only the first GIF/WebP frame, without
/// collecting animation frames. Validate dimensions before allocating pixels.
fn decode_bounded(
    bytes: &[u8],
    format: ImageFormat,
    max_encoded_bytes: usize,
) -> Option<image::DynamicImage> {
    if bytes.is_empty() || bytes.len() > max_encoded_bytes {
        return None;
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_PIXELS as u32);
    limits.max_image_height = Some(MAX_IMAGE_PIXELS as u32);
    limits.max_alloc = Some(MAX_DIB_BYTES as u64);
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    reader.limits(limits.clone());
    let mut decoder = reader.into_decoder().ok()?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
        return None;
    }
    // Share the 64 MiB decoding budget between output pixels and decoder scratch.
    limits.reserve(decoder.total_bytes()).ok()?;
    decoder.set_limits(limits).ok()?;
    image::DynamicImage::from_decoder(decoder).ok()
}

fn encode_png(decoded: image::DynamicImage) -> Option<Vec<u8>> {
    let mut png = Vec::new();
    decoded
        .write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
        .ok()?;
    (!png.is_empty() && png.len() <= MAX_PNG_BYTES).then_some(png)
}

/// Validate a registered PNG before retaining its original bytes and alpha.
pub fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.is_empty() || bytes.len() > MAX_PNG_BYTES {
        return None;
    }
    let mut decoder = image::codecs::png::PngDecoder::new(std::io::Cursor::new(bytes)).ok()?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
        return None;
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(width);
    limits.max_image_height = Some(height);
    limits.max_alloc = Some(MAX_DIB_BYTES as u64);
    decoder.set_limits(limits).ok()?;
    image::DynamicImage::from_decoder(decoder).ok()?;
    Some((width, height))
}

fn format_available(format: u32) -> bool {
    unsafe { IsClipboardFormatAvailable(format).is_ok() }
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
        let format = if compression == BI_PNG {
            ImageFormat::Png
        } else {
            ImageFormat::Jpeg
        };
        // Preserve the existing DIB decoder allocation conditions, including
        // 16-bit PNGs whose decoded storage can exceed 64 MiB. The outer DIB
        // already bounds input bytes and pixels; validate the actual dimensions
        // before pixel allocation rather than applying registered-format limits.
        let decoder = image::ImageReader::with_format(std::io::Cursor::new(encoded), format)
            .into_decoder()
            .ok()?;
        if decoder.dimensions() != (width, height) {
            return None;
        }
        image::DynamicImage::from_decoder(decoder).ok()?
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

        let file_offset = u32::try_from(14usize.checked_add(dib_pixel_offset)?).ok()?;
        let file_size = u32::try_from(14usize.checked_add(bytes.len())?).ok()?;
        let mut header = [0u8; 14];
        header[..2].copy_from_slice(b"BM");
        header[2..6].copy_from_slice(&file_size.to_le_bytes());
        header[10..14].copy_from_slice(&file_offset.to_le_bytes());
        // Keep the explicit pixel offset, including V5 bitfields, without
        // copying the entire DIB to prepend the 14-byte BMP file header.
        let reader = BmpDibReader {
            header,
            dib: bytes,
            position: 0,
        };
        let decoder = image::codecs::bmp::BmpDecoder::new(std::io::BufReader::new(reader)).ok()?;
        image::DynamicImage::from_decoder(decoder).ok()?
    };

    if decoded.width() != width || decoded.height() != height {
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

/// Seekable BMP file view over a small file header and borrowed DIB bytes.
struct BmpDibReader<'a> {
    header: [u8; 14],
    dib: &'a [u8],
    position: u64,
}

impl std::io::Read for BmpDibReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let source = if self.position < 14 {
            &self.header[self.position as usize..]
        } else {
            let offset = usize::try_from(self.position - 14).unwrap_or(usize::MAX);
            self.dib.get(offset..).unwrap_or_default()
        };
        let length = source.len().min(output.len());
        output[..length].copy_from_slice(&source[..length]);
        self.position += length as u64;
        Ok(length)
    }
}

impl std::io::Seek for BmpDibReader<'_> {
    fn seek(&mut self, from: std::io::SeekFrom) -> std::io::Result<u64> {
        let position = match from {
            std::io::SeekFrom::Start(position) => i128::from(position),
            std::io::SeekFrom::Current(offset) => i128::from(self.position) + i128::from(offset),
            std::io::SeekFrom::End(offset) => 14 + self.dib.len() as i128 + i128::from(offset),
        };
        self.position = u64::try_from(position).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid BMP seek")
        })?;
        Ok(self.position)
    }
}

/// Re-copy a real clipboard payload. Files are always replayed as a copy;
/// stale paths fail before the current clipboard is emptied.
pub fn write_payload(payload: &ClipboardPayload) -> Result<u32, ClipboardWriteError> {
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
            let dibv5 = encode_dibv5(png, *width, *height)?;
            let png_format = unsafe { RegisterClipboardFormatW(windows::core::w!("PNG")) };
            if png_format == 0 {
                return Err("无法注册 PNG 剪贴板格式".into());
            }
            // Retain only the final Windows buffers and compact verification
            // data. Finish every conversion before changing the clipboard.
            let mut buffers = Vec::with_capacity(3);
            let mut expected = Vec::with_capacity(3);
            prepare_image_format(CF_DIBV5, &dibv5, &mut buffers, &mut expected)?;
            let dib = encode_legacy_dib(&dibv5, *width, *height)?;
            drop(dibv5);
            prepare_image_format(CF_DIB, &dib, &mut buffers, &mut expected)?;
            drop(dib);
            prepare_image_format(png_format, png, &mut buffers, &mut expected)?;
            // Preserve the original publication order: PNG, DIBV5, DIB.
            buffers.rotate_right(1);
            expected.rotate_right(1);
            write_prepared_payloads(buffers, &expected)
        }
    }
}

/// Compatibility wrapper for existing text-only callers.
pub fn write_unicode_text(text: &str) -> Option<u32> {
    write_payload(&ClipboardPayload::Text(text.to_string().into())).ok()
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
        .filter(|total| *total <= MAX_DIB_BYTES)
        .ok_or_else(|| "图片尺寸超出支持范围".to_string())?;
    let image_size = u32::try_from(pixel_bytes).map_err(|_| "图片数据超出支持大小".to_string())?;

    // Reuse an RGBA decoder buffer, consuming other PNG color types when
    // conversion is necessary. A borrowed conversion would clone RGBA.
    let mut dib = decoded.into_rgba8().into_raw();
    for pixel in dib.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    // Insert the small header into the owned decoder buffer instead of keeping
    // a second full-sized destination alongside the decoded pixels.
    dib.reserve_exact(124);
    dib.resize(total, 0);
    dib.copy_within(..pixel_bytes, 124);
    dib[..124].fill(0);
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
    write_u32(&mut dib, 108, LCS_GM_IMAGES);

    Ok(dib)
}

/// Older consumers expect a 40-byte header with opaque, bottom-up BGR rows.
/// The V5 buffer already contains decoded pixels, so do not decode PNG twice.
fn encode_legacy_dib(dibv5: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let width = width as usize;
    let height = height as usize;
    let stride = width
        .checked_mul(3)
        .and_then(|bytes| bytes.checked_add(3))
        .map(|bytes| bytes & !3)
        .ok_or("图片尺寸超出支持范围")?;
    let pixel_bytes = stride.checked_mul(height).ok_or("图片尺寸超出支持范围")?;
    let total = 40usize
        .checked_add(pixel_bytes)
        .filter(|total| *total <= MAX_DIB_BYTES)
        .ok_or("图片数据超出支持大小")?;
    let source_stride = width.checked_mul(4).ok_or("图片尺寸超出支持范围")?;
    let source_bytes = source_stride
        .checked_mul(height)
        .ok_or("图片尺寸超出支持范围")?;
    let pixels = dibv5
        .get(124..)
        .filter(|pixels| pixels.len() == source_bytes)
        .ok_or("图片数据无效")?;
    let mut dib = vec![0u8; total];
    write_u32(&mut dib, 0, 40);
    write_i32(&mut dib, 4, width as i32);
    write_i32(&mut dib, 8, height as i32);
    write_u16(&mut dib, 12, 1);
    write_u16(&mut dib, 14, 24); // BI_RGB = 0, no masks or palette
    write_u32(&mut dib, 20, pixel_bytes as u32);
    for (row, source) in pixels.chunks_exact(source_stride).enumerate() {
        let destination = 40 + (height - 1 - row) * stride;
        for (column, pixel) in source.chunks_exact(4).enumerate() {
            let alpha = u32::from(pixel[3]);
            for channel in 0..3 {
                // Composite straight alpha onto white for formats without alpha.
                dib[destination + column * 3 + channel] =
                    ((u32::from(pixel[channel]) * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
            }
        }
    }
    Ok(dib)
}

/// Frees only memory whose ownership has not passed to Windows.
struct ClipboardBuffer(Option<HGLOBAL>);

impl ClipboardBuffer {
    unsafe fn new(bytes: &[u8]) -> Result<Self, String> {
        let buffer = Self(Some(
            GlobalAlloc(GMEM_MOVEABLE, bytes.len())
                .map_err(|error| format!("无法分配剪贴板内存：{error}"))?,
        ));
        let handle = buffer.0.unwrap();
        let pointer = GlobalLock(handle);
        if pointer.is_null() {
            return Err("无法锁定剪贴板内存".to_string());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.cast::<u8>(), bytes.len());
        let _ = GlobalUnlock(handle);
        Ok(buffer)
    }
}

impl Drop for ClipboardBuffer {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            unsafe {
                let _ = GlobalFree(Some(handle));
            }
        }
    }
}

fn write_global_payload(format: u32, bytes: &[u8]) -> Result<u32, ClipboardWriteError> {
    write_global_payloads(&[(format, bytes)])
}

fn write_global_payloads(payloads: &[(u32, &[u8])]) -> Result<u32, ClipboardWriteError> {
    if payloads.is_empty()
        || payloads.iter().any(|(_, bytes)| {
            bytes.is_empty() || bytes.len() > MAX_DIB_BYTES.max(MAX_TEXT_UNITS * 2)
        })
    {
        return Err("剪贴板数据超出支持大小".into());
    }
    let buffers = payloads
        .iter()
        .map(|(_, bytes)| unsafe { ClipboardBuffer::new(bytes) })
        .collect::<Result<Vec<_>, _>>()?;
    let expected: Vec<_> = payloads
        .iter()
        .map(|(format, bytes)| (*format, ReplayContent::Bytes(bytes)))
        .collect();
    write_prepared_payloads(buffers, &expected)
}

enum ReplayContent<'a> {
    Bytes(&'a [u8]),
    Image { length: usize, digest: [u8; 32] },
}

impl ReplayContent<'_> {
    fn len(&self) -> usize {
        match self {
            Self::Bytes(bytes) => bytes.len(),
            Self::Image { length, .. } => *length,
        }
    }

    fn matches(&self, bytes: &[u8]) -> bool {
        match self {
            Self::Bytes(expected) => replay_bytes_match(expected, bytes),
            Self::Image { length, digest } => {
                bytes.len() == *length && Sha256::digest(bytes).as_slice() == digest
            }
        }
    }
}

fn prepare_image_format(
    format: u32,
    bytes: &[u8],
    buffers: &mut Vec<ClipboardBuffer>,
    expected: &mut Vec<(u32, ReplayContent<'static>)>,
) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > MAX_DIB_BYTES {
        return Err("剪贴板数据超出支持大小".to_string());
    }
    let content = ReplayContent::Image {
        length: bytes.len(),
        digest: Sha256::digest(bytes).into(),
    };
    buffers.push(unsafe { ClipboardBuffer::new(bytes)? });
    expected.push((format, content));
    Ok(())
}

fn write_prepared_payloads(
    mut buffers: Vec<ClipboardBuffer>,
    expected: &[(u32, ReplayContent<'_>)],
) -> Result<u32, ClipboardWriteError> {
    let _access = CLIPBOARD_ACCESS
        .lock()
        .map_err(|_| "无法写入剪贴板：剪贴板访问锁已失效。".to_string())?;
    unsafe {
        let owner = clipboard_owner_hwnd().ok_or_else(|| "剪贴板窗口尚未启动".to_string())?;
        if let Err(error) = OpenClipboard(Some(owner)) {
            return Err(format!("无法打开剪贴板：{error}").into());
        }
        if let Err(error) = EmptyClipboard() {
            let _ = CloseClipboard();
            return Err(format!("无法清空剪贴板：{error}").into());
        }
        // From here onward, this attempt has already changed the clipboard,
        // even if writing or read-only confirmation subsequently fails.
        let changed_result: Result<u32, String> = (|| {
            for ((format, _), buffer) in expected.iter().zip(&mut buffers) {
                let hglobal = buffer.0.unwrap();
                if let Err(error) = SetClipboardData(*format, Some(HANDLE(hglobal.0 as *mut _))) {
                    let _ = CloseClipboard();
                    return Err(format!("无法写入剪贴板：{error}"));
                }
                buffer.0 = None;
            }
            // Closing a write can advance the sequence while Windows adds automatic
            // formats. Reopen read-only and bind the settled sequence to our owner
            // and exact original format bytes, rather than acknowledging a new writer.
            CloseClipboard().map_err(|error| format!("无法确认剪贴板写入完成：{error}"))?;
            OpenClipboard(Some(owner))
                .map_err(|error| format!("无法确认剪贴板内容，请重新选择：{error}"))?;
            let verification = (|| {
                if GetClipboardOwner().unwrap_or_default() != owner {
                    return Err("剪贴板已变化，请重新选择。".to_string());
                }
                for (format, content) in expected {
                    let handle = GetClipboardData(*format)
                        .map_err(|error| format!("无法确认剪贴板内容，请重新选择：{error}"))?;
                    let current = HGLOBAL(handle.0);
                    if GlobalSize(current) < content.len() {
                        return Err("剪贴板已变化，请重新选择。".to_string());
                    }
                    let pointer = GlobalLock(current);
                    if pointer.is_null() {
                        return Err("无法确认剪贴板内存，请重新选择。".to_string());
                    }
                    // Borrow only the payload range; GlobalAlloc may add trailing padding.
                    // The system owns this handle after SetClipboardData, so never free it.
                    let current_bytes =
                        std::slice::from_raw_parts(pointer.cast::<u8>(), content.len());
                    let matches = content.matches(current_bytes);
                    let _ = GlobalUnlock(current);
                    if !matches {
                        return Err("剪贴板已变化，请重新选择。".to_string());
                    }
                }
                Ok(GetClipboardSequenceNumber())
            })();
            CloseClipboard().map_err(|error| format!("无法确认剪贴板读取完成：{error}"))?;
            verification
        })();
        changed_result.map_err(|message| ClipboardWriteError {
            message,
            changed: true,
        })
    }
}

fn replay_bytes_match(expected: &[u8], current: &[u8]) -> bool {
    current.get(..expected.len()) == Some(expected)
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
    fn rejected_payloads_report_clipboard_unchanged() {
        let errors = [
            write_global_payload(CF_UNICODETEXT, &[]).unwrap_err(),
            write_payload(&ClipboardPayload::Files(Vec::new())).unwrap_err(),
            write_payload(&ClipboardPayload::Image {
                png: Vec::new().into(),
                width: 1,
                height: 1,
            })
            .unwrap_err(),
        ];
        for error in errors {
            assert!(!error.changed);
            assert!(!error.message.is_empty());
            assert_eq!(error.to_string(), error.message);
        }
    }

    #[test]
    fn replay_verification_compares_exact_payload_and_ignores_allocation_padding() {
        let payload = [0x56, 0, 0x20, 0, 0, 0];
        assert!(replay_bytes_match(&payload, &payload));
        assert!(replay_bytes_match(
            &payload,
            &[0x56, 0, 0x20, 0, 0, 0, 0x99, 0x88]
        ));
        assert!(!replay_bytes_match(&payload, &payload[..5]));
        assert!(!replay_bytes_match(&payload, &[0x56, 0, 0x21, 0, 0, 0]));
        assert!(!replay_bytes_match(&payload, &[0x56, 0, 0x20, 0, 0, 1]));
    }

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
                png: png.clone().into(),
                width,
                height,
            }),
            veya_core::payload_hash(&ClipboardPayload::Image {
                png: round_trip.into(),
                width,
                height,
            })
        );
    }

    #[test]
    fn dibv5_replay_preserves_png_color_conversion_and_alpha() {
        let images = [
            image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                2,
                1,
                image::Rgb([12, 34, 56]),
            )),
            image::DynamicImage::ImageLuma8(image::GrayImage::from_pixel(2, 1, image::Luma([123]))),
            image::DynamicImage::ImageLumaA8(image::GrayAlphaImage::from_pixel(
                2,
                1,
                image::LumaA([123, 78]),
            )),
            image::DynamicImage::ImageRgba16(image::ImageBuffer::from_pixel(
                2,
                1,
                image::Rgba([0x1234u16, 0x5678, 0x9abc, 0xdef0]),
            )),
        ];
        for image in images {
            let expected = image.to_rgba8();
            let mut png = Vec::new();
            image
                .write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
                .unwrap();
            let dib = encode_dibv5(&png, 2, 1).unwrap();
            let (round_trip, width, height) = dib_to_png(CF_DIBV5, &dib).unwrap();
            let actual = image::load_from_memory_with_format(&round_trip, ImageFormat::Png)
                .unwrap()
                .into_rgba8();
            assert_eq!((width, height), (2, 1));
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn embedded_png_dib_checks_decoded_dimensions() {
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            1,
            image::Rgba([12, 34, 56, 78]),
        ))
        .write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
        .unwrap();
        let mut dib = vec![0; 124];
        write_u32(&mut dib, 0, 124);
        write_i32(&mut dib, 4, 2);
        write_i32(&mut dib, 8, 1);
        write_u16(&mut dib, 12, 1);
        write_u16(&mut dib, 14, 32);
        write_u32(&mut dib, 16, BI_PNG);
        write_u32(&mut dib, 20, png.len() as u32);
        dib.extend_from_slice(&png);
        assert_eq!(dib_to_png(CF_DIBV5, &dib), Some((png, 2, 1)));
        write_i32(&mut dib, 4, 1);
        assert!(dib_to_png(CF_DIBV5, &dib).is_none());
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
