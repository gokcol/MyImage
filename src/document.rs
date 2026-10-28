//! Editable documents keep the original pixels and annotations separately.
//!
//! Project files contain an eight-byte magic, a little-endian u32 version,
//! u32 JSON byte count, u64 PNG byte count, JSON metadata, then the original PNG.
//! The embedded image makes a project portable without sidecar files. Version 2
//! adds reversible geometry; version 1 projects load with identity geometry.
//! Version 3 adds adjustable blur and pixelation objects to the annotation stack.
//! Version 4 adds numbered steps, callouts, and highlighters as editable objects.
//! Version 5 adds freehand strokes as editable point sequences.
//! Version 6 adds persistent, flat annotation groups without changing layer order.
//! Version 7 adds layer names, visibility, and locks for annotations and groups.
//! Version 8 adds rounded rectangles and polygon shapes.
//! Version 9 adds reversible area moves, pixel deletion and 3D diagram shapes.
//! Version 10 adds embedded stamps/signatures and whole-object text formatting.
//! Version 14 adds focus effects, pixel-selection masks and reversible cut-out strips.

use crate::geometry::ImageGeometry;
use image::{ImageDecoder, ImageFormat, ImageReader, RgbaImage};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs::File,
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::Arc,
};

const MAGIC: &[u8; 8] = b"MYIMAGE\0";
const VERSION: u32 = 23;
const HEADER_BYTES: u64 = 24;
const MAX_PROJECT_BYTES: u64 = 512 * 1024 * 1024;
pub(crate) const MAX_METADATA_BYTES: usize = 48 * 1024 * 1024;
pub(crate) const MAX_IMAGE_SIDE: u32 = 32_768;
const MAX_IMAGE_PIXELS: u64 = 100_000_000;
pub(crate) const MAX_ANNOTATIONS: usize = 10_000;
const MAX_TEXT_BYTES: usize = 64 * 1024;
const MAX_TOTAL_TEXT_BYTES: usize = 2 * 1024 * 1024;
const MAX_COORDINATE: f32 = 1_000_000.0;
pub const MAX_FONT_SIZE: f32 = 512.0;
pub const MAX_STROKE_WIDTH: f32 = 512.0;
pub const MAX_EFFECT_SIZE: u32 = 128;
pub const MAX_STEP_NUMBER: u32 = 9_999;
pub const MAX_STROKE_POINTS: usize = 16_384;
pub const MAX_TOTAL_STROKE_POINTS: usize = 65_536;
const MAX_HISTORY_STATES: usize = 64;
const MAX_HISTORY_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnnotationKind {
    Text,
    Stamp,
    Fill,
    Arrow,
    Rectangle,
    Ellipse,
    RoundedRectangle,
    Triangle,
    Diamond,
    Pentagon,
    Hexagon,
    Star,
    Cross,
    Parallelogram,
    Cylinder,
    Prism,
    Cone,
    Sphere,
    HexagonalPrism,
    Torus,
    Tetrahedron,

    Magnify,
    Spotlight,
    Simplify,
    Blur,
    Pixelate,
    Step,
    Callout,
    Highlighter,
    Freehand,
    MoveArea,
    DeleteArea,
}

impl AnnotationKind {
    pub fn is_polygon_shape(self) -> bool {
        matches!(
            self,
            Self::RoundedRectangle
                | Self::Triangle
                | Self::Diamond
                | Self::Pentagon
                | Self::Hexagon
                | Self::Star
                | Self::Cross
                | Self::Parallelogram
                | Self::Cylinder
                | Self::Prism
                | Self::Cone
                | Self::Sphere
                | Self::HexagonalPrism
                | Self::Torus
                | Self::Tetrahedron
        )
    }
    pub fn is_3d(self) -> bool {
        matches!(
            self,
            Self::Cylinder
                | Self::Prism
                | Self::Cone
                | Self::Sphere
                | Self::HexagonalPrism
                | Self::Torus
                | Self::Tetrahedron
        )
    }
    pub fn is_shape(self) -> bool {
        self.is_polygon_shape() || matches!(self, Self::Rectangle | Self::Ellipse)
    }
    pub fn is_area_edit(self) -> bool {
        matches!(self, Self::MoveArea | Self::DeleteArea)
    }
    pub fn is_studio(self) -> bool {
        matches!(self, Self::Magnify | Self::Spotlight | Self::Simplify)
    }
    pub fn needs_composite(self) -> bool {
        self.is_effect() || self.is_studio() || self.is_area_edit() || self == Self::Fill
    }
    pub fn is_effect(self) -> bool {
        matches!(self, Self::Blur | Self::Pixelate)
    }
}

const fn default_effect_size() -> u32 {
    16
}

const fn default_step_number() -> u32 {
    1
}

const fn default_text_color() -> [u8; 4] {
    [255; 4]
}

pub const MAX_LAYER_NAME_BYTES: usize = 256;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayerSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_id: Option<u64>,
    pub name: String,
    pub hidden: bool,
    pub locked: bool,
}

impl LayerSettings {
    fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

/// Positions, stroke widths, and font sizes are measured in original image pixels.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub kind: AnnotationKind,
    pub start: Point,
    pub end: Point,
    pub color: [u8; 4],
    pub stroke_width: f32,
    pub fill: bool,
    pub text: String,
    pub font_size: f32,
    /// Blur radius or pixelation block side, measured in original image pixels.
    #[serde(default = "default_effect_size")]
    pub effect_size: u32,
    /// Displayed number of a step marker; independent of the object's layer order.
    #[serde(default = "default_step_number")]
    pub step_number: u32,
    /// Callout tip, in original image pixels; its bubble uses `start` and `end`.
    #[serde(default)]
    pub tail: Option<Point>,
    /// Foreground for step numbers and callout text; `color` controls their fill.
    #[serde(default = "default_text_color")]
    pub text_color: [u8; 4],
    /// Ordered freehand samples in original image pixels; one point draws a dot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<Point>,
    /// Nonzero group identity. Members retain their own geometry and stack positions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<u64>,
    #[serde(default, skip_serializing_if = "LayerSettings::is_default")]
    pub layer: LayerSettings,
    // Group settings travel with each member so rendering stays self-contained.
    // Validation requires identical settings across members of the same group.
    #[serde(default, skip_serializing_if = "LayerSettings::is_default")]
    pub group_layer: LayerSettings,
    /// Source pixels in the composition below an area move; destination uses start/end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area_source: Option<[Point; 2]>,
    /// Closed lasso in normalized region coordinates. Empty means rectangle.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub area_mask: Vec<Point>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stamp: Option<crate::stamps::StampSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_region: Option<crate::fill::FillRegion>,
    #[serde(
        default,
        skip_serializing_if = "crate::text_style::TextStyle::is_default"
    )]
    pub text_style: crate::text_style::TextStyle,
    #[serde(
        default,
        skip_serializing_if = "crate::callout_style::CalloutStyle::is_default"
    )]
    pub callout_style: crate::callout_style::CalloutStyle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pixel_mask: Option<crate::fill::FillRegion>,
    #[serde(default, skip_serializing_if = "crate::studio::Options::is_default")]
    pub studio: crate::studio::Options,
    #[serde(
        default,
        skip_serializing_if = "crate::arrow_style::ArrowStyle::is_default"
    )]
    pub arrow: crate::arrow_style::ArrowStyle,
}

impl Annotation {
    pub fn visible(&self) -> bool {
        !self.layer.hidden && !self.group_layer.hidden
    }
    pub fn locked(&self) -> bool {
        self.layer.locked || self.group_layer.locked
    }
}

#[derive(Clone, Debug)]
pub struct Document {
    pub image: Arc<RgbaImage>,
    pub annotations: Vec<Annotation>,
    pub geometry: ImageGeometry,
}

impl Document {
    pub fn output_size(&self) -> [u32; 2] {
        self.geometry
            .output_size([self.image.width(), self.image.height()])
    }

    pub fn edit_state(&self) -> EditState {
        EditState {
            annotations: self.annotations.clone(),
            geometry: self.geometry,
        }
    }

