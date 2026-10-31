//! Bounded, read-only import of portable Snagit `.snagx` projects.
//! Archive paths are names only: no member is ever extracted to the filesystem.
use crate::document::{self, Annotation, AnnotationKind, Document, Point};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{ImageReader, Rgba, RgbaImage};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs::File,
    io::{Cursor, Read, Seek},
    path::Path,
    sync::Arc,
};
use zip::ZipArchive;
mod rtf;

const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_MEMBERS: usize = 2048;
const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_RTF_BYTES: usize = 256 * 1024;

pub struct ImportResult {
    pub document: Document,
    pub warnings: Vec<String>,
}

pub fn import(path: &Path) -> Result<ImportResult, String> {
    let file = File::open(path).map_err(|e| format!("Cannot open Snagit project: {e}"))?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_ARCHIVE_BYTES {
        return Err("Snagit projects larger than 512 MB are not supported.".into());
    }
    import_reader(file)
}

fn import_reader<R: Read + Seek>(reader: R) -> Result<ImportResult, String> {
    let mut archive = ZipArchive::new(reader)
        .map_err(|e| format!("This is not a readable .snagx project: {e}"))?;
    if archive.len() > MAX_MEMBERS {
        return Err("The Snagit project has too many archive members.".into());
    }
    let mut names = HashSet::new();
    let mut total = 0u64;
    for i in 0..archive.len() {
        let member = archive
            .by_index(i)
            .map_err(|e| format!("Cannot read Snagit archive: {e}"))?;
        let name = member
            .name()
            .map_err(|e| format!("Invalid Snagit archive member name: {e}"))?;
        if !safe_name(&name) || !names.insert(name.into_owned()) {
            return Err("The Snagit archive contains an unsafe or duplicate member name.".into());
        }
        total = total
            .checked_add(member.size())
            .ok_or("Snagit archive size overflow.")?;
        if total > MAX_ARCHIVE_BYTES {
            return Err("The unpacked Snagit project exceeds 512 MB.".into());
        }
    }
    let index = read_json(&mut archive, "index.json", 1024 * 1024)?;
    if index.get("Version").and_then(Value::as_str) != Some("1.0") {
        return Err("This Snagit project uses an unsupported archive version.".into());
    }
    let pages = index
        .get("Pages")
        .and_then(Value::as_array)
        .ok_or("The Snagit project has no page index.")?;
    if pages.len() != 1 {
        return Err("Multi-page Snagit projects are not supported yet. Save each page separately in Snagit before importing.".into());
    }
    let page_name = pages[0]
        .as_str()
        .ok_or("The Snagit page reference is invalid.")?;
    let page = read_json(&mut archive, page_name, document::MAX_METADATA_BYTES)?;
    load_page(&mut archive, &page)
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && !name.contains(':')
        && !name
            .split('/')
            .any(|part| part == ".." || part == "." || part.is_empty())
}
fn read_member<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
    limit: usize,
) -> Result<Vec<u8>, String> {
    if !safe_name(name) {
        return Err("The Snagit project references an unsafe archive name.".into());
    }
    let member = archive
        .by_name(name)
        .map_err(|_| format!("The Snagit project is missing {name}."))?;
    if member.size() > limit as u64 {
        return Err(format!("Snagit archive member {name} is too large."));
    }
    let mut bytes = Vec::new();
    member
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read Snagit archive member {name}: {e}"))?;
    if bytes.len() > limit {
        return Err(format!(
            "Snagit archive member {name} exceeds its size limit."
        ));
    }
    Ok(bytes)
}
fn read_json<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
    limit: usize,
) -> Result<Value, String> {
    serde_json::from_slice(&read_member(archive, name, limit)?)
        .map_err(|e| format!("Invalid Snagit metadata in {name}: {e}"))
}
fn decode_pixels(bytes: &[u8]) -> Result<RgbaImage, String> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(document::MAX_IMAGE_SIDE);
    limits.max_image_height = Some(document::MAX_IMAGE_SIDE);
    limits.max_alloc = Some(800_000_000);
    reader.limits(limits);
    let decoder = reader
        .into_decoder()
        .map_err(|e| format!("Cannot decode Snagit image: {e}"))?;
    use image::ImageDecoder;
    let (w, h) = decoder.dimensions();
    document::validate_dimensions(w, h)?;
    image::DynamicImage::from_decoder(decoder)
        .map(|i| i.to_rgba8())
        .map_err(|e| e.to_string())
}
fn finite(v: &Value, key: &str, default: f64) -> Result<f64, String> {
    match v.get(key) {
        None => Ok(default),
        Some(value) => value
            .as_f64()
            .filter(|n| n.is_finite())
            .ok_or_else(|| format!("Invalid Snagit {key}.")),
    }
}
fn scalar(v: &Value, key: &str, scale: f32, default: f64) -> Result<f32, String> {
    let n = finite(v, key, default)? * f64::from(scale);
    if n.abs() > 1_000_000. {
        return Err(format!("Snagit {key} is outside the supported range."));
    }
    Ok(n as f32)
}
fn pair(v: &Value) -> Result<Point, String> {
    let text = v.as_str().ok_or("Invalid Snagit coordinate pair.")?;
    let (x, y) = text
        .split_once(',')
        .ok_or("Invalid Snagit coordinate pair.")?;
    let p = Point {
        x: x.trim()
            .parse()
            .map_err(|_| "Invalid Snagit X coordinate.")?,
        y: y.trim()
            .parse()
            .map_err(|_| "Invalid Snagit Y coordinate.")?,
    };
    if !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > 1_000_000. || p.y.abs() > 1_000_000. {
        return Err("Snagit coordinates exceed the supported range.".into());
    }
    Ok(p)
}
fn scaled(p: Point, scale: f32) -> Point {
    Point {
        x: p.x * scale,
        y: p.y * scale,
    }
}
fn color(text: &str) -> Result<[u8; 4], String> {
    let s = text.strip_prefix('#').ok_or("Invalid Snagit color.")?;
    if !matches!(s.len(), 6 | 8) || !s.is_ascii() {
        return Err("Invalid Snagit color.".into());
    }
    let n = u32::from_str_radix(s, 16).map_err(|_| "Invalid Snagit color.")?;
    Ok(if s.len() == 8 {
        [(n >> 16) as u8, (n >> 8) as u8, n as u8, (n >> 24) as u8]
    } else {
        [(n >> 16) as u8, (n >> 8) as u8, n as u8, 255]
    })
}
fn object_color(o: &Value, key: &str, default: [u8; 4], opacity: f32) -> Result<[u8; 4], String> {
    let mut c = o.get(key).map_or(Ok(default), |v| {
        color(v.as_str().ok_or("Invalid Snagit color value.")?)
    })?;
    c[3] = (f32::from(c[3]) * opacity).round() as u8;
    Ok(c)
}
fn flag(v: &Value, key: &str, default: bool) -> Result<bool, String> {
    match v.get(key) {
        None => Ok(default),
        Some(Value::Bool(value)) => Ok(*value),
        Some(value) if value.as_i64() == Some(0) => Ok(false),
        Some(value) if value.as_i64() == Some(1) => Ok(true),
        _ => Err(format!("Invalid Snagit {key} flag.")),
    }
}
fn nonempty(v: &Value, key: &str) -> Result<bool, String> {
    match v.get(key) {
        None => Ok(false),
        Some(Value::Array(values)) => Ok(!values.is_empty()),
        _ => Err(format!("Invalid Snagit {key} list.")),
    }
}
fn note(warnings: &mut Vec<String>, message: &str) {
    if !warnings.iter().any(|w| w == message) {
        warnings.push(message.into());
    }
}
fn load_page<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    page: &Value,
) -> Result<ImportResult, String> {
    if !page.is_object() {
        return Err("Invalid Snagit page metadata.".into());
    }
    let smart = flag(page, "SmartRedactEnabled", false)?;
    let smart_properties = if let Some(properties) = page.get("SmartRedactProperties") {
        if !properties.is_object() {
            return Err("Invalid Snagit smart-redaction settings.".into());
        }
        flag(properties, "SmartRedactEnabled", false)?
    } else {
        false
    };
    if smart || smart_properties {
        return Err("Smart-redacted Snagit projects cannot be imported safely yet. Export a redacted PNG from Snagit instead.".into());
    }
    if nonempty(page, "Effects")? {
        return Err("This Snagit project has whole-image effects that MyImage cannot import faithfully yet. Remove the effects in a copy in Snagit, or export a PNG instead.".into());
    }
    if nonempty(page, "Sections")? {
        return Err("Snagit template sections are not supported yet. Export a PNG or save an ordinary image project from Snagit.".into());
    }
    let dpi = finite(page, "DPI", 72.)?;
    if !(1. ..=2400.).contains(&dpi) {
        return Err("Invalid Snagit pixel density.".into());
    }
    let scale = (dpi / 72.) as f32;
    let dimension = |key: &str| -> Result<u32, String> {
        let n = scalar(page, key, scale, 0.)?;
        if n < 1. || n > document::MAX_IMAGE_SIDE as f32 {
            return Err("Snagit canvas dimensions exceed MyImage's supported limits.".into());
        }
        Ok(n.round() as u32)
    };
    let width = dimension("CaptureCanvasWidth")?;
    let height = dimension("CaptureCanvasHeight")?;
    document::validate_dimensions(width, height)?;
    let background = page
        .get("CaptureBackgroundColor")
        .and_then(Value::as_str)
        .map(color)
        .transpose()?
        .unwrap_or([0; 4]);
    let mut warnings = Vec::new();
    if finite(page, "CornerRadiusRatio", 0.)? > 0. {
        note(
            &mut warnings,
            "Canvas corner rounding is not reproduced; the original background pixels remain intact.",
        );
    }
    if let Some(gradient) = page.get("GradientBackgroundColor")
        && gradient
            .get("GradientColors")
            .and_then(Value::as_array)
            .is_some_and(|a| a.len() > 1)
    {
        return Err("Gradient Snagit canvas backgrounds are not supported yet. Export a PNG to preserve the appearance.".into());
    }
    let background_image = if let Some(reference) = page
        .get("CaptureBackgroundImage")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        let pixels = decode_pixels(&read_member(archive, reference, MAX_IMAGE_BYTES)?)?;
        let location = page
            .get("CaptureBackgroundLocation")
            .map(pair)
            .transpose()?
            .unwrap_or(Point { x: 0., y: 0. });
        let target_size = page
            .get("BackgroundSize")
            .map(pair)
            .transpose()?
            .unwrap_or(Point { x: 0., y: 0. });
        let size = if target_size.x > 0. && target_size.y > 0. {
            [
                (target_size.x * scale).round() as u32,
                (target_size.y * scale).round() as u32,
            ]
        } else {
            [pixels.width(), pixels.height()]
        };
        document::validate_dimensions(size[0], size[1])?;
        let pixels = if size != [pixels.width(), pixels.height()] {
            image::imageops::resize(
                &pixels,
                size[0],
                size[1],
                image::imageops::FilterType::Lanczos3,
            )
        } else {
            pixels
        };
        Some((
            pixels,
            (location.x * scale).round() as i64,
            (location.y * scale).round() as i64,
        ))
    } else {
        None
    };
    let (left, top, right, bottom) = background_image.as_ref().map_or(
        (0, 0, i64::from(width), i64::from(height)),
        |(pixels, x, y)| {
            (
                (*x).min(0),
                (*y).min(0),
                (*x + i64::from(pixels.width())).max(i64::from(width)),
                (*y + i64::from(pixels.height())).max(i64::from(height)),
            )
        },
    );
    let retained_width =
        u32::try_from(right - left).map_err(|_| "Snagit retained image bounds are too large.")?;
    let retained_height =
        u32::try_from(bottom - top).map_err(|_| "Snagit retained image bounds are too large.")?;
    document::validate_dimensions(retained_width,retained_height).map_err(|_|"The Snagit canvas and hidden background together exceed MyImage's supported limits. No pixels were discarded.")?;
    let offset = [-left as f32, -top as f32];
    let mut canvas = RgbaImage::new(retained_width, retained_height);
    // Only the original visible window owns the canvas background color. New
    // space outside it stays transparent; all hidden source pixels are retained.
    if background[3] > 0 {
        for y in (-top) as u32..(-top) as u32 + height {
            for x in (-left) as u32..(-left) as u32 + width {
                canvas.put_pixel(x, y, Rgba(background));
            }
        }
    }
    if let Some((pixels, x, y)) = background_image {
        image::imageops::overlay(&mut canvas, &pixels, x - left, y - top);
    }
    let objects = page
        .get("CaptureObjects")
        .and_then(Value::as_array)
        .ok_or("The Snagit page has no object list.")?;
    if objects.len() > document::MAX_ANNOTATIONS {
        return Err("The Snagit project has too many objects.".into());
    }
    let mut annotations = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        let imported = import_object(object, scale, &mut warnings)
            .map_err(|e| format!("Cannot import Snagit object {}: {e}", index + 1))?;
        let mut imported = imported;
        if imported.len() > 1 {
            for a in &mut imported {
                a.group_id = Some(index as u64 + 1);
                a.group_layer.name = "Snagit shape".into();
            }
        }
        annotations.extend(imported);
        if annotations.len() > document::MAX_ANNOTATIONS {
            return Err("The converted Snagit project has too many objects.".into());
        }
    }
    if offset != [0., 0.] {
        for a in &mut annotations {
            translate(a, offset);
        }
    }
    let mut geometry = crate::geometry::ImageGeometry::default();
    if retained_width != width || retained_height != height || offset != [0., 0.] {
        geometry.canvas = Some(crate::geometry::CanvasRect {
            x: 0,
            y: 0,
            width: retained_width,
            height: retained_height,
        });
        geometry.viewport = Some(crate::geometry::CanvasRect {
            x: offset[0] as i32,
            y: offset[1] as i32,
            width,
            height,
        });
    }
    let document = Document {
        image: Arc::new(canvas),
        annotations,
        geometry,
    };
    document::validate_document(&document)?;
    Ok(ImportResult { document, warnings })
}

