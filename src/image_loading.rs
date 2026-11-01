//! Shared file-opening worker for Recents, Library and the file picker.
//! Decoding, resampling and RGBA conversion never run in an interactive frame.

use crate::document::{self, Document};
use eframe::egui::ColorImage;
use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Instant, SystemTime},
};

const CACHE_BYTES: usize = 128 * 1024 * 1024;
const CACHE_FILES: usize = 4;

#[derive(Clone)]
pub struct PreparedDocument {
    pub document: Document,
    pub warnings: Vec<String>,
    pub source: Arc<ColorImage>,
    pub composition: Option<Arc<ColorImage>>,
    pub max_side: u32,
}

impl PreparedDocument {
    fn prepare(document: Document, max_side: u32) -> Result<Self, String> {
        let source = Arc::new(prepare_pixels(&document.image, max_side));
        let composition = if document.geometry.canvas.is_some()
            || (document.geometry.has_cutouts() || document.geometry.effects.active())
            || document
                .annotations
                .iter()
                .any(|a| a.visible() && a.kind.needs_composite())
        {
            let pixels = crate::render::privacy_preview(
                &document,
                max_side.min(crate::privacy_preview::MAX_PREVIEW_SIDE),
            )?;
            Some(Arc::new(color_image(&pixels)))
        } else {
            None
        };
        Ok(Self {
            document,
            warnings: Vec::new(),
            source,
            composition,
            max_side,
        })
    }

    fn bytes(&self) -> usize {
        self.document.image.len()
            + document::state_bytes(&self.document.edit_state())
            + self.warnings.iter().map(String::len).sum::<usize>()
            + self.source.pixels.len() * 4
            + self
                .composition
                .as_ref()
                .map_or(0, |image| image.pixels.len() * 4)
    }
}

pub fn color_image(image: &image::RgbaImage) -> ColorImage {
    ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    )
}

pub fn prepare_pixels(image: &image::RgbaImage, max_side: u32) -> ColorImage {
    let max_side = max_side.max(1);
    if image.width().max(image.height()) <= max_side {
        return color_image(image);
    }
    let scale = max_side as f64 / image.width().max(image.height()) as f64;
    color_image(&image::imageops::resize(
        image,
        (image.width() as f64 * scale).round().max(1.0) as u32,
        (image.height() as f64 * scale).round().max(1.0) as u32,
        image::imageops::FilterType::Triangle,
    ))
}

#[derive(PartialEq)]
struct FileVersion {
    bytes: u64,
    modified: SystemTime,
    created: Option<SystemTime>,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}

impl FileVersion {
    fn read(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        Some(Self {
            bytes: metadata.len(),
            modified: metadata.modified().ok()?,
            created: metadata.created().ok(),
            #[cfg(unix)]
            identity: {
                use std::os::unix::fs::MetadataExt;
                (
                    metadata.dev(),
                    metadata.ino(),
                    metadata.ctime(),
                    metadata.ctime_nsec(),
                )
            },
        })
    }
}

struct CachedFile {
    path: PathBuf,
    version: FileVersion,
    prepared: PreparedDocument,
    bytes: usize,
}

#[derive(Default)]
pub struct LoadCache {
    files: VecDeque<CachedFile>,
    bytes: usize,
    active: Option<PathBuf>,
}

impl LoadCache {
    pub fn open(&mut self, path: &Path, max_side: u32) -> Result<PreparedDocument, String> {
        self.open_with_budget(path, max_side, None)
    }