    fn replace_edit_state(&mut self, state: EditState) -> EditState {
        EditState {
            annotations: std::mem::replace(&mut self.annotations, state.annotations),
            geometry: std::mem::replace(&mut self.geometry, state.geometry),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditState {
    pub annotations: Vec<Annotation>,
    pub geometry: ImageGeometry,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    width: u32,
    height: u32,
    annotations: Vec<Annotation>,
    geometry: ImageGeometry,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyMetadata {
    width: u32,
    height: u32,
    annotations: Vec<Annotation>,
}

pub fn validate_document(document: &Document) -> Result<(), String> {
    validate_parts(
        document.image.width(),
        document.image.height(),
        &document.annotations,
    )?;
    document
        .geometry
        .validate([document.image.width(), document.image.height()])
}

pub(crate) fn validate_dimensions(width: u32, height: u32) -> Result<(), String> {
    if width == 0
        || height == 0
        || width > MAX_IMAGE_SIDE
        || height > MAX_IMAGE_SIDE
        || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS
    {
        return Err("The image dimensions exceed the supported project limits.".into());
    }
    Ok(())
}

fn validate_parts(width: u32, height: u32, annotations: &[Annotation]) -> Result<(), String> {
    validate_dimensions(width, height)?;
    validate_annotations(annotations)
}

pub(crate) fn validate_annotations(annotations: &[Annotation]) -> Result<(), String> {
    if annotations.len() > MAX_ANNOTATIONS {
        return Err("A project can contain at most 10,000 annotations.".into());
    }
    let mut object_ids = std::collections::BTreeSet::new();
    let mut group_settings = std::collections::BTreeMap::new();
    let mut text_bytes = 0;
    let mut stamp_bytes = 0;
    let mut raster_bytes = 0;
    let mut fill_runs = 0;
    let mut stroke_points = 0;
    for annotation in annotations {
        validate_annotation(annotation)?;
        if let Some(id) = annotation.layer.object_id
            && (id == 0 || !object_ids.insert(id))
        {
            return Err("Object identities must be unique and nonzero.".into());
        }
        if annotation.group_layer.object_id.is_some() {
            return Err("Object identities belong to individual layers.".into());
        }
        fill_runs += annotation.fill_region.as_ref().map_or(0, |r| r.runs.len())
            + annotation.pixel_mask.as_ref().map_or(0, |r| r.runs.len());
        if fill_runs > crate::fill::MAX_TOTAL_RUNS {
            return Err("The project contains too many detailed fill regions.".into());
        }
        if let Some(source @ crate::stamps::StampSource::Raster { .. }) = &annotation.stamp {
            raster_bytes += source.stored_bytes();
        } else {
            stamp_bytes += annotation.stamp.as_ref().map_or(0, |s| s.stored_bytes());
        }
        if raster_bytes > 32 * 1024 * 1024 {
            return Err("Pasted image data exceeds the 32 MB project limit.".into());
        }
        if stamp_bytes > 3 * 1024 * 1024 {
            return Err("Embedded stamp images exceed the 3 MB project limit. Use smaller signature images.".into());
        }
        if let Some(id) = annotation.group_id
            && let Some(previous) = group_settings.insert(id, &annotation.group_layer)
            && previous != &annotation.group_layer
        {
            return Err("Members of a group must have matching layer settings.".into());
        }
        text_bytes +=
            annotation.text.len() + annotation.layer.name.len() + annotation.group_layer.name.len();
        if text_bytes > MAX_TOTAL_TEXT_BYTES {
            return Err("The annotation text exceeds the supported project limits.".into());
        }
        stroke_points += annotation.points.len() + annotation.area_mask.len();
        if stroke_points > MAX_TOTAL_STROKE_POINTS {
            return Err(format!(
                "A project can contain at most {MAX_TOTAL_STROKE_POINTS} freehand points."
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_annotation(annotation: &Annotation) -> Result<(), String> {
    if let Some(path) = &annotation.text_style.path {
        if annotation.kind != AnnotationKind::Text {
            return Err("Text paths require a Text annotation.".into());
        }
        path.validate()?;
    }
    if annotation.kind != AnnotationKind::Callout && !annotation.callout_style.is_default() {
        return Err("Callout shapes require a callout object.".into());
    }
    annotation.studio.validate()?;
    annotation.arrow.validate()?;
    if annotation.kind != AnnotationKind::Arrow {
        let mut style = annotation.arrow.clone();
        if annotation.kind.is_shape() {
            style.pattern = crate::arrow_style::Pattern::Solid;
        }
        if !style.is_default() {
            return Err("Endpoint and curve styles require an arrow object; outline patterns require a shape.".into());
        }
    }
    if let Some(mask) = &annotation.pixel_mask {
        mask.validate()?;
        if !annotation.kind.needs_composite() {
            return Err("Pixel masks require a pixel operation.".into());
        }
    }
    if !annotation.kind.is_studio() && !annotation.studio.is_default() {
        return Err("Effect settings are attached to another tool.".into());
    }
    if (annotation.kind == AnnotationKind::Fill) != annotation.fill_region.is_some() {
        return Err("Fill mask is missing or attached to another tool.".into());
    }
    if let Some(region) = &annotation.fill_region {
        region.validate()?;
        if annotation.end.x <= annotation.start.x || annotation.end.y <= annotation.start.y {
            return Err("A fill requires positive region bounds.".into());
        }
    }
    if (annotation.kind == AnnotationKind::Stamp) != annotation.stamp.is_some() {
        return Err("Stamp artwork is missing or attached to a non-stamp object.".into());
    }
    if let Some(stamp) = &annotation.stamp {
        stamp.validate()?;
    }
    if !annotation.area_mask.is_empty()
        && (!annotation.kind.needs_composite()
            || !(3..=MAX_STROKE_POINTS).contains(&annotation.area_mask.len())
            || annotation.area_mask.iter().any(|p| {
                !p.x.is_finite()
                    || !p.y.is_finite()
                    || !(0.0..=1.0).contains(&p.x)
                    || !(0.0..=1.0).contains(&p.y)
            }))
    {
        return Err("An area lasso has invalid points.".into());
    }
    if annotation.kind == AnnotationKind::MoveArea {
        let Some([a, b]) = annotation.area_source else {
            return Err("An area move requires source bounds.".into());
        };
        if [a.x, a.y, b.x, b.y]
            .iter()
            .any(|v| !v.is_finite() || v.abs() > MAX_COORDINATE)
            || b.x <= a.x
            || b.y <= a.y
        {
            return Err("An area move has invalid source bounds.".into());
        }
    } else if annotation.area_source.is_some() {
        return Err("Only area moves can contain source bounds.".into());
    }
    if annotation.kind.is_area_edit()
        && (annotation.end.x <= annotation.start.x || annotation.end.y <= annotation.start.y)
    {
        return Err("An area edit requires positive destination bounds.".into());
    }
    if annotation.group_id == Some(0) {
        return Err("An annotation group must have a nonzero identity.".into());
    }
    for settings in [&annotation.layer, &annotation.group_layer] {
        if settings.name.len() > MAX_LAYER_NAME_BYTES || settings.name.chars().any(char::is_control)
        {
            return Err("Layer names must be single-line text of at most 256 bytes.".into());
        }
    }
    if annotation.group_id.is_none() && !annotation.group_layer.is_default() {
        return Err("An ungrouped annotation cannot contain group layer settings.".into());
    }
    let coordinates = [
        annotation.start.x,
        annotation.start.y,
        annotation.end.x,
        annotation.end.y,
    ];
    if coordinates
        .iter()
        .any(|value| !value.is_finite() || value.abs() > MAX_COORDINATE)
    {
        return Err("An annotation has invalid coordinates.".into());
    }
    if let Some(tail) = annotation.tail
        && [tail.x, tail.y]
            .iter()
            .any(|value| !value.is_finite() || value.abs() > MAX_COORDINATE)
    {
        return Err("An annotation has invalid tail coordinates.".into());
    }
    if annotation.kind == AnnotationKind::Callout && annotation.tail.is_none() {
        return Err("A callout must have a tail point.".into());
    }
    if annotation.kind == AnnotationKind::Freehand {
        if !(1..=MAX_STROKE_POINTS).contains(&annotation.points.len()) {
            return Err(format!(
                "A freehand stroke must contain between 1 and {MAX_STROKE_POINTS} points."
            ));
        }
        if annotation.points.iter().any(|point| {
            [point.x, point.y]
                .iter()
                .any(|value| !value.is_finite() || value.abs() > MAX_COORDINATE)
        }) {
            return Err("A freehand stroke has invalid point coordinates.".into());
        }
    } else if !annotation.points.is_empty() {
        return Err("Only freehand strokes can contain drawing points.".into());
    }
    if annotation.kind == AnnotationKind::Step
        && !(1..=MAX_STEP_NUMBER).contains(&annotation.step_number)
    {
        return Err(format!(
            "Step numbers must be between 1 and {MAX_STEP_NUMBER}."
        ));
    }
    if !annotation.stroke_width.is_finite()
        || !(0.1..=MAX_STROKE_WIDTH).contains(&annotation.stroke_width)
        || !annotation.font_size.is_finite()
        || !(1.0..=MAX_FONT_SIZE).contains(&annotation.font_size)
    {
        return Err("An annotation has an invalid stroke width or text size.".into());
    }
    if annotation.kind.is_effect() && !(2..=MAX_EFFECT_SIZE).contains(&annotation.effect_size) {
        return Err(format!(
            "Blur and pixelation sizes must be between 2 and {MAX_EFFECT_SIZE} pixels."
        ));
    }
    if annotation.text.len() > MAX_TEXT_BYTES {
        return Err("The annotation text exceeds the supported project limits.".into());
    }
    Ok(())
}

/// Atomically replaces a project only after its complete contents are flushed.
/// A failed validation, encoding, write, or publish leaves an existing file intact.
pub fn save_project(document: &Document, destination: &Path) -> Result<(), String> {
    let resolved = crate::connectors::resolved(document);
    let document = resolved.as_ref();
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    project_temporary(document, parent)?
        .persist(destination)
        .map_err(|e| format!("Cannot save project {}: {}", destination.display(), e.error))?;
    Ok(())
}

/// Publish a new project without ever replacing a same-named document.
pub fn save_new_project(
    document: &Document,
    directory: &Path,
    stem: &str,
) -> Result<std::path::PathBuf, String> {
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let stem: String = stem
        .chars()
        .filter(|c| !c.is_control() && !"/\\:*?\"<>|".contains(*c))
        .take(100)
        .collect();
    let stem = stem.trim().trim_matches('.');
    let stem = if stem.is_empty() {
        "Untitled image"
    } else {
        stem
    };
    let mut temporary = project_temporary(document, directory)?;
    for n in 0..10000 {
        let name = if n == 0 {
            stem.to_owned()
        } else {
            format!("{stem} ({n})")
        };
        let destination =
            crate::library_files::rename_target(&directory.join("image.myimage"), &name)
                .unwrap_or_else(|_| directory.join(format!("Image-{n}.myimage")));
        match temporary.persist_noclobber(&destination) {
            Ok(_) => return Ok(destination),
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => temporary = e.file,
            Err(e) => return Err(format!("Cannot create project: {}", e.error)),
        }
    }
    Err("Cannot allocate a unique project filename.".into())
}

fn project_temporary(
    document: &Document,
    parent: &Path,
) -> Result<tempfile::NamedTempFile, String> {
    validate_document(document)?;
    let metadata = Metadata {
        width: document.image.width(),
        height: document.image.height(),
        annotations: document.annotations.clone(),
        geometry: document.geometry,
    };
    let json = serde_json::to_vec(&metadata)
        .map_err(|error| format!("Cannot encode project annotations: {error}"))?;
    if json.len() > MAX_METADATA_BYTES {
        return Err("The project annotation data is too large.".into());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|error| {
        format!(
            "Cannot create a project file in {}: {error}",
            parent.display()
        )
    })?;
    {
        let mut writer = BufWriter::new(temporary.as_file_mut());
        write_contents(&mut writer, document, &json)
            .map_err(|error| format!("Cannot write project: {error}"))?;
    }
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("Cannot flush project to disk: {error}"))?;
    Ok(temporary)
}

fn write_contents(
    writer: &mut (impl Write + Seek),
    document: &Document,
    json: &[u8],
) -> Result<(), String> {
    writer.write_all(MAGIC).map_err(|error| error.to_string())?;
    writer
        .write_all(&VERSION.to_le_bytes())
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&(json.len() as u32).to_le_bytes())
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&0_u64.to_le_bytes())
        .map_err(|error| error.to_string())?;
    writer.write_all(json).map_err(|error| error.to_string())?;
    document
        .image
        .write_to(writer, ImageFormat::Png)
        .map_err(|error| format!("Cannot encode the original image: {error}"))?;
    let end = writer
        .stream_position()
        .map_err(|error| error.to_string())?;
    if end > MAX_PROJECT_BYTES {
        return Err("The project file exceeds the 512 MiB size limit.".into());
    }
    let png_bytes = end - HEADER_BYTES - json.len() as u64;
    writer
        .seek(SeekFrom::Start(16))
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&png_bytes.to_le_bytes())
        .map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())
}

pub fn load_project(path: &Path) -> Result<Document, String> {
    let file = File::open(path)
        .map_err(|error| format!("Cannot open project {}: {error}", path.display()))?;
    let file_bytes = file
        .metadata()
        .map_err(|error| format!("Cannot inspect project: {error}"))?
        .len();
    if !(HEADER_BYTES..=MAX_PROJECT_BYTES).contains(&file_bytes) {
        return Err("The project is truncated or exceeds the 512 MiB size limit.".into());
    }
    let mut reader = BufReader::new(file);
    let mut header = [0_u8; HEADER_BYTES as usize];
    reader
        .read_exact(&mut header)
        .map_err(|error| format!("Cannot read project header: {error}"))?;
    if &header[..8] != MAGIC {
        return Err("This file is not a MyImage project.".into());
    }
    let version = u32::from_le_bytes(header[8..12].try_into().unwrap());
    if !(1..=VERSION).contains(&version) {
        return Err(format!(
            "Project version {version} is not supported by this version of MyImage."
        ));
    }
    let json_bytes = u32::from_le_bytes(header[12..16].try_into().unwrap()) as usize;
    let png_bytes = u64::from_le_bytes(header[16..24].try_into().unwrap());
    if json_bytes > MAX_METADATA_BYTES
        || png_bytes == 0
        || HEADER_BYTES
            .checked_add(json_bytes as u64)
            .and_then(|bytes| bytes.checked_add(png_bytes))
            != Some(file_bytes)
    {
        return Err("The project contains invalid or truncated data lengths.".into());
    }
    let mut json = vec![0; json_bytes];
    reader
        .read_exact(&mut json)
        .map_err(|error| format!("Cannot read project annotations: {error}"))?;
    let metadata: Metadata = if version == 1 {
        let legacy: LegacyMetadata = serde_json::from_slice(&json)
            .map_err(|error| format!("Cannot read project annotations: {error}"))?;
        Metadata {
            width: legacy.width,
            height: legacy.height,
            annotations: legacy.annotations,
            geometry: ImageGeometry::default(),
        }
    } else {
        serde_json::from_slice(&json)
            .map_err(|error| format!("Cannot read project metadata: {error}"))?
    };
    if version < 23
        && metadata.annotations.iter().any(|a| {
            a.text_style.path.is_some()
                || a.layer.object_id.is_some()
                || a.arrow.connection.is_some()
                || a.arrow.elbow
        })
    {
        return Err("Text paths require MyImage project version 23.".into());
    }
    if version < 22
        && metadata
            .annotations
            .iter()
            .any(|a| !a.callout_style.is_default() || crate::studio::advanced(&a.studio))
    {
        return Err(
            "Callout shapes and advanced lenses require MyImage project version 22.".into(),
        );
    }
    if version < 21
        && metadata.annotations.iter().any(|a| {
            matches!(
                a.arrow.pattern,
                crate::arrow_style::Pattern::DashDot | crate::arrow_style::Pattern::LongDash
            ) || (a.kind.is_shape() && a.arrow.pattern != crate::arrow_style::Pattern::Solid)
        })
    {
        return Err("Expanded stroke styles require MyImage project version 21.".into());
    }
    if version < 20
        && (metadata.geometry.effects.edges != Default::default()
            || metadata.geometry.effects.styles != Default::default())
    {
        return Err("Expanded image styling requires MyImage project version 20.".into());
    }
    if version < 19 && !metadata.geometry.effects.is_default() {
        return Err("Image styling requires MyImage project version 19.".into());
    }
    if version < 18 && metadata.annotations.iter().any(|a| a.arrow.curve.is_some()) {
        return Err("Curved arrows require MyImage project version 18.".into());
    }
    if version < 17 && metadata.geometry.viewport.is_some() {
        return Err("Reversible canvas windows require MyImage project version 17.".into());
    }
    if version < 16 && metadata.annotations.iter().any(|a| !a.arrow.is_default()) {
        return Err("Arrow styles require MyImage project version 16.".into());
    }
    if version < 15
        && (metadata.geometry.canvas.is_some()
            || metadata.geometry.flip_horizontal
            || metadata.geometry.flip_vertical)
    {
        return Err("Expanded canvases and image flips require MyImage project version 15.".into());
    }
    if version < 3
        && metadata
            .annotations
            .iter()
            .any(|annotation| annotation.kind.is_effect())
    {
        return Err("Blur and pixelation objects require MyImage project version 3.".into());
    }
    if version < 4
        && metadata.annotations.iter().any(|annotation| {
            matches!(
                annotation.kind,
                AnnotationKind::Step | AnnotationKind::Callout | AnnotationKind::Highlighter
            )
        })
    {
        return Err(
            "Step markers, callouts, and highlighters require MyImage project version 4.".into(),
        );
    }
    if version < 5
        && metadata
            .annotations
            .iter()
            .any(|annotation| annotation.kind == AnnotationKind::Freehand)
    {
        return Err("Freehand strokes require MyImage project version 5.".into());
    }
    if version < 6 && metadata.annotations.iter().any(|a| a.group_id.is_some()) {
        return Err("Annotation groups require MyImage project version 6.".into());
    }
    if version < 7
        && metadata
            .annotations
            .iter()
            .any(|a| !a.layer.is_default() || !a.group_layer.is_default())
    {
        return Err("Layer settings require MyImage project version 7.".into());
    }
    if version < 14
        && (metadata.geometry.has_cutouts()
            || metadata
                .annotations
                .iter()
                .any(|a| a.kind.is_studio() || a.pixel_mask.is_some() || !a.studio.is_default()))
    {
        return Err(
            "Editor effects, magic-wand masks and cut outs require MyImage project version 14."
                .into(),
        );
    }
    if version < 13
        && metadata
            .annotations
            .iter()
            .any(|a| matches!(a.stamp, Some(crate::stamps::StampSource::Raster { .. })))
    {
        return Err("Pasted image layers require MyImage project version 13.".into());
    }
    if version < 12
        && metadata
            .annotations
            .iter()
            .any(|a| matches!(a.stamp, Some(crate::stamps::StampSource::Svg { .. })))
    {
        return Err("SVG stamps require MyImage project version 12.".into());
    }
    if version < 11
        && metadata
            .annotations
            .iter()
            .any(|a| a.fill_region.is_some() || a.kind == AnnotationKind::Fill)
    {
        return Err("Fill regions require MyImage project version 11.".into());
    }
    if version < 10
        && metadata
            .annotations
            .iter()
            .any(|a| a.kind == AnnotationKind::Stamp || !a.text_style.is_default())
    {
        return Err("Stamps and formatted text require MyImage project version 10.".into());
    }
    if version < 9
        && metadata.annotations.iter().any(|a| {
            a.kind.is_area_edit()
                || a.kind.is_3d()
                || a.area_source.is_some()
                || !a.area_mask.is_empty()
        })
    {
        return Err("Area edits and 3D shapes require MyImage project version 9.".into());
    }
    if version < 8
        && metadata
            .annotations
            .iter()
            .any(|a| a.kind.is_polygon_shape())
    {
        return Err("Additional shapes require MyImage project version 8.".into());
    }
    validate_parts(metadata.width, metadata.height, &metadata.annotations)?;
    metadata
        .geometry
        .validate([metadata.width, metadata.height])?;

    // Decode directly from the file, avoiding a second copy of the embedded PNG.
    // Limits apply before allocation, including PNG metadata and ancillary chunks.
    let mut png_reader = ImageReader::with_format(reader, ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    limits.max_alloc = Some(MAX_PROJECT_BYTES);
    png_reader.limits(limits);
    let decoder = png_reader
        .into_decoder()
        .map_err(|error| format!("Cannot read the project's original image: {error}"))?;
    let (width, height) = decoder.dimensions();
    validate_dimensions(width, height)?;
    if (width, height) != (metadata.width, metadata.height) {
        return Err("The project's original image does not match its metadata.".into());
    }
    if decoder.total_bytes() > MAX_PROJECT_BYTES {
        return Err("The project's original image is too large to decode.".into());
    }
    let image = image::DynamicImage::from_decoder(decoder)
        .map_err(|error| format!("Cannot decode the project's original image: {error}"))?
        .into_rgba8();
    Ok(Document {
        image: Arc::new(image),
        annotations: metadata.annotations,
        geometry: metadata.geometry,
    })
}

/// History stores editable geometry and objects; original pixel buffers are never copied.
pub struct EditHistory {
    undo: VecDeque<EditState>,
    redo: Vec<EditState>,
    saved: EditState,
    revision: u64,
}

impl EditHistory {
    pub fn new(initial: EditState) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            saved: initial,
            revision: 0,
        }
    }

    /// Call once per completed gesture or property edit, not once per paint frame.
    pub fn commit(&mut self, before: EditState, after: &Document) {
        if before.geometry == after.geometry && before.annotations == after.annotations {
            return;
        }
        self.revision = self.revision.wrapping_add(1);
        self.undo.push_back(before);
        self.redo.clear();
        self.trim();
    }

    pub fn undo(&mut self, document: &mut Document) -> bool {
        let Some(previous) = self.undo.pop_back() else {
            return false;
        };
        self.revision = self.revision.wrapping_add(1);
        self.redo.push(document.replace_edit_state(previous));
        self.trim();
        true
    }

    pub fn redo(&mut self, document: &mut Document) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.revision = self.revision.wrapping_add(1);
        self.undo.push_back(document.replace_edit_state(next));
        self.trim();
        true
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn mark_saved(&mut self, state: &EditState) {
        self.saved = state.clone();
    }

    pub fn is_dirty(&self, document: &Document) -> bool {
        self.saved.geometry != document.geometry || self.saved.annotations != document.annotations
    }

    fn trim(&mut self) {
        let mut bytes: usize = self.undo.iter().chain(&self.redo).map(state_bytes).sum();
        while self.undo.len() + self.redo.len() > MAX_HISTORY_STATES || bytes > MAX_HISTORY_BYTES {
            let removed = if !self.undo.is_empty() {
                self.undo.pop_front()
            } else if !self.redo.is_empty() {
                Some(self.redo.remove(0))
            } else {
                None
            };
            let Some(removed) = removed else { break };
            bytes = bytes.saturating_sub(state_bytes(&removed));
        }
    }
}

pub(crate) fn state_bytes(state: &EditState) -> usize {
    let annotations = &state.annotations;
    std::mem::size_of::<ImageGeometry>()
        + annotations.capacity() * std::mem::size_of::<Annotation>()
        + annotations
            .iter()
            .map(|annotation| {
                annotation.text.capacity()
                    + annotation.layer.name.capacity()
                    + annotation.group_layer.name.capacity()
                    + annotation.stamp.as_ref().map_or(0, |s| s.stored_bytes())
                    + annotation
                        .fill_region
                        .as_ref()
                        .map_or(0, |r| r.runs.capacity() * std::mem::size_of::<[u32; 3]>())
                    + annotation
                        .pixel_mask
                        .as_ref()
                        .map_or(0, |r| r.runs.capacity() * std::mem::size_of::<[u32; 3]>())
                    + (annotation.points.capacity() + annotation.area_mask.capacity())
                        * std::mem::size_of::<Point>()
            })
            .sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Cursor};

    fn sample_document() -> Document {
        let mut image = RgbaImage::from_pixel(6, 4, image::Rgba([1, 40, 220, 255]));
        image.put_pixel(0, 0, image::Rgba([50, 80, 120, 0]));
        image.put_pixel(5, 3, image::Rgba([210, 30, 80, 127]));
        Document {
            image: Arc::new(image),
            annotations: [
                AnnotationKind::Text,
                AnnotationKind::Arrow,
                AnnotationKind::Rectangle,
                AnnotationKind::Ellipse,
            ]
            .into_iter()
            .map(|kind| Annotation {
                kind,
                start: Point { x: 0.5, y: 1.25 },
                end: Point { x: 5.5, y: 3.5 },
                color: [245, 70, 10, 180],
                stroke_width: 2.5,
                fill: kind == AnnotationKind::Ellipse,
                text: if kind == AnnotationKind::Text {
                    "Editable text\nTürkçe ✓".into()
                } else {
                    String::new()
                },
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
            })
            .collect(),
            geometry: ImageGeometry::default(),
        }
    }

    #[test]
    fn portable_project_round_trip_preserves_original_pixels_and_editable_objects() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.myimage");
        let moved = directory.path().join("moved.myimage");
        let mut original = sample_document();
        original.geometry = ImageGeometry {
            effects: Default::default(),
            viewport: None,
            flip_horizontal: false,
            flip_vertical: false,
            canvas: None,
            cutouts: [None; 16],
            crop: Some(crate::geometry::CropRect {
                x: 1,
                y: 1,
                width: 4,
                height: 2,
            }),
            quarter_turns: 3,
            resize: Some([12, 21]),
        };
        save_project(&original, &first).unwrap();
        assert_eq!(
            u32::from_le_bytes(fs::read(&first).unwrap()[8..12].try_into().unwrap()),
            VERSION
        );
        fs::rename(first, &moved).unwrap();
        let mut reopened = load_project(&moved).unwrap();
        assert_eq!(*reopened.image, *original.image);
        assert_eq!(reopened.annotations, original.annotations);
        assert_eq!(reopened.geometry, original.geometry);
        assert_eq!(reopened.output_size(), [12, 21]);

        reopened.annotations[0].text = "Changed after reopening".into();
        reopened.annotations[1].end = Point { x: 2.0, y: 1.0 };
        reopened.annotations[2].color = [20, 200, 10, 255];
        reopened.annotations[3].fill = false;
        reopened.geometry = ImageGeometry::default();
        save_project(&reopened, &moved).unwrap();
        let twice = load_project(&moved).unwrap();
        assert_eq!(twice.annotations, reopened.annotations);
        assert_eq!(*twice.image, *original.image);
        assert_eq!(twice.geometry, ImageGeometry::default());
        assert_eq!(twice.output_size(), [6, 4]);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    fn write_metadata_fixture(path: &Path, version: u32, metadata: &serde_json::Value) {
        let mut encoded = Cursor::new(Vec::new());
        write_contents(
            &mut encoded,
            &sample_document(),
            &serde_json::to_vec(metadata).unwrap(),
        )
        .unwrap();
        let mut bytes = encoded.into_inner();
        bytes[8..12].copy_from_slice(&version.to_le_bytes());
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn version_one_projects_load_with_identity_geometry_and_save_as_current_version() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy.myimage");
        let original = sample_document();
        let mut metadata =
            serde_json::json!({ "width": 6, "height": 4, "annotations": original.annotations });
        for annotation in metadata["annotations"].as_array_mut().unwrap() {
            let fields = annotation.as_object_mut().unwrap();
            for name in ["effect_size", "step_number", "tail", "text_color"] {
                fields.remove(name);
            }
        }
        write_metadata_fixture(&path, 1, &metadata);
        let loaded = load_project(&path).unwrap();
        assert_eq!(loaded.geometry, ImageGeometry::default());
        assert_eq!(*loaded.image, *original.image);
        assert_eq!(loaded.annotations, original.annotations);
        save_project(&loaded, &path).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(
            u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            VERSION
        );
        assert_eq!(
            load_project(&path).unwrap().edit_state(),
            loaded.edit_state()
        );
    }

    #[test]
    fn version_two_requires_complete_valid_geometry_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("invalid.myimage");
        let base = serde_json::json!({ "width": 6, "height": 4, "annotations": [] });
        write_metadata_fixture(&path, 2, &base);
        assert!(load_project(&path).is_err());
        for geometry in [
            serde_json::json!(null),
            serde_json::json!({ "quarter_turns": 0 }),
            serde_json::json!({ "crop": null, "quarter_turns": 0 }),
            serde_json::json!({ "crop": null, "quarter_turns": 4, "resize": null }),
            serde_json::json!({ "crop": null, "quarter_turns": 0, "resize": [0, 1] }),
            serde_json::json!({ "crop": null, "quarter_turns": 0, "resize": [20_000, 20_000] }),
            serde_json::json!({ "crop": { "x": 5, "y": 0, "width": 2, "height": 1 }, "quarter_turns": 0, "resize": null }),
            serde_json::json!({ "crop": null, "quarter_turns": 0, "resize": null, "future_edit": true }),
        ] {
            let mut metadata = base.clone();
            metadata["geometry"] = geometry;
            write_metadata_fixture(&path, 2, &metadata);
            assert!(load_project(&path).is_err(), "accepted {metadata}");
        }
    }

    #[test]
    fn version_two_preserves_geometry_and_defaults_legacy_effect_sizes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy-geometry.myimage");
        let mut original = sample_document();
        original.geometry.quarter_turns = 3;
        original.geometry.resize = Some([9, 15]);
        let mut metadata = serde_json::json!({
            "width": 6, "height": 4, "annotations": original.annotations,
            "geometry": original.geometry,
        });
        for annotation in metadata["annotations"].as_array_mut().unwrap() {
            let fields = annotation.as_object_mut().unwrap();
            for name in ["effect_size", "step_number", "tail", "text_color"] {
                fields.remove(name);
            }
        }
        write_metadata_fixture(&path, 2, &metadata);
        let loaded = load_project(&path).unwrap();
        assert_eq!(loaded.edit_state(), original.edit_state());
        assert_eq!(*loaded.image, *original.image);
        save_project(&loaded, &path).unwrap();
        assert_eq!(
            u32::from_le_bytes(fs::read(&path).unwrap()[8..12].try_into().unwrap()),
            VERSION
        );
    }

