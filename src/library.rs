//! Filesystem-backed library: indexing reads metadata, thumbnails decode on demand.

use crate::{document, render};
use chrono::{DateTime, Local, NaiveDate};
use image::{ImageDecoder, ImageReader, RgbaImage};
use std::{
    fs,
    path::{Component, Path, PathBuf},
    time::SystemTime,
};

const MAX_SCAN_ITEMS: usize = 100_000;
const MAX_DEPTH: usize = 32;
const MAX_WARNINGS: usize = 100;
const MAX_IMAGE_SIDE: u32 = 32_768;
const MAX_IMAGE_PIXELS: u64 = 100_000_000;
const MAX_DECODE_BYTES: u64 = 512 * 1024 * 1024;
const THUMBNAIL_WIDTH: u32 = 384;
const THUMBNAIL_HEIGHT: u32 = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Image,
    Project,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryEntry {
    pub path: PathBuf,
    pub name: String,
    pub date: NaiveDate,
    pub kind: EntryKind,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SortBy {
    #[default]
    Modified,
    Created,
    Name,
    Size,
}

impl SortBy {
    pub const ALL: [Self; 4] = [Self::Modified, Self::Created, Self::Name, Self::Size];
    pub fn label(self) -> &'static str {
        match self {
            Self::Modified => "Date modified",
            Self::Created => "Date created",
            Self::Name => "Name",
            Self::Size => "Size",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GalleryView {
    #[default]
    Grid,
    List,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TileSize {
    Small,
    #[default]
    Medium,
    Large,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct BrowserPreferences {
    pub sort: SortBy,
    pub descending: bool,
    pub view: GalleryView,
    pub tile_size: TileSize,
    pub favorites: std::collections::BTreeSet<PathBuf>,
    pub hidden_recents: std::collections::BTreeSet<PathBuf>,
}

impl Default for BrowserPreferences {
    fn default() -> Self {
        Self {
            sort: SortBy::Modified,
            descending: true,
            view: GalleryView::Grid,
            tile_size: TileSize::Medium,
            favorites: Default::default(),
            hidden_recents: Default::default(),
        }
    }
}

/// Sort only the displayed indices; the scan order remains useful for Recents.
/// Missing filesystem dates stay last in either direction. Paths break ties.
pub fn sort_entries(
    entries: &[LibraryEntry],
    indices: &mut [usize],
    sort: SortBy,
    descending: bool,
) {
    use std::cmp::Ordering;
    let compare = |a: &LibraryEntry, b: &LibraryEntry| {
        let order = match sort {
            SortBy::Modified | SortBy::Created => {
                let (a, b) = if sort == SortBy::Modified {
                    (a.modified, b.modified)
                } else {
                    (a.created, b.created)
                };
                match (a, b) {
                    (Some(a), Some(b)) => a.cmp(&b),
                    (None, Some(_)) => return Ordering::Greater,
                    (Some(_), None) => return Ordering::Less,
                    (None, None) => Ordering::Equal,
                }
            }
            SortBy::Size => a.bytes.cmp(&b.bytes),
            SortBy::Name => unreachable!(),
        };
        if descending { order.reverse() } else { order }
    };
    if sort == SortBy::Name {
        // Cache normalized names once instead of allocating in every comparison.
        indices.sort_by_cached_key(|&i| (entries[i].name.to_lowercase(), entries[i].path.clone()));
        if descending {
            indices.reverse();
        }
    } else {
        indices.sort_by(|&a, &b| {
            compare(&entries[a], &entries[b]).then_with(|| entries[a].path.cmp(&entries[b].path))
        });
    }
}

#[derive(Debug, Default)]
pub struct LibraryIndex {
    pub entries: Vec<LibraryEntry>,
    pub warnings: Vec<String>,
}

pub struct Thumbnail {
    pub image: RgbaImage,
    pub original_size: [u32; 2],
}

/// Capture dates come from `root/YYYY/MM/DD`, including files in its subfolders.
/// Other files use their modification date in the user's local timezone. Saving
/// a project beside its original therefore never moves it to another day.
pub fn scan(root: &Path) -> Result<LibraryIndex, String> {
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LibraryIndex::default());
        }
        Err(error) => return Err(format!("Cannot inspect {}: {error}", root.display())),
    };
    if is_link(&metadata) || !metadata.is_dir() {
        return Err(format!(
            "The library folder must be a directory, not a file or symbolic link: {}",
            root.display()
        ));
    }
    let mut index = LibraryIndex::default();
    let mut remaining = MAX_SCAN_ITEMS;
    scan_directory(root, root, 0, &mut remaining, &mut index)?;
    index.entries.sort_by(|a, b| {
        b.date
            .cmp(&a.date)
            .then_with(|| b.modified.cmp(&a.modified))
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(index)
}

fn warning(index: &mut LibraryIndex, message: String) {
    if index.warnings.len() < MAX_WARNINGS {
        index.warnings.push(message);
    } else if index.warnings.len() == MAX_WARNINGS {
        index
            .warnings
            .push("Additional library warnings were omitted.".into());
    }
}

fn scan_limit_warning(index: &mut LibraryIndex) {
    let message = format!(
        "The library scan stopped after {MAX_SCAN_ITEMS} filesystem entries. Choose a smaller library folder."
    );
    if !index.warnings.contains(&message) {
        warning(index, message);
    }
}

fn scan_directory(
    root: &Path,
    directory: &Path,
    depth: usize,
    remaining: &mut usize,
    index: &mut LibraryIndex,
) -> Result<(), String> {
    let listing = match fs::read_dir(directory) {
        Ok(listing) => listing,
        Err(error) => {
            let message = format!("Cannot read {}: {error}", directory.display());
            if depth == 0 {
                return Err(message);
            }
            warning(index, message);
            return Ok(());
        }
    };
    // Bound both the pending traversal and index, including unsupported files.
    // Sorting directory entries also makes ordinary scans reproducible.
    let mut children = Vec::new();
    for child in listing {
        if *remaining == 0 {
            scan_limit_warning(index);
            break;
        }
        *remaining -= 1;
        match child {
            Ok(child) => children.push(child),
            Err(error) => warning(
                index,
                format!("Cannot read an entry in {}: {error}", directory.display()),
            ),
        }
    }
    children.sort_by_key(|child| child.file_name());
    for child in children {
        let path = child.path();
        let metadata = match child.metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                warning(index, format!("Cannot inspect {}: {error}", path.display()));
                continue;
            }
        };
        if is_link(&metadata) {
            continue;
        }
        if metadata.is_dir() {
            if is_hidden_directory(&path, &metadata) {
                continue;
            }
            if depth >= MAX_DEPTH {
                warning(
                    index,
                    format!(
                        "Skipped a folder deeper than {MAX_DEPTH} levels: {}",
                        path.display()
                    ),
                );
            } else if *remaining > 0 {
                scan_directory(root, &path, depth + 1, remaining, index)?;
            } else {
                scan_limit_warning(index);
            }
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let Some(kind) = entry_kind(&path) else {
            continue;
        };
        let modified = match metadata.modified() {
            Ok(modified) => Some(modified),
            Err(error) => {
                warning(
                    index,
                    format!("Cannot read the date of {}: {error}", path.display()),
                );
                None
            }
        };
        let date = capture_date(root, &path).unwrap_or_else(|| {
            DateTime::<Local>::from(modified.unwrap_or(SystemTime::UNIX_EPOCH)).date_naive()
        });
        index.entries.push(LibraryEntry {
            name: child.file_name().to_string_lossy().into_owned(),
            path,
            date,
            kind,
            modified,
            created: metadata.created().ok(),
            bytes: metadata.len(),
        });
    }
    Ok(())
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        // Reparse points include directory junctions as well as symbolic links.
        metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(target_os = "windows"))]
    {
        metadata.file_type().is_symlink()
    }
}

fn is_hidden_directory(path: &Path, _metadata: &fs::Metadata) -> bool {
    let dot_name = path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'));
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        dot_name || _metadata.file_attributes() & 0x2 != 0
    }
    #[cfg(not(target_os = "windows"))]
    {
        dot_name
    }
}