    fn open_with_budget(
        &mut self,
        path: &Path,
        max_side: u32,
        budget: Option<u64>,
    ) -> Result<PreparedDocument, String> {
        let started = Instant::now();
        let canonical =
            fs::canonicalize(path).map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
        let version = FileVersion::read(&canonical);
        let max_side = max_side.max(1);
        if let Some(index) = self.files.iter().position(|file| file.path == canonical) {
            let file = self.files.remove(index).unwrap();
            if version.as_ref() == Some(&file.version) && max_side == file.prepared.max_side {
                let result = file.prepared.clone();
                self.active = Some(canonical);
                self.files.push_back(file);
                log::debug!("image opening: cache hit in {:?}", started.elapsed());
                return Ok(result);
            }
            self.bytes -= file.bytes;
        }
        let mut warnings = Vec::new();
        let document = if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("myimage"))
        {
            if let Some(budget) = budget {
                document::load_project_with_budget(&canonical, budget)?
            } else {
                document::load_project(&canonical)?
            }
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("snagx"))
        {
            let imported = crate::snagit_import::import(&canonical)?;
            warnings = imported.warnings;
            imported.document
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("heif") || e.eq_ignore_ascii_case("heic"))
        {
            Document {
                image: Arc::new(crate::heif::load(&canonical)?),
                annotations: Vec::new(),
                geometry: Default::default(),
            }
        } else {
            Document {
                // Preserve decoder allocation limits and the complete original pixels.
                image: Arc::new(
                    image::ImageReader::open(&canonical)
                        .map_err(|e| e.to_string())?
                        .with_guessed_format()
                        .map_err(|e| e.to_string())?
                        .decode()
                        .map_err(|e| e.to_string())?
                        .into_rgba8(),
                ),
                annotations: Vec::new(),
                geometry: Default::default(),
            }
        };
        let decoded = started.elapsed();
        let mut prepared = PreparedDocument::prepare(document, max_side)?;
        prepared.warnings = warnings;
        // A file replaced while decoding must not be cached as the newer version.
        if version != FileVersion::read(&canonical) {
            return Err("The file changed while opening. Please open it again.".into());
        }
        let bytes = prepared.bytes();
        if let Some(version) = version.filter(|_| bytes <= CACHE_BYTES) {
            while self.bytes + bytes > CACHE_BYTES || self.files.len() >= CACHE_FILES {
                if let Some(file) = self.files.pop_front() {
                    self.bytes -= file.bytes;
                } else {
                    break;
                }
            }
            self.files.push_back(CachedFile {
                path: canonical.clone(),
                version,
                prepared: prepared.clone(),
                bytes,
            });
            self.bytes += bytes;
        }
        self.active = Some(canonical);
        log::debug!(
            "image opening: decode={decoded:?}, prepared={:?}",
            started.elapsed()
        );
        Ok(prepared)
    }
}

impl LoadCache {
    fn contains_current(&self, path: &Path, side: u32) -> bool {
        let Ok(path) = fs::canonicalize(path) else {
            return false;
        };
        let version = FileVersion::read(&path);
        self.files.iter().any(|f| {
            f.path == path
                && version.as_ref() == Some(&f.version)
                && f.prepared.max_side == side.max(1)
        })
    }