fn translate(a: &mut Annotation, offset: [f32; 2]) {
    let move_point = |p: Point| Point {
        x: p.x + offset[0],
        y: p.y + offset[1],
    };
    a.start = move_point(a.start);
    a.end = move_point(a.end);
    a.tail = a.tail.map(move_point);
    a.arrow.curve = a.arrow.curve.map(|points| points.map(move_point));
    for point in &mut a.points {
        *point = move_point(*point);
    }
}

fn base_annotation(kind: AnnotationKind, start: Point, end: Point) -> Annotation {
    Annotation {
        kind,
        start,
        end,
        color: [0, 0, 0, 255],
        stroke_width: 1.,
        fill: false,
        text: String::new(),
        font_size: 28.,
        effect_size: 16,
        step_number: 1,
        tail: None,
        text_color: [255; 4],
        points: vec![],
        group_id: None,
        layer: Default::default(),
        group_layer: Default::default(),
        area_source: None,
        area_mask: vec![],
        stamp: None,
        fill_region: None,
        text_style: Default::default(),
        callout_style: Default::default(),
        pixel_mask: None,
        studio: Default::default(),
        arrow: Default::default(),
    }
}
fn bounds(o: &Value, scale: f32) -> Result<(Point, Point), String> {
    let points = o
        .get("PointsArray")
        .and_then(Value::as_array)
        .filter(|p| p.len() >= 2);
    let (a, b) = if let Some(points) = points {
        (pair(&points[0])?, pair(&points[points.len() - 1])?)
    } else {
        (
            pair(o.get("StartPoint").ok_or("Missing Snagit start point.")?)?,
            pair(o.get("EndPoint").ok_or("Missing Snagit end point.")?)?,
        )
    };
    Ok((scaled(a, scale), scaled(b, scale)))
}
fn import_object(
    o: &Value,
    scale: f32,
    warnings: &mut Vec<String>,
) -> Result<Vec<Annotation>, String> {
    let mode = o
        .get("ToolMode")
        .and_then(Value::as_str)
        .ok_or("Missing Snagit object type.")?;
    for key in [
        "Effects",
        "Children",
        "Objects",
        "SubObjects",
        "GroupObjects",
    ] {
        if nonempty(o, key)? {
            return Err("nested groups or per-object effects are not supported yet; no layers were discarded.".into());
        }
    }
    if o.as_object().is_some_and(|fields| {
        fields.iter().any(|(key, value)| {
            key.to_ascii_lowercase().contains("group")
                && !value.is_null()
                && value.as_i64() != Some(0)
                && value.as_bool() != Some(false)
                && value.as_str() != Some("0")
                && value.as_str() != Some("")
                && value.as_array().is_none_or(|a| !a.is_empty())
                && value.as_object().is_none_or(|o| !o.is_empty())
        })
    }) {
        return Err("this Snagit grouping layout is not supported yet; ungroup a copy in Snagit before importing.".into());
    }
    if finite(o, "ObjectPriority", 0.)? != 0. {
        return Err("this Snagit stacking priority is not supported yet. Flatten it in a copy in Snagit or export a PNG to preserve the appearance.".into());
    }
    let rotation = finite(o, "RotationAngle", 0.)?;
    if rotation % 360. != 0. {
        return Err(format!(
            "rotated {mode} objects are not supported yet; export the object as a PNG in Snagit to preserve it."
        ));
    }
    if o.get("RedactionType")
        .and_then(Value::as_str)
        .is_some_and(|s| s != "None")
    {
        return Err("smart-redacted objects cannot be imported faithfully yet.".into());
    }
    let opacity = finite(o, "Opacity", 100.)?;
    if !(0. ..=100.).contains(&opacity) {
        return Err("Invalid Snagit opacity.".into());
    }
    let opacity = opacity as f32 / 100.;
    let (mut start, mut end) = bounds(o, scale)?;
    if mode != "Arrow" && mode != "Line" && mode != "Pen" && mode != "Freehand" {
        let lo = Point {
            x: start.x.min(end.x),
            y: start.y.min(end.y),
        };
        let hi = Point {
            x: start.x.max(end.x),
            y: start.y.max(end.y),
        };
        start = lo;
        end = hi;
    }
    if mode != "Pen"
        && mode != "Freehand"
        && o.get("PointsArray")
            .and_then(Value::as_array)
            .is_some_and(|points| points.len() > 2)
    {
        return Err(
            "multi-point Snagit object geometry is not supported yet; no geometry was discarded."
                .into(),
        );
    }
    let kind = match mode {
        "Text" => AnnotationKind::Text,
        "Arrow" | "Line" => AnnotationKind::Arrow,
        "Image" | "Stamp" => AnnotationKind::Stamp,
        "Shape" => shape_kind(
            o.get("ToolShape")
                .and_then(Value::as_str)
                .unwrap_or("Rectangle"),
        )?,
        "Callout" => AnnotationKind::Callout,
        "Magnify" => AnnotationKind::Magnify,
        "Pen" | "Freehand" => AnnotationKind::Freehand,
        "Highlight" | "Highlighter" => AnnotationKind::Highlighter,
        _ => {
            return Err(format!(
                "the {mode} tool is not supported yet. No objects were discarded; the import was stopped."
            ));
        }
    };
    let mut a = base_annotation(kind, start, end);
    a.layer.hidden = flag(o, "IsHidden", false)?;
    a.layer.locked = flag(o, "IsLocked", false)?;
    a.layer.name = format!("Snagit {mode}");
    let foreground = object_color(o, "ForegroundColor", [0, 0, 0, 255], opacity)?;
    let background = object_color(o, "BackgroundColor", [0; 4], opacity)?;
    a.color = foreground;
    a.stroke_width = scalar(o, "StrokeWidth", scale, 1.)?;
    if a.stroke_width < 0. || a.stroke_width > document::MAX_STROKE_WIDTH {
        return Err("Snagit stroke weight exceeds MyImage's supported range.".into());
    }
    a.arrow.pattern = match o.get("DashType").and_then(Value::as_str).unwrap_or("Solid") {
        "Solid" => crate::arrow_style::Pattern::Solid,
        "Dash" | "Dashed" => crate::arrow_style::Pattern::Dashed,
        "Dot" | "Dotted" => crate::arrow_style::Pattern::Dotted,
        "DashDot" | "Dash-Dot" => crate::arrow_style::Pattern::DashDot,
        "LongDash" => crate::arrow_style::Pattern::LongDash,
        value => return Err(format!("line pattern {value} is not supported yet.")),
    };
    if flag(o, "DropShadowEnabled", false)? {
        note(
            warnings,
            "Object shadows are not reproduced; their objects remain editable.",
        );
    }
    if o.get("GradientBackgroundColor")
        .and_then(|g| g.get("GradientColors"))
        .and_then(Value::as_array)
        .is_some_and(|c| c.len() > 1)
    {
        note(
            warnings,
            "Object gradients use their solid background color; objects remain editable.",
        );
    }
    if kind.is_shape() {
        if finite(o, "CornerRadiusRatio", 0.)? > 0. {
            if kind == AnnotationKind::Rectangle {
                a.kind = AnnotationKind::RoundedRectangle;
            }
            note(
                warnings,
                "Rounded corners use MyImage's standard corner radius.",
            );
        }
        if o.get("BorderStyle")
            .and_then(Value::as_str)
            .is_some_and(|style| style != "Middle")
        {
            note(
                warnings,
                "Shape outlines are centered on their borders in MyImage.",
            );
        }
    }
    if kind == AnnotationKind::Text || kind == AnnotationKind::Callout {
        a.text = object_text(o)?;
        a.font_size = scalar(o, "FontSize", scale, 24.)?;
        if !(1. ..=document::MAX_FONT_SIZE).contains(&a.font_size) {
            return Err("Snagit text size exceeds MyImage's supported range.".into());
        }
        a.text_color = if o.get("TextSelectionColor").is_some() {
            object_color(o, "TextSelectionColor", foreground, opacity)?
        } else {
            foreground
        };
        a.text_style.italic = flag(o, "TextSelectionItalic", false)?;
        a.text_style.underline = flag(o, "TextSelectionUnderline", false)?;
        if flag(o, "TextSelectionStrikethrough", false)?
            || finite(o, "TextOutlineWidth", 0.)? > 0.
            || finite(o, "TextLineSpacing", 0.)? != 0.
        {
            note(
                warnings,
                "Text outlines, strikethrough and custom line spacing are not reproduced; the text remains editable.",
            );
        }
        if o.get("ToolVerticalAlign")
            .and_then(Value::as_str)
            .is_some_and(|align| align != "Center" && align != "Top")
        {
            note(
                warnings,
                "Custom vertical text alignment uses MyImage's standard text placement.",
            );
        }
        a.text_style.monospace = o
            .get("FontName")
            .and_then(Value::as_str)
            .is_some_and(|s| s.contains("Mono") || s.contains("Courier"));
        a.text_style.alignment = match o
            .get("ToolHorizontalAlign")
            .and_then(Value::as_str)
            .unwrap_or("Left")
        {
            "Left" => crate::text_style::Alignment::Left,
            "Center" => crate::text_style::Alignment::Center,
            "Right" => crate::text_style::Alignment::Right,
            _ => {
                note(warnings, "Justified text uses left alignment in MyImage.");
                crate::text_style::Alignment::Left
            }
        };
        note(
            warnings,
            "Text remains editable using MyImage fonts; exact Snagit font faces and mixed formatting may differ.",
        );
        if kind == AnnotationKind::Text {
            a.color = a.text_color;
            a.stroke_width = 0.1;
        }
    }
    match kind {
        AnnotationKind::Arrow => {
            a.arrow.start = endpoint(
                o.get("ArrowStart")
                    .and_then(Value::as_str)
                    .unwrap_or("None"),
                warnings,
            )?;
            a.arrow.end = endpoint(
                o.get("ArrowEnd")
                    .and_then(Value::as_str)
                    .unwrap_or(if mode == "Line" { "None" } else { "Arrow" }),
                warnings,
            )?;
            a.arrow.start_size = finite(o, "ArrowStartWidth", 3.)? as f32;
            a.arrow.end_size = finite(o, "ArrowEndWidth", 3.)? as f32;
            if flag(o, "BezierCurve", false)? {
                let p = o
                    .get("ControlPoints")
                    .and_then(Value::as_array)
                    .filter(|p| p.len() == 2)
                    .ok_or("Curved Snagit arrows require two supported control points.")?;
                a.arrow.curve = Some([scaled(pair(&p[0])?, scale), scaled(pair(&p[1])?, scale)]);
            }
        }
        AnnotationKind::Magnify => {
            let offset = o
                .get("MagnifyOffset")
                .map(pair)
                .transpose()?
                .unwrap_or(Point { x: 0., y: 0. });
            if offset.x != 0. || offset.y != 0. {
                return Err("linked Snagit Magnify source offsets are not supported yet. Use a standalone lens in a copy in Snagit before importing.".into());
            }
            if opacity != 1. {
                return Err("translucent Snagit Magnify lenses are not supported yet.".into());
            }
            a.studio.zoom = (finite(o, "MagnifyScale", 200.)? / 100.) as f32;
            a.studio.linked = false;
            match o
                .get("ToolShape")
                .and_then(Value::as_str)
                .unwrap_or("Ellipse")
            {
                "Ellipse" | "Oval" | "Circle" => a.studio.ellipse = true,
                "Rectangle" | "Rect" => a.studio.ellipse = false,
                "RoundedRectangle" | "RoundedRect" => {
                    a.studio.ellipse = false;
                    a.studio.rounded = true;
                    note(
                        warnings,
                        "Rounded corners use MyImage's standard corner radius.",
                    );
                }
                shape => {
                    return Err(format!(
                        "Snagit Magnify shape {shape} is not supported yet."
                    ));
                }
            }
        }
        AnnotationKind::Stamp => {
            let data = o
                .get("Image")
                .and_then(Value::as_str)
                .ok_or("Snagit image layer has no embedded pixels.")?;
            if data.len() > MAX_IMAGE_BYTES.div_ceil(3) * 4 {
                return Err("Snagit image layer is too large.".into());
            }
            let bytes = STANDARD
                .decode(data)
                .map_err(|_| "Invalid embedded Snagit image.")?;
            let pixels = decode_pixels(&bytes)?;
            a.stamp = Some(crate::stamps::StampSource::from_pixels(&pixels)?);
            a.color = [255, 255, 255, (opacity * 255.).round() as u8];
            a.fill = true;
            a.stroke_width = 0.1;
        }
        AnnotationKind::Callout => {
            a.callout_style = callout_style(
                o.get("CalloutShape")
                    .and_then(Value::as_str)
                    .unwrap_or("CTRoundedRect"),
                warnings,
            )?;
            if foreground[3] > 0
                && background[3] > 0
                && foreground != background
                && a.stroke_width > 0.
            {
                note(
                    warnings,
                    "Callout outlines use the callout fill color; text and body remain editable.",
                );
            }
            a.color = if background[3] > 0 {
                background
            } else {
                foreground
            };
            a.fill = background[3] > 0;
            if let Some(tails) = o.get("CalloutTails").and_then(Value::as_array) {
                if tails.len() > 1 {
                    return Err("Multi-tail Snagit callouts are not supported yet.".into());
                }
                a.tail = tails
                    .first()
                    .map(pair)
                    .transpose()?
                    .map(|p| scaled(p, scale));
            }
            if o.get("TailStyle").and_then(Value::as_str) == Some("Arrow") {
                note(
                    warnings,
                    "Arrow-style callout tails use MyImage's editable speech-bubble tail.",
                );
            }
        }
        AnnotationKind::Freehand => {
            let points = o
                .get("PointsArray")
                .and_then(Value::as_array)
                .ok_or("Missing Snagit pen points.")?;
            if points.len() > document::MAX_STROKE_POINTS {
                return Err("Snagit pen stroke has too many points.".into());
            }
            a.points = points
                .iter()
                .map(|v| pair(v).map(|p| scaled(p, scale)))
                .collect::<Result<_, _>>()?;
        }
        AnnotationKind::Highlighter => {
            a.fill = true;
            a.color = if background[3] > 0 {
                background
            } else {
                foreground
            };
            if background[3] > 0
                && foreground[3] > 0
                && foreground != background
                && a.stroke_width > 0.
            {
                note(
                    warnings,
                    "Separate highlighter outlines are not reproduced; the editable highlight fill is preserved.",
                );
            }
        }
        _ if kind.is_shape() => {
            if background[3] > 0 {
                a.fill = true;
                a.color = background;
                if foreground[3] > 0 && a.stroke_width > 0. && foreground != background {
                    let mut outline = a.clone();
                    outline.fill = false;
                    outline.color = foreground;
                    a.stroke_width = 0.1;
                    return Ok(vec![a, outline]);
                }
                if foreground[3] == 0 {
                    a.stroke_width = 0.1;
                }
            }
        }
        _ => {}
    }
    a.stroke_width = a.stroke_width.max(0.1);
    if a.kind == AnnotationKind::Callout && a.tail.is_none() {
        a.tail = Some(Point {
            x: (a.start.x + a.end.x) * 0.5,
            y: (a.start.y + a.end.y) * 0.5,
        });
    }
    Ok(vec![a])
}
fn object_text(o: &Value) -> Result<String, String> {
    if let Some(encoded) = o.get("RTFEncodedText").and_then(Value::as_str) {
        if encoded.len() > MAX_RTF_BYTES.div_ceil(3) * 4 {
            return Err("Snagit text is too large.".into());
        }
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| "Invalid Snagit rich text.")?;
        if bytes.len() > MAX_RTF_BYTES {
            return Err("Snagit text is too large.".into());
        }
        return rtf::plain_text(&bytes);
    }
    if let Some(text) = o.get("Text").and_then(Value::as_str) {
        return Ok(text.into());
    }
    Ok(String::new())
}
fn shape_kind(name: &str) -> Result<AnnotationKind, String> {
    Ok(match name {
        "Rectangle" | "Rect" => AnnotationKind::Rectangle,
        "RoundedRectangle" | "RoundedRect" => AnnotationKind::RoundedRectangle,
        "Ellipse" | "Oval" | "Circle" => AnnotationKind::Ellipse,
        "Triangle" => AnnotationKind::Triangle,
        "Diamond" => AnnotationKind::Diamond,
        "Pentagon" => AnnotationKind::Pentagon,
        "Hexagon" => AnnotationKind::Hexagon,
        "Star" => AnnotationKind::Star,
        "Cross" => AnnotationKind::Cross,
        "Parallelogram" => AnnotationKind::Parallelogram,
        _ => return Err(format!("Snagit shape {name} is not supported yet.")),
    })
}
fn endpoint(
    name: &str,
    warnings: &mut Vec<String>,
) -> Result<crate::arrow_style::Endpoint, String> {
    use crate::arrow_style::Endpoint;
    Ok(match name {
        "None" | "Round" | "Flat" => Endpoint::None,
        "Arrow" | "Triangle" | "FilledArrow" => Endpoint::Triangle,
        "RoundedArrow" => {
            note(
                warnings,
                "Rounded Snagit arrowheads use MyImage's triangular arrowhead.",
            );
            Endpoint::Triangle
        }
        "OpenArrow" | "Open" => Endpoint::Open,
        "Circle" | "FilledCircle" => Endpoint::Circle,
        "Diamond" | "FilledDiamond" => Endpoint::Diamond,
        "Bar" | "Line" => Endpoint::Bar,
        _ => return Err(format!("Snagit line endpoint {name} is not supported yet.")),
    })
}
fn callout_style(
    name: &str,
    warnings: &mut Vec<String>,
) -> Result<crate::callout_style::CalloutStyle, String> {
    use crate::callout_style::CalloutStyle;
    Ok(match name {
        "CTRoundedRect"
        | "CTRoundedRectWithTail"
        | "CTRoundedRectWithArrow"
        | "RoundedRectangle" => CalloutStyle::Rounded,
        "CTRect" | "CTRectWithTail" | "CTRectangle" | "Rectangle" => CalloutStyle::Rectangle,
        "CTEllipse" | "CTOval" | "CTEllipseWithTail" | "Oval" => CalloutStyle::Oval,
        "CTCloud" | "Cloud" => CalloutStyle::Cloud,
        "CTThoughtCloud" | "ThoughtCloud" => CalloutStyle::ThoughtCloud,
        "CTThoughtEllipse" | "ThoughtOval" => CalloutStyle::ThoughtOval,
        "CTArrowLeft" | "ArrowLeft" => CalloutStyle::ArrowLeft,
        "CTArrowRight" | "ArrowRight" => CalloutStyle::ArrowRight,
        "CTDoubleArrow" | "DoubleArrow" => CalloutStyle::DoubleArrow,
        "CTBurst" | "Burst" => CalloutStyle::Burst,
        "CTNote" | "Note" => CalloutStyle::Note,
        "CTNoTailRect" => CalloutStyle::PlainRectangle,
        _ => {
            let _ = warnings;
            return Err(format!("Snagit callout style {name} is not supported yet."));
        }
    })
}