    #[test]
    fn privacy_objects_round_trip_with_editable_strength_regions_and_history() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("privacy.myimage");
        let mut original = sample_document();
        let mut blur = original.annotations[2].clone();
        blur.kind = AnnotationKind::Blur;
        blur.effect_size = 32;
        let mut pixelate = blur.clone();
        pixelate.kind = AnnotationKind::Pixelate;
        pixelate.effect_size = 7;
        original.annotations.extend([blur, pixelate]);
        original.geometry.quarter_turns = 1;
        save_project(&original, &path).unwrap();
        let mut loaded = load_project(&path).unwrap();
        assert_eq!(loaded.edit_state(), original.edit_state());
        assert_eq!(*loaded.image, *original.image);
        let initial = loaded.edit_state();
        let mut history = EditHistory::new(initial.clone());
        loaded.annotations[4].effect_size = 80;
        loaded.annotations[4].end.x = 4.0;
        loaded.annotations[5].start.y = -2.5;
        history.commit(initial.clone(), &loaded);
        let changed = loaded.edit_state();
        assert!(history.is_dirty(&loaded));
        assert!(history.undo(&mut loaded));
        assert_eq!(loaded.edit_state(), initial);
        assert!(history.redo(&mut loaded));
        assert_eq!(loaded.edit_state(), changed);
        save_project(&loaded, &path).unwrap();
        let reopened = load_project(&path).unwrap();
        assert_eq!(reopened.edit_state(), changed);
        assert_eq!(*reopened.image, *original.image);
    }

    #[test]
    fn privacy_objects_require_version_three_and_valid_sizes_before_save_or_load() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("privacy.myimage");
        let mut document = sample_document();
        save_project(&document, &path).unwrap();
        let original_file = fs::read(&path).unwrap();
        for kind in [AnnotationKind::Blur, AnnotationKind::Pixelate] {
            document.annotations[0].kind = kind;
            for size in [0, 1, MAX_EFFECT_SIZE + 1, u32::MAX] {
                document.annotations[0].effect_size = size;
                assert!(save_project(&document, &path).is_err());
                assert_eq!(fs::read(&path).unwrap(), original_file);
            }
            for version in 1..=VERSION {
                for size in [0, 1, 16, MAX_EFFECT_SIZE + 1] {
                    document.annotations[0].effect_size = size;
                    let mut metadata = serde_json::json!({
                        "width": 6, "height": 4, "annotations": document.annotations,
                    });
                    if version != 1 {
                        metadata["geometry"] = serde_json::to_value(document.geometry).unwrap();
                    }
                    write_metadata_fixture(&path, version, &metadata);
                    assert_eq!(load_project(&path).is_ok(), version >= 3 && size == 16);
                }
            }
            fs::write(&path, &original_file).unwrap();
        }
    }

    fn markup_document() -> Document {
        let mut document = sample_document();
        let mut step = document.annotations[2].clone();
        step.kind = AnnotationKind::Step;
        step.step_number = 42;
        step.text_color = [240, 248, 255, 230];
        let mut callout = step.clone();
        callout.kind = AnnotationKind::Callout;
        callout.text = "Editable callout\nTürkçe ✓".into();
        callout.tail = Some(Point { x: -4.5, y: 8.0 });
        let mut highlighter = document.annotations[2].clone();
        highlighter.kind = AnnotationKind::Highlighter;
        highlighter.color = [255, 225, 30, 85];
        document.annotations.extend([step, callout, highlighter]);
        document
    }

    #[test]
    fn markup_round_trip_and_history_preserve_independent_properties_and_original_pixels() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("markup.myimage");
        let original = markup_document();
        save_project(&original, &path).unwrap();
        assert_eq!(
            u32::from_le_bytes(fs::read(&path).unwrap()[8..12].try_into().unwrap()),
            VERSION
        );
        let mut loaded = load_project(&path).unwrap();
        assert_eq!(loaded.edit_state(), original.edit_state());
        assert_eq!(*loaded.image, *original.image);
        let pixels = loaded.image.clone();
        let before = loaded.edit_state();
        let mut history = EditHistory::new(before.clone());
        loaded.annotations[4].step_number = MAX_STEP_NUMBER;
        loaded.annotations[4].start.x = -20.0;
        loaded.annotations[4].text_color = [40, 50, 60, 255];
        loaded.annotations[5].tail = Some(Point { x: 24.25, y: 3.5 });
        loaded.annotations[5].text = "Changed after reopening".into();
        loaded.annotations[5].font_size = 36.0;
        loaded.annotations[6].color[3] = 135;
        loaded.annotations[6].end.y = 3.0;
        loaded.annotations.swap(4, 6);
        loaded.geometry.quarter_turns = 1;
        loaded.geometry.resize = Some([24, 36]);
        history.commit(before.clone(), &loaded);
        let after = loaded.edit_state();
        assert!(history.is_dirty(&loaded));
        assert!(history.undo(&mut loaded));
        assert_eq!(loaded.edit_state(), before);
        assert!(!history.is_dirty(&loaded));
        assert!(history.redo(&mut loaded));
        assert_eq!(loaded.edit_state(), after);
        assert!(Arc::ptr_eq(&loaded.image, &pixels));
        save_project(&loaded, &path).unwrap();
        history.mark_saved(&loaded.edit_state());
        assert!(!history.is_dirty(&loaded));
        let reopened = load_project(&path).unwrap();
        assert_eq!(reopened.edit_state(), after);
        assert_eq!(*reopened.image, *original.image);
        assert_eq!(reopened.annotations[6].step_number, MAX_STEP_NUMBER);
    }

    #[test]
    fn version_three_privacy_projects_default_new_fields_without_changing_effects() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy-privacy.myimage");
        let mut original = sample_document();
        original.annotations[1].kind = AnnotationKind::Blur;
        original.annotations[1].effect_size = 31;
        original.annotations[2].kind = AnnotationKind::Pixelate;
        original.annotations[2].effect_size = 9;
        original.geometry.quarter_turns = 2;
        let mut metadata = serde_json::json!({
            "width": 6, "height": 4, "annotations": original.annotations,
            "geometry": original.geometry,
        });
        for annotation in metadata["annotations"].as_array_mut().unwrap() {
            let fields = annotation.as_object_mut().unwrap();
            for name in ["step_number", "tail", "text_color"] {
                fields.remove(name);
            }
        }
        write_metadata_fixture(&path, 3, &metadata);
        let loaded = load_project(&path).unwrap();
        assert_eq!(loaded.edit_state(), original.edit_state());
        assert_eq!(*loaded.image, *original.image);
        save_project(&loaded, &path).unwrap();
        assert_eq!(
            u32::from_le_bytes(fs::read(&path).unwrap()[8..12].try_into().unwrap()),
            VERSION
        );
    }

    #[test]
    fn canvas_windows_require_version_seventeen_and_valid_retained_bounds() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("canvas-window.myimage");
        let d = sample_document();
        let mut metadata = serde_json::json!({
            "width": 6, "height": 4, "annotations": d.annotations,
            "geometry": ImageGeometry::default(),
        });
        metadata["geometry"]["canvas"] =
            serde_json::json!({"x": 0, "y": 0, "width": 6, "height": 4});
        metadata["geometry"]["viewport"] =
            serde_json::json!({"x": 1, "y": 1, "width": 3, "height": 2});
        write_metadata_fixture(&path, 16, &metadata);
        assert!(load_project(&path).unwrap_err().contains("version 17"));
        write_metadata_fixture(&path, VERSION, &metadata);
        assert_eq!(load_project(&path).unwrap().output_size(), [3, 2]);
        for view in [
            serde_json::json!({"x": -1, "y": 1, "width": 3, "height": 2}),
            serde_json::json!({"x": 1, "y": 1, "width": 8, "height": 2}),
            serde_json::json!({"x": 1, "y": 1, "width": 0, "height": 2}),
        ] {
            metadata["geometry"]["viewport"] = view;
            write_metadata_fixture(&path, VERSION, &metadata);
            assert!(load_project(&path).is_err());
        }
    }

    #[test]
    fn markup_objects_require_version_four() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("markup.myimage");
        let document = markup_document();
        for annotation in &document.annotations[4..] {
            for version in 1..=VERSION {
                let mut metadata = serde_json::json!({
                    "width": 6, "height": 4, "annotations": [annotation],
                });
                if version != 1 {
                    metadata["geometry"] = serde_json::to_value(document.geometry).unwrap();
                }
                write_metadata_fixture(&path, version, &metadata);
                let result = load_project(&path);
                if version < 4 {
                    assert!(result.unwrap_err().contains("version 4"));
                } else {
                    assert_eq!(
                        result.unwrap().annotations,
                        std::slice::from_ref(annotation)
                    );
                }
            }
        }
    }

    #[test]
    fn invalid_step_numbers_and_callout_tails_are_rejected_without_replacing_saved_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("markup.myimage");
        let mut document = markup_document();
        save_project(&document, &path).unwrap();
        let original_file = fs::read(&path).unwrap();
        for number in [0, MAX_STEP_NUMBER + 1, u32::MAX] {
            document.annotations[4].step_number = number;
            assert!(save_project(&document, &path).is_err());
            assert_eq!(fs::read(&path).unwrap(), original_file);
            let metadata = serde_json::json!({
                "width": 6, "height": 4, "annotations": document.annotations,
                "geometry": document.geometry,
            });
            let invalid_path = directory.path().join("invalid.myimage");
            write_metadata_fixture(&invalid_path, 4, &metadata);
            assert!(load_project(&invalid_path).is_err());
        }
        document.annotations[4].step_number = MAX_STEP_NUMBER;
        for tail in [
            None,
            Some(Point {
                x: f32::NAN,
                y: 0.0,
            }),
            Some(Point {
                x: 0.0,
                y: f32::INFINITY,
            }),
            Some(Point {
                x: MAX_COORDINATE + 1.0,
                y: 0.0,
            }),
            Some(Point {
                x: 0.0,
                y: -MAX_COORDINATE - 1.0,
            }),
        ] {
            document.annotations[5].tail = tail;
            assert!(save_project(&document, &path).is_err());
            assert_eq!(fs::read(&path).unwrap(), original_file);
            let metadata = serde_json::json!({
                "width": 6, "height": 4, "annotations": document.annotations,
                "geometry": document.geometry,
            });
            let invalid_path = directory.path().join("invalid.myimage");
            write_metadata_fixture(&invalid_path, 4, &metadata);
            assert!(load_project(&invalid_path).is_err());
        }
        document.annotations[5].tail = Some(Point {
            x: -MAX_COORDINATE,
            y: MAX_COORDINATE,
        });
        document.annotations[0].step_number = 0; // Not used by an ordinary text annotation.
        assert!(validate_document(&document).is_ok());
        document.annotations[0].tail = Some(Point {
            x: f32::NEG_INFINITY,
            y: 0.0,
        });
        assert!(validate_document(&document).is_err());
    }

    fn freehand_document() -> Document {
        let mut document = sample_document();
        let mut stroke = document.annotations[1].clone();
        stroke.kind = AnnotationKind::Freehand;
        stroke.points = vec![
            Point { x: 0.5, y: 1.25 },
            Point { x: 2.0, y: 2.5 },
            Point { x: 5.5, y: 3.5 },
        ];
        let mut dot = stroke.clone();
        dot.start = Point { x: 3.5, y: 2.0 };
        dot.end = dot.start;
        dot.points = vec![dot.start];
        document.annotations = vec![stroke, dot];
        document
    }

    #[test]
    fn freehand_round_trip_keeps_samples_styles_geometry_and_edit_history() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("freehand.myimage");
        let mut original = freehand_document();
        original.geometry.quarter_turns = 1;
        original.geometry.resize = Some([12, 18]);
        save_project(&original, &path).unwrap();
        assert_eq!(
            u32::from_le_bytes(fs::read(&path).unwrap()[8..12].try_into().unwrap()),
            VERSION
        );
        let mut loaded = load_project(&path).unwrap();
        assert_eq!(loaded.edit_state(), original.edit_state());
        assert_eq!(*loaded.image, *original.image);
        let pixels = loaded.image.clone();
        let before = loaded.edit_state();
        let mut history = EditHistory::new(before.clone());
        loaded.annotations[0].points[1] = Point { x: -3.0, y: 8.25 };
        loaded.annotations[0].color = [90, 40, 220, 180];
        loaded.annotations[0].stroke_width = 7.5;
        loaded.annotations[0].points.remove(0);
        loaded.annotations.swap(0, 1);
        loaded.geometry.quarter_turns = 3;
        history.commit(before.clone(), &loaded);
        let after = loaded.edit_state();
        assert!(history.is_dirty(&loaded));
        assert!(history.undo(&mut loaded));
        assert_eq!(loaded.edit_state(), before);
        assert!(!history.is_dirty(&loaded));
        assert!(history.redo(&mut loaded));
        assert_eq!(loaded.edit_state(), after);
        assert!(Arc::ptr_eq(&loaded.image, &pixels));
        save_project(&loaded, &path).unwrap();
        history.mark_saved(&loaded.edit_state());
        assert!(!history.is_dirty(&loaded));
        let reopened = load_project(&path).unwrap();
        assert_eq!(reopened.edit_state(), after);
        assert_eq!(*reopened.image, *original.image);
    }

    #[test]
    fn freehand_requires_version_five_and_version_four_markup_defaults_empty_points() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("versioned.myimage");
        let document = freehand_document();
        for version in 1..=VERSION {
            let mut metadata = serde_json::json!({
                "width": 6, "height": 4, "annotations": document.annotations,
            });
            if version != 1 {
                metadata["geometry"] = serde_json::to_value(document.geometry).unwrap();
            }
            write_metadata_fixture(&path, version, &metadata);
            if version < 5 {
                assert!(load_project(&path).unwrap_err().contains("version 5"));
            } else {
                assert_eq!(
                    load_project(&path).unwrap().edit_state(),
                    document.edit_state()
                );
            }
        }
        let legacy = markup_document();
        let metadata = serde_json::json!({
            "width": 6, "height": 4, "annotations": legacy.annotations,
            "geometry": legacy.geometry,
        });
        assert!(
            metadata["annotations"]
                .as_array()
                .unwrap()
                .iter()
                .all(|annotation| annotation.get("points").is_none())
        );
        write_metadata_fixture(&path, 4, &metadata);
        let loaded = load_project(&path).unwrap();
        assert_eq!(loaded.edit_state(), legacy.edit_state());
        assert!(
            loaded
                .annotations
                .iter()
                .all(|annotation| annotation.points.is_empty())
        );
        save_project(&loaded, &path).unwrap();
        assert_eq!(
            load_project(&path).unwrap().edit_state(),
            legacy.edit_state()
        );
    }

    #[test]
    fn invalid_freehand_samples_reject_save_and_load_without_replacing_saved_projects() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("freehand.myimage");
        let invalid_path = directory.path().join("invalid.myimage");
        let mut document = freehand_document();
        save_project(&document, &path).unwrap();
        let saved = fs::read(&path).unwrap();
        for points in [
            Vec::new(),
            vec![Point {
                x: f32::NAN,
                y: 1.0,
            }],
            vec![Point {
                x: 1.0,
                y: f32::INFINITY,
            }],
            vec![Point {
                x: f32::NEG_INFINITY,
                y: 0.0,
            }],
            vec![Point {
                x: MAX_COORDINATE + 1.0,
                y: 0.0,
            }],
            vec![Point {
                x: 0.0,
                y: -MAX_COORDINATE - 1.0,
            }],
            vec![Point { x: 1.0, y: 1.0 }; MAX_STROKE_POINTS + 1],
        ] {
            document.annotations[0].points = points;
            assert!(save_project(&document, &path).is_err());
            assert_eq!(fs::read(&path).unwrap(), saved);
            let metadata = serde_json::json!({
                "width": 6, "height": 4, "annotations": document.annotations,
                "geometry": document.geometry,
            });
            write_metadata_fixture(&invalid_path, 5, &metadata);
            assert!(load_project(&invalid_path).is_err());
        }
        document.annotations[0].points = vec![Point {
            x: -MAX_COORDINATE,
            y: MAX_COORDINATE,
        }];
        assert!(validate_document(&document).is_ok());
        document.annotations[0].kind = AnnotationKind::Rectangle;
        assert!(
            validate_document(&document)
                .unwrap_err()
                .contains("Only freehand")
        );
        document.annotations[0].points.clear();
        assert!(validate_document(&document).is_ok());
    }

    #[test]
    fn freehand_total_point_limit_is_enforced_on_save_and_load() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("limit.myimage");
        let mut document = freehand_document();
        document.annotations.truncate(1);
        document.annotations[0].points = vec![Point { x: 1.0, y: 1.0 }; MAX_STROKE_POINTS];
        document.annotations =
            vec![document.annotations[0].clone(); MAX_TOTAL_STROKE_POINTS / MAX_STROKE_POINTS];
        save_project(&document, &path).unwrap();
        let loaded = load_project(&path).unwrap();
        assert_eq!(loaded.edit_state(), document.edit_state());
        let saved = fs::read(&path).unwrap();
        document
            .annotations
            .push(freehand_document().annotations.remove(1));
        assert!(
            save_project(&document, &path)
                .unwrap_err()
                .contains("freehand points")
        );
        assert_eq!(fs::read(&path).unwrap(), saved);
        let metadata = serde_json::json!({
            "width": 6, "height": 4, "annotations": document.annotations,
            "geometry": document.geometry,
        });
        let invalid_path = directory.path().join("over-limit.myimage");
        write_metadata_fixture(&invalid_path, 5, &metadata);
        assert!(
            load_project(&invalid_path)
                .unwrap_err()
                .contains("freehand points")
        );
    }

    #[test]
    fn freehand_history_accounts_for_point_capacity_and_trims_to_memory_budget() {
        let mut document = freehand_document();
        let mut state = document.edit_state();
        let original_bytes = state_bytes(&state);
        let original_capacity = state.annotations[0].points.capacity();
        state.annotations[0].points.reserve_exact(MAX_STROKE_POINTS);
        assert_eq!(
            state_bytes(&state) - original_bytes,
            (state.annotations[0].points.capacity() - original_capacity)
                * std::mem::size_of::<Point>()
        );
        document.annotations[0].points = vec![Point { x: 1.0, y: 1.0 }; MAX_STROKE_POINTS];
        document.annotations =
            vec![document.annotations[0].clone(); MAX_TOTAL_STROKE_POINTS / MAX_STROKE_POINTS];
        let pixels = document.image.clone();
        let mut history = EditHistory::new(document.edit_state());
        for index in 0..MAX_HISTORY_STATES {
            let before = document.edit_state();
            document.annotations[0].points[0].x = index as f32 + 2.0;
            history.commit(before, &document);
        }
        assert!(history.undo.len() > 1 && history.undo.len() < MAX_HISTORY_STATES);
        assert!(history.undo.iter().map(state_bytes).sum::<usize>() <= MAX_HISTORY_BYTES);
        while history.undo(&mut document) {
            assert!(
                history
                    .undo
                    .iter()
                    .chain(&history.redo)
                    .map(state_bytes)
                    .sum::<usize>()
                    <= MAX_HISTORY_BYTES
            );
        }
        assert!(history.can_redo());
        assert!(Arc::ptr_eq(&document.image, &pixels));
    }

    #[test]
    fn groups_round_trip_and_history_preserve_objects_without_reordering() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("groups.myimage");
        let mut document = freehand_document();
        let ungrouped = document.edit_state();
        let mut history = EditHistory::new(ungrouped.clone());
        for a in &mut document.annotations {
            a.group_id = Some(u64::MAX);
        }
        history.commit(ungrouped.clone(), &document);
        save_project(&document, &path).unwrap();
        let mut loaded = load_project(&path).unwrap();
        assert_eq!(loaded.edit_state(), document.edit_state());
        assert_eq!(*loaded.image, *document.image);
        assert!(history.undo(&mut document));
        assert_eq!(document.edit_state(), ungrouped);
        assert!(history.redo(&mut document));
        assert_eq!(loaded.edit_state(), document.edit_state());
        loaded.annotations[0].stroke_width = 12.0;
        loaded.annotations[1].points[0].x += 2.0;
        save_project(&loaded, &path).unwrap();
        assert_eq!(
            load_project(&path).unwrap().edit_state(),
            loaded.edit_state()
        );
    }

    #[test]
    fn group_metadata_requires_version_six_and_valid_nonzero_ids() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("versioned-group.myimage");
        let doc = sample_document();
        for version in 1..=VERSION {
            for id in [0, 1, u64::MAX] {
                let mut metadata =
                    serde_json::json!({ "width":6, "height":4, "annotations":doc.annotations });
                metadata["annotations"][0]["group_id"] = serde_json::json!(id);
                if version != 1 {
                    metadata["geometry"] = serde_json::json!(ImageGeometry::default());
                }
                write_metadata_fixture(&path, version, &metadata);
                assert_eq!(load_project(&path).is_ok(), version >= 6 && id != 0);
            }
        }
        let mut invalid = doc.clone();
        invalid.annotations[0].group_id = Some(0);
        std::fs::write(&path, b"keep existing").unwrap();
        assert!(save_project(&invalid, &path).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"keep existing");
    }

    #[test]
    fn version_five_projects_default_to_ungrouped() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy-pen.myimage");
        let doc = freehand_document();
        let metadata = serde_json::json!({ "width":6, "height":4, "annotations":doc.annotations, "geometry":doc.geometry });
        assert!(
            metadata["annotations"]
                .as_array()
                .unwrap()
                .iter()
                .all(|a| a.get("group_id").is_none())
        );
        write_metadata_fixture(&path, 5, &metadata);
        let loaded = load_project(&path).unwrap();
        assert_eq!(loaded.annotations, doc.annotations);
        assert!(loaded.annotations.iter().all(|a| a.group_id.is_none()));
    }

    #[test]
    fn layer_format_roundtrip_and_legacy_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("layers.myimage");
        let mut d = sample_document();
        for version in 1..=6 {
            let mut metadata = serde_json::json!({"width":6,"height":4,"annotations":d.annotations,"geometry":d.geometry});
            if version == 1 {
                metadata.as_object_mut().unwrap().remove("geometry");
            }
            write_metadata_fixture(&path, version, &metadata);
            let loaded = load_project(&path).unwrap();
            assert!(
                loaded
                    .annotations
                    .iter()
                    .all(|a| a.layer == LayerSettings::default()
                        && a.group_layer == LayerSettings::default())
            );
        }
        d.annotations[0].layer = LayerSettings {
            object_id: None,
            name: "Başlık ✨".into(),
            hidden: true,
            locked: true,
        };
        for a in &mut d.annotations[1..] {
            a.group_id = Some(5);
            a.group_layer = LayerSettings {
                object_id: None,
                name: "Review".into(),
                hidden: false,
                locked: true,
            };
        }
        save_project(&d, &path).unwrap();
        assert_eq!(load_project(&path).unwrap().edit_state(), d.edit_state());
        let metadata = serde_json::json!({"width":6,"height":4,"annotations":d.annotations,"geometry":d.geometry});
        write_metadata_fixture(&path, 6, &metadata);
        assert!(load_project(&path).unwrap_err().contains("version 7"));
        d.annotations[1].group_layer.name = "Mismatch".into();
        assert!(validate_document(&d).unwrap_err().contains("matching"));
        d.annotations[1].group_id = None;
        assert!(validate_document(&d).unwrap_err().contains("ungrouped"));
    }

    #[test]
    fn all_shape_kinds_roundtrip_with_layers_and_require_version_eight() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("shapes.myimage");
        let mut doc = sample_document();
        let template = doc.annotations[2].clone();
        doc.annotations = crate::shapes::CHOICES
            .iter()
            .map(|(kind, name)| {
                let mut a = template.clone();
                a.kind = *kind;
                a.layer.name = (*name).into();
                a.fill = true;
                a
            })
            .collect();
        save_project(&doc, &path).unwrap();
        let loaded = load_project(&path).unwrap();
        assert_eq!(loaded.edit_state(), doc.edit_state());
        assert_eq!(*loaded.image, *doc.image);
        let mut bytes = fs::read(&path).unwrap();
        bytes[8..12].copy_from_slice(&7_u32.to_le_bytes());
        fs::write(&path, bytes).unwrap();
        assert!(load_project(&path).unwrap_err().contains("version 8"));
    }

    #[test]
    fn rejects_truncation_unknown_version_and_corrupt_image() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("project.myimage");
        save_project(&sample_document(), &path).unwrap();
        let original = fs::read(&path).unwrap();
        for length in [0, 10, 23, original.len() - 1] {
            fs::write(&path, &original[..length]).unwrap();
            assert!(load_project(&path).is_err());
        }
        let mut invalid = original.clone();
        invalid[8..12].copy_from_slice(&(VERSION + 1).to_le_bytes());
        fs::write(&path, invalid).unwrap();
        assert!(
            load_project(&path)
                .unwrap_err()
                .contains(&format!("version {}", VERSION + 1))
        );

        let mut invalid = original.clone();
        invalid[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        fs::write(&path, invalid).unwrap();
        assert!(load_project(&path).is_err());

        let mut invalid = original;
        let json_bytes = u32::from_le_bytes(invalid[12..16].try_into().unwrap()) as usize;
        invalid[HEADER_BYTES as usize + json_bytes] = 0;
        fs::write(&path, invalid).unwrap();
        assert!(load_project(&path).is_err());
    }

    #[test]
    fn invalid_models_and_failed_save_leave_existing_project_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("project.myimage");
        let mut document = sample_document();
        save_project(&document, &path).unwrap();
        let original = fs::read(&path).unwrap();
        document.geometry.resize = Some([0, 10]);
        assert!(save_project(&document, &path).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        document.geometry = ImageGeometry::default();
        document.annotations[0].start.x = f32::NAN;
        assert!(save_project(&document, &path).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        document.annotations[0].start.x = 1.0;
        document.annotations[0].font_size = f32::INFINITY;
        assert!(validate_document(&document).is_err());
        document.annotations[0].font_size = MAX_FONT_SIZE + 1.0;
        assert!(validate_document(&document).is_err());
        document.annotations[0].font_size = MAX_FONT_SIZE;
        document.annotations[0].stroke_width = MAX_STROKE_WIDTH;
        assert!(validate_document(&document).is_ok());
        document.annotations[0].stroke_width = MAX_STROKE_WIDTH + 1.0;
        assert!(validate_document(&document).is_err());
        document.annotations[0].stroke_width = 2.5;
        document.annotations[0].font_size = 24.0;
        document.annotations[0].text = "x".repeat(MAX_TEXT_BYTES + 1);
        assert!(validate_document(&document).is_err());
        document.annotations.clear();
        document.image = Arc::new(RgbaImage::new(0, 0));
        assert!(validate_document(&document).is_err());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn rejects_invalid_annotation_and_dimension_metadata_on_load() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("project.myimage");
        let mut document = sample_document();
        document.annotations[0].stroke_width = -1.0;
        let metadata = Metadata {
            width: 6,
            height: 4,
            annotations: document.annotations.clone(),
            geometry: document.geometry,
        };
        let mut encoded = Cursor::new(Vec::new());
        write_contents(
            &mut encoded,
            &document,
            &serde_json::to_vec(&metadata).unwrap(),
        )
        .unwrap();
        fs::write(&path, encoded.into_inner()).unwrap();
        assert!(load_project(&path).is_err());

        let metadata = Metadata {
            width: 32_768,
            height: 32_768,
            annotations: Vec::new(),
            geometry: document.geometry,
        };
        let mut encoded = Cursor::new(Vec::new());
        write_contents(
            &mut encoded,
            &document,
            &serde_json::to_vec(&metadata).unwrap(),
        )
        .unwrap();
        fs::write(&path, encoded.into_inner()).unwrap();
        assert!(load_project(&path).is_err());
    }

    #[test]
    fn history_tracks_saved_state_across_mixed_edits_undo_redo_and_branches() {
        let mut document = sample_document();
        let original = document.edit_state();
        let pixels = document.image.clone();
        let mut history = EditHistory::new(original.clone());
        assert!(!history.is_dirty(&document));
        document.annotations[0].text = "first edit".into();
        history.commit(original.clone(), &document);
        let annotation_edit = document.edit_state();
        document.geometry.quarter_turns = 1;
        document.geometry.resize = Some([80, 120]);
        history.commit(annotation_edit.clone(), &document);
        assert!(history.is_dirty(&document));
        history.mark_saved(&document.edit_state());
        assert!(!history.is_dirty(&document));
        assert!(history.undo(&mut document));
        assert_eq!(document.edit_state(), annotation_edit);
        assert!(history.is_dirty(&document));
        assert!(history.undo(&mut document));
        assert_eq!(document.edit_state(), original);
        assert!(history.can_redo());
        assert!(history.redo(&mut document));
        assert!(history.is_dirty(&document));
        assert!(history.redo(&mut document));
        assert!(!history.is_dirty(&document));
        assert!(history.can_undo());
        assert!(history.undo(&mut document));
        let before = document.edit_state();
        document.annotations.pop();
        history.commit(before, &document);
        assert!(!history.can_redo());
        assert!(history.is_dirty(&document));
        assert!(Arc::ptr_eq(&document.image, &pixels));
    }

    #[test]
    fn history_bounds_memory_and_ignores_empty_edits() {
        let mut document = sample_document();
        let mut history = EditHistory::new(document.edit_state());
        history.commit(document.edit_state(), &document);
        assert!(!history.can_undo());
        for index in 0..MAX_HISTORY_STATES + 10 {
            let before = document.edit_state();
            document.annotations[0].text = index.to_string();
            history.commit(before, &document);
        }
        let mut undo_count = 0;
        while history.undo(&mut document) {
            undo_count += 1;
        }
        assert_eq!(undo_count, MAX_HISTORY_STATES);
        assert!(history.can_redo());
    }
}