    /// Speculation must retain the active file and never replace identical pixels
    /// already opened in the foreground (their identity also keys the GPU cache).
    fn warm(&mut self, file: CachedFile) {
        if FileVersion::read(&file.path).as_ref() != Some(&file.version)
            || self.files.iter().any(|f| {
                f.path == file.path
                    && f.version == file.version
                    && f.prepared.max_side == file.prepared.max_side
            })
        {
            return;
        }
        let active_bytes = self
            .files
            .iter()
            .filter(|f| Some(&f.path) == self.active.as_ref() && f.path != file.path)
            .map(|f| f.bytes)
            .sum::<usize>();
        if active_bytes.saturating_add(file.bytes) > CACHE_BYTES {
            return;
        }
        if let Some(index) = self.files.iter().position(|f| f.path == file.path) {
            self.bytes -= self.files.remove(index).unwrap().bytes;
        }
        while self.bytes + file.bytes > CACHE_BYTES || self.files.len() >= CACHE_FILES {
            let Some(index) = self
                .files
                .iter()
                .position(|f| Some(&f.path) != self.active.as_ref())
            else {
                return;
            };
            self.bytes -= self.files.remove(index).unwrap().bytes;
        }
        self.bytes += file.bytes;
        self.files.push_back(file);
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Prefetch {
    path: PathBuf,
    side: u32,
    revision: u64,
}
enum Request {
    Open(PathBuf, u32),
    Prefetch(Prefetch),
    Warm(u64, Box<CachedFile>),
    Clear,
    Stop,
}

pub struct Loader {
    tx: mpsc::SyncSender<Request>,
    revision: Arc<AtomicU64>,
    stopped: Arc<AtomicBool>,
    last_prefetch: Mutex<Option<(PathBuf, u32)>>,
    #[cfg(test)]
    pub(crate) warmed: mpsc::Receiver<(PathBuf, std::sync::Weak<ColorImage>)>,
}

impl Loader {
    pub fn start(
        complete: impl Fn(PathBuf, Result<PreparedDocument, String>) + Send + 'static,
    ) -> Result<Self, String> {
        Self::start_inner(complete, |_| {})
    }

    fn start_inner(
        complete: impl Fn(PathBuf, Result<PreparedDocument, String>) + Send + 'static,
        before_prefetch: impl Fn(&Path) + Send + 'static,
    ) -> Result<Self, String> {
        let (tx, rx) = mpsc::sync_channel(4);
        let revision = Arc::new(AtomicU64::new(0));
        let stopped = Arc::new(AtomicBool::new(false));
        let latest = Arc::new(Mutex::new(None::<Prefetch>));
        let (wake_tx, wake_rx) = mpsc::sync_channel(1);
        let warm_tx = tx.clone();
        let warm_revision = revision.clone();
        let warm_latest = latest.clone();
        // A single speculative decode runs separately, so selecting another file
        // never waits behind it. Only the latest candidate can enter the cache.
        std::thread::Builder::new()
            .name("myimage-prefetch".into())
            .spawn(move || {
                while wake_rx.recv().is_ok() {
                    let Some(request) = warm_latest.lock().unwrap().take() else {
                        continue;
                    };
                    if warm_revision.load(Ordering::Relaxed) != request.revision {
                        continue;
                    }
                    before_prefetch(&request.path);
                    if warm_revision.load(Ordering::Relaxed) != request.revision {
                        continue;
                    }
                    let mut cache = LoadCache::default();
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        // Leave room for both original RGBA and display pixels.
                        cache.open_with_budget(
                            &request.path,
                            request.side,
                            Some((CACHE_BYTES / 2) as u64),
                        )
                    }));
                    // Speculative failures remain silent. The normal open path still
                    // reports errors/removes missing entries when actually selected.
                    if matches!(result, Ok(Ok(_)))
                        && warm_revision.load(Ordering::Relaxed) == request.revision
                        && let Some(file) = cache.files.pop_back()
                    {
                        let _ = warm_tx.try_send(Request::Warm(request.revision, Box::new(file)));
                    }
                }
            })
            .map_err(|e| format!("Could not start image preparation: {e}"))?;
        #[cfg(test)]
        let (warmed_tx, warmed) = mpsc::channel();
        let worker_revision = revision.clone();
        let worker_stopped = stopped.clone();
        std::thread::Builder::new()
            .name("myimage-open".into())
            .spawn(move || {
                let mut cache = LoadCache::default();
                while let Ok(request) = rx.recv() {
                    if worker_stopped.load(Ordering::Relaxed) {
                        break;
                    }
                    match request {
                        Request::Stop => break,
                        Request::Clear => cache = LoadCache::default(),
                        Request::Prefetch(request) => {
                            if worker_revision.load(Ordering::Relaxed) == request.revision
                                && !cache.contains_current(&request.path, request.side)
                            {
                                *latest.lock().unwrap() = Some(request);
                                let _ = wake_tx.try_send(());
                            }
                        }
                        Request::Warm(revision, file) => {
                            if revision == worker_revision.load(Ordering::Relaxed) {
                                #[cfg(test)]
                                let warmed =
                                    (file.path.clone(), Arc::downgrade(&file.prepared.source));
                                cache.warm(*file);
                                #[cfg(test)]
                                let _ = warmed_tx.send(warmed);
                            }
                        }
                        Request::Open(path, side) => {
                            let result =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    cache.open(&path, side)
                                }))
                                .unwrap_or_else(|_| Err("The image could not be prepared.".into()));
                            complete(path, result);
                        }
                    }
                }
            })
            .map_err(|e| format!("Could not start the image loader: {e}"))?;
        Ok(Self {
            tx,
            revision,
            stopped,
            last_prefetch: Mutex::new(None),
            #[cfg(test)]
            warmed,
        })
    }

    pub fn open(&self, path: PathBuf, max_side: u32) -> Result<(), String> {
        self.tx
            .try_send(Request::Open(path, max_side))
            .map_err(|_| "The image loader is unavailable. Please reopen MyImage.".into())
    }

    pub fn prefetch(&self, path: PathBuf, max_side: u32) {
        if !crate::project_store::is_project(&path) {
            return;
        }
        let mut last = self.last_prefetch.lock().unwrap();
        let side = max_side.max(1);
        if last.as_ref() == Some(&(path.clone(), side)) {
            return;
        }
        let revision = self.revision.fetch_add(1, Ordering::Relaxed) + 1;
        if self
            .tx
            .try_send(Request::Prefetch(Prefetch {
                path: path.clone(),
                side,
                revision,
            }))
            .is_ok()
        {
            *last = Some((path, side));
        }
    }

    pub fn clear(&self) {
        self.revision.fetch_add(1, Ordering::Relaxed);
        *self.last_prefetch.lock().unwrap() = None;
        let _ = self.tx.try_send(Request::Clear);
    }
}

