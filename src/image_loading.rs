//! Shared file-opening worker for Recents, Library and the file picker.
//! Decoding, resampling and RGBA conversion never run in an interactive frame.

use crate::document::{self, Document};
use eframe::egui::ColorImage;
use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::{Instant, SystemTime},
};

const CACHE_BYTES: usize = 128 * 1024 * 1024;
const CACHE_FILES: usize = 4;

#[derive(Clone)]
pub struct PreparedDocument {
    pub document: Document,
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
            source,
            composition,
            max_side,
        })
    }

    fn bytes(&self) -> usize {
        self.document.image.len()
            + document::state_bytes(&self.document.edit_state())
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
}

impl LoadCache {
    pub fn open(&mut self, path: &Path, max_side: u32) -> Result<PreparedDocument, String> {
        let started = Instant::now();
        let canonical =
            fs::canonicalize(path).map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
        let version = FileVersion::read(&canonical);
        let max_side = max_side.max(1);
        if let Some(index) = self.files.iter().position(|file| file.path == canonical) {
            let file = self.files.remove(index).unwrap();
            if version.as_ref() == Some(&file.version) && max_side == file.prepared.max_side {
                let result = file.prepared.clone();
                self.files.push_back(file);
                log::debug!("image opening: cache hit in {:?}", started.elapsed());
                return Ok(result);
            }
            self.bytes -= file.bytes;
        }
        let document = if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("myimage"))
        {
            document::load_project(&canonical)?
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
        let prepared = PreparedDocument::prepare(document, max_side)?;
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
                path: canonical,
                version,
                prepared: prepared.clone(),
                bytes,
            });
            self.bytes += bytes;
        }
        log::debug!(
            "image opening: decode={decoded:?}, prepared={:?}",
            started.elapsed()
        );
        Ok(prepared)
    }
}

enum Request {
    Open(PathBuf, u32),
    Clear,
}

pub struct Loader {
    tx: mpsc::SyncSender<Request>,
}

impl Loader {
    pub fn start(
        complete: impl Fn(PathBuf, Result<PreparedDocument, String>) + Send + 'static,
    ) -> Result<Self, String> {
        let (tx, rx) = mpsc::sync_channel(4);
        std::thread::Builder::new()
            .name("myimage-open".into())
            .spawn(move || {
                let mut cache = LoadCache::default();
                while let Ok(request) = rx.recv() {
                    match request {
                        Request::Clear => cache = LoadCache::default(),
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
        Ok(Self { tx })
    }

    pub fn open(&self, path: PathBuf, max_side: u32) -> Result<(), String> {
        self.tx
            .try_send(Request::Open(path, max_side))
            .map_err(|_| "The image loader is unavailable. Please reopen MyImage.".into())
    }

    pub fn clear(&self) {
        let _ = self.tx.try_send(Request::Clear);
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
