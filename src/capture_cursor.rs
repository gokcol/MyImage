//! Composite the actual Windows cursor with its hotspot and native AND/XOR mask.
use image::RgbaImage;
use windows::Win32::{
    Graphics::Gdi::*,
    UI::WindowsAndMessaging::{
        CURSOR_SHOWING, CURSORINFO, DI_NORMAL, DrawIconEx, GetCursorInfo, GetIconInfo, HICON,
        ICONINFO,
    },
};

pub(super) fn draw(image: &mut RgbaImage, origin_x: i32, origin_y: i32) -> Result<(), String> {
    // All handles are used synchronously on this worker. The DIB has exactly
    // image.len() bytes and is released after it is deselected from its DC.
    unsafe {
        let mut cursor = CURSORINFO {
            cbSize: std::mem::size_of::<CURSORINFO>() as u32,
            ..Default::default()
        };
        GetCursorInfo(&mut cursor).map_err(|e| format!("Cannot read the pointer: {e}"))?;
        if cursor.flags != CURSOR_SHOWING {
            return Ok(());
        }
        let mut icon = ICONINFO::default();
        GetIconInfo(HICON(cursor.hCursor.0), &mut icon).map_err(|e| e.to_string())?;
        let dc = CreateCompatibleDC(None);
        let result = (|| {
            if dc.0.is_null() {
                return Err("Cannot create a pointer drawing surface.".into());
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: image.width() as i32,
                    biHeight: -(image.height() as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bitmap = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
                .map_err(|e| e.to_string())?;
            let result = (|| {
                if bits.is_null() {
                    return Err("The pointer drawing surface has no pixels.".into());
                }
                let bytes = std::slice::from_raw_parts_mut(bits.cast::<u8>(), image.as_raw().len());
                for (dst, src) in bytes.chunks_exact_mut(4).zip(image.pixels()) {
                    dst.copy_from_slice(&[src[2], src[1], src[0], src[3]]);
                }
                let old = SelectObject(dc, bitmap.into());
                let drawn = DrawIconEx(
                    dc,
                    cursor.ptScreenPos.x - origin_x - icon.xHotspot as i32,
                    cursor.ptScreenPos.y - origin_y - icon.yHotspot as i32,
                    HICON(cursor.hCursor.0),
                    0,
                    0,
                    0,
                    None,
                    DI_NORMAL,
                );
                let _ = GdiFlush();
                SelectObject(dc, old);
                drawn.map_err(|e| e.to_string())?;
                for (src, dst) in bytes.chunks_exact(4).zip(image.pixels_mut()) {
                    dst[0] = src[2];
                    dst[1] = src[1];
                    dst[2] = src[0];
                }
                Ok(())
            })();
            let _ = DeleteObject(bitmap.into());
            result
        })();
        if !dc.0.is_null() {
            let _ = DeleteDC(dc);
        }
        if !icon.hbmMask.0.is_null() {
            let _ = DeleteObject(icon.hbmMask.into());
        }
        if !icon.hbmColor.0.is_null() {
            let _ = DeleteObject(icon.hbmColor.into());
        }
        result
    }
}