impl Drop for Loader {
    fn drop(&mut self) {
        self.revision.fetch_add(1, Ordering::Relaxed);
        self.stopped.store(true, Ordering::Relaxed);
        // Break the worker-channel ownership cycle without waiting for decoding.
        let _ = self.tx.try_send(Request::Stop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui;
    use image::{Rgba, RgbaImage};

    fn fixture(path: &Path, width: u32) {
        RgbaImage::from_pixel(width, 48, Rgba([190, 90, 40, 128]))
            .save(path)
            .unwrap();
    }

    fn project_fixture(path: &Path, width: u32) {
        document::save_project(
            &Document {
                image: Arc::new(RgbaImage::from_pixel(width, 48, Rgba([190, 90, 40, 128]))),
                annotations: vec![],
                geometry: Default::default(),
            },
            path,
        )
        .unwrap();
    }

    #[test]
    fn speculative_project_budget_rejects_large_decode_without_limiting_explicit_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.myimage");
        project_fixture(&path, 100);
        let mut cache = LoadCache::default();
        assert!(cache.open_with_budget(&path, 1024, Some(100)).is_err());
        assert!(cache.files.is_empty());
        let loaded = cache.open(&path, 1024).unwrap();
        assert_eq!(loaded.document.image.dimensions(), (100, 48));
    }

    fn cached_file(path: &Path) -> CachedFile {
        let mut cache = LoadCache::default();
        cache.open(path, 1024).unwrap();
        cache.files.pop_back().unwrap()
    }

    #[test]
    fn speculative_cache_retains_active_pixels_and_rejects_changed_files() {
        let dir = tempfile::tempdir().unwrap();
        let active = dir.path().join("active.png");
        fixture(&active, 64);
        let mut cache = LoadCache::default();
        let opened = cache.open(&active, 1024).unwrap();
        // An overlapping foreground open wins: warming must not change identity.
        cache.warm(cached_file(&active));
        assert!(Arc::ptr_eq(
            &opened.source,
            &cache.open(&active, 1024).unwrap().source
        ));
        for index in 0..CACHE_FILES + 2 {
            let path = dir.path().join(format!("next-{index}.png"));
            fixture(&path, 70);
            cache.warm(cached_file(&path));
        }
        assert!(Arc::ptr_eq(
            &opened.source,
            &cache.open(&active, 1024).unwrap().source
        ));
        assert_eq!(cache.files.len(), CACHE_FILES);
        assert_eq!(
            cache.bytes,
            cache.files.iter().map(|f| f.bytes).sum::<usize>()
        );
        let next = dir.path().join("changed.png");
        fixture(&next, 64);
        let stale = cached_file(&next);
        fixture(&next, 65);
        cache.warm(stale);
        assert!(!cache.contains_current(&next, 1024));
        let mut too_large = cached_file(&next);
        too_large.bytes = CACHE_BYTES;
        cache.warm(too_large);
        assert!(!cache.contains_current(&next, 1024));
        assert!(cache.bytes <= CACHE_BYTES);
        assert!(Arc::ptr_eq(
            &opened.source,
            &cache.open(&active, 1024).unwrap().source
        ));
    }

    #[test]
    fn speculative_open_reuses_ready_pixels_and_revalidates_on_selection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("next.myimage");
        project_fixture(&path, 64);
        let (tx, rx) = mpsc::channel();
        let loader = Loader::start(move |path, result| {
            let _ = tx.send((path, result));
        })
        .unwrap();
        loader.prefetch(path.clone(), 1024);
        let (_, weak) = loader
            .warmed
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let ready = weak.upgrade().expect("prepared image remains cached");
        assert!(
            rx.try_recv().is_err(),
            "speculation must not open a document"
        );
        loader.open(path.clone(), 1024).unwrap();
        let (_, selected) = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert!(Arc::ptr_eq(&ready, &selected.unwrap().source));
        project_fixture(&path, 71);
        loader.open(path.clone(), 1024).unwrap();
        let (_, changed) = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        let changed = changed.unwrap();
        assert_eq!(changed.source.size[0], 71);
        assert!(!Arc::ptr_eq(&ready, &changed.source));
        fs::remove_file(&path).unwrap();
        loader.open(path, 1024).unwrap();
        assert!(
            rx.recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
                .1
                .is_err()
        );
    }

