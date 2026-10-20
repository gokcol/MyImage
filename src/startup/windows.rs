use super::Status;
use windows::{
    ApplicationModel::{StartupTask, StartupTaskState},
    Win32::{
        Foundation::{
            APPMODEL_ERROR_NO_PACKAGE, ERROR_FILE_NOT_FOUND, ERROR_INSUFFICIENT_BUFFER,
            ERROR_SUCCESS,
        },
        Storage::Packaging::Appx::GetCurrentPackageFullName,
        System::Registry::*,
    },
    core::{HSTRING, PCWSTR, w},
};

fn packaged() -> Result<bool, String> {
    let mut size = 0;
    // SAFETY: Size query only; no buffer is supplied.
    match unsafe { GetCurrentPackageFullName(&mut size, None) } {
        APPMODEL_ERROR_NO_PACKAGE => Ok(false),
        ERROR_INSUFFICIENT_BUFFER | ERROR_SUCCESS => Ok(true),
        error => Err(format!(
            "Could not identify the installed package: {error:?}"
        )),
    }
}

fn task() -> Result<StartupTask, String> {
    StartupTask::GetAsync(&HSTRING::from("MyImageStartup"))
        .and_then(|op| op.get())
        .map_err(|e| e.to_string())
}

fn task_status(value: StartupTaskState) -> Result<Status, String> {
    match value {
        StartupTaskState::Disabled => Ok(Status::Off),
        StartupTaskState::Enabled => Ok(Status::On),
        StartupTaskState::DisabledByUser => Ok(Status::ApprovalRequired),
        StartupTaskState::DisabledByPolicy => Ok(Status::ManagedOff),
        StartupTaskState::EnabledByPolicy => Ok(Status::ManagedOn),
        _ => Err("Windows returned an unknown startup status.".into()),
    }
}

const RUN: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const APPROVAL: PCWSTR =
    w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run");
const NAME: PCWSTR = w!("MyImage");

fn registry_value(key: PCWSTR, flags: REG_ROUTINE_FLAGS) -> Result<Option<Vec<u8>>, String> {
    let mut size = 0;
    // SAFETY: Query size then read into a sufficiently sized owned buffer.
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            NAME,
            flags,
            None,
            None,
            Some(&mut size),
        )
    };
    if result == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    result.ok().map_err(|e| e.to_string())?;
    let mut data = vec![0u8; size as usize];
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            NAME,
            flags,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&mut size),
        )
    }
    .ok()
    .map_err(|e| e.to_string())?;
    data.truncate(size as usize);
    Ok(Some(data))
}

pub fn status() -> Result<Status, String> {
    if packaged()? {
        return task_status(task()?.State().map_err(|e| e.to_string())?);
    }
    if registry_value(RUN, RRF_RT_REG_SZ)?.is_none() {
        return Ok(Status::Off);
    }
    // Respect Task Manager's disable flag. Never write StartupApproved or
    // override a system/user disable when the app starts or is updated.
    if registry_value(APPROVAL, RRF_RT_REG_BINARY)?
        .is_some_and(|bytes| matches!(bytes.first(), Some(3 | 7)))
    {
        return Ok(Status::ApprovalRequired);
    }
    Ok(Status::On)
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    if packaged()? {
        let task = task()?;
        if enabled {
            let state = task
                .RequestEnableAsync()
                .and_then(|op| op.get())
                .map_err(|e| e.to_string())?;
            match task_status(state)? {
                Status::On | Status::ManagedOn => Ok(()),
                _ => Err("Enable MyImage in Windows Settings → Apps → Startup.".into()),
            }
        } else {
            task.Disable().map_err(|e| e.to_string())
        }
    } else if enabled {
        if status()? == Status::ApprovalRequired {
            return Err("Enable MyImage in Windows Settings → Apps → Startup.".into());
        }
        use std::os::windows::ffi::OsStrExt;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut command = vec![b'"' as u16];
        command.extend(exe.as_os_str().encode_wide());
        command.extend([b'"' as u16, 0]);
        // SAFETY: NUL-terminated UTF-16 REG_SZ, current user only, one named value.
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN,
                NAME,
                REG_SZ.0,
                Some(command.as_ptr().cast()),
                (command.len() * 2) as u32,
            )
        }
        .ok()
        .map_err(|e| e.to_string())
    } else {
        // SAFETY: Deletes only our named value, not the Run key or other apps.
        let result = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN, NAME) };
        if result == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            result.ok().map_err(|e| e.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn respects_user_and_admin_startup_decisions() {
        assert_eq!(
            task_status(StartupTaskState::Disabled).unwrap(),
            Status::Off
        );
        assert_eq!(
            task_status(StartupTaskState::DisabledByUser).unwrap(),
            Status::ApprovalRequired
        );
        assert!(
            !task_status(StartupTaskState::DisabledByPolicy)
                .unwrap()
                .editable()
        );
        assert!(
            !task_status(StartupTaskState::EnabledByPolicy)
                .unwrap()
                .editable()
        );
    }
}
