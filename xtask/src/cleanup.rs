//! Release-only housekeeping. Every deletion is confined to an explicit build
//! path/name; links, user media, projects, unknown files and running apps survive.
use crate::{StagingDirectory, TaskResult};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy)]
#[allow(dead_code)] // Both formats are exercised in tests on either host.
pub enum ReleaseKind {
    Mac(&'static str),
    Windows(&'static str),
}

impl ReleaseKind {
    fn filename(self) -> String {
        match self {
            Self::Mac(arch) => format!("MyImage-rollback-macos-{arch}.zip"),
            Self::Windows(arch) => format!("MyImage-rollback-windows-{arch}.exe"),
        }
    }

    fn owns_backup(self, name: &str) -> bool {
        name == self.filename()
            || matches!(self, Self::Mac(_))
                && name.starts_with("MyImage-before-")
                && name.ends_with(".zip")
    }
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Include junctions and other reparse points, not just symlinks.
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    metadata.file_type().is_symlink()
}

fn check_directory(path: &Path) -> TaskResult<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && !is_link(&meta) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        _ => Err(format!(
            "Refusing linked or non-directory build path: {}",
            path.display()
        )
        .into()),
    }
}

pub fn check_workspace(workspace: &Path) -> TaskResult<()> {
    for relative in ["target", "target/bundle", "target/review", "backups"] {
        check_directory(&workspace.join(relative))?;
    }
    Ok(())
}

pub fn same_file_contents(left: &Path, right: &Path) -> io::Result<bool> {
    let mut files = Vec::new();
    for path in [left, right] {
        let meta = match fs::symlink_metadata(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            result => result?,
        };
        if !meta.is_file() || is_link(&meta) {
            return Err(io::Error::other("Expected a regular artifact file"));
        }
        files.push(File::open(path)?);
    }
    if files[0].metadata()?.len() != files[1].metadata()?.len() {
        return Ok(false);
    }
    let mut a = [0; 64 * 1024];
    let mut b = [0; 64 * 1024];
    loop {
        let count = files[0].read(&mut a)?;
        if count == 0 {
            return Ok(true);
        }
        files[1].read_exact(&mut b[..count])?;
        if a[..count] != b[..count] {
            return Ok(false);
        }
    }
}

/// The OS releases the lock on close/crash. Cargo uses the same profile lock.
/// Keeping .cargo-lock in place avoids replacing an inode another build waits on.
pub struct BuildLock {
    _file: File,
}

