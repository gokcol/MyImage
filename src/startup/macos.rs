use super::Status;
use objc2::{MainThreadMarker, class, msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::NSError;

#[link(name = "ServiceManagement", kind = "framework")]
unsafe extern "C" {}

fn service() -> Result<Retained<AnyObject>, String> {
    MainThreadMarker::new().ok_or("Login settings must be accessed on the app’s main thread.")?;
    // SAFETY: The linked framework is available since macOS 13 (minimum 14).
    // mainAppService returns a non-owned object; Retained handles its lifetime.
    unsafe {
        let service: Option<Retained<AnyObject>> = msg_send![class!(SMAppService), mainAppService];
        service.ok_or_else(|| "Install MyImage in Applications to manage login startup.".into())
    }
}

pub fn status() -> Result<Status, String> {
    let service = service()?;
    // SAFETY: SMAppService.status is an NSInteger on a live service object.
    let value: isize = unsafe { msg_send![&service, status] };
    match value {
        // A new main app can report NotFound until its first registration.
        // Both states mean it is not scheduled to launch. Let register report
        // any bundle/signature error when the user actually opts in.
        0 | 3 => Ok(Status::Off),
        1 => Ok(Status::On),
        2 => Ok(Status::ApprovalRequired),
        _ => Err("Install the signed MyImage app in Applications to manage startup.".into()),
    }
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let current = status()?;
    if (enabled && current == Status::On) || (!enabled && current == Status::Off) {
        return Ok(());
    }
    let service = service()?;
    // SAFETY: These methods use the standard BOOL + autoreleased NSError**
    // convention; objc2 retains any returned error. Only explicit UI clicks call here.
    let result: Result<(), Retained<NSError>> = unsafe {
        if enabled {
            msg_send![&service, registerAndReturnError: _]
        } else {
            msg_send![&service, unregisterAndReturnError: _]
        }
    };
    result.map_err(|error| {
        format!(
            "Could not change login startup: {}",
            error.localizedDescription()
        )
    })
}