#[cfg(test)]
pub(crate) fn test_project_bytes() -> Vec<u8> {
    tests::project_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use zip::{ZipWriter, write::SimpleFileOptions};
    pub(super) fn archive(
        page: &Value,
        extra: &[(&str, Vec<u8>)],
        method: zip::CompressionMethod,
    ) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(method);
        let index = json!({"Version":"1.0","Pages":["page.json"]});
        for (name, bytes) in [
            ("index.json", serde_json::to_vec(&index).unwrap()),
            ("page.json", serde_json::to_vec(page).unwrap()),
        ]
        .into_iter()
        .chain(extra.iter().map(|(n, b)| (*n, b.clone())))
        {
            zip.start_file(name, options).unwrap();
            zip.write_all(&bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
    fn page(objects: Vec<Value>) -> Value {
        json!({"Version":"1.0","CaptureCanvasWidth":120,"CaptureCanvasHeight":80,"DPI":144,"CaptureBackgroundColor":"#00000000","CaptureObjects":objects,"Effects":[],"Sections":[]})
    }
    fn object(mode: &str) -> Value {
        json!({"ToolMode":mode,"StartPoint":"10,20","EndPoint":"50,40","PointsArray":["10,20","50,40"],"ForegroundColor":"#FF123456","BackgroundColor":"#00000000","StrokeWidth":2,"Opacity":100,"RotationAngle":0,"DashType":"Solid"})
    }
    fn read(p: &Value, extra: &[(&str, Vec<u8>)]) -> Result<ImportResult, String> {
        import_reader(Cursor::new(archive(
            p,
            extra,
            zip::CompressionMethod::Stored,
        )))
    }
    fn png(pixels: &RgbaImage) -> Vec<u8> {
        let mut encoded = Cursor::new(Vec::new());
        pixels
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        encoded.into_inner()
    }
    pub(super) fn project_bytes() -> Vec<u8> {
        let base = RgbaImage::from_pixel(4, 3, Rgba([30, 80, 140, 255]));
        let mut arrow = object("Arrow");
        arrow["StartPoint"] = "1,1".into();
        arrow["EndPoint"] = "3,2".into();
        arrow["PointsArray"] = json!(["1,1", "3,2"]);
        let mut p = page(vec![arrow]);
        p["CaptureCanvasWidth"] = 4.into();
        p["CaptureCanvasHeight"] = 3.into();
        p["DPI"] = 72.into();
        p["CaptureBackgroundImage"] = "base.png".into();
        p["CaptureBackgroundLocation"] = "0,0".into();
        p["BackgroundSize"] = "4,3".into();
        archive(
            &p,
            &[("base.png", png(&base))],
            zip::CompressionMethod::Stored,
        )
    }
    #[test]
    fn editable_objects_keep_retina_geometry_text_colors_and_order() {
        let mut text = object("Text");
        text["RTFEncodedText"] = STANDARD
            .encode(br"{\rtf1\ansi{\fonttbl{\f0 Arial;}}\f0 Hello \u287?\u252?zel}")
            .into();
        text["FontSize"] = 20.into();
        text["TextSelectionColor"] = "#FFAA3300".into();
        text["TextSelectionItalic"] = true.into();
        text["ToolHorizontalAlign"] = "Center".into();
        let mut arrow = object("Arrow");
        arrow["ArrowStart"] = "Circle".into();
        arrow["ArrowEnd"] = "OpenArrow".into();
        arrow["DashType"] = "Dashed".into();
        let mut rect = object("Shape");
        rect["ToolShape"] = "Rectangle".into();
        rect["IsHidden"] = true.into();
        rect["IsLocked"] = 1.into();
        let mut callout = object("Callout");
        callout["StartPoint"] = "0.9,0".into();
        callout["EndPoint"] = "0.9,0".into();
        callout["CalloutTails"] = json!(["5,12"]);
        callout["CalloutShape"] = "CTRoundedRectWithArrow".into();
        callout["Text"] = "Note".into();
        callout["BackgroundColor"] = "#FFCC0000".into();
        let imported = read(&page(vec![text, arrow, rect, callout]), &[]).unwrap();
        assert_eq!(imported.document.output_size(), [240, 160]);
        let a = &imported.document.annotations;
        assert_eq!(a.len(), 4);
        assert_eq!(a[0].kind, AnnotationKind::Text);
        assert_eq!(a[0].text, "Hello ğüzel");
        assert_eq!(a[0].font_size, 40.);
        assert_eq!(a[0].start, Point { x: 20., y: 40. });
        assert_eq!(a[0].color, [170, 51, 0, 255]);
        assert!(a[0].text_style.italic);
        assert_eq!(a[1].kind, AnnotationKind::Arrow);
        assert_eq!(a[1].arrow.start, crate::arrow_style::Endpoint::Circle);
        assert_eq!(a[1].arrow.end, crate::arrow_style::Endpoint::Open);
        assert_eq!(a[1].stroke_width, 4.);
        assert_eq!(a[2].kind, AnnotationKind::Rectangle);
        assert!(a[2].layer.hidden && a[2].layer.locked);
        assert_eq!(a[3].kind, AnnotationKind::Callout);
        assert_eq!(a[3].start, Point { x: 20., y: 40. });
        assert_eq!(a[3].tail, Some(Point { x: 10., y: 24. }));
        let dir = tempfile::tempdir().unwrap();
        let saved = dir.path().join("roundtrip.myimage");
        document::save_project(&imported.document, &saved).unwrap();
        let reopened = document::load_project(&saved).unwrap();
        assert_eq!(reopened.edit_state(), imported.document.edit_state());
    }
    #[test]
    fn full_resolution_background_and_image_layers_are_not_thumbnail_imports() {
        let base = RgbaImage::from_fn(8, 4, |x, y| Rgba([x as u8, y as u8, 200, 255]));
        let mut p = page(vec![]);
        p["CaptureCanvasWidth"] = 8.into();
        p["CaptureCanvasHeight"] = 6.into();
        p["BackgroundSize"] = "4,2".into();
        p["CaptureBackgroundLocation"] = "1,1".into();
        p["CaptureBackgroundImage"] = "base.png".into();
        let mut image = object("Image");
        image["Image"] = STANDARD.encode(png(&base)).into();
        image["StrokeWidth"] = 0.into();
        p["CaptureObjects"] = json!([image]);
        let imported = read(&p, &[("base.png", png(&base))]).unwrap();
        assert_eq!(imported.document.output_size(), [16, 12]);
        assert_eq!(
            imported.document.image.get_pixel(2, 2),
            base.get_pixel(0, 0)
        );
        assert_eq!(
            imported.document.image.get_pixel(9, 5),
            base.get_pixel(7, 3)
        );
        assert_eq!(imported.document.image.get_pixel(0, 0)[3], 0);
        let layer = &imported.document.annotations[0];
        assert_eq!(layer.kind, AnnotationKind::Stamp);
        assert_eq!(
            *crate::stamps::raster(layer.stamp.as_ref().unwrap()).unwrap(),
            base
        );
    }
    #[test]
    fn cropped_background_pixels_and_annotation_geometry_survive_reopen_and_growth() {
        let base = RgbaImage::from_fn(8, 6, |x, y| Rgba([x as u8 * 20, y as u8 * 30, 100, 255]));
        let mut arrow = object("Arrow");
        arrow["PointsArray"] = json!(["0,0", "3,2"]);
        let mut p = page(vec![arrow]);
        p["DPI"] = 72.into();
        p["CaptureCanvasWidth"] = 4.into();
        p["CaptureCanvasHeight"] = 3.into();
        p["CaptureBackgroundLocation"] = "-2,-1".into();
        p["BackgroundSize"] = "8,6".into();
        p["CaptureBackgroundImage"] = "base.png".into();
        let imported = read(&p, &[("base.png", png(&base))]).unwrap();
        assert_eq!(imported.document.output_size(), [4, 3]);
        assert_eq!(*imported.document.image, base);
        assert_eq!(
            imported.document.annotations[0].start,
            Point { x: 2., y: 1. }
        );
        assert_eq!(imported.document.annotations[0].end, Point { x: 5., y: 3. });
        let visible = imported
            .document
            .geometry
            .apply(&imported.document.image)
            .unwrap();
        assert_eq!(
            visible,
            image::imageops::crop_imm(&base, 2, 1, 4, 3).to_image()
        );
        let dir = tempfile::tempdir().unwrap();
        let saved = dir.path().join("cropped.myimage");
        document::save_project(&imported.document, &saved).unwrap();
        let mut reopened = document::load_project(&saved).unwrap();
        assert_eq!(reopened.edit_state(), imported.document.edit_state());
        crate::canvas::set_view_rect(
            &mut reopened,
            eframe::egui::Rect::from_min_size(eframe::egui::Pos2::ZERO, eframe::egui::vec2(8., 6.)),
        )
        .unwrap();
        assert_eq!(reopened.geometry.apply(&reopened.image).unwrap(), base);
        p["CaptureBackgroundLocation"] = "-40000,0".into();
        assert!(
            read(&p, &[("base.png", png(&base))])
                .err()
                .unwrap()
                .contains("No pixels were discarded")
        );
    }
    #[test]
    fn deflated_archive_and_two_tone_shapes_keep_all_editable_parts() {
        let mut shape = object("Shape");
        shape["ToolShape"] = "Ellipse".into();
        shape["BackgroundColor"] = "#FF0000FF".into();
        let p = page(vec![shape]);
        let imported = import_reader(Cursor::new(archive(
            &p,
            &[],
            zip::CompressionMethod::Deflated,
        )))
        .unwrap();
        let a = imported.document.annotations;
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].kind, AnnotationKind::Ellipse);
        assert!(a[0].fill);
        assert_eq!(a[0].color, [0, 0, 255, 255]);
        assert!(!a[1].fill);
        assert_eq!(a[1].color, [18, 52, 86, 255]);
    }
    #[test]
    fn highlighter_fill_is_not_duplicated_by_an_outline_layer() {
        let mut highlight = object("Highlighter");
        highlight["BackgroundColor"] = "#FF0000FF".into();
        highlight["ForegroundColor"] = "#FFFF0000".into();
        highlight["Opacity"] = 50.into();
        let imported = read(&page(vec![highlight]), &[]).unwrap();
        assert_eq!(imported.document.annotations.len(), 1);
        assert_eq!(imported.document.annotations[0].color, [0, 0, 255, 128]);
        let pixels = crate::render::flatten(&imported.document).unwrap();
        assert_eq!(pixels.get_pixel(50, 50).0, [0, 0, 255, 128]);
        assert!(
            imported
                .warnings
                .iter()
                .any(|n| n.contains("highlighter outlines"))
        );
    }
    #[test]
    fn unsupported_content_fails_instead_of_discarding_layers() {
        let mut p = page(vec![object("UnknownTool")]);
        assert!(read(&p, &[]).err().unwrap().contains("UnknownTool"));
        p["CaptureObjects"] = json!([]);
        p["Effects"] = json!([{"ToolMode":"Shadow"}]);
        assert!(read(&p, &[]).err().unwrap().contains("whole-image effects"));
        p["Effects"] = json!([]);
        p["Sections"] = json!([{}]);
        assert!(read(&p, &[]).err().unwrap().contains("template sections"));
        let mut rotated = object("Text");
        rotated["RotationAngle"] = 90.into();
        assert!(
            read(&page(vec![rotated]), &[])
                .err()
                .unwrap()
                .contains("rotated")
        );
    }
    #[test]
    fn hostile_members_references_metadata_and_dimensions_are_bounded() {
        let p = page(vec![]);
        for name in ["../escape.png", "/absolute.png", "C:drive.png", "a\\b.png"] {
            assert!(read(&p, &[(name, vec![0])]).is_err());
        }
        let mut p = page(vec![]);
        p["CaptureBackgroundImage"] = "../outside.png".into();
        assert!(read(&p, &[]).err().unwrap().contains("unsafe"));
        p["CaptureBackgroundImage"] = "missing.png".into();
        assert!(read(&p, &[]).err().unwrap().contains("missing"));
        p.as_object_mut().unwrap().remove("CaptureBackgroundImage");
        p["CaptureCanvasWidth"] = 40000.into();
        assert!(read(&p, &[]).is_err());
        assert!(import_reader(Cursor::new(b"not a zip")).is_err());
    }
    #[test]
    fn hidden_flags_groups_priorities_and_object_effects_fail_safely() {
        let mut defaults = object("Shape");
        defaults["ToolShape"] = "Rectangle".into();
        defaults["IsGrouped"] = false.into();
        defaults["GroupID"] = "0".into();
        assert!(read(&page(vec![defaults]), &[]).is_ok());
        let mut hidden = object("Shape");
        hidden["ToolShape"] = "Rectangle".into();
        hidden["IsHidden"] = 1.into();
        assert!(
            read(&page(vec![hidden.clone()]), &[])
                .unwrap()
                .document
                .annotations[0]
                .layer
                .hidden
        );
        hidden["IsHidden"] = "true".into();
        assert!(
            read(&page(vec![hidden]), &[])
                .err()
                .unwrap()
                .contains("IsHidden")
        );
        for (field, value) in [
            ("Effects", json!([{}])),
            ("Children", json!([{}])),
            ("DropZoneGroupId", json!(7)),
            ("ObjectPriority", json!(4)),
        ] {
            let mut a = object("Shape");
            a["ToolShape"] = "Rectangle".into();
            a[field] = value;
            assert!(
                read(&page(vec![a]), &[]).is_err(),
                "Silently accepted unsupported {field}"
            );
        }
        let mut shape = object("Shape");
        shape["ToolShape"] = "Rectangle".into();
        shape["BackgroundColor"] = "#FF0000FF".into();
        let imported = read(&page(vec![shape]), &[]).unwrap();
        assert_eq!(imported.document.annotations.len(), 2);
        let group = imported.document.annotations[0].group_id;
        assert!(group.is_some());
        assert_eq!(imported.document.annotations[1].group_id, group);
    }
    #[test]
    fn standalone_magnify_preserves_zoom_outline_shape_and_editability() {
        let mut lens = object("Magnify");
        lens["ToolShape"] = "Ellipse".into();
        lens["MagnifyScale"] = 250.into();
        lens["MagnifyOffset"] = "0,-0".into();
        let imported = read(&page(vec![lens.clone()]), &[]).unwrap();
        let a = &imported.document.annotations[0];
        assert_eq!(a.kind, AnnotationKind::Magnify);
        assert_eq!(a.start, Point { x: 20., y: 40. });
        assert_eq!(a.stroke_width, 4.);
        assert_eq!(a.studio.zoom, 2.5);
        assert!(a.studio.ellipse);
        assert!(!a.studio.linked);
        assert!(imported.warnings.is_empty());
        let dir = tempfile::tempdir().unwrap();
        let saved = dir.path().join("magnify.myimage");
        document::save_project(&imported.document, &saved).unwrap();
        let mut reopened = document::load_project(&saved).unwrap();
        reopened.annotations[0].studio.zoom = 3.;
        document::save_project(&reopened, &saved).unwrap();
        assert_eq!(
            document::load_project(&saved).unwrap().annotations[0]
                .studio
                .zoom,
            3.
        );
        lens["MagnifyOffset"] = "10,0".into();
        assert!(
            read(&page(vec![lens]), &[])
                .err()
                .unwrap()
                .contains("offsets")
        );
    }
    #[test]
    fn smart_redaction_is_not_exposed_by_importing_the_unredacted_source() {
        let mut p = page(vec![]);
        p["SmartRedactEnabled"] = true.into();
        assert!(read(&p, &[]).err().unwrap().contains("Smart-redacted"));
        p["SmartRedactEnabled"] = false.into();
        p["SmartRedactProperties"] = json!({"SmartRedactEnabled":true});
        assert!(read(&p, &[]).is_err());
        p["SmartRedactProperties"] = json!({"SmartRedactEnabled":"false"});
        assert!(read(&p, &[]).is_err());
    }
    #[test]
    fn malformed_effects_and_nested_geometry_are_not_ignored() {
        for key in ["Effects", "Sections"] {
            let mut p = page(vec![]);
            p[key] = json!({"unexpected":true});
            assert!(read(&p, &[]).err().unwrap().contains(key));
        }
        for key in ["Effects", "Children", "GroupObjects"] {
            let mut a = object("Text");
            a[key] = json!("invalid");
            assert!(read(&page(vec![a]), &[]).err().unwrap().contains(key));
        }
        let mut arrow = object("Arrow");
        arrow["PointsArray"] = json!(["1,1", "5,10", "20,3"]);
        assert!(
            read(&page(vec![arrow]), &[])
                .err()
                .unwrap()
                .contains("multi-point")
        );
    }
    #[test]
    fn oversized_declared_archive_member_is_rejected_before_expansion() {
        let mut bytes = archive(&page(vec![]), &[], zip::CompressionMethod::Stored);
        let header = bytes
            .windows(4)
            .position(|window| window == b"PK\x01\x02")
            .unwrap();
        bytes[header + 24..header + 28].copy_from_slice(&(600u32 * 1024 * 1024).to_le_bytes());
        assert!(import_reader(Cursor::new(bytes)).is_err());
    }
    #[test]
    fn multi_page_and_invalid_unicode_are_not_silently_truncated() {
        let mut z = ZipWriter::new(Cursor::new(Vec::new()));
        z.start_file("index.json", SimpleFileOptions::default())
            .unwrap();
        z.write_all(br#"{"Version":"1.0","Pages":["a.json","b.json"]}"#)
            .unwrap();
        assert!(
            import_reader(z.finish().unwrap())
                .err()
                .unwrap()
                .contains("Multi-page")
        );
        let mut text = object("Text");
        text["RTFEncodedText"] = STANDARD.encode(br"{\rtf1\u-10179?}").into();
        assert!(read(&page(vec![text]), &[]).is_err());
    }
    #[test]
    #[ignore = "Requires explicitly supplied synthetic Snagit-generated fixtures"]
    fn native_snagit_fixture_import_keeps_layers_and_pixels() {
        let source = std::env::var_os("MYIMAGE_SNAGX_FIXTURE")
            .expect("Set MYIMAGE_SNAGX_FIXTURE to a synthetic .snagx fixture");
        let imported = import(Path::new(&source)).unwrap();
        if let Some(png) = std::env::var_os("MYIMAGE_SNAGX_REFERENCE_PNG") {
            let expected = image::open(png).unwrap().to_rgba8();
            assert_eq!(*imported.document.image, expected);
        }
        if let Ok(count) = std::env::var("MYIMAGE_SNAGX_EXPECT_OBJECTS") {
            assert_eq!(
                imported.document.annotations.len(),
                count.parse::<usize>().unwrap()
            );
        }
        let dir = tempfile::tempdir().unwrap();
        let saved = dir.path().join("imported.myimage");
        document::save_project(&imported.document, &saved).unwrap();
        let reopened = document::load_project(&saved).unwrap();
        assert_eq!(reopened.edit_state(), imported.document.edit_state());
        assert_eq!(*reopened.image, *imported.document.image);
        println!(
            "Imported {}×{} with {} editable layers, {} disclosed style notes",
            imported.document.output_size()[0],
            imported.document.output_size()[1],
            imported.document.annotations.len(),
            imported.warnings.len()
        );
    }
}
