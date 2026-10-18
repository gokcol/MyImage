//! Private, atomic editable snapshots. A serial worker owns OS locks for its
//! current draft and all offered recoveries, so live windows never steal drafts.
use crate::document::{self, Document};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, SystemTime},
};

pub const INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone, Debug)]
pub struct Entry {
    pub id: PathBuf,
    pub title: String,
    pub modified: SystemTime,
}
#[derive(Serialize, Deserialize)]
struct Label {
    title: String,
}
struct Slot {
    path: PathBuf,
    lock: Option<File>,
}
impl Slot {
    fn create(root: &Path) -> Result<Self, String> {
        let path = tempfile::Builder::new()
            .prefix("draft-")
            .tempdir_in(root)
            .map_err(|e| e.to_string())?
            .keep();
        let slot = Self::lock(&path)?.ok_or("Cannot lock the new recovery draft.")?;
        // Publish only after the OS lock is held. Another launch cannot claim a
        // freshly created directory in the gap between creation and locking.
        File::create_new(path.join("ready")).map_err(|e| e.to_string())?;
        Ok(slot)
    }
    fn open(path: &Path) -> Result<Option<Self>, String> {
        if !fs::symlink_metadata(path.join("ready")).is_ok_and(|m| m.is_file()) {
            return Ok(None);
        }
        Self::lock(path)
    }
    fn lock(path: &Path) -> Result<Option<Self>, String> {
        if !fs::symlink_metadata(path).is_ok_and(|m| m.is_dir()) {
            return Ok(None);
        }
        let lock_path = path.join("session.lock");
        if fs::symlink_metadata(&lock_path).is_ok_and(|m| !m.is_file()) {
            return Ok(None);
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0);
        }
        let lock = match options.open(lock_path) {
            Ok(file) => file,
            #[cfg(target_os = "windows")]
            Err(e) if e.raw_os_error() == Some(32) => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        #[cfg(target_os = "macos")]
        {
            use std::os::fd::AsRawFd;
            // The descriptor remains owned by Slot for the complete session.
            if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                let e = std::io::Error::last_os_error();
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    return Ok(None);
                }
                return Err(e.to_string());
            }
        }
        Ok(Some(Self {
            path: path.to_owned(),
            lock: Some(lock),
        }))
    }
    fn project(&self) -> PathBuf {
        self.path.join("recovery.myimage")
    }
    fn clear(&self) -> Result<(), String> {
        // Never recursively remove a recovery directory or follow links.
        for name in ["recovery.myimage", "label.json"] {
            match fs::remove_file(self.path.join(name)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("Cannot remove recovery copy: {e}")),
            }
        }
        Ok(())
    }
    fn write(&self, document: &Document, title: &str) -> Result<(), String> {
        document::save_project(document, &self.project())?;
        // The label is informational; the single atomic project is authoritative.
        let mut label = tempfile::NamedTempFile::new_in(&self.path).map_err(|e| e.to_string())?;
        serde_json::to_writer(
            &mut label,
            &Label {
                title: title.chars().take(512).collect(),
            },
        )
        .map_err(|e| e.to_string())?;
        label.as_file().sync_all().map_err(|e| e.to_string())?;
        label
            .persist(self.path.join("label.json"))
            .map_err(|e| e.to_string())?;
        #[cfg(target_os = "macos")]
        File::open(&self.path)
            .and_then(|directory| directory.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn entry(&self) -> Option<Entry> {
        let metadata = fs::symlink_metadata(self.project()).ok()?;
        if !metadata.is_file() {
            return None;
        }
        let label = self.path.join("label.json");
        let title = fs::symlink_metadata(&label)
            .ok()
            .filter(|m| m.is_file() && m.len() < 8192)
            .and_then(|_| fs::read(&label).ok())
            .and_then(|bytes| serde_json::from_slice::<Label>(&bytes).ok())
            .map(|label| label.title)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Untitled image".into());
        Some(Entry {
            id: self.path.clone(),
            title,
            modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        })
    }
}
impl Drop for Slot {
    fn drop(&mut self) {
        // Unfinished drafts survive all unexpected exits. Only empty slots are tidied.
        if !self.project().exists() {
            if let Ok(items) = fs::read_dir(&self.path) {
                for item in items.flatten() {
                    if item.file_name().to_string_lossy().starts_with(".tmp")
                        && item.file_type().is_ok_and(|kind| kind.is_file())
                    {
                        let _ = fs::remove_file(item.path());
                    }
                }
            }
            let _ = fs::remove_file(self.path.join("ready"));
            self.lock.take();
            let _ = fs::remove_file(self.path.join("label.json"));
            let _ = fs::remove_file(self.path.join("session.lock"));
            let _ = fs::remove_dir(&self.path);
        }
    }
}

struct Store {
    active: Slot,
    pending: Vec<Slot>,
}
impl Store {
    fn open(root: &Path) -> Result<Self, String> {
        fs::create_dir_all(root).map_err(|e| format!("Cannot create recovery folder: {e}"))?;
        let mut pending = Vec::new();
        for item in fs::read_dir(root).map_err(|e| e.to_string())? {
            let item = item.map_err(|e| e.to_string())?;
            if !item.file_name().to_string_lossy().starts_with("draft-") {
                continue;
            }
            if let Some(slot) = Slot::open(&item.path())?
                && slot.entry().is_some()
            {
                pending.push(slot);
            }
        }
        Ok(Self {
            active: Slot::create(root)?,
            pending,
        })
    }
    fn entries(&self) -> Vec<Entry> {
        let mut entries: Vec<_> = self.pending.iter().filter_map(Slot::entry).collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.modified));
        entries
    }
    fn recover(&mut self, id: &Path) -> Result<Document, String> {
        let index = self
            .pending
            .iter()
            .position(|s| s.path == id)
            .ok_or("Recovery no longer available.")?;
        let slot = &self.pending[index];
        let document = document::load_project(&slot.project())?;
        self.active.clear()?;
        self.active = self.pending.remove(index);
        Ok(document)
    }
    fn discard(&mut self, id: &Path) -> Result<(), String> {
        let index = self
            .pending
            .iter()
            .position(|s| s.path == id)
            .ok_or("Recovery no longer available.")?;
        self.pending[index].clear()?;
        self.pending.remove(index);
        Ok(())
    }
}

