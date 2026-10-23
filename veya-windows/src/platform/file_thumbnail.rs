//! Small file previews provided by Windows Shell; payloads remain file paths.

use std::{marker::PhantomData, path::Path, rc::Rc};

/// An opaque, top-down RGBA thumbnail suitable for a UI image handle.
pub struct FileThumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Reuse on one background thread. COM initialization and teardown stay there.
/// The reader is deliberately neither Send nor Sync.
pub struct ThumbnailReader {
    _thread_bound: PhantomData<Rc<()>>,
}

impl ThumbnailReader {
    /// Initialize COM on the calling thread. An incompatible apartment returns None.
    pub fn new() -> Option<Self> {
        #[cfg(windows)]
        {
            use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().ok()? };
            Some(Self {
                _thread_bound: PhantomData,
            })
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    /// Request a real thumbnail, never a file icon. Maximum supported edge is 512px.
    pub fn read(&self, path: &Path, max_edge: u32) -> Option<FileThumbnail> {
        #[cfg(windows)]
        {
            native::read(path, max_edge)
        }
        #[cfg(not(windows))]
        {
            let _ = (path, max_edge);
            None
        }
    }
}

impl Drop for ThumbnailReader {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            windows::Win32::System::Com::CoUninitialize();
        }
    }
}

#[cfg(windows)]
mod native {
    use super::FileThumbnail;
    use std::{mem::size_of, os::windows::ffi::OsStrExt, path::Path};
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::SIZE,
            Graphics::Gdi::{
                CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP,
                BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
            },
            UI::Shell::{
                IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_THUMBNAILONLY,
            },
        },
    };

    struct Bitmap(HBITMAP);
    impl Drop for Bitmap {
        fn drop(&mut self) {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(self.0 .0));
            }
        }
    }

    struct Dc(HDC);
    impl Drop for Dc {
        fn drop(&mut self) {
            unsafe {
                let _ = DeleteDC(self.0);
            }
        }
    }

    pub(super) fn read(path: &Path, max_edge: u32) -> Option<FileThumbnail> {
        if !path.is_absolute() || !(1..=512).contains(&max_edge) {
            return None;
        }
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        if wide.contains(&0) {
            return None;
        }
        wide.push(0);
        let factory: IShellItemImageFactory =
            unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) }.ok()?;
        let bitmap = Bitmap(
            unsafe {
                factory.GetImage(
                    SIZE {
                        cx: max_edge as i32,
                        cy: max_edge as i32,
                    },
                    SIIGBF_THUMBNAILONLY,
                )
            }
            .ok()?,
        );
        let mut object = BITMAP::default();
        if unsafe {
            GetObjectW(
                HGDIOBJ(bitmap.0 .0),
                size_of::<BITMAP>() as i32,
                Some((&mut object as *mut BITMAP).cast()),
            )
        } != size_of::<BITMAP>() as i32
        {
            return None;
        }
        let width = u32::try_from(object.bmWidth).ok()?;
        let height = object.bmHeight.checked_abs()? as u32;
        if width == 0 || height == 0 || width > max_edge || height > max_edge {
            return None;
        }
        let dc = Dc(unsafe { CreateCompatibleDC(None) });
        if dc.0 .0.is_null() {
            return None;
        }
        let mut rgba = vec![0; width as usize * height as usize * 4];
        let mut info = BITMAPINFO::default();
        info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width as i32;
        info.bmiHeader.biHeight = -(height as i32);
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB.0;
        if unsafe {
            GetDIBits(
                dc.0,
                bitmap.0,
                0,
                height,
                Some(rgba.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            )
        } != height as i32
        {
            return None;
        }
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        Some(FileThumbnail {
            width,
            height,
            rgba,
        })
    }
}
