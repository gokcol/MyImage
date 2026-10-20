//! Route native macOS quit and Dock/Finder reopen requests through the app.
//!
//! Winit 0.30 supplies applicationWillTerminate:, but not the cancellable
//! applicationShouldTerminate:. Adding only that missing optional method keeps
//! winit's delegate object, class, lifecycle callbacks, and ivars unchanged.

use super::DesktopEvent;
use eframe::egui;
use objc2::{
    MainThreadMarker, class, ffi, msg_send,
    runtime::{AnyClass, AnyObject, Imp, Sel},
    sel,
};
use std::sync::{Arc, Mutex, Weak, mpsc::Sender};

// NSApplication.h declares NSApplicationTerminateReply as NSUInteger.
const TERMINATE_CANCEL: usize = 0;
const TERMINATE_NOW: usize = 1;

struct QuitSink {
    ctx: egui::Context,
    sender: Sender<DesktopEvent>,
}

static QUIT_TARGET: Mutex<Option<Weak<QuitSink>>> = Mutex::new(None);

/// Kept by the app independently of tray initialization, which can fail.
pub struct TerminationGuard {
    sink: Arc<QuitSink>,
}

impl TerminationGuard {
    /// Install once on the UI thread, after winit creates NSApplication's delegate.
    pub fn install(ctx: egui::Context, sender: Sender<DesktopEvent>) -> Result<Self, String> {
        let _main_thread = MainThreadMarker::new()
            .ok_or("Native quit protection must be installed on the main thread.")?;
        // SAFETY: AppKit's sharedApplication and delegate accessors run on the
        // checked main thread. Both objects remain owned by AppKit/winit here.
        let application: *mut AnyObject =
            unsafe { msg_send![class!(NSApplication), sharedApplication] };
        if application.is_null() {
            return Err("macOS did not provide the application for native quit protection.".into());
        }
        // SAFETY: `application` was just checked and is the live NSApplication.
        let delegate: *mut AnyObject = unsafe { msg_send![application, delegate] };
        // SAFETY: The delegate is borrowed only during installation. No pointer
        // to it is retained by our hook, which never accesses delegate state.
        let delegate = unsafe { delegate.as_ref() }
            .ok_or("macOS did not provide an application delegate for native quit protection.")?;
        let delegate_class = delegate.class();
        if delegate_class.name() != c"WinitApplicationDelegate" {
            return Err(
                "The native application delegate is not the supported winit delegate.".into(),
            );
        }
        add_missing_hook(delegate_class)?;
        add_reopen_hook(delegate_class)?;
        let guard = Self::bind(ctx, sender)?;
        // Reassign the same delegate so AppKit can refresh any cached optional
        // selector checks. Winit retains the identical object and all its state.
        // SAFETY: Both objects are live and this is the checked main thread;
        // NSApplication's setter borrows the existing delegate without taking
        // responsibility for winit's ownership.
        unsafe {
            let _: () = msg_send![application, setDelegate: delegate];
        }
        Ok(guard)
    }

    fn bind(ctx: egui::Context, sender: Sender<DesktopEvent>) -> Result<Self, String> {
        let mut target = QUIT_TARGET
            .lock()
            .map_err(|_| "Native quit event routing is unavailable.".to_owned())?;
        if target.as_ref().and_then(Weak::upgrade).is_some() {
            return Err("Native quit protection is already active.".into());
        }
        let sink = Arc::new(QuitSink { ctx, sender });
        *target = Some(Arc::downgrade(&sink));
        Ok(Self { sink })
    }
}

impl Drop for TerminationGuard {
    fn drop(&mut self) {
        if let Ok(mut target) = QUIT_TARGET.lock()
            && target
                .as_ref()
                .is_some_and(|weak| weak.ptr_eq(&Arc::downgrade(&self.sink)))
        {
            *target = None;
        }
        // Objective-C methods cannot be removed safely. The permanent method
        // contains no owned state and permits normal termination without a sink.
    }
}