pub enum Command {
    Save(u64, Box<Document>, String),
    Clear(u64),
    Recover(PathBuf),
    Discard(PathBuf),
    Arm,
    Stop,
}
pub enum Event {
    Ready(Result<Vec<Entry>, String>),
    Written(u64, Result<(), String>),
    Cleared(u64, Result<(), String>),
    Recovered(PathBuf, Result<Box<Document>, String>),
    Discarded(PathBuf, Result<(), String>),
    Tick,
    Stopped(Result<(), String>),
}
pub fn directory(demo: bool) -> Result<PathBuf, String> {
    if demo {
        return Ok(std::env::var_os("MYIMAGE_RECOVERY_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("MyImage-demo-recovery")));
    }
    directories::ProjectDirs::from("com", "MyImage", "MyImage")
        .map(|dirs| dirs.data_local_dir().join("Recovery"))
        .ok_or_else(|| "Cannot locate the recovery folder.".into())
}
pub fn start(
    root: Result<PathBuf, String>,
    notify: impl Fn(Event) + Send + 'static,
) -> Result<mpsc::Sender<Command>, String> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("myimage-autosave".into())
        .spawn(move || {
            let mut store = match root.and_then(|root| Store::open(&root)) {
                Ok(store) => store,
                Err(error) => {
                    notify(Event::Ready(Err(error)));
                    return;
                }
            };
            notify(Event::Ready(Ok(store.entries())));
            let mut armed = false;
            loop {
                let command = if armed {
                    match rx.recv_timeout(INTERVAL) {
                        Ok(command) => command,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            armed = false;
                            notify(Event::Tick);
                            continue;
                        }
                        Err(_) => break,
                    }
                } else {
                    match rx.recv() {
                        Ok(command) => command,
                        Err(_) => break,
                    }
                };
                match command {
                    Command::Arm => armed = true,
                    Command::Save(epoch, document, title) => {
                        notify(Event::Written(epoch, store.active.write(&document, &title)))
                    }
                    Command::Clear(epoch) => notify(Event::Cleared(epoch, store.active.clear())),
                    Command::Recover(id) => {
                        let result = store.recover(&id).map(Box::new);
                        notify(Event::Recovered(id, result));
                    }
                    Command::Discard(id) => {
                        let result = store.discard(&id);
                        notify(Event::Discarded(id, result));
                    }
                    Command::Stop => {
                        notify(Event::Stopped(store.active.clear()));
                        break;
                    }
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{document::AnnotationKind, markup_tests::object};
    use std::sync::Arc;

    fn fixture() -> Document {
        let mut text = object(AnnotationKind::Text, [10., 10.], [90., 45.]);
        text.text = "Still editable · Türkçe ✓".into();
        text.group_id = Some(7);
        text.layer.name = "Title".into();
        let mut arrow = object(AnnotationKind::Arrow, [12., 65.], [110., 95.]);
        arrow.group_id = Some(7);
        let mut pen = object(AnnotationKind::Freehand, [20., 20.], [40., 40.]);
        pen.points = vec![pen.start, pen.end];
        Document {
            image: Arc::new(image::RgbaImage::from_pixel(
                128,
                100,
                image::Rgba([40, 80, 120, 127]),
            )),
            annotations: vec![
                text,
                arrow,
                pen,
                object(AnnotationKind::Blur, [60., 30.], [90., 60.]),
            ],
            geometry: crate::geometry::ImageGeometry {
                flip_horizontal: true,
                ..Default::default()
            },
        }
    }
    #[test]
    fn unfinished_draft_reopens_as_editable_project_without_touching_original() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("Original.myimage");
        let document = fixture();
        document::save_project(&document, &original).unwrap();
        let original_bytes = fs::read(&original).unwrap();
        let recovery = root.path().join("Recovery");
        {
            let store = Store::open(&recovery).unwrap();
            store.active.write(&document, "Original.myimage").unwrap();
            // Drop represents an exit without the explicit clean-close command.
        }
        let mut restarted = Store::open(&recovery).unwrap();
        let entry = restarted.entries().remove(0);
        assert_eq!(entry.title, "Original.myimage");
        let mut restored = restarted.recover(&entry.id).unwrap();
        assert_eq!(restored.edit_state(), document.edit_state());
        assert_eq!(restored.image, document.image);
        restored.annotations[0].text = "Edited after recovery".into();
        restarted
            .active
            .write(&restored, "Recovered original")
            .unwrap();
        drop(restarted);
        let mut restarted = Store::open(&recovery).unwrap();
        let entry = restarted.entries().remove(0);
        assert_eq!(
            restarted.recover(&entry.id).unwrap().annotations[0].text,
            "Edited after recovery"
        );
        assert_eq!(fs::read(original).unwrap(), original_bytes);
    }
    #[test]
    fn live_sessions_and_offered_recoveries_are_exclusively_locked() {
        let root = tempfile::tempdir().unwrap();
        let first = Store::open(root.path()).unwrap();
        first.active.write(&fixture(), "First").unwrap();
        let second = Store::open(root.path()).unwrap();
        assert!(second.entries().is_empty());
        second.active.write(&fixture(), "Second").unwrap();
        drop(first);
        let third = Store::open(root.path()).unwrap();
        assert_eq!(third.entries().len(), 1);
        assert_eq!(third.entries()[0].title, "First");
        let fourth = Store::open(root.path()).unwrap();
        assert!(
            fourth.entries().is_empty(),
            "pending prompt retains its lock"
        );
        drop((third, second, fourth));
        assert_eq!(Store::open(root.path()).unwrap().entries().len(), 2);
    }
    #[test]
    fn failed_snapshot_keeps_last_complete_draft() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let good = fixture();
        store.active.write(&good, "Good").unwrap();
        let mut invalid = good.clone();
        invalid.annotations[0].font_size = f32::NAN;
        assert!(store.active.write(&invalid, "Bad").is_err());
        assert_eq!(
            document::load_project(&store.active.project())
                .unwrap()
                .edit_state(),
            good.edit_state()
        );
    }
    #[test]
    fn discard_corrupt_recovery_does_not_remove_other_drafts_or_saved_files() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        store.active.write(&fixture(), "Bad").unwrap();
        fs::write(store.active.project(), b"truncated").unwrap();
        drop(store);
        let mut store = Store::open(root.path()).unwrap();
        let entry = store.entries().remove(0);
        assert!(store.recover(&entry.id).is_err());
        assert_eq!(store.entries().len(), 1);
        store.discard(&entry.id).unwrap();
        assert!(store.entries().is_empty());
        assert!(!entry.id.exists());
        assert!(store.active.path.exists());
    }
    #[test]
    fn serial_worker_clears_after_queued_write_and_clean_quit_preserves_deferred_recoveries() {
        let root = tempfile::tempdir().unwrap();
        let prior = Store::open(root.path()).unwrap();
        prior.active.write(&fixture(), "Deferred").unwrap();
        drop(prior);
        let (events_tx, events_rx) = mpsc::channel();
        let worker = start(Ok(root.path().to_owned()), move |e| {
            events_tx.send(e).unwrap();
        })
        .unwrap();
        assert!(
            matches!(events_rx.recv().unwrap(), Event::Ready(Ok(entries)) if entries.len() == 1)
        );
        worker
            .send(Command::Save(1, Box::new(fixture()), "Current".into()))
            .unwrap();
        worker.send(Command::Clear(2)).unwrap();
        worker.send(Command::Stop).unwrap();
        assert!(matches!(
            events_rx.recv().unwrap(),
            Event::Written(1, Ok(()))
        ));
        assert!(matches!(
            events_rx.recv().unwrap(),
            Event::Cleared(2, Ok(()))
        ));
        assert!(matches!(events_rx.recv().unwrap(), Event::Stopped(Ok(()))));
        // Channel disconnection means the worker (and all its locks) was dropped.
        assert!(events_rx.recv_timeout(Duration::from_secs(2)).is_err());
        let restarted = Store::open(root.path()).unwrap();
        assert_eq!(restarted.entries().len(), 1);
        assert_eq!(restarted.entries()[0].title, "Deferred");
    }
    #[test]
    fn one_snapshot_per_session_is_replaced_without_history_growth() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let mut document = fixture();
        for i in 0..12 {
            document.annotations[0].text = format!("Revision {i}");
            store.active.write(&document, "Work").unwrap();
        }
        assert_eq!(fs::read_dir(&store.active.path).unwrap().count(), 4);
        assert_eq!(
            document::load_project(&store.active.project())
                .unwrap()
                .annotations[0]
                .text,
            "Revision 11"
        );
        store.active.clear().unwrap();
        drop(store);
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
    #[cfg(unix)]
    #[test]
    fn linked_recovery_folders_are_ignored() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("recovery.myimage"), b"do not touch").unwrap();
        symlink(outside.path(), root.path().join("draft-link")).unwrap();
        assert!(Store::open(root.path()).unwrap().entries().is_empty());
        assert_eq!(
            fs::read(outside.path().join("recovery.myimage")).unwrap(),
            b"do not touch"
        );
    }
    #[test]
    fn abrupt_process_exit_releases_lock_and_recovers_complete_snapshot() {
        let root = tempfile::tempdir().unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "recovery::tests::crash_child", "--ignored"])
            .env("MYIMAGE_CRASH_TEST_ROOT", root.path())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(86));
        let mut store = Store::open(root.path()).unwrap();
        let entry = store.entries().remove(0);
        assert_eq!(
            store.recover(&entry.id).unwrap().edit_state(),
            fixture().edit_state()
        );
    }
    #[test]
    #[ignore = "subprocess helper: exits without running destructors"]
    fn crash_child() {
        let Some(root) = std::env::var_os("MYIMAGE_CRASH_TEST_ROOT") else {
            return;
        };
        let store = Store::open(Path::new(&root)).unwrap();
        store.active.write(&fixture(), "Crash test").unwrap();
        // An unfinished next write must not replace the complete published file.
        fs::write(
            store.active.path.join(".tmp-incomplete"),
            b"partial next snapshot",
        )
        .unwrap();
        std::process::exit(86);
    }
}
