//! Background checks keep stale thumbnails and externally deleted files out of
//! Recents without adding filesystem I/O to every interactive frame.
use super::*;
use std::time::{Duration, Instant};
const CHECK_INTERVAL: Duration = Duration::from_secs(5);

impl LibraryPanel {
    pub(super) fn check_paths(&mut self, ctx: &egui::Context, paths: &[PathBuf]) {
        let now = Instant::now();
        let paths: Vec<_> = paths
            .iter()
            .filter(|p| {
                !self.missing_paths.contains(*p)
                    && self
                        .checked_paths
                        .get(*p)
                        .is_none_or(|t| now.duration_since(*t) >= CHECK_INTERVAL)
            })
            .cloned()
            .collect();
        if !paths.is_empty()
            && let Some(worker) = &self.worker
            && worker
                .requests
                .try_send(Request::CheckPaths(
                    self.generation.load(Ordering::Relaxed),
                    paths.clone(),
                ))
                .is_ok()
        {
            // Bound metadata too, independently of the thumbnail cache.
            if self.checked_paths.len() > MAX_TEXTURES * 2 {
                self.checked_paths
                    .retain(|_, t| now.duration_since(*t) < CHECK_INTERVAL);
            }
            for path in paths {
                self.checked_paths.insert(path, now);
            }
        }
        ctx.request_repaint_after(CHECK_INTERVAL);
    }

    pub(super) fn note_missing(&mut self, paths: &[PathBuf]) {
        if paths.is_empty() {
            return;
        }
        self.forget_paths(paths);
        self.entries.retain(|e| !paths.contains(&e.path));
        self.filter_dirty = true;
        for path in paths {
            self.pending.remove(path);
            if let Some(preview) = self.previews.remove(path) {
                self.preview_bytes -= preview.bytes;
            }
            self.missing_paths.insert(path.clone());
        }
    }

    pub fn take_missing_paths(&mut self) -> Vec<PathBuf> {
        self.missing_paths.drain().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn externally_deleted_recents_drop_cached_thumbnails_and_both_selections() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("removed.png");
        let present = dir.path().join("retained.png");
        std::fs::write(&missing, b"image").unwrap();
        std::fs::write(&present, b"image").unwrap();
        let mut p = LibraryPanel::default();
        let paths = vec![missing.clone(), present.clone()];
        p.entries = paths
            .iter()
            .map(|path| LibraryEntry {
                path: path.clone(),
                name: "image.png".into(),
                date: chrono::Local::now().date_naive(),
                kind: EntryKind::Image,
                modified: None,
                created: None,
                bytes: 5,
            })
            .collect();
        p.selection.paths.extend(paths.clone());
        p.recent_selection.paths.extend(paths.clone());
        p.preferences.favorites.insert(missing.clone());
        p.previews.insert(
            missing.clone(),
            CachedPreview {
                preview: Preview::Failed("stale thumbnail".into()),
                bytes: 0,
                used: 0,
            },
        );
        std::fs::remove_file(&missing).unwrap();
        let ctx = egui::Context::default();
        p.worker = Some(Worker::start(ctx.clone(), p.generation.clone()).unwrap());
        p.check_paths(&ctx, &paths);
        let reply = p
            .worker
            .as_ref()
            .unwrap()
            .replies
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        let Reply::Missing(_, absent) = reply else {
            panic!("Expected availability result")
        };
        assert_eq!(absent, vec![missing.clone()]);
        p.note_missing(&absent);
        assert_eq!(p.entries.len(), 1);
        assert_eq!(p.selection.list(), vec![present.clone()]);
        assert_eq!(p.recent_selection.list(), vec![present.clone()]);
        assert!(!p.previews.contains_key(&missing));
        assert!(!p.preferences.favorites.contains(&missing));
        assert_eq!(p.take_missing_paths(), vec![missing]);
        assert!(p.take_missing_paths().is_empty());
        assert_eq!(std::fs::read(present).unwrap(), b"image");
    }
}
