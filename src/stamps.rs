//! Portable stamp payloads. Imported images are embedded, never linked to a local path.
use base64::{Engine, engine::general_purpose::STANDARD};
use eframe::egui;
use image::{ImageDecoder, ImageFormat, ImageReader, RgbaImage};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::VecDeque,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

pub const CATEGORIES: &[&str] = &[
    "Symbols",
    "Stickers",
    "Activity",
    "Business",
    "Smileys and People",
    "Signatures",
];
const MAX_PNG_BYTES: usize = 1024 * 1024;
const MAX_RASTER_BYTES: usize = 16 * 1024 * 1024;
const MAX_LIBRARY_BYTES: u64 = 16 * 1024 * 1024;
const CACHE_BYTES: usize = 32 * 1024 * 1024;
pub struct Builtin {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub png: &'static [u8],
}
include!("stamp_catalog.rs");

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum StampSource {
    Builtin {
        id: String,
    },
    Image {
        name: String,
        png: Arc<String>,
    },
    /// Lossless full-resolution clipboard artwork, independent of the original canvas.
    Raster {
        name: String,
        png: Arc<String>,
    },
    Svg {
        name: String,
        svg: Arc<String>,
        #[serde(default)]
        prompt: String,
    },
}
impl StampSource {
    pub fn name(&self) -> &str {
        match self {
            Self::Builtin { id } => BUILTINS
                .iter()
                .find(|s| s.id == id)
                .map_or("Stamp", |s| s.name),
            Self::Image { name, .. } | Self::Raster { name, .. } | Self::Svg { name, .. } => name,
        }
    }
    pub fn stored_bytes(&self) -> usize {
        match self {
            Self::Builtin { id } => id.len(),
            Self::Image { name, png } | Self::Raster { name, png } => name.len() + png.len(),
            Self::Svg { name, svg, prompt } => name.len() + svg.len() + prompt.len(),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Svg { name, svg, prompt } => {
                if name.trim().is_empty()
                    || name.len() > 256
                    || name.chars().any(char::is_control)
                    || prompt.len() > 8000
                {
                    return Err("Invalid SVG stamp name or prompt.".into());
                }
                crate::svg_stamp::tree(svg)?;
            }
            Self::Builtin { id } => {
                if !BUILTINS.iter().any(|s| s.id == id) {
                    return Err("Unknown built-in stamp.".into());
                }
            }
            Self::Image { name, png } | Self::Raster { name, png } => {
                if name.is_empty()
                    || name.len() > 256
                    || name.chars().any(char::is_control)
                    || png.len() > self.png_limit().div_ceil(3) * 4
                {
                    return Err("Invalid signature name or image size.".into());
                }
                raster(self)?;
            }
        }
        Ok(())
    }
    fn png_limit(&self) -> usize {
        if matches!(self, Self::Raster { .. }) {
            MAX_RASTER_BYTES
        } else {
            MAX_PNG_BYTES
        }
    }
    pub fn from_pixels(image: &RgbaImage) -> Result<Self, String> {
        crate::document::validate_dimensions(image.width(), image.height())?;
        if u64::from(image.width()) * u64::from(image.height()) > 64_000_000 {
            return Err("Clipboard images are limited to 64 megapixels.".into());
        }
        let mut png = Cursor::new(Vec::new());
        image
            .write_to(&mut png, ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        if png.get_ref().len() > MAX_RASTER_BYTES {
            return Err(
                "This pasted image exceeds the 16 MB lossless image limit. Copy a smaller area."
                    .into(),
            );
        }
        Ok(Self::Raster {
            name: "Pasted image".into(),
            png: Arc::new(STANDARD.encode(png.into_inner())),
        })
    }
}
#[derive(Default)]
struct RasterCache {
    entries: VecDeque<(StampSource, Arc<RgbaImage>)>,
    bytes: usize,
}
thread_local! { static RASTERS: RefCell<RasterCache> = RefCell::new(RasterCache::default()); }
pub fn raster(source: &StampSource) -> Result<Arc<RgbaImage>, String> {
    RASTERS.with_borrow_mut(|cache| {
        if let Some(index) = cache.entries.iter().position(|e| &e.0 == source) {
            let entry = cache.entries.remove(index).unwrap();
            let image = entry.1.clone();
            cache.entries.push_front(entry);
            return Ok(image);
        }
        let decoded;
        if let StampSource::Svg { svg, .. } = source {
            let image = Arc::new(crate::svg_stamp::raster(svg, 512)?);
            while cache.bytes + image.len() > CACHE_BYTES {
                if let Some((_, old)) = cache.entries.pop_back() {
                    cache.bytes -= old.len();
                } else {
                    break;
                }
            }
            cache.bytes += image.len();
            cache.entries.push_front((source.clone(), image.clone()));
            return Ok(image);
        }
        let bytes = match source {
            StampSource::Svg { .. } => unreachable!(),
            StampSource::Builtin { id } => {
                BUILTINS
                    .iter()
                    .find(|s| s.id == id)
                    .ok_or("Unknown stamp.")?
                    .png
            }
            StampSource::Image { png, .. } | StampSource::Raster { png, .. } => {
                if png.len() > source.png_limit().div_ceil(3) * 4 {
                    return Err("Signature is too large.".into());
                }
                decoded = STANDARD
                    .decode(png.as_bytes())
                    .map_err(|_| "Invalid signature data.")?;
                if decoded.len() > source.png_limit() {
                    return Err("Signature is too large.".into());
                }
                &decoded
            }
        };
        let (side, pixels) = if matches!(source, StampSource::Raster { .. }) {
            (32_768, 64_000_000)
        } else {
            (2048, 4_194_304)
        };
        let image = Arc::new(decode(bytes, Some(ImageFormat::Png), side, pixels)?);
        while cache.bytes + image.len() > CACHE_BYTES {
            if let Some((_, old)) = cache.entries.pop_back() {
                cache.bytes -= old.len();
            } else {
                break;
            }
        }
        cache.bytes += image.len();
        cache.entries.push_front((source.clone(), image.clone()));
        Ok(image)
    })
}
fn decode(
    bytes: &[u8],
    format: Option<ImageFormat>,
    max_side: u32,
    max_pixels: u64,
) -> Result<RgbaImage, String> {
    let mut reader = ImageReader::new(Cursor::new(bytes));
    if let Some(format) = format {
        reader.set_format(format);
    } else {
        reader = reader.with_guessed_format().map_err(|e| e.to_string())?;
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(max_side);
    limits.max_image_height = Some(max_side);
    limits.max_alloc = Some(max_pixels * 8);
    reader.limits(limits);
    let decoder = reader
        .into_decoder()
        .map_err(|e| format!("Cannot decode stamp: {e}"))?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || width as u64 * height as u64 > max_pixels {
        return Err("Stamp image dimensions exceed the supported limit.".into());
    }
    image::DynamicImage::from_decoder(decoder)
        .map(|i| i.to_rgba8())
        .map_err(|e| e.to_string())
}
pub fn import(path: &Path) -> Result<StampSource, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("Choose a signature image smaller than 32 MB.".into());
    }
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
    {
        let svg = String::from_utf8(bytes).map_err(|_| "SVG must be UTF-8 text.")?;
        let source = StampSource::Svg {
            name: path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("SVG stamp")
                .chars()
                .take(60)
                .collect(),
            svg: Arc::new(crate::svg_stamp::clean(&svg)),
            prompt: String::new(),
        };
        source.validate()?;
        return Ok(source);
    }
    let mut image = decode(&bytes, None, 8192, 16_777_216)?;
    if image.width().max(image.height()) > 2048 {
        image = image::DynamicImage::ImageRgba8(image)
            .resize(2048, 2048, image::imageops::FilterType::Lanczos3)
            .to_rgba8();
    }
    let mut png = Cursor::new(Vec::new());
    image
        .write_to(&mut png, ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    if png.get_ref().len() > MAX_PNG_BYTES {
        return Err("Signature is too detailed. Choose a smaller image (PNG up to 1 MB).".into());
    }
    let name: String = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Signature")
        .chars()
        .filter(|c| !c.is_control())
        .take(60)
        .collect();
    let source = StampSource::Image {
        name: if name.is_empty() {
            "Signature".into()
        } else {
            name
        },
        png: Arc::new(STANDARD.encode(png.into_inner())),
    };
    source.validate()?;
    Ok(source)
}
#[derive(Clone)]
struct TextureEntry {
    source: StampSource,
    texture: egui::TextureHandle,
    frame: u64,
    bytes: usize,
}
#[derive(Clone, Default)]
struct TextureCache {
    entries: VecDeque<TextureEntry>,
    bytes: usize,
}
pub fn texture(ctx: &egui::Context, source: &StampSource) -> Result<egui::TextureHandle, String> {
    let cache = ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Arc<std::sync::Mutex<TextureCache>>>(egui::Id::new(
            "stamp-textures",
        ))
        .clone()
    });
    let mut cache = cache
        .lock()
        .map_err(|_| "Stamp preview cache unavailable.")?;
    let frame = ctx.cumulative_frame_nr();
    if let Some(i) = cache.entries.iter().position(|e| &e.source == source) {
        let mut entry = cache.entries.remove(i).unwrap();
        entry.frame = frame;
        let texture = entry.texture.clone();
        cache.entries.push_front(entry);
        return Ok(texture);
    }
    let image = raster(source)?;
    let bytes = image.len();
    while cache.bytes + bytes > CACHE_BYTES {
        let Some(i) = cache.entries.iter().rposition(|e| e.frame != frame) else {
            return Err("Stamp preview memory limit reached.".into());
        };
        let old = cache.entries.remove(i).unwrap();
        cache.bytes -= old.bytes;
    }
    let texture = ctx.load_texture(
        "Stamp",
        egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            &image,
        ),
        egui::TextureOptions::LINEAR,
    );
    cache.bytes += bytes;
    cache.entries.push_front(TextureEntry {
        source: source.clone(),
        texture: texture.clone(),
        frame,
        bytes,
    });
    Ok(texture)
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Library {
    pub signatures: Vec<StampSource>,
    #[serde(default)]
    pub categories: Vec<CustomCategory>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomCategory {
    pub name: String,
    pub stamps: Vec<StampSource>,
}
pub fn load(path: &Path) -> Result<Library, String> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Library::default()),
        Err(e) => return Err(e.to_string()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_LIBRARY_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_LIBRARY_BYTES {
        return Err("Signature library is too large.".into());
    }
    let library: Library = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if library.signatures.len() > 64 {
        return Err("Signature library contains too many images.".into());
    }
    for entry in &library.signatures {
        entry.validate()?;
        if !matches!(entry, StampSource::Image { .. } | StampSource::Svg { .. }) {
            return Err("Invalid signature entry.".into());
        }
    }
    if library.categories.len() > 32 {
        return Err("The stamp library holds up to 32 custom categories.".into());
    }
    let mut names = std::collections::HashSet::new();
    for category in &library.categories {
        validate_category(&category.name)?;
        if !names.insert(category.name.to_lowercase()) || category.stamps.len() > 128 {
            return Err("Invalid or duplicate stamp category.".into());
        }
        for stamp in &category.stamps {
            stamp.validate()?;
        }
    }
    Ok(library)
}
pub fn add(path: &PathBuf, source: StampSource) -> Result<Library, String> {
    add_to(path, "Signatures", source)
}
fn validate_category(name: &str) -> Result<(), String> {
    if name.trim() != name
        || name.is_empty()
        || name.len() > 64
        || name.chars().any(char::is_control)
        || CATEGORIES.iter().any(|c| c.eq_ignore_ascii_case(name))
    {
        return Err("Choose a unique category name (up to 64 bytes).".into());
    }
    Ok(())
}
pub fn create_category(path: &PathBuf, name: &str) -> Result<Library, String> {
    validate_category(name)?;
    let mut library = load(path)?;
    if library.categories.len() >= 32
        || library
            .categories
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(name))
    {
        return Err("Category already exists or the 32-category limit was reached.".into());
    }
    library.categories.push(CustomCategory {
        name: name.into(),
        stamps: vec![],
    });
    save_library(path, &library)?;
    Ok(library)
}
pub fn add_to(path: &PathBuf, category: &str, source: StampSource) -> Result<Library, String> {
    put_in(path, category, None, source)
}
pub fn put_in(
    path: &PathBuf,
    category: &str,
    old: Option<&StampSource>,
    source: StampSource,
) -> Result<Library, String> {
    source.validate()?;
    let mut library = load(path)?;
    let entries = if category == "Signatures" {
        &mut library.signatures
    } else {
        &mut library
            .categories
            .iter_mut()
            .find(|c| c.name == category)
            .ok_or("Choose or create a custom category first.")?
            .stamps
    };
    if entries.contains(&source) {
        return Ok(library);
    }
    if let Some(index) = old.and_then(|old| entries.iter().position(|s| s == old)) {
        entries[index] = source;
        save_library(path, &library)?;
        return Ok(library);
    }
    if entries.len() >= if category == "Signatures" { 64 } else { 128 } {
        return Err("This stamp category is full.".into());
    }
    entries.push(source);
    save_library(path, &library)?;
    Ok(library)
}
fn save_library(path: &PathBuf, library: &Library) -> Result<(), String> {
    let bytes = serde_json::to_vec(&library).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_LIBRARY_BYTES {
        return Err("Signature library is full (16 MB).".into());
    }
    let parent = path.parent().ok_or("Invalid signature library path.")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
