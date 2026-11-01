//! Native printer selection and one-page GDI spooling on a dedicated STA thread.
use super::{Outcome, Prepared, Result, fit};
use eframe::egui;
use std::sync::mpsc;
use windows::{
    Win32::{
        Foundation::{GlobalFree, HWND},
        Graphics::Gdi::*,
        Storage::Xps::{AbortDoc, DOCINFOW, EndDoc, EndPage, StartDocW, StartPage},
        System::Com::{
            COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
        },
        UI::Controls::Dialogs::*,
    },
    core::PCWSTR,
};

pub fn start(
    ctx: egui::Context,
    parent: isize,
    prepared: Prepared,
    sender: mpsc::Sender<Result>,
) -> std::result::Result<(), String> {
    std::thread::Builder::new()
        .name("myimage-print".into())
        .spawn(move || {
            let result = print(parent, prepared);
            let _ = sender.send(result);
            ctx.request_repaint();
        })
        .map(|_| ())
        .map_err(|error| format!("Could not open printing: {error}"))
}
struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
struct Dialog(PRINTDLGEXW);
impl Drop for Dialog {
    fn drop(&mut self) {
        // SAFETY: These handles are returned/owned by this print dialog only.
        unsafe {
            if !self.0.hDC.is_invalid() {
                let _ = DeleteDC(self.0.hDC);
            }
            if !self.0.hDevMode.is_invalid() {
                let _ = GlobalFree(Some(self.0.hDevMode));
            }
            if !self.0.hDevNames.is_invalid() {
                let _ = GlobalFree(Some(self.0.hDevNames));
            }
        }
    }
}
struct Spool {
    dc: HDC,
    committed: bool,
}
impl Drop for Spool {
    fn drop(&mut self) {
        if !self.committed {
            unsafe {
                AbortDoc(self.dc);
            }
        }
    }
}
fn print(parent: isize, prepared: Prepared) -> Result {
    // SAFETY: This is a new dedicated thread with a matching COM teardown guard.
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).ok() }
        .map_err(|error| format!("Could not initialize printing: {error}"))?;
    let _apartment = Apartment;
    let mut dialog = Dialog(PRINTDLGEXW {
        lStructSize: std::mem::size_of::<PRINTDLGEXW>() as u32,
        hwndOwner: HWND(parent as *mut _),
        Flags: PD_RETURNDC
            | PD_USEDEVMODECOPIESANDCOLLATE
            | PD_NOPAGENUMS
            | PD_NOSELECTION
            | PD_NOCURRENTPAGE
            | PD_DISABLEPRINTTOFILE,
        nCopies: 1,
        nStartPage: START_PAGE_GENERAL,
        ..Default::default()
    });
    // SAFETY: Owner is the live eframe window; the app keeps it alive while busy.
    unsafe { PrintDlgExW(&mut dialog.0) }
        .map_err(|error| format!("Could not open the printer dialog: {error}"))?;
    if dialog.0.dwResultAction != PD_RESULT_PRINT {
        return Ok(Outcome::NotCompleted);
    }
    let dc = dialog.0.hDC;
    if dc.is_invalid() {
        return Err("The selected printer did not provide a print context.".into());
    }
    // SAFETY: All GDI operations below use the printer DC returned by the dialog.
    let page = unsafe {
        [
            GetDeviceCaps(Some(dc), HORZRES),
            GetDeviceCaps(Some(dc), VERTRES),
        ]
    };
    let dpi = unsafe {
        [
            GetDeviceCaps(Some(dc), LOGPIXELSX),
            GetDeviceCaps(Some(dc), LOGPIXELSY),
        ]
    };
    let [x, y, width, height] = fit(prepared.size, page, dpi)?;
    let title: Vec<u16> = prepared
        .title
        .replace('\0', " ")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let doc = DOCINFOW {
        cbSize: std::mem::size_of::<DOCINFOW>() as i32,
        lpszDocName: PCWSTR(title.as_ptr()),
        ..Default::default()
    };
    if unsafe { StartDocW(dc, &doc) } <= 0 {
        return Err("The printer could not start the job.".into());
    }
    let mut spool = Spool {
        dc,
        committed: false,
    };
    if unsafe { StartPage(dc) } <= 0 {
        return Err("The printer could not start the page.".into());
    }
    let bitmap = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: prepared.size[0] as i32,
            biHeight: -(prepared.size[1] as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    unsafe {
        SetStretchBltMode(dc, HALFTONE);
        let _ = SetBrushOrgEx(dc, 0, 0, None);
    }
    let rows = unsafe {
        StretchDIBits(
            dc,
            x,
            y,
            width,
            height,
            0,
            0,
            prepared.size[0] as i32,
            prepared.size[1] as i32,
            Some(prepared.bytes.as_ptr().cast()),
            &bitmap,
            DIB_RGB_COLORS,
            SRCCOPY,
        )
    };
    if rows == 0 || rows == -1 {
        return Err("The printer could not render the image.".into());
    }
    if unsafe { EndPage(dc) } <= 0 || unsafe { EndDoc(dc) } <= 0 {
        return Err("The printer did not accept the complete job.".into());
    }
    spool.committed = true;
    Ok(Outcome::Submitted)
}