impl BuildLock {
    fn acquire(path: &Path) -> io::Result<Self> {
        match fs::symlink_metadata(path) {
            Ok(meta) if !meta.is_file() || is_link(&meta) => {
                return Err(io::Error::other("Refusing linked/non-file lock"));
            }
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            // SAFETY: a live owned File supplies the descriptor. Nonblocking.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::{Storage::FileSystem::*, System::IO::OVERLAPPED};
            let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
            // SAFETY: synchronous operation, live handle, valid OVERLAPPED.
            if unsafe {
                LockFileEx(
                    file.as_raw_handle(),
                    LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                    0,
                    u32::MAX,
                    u32::MAX,
                    &mut overlapped,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(Self { _file: file })
    }
}

pub fn release_lock(workspace: &Path) -> TaskResult<BuildLock> {
    check_workspace(workspace)?;
    let bundle = workspace.join("target/bundle");
    fs::create_dir_all(&bundle)?;
    BuildLock::acquire(&bundle.join(".release.lock"))
        .map_err(|error| format!("Release packaging is busy or unavailable: {error}").into())
}

fn backup_candidates(workspace: &Path, kind: ReleaseKind) -> TaskResult<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for relative in ["backups", "target/review"] {
        let folder = workspace.join(relative);
        if !folder.exists() {
            continue;
        }
        for entry in fs::read_dir(folder)? {
            let entry = entry?;
            if kind.owns_backup(&entry.file_name().to_string_lossy()) {
                let meta = fs::symlink_metadata(entry.path())?;
                if !meta.is_file() || is_link(&meta) || meta.len() == 0 {
                    return Err(format!("Invalid rollback file: {}", entry.path().display()).into());
                }
                paths.push(entry.path());
            }
        }
    }
    paths.sort_by_key(|path| {
        (
            fs::metadata(path).and_then(|m| m.modified()).ok(),
            path.clone(),
        )
    });
    Ok(paths)
}

fn copy_backup(source: &Path, destination: &Path) -> TaskResult<()> {
    let meta = fs::symlink_metadata(source)?;
    if !meta.is_file() || is_link(&meta) || meta.len() == 0 {
        return Err(
            "Previous package is not a nonempty regular file; keeping existing backups".into(),
        );
    }
    let parent = destination.parent().ok_or("Missing backup directory")?;
    fs::create_dir_all(parent)?;
    let mut stage = StagingDirectory::create(parent)?;
    let staged = stage.path.join("rollback");
    fs::copy(source, &staged)?;
    OpenOptions::new().write(true).open(&staged)?.sync_all()?;
    if !same_file_contents(source, &staged)? {
        return Err("Rollback verification failed; keeping existing backups".into());
    }
    stage.publish(&staged, destination, "previous-backup")?;
    Ok(())
}

fn retain_rollback(
    workspace: &Path,
    kind: ReleaseKind,
    previous: &Path,
    same_release: bool,
) -> TaskResult<Option<PathBuf>> {
    let candidates = backup_candidates(workspace, kind)?;
    let latest = candidates.last();
    let destination = workspace.join("backups").join(kind.filename());
    let keep = if previous.try_exists()? && (!same_release || latest.is_none()) {
        copy_backup(previous, &destination)?;
        Some(destination)
    } else if let Some(latest) = latest {
        if latest.parent() == Some(workspace.join("backups").as_path()) {
            Some(latest.clone())
        } else {
            copy_backup(latest, &destination)?;
            Some(destination)
        }
    } else {
        None // The very first release has no previous package to roll back to.
    };
    // Never prune before a retained backup has been successfully established.
    if let Some(keep) = &keep {
        for old in candidates {
            if old != *keep {
                fs::remove_file(old)?;
            }
        }
        println!("Retained rollback: {}", keep.display());
    }
    Ok(keep)
}

fn running_executables() -> TaskResult<Vec<PathBuf>> {
    let paths = vec![std::env::current_exe()?];
    #[cfg(unix)]
    {
        let mut paths = paths;
        let output = std::process::Command::new("/bin/ps")
            .args(["-ww", "-axo", "comm="])
            .output()?;
        if !output.status.success() {
            return Err("Cannot check running apps; cache/QA cleanup deferred".into());
        }
        paths.extend(
            String::from_utf8(output.stdout)?
                .lines()
                .map(str::trim)
                .filter(|line| line.starts_with('/'))
                .map(PathBuf::from),
        );
        Ok(paths)
    }
    #[cfg(not(unix))]
    {
        Ok(paths)
    }
}

fn contains_protected_files(path: &Path, qa_app: bool) -> io::Result<bool> {
    let meta = fs::symlink_metadata(path)?;
    if is_link(&meta) {
        return Ok(true);
    }
    let extension = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    if extension == "myimage"
        || !qa_app
            && matches!(
                extension.as_str(),
                "png" | "jpg" | "jpeg" | "webp" | "gif" | "tif" | "tiff" | "bmp" | "pdf" | "svg"
            )
    {
        return Ok(true);
    }
    #[cfg(windows)]
    if meta.is_file() {
        use std::os::windows::fs::OpenOptionsExt;
        // Probe before deleting any part of a tree containing an in-use file.
        if OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(path)
            .is_err()
        {
            return Ok(true);
        }
    }
    if meta.is_dir() {
        for entry in fs::read_dir(path)? {
            if contains_protected_files(&entry?.path(), qa_app)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[derive(Default)]
struct Report {
    removed: usize,
    skipped: Vec<String>,
}

impl Report {
    fn remove(&mut self, path: &Path, qa_app: bool, running: &[PathBuf]) {
        let result = (|| -> TaskResult<bool> {
            if running
                .iter()
                .any(|executable| executable.starts_with(path))
                || contains_protected_files(path, qa_app)?
            {
                return Ok(false);
            }
            if path.is_dir() {
                fs::remove_dir_all(path)?;
            } else {
                fs::remove_file(path)?;
            }
            Ok(true)
        })();
        match result {
            Ok(true) => self.removed += 1,
            Ok(false) => self
                .skipped
                .push(format!("{} (protected or in use)", path.display())),
            Err(error) => self.skipped.push(format!("{} ({error})", path.display())),
        }
    }
}

fn clean_outputs(workspace: &Path, running: &[PathBuf]) -> TaskResult<Report> {
    check_workspace(workspace)?;
    let target = workspace.join("target");
    let mut report = Report::default();
    for triple in [
        "",
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "x86_64-pc-windows-msvc",
        "aarch64-pc-windows-msvc",
    ] {
        let root = target.join(triple);
        if !root.exists() {
            continue;
        }
        if let Err(error) = check_directory(&root) {
            report.skipped.push(error.to_string());
            continue;
        }
        for profile in ["debug", "release"] {
            let folder = root.join(profile);
            if !folder.exists() {
                continue;
            }
            if let Err(error) = check_directory(&folder) {
                report.skipped.push(error.to_string());
                continue;
            }
            let _lock = match BuildLock::acquire(&folder.join(".cargo-lock")) {
                Ok(lock) => lock,
                Err(error) => {
                    report
                        .skipped
                        .push(format!("{} (build busy: {error})", folder.display()));
                    continue;
                }
            };
            for entry in fs::read_dir(&folder)? {
                let entry = entry?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if matches!(
                    name.as_ref(),
                    "deps" | "build" | "incremental" | ".fingerprint" | "examples"
                ) {
                    let path = entry.path();
                    if let Err(error) = check_directory(&path) {
                        report.skipped.push(error.to_string());
                        continue;
                    }
                    // On Windows the running xtask has a locked hard link in
                    // deps. Keep that file, but clean the other dependencies.
                    for child in fs::read_dir(path)? {
                        report.remove(&child?.path(), false, running);
                    }
                } else if matches!(
                    name.as_ref(),
                    "myimage"
                        | "myimage.exe"
                        | "myimage.d"
                        | "myimage.pdb"
                        | "myimage.dSYM"
                        | "xtask"
                        | "xtask.exe"
                        | "xtask.d"
                        | "xtask.pdb"
                        | "xtask.dSYM"
                ) {
                    report.remove(&entry.path(), false, running);
                }
            }
        }
    }
    let review = target.join("review");
    if review.exists() {
        for entry in fs::read_dir(review)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let qa_name = name.starts_with("MyImage ")
                && name.ends_with(".app")
                && (name.contains(" Review") || name.contains(" QA") || name == "MyImage Demo.app");
            if (qa_name || name == "previous-library-build")
                && path.join("Contents/Info.plist").is_file()
                && path.join("Contents/MacOS/MyImage").is_file()
            {
                report.remove(&path, true, running);
            } else if name.starts_with("MyImage ")
                && (name.ends_with(" Review.exe")
                    || name.ends_with(" QA.exe")
                    || name == "MyImage Demo.exe")
            {
                report.remove(&path, false, running);
            }
        }
    }
    Ok(report)
}

pub fn after_release(
    workspace: &Path,
    kind: ReleaseKind,
    previous: &Path,
    same_release: bool,
) -> TaskResult<()> {
    // Called only after successful packaging/publication, while the previous
    // package is still in the staging directory. Backup failure stops cleanup.
    check_workspace(workspace)?;
    retain_rollback(workspace, kind, previous, same_release)
        .map_err(|error| format!("Release created, but rollback preservation failed: {error}"))?;
    let report = match running_executables().and_then(|running| clean_outputs(workspace, &running))
    {
        Ok(report) => report,
        Err(error) => {
            eprintln!("Release created; cleanup deferred: {error}");
            return Ok(());
        }
    };
    println!(
        "Release cleanup removed {} cache/QA entries.",
        report.removed
    );
    for skipped in report.skipped {
        println!("Kept {skipped}; cleanup will retry after the next release.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::{TempDir, tempdir};

    const MAC: ReleaseKind = ReleaseKind::Mac("aarch64");
    const WINDOWS: ReleaseKind = ReleaseKind::Windows("x86_64");

    fn write(root: &Path, relative: &str, bytes: &[u8]) -> PathBuf {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        path
    }

    fn fixture() -> TempDir {
        let dir = tempdir().unwrap();
        for relative in [
            "target/debug/deps/old.rlib",
            "target/debug/incremental/session/cache.bin",
            "target/debug/.fingerprint/old/lib.json",
            "target/release/build/old/output",
            "target/release/myimage",
            "target/x86_64-pc-windows-msvc/debug/deps/old.rmeta",
            "target/review/MyImage Old Review.app/Contents/Info.plist",
            "target/review/MyImage Old Review.app/Contents/MacOS/MyImage",
            "target/review/MyImage Demo.exe",
        ] {
            write(dir.path(), relative, b"disposable build data");
        }
        dir
    }

    #[test]
    fn cleanup_removes_only_recognized_outputs_and_preserves_user_files() {
        let dir = fixture();
        let root = dir.path();
        let protected = [
            "Applications/MyImage.app/Contents/MacOS/MyImage",
            "Pictures/MyImage/capture.png",
            "projects/editable.myimage",
            "target/bundle/MyImage.app/Contents/MacOS/MyImage",
            "target/bundle/MyImage-macos-aarch64.zip",
            "target/review/evidence.png",
            "target/review/custom.myimage",
            "target/review/source-snapshot/main.rs",
            "target/review/Unrelated.app/Contents/MacOS/MyImage",
            "target/debug/notes.txt",
            "target/debug/deps/accidental.myimage",
            "target/debug/build/my-captures/capture.png",
            "target/review/MyImage Project QA.app/Contents/Info.plist",
            "target/review/MyImage Project QA.app/Contents/MacOS/MyImage",
            "target/review/MyImage Project QA.app/Contents/project.myimage",
        ];
        for path in protected {
            write(root, path, b"must survive");
        }
        let report = clean_outputs(root, &[]).unwrap();
        assert!(report.removed >= 8);
        for path in protected {
            assert_eq!(
                fs::read(root.join(path)).unwrap(),
                b"must survive",
                "{path}"
            );
        }
        assert!(!root.join("target/debug/deps/old.rlib").exists());
        assert!(!root.join("target/review/MyImage Old Review.app").exists());
        assert!(!root.join("target/review/MyImage Demo.exe").exists());
    }

    #[test]
    fn backup_promotes_previous_package_and_prunes_only_same_platform_backups() {
        let dir = fixture();
        let root = dir.path();
        let old = write(
            root,
            "backups/MyImage-before-boundary-build30.zip",
            b"older rollback",
        );
        let legacy = write(
            root,
            "target/review/MyImage-before-old-build29.zip",
            b"legacy",
        );
        let unrelated = write(root, "backups/family.zip", b"personal backup");
        let other_platform = write(
            root,
            &format!("backups/{}", WINDOWS.filename()),
            b"windows rollback",
        );
        let source = write(
            root,
            "target/bundle/stage/previous.zip",
            b"previous complete release",
        );
        let retained = retain_rollback(root, MAC, &source, false).unwrap().unwrap();
        assert_eq!(fs::read(retained).unwrap(), b"previous complete release");
        assert!(source.exists());
        assert!(!old.exists());
        assert!(!legacy.exists());
        assert_eq!(fs::read(unrelated).unwrap(), b"personal backup");
        assert_eq!(fs::read(other_platform).unwrap(), b"windows rollback");
    }

    #[test]
    fn repeating_release_preserves_meaningful_older_rollback() {
        let dir = fixture();
        let root = dir.path();
        let keep = write(
            root,
            "backups/MyImage-before-boundary-build30.zip",
            b"older release",
        );
        let source = write(
            root,
            "target/bundle/stage/previous.zip",
            b"same release as current",
        );
        assert_eq!(
            retain_rollback(root, MAC, &source, true).unwrap(),
            Some(keep.clone())
        );
        assert_eq!(fs::read(&keep).unwrap(), b"older release");
        assert_eq!(fs::read_dir(root.join("backups")).unwrap().count(), 1);
    }

    #[test]
    fn rollback_migrates_out_of_target_and_first_release_needs_no_previous() {
        let dir = fixture();
        let root = dir.path();
        let missing = root.join("target/bundle/stage/previous.zip");
        assert!(
            retain_rollback(root, MAC, &missing, false)
                .unwrap()
                .is_none()
        );
        let legacy = write(
            root,
            "target/review/MyImage-before-old-build30.zip",
            b"legacy rollback",
        );
        let retained = retain_rollback(root, MAC, &missing, false)
            .unwrap()
            .unwrap();
        assert_eq!(retained, root.join("backups").join(MAC.filename()));
        assert_eq!(fs::read(retained).unwrap(), b"legacy rollback");
        assert!(!legacy.exists());
    }

    #[test]
    fn windows_previous_executable_is_retained() {
        let dir = fixture();
        let root = dir.path();
        let source = write(
            root,
            "target/bundle/stage/previous.exe",
            b"MZ previous release",
        );
        let retained = retain_rollback(root, WINDOWS, &source, false)
            .unwrap()
            .unwrap();
        assert_eq!(retained, root.join("backups").join(WINDOWS.filename()));
        assert_eq!(fs::read(retained).unwrap(), b"MZ previous release");
    }

    #[test]
    fn backup_failure_stops_cleanup_and_preserves_the_old_rollback() {
        let dir = fixture();
        let root = dir.path();
        let keep = write(
            root,
            "backups/MyImage-before-boundary-build30.zip",
            b"last good rollback",
        );
        let source = write(root, "target/bundle/stage/previous.zip", b"");
        assert!(after_release(root, MAC, &source, false).is_err());
        assert_eq!(fs::read(keep).unwrap(), b"last good rollback");
        assert!(root.join("target/debug/deps/old.rlib").exists());
        assert!(root.join("target/review/MyImage Old Review.app").exists());
    }

    #[test]
    fn backup_directory_failure_preserves_the_previous_release_staging_files() {
        let dir = fixture();
        let root = dir.path();
        let bundle = root.join("target/bundle");
        fs::create_dir_all(&bundle).unwrap();
        let mut stage = StagingDirectory::create(&bundle).unwrap();
        let previous = write(&stage.path, "previous.zip", b"latest previous release");
        // An unexpected file prevents creating backups; never delete it.
        write(root, "backups", b"unrelated user file");
        assert!(
            stage
                .finish_release(root, MAC, "previous.zip", false)
                .is_err()
        );
        drop(stage);
        assert_eq!(fs::read(previous).unwrap(), b"latest previous release");
        assert_eq!(
            fs::read(root.join("backups")).unwrap(),
            b"unrelated user file"
        );
        assert!(root.join("target/debug/deps/old.rlib").exists());
    }

    #[test]
    fn active_build_and_running_apps_are_deferred_without_blocking_other_cleanup() {
        let dir = fixture();
        let root = dir.path();
        let guard = BuildLock::acquire(&root.join("target/debug/.cargo-lock")).unwrap();
        let qa_exe = root.join("target/review/MyImage Old Review.app/Contents/MacOS/MyImage");
        let report = clean_outputs(root, std::slice::from_ref(&qa_exe)).unwrap();
        assert!(report.skipped.iter().any(|s| s.contains("build busy")));
        assert!(root.join("target/debug/deps/old.rlib").exists());
        assert!(qa_exe.exists());
        assert!(!root.join("target/release/myimage").exists());
        drop(guard);
        clean_outputs(root, &[]).unwrap();
        assert!(!root.join("target/debug/deps/old.rlib").exists());
        assert!(!qa_exe.exists());
    }

    #[test]
    fn running_helper_does_not_keep_unrelated_dependencies() {
        let dir = fixture();
        let root = dir.path();
        let helper = write(root, "target/debug/deps/xtask-running.exe", b"helper");
        clean_outputs(root, std::slice::from_ref(&helper)).unwrap();
        assert!(helper.exists());
        assert!(!root.join("target/debug/deps/old.rlib").exists());
    }

    #[test]
    fn second_release_is_rejected_while_the_first_holds_its_lock() {
        let dir = fixture();
        let first = release_lock(dir.path()).unwrap();
        assert!(release_lock(dir.path()).is_err());
        drop(first);
        assert!(release_lock(dir.path()).is_ok());
    }

    #[test]
    fn failed_publication_restores_previous_artifact_without_cleanup() {
        let dir = fixture();
        let root = dir.path();
        let current = write(root, "target/bundle/MyImage.zip", b"good package");
        let keep = write(root, "backups/MyImage-before-test.zip", b"good rollback");
        assert!(
            crate::replace_artifact(&root.join("missing"), &current, &root.join("staged-old"))
                .is_err()
        );
        assert_eq!(fs::read(current).unwrap(), b"good package");
        assert_eq!(fs::read(keep).unwrap(), b"good rollback");
        assert!(root.join("target/debug/deps/old.rlib").exists());
    }

    #[cfg(unix)]
    #[test]
    fn linked_roots_cache_entries_and_backup_paths_never_escape_workspace() {
        use std::os::unix::fs::symlink;
        let outside = tempdir().unwrap();
        let user_file = write(outside.path(), "project.myimage", b"outside project");
        for relative in ["target", "backups", "target/review"] {
            let dir = tempdir().unwrap();
            let link = dir.path().join(relative);
            fs::create_dir_all(link.parent().unwrap()).unwrap();
            symlink(outside.path(), &link).unwrap();
            assert!(check_workspace(dir.path()).is_err());
        }
        let dir = fixture();
        let root = dir.path();
        symlink(outside.path(), root.join("target/debug/deps/linked")).unwrap();
        symlink(outside.path(), root.join("target/aarch64-apple-darwin")).unwrap();
        clean_outputs(root, &[]).unwrap();
        assert!(root.join("target/debug/deps/linked").is_symlink());
        let backup_link = root.join("backups/MyImage-before-unsafe.zip");
        fs::create_dir_all(backup_link.parent().unwrap()).unwrap();
        symlink(&user_file, backup_link).unwrap();
        let source = write(root, "target/bundle/previous.zip", b"previous release");
        assert!(retain_rollback(root, MAC, &source, false).is_err());
        assert_eq!(fs::read(user_file).unwrap(), b"outside project");
    }
}
