//! AppKit's asynchronous print sheet avoids re-entering winit's event loop.
use super::{Outcome, Prepared, Result};
use eframe::egui;
use objc2::{
    AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained, sel,
};
use objc2_app_kit::{
    NSImage, NSImageView, NSPaperOrientation, NSPrintInfo, NSPrintOperation,
    NSPrintingPaginationMode, NSView,
};
use objc2_foundation::{
    NSCopying, NSData, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};
use std::{
    ffi::c_void,
    ptr::NonNull,
    sync::{Mutex, mpsc},
};

pub struct Completion {
    sender: Mutex<Option<mpsc::Sender<Result>>>,
    ctx: egui::Context,
}
define_class!(
    // SAFETY: NSObject has no subclassing requirements; callback state is synchronized.
    #[unsafe(super = NSObject)]
    #[name = "MyImagePrintDelegate"]
    #[ivars = Completion]
    struct PrintDelegate;
    unsafe impl NSObjectProtocol for PrintDelegate {}
    impl PrintDelegate {
        // SAFETY: Matches NSPrintOperation's documented completion selector.
        #[unsafe(method(printOperationDidRun:success:contextInfo:))]
        fn did_run(&self, _operation: &NSPrintOperation, success: bool, _context: *mut c_void) {
            if let Ok(mut sender) = self.ivars().sender.lock()
                && let Some(sender) = sender.take() {
                let _ = sender.send(Ok(if success { Outcome::Submitted } else { Outcome::NotCompleted }));
                self.ivars().ctx.request_repaint();
            }
        }
    }
);

pub struct NativeJob {
    // Retain everything through completion, then release on the app's main thread.
    _operation: Retained<NSPrintOperation>,
    _delegate: Retained<PrintDelegate>,
    _view: Retained<NSImageView>,
    _image: Retained<NSImage>,
}

pub fn start(
    ctx: egui::Context,
    parent: NonNull<c_void>,
    prepared: Prepared,
    sender: mpsc::Sender<Result>,
) -> std::result::Result<NativeJob, String> {
    let main = MainThreadMarker::new().ok_or("Printing must start from the main window.")?;
    // SAFETY: The AppKit raw-window handle is the live NSView owned by eframe.
    let parent = unsafe { parent.cast::<NSView>().as_ref() };
    let window = parent
        .window()
        .ok_or("The print window is no longer available.")?;
    let data = NSData::with_bytes(&prepared.bytes);
    let image = NSImage::initWithData(NSImage::alloc(), &data)
        .ok_or("macOS could not prepare the print image.")?;
    // Use 96 image pixels per inch. Fit pagination scales to the selected paper.
    let size = NSSize::new(
        f64::from(prepared.size[0]) * 0.75,
        f64::from(prepared.size[1]) * 0.75,
    );
    image.setSize(size);
    let view =
        NSImageView::initWithFrame(NSImageView::alloc(main), NSRect::new(NSPoint::ZERO, size));
    view.setImage(Some(&image));
    let info = NSPrintInfo::sharedPrintInfo().copy();
    info.setHorizontalPagination(NSPrintingPaginationMode::Fit);
    info.setVerticalPagination(NSPrintingPaginationMode::Fit);
    info.setHorizontallyCentered(true);
    info.setVerticallyCentered(true);
    if prepared.size[0] > prepared.size[1] {
        info.setOrientation(NSPaperOrientation::Landscape);
    }
    let operation = NSPrintOperation::printOperationWithView_printInfo(&view, &info);
    operation.setJobTitle(Some(&NSString::from_str(&prepared.title)));
    operation.setShowsPrintPanel(true);
    operation.setShowsProgressPanel(true);
    operation.setCanSpawnSeparateThread(true);
    // SAFETY: NSObject init initializes the allocated delegate with owned callback state.
    let delegate = PrintDelegate::alloc().set_ivars(Completion {
        sender: Mutex::new(Some(sender)),
        ctx,
    });
    let delegate: Retained<PrintDelegate> = unsafe { msg_send![super(delegate), init] };
    // SAFETY: The declared selector has AppKit's exact signature. All objects are
    // held by NativeJob until completion is received; no nested runModal/block_on.
    unsafe {
        operation.runOperationModalForWindow_delegate_didRunSelector_contextInfo(
            &window,
            Some(&delegate),
            Some(sel!(printOperationDidRun:success:contextInfo:)),
            std::ptr::null_mut(),
        );
    }
    Ok(NativeJob {
        _operation: operation,
        _delegate: delegate,
        _view: view,
        _image: image,
    })
}