fn entry_kind(path: &Path) -> Option<EntryKind> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tif" | "tiff" | "gif" => Some(EntryKind::Image),
        "myimage" => Some(EntryKind::Project),
        _ => None,
    }
}

fn capture_date(root: &Path, path: &Path) -> Option<NaiveDate> {
    let mut components = path.strip_prefix(root).ok()?.components();
    fn number(component: Component<'_>, length: usize) -> Option<u32> {
        let value = component.as_os_str().to_str()?;
        (value.len() == length && value.bytes().all(|b| b.is_ascii_digit()))
            .then(|| value.parse().ok())
            .flatten()
    }
    let year = number(components.next()?, 4)?;
    let month = number(components.next()?, 2)?;
    let day = number(components.next()?, 2)?;
    // The three components must be directories, not the filename itself.
    components.next()?;
    NaiveDate::from_ymd_opt(year as i32, month, day)
}

/// Decode a single visible entry. Call from a bounded worker, never the UI thread.
/// An annotated project is rendered at up to four times tile resolution instead
/// of allocating another full-size image and annotation overlay for every tile.
pub fn thumbnail(entry: &LibraryEntry) -> Result<Thumbnail, String> {
    match entry.kind {
        EntryKind::Image => {
            let image = decode_image(&entry.path)?;
            let original_size = [image.width(), image.height()];
            Ok(Thumbnail {
                image: fit_image(&image, THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT),
                original_size,
            })
        }
        EntryKind::Project => {
            let document = document::load_project(&entry.path)?;
            Ok(Thumbnail {
                image: render::thumbnail(&document, THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT)?,
                original_size: document.output_size(),
            })
        }
    }
}

