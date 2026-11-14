//! Only a Store-signed installation may ask the Store about its own updates.
use super::{CHECK_TIMEOUT, Outcome, STORE_URL};
use std::time::{Duration, Instant};
use windows::{
    ApplicationModel::{Package, PackageSignatureKind},
    Services::Store::StoreContext,
    Win32::{
        Foundation::{APPMODEL_ERROR_NO_PACKAGE, ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, HWND},
        Storage::Packaging::Appx::GetCurrentPackageFullName,
        System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
        UI::Shell::IInitializeWithWindow,
    },
    core::Interface,
};

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { RoUninitialize() };
    }
}

pub(super) fn check(parent: Option<isize>) -> Result<Outcome, String> {
    let mut size = 0;
    // SAFETY: A size query, no output buffer.
    match unsafe { GetCurrentPackageFullName(&mut size, None) } {
        APPMODEL_ERROR_NO_PACKAGE => return Ok(Outcome::StoreInstallRequired),
        ERROR_INSUFFICIENT_BUFFER | ERROR_SUCCESS => (),
        _ => {
            return Err(
                "Windows could not identify this installation. Open the Store to check manually."
                    .into(),
            );
        }
    }
    // Dedicated worker thread, balanced even on errors; all COM objects drop first.
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
        .map_err(|_| "Could not initialize the Microsoft Store check.")?;
    let _apartment = Apartment;
    query(parent).map_err(|e| {
        format!("Couldn't check Microsoft Store: {e}. Open the Store or try again later.")
    })
}

fn query(parent: Option<isize>) -> Result<Outcome, String> {
    let native = || -> windows::core::Result<_> {
        let package = Package::Current()?;
        if package.SignatureKind()? != PackageSignatureKind::Store || package.IsDevelopmentMode()? {
            return Ok(None);
        }
        let family = package.Id()?.FamilyName()?;
        let context = StoreContext::GetDefault()?;
        if let Some(parent) = parent {
            // SAFETY: The live application window owns any Store UI; no install is requested.
            unsafe {
                context
                    .cast::<IInitializeWithWindow>()?
                    .Initialize(HWND(parent as *mut _))?;
            }
        }
        Ok(Some((
            family,
            context.GetAppAndOptionalStorePackageUpdatesAsync()?,
        )))
    };
    let Some((family, operation)) = native().map_err(|e| e.to_string())? else {
        return Ok(Outcome::StoreInstallRequired);
    };
    let started = Instant::now();
    while operation.Status().map_err(|e| e.to_string())?.0 == 0 {
        if started.elapsed() >= CHECK_TIMEOUT - Duration::from_secs(2) {
            let _ = operation.Cancel();
            return Err("the service timed out".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let updates = operation.GetResults().map_err(|e| e.to_string())?;
    for update in updates {
        let id = update
            .Package()
            .and_then(|p| p.Id())
            .map_err(|e| e.to_string())?;
        if id.FamilyName().map_err(|e| e.to_string())? == family {
            return Ok(Outcome::Available {
                // Store's Package is the installed package, not a promised target version.
                id: format!("store:{}", super::release_id()),
                title: "An update is available in Microsoft Store".into(),
                notes: "Open MyImage in Microsoft Store to see the latest details and update. Your installed version's changes are listed below.".into(),
                url: STORE_URL.into(),
            });
        }
    }
    Ok(Outcome::Current)
}