fn hook_implementation() -> Imp {
    // SAFETY: Objective-C IMP erases a method's signature. The registered
    // encoding below exactly matches this function's receiver, selector,
    // NSApplication argument, and NSUInteger return type on 64-bit macOS.
    unsafe {
        std::mem::transmute::<extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize, Imp>(
            application_should_terminate,
        )
    }
}

fn add_missing_hook(delegate_class: &AnyClass) -> Result<(), String> {
    let selector = sel!(applicationShouldTerminate:);
    if let Some(existing) = delegate_class.instance_method(selector) {
        if std::ptr::fn_addr_eq(existing.implementation(), hook_implementation()) {
            return Ok(());
        }
        return Err("The application already has a different native quit handler.".into());
    }
    // SAFETY: Called on the main thread during app initialization (or against an
    // isolated test class). We add one previously absent optional method. `Q@:@`
    // encodes NSUInteger, receiver, selector, and an object argument on all
    // supported 64-bit Macs. The function lives for the process lifetime.
    let added = unsafe {
        ffi::class_addMethod(
            (delegate_class as *const AnyClass).cast_mut(),
            selector,
            hook_implementation(),
            c"Q@:@".as_ptr(),
        )
    };
    if !added.as_bool() {
        return Err("Could not install native quit protection.".into());
    }
    Ok(())
}

// Dock/Finder relaunches an already-running app through this optional method.
// Merely ordering its hidden NSWindow front does not clear our hidden UI state.
fn reopen_implementation() -> Imp {
    // SAFETY: Matches AppKit's BOOL, id, SEL, NSApplication*, BOOL signature.
    unsafe {
        std::mem::transmute::<
            extern "C-unwind" fn(
                *mut AnyObject,
                Sel,
                *mut AnyObject,
                objc2::runtime::Bool,
            ) -> objc2::runtime::Bool,
            Imp,
        >(application_should_handle_reopen)
    }
}

fn add_reopen_hook(delegate_class: &AnyClass) -> Result<(), String> {
    let selector = sel!(applicationShouldHandleReopen:hasVisibleWindows:);
    if let Some(existing) = delegate_class.instance_method(selector) {
        return if std::ptr::fn_addr_eq(existing.implementation(), reopen_implementation()) {
            Ok(())
        } else {
            Err("The application already has a different native reopen handler.".into())
        };
    }
    let encoding = std::ffi::CString::new(format!(
        "{}@:@{}",
        <objc2::runtime::Bool as objc2::Encode>::ENCODING,
        <objc2::runtime::Bool as objc2::Encode>::ENCODING,
    ))
    .expect("Objective-C encoding has no NUL");
    // SAFETY: Add an absent method only, on the main thread, with the
    // architecture-specific BOOL encoding. No delegate ownership is changed.
    let added = unsafe {
        ffi::class_addMethod(
            (delegate_class as *const AnyClass).cast_mut(),
            selector,
            reopen_implementation(),
            encoding.as_ptr(),
        )
    };
    if added.as_bool() {
        Ok(())
    } else {
        Err("Could not install native reopen handling.".into())
    }
}

extern "C-unwind" fn application_should_handle_reopen(
    _delegate: *mut AnyObject,
    _selector: Sel,
    _application: *mut AnyObject,
    _has_visible_windows: objc2::runtime::Bool,
) -> objc2::runtime::Bool {
    let sink = QUIT_TARGET
        .lock()
        .ok()
        .and_then(|target| target.as_ref().and_then(Weak::upgrade));
    if let Some(sink) = sink
        && sink.sender.send(DesktopEvent::Show).is_ok()
    {
        sink.ctx.request_repaint_of(egui::ViewportId::ROOT);
        objc2::runtime::Bool::NO
    } else {
        objc2::runtime::Bool::YES
    }
}

