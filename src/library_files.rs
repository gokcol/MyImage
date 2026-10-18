//! File operations never delete permanently or modify neighboring exports/projects.
use std::{
    fs, io,
    path::{Path, PathBuf},
};
#[derive(Clone)]
pub enum Operation {
    Trash(Vec<PathBuf>),
    Duplicate(Vec<PathBuf>),
    Rename(PathBuf, String),
}
impl Operation {
    pub fn paths(&self) -> Vec<PathBuf> {
        match self {
            Self::Trash(p) | Self::Duplicate(p) => p.clone(),
            Self::Rename(p, _) => vec![p.clone()],
        }
    }
}
#[derive(Default)]
pub struct Outcome {
    pub removed: Vec<PathBuf>,
    pub renamed: Vec<(PathBuf, PathBuf)>,
    pub created: Vec<PathBuf>,
    pub errors: Vec<String>,
}
fn regular_file(path: &Path) -> Result<(), String> {
    let m = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !path.is_absolute() || !m.file_type().is_file() {
        return Err("Only regular image/project files can be changed.".into());
    }
    Ok(())
}
pub fn rename_target(path: &Path, stem: &str) -> Result<PathBuf, String> {
    let stem = stem.trim();
    if stem.is_empty()
        || stem == "."
        || stem == ".."
        || stem.ends_with('.')
        || stem
            .chars()
            .any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
    {
        return Err(
            "Enter a name without folder separators or special filename characters.".into(),
        );
    }
    let reserved = stem
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if ["CON", "PRN", "AUX", "NUL"].contains(&reserved.as_str())
        || (reserved.len() == 4
            && (reserved.starts_with("COM") || reserved.starts_with("LPT"))
            && matches!(reserved.as_bytes()[3], b'1'..=b'9'))
    {
        return Err("That name is reserved by Windows. Choose another name.".into());
    }
    let mut name = std::ffi::OsString::from(stem);
    if let Some(ext) = path.extension() {
        name.push(".");
        name.push(ext);
    }
    Ok(path.with_file_name(name))
}
fn copy_new(source: &Path, target: &Path) -> Result<(), io::Error> {
    let mut input = fs::File::open(source)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    if let Err(e) = io::copy(&mut input, &mut output).and_then(|_| output.sync_all()) {
        drop(output);
        let _ = fs::remove_file(target);
        return Err(e);
    }
    Ok(())
}
fn duplicate(path: &Path) -> Result<PathBuf, String> {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    for n in 1..=10000 {
        let name = if n == 1 {
            format!("{stem} copy")
        } else {
            format!("{stem} copy {n}")
        };
        let target = rename_target(path, &name)?;
        match copy_new(path, &target) {
            Ok(()) => return Ok(target),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("Too many copies with the same name.".into())
}
fn rename(path: &Path, name: &str) -> Result<PathBuf, String> {
    let target = rename_target(path, name)?;
    if target == path {
        return Ok(target);
    }
    // A hard link reserves the destination atomically, so rename never overwrites a file.
    fs::hard_link(path, &target)
        .map_err(|e| format!("Cannot rename (the name may already exist): {e}"))?;
    if let Err(e) = fs::remove_file(path) {
        let _ = fs::remove_file(&target);
        return Err(e.to_string());
    }
    Ok(target)
}
pub fn run(operation: Operation) -> Outcome {
    let mut outcome = Outcome::default();
    for path in operation
        .paths()
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
    {
        let result = regular_file(&path).and_then(|_| match &operation {
            Operation::Trash(_) => trash(&path).map(|_| outcome.removed.push(path.clone())),
            Operation::Duplicate(_) => duplicate(&path).map(|p| outcome.created.push(p)),
            Operation::Rename(_, name) => {
                rename(&path, name).map(|p| outcome.renamed.push((path.clone(), p)))
            }
        });
        if let Err(e) = result {
            outcome.errors.push(format!("{}: {e}", path.display()));
        }
    }
    outcome
}
#[cfg(target_os = "macos")]
fn trash(path: &Path) -> Result<(), String> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};
    objc2::rc::autoreleasepool(|_| {
        let path = path.to_str().ok_or("The filename is not valid Unicode.")?;
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        NSFileManager::defaultManager()
            .trashItemAtURL_resultingItemURL_error(&url, None)
            .map_err(|e| e.to_string())
    })
}
#[cfg(target_os = "windows")]
fn trash(path: &Path) -> Result<(), String> {
    use windows::{
        Win32::{System::Com::*, UI::Shell::*},
        core::HSTRING,
    };
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(|e| e.to_string())?;
        let result = (|| -> windows::core::Result<()> {
            let op: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)?;
            op.SetOperationFlags(
                FOF_ALLOWUNDO | FOFX_RECYCLEONDELETE | FOFX_EARLYFAILURE | FOF_WANTNUKEWARNING,
            )?;
            let item: IShellItem =
                SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None)?;
            op.DeleteItem(&item, None)?;
            op.PerformOperations()?;
            if op.GetAnyOperationsAborted()?.as_bool() || path.exists() {
                return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                    0x800704c7u32 as i32,
                )));
            }
            Ok(())
        })();
        CoUninitialize();
        result.map_err(|e| e.to_string())
    }
}
pub fn open_folder(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| format!("Cannot create {}: {e}", path.display()))?;
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("/usr/bin/open")
        .arg(path)
        .spawn();
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer.exe").arg(path).spawn();
    result.map(|_| ()).map_err(|e| e.to_string())
}

pub fn reveal(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(path)
        .spawn();
    #[cfg(target_os = "windows")]
    let result = {
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(path);
        std::process::Command::new("explorer.exe").arg(arg).spawn()
    };
    result.map(|_| ()).map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_and_rename_preserve_bytes_extension_and_existing_files() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.myimage");
        fs::write(&p, b"editable project").unwrap();
        let a = run(Operation::Duplicate(vec![p.clone(), p.clone()]));
        assert!(a.errors.is_empty());
        assert_eq!(a.created.len(), 1);
        let b = run(Operation::Duplicate(vec![p.clone()]));
        assert_ne!(a.created, b.created);
        assert_eq!(fs::read(&a.created[0]).unwrap(), fs::read(&p).unwrap());
        let conflict = run(Operation::Rename(p.clone(), "a copy".into()));
        assert_eq!(conflict.errors.len(), 1);
        assert!(p.exists());
        let moved = run(Operation::Rename(p.clone(), "renamed".into()));
        assert!(moved.errors.is_empty());
        assert!(!p.exists());
        assert_eq!(moved.renamed[0].1.extension().unwrap(), "myimage");
    }
    #[test]
    fn invalid_names_and_non_files_cannot_mutate_other_paths() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("safe.png");
        fs::write(&p, b"image").unwrap();
        for name in [
            "",
            "..",
            "../escape",
            "C:\\escape",
            "nul",
            "CON.txt",
            "LPT1",
            "trailing.",
        ] {
            assert!(rename_target(&p, name).is_err(), "{name}");
        }
        assert!(
            !run(Operation::Duplicate(vec![d.path().to_owned()]))
                .errors
                .is_empty()
        );
        assert!(p.exists());
        #[cfg(unix)]
        {
            let link = d.path().join("link.png");
            std::os::unix::fs::symlink(&p, &link).unwrap();
            assert!(!run(Operation::Rename(link, "new".into())).errors.is_empty());
            assert!(p.exists());
        }
    }
}
