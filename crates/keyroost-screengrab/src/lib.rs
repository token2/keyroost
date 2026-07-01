//! Windows-only still screen capture for keyroost's QR-from-screen feature.
//!
//! A single GDI `BitBlt` of the virtual screen (all monitors) into a top-down
//! 32-bit DIB, returned as RGBA. This crate exists purely to isolate the
//! `unsafe` Win32 FFI from the GUI crate, which forbids unsafe code; Linux and
//! macOS capture live on the safe path in `keyroost`. On non-Windows targets
//! every entry point is inert.

/// A captured frame: `width * height * 4` bytes of RGBA8, top-down.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Capture the whole virtual screen (all monitors). Returns an error string
/// describing why capture failed; on non-Windows targets it is always an error.
#[cfg(windows)]
pub fn capture_virtual_screen() -> Result<Frame, String> {
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        SRCCOPY,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };

    unsafe {
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        if w <= 0 || h <= 0 {
            return Err("no virtual screen to capture".into());
        }

        let screen = GetDC(std::ptr::null_mut());
        if screen.is_null() {
            return Err("GetDC(screen) failed".into());
        }
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, w, h);
        if mem.is_null() || bmp.is_null() {
            if !bmp.is_null() {
                DeleteObject(bmp as _);
            }
            if !mem.is_null() {
                DeleteDC(mem);
            }
            ReleaseDC(std::ptr::null_mut(), screen);
            return Err("could not allocate a capture buffer".into());
        }
        let prev = SelectObject(mem, bmp as _);

        let blit_ok = BitBlt(mem, 0, 0, w, h, screen, x, y, SRCCOPY) != 0;

        // Top-down (negative height) 32-bpp BGRA readback.
        let mut info: BITMAPINFO = core::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: core::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..core::mem::zeroed()
        };
        let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
        let lines = GetDIBits(
            mem,
            bmp,
            0,
            h as u32,
            buf.as_mut_ptr().cast(),
            &mut info,
            DIB_RGB_COLORS,
        );

        // Release GDI objects regardless of outcome.
        SelectObject(mem, prev);
        DeleteObject(bmp as _);
        DeleteDC(mem);
        ReleaseDC(std::ptr::null_mut(), screen);

        if !blit_ok {
            return Err("BitBlt failed".into());
        }
        if lines == 0 {
            return Err("GetDIBits returned no scanlines".into());
        }

        // BGRA -> RGBA (and force opaque alpha).
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2);
            px[3] = 255;
        }
        Ok(Frame {
            width: w as u32,
            height: h as u32,
            rgba: buf,
        })
    }
}

/// Inert on non-Windows targets — keyroost captures those platforms itself.
#[cfg(not(windows))]
pub fn capture_virtual_screen() -> Result<Frame, String> {
    Err("the GDI screen-capture backend is Windows-only".into())
}