extern "C-unwind" fn application_should_terminate(
    _delegate: *mut AnyObject,
    _selector: Sel,
    _application: *mut AnyObject,
) -> usize {
    let sink = QUIT_TARGET
        .lock()
        .ok()
        .and_then(|target| target.as_ref().and_then(Weak::upgrade));
    if let Some(sink) = sink
        && sink.sender.send(DesktopEvent::Quit).is_ok()
    {
        // Only enqueue work. AppKit is still inside terminate: and must return
        // before the app opens its save/discard prompt or a native file dialog.
        sink.ctx.request_repaint_of(egui::ViewportId::ROOT);
        TERMINATE_CANCEL
    } else {
        TERMINATE_NOW
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2::{rc::Retained, runtime::ClassBuilder};
    use std::{ptr, sync::mpsc};

    #[test]
    fn native_quit_is_routed_without_terminating_and_hook_is_safe_after_drop() {
        let class = ClassBuilder::new(c"MyImageTerminationGuardTest", class!(NSObject))
            .unwrap()
            .register();
        add_missing_hook(class).unwrap();
        add_reopen_hook(class).unwrap();
        add_reopen_hook(class).unwrap();
        // Reinitialization must recognize its own hook without replacing it.
        add_missing_hook(class).unwrap();
        // SAFETY: This test subclass has no ivars and inherits NSObject's new.
        let delegate: Retained<AnyObject> = unsafe { msg_send![class, new] };
        let (sender, receiver) = mpsc::channel();
        let guard = TerminationGuard::bind(egui::Context::default(), sender).unwrap();
        // SAFETY: The test receiver has exactly our registered method. The null
        // sender is intentionally unused by the callback and never dereferenced.
        let reply: usize = unsafe {
            msg_send![&delegate, applicationShouldTerminate: ptr::null_mut::<AnyObject>()]
        };
        assert_eq!(reply, TERMINATE_CANCEL);
        assert_eq!(receiver.try_recv().unwrap(), DesktopEvent::Quit);
        assert!(receiver.try_recv().is_err());
        for visible in [objc2::runtime::Bool::YES, objc2::runtime::Bool::NO] {
            // SAFETY: Test class implements the exact callback signature.
            let reopen: objc2::runtime::Bool = unsafe {
                msg_send![&delegate, applicationShouldHandleReopen: ptr::null_mut::<AnyObject>(), hasVisibleWindows: visible]
            };
            assert!(!reopen.as_bool());
            assert_eq!(receiver.try_recv().unwrap(), DesktopEvent::Show);
        }

        drop(guard);
        // SAFETY: Same registered receiver and signature as above; the hook has
        // no borrowed references to the guard after its drop.
        let reply: usize = unsafe {
            msg_send![&delegate, applicationShouldTerminate: ptr::null_mut::<AnyObject>()]
        };
        assert_eq!(reply, TERMINATE_NOW);
    }

    #[test]
    fn an_existing_native_quit_handler_is_never_replaced() {
        extern "C-unwind" fn existing_handler(
            _receiver: *mut AnyObject,
            _selector: Sel,
            _sender: *mut AnyObject,
        ) -> usize {
            42
        }
        let mut class =
            ClassBuilder::new(c"MyImageExistingTerminationTest", class!(NSObject)).unwrap();
        // SAFETY: The method's Rust signature matches the optional AppKit hook.
        unsafe {
            class.add_method(
                sel!(applicationShouldTerminate:),
                existing_handler
                    as extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize,
            );
        }
        let class = class.register();
        let original = class
            .instance_method(sel!(applicationShouldTerminate:))
            .unwrap()
            .implementation();
        assert!(add_missing_hook(class).is_err());
        assert!(std::ptr::fn_addr_eq(
            original,
            class
                .instance_method(sel!(applicationShouldTerminate:))
                .unwrap()
                .implementation()
        ));
    }
}