    #[test]
    fn slow_prefetch_never_blocks_foreground_and_only_latest_candidate_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let paths: Vec<_> = (0..4)
            .map(|i| dir.path().join(format!("{i}.myimage")))
            .collect();
        for path in &paths {
            project_fixture(path, 64);
        }
        let (started_tx, started_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let gated = paths[0].clone();
        let (tx, rx) = mpsc::channel();
        let loader = Loader::start_inner(
            move |path, result| {
                let _ = tx.send((path, result));
            },
            move |path| {
                if path == gated {
                    started_tx.send(()).unwrap();
                    resume_rx
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .unwrap();
                }
            },
        )
        .unwrap();
        loader.prefetch(paths[0].clone(), 1024);
        started_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        loader.open(paths[1].clone(), 1024).unwrap();
        let (path, result) = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(path, paths[1]);
        assert!(result.is_ok());
        // The speculative thread is still blocked here. Coalesce to newest work.
        loader.prefetch(paths[2].clone(), 1024);
        loader.prefetch(paths[3].clone(), 1024);
        resume_tx.send(()).unwrap();
        let (warmed, _) = loader
            .warmed
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert_eq!(warmed, fs::canonicalize(&paths[3]).unwrap());
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn clearing_cancels_inflight_prefetch_and_allows_the_same_candidate_again() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("next.myimage");
        project_fixture(&path, 64);
        let (started_tx, started_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let (closed_tx, closed_rx) = mpsc::channel::<()>();
        let loader = Loader::start_inner(
            move |_, _| {
                let _ = closed_tx.send(());
            },
            move |_| {
                started_tx.send(()).unwrap();
                resume_rx
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
            },
        )
        .unwrap();
        loader.prefetch(path.clone(), 1024);
        started_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        loader.clear();
        loader.prefetch(path, 1024);
        resume_tx.send(()).unwrap();
        started_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(
            loader.warmed.try_recv().is_err(),
            "cancelled work cannot repopulate the cache"
        );
        resume_tx.send(()).unwrap();
        let (_, weak) = loader
            .warmed
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(weak.upgrade().is_some());
        loader.clear();
        drop(loader);
        assert!(matches!(
            closed_rx.recv_timeout(std::time::Duration::from_secs(5)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
        assert!(matches!(
            started_rx.recv_timeout(std::time::Duration::from_secs(5)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
        assert!(
            weak.upgrade().is_none(),
            "both workers and cached pixels are released"
        );
    }

    #[test]
    #[ignore = "manual strip browsing performance measurement on a generated project"]
    fn measure_strip_prefetch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("capture.myimage");
        let document = Document {
            image: Arc::new(RgbaImage::from_fn(3200, 2000, |x, y| {
                Rgba([(x % 251) as u8, (y % 241) as u8, ((x + y) % 239) as u8, 255])
            })),
            annotations: vec![],
            geometry: Default::default(),
        };
        document::save_project(&document, &path).unwrap();
        let (tx, rx) = mpsc::channel();
        let loader = Loader::start(move |_, result| {
            tx.send(result).unwrap();
        })
        .unwrap();
        let start = Instant::now();
        loader.open(path.clone(), 8192).unwrap();
        let cold = rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap()
            .unwrap();
        let cold_time = start.elapsed();
        drop(cold);
        loader.clear();
        loader.prefetch(path.clone(), 8192);
        let (_, prepared) = loader
            .warmed
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap();
        let start = Instant::now();
        loader.open(path, 8192).unwrap();
        let warmed = rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap()
            .unwrap();
        let ready_time = start.elapsed();
        assert!(Arc::ptr_eq(&warmed.source, &prepared.upgrade().unwrap()));
        eprintln!(
            "strip project 3200x2000: cold={cold_time:?}; prefetched selection={ready_time:?}; original={}x{}",
            warmed.document.image.width(),
            warmed.document.image.height()
        );
    }

    #[test]
    fn reopening_reuses_pixels_but_edits_do_not_mutate_the_saved_document() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.png");
        fixture(&path, 64);
        let mut cache = LoadCache::default();
        let mut first = cache.open(&path, 8192).unwrap();
        first.document.geometry.quarter_turns = 1;
        let again = cache.open(&path, 8192).unwrap();
        assert!(Arc::ptr_eq(&first.document.image, &again.document.image));
        assert!(Arc::ptr_eq(&first.source, &again.source));
        assert_eq!(again.document.geometry.quarter_turns, 0);
        assert_eq!(
            again.source.pixels[0],
            egui::Color32::from_rgba_unmultiplied(190, 90, 40, 128)
        );
    }

    #[test]
    fn replaced_deleted_or_corrupt_files_never_reuse_stale_cache() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.png");
        fixture(&path, 64);
        let mut cache = LoadCache::default();
        let original = cache.open(&path, 1024).unwrap();
        fixture(&path, 70);
        let changed = cache.open(&path, 1024).unwrap();
        assert_eq!(changed.document.image.width(), 70);
        assert!(!Arc::ptr_eq(&original.source, &changed.source));
        fs::write(&path, b"invalid").unwrap();
        assert!(cache.open(&path, 1024).is_err());
        fs::remove_file(&path).unwrap();
        assert!(cache.open(&path, 1024).is_err());
    }

    #[test]
    fn cache_is_lru_and_bounded_and_texture_limits_preserve_original_pixels() {
        let directory = tempfile::tempdir().unwrap();
        let mut cache = LoadCache::default();
        for i in 0..CACHE_FILES {
            let path = directory.path().join(format!("{i}.png"));
            fixture(&path, 64);
            cache.open(&path, 32).unwrap();
        }
        let first = directory.path().join("0.png");
        let prepared = cache.open(&first, 32).unwrap();
        assert_eq!(prepared.source.size, [32, 24]);
        assert_eq!(prepared.document.image.dimensions(), (64, 48));
        assert_eq!(
            prepared.document.image.get_pixel(0, 0),
            &Rgba([190, 90, 40, 128])
        );
        let extra = directory.path().join("extra.png");
        fixture(&extra, 64);
        cache.open(&extra, 32).unwrap();
        assert_eq!(cache.files.len(), CACHE_FILES);
        assert!(!cache.files.iter().any(|f| f.path.ends_with("1.png")));
        assert_eq!(
            cache.bytes,
            cache.files.iter().map(|f| f.bytes).sum::<usize>()
        );
        assert!(cache.bytes <= CACHE_BYTES);
        let larger = cache.open(&first, 64).unwrap();
        assert_eq!(larger.source.size, [64, 48]);
        assert!(!Arc::ptr_eq(&prepared.source, &larger.source));
        // Simulate the byte budget independently of allocating hundreds of MiB.
        cache.files.back_mut().unwrap().bytes += CACHE_BYTES;
        cache.bytes += CACHE_BYTES;
        let final_path = directory.path().join("final.png");
        fixture(&final_path, 64);
        cache.open(&final_path, 64).unwrap();
        assert_eq!(cache.files.len(), 1);
        assert!(cache.bytes <= CACHE_BYTES);
    }

    #[test]
    fn project_effects_are_prepared_without_exposing_unprocessed_pixels() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("effects.myimage");
        let mut document = crate::templates::build(&crate::templates::Spec {
            kind: crate::templates::Kind::TwoSteps,
            title: "Test".into(),
            size: [1200, 800],
            background: [255; 4],
        })
        .unwrap();
        document.annotations.truncate(1);
        document.annotations[0].kind = document::AnnotationKind::Pixelate;
        document.annotations[0].start = document::Point { x: 0., y: 0. };
        document.annotations[0].end = document::Point { x: 64., y: 48. };
        document.image = Arc::new(RgbaImage::from_fn(64, 48, |x, y| {
            Rgba([(x * 4) as u8, (y * 5) as u8, 255, 255])
        }));
        document.geometry.cutouts[0] = Some(crate::geometry::Cutout {
            axis: 0,
            start: 10,
            end: 20,
        });
        crate::document::save_project(&document, &path).unwrap();
        let mut cache = LoadCache::default();
        let opened = cache.open(&path, 1024).unwrap();
        assert!(opened.composition.is_some());
        assert_ne!(opened.composition.as_ref().unwrap(), &opened.source);
        assert_eq!(opened.document.edit_state(), document.edit_state());
        assert_eq!(opened.document.image.as_ref(), document.image.as_ref());
        assert_eq!(
            opened.composition.as_ref().unwrap().as_ref(),
            &color_image(&crate::render::privacy_preview(&document, 1024).unwrap())
        );
    }

    #[test]
    fn loader_runs_off_caller_thread_and_clear_releases_cached_images() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.png");
        fixture(&path, 64);
        let (tx, rx) = mpsc::channel();
        let loader = Loader::start(move |_, result| {
            tx.send((std::thread::current().id(), result)).unwrap();
        })
        .unwrap();
        loader.open(path.clone(), 1024).unwrap();
        let (thread, first) = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        assert_ne!(thread, std::thread::current().id());
        let weak = Arc::downgrade(&first.unwrap().source);
        assert!(weak.upgrade().is_some());
        loader.clear();
        loader.open(path, 1024).unwrap();
        let (_, second) = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        assert!(second.is_ok());
        assert!(weak.upgrade().is_none());
    }
}