fn fit_image(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    if image.width() <= width && image.height() <= height {
        image.clone()
    } else {
        let scale = (f64::from(width) / f64::from(image.width()))
            .min(f64::from(height) / f64::from(image.height()));
        let width = (f64::from(image.width()) * scale).round().max(1.0) as u32;
        let height = (f64::from(image.height()) * scale).round().max(1.0) as u32;
        image::imageops::thumbnail(image, width, height)
    }
}

fn decode_image(path: &Path) -> Result<RgbaImage, String> {
    let file =
        fs::File::open(path).map_err(|error| format!("Cannot open {}: {error}", path.display()))?;
    let bytes = file
        .metadata()
        .map_err(|error| format!("Cannot inspect {}: {error}", path.display()))?
        .len();
    if bytes > MAX_DECODE_BYTES {
        return Err("This image exceeds the 512 MiB thumbnail size limit.".into());
    }
    let mut reader = ImageReader::new(std::io::BufReader::new(file))
        .with_guessed_format()
        .map_err(|error| format!("Cannot read image format: {error}"))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let decoder = reader
        .into_decoder()
        .map_err(|error| format!("Cannot read thumbnail: {error}"))?;
    let (width, height) = decoder.dimensions();
    if width == 0
        || height == 0
        || width > MAX_IMAGE_SIDE
        || height > MAX_IMAGE_SIDE
        || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS
        || decoder.total_bytes() > MAX_DECODE_BYTES
    {
        return Err("This image exceeds the supported thumbnail dimensions.".into());
    }
    image::DynamicImage::from_decoder(decoder)
        .map(image::DynamicImage::into_rgba8)
        .map_err(|error| format!("Cannot decode thumbnail: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Annotation, AnnotationKind, Document, Point};
    use image::Rgba;
    use std::sync::Arc;

    fn write_file(root: &Path, relative: &str) -> PathBuf {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"only metadata is needed for indexing").unwrap();
        path
    }

    fn set_modified(path: &Path, seconds: u64) {
        let file = fs::File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(seconds))
            .unwrap();
    }

    #[test]
    fn capture_folder_date_wins_over_file_modification_date() {
        let directory = tempfile::tempdir().unwrap();
        let path = write_file(directory.path(), "2024/02/29/edits/old.myimage");
        set_modified(&path, 1_750_000_000);
        let index = scan(directory.path()).unwrap();
        assert!(index.warnings.is_empty());
        assert_eq!(
            index.entries[0].date,
            NaiveDate::from_ymd_opt(2024, 2, 29).unwrap()
        );
        assert_eq!(index.entries[0].kind, EntryKind::Project);
        assert_eq!(index.entries[0].name, "old.myimage");
    }

    #[test]
    fn other_paths_and_invalid_capture_dates_use_local_modification_date() {
        let directory = tempfile::tempdir().unwrap();
        for name in [
            "imported/photo.png",
            "2023/02/29/bad-date.jpg",
            "2024/2/03/non-padded.bmp",
            "archive/2024/01/02/nested.gif",
        ] {
            let path = write_file(directory.path(), name);
            set_modified(&path, 1_750_000_000);
        }
        let expected = DateTime::<Local>::from(
            SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_750_000_000),
        )
        .date_naive();
        let index = scan(directory.path()).unwrap();
        assert_eq!(index.entries.len(), 4);
        assert!(index.entries.iter().all(|entry| entry.date == expected));
    }

    #[test]
    fn scan_filters_hidden_folders_and_orders_dates_then_modified_then_path() {
        let directory = tempfile::tempdir().unwrap();
        for (name, seconds) in [
            ("2025/10/09/a.PNG", 100),
            ("2025/10/09/b.JPEG", 200),
            ("2025/10/09/c.MyImage", 200),
            ("2025/10/10/older.webp", 1),
            (".hidden/secret.png", 500),
            ("2025/10/10/.internal/secret.tif", 600),
            ("2025/10/10/notes.txt", 700),
            ("2025/10/10/partial.png.tmp", 800),
        ] {
            let path = write_file(directory.path(), name);
            set_modified(&path, seconds);
        }
        let index = scan(directory.path()).unwrap();
        assert_eq!(
            index
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["older.webp", "b.JPEG", "c.MyImage", "a.PNG"]
        );
        assert_eq!(index.entries, scan(directory.path()).unwrap().entries);
    }

    #[test]
    fn missing_library_is_empty_but_a_file_is_an_error() {
        let directory = tempfile::tempdir().unwrap();
        assert!(
            scan(&directory.path().join("not-yet-created"))
                .unwrap()
                .entries
                .is_empty()
        );
        let file = write_file(directory.path(), "file.png");
        assert!(scan(&file).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn scan_does_not_follow_symlink_files_folders_or_cycles() {
        use std::os::unix::fs::symlink;
        let directory = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let file = write_file(external.path(), "external.png");
        symlink(external.path(), directory.path().join("outside")).unwrap();
        symlink(&file, directory.path().join("linked.png")).unwrap();
        symlink(directory.path(), directory.path().join("cycle")).unwrap();
        write_file(directory.path(), "2026/01/02/actual.png");
        let index = scan(directory.path()).unwrap();
        assert_eq!(index.entries.len(), 1);
        assert_eq!(index.entries[0].name, "actual.png");
        assert!(scan(&directory.path().join("outside")).is_err());
    }

    #[test]
    fn traversal_depth_and_scan_budget_report_partial_results() {
        let directory = tempfile::tempdir().unwrap();
        write_file(directory.path(), "visible.png");
        let deep = std::iter::repeat_n("sub", MAX_DEPTH + 2)
            .collect::<Vec<_>>()
            .join("/");
        write_file(directory.path(), &format!("{deep}/too-deep.png"));
        let index = scan(directory.path()).unwrap();
        assert_eq!(index.entries.len(), 1);
        assert!(
            index
                .warnings
                .iter()
                .any(|message| message.contains("deeper"))
        );
        let mut partial = LibraryIndex::default();
        scan_directory(directory.path(), directory.path(), 0, &mut 1, &mut partial).unwrap();
        assert!(
            partial
                .warnings
                .iter()
                .any(|message| message.contains("stopped"))
        );
    }

    #[test]
    fn exhausted_budget_reports_skipped_child_even_at_exact_directory_boundary() {
        let directory = tempfile::tempdir().unwrap();
        write_file(directory.path(), "sub/unvisited.png");
        let mut index = LibraryIndex::default();
        scan_directory(directory.path(), directory.path(), 0, &mut 1, &mut index).unwrap();
        assert!(index.entries.is_empty());
        assert!(
            index
                .warnings
                .iter()
                .any(|message| message.contains("stopped"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_child_folder_reports_warning_and_preserves_other_entries() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        write_file(directory.path(), "visible.png");
        write_file(directory.path(), "blocked/inaccessible.png");
        let blocked = directory.path().join("blocked");
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o0)).unwrap();
        let blocked_is_readable = fs::read_dir(&blocked).is_ok();
        let result = scan(directory.path());
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700)).unwrap();
        let index = result.unwrap();
        // Root users can still read mode-000 folders; ordinary desktop users cannot.
        if !blocked_is_readable {
            assert_eq!(index.entries.len(), 1);
            assert!(
                index
                    .warnings
                    .iter()
                    .any(|message| message.contains("blocked"))
            );
        }
    }

    #[test]
    fn image_thumbnail_keeps_aspect_alpha_and_original_dimensions() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("transparent.png");
        let image = RgbaImage::from_pixel(1200, 600, Rgba([60, 120, 200, 80]));
        image.save(&path).unwrap();
        let entry = scan(directory.path()).unwrap().entries.remove(0);
        let thumbnail = thumbnail(&entry).unwrap();
        assert_eq!(thumbnail.original_size, [1200, 600]);
        assert_eq!(thumbnail.image.dimensions(), (384, 192));
        assert_eq!(
            thumbnail.image.get_pixel(100, 100),
            &Rgba([60, 120, 200, 80])
        );
    }

    #[test]
    fn project_thumbnail_contains_annotations_and_leaves_project_editable() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("annotated.myimage");
        let document = Document {
            geometry: Default::default(),
            image: Arc::new(RgbaImage::from_pixel(2400, 1600, Rgba([0, 0, 0, 0]))),
            annotations: vec![Annotation {
                kind: AnnotationKind::Rectangle,
                start: Point { x: 600.0, y: 400.0 },
                end: Point {
                    x: 1800.0,
                    y: 1200.0,
                },
                color: [240, 30, 20, 255],
                stroke_width: 8.0,
                fill: true,
                text: String::new(),
                font_size: 24.0,
                effect_size: 16,
                step_number: 1,
                tail: None,
                text_color: [255; 4],
                group_id: None,
                layer: Default::default(),
                group_layer: Default::default(),
                area_source: None,
                area_mask: Vec::new(),
                stamp: None,
                fill_region: None,
                text_style: Default::default(),
                callout_style: Default::default(),
                pixel_mask: None,
                arrow: Default::default(),
                studio: Default::default(),
                points: Vec::new(),
            }],
        };
        document::save_project(&document, &path).unwrap();
        let before = fs::read(&path).unwrap();
        let entry = scan(directory.path()).unwrap().entries.remove(0);
        let preview = thumbnail(&entry).unwrap();
        assert_eq!(preview.original_size, [2400, 1600]);
        assert_eq!(preview.image.dimensions(), (384, 256));
        assert_eq!(preview.image.get_pixel(192, 128), &Rgba([240, 30, 20, 255]));
        assert_eq!(preview.image.get_pixel(0, 0)[3], 0);
        assert_eq!(fs::read(&path).unwrap(), before);
        let reopened = document::load_project(&path).unwrap();
        assert_eq!(reopened.annotations, document.annotations);
        assert_eq!(reopened.image, document.image);
    }

    #[test]
    fn project_thumbnail_uses_output_geometry_and_leaves_original_objects_intact() {
        use crate::geometry::{CropRect, ImageGeometry};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("transformed.myimage");
        let document = Document {
            image: Arc::new(RgbaImage::from_pixel(1600, 1200, Rgba([20, 80, 230, 128]))),
            annotations: vec![Annotation {
                kind: AnnotationKind::Rectangle,
                start: Point { x: 400.0, y: 200.0 },
                end: Point { x: 800.0, y: 600.0 },
                color: [240, 30, 20, 255],
                stroke_width: 8.0,
                fill: true,
                text: String::new(),
                font_size: 24.0,
                effect_size: 16,
                step_number: 1,
                tail: None,
                text_color: [255; 4],
                group_id: None,
                layer: Default::default(),
                group_layer: Default::default(),
                area_source: None,
                area_mask: Vec::new(),
                stamp: None,
                fill_region: None,
                text_style: Default::default(),
                callout_style: Default::default(),
                pixel_mask: None,
                arrow: Default::default(),
                studio: Default::default(),
                points: Vec::new(),
            }],
            geometry: ImageGeometry {
                effects: Default::default(),
                viewport: None,
                flip_horizontal: false,
                flip_vertical: false,
                canvas: None,
                cutouts: [None; 16],
                crop: Some(CropRect {
                    x: 400,
                    y: 200,
                    width: 800,
                    height: 800,
                }),
                quarter_turns: 1,
                resize: Some([600, 300]),
            },
        };
        document::save_project(&document, &path).unwrap();
        let before = fs::read(&path).unwrap();
        let entry = scan(directory.path()).unwrap().entries.remove(0);
        let preview = thumbnail(&entry).unwrap();
        assert_eq!(preview.original_size, [600, 300]);
        assert_eq!(preview.image.dimensions(), (384, 192));
        assert_eq!(preview.image.get_pixel(288, 48), &Rgba([240, 30, 20, 255]));
        assert_eq!(preview.image.get_pixel(96, 144), &Rgba([20, 80, 230, 128]));
        assert_eq!(fs::read(&path).unwrap(), before);
        let reopened = document::load_project(&path).unwrap();
        assert_eq!(reopened.image, document.image);
        assert_eq!(reopened.edit_state(), document.edit_state());
    }

    #[test]
    fn project_thumbnail_displays_privacy_filters_while_retaining_adjustable_objects() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("private.myimage");
        let doc = Document {
            image: std::sync::Arc::new(RgbaImage::from_fn(96, 64, |x, y| {
                let value = if (x + y) % 2 == 0 { 0 } else { 255 };
                Rgba([value, value, value, 255])
            })),
            annotations: vec![Annotation {
                kind: AnnotationKind::Pixelate,
                start: Point { x: 0.0, y: 0.0 },
                end: Point { x: 96.0, y: 64.0 },
                color: [0; 4],
                stroke_width: 2.0,
                fill: false,
                text: String::new(),
                font_size: 24.0,
                effect_size: 16,
                step_number: 1,
                tail: None,
                text_color: [255; 4],
                group_id: None,
                layer: Default::default(),
                group_layer: Default::default(),
                area_source: None,
                area_mask: Vec::new(),
                stamp: None,
                fill_region: None,
                text_style: Default::default(),
                callout_style: Default::default(),
                pixel_mask: None,
                arrow: Default::default(),
                studio: Default::default(),
                points: Vec::new(),
            }],
            geometry: Default::default(),
        };
        document::save_project(&doc, &path).unwrap();
        let entry = scan(directory.path()).unwrap().entries.remove(0);
        let preview = thumbnail(&entry).unwrap();
        assert_eq!(preview.original_size, [96, 64]);
        assert_eq!(preview.image, render::flatten(&doc).unwrap());
        assert!(
            preview
                .image
                .pixels()
                .all(|pixel| (127..=128).contains(&pixel[0]))
        );
        let mut reopened = document::load_project(&path).unwrap();
        assert_eq!(reopened.image, doc.image);
        assert_eq!(reopened.annotations, doc.annotations);
        reopened.annotations.clear();
        assert_eq!(render::flatten(&reopened).unwrap(), *doc.image);
    }

    #[test]
    fn corrupt_and_oversized_files_are_indexed_but_fail_only_their_thumbnail() {
        let directory = tempfile::tempdir().unwrap();
        write_file(directory.path(), "corrupt.png");
        write_file(directory.path(), "corrupt.myimage");
        let oversized = fs::File::create(directory.path().join("oversized.bmp")).unwrap();
        oversized.set_len(MAX_DECODE_BYTES + 1).unwrap();
        let index = scan(directory.path()).unwrap();
        assert_eq!(index.entries.len(), 3);
        assert!(index.warnings.is_empty());
        for entry in index.entries {
            assert!(thumbnail(&entry).is_err(), "{}", entry.name);
        }
    }

    #[test]
    fn oversized_dimensions_are_rejected_before_pixel_decoding() {
        let directory = tempfile::tempdir().unwrap();
        RgbaImage::new(MAX_IMAGE_SIDE + 1, 1)
            .save(directory.path().join("too-wide.png"))
            .unwrap();
        let entry = scan(directory.path()).unwrap().entries.remove(0);
        assert!(thumbnail(&entry).is_err());
    }
}
