//! Annotation coordinates are always pixels of the original image.
//!
//! The editor transforms that coordinate space only when painting. Export uses
//! the same geometry, and egui's CPU font atlas, without requiring a GPU/window.

use std::{
    cell::RefCell,
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use eframe::egui::{self, Color32, FontId, Pos2, Rect, Shape, Stroke, Vec2};
use image::RgbaImage;
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Transform};

use crate::document::{Annotation, AnnotationKind, Document, Point};

#[path = "text_path_render.rs"]
mod text_path_render;

const TEXT_ATLAS_SIDE: usize = 4096;
const MAX_TEXT_RASTER_PIXELS: usize = 4 * 1024 * 1024;
const MAX_TEXT_CACHE_BYTES: usize = 8 * 1024 * 1024;
const MAX_PREVIEW_CACHE_BYTES: usize = 32 * 1024 * 1024;
const MAX_TEXT_CACHE_ENTRIES: usize = 512;
const MAX_INK_PREVIEW_PIXELS: f32 = 1024.0 * 1024.0;
const TEXT_ALPHA: egui::epaint::AlphaFromCoverage = egui::epaint::AlphaFromCoverage::Linear;

thread_local! {
    // The UI and export worker each own a CPU font cache. They never share a GPU
    // texture or hold a context lock while saving a large image.
    static FONTS: RefCell<egui::epaint::text::Fonts> = RefCell::new(
        egui::epaint::text::Fonts::new(
            TEXT_ATLAS_SIDE,
            TEXT_ALPHA,
            egui::FontDefinitions::default(),
        )
    );
    static TEXT_CACHE: RefCell<TextCache> = RefCell::new(TextCache::default());
}

#[derive(Clone, PartialEq, Eq)]
struct TextKey {
    text: String,
    font_size: u32,
    kind: u8,
    size: [u32; 2],
    number: u32,
    style: crate::text_style::TextStyle,
    callout_style: crate::callout_style::CalloutStyle,
}

impl TextKey {
    fn matches(&self, annotation: &Annotation) -> bool {
        self.style == annotation.text_style
            && self.callout_style == annotation.callout_style
            && self.text == annotation.text
            && self.font_size == annotation.font_size.to_bits()
            && self.kind == Self::kind(annotation)
            && self.size == Self::size(annotation)
            && self.number
                == if annotation.kind == AnnotationKind::Step {
                    annotation.step_number
                } else {
                    0
                }
    }

    fn new(annotation: &Annotation) -> Self {
        Self {
            text: annotation.text.clone(),
            style: annotation.text_style.clone(),
            callout_style: annotation.callout_style,
            font_size: annotation.font_size.to_bits(),
            kind: Self::kind(annotation),
            size: Self::size(annotation),
            number: if annotation.kind == AnnotationKind::Step {
                annotation.step_number
            } else {
                0
            },
        }
    }
    fn kind(annotation: &Annotation) -> u8 {
        match annotation.kind {
            AnnotationKind::Step => 1,
            AnnotationKind::Callout => 2,
            _ => 0,
        }
    }
    fn size(annotation: &Annotation) -> [u32; 2] {
        if matches!(
            annotation.kind,
            AnnotationKind::Step | AnnotationKind::Callout
        ) {
            let size = Rect::from_two_pos(pos(annotation.start), pos(annotation.end)).size();
            [size.x.to_bits(), size.y.to_bits()]
        } else {
            [0; 2]
        }
    }
}

struct TextRaster {
    bounds: Rect,
    offset: Vec2,
    size: [usize; 2],
    // White glyph coverage; color is applied when compositing/painting. Keeping
    // alpha alone makes changing annotation color cheap and keeps caches small.
    alpha: Vec<u8>,
}

struct CachedText {
    key: TextKey,
    raster: Result<Arc<TextRaster>, String>,
    bytes: usize,
}

#[derive(Default)]
struct TextCache {
    entries: VecDeque<CachedText>,
    bytes: usize,
}

struct PreviewText {
    key: TextKey,
    texture: egui::TextureHandle,
    bytes: usize,
    frame: u64,
}

#[derive(Default)]
struct PreviewCache {
    entries: VecDeque<PreviewText>,
    bytes: usize,
}

struct PreviewInk {
    points: Vec<Point>,
    width: f32,
    rect: Rect,
    texture: egui::TextureHandle,
    bytes: usize,
    frame: u64,
}

#[derive(Default)]
struct InkPreviewCache {
    entries: VecDeque<PreviewInk>,
    bytes: usize,
}

fn text_raster(annotation: &Annotation) -> Result<Arc<TextRaster>, String> {
    TEXT_CACHE.with_borrow_mut(|cache| {
        if let Some(index) = cache
            .entries
            .iter()
            .position(|entry| entry.key.matches(annotation))
        {
            let entry = cache.entries.remove(index).expect("cache position exists");
            let raster = entry.raster.clone();
            cache.entries.push_front(entry);
            return raster;
        }
        let raster = build_text_raster(annotation).map(Arc::new);
        let bytes = annotation.text.len() + raster.as_ref().map_or(0, |raster| raster.alpha.len());
        while cache.bytes + bytes > MAX_TEXT_CACHE_BYTES
            || cache.entries.len() >= MAX_TEXT_CACHE_ENTRIES
        {
            let Some(entry) = cache.entries.pop_back() else {
                break;
            };
            cache.bytes -= entry.bytes;
        }
        cache.bytes += bytes;
        cache.entries.push_front(CachedText {
            key: TextKey::new(annotation),
            raster: raster.clone(),
            bytes,
        });
        raster
    })
}

fn pos(point: Point) -> Pos2 {
    egui::pos2(point.x, point.y)
}

fn color(annotation: &Annotation) -> Color32 {
    let [r, g, b, a] = annotation.color;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn text_anchor(annotation: &Annotation) -> Pos2 {
    if matches!(
        annotation.kind,
        AnnotationKind::Step | AnnotationKind::Callout
    ) {
        Rect::from_two_pos(pos(annotation.start), pos(annotation.end)).min
    } else {
        pos(annotation.start)
    }
}

fn text_ink(annotation: &Annotation) -> [u8; 4] {
    if matches!(
        annotation.kind,
        AnnotationKind::Step | AnnotationKind::Callout
    ) {
        annotation.text_color
    } else {
        annotation.color
    }
}

fn has_text(annotation: &Annotation) -> bool {
    matches!(
        annotation.kind,
        AnnotationKind::Text | AnnotationKind::Step | AnnotationKind::Callout
    )
}

fn text_layout(
    fonts: &mut egui::epaint::text::Fonts,
    annotation: &Annotation,
) -> Result<(Arc<egui::Galley>, Vec2), String> {
    if annotation.kind == AnnotationKind::Text {
        let galley = styled_layout(fonts, annotation, annotation.font_size, f32::INFINITY);
        let shift = -galley.rect.min.to_vec2();
        return Ok((galley, shift));
    }
    let original_body = Rect::from_two_pos(pos(annotation.start), pos(annotation.end));
    let body = if annotation.kind == AnnotationKind::Callout {
        crate::callout_style::text_body(annotation)
    } else {
        original_body
    };
    let step = annotation.kind == AnnotationKind::Step;
    let diameter = body.width().min(body.height());
    let padding = if step {
        diameter * 0.17
    } else {
        (body.width().min(body.height()) * 0.12).min(14.0)
    };
    let available = if step {
        Vec2::splat(diameter - padding * 2.0)
    } else {
        body.size() - Vec2::splat(padding * 2.0)
    };
    if available.min_elem() < 1.0 {
        return Ok((
            fonts.with_pixels_per_point(1.0).layout_no_wrap(
                String::new(),
                FontId::proportional(1.0),
                Color32::WHITE,
            ),
            Vec2::ZERO,
        ));
    }
    let text = if step {
        annotation.step_number.to_string()
    } else {
        annotation.text.clone()
    };
    let desired = if step {
        (diameter * 0.6).min(512.0)
    } else {
        annotation.font_size
    };
    let layout = |fonts: &mut egui::epaint::text::Fonts, size| {
        if step {
            fonts.with_pixels_per_point(1.0).layout_no_wrap(
                text.clone(),
                FontId::proportional(size),
                Color32::WHITE,
            )
        } else {
            styled_layout(fonts, annotation, size, available.x)
        }
    };
    let fits = |galley: &egui::Galley| {
        let bounds = galley.rect.union(galley.mesh_bounds);
        bounds.width() <= available.x && bounds.height() <= available.y
    };
    let mut galley = layout(fonts, desired.max(1.0));
    if !fits(&galley) {
        let mut low = 1.0;
        let mut high = desired.max(1.0);
        galley = layout(fonts, low);
        if !fits(&galley) {
            return Err(if step {
                "This marker is too small for its number. Enlarge it."
            } else {
                "This callout is too small for its text. Enlarge the bubble or shorten the text."
            }
            .into());
        }
        for _ in 0..10 {
            let mid = (low + high) * 0.5;
            let candidate = layout(fonts, mid);
            if fits(&candidate) {
                low = mid;
                galley = candidate;
            } else {
                high = mid;
            }
        }
    }
    let bounds = galley.rect.union(galley.mesh_bounds);
    let shift =
        (body.size() - bounds.size()) * 0.5 - bounds.min.to_vec2() + (body.min - original_body.min);
    Ok((galley, shift))
}

/// Metrics from the same layout used by saved and exported annotation text.
pub(crate) struct EditableTextLayout {
    pub origin: Pos2,
    pub size: Vec2,
    pub font_size: f32,
    pub wrap_width: f32,
}
pub(crate) fn editable_text_layout(annotation: &Annotation) -> Result<EditableTextLayout, String> {
    FONTS.with_borrow_mut(|fonts| {
        fonts.begin_pass(TEXT_ATLAS_SIDE, TEXT_ALPHA);
        let (galley, shift) = text_layout(fonts, annotation)?;
        Ok(EditableTextLayout {
            origin: text_anchor(annotation) + shift + galley.rect.min.to_vec2(),
            size: galley.size(),
            font_size: galley
                .job
                .sections
                .first()
                .map_or(annotation.font_size, |s| s.format.font_id.size),
            wrap_width: galley.job.wrap.max_width,
        })
    })
}

fn styled_layout(
    fonts: &mut egui::epaint::text::Fonts,
    annotation: &Annotation,
    size: f32,
    width: f32,
) -> Arc<egui::Galley> {
    let style = &annotation.text_style;
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = width;
    job.halign = match style.alignment {
        crate::text_style::Alignment::Left => egui::Align::Min,
        crate::text_style::Alignment::Center => egui::Align::Center,
        crate::text_style::Alignment::Right => egui::Align::Max,
    };
    job.append(
        &annotation.text,
        0.,
        egui::TextFormat {
            font_id: if style.monospace {
                FontId::monospace(size)
            } else {
                FontId::proportional(size)
            },
            color: Color32::WHITE,
            italics: style.italic,
            underline: if style.underline {
                Stroke::new((size / 18.).max(1.), Color32::WHITE)
            } else {
                Stroke::NONE
            },
            ..Default::default()
        },
    );
    fonts.with_pixels_per_point(1.0).layout_job(job)
}

fn callout_radius(rect: Rect) -> f32 {
    (rect.width().min(rect.height()) * 0.18).clamp(0.0, 16.0)
}

/// Tail tip and base vertices, in clockwise bubble-edge order. An interior tip
/// produces a degenerate triangle, so a tail never folds across its own bubble.
pub fn callout_tail(annotation: &Annotation) -> [Pos2; 3] {
    let rect = Rect::from_two_pos(pos(annotation.start), pos(annotation.end));
    let tip = annotation.tail.map(pos).unwrap_or(rect.center());
    if !rect.is_positive() || rect.contains(tip) {
        return [tip; 3];
    }
    let delta = tip - rect.center();
    let radius = callout_radius(rect);
    let half = (rect.width().min(rect.height()) * 0.16).min(18.0);
    if delta.x.abs() / rect.width() > delta.y.abs() / rect.height() {
        let x = if delta.x > 0.0 {
            rect.right()
        } else {
            rect.left()
        };
        let y = (rect.center().y + delta.y * ((x - rect.center().x) / delta.x))
            .clamp(rect.top() + radius + half, rect.bottom() - radius - half);
        if delta.x > 0.0 {
            [tip, egui::pos2(x, y - half), egui::pos2(x, y + half)]
        } else {
            [tip, egui::pos2(x, y + half), egui::pos2(x, y - half)]
        }
    } else {
        let y = if delta.y > 0.0 {
            rect.bottom()
        } else {
            rect.top()
        };
        let x = (rect.center().x + delta.x * ((y - rect.center().y) / delta.y))
            .clamp(rect.left() + radius + half, rect.right() - radius - half);
        if delta.y > 0.0 {
            [tip, egui::pos2(x + half, y), egui::pos2(x - half, y)]
        } else {
            [tip, egui::pos2(x - half, y), egui::pos2(x + half, y)]
        }
    }
}

/// One simple outline means translucent body and tail are blended only once.
pub(crate) fn callout_outline(annotation: &Annotation) -> Vec<Pos2> {
    let rect = Rect::from_two_pos(pos(annotation.start), pos(annotation.end));
    if !rect.is_positive() {
        return Vec::new();
    }
    let radius = callout_radius(rect);
    let [tip, base1, base2] = callout_tail(annotation);
    let tail_edge = if base1 == base2 {
        None
    } else if base1.y == rect.top() && base2.y == rect.top() {
        Some(0)
    } else if base1.x == rect.right() && base2.x == rect.right() {
        Some(1)
    } else if base1.y == rect.bottom() && base2.y == rect.bottom() {
        Some(2)
    } else {
        Some(3)
    };
    let mut points = Vec::with_capacity(36);
    let centers = [
        egui::pos2(rect.right() - radius, rect.top() + radius),
        egui::pos2(rect.right() - radius, rect.bottom() - radius),
        egui::pos2(rect.left() + radius, rect.bottom() - radius),
        egui::pos2(rect.left() + radius, rect.top() + radius),
    ];
    points.push(egui::pos2(rect.left() + radius, rect.top()));
    for (edge, center) in centers.into_iter().enumerate() {
        if tail_edge == Some(edge) {
            points.extend([base1, tip, base2]);
        }
        for i in 0..=6 {
            let angle = (edge as f32 - 1.0 + i as f32 / 6.0) * std::f32::consts::FRAC_PI_2;
            points.push(center + egui::vec2(angle.cos(), angle.sin()) * radius);
        }
    }
    points.dedup_by(|a, b| a.distance_sq(*b) < 0.000001);
    if points
        .first()
        .zip(points.last())
        .is_some_and(|(a, b)| a.distance_sq(*b) < 0.000001)
    {
        points.pop();
    }
    points
}

fn polygon_mesh(points: &[Pos2], color: Color32) -> egui::Mesh {
    let mut mesh = egui::Mesh::default();
    for point in points {
        mesh.colored_vertex(*point, color);
    }
    let cross = |a: Pos2, b: Pos2, c: Pos2| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let mut indices: Vec<usize> = (0..points.len()).collect();
    while indices.len() >= 3 {
        let mut found = false;
        for i in 0..indices.len() {
            let a = indices[(i + indices.len() - 1) % indices.len()];
            let b = indices[i];
            let c = indices[(i + 1) % indices.len()];
            if cross(points[a], points[b], points[c]) <= 0.00001 {
                continue;
            }
            let contains = indices.iter().copied().any(|p| {
                p != a
                    && p != b
                    && p != c
                    && cross(points[a], points[b], points[p]) >= 0.0
                    && cross(points[b], points[c], points[p]) >= 0.0
                    && cross(points[c], points[a], points[p]) >= 0.0
            });
            if contains {
                continue;
            }
            mesh.add_triangle(a as u32, b as u32, c as u32);
            indices.remove(i);
            found = true;
            break;
        }
        if !found {
            break;
        }
    }
    mesh
}

/// The selectable extent in original image pixels, including stroke and arrowhead.
pub fn annotation_bounds(annotation: &Annotation) -> Rect {
    match annotation.kind {
        AnnotationKind::Text => text_raster(annotation)
            .map(|raster| {
                let mut bounds = raster.bounds;
                if let Some(path) = &annotation.text_style.path {
                    bounds = bounds.union(
                        Rect::from_points(&path.geometry().points)
                            .expand(f32::from(path.stroke) * 0.5),
                    );
                }
                bounds.translate(pos(annotation.start).to_vec2())
            })
            .unwrap_or_else(|_| {
                Rect::from_min_size(pos(annotation.start), egui::vec2(240.0, 24.0))
            }),
        AnnotationKind::Arrow => {
            let mut bounds = Rect::from_two_pos(pos(annotation.start), pos(annotation.end));
            for part in crate::arrow_style::parts(annotation) {
                for point in part.points {
                    bounds.extend_with(point);
                }
            }
            bounds.expand(annotation.stroke_width * 0.5)
        }
        AnnotationKind::Rectangle
        | AnnotationKind::Ellipse
        | AnnotationKind::RoundedRectangle
        | AnnotationKind::Triangle
        | AnnotationKind::Diamond
        | AnnotationKind::Pentagon
        | AnnotationKind::Hexagon
        | AnnotationKind::Star
        | AnnotationKind::Cross
        | AnnotationKind::Parallelogram
        | AnnotationKind::Cylinder
        | AnnotationKind::Prism
        | AnnotationKind::Cone
        | AnnotationKind::Sphere
        | AnnotationKind::HexagonalPrism
        | AnnotationKind::Torus
        | AnnotationKind::Tetrahedron => {
            Rect::from_two_pos(pos(annotation.start), pos(annotation.end))
                .expand(annotation.stroke_width * 0.5)
        }
        AnnotationKind::Freehand => {
            let mut bounds = Rect::NOTHING;
            for point in &annotation.points {
                bounds.extend_with(pos(*point));
            }
            bounds.expand(annotation.stroke_width * 0.5)
        }
        AnnotationKind::Magnify => {
            let lens = crate::studio::lens_rect(annotation);
            if annotation.studio.linked {
                lens.union(crate::studio::source_rect(annotation))
            } else {
                lens
            }
        }
        AnnotationKind::Fill
        | AnnotationKind::Stamp
        | AnnotationKind::Blur
        | AnnotationKind::Pixelate
        | AnnotationKind::Spotlight
        | AnnotationKind::Simplify
        | AnnotationKind::MoveArea
        | AnnotationKind::DeleteArea => {
            Rect::from_two_pos(pos(annotation.start), pos(annotation.end))
        }
        AnnotationKind::Step | AnnotationKind::Highlighter => {
            Rect::from_two_pos(pos(annotation.start), pos(annotation.end))
        }
        AnnotationKind::Callout => {
            let mut bounds = Rect::from_two_pos(pos(annotation.start), pos(annotation.end));
            for point in crate::callout_style::parts(annotation)
                .into_iter()
                .flatten()
            {
                bounds.extend_with(point);
            }
            bounds
        }
    }
}

/// Paint annotations over an untransformed preview whose top-left pixel is `origin`.
pub fn paint_annotations(
    painter: &egui::Painter,
    annotations: &[Annotation],
    origin: Pos2,
    scale: f32,
) {
    if !scale.is_finite() || scale <= 0.0 {
        return;
    }
    let transform = egui::emath::TSTransform {
        scaling: scale,
        translation: origin.to_vec2(),
    };
    for mut shape in annotation_shapes(
        painter,
        annotations,
        transform.inverse() * painter.clip_rect(),
    ) {
        shape.transform(transform);
        painter.add(shape);
    }
}

/// Draw a texture covering the original image coordinate space through the crop
/// and rotation. This can be the original pixels or a bounded privacy composite
/// with the same aspect ratio. Geometry changes only the quad and texture UVs.
pub fn paint_document_image(
    painter: &egui::Painter,
    texture: egui::TextureId,
    document: &Document,
    rect: Rect,
) {
    let original = [document.image.width(), document.image.height()];
    let source_bounds = document.geometry.canvas.map_or(
        Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(original[0] as f32, original[1] as f32),
        ),
        |c| {
            Rect::from_min_size(
                egui::pos2(c.x as f32, c.y as f32),
                egui::vec2(c.width as f32, c.height as f32),
            )
        },
    );
    let mut mesh = egui::Mesh::with_texture(texture);
    for (left, right) in document.geometry.kept_intervals(original, 0) {
        for (top, bottom) in document.geometry.kept_intervals(original, 1) {
            let source = Rect::from_min_max(
                egui::pos2(left as f32, top as f32),
                egui::pos2(right as f32, bottom as f32),
            );
            let uv = Rect::from_min_max(
                egui::pos2(
                    (source.min.x - source_bounds.left()) / source_bounds.width(),
                    (source.min.y - source_bounds.top()) / source_bounds.height(),
                ),
                egui::pos2(
                    (source.max.x - source_bounds.left()) / source_bounds.width(),
                    (source.max.y - source_bounds.top()) / source_bounds.height(),
                ),
            );
            mesh.add_rect_with_uv(source, uv, Color32::WHITE);
        }
    }
    let output = document.output_size();
    let scale = rect.size() / egui::vec2(output[0] as f32, output[1] as f32);
    for vertex in &mut mesh.vertices {
        let mapped = document.geometry.unwarped().map_point(
            Point {
                x: vertex.pos.x,
                y: vertex.pos.y,
            },
            original,
        );
        vertex.pos = rect.min + egui::vec2(mapped.x, mapped.y) * scale;
    }
    painter
        .with_clip_rect(rect.intersect(painter.clip_rect()))
        .add(Shape::mesh(mesh));
}

/// Transform the annotation meshes, including text texels and stroke outlines,
/// so rotation and non-uniform resize have exactly the same geometry as export.
/// `scale` converts output pixels to logical screen points.
pub fn paint_document_annotations(
    painter: &egui::Painter,
    document: &Document,
    origin: Pos2,
    scale: f32,
) {
    if !scale.is_finite()
        || scale <= 0.0
        || document.geometry.effects.active()
        || document.geometry.has_cutouts()
        || document.geometry.canvas.is_some()
        || document
            .annotations
            .iter()
            .any(|annotation| annotation.visible() && annotation.kind.needs_composite())
    {
        return;
    }
    let output = document.output_size();
    let bounds = Rect::from_min_size(
        origin,
        egui::vec2(output[0] as f32, output[1] as f32) * scale,
    );
    let painter = painter.with_clip_rect(bounds.intersect(painter.clip_rect()));
    if document.geometry == Default::default() {
        paint_annotations(&painter, &document.annotations, origin, scale);
        return;
    }
    let original = [document.image.width(), document.image.height()];
    let crop = document.geometry.crop_rect(original);
    let visible = Rect::from_min_size(
        egui::pos2(crop.x as f32, crop.y as f32),
        egui::vec2(crop.width as f32, crop.height as f32),
    );
    let rotated = document.geometry.rotated_size(original);
    let density = (output[0] as f32 / rotated[0] as f32).max(output[1] as f32 / rotated[1] as f32)
        * scale
        * painter.ctx().pixels_per_point();
    let shapes = annotation_shapes(&painter, &document.annotations, visible)
        .into_iter()
        .map(|shape| egui::epaint::ClippedShape {
            clip_rect: visible,
            shape,
        })
        .collect();
    for primitive in painter.ctx().tessellate(shapes, density) {
        if let egui::epaint::Primitive::Mesh(mut mesh) = primitive.primitive {
            for vertex in &mut mesh.vertices {
                let mapped = document.geometry.map_point(
                    Point {
                        x: vertex.pos.x,
                        y: vertex.pos.y,
                    },
                    original,
                );
                vertex.pos = origin + egui::vec2(mapped.x, mapped.y) * scale;
            }
            painter.add(Shape::mesh(mesh));
        }
    }
}

fn stroke_preview(points: Vec<Pos2>, stroke: Stroke) -> Shape {
    if points.len() == 2 && points[0].distance(points[1]) < 0.1 {
        Shape::circle_filled(
            points[0].lerp(points[1], 0.5),
            stroke.width * 0.5,
            stroke.color,
        )
    } else {
        Shape::line(points, stroke)
    }
}

fn annotation_shapes(
    painter: &egui::Painter,
    annotations: &[Annotation],
    visible: Rect,
) -> Vec<Shape> {
    let mut shapes = Vec::new();
    // Share a fixed texture budget across the visible strokes. A document with
    // many large strokes must not lose its later layers merely because earlier
    // ones consumed the cache. Small dots keep their full native resolution.
    let (ink_count, ink_point_bytes) = annotations
        .iter()
        .filter(|annotation| {
            annotation.visible()
                && annotation.kind == AnnotationKind::Freehand
                && annotation_bounds(annotation)
                    .expand(1.0)
                    .intersects(visible)
        })
        .fold((0, 0), |(count, bytes), annotation| {
            (
                count + 1,
                bytes + annotation.points.len() * std::mem::size_of::<Point>(),
            )
        });
    // Keep half the cache available for a revised layout pass in this frame;
    // old texture IDs cannot be freed while an earlier shape still holds them.
    let ink_pixels =
        ((MAX_PREVIEW_CACHE_BYTES / 2).saturating_sub(ink_point_bytes) / 4 / ink_count.max(1))
            .clamp(1, MAX_INK_PREVIEW_PIXELS as usize);
    for annotation in annotations.iter().filter(|a| a.visible()) {
        let color = color(annotation);
        let patterned = annotation.kind.is_shape()
            && annotation.arrow.pattern != crate::arrow_style::Pattern::Solid;
        let stroke = if patterned {
            Stroke::NONE
        } else {
            Stroke::new(annotation.stroke_width, color)
        };
        let fill = if annotation.fill {
            color
        } else {
            Color32::TRANSPARENT
        };
        let rect = Rect::from_two_pos(pos(annotation.start), pos(annotation.end));
        match annotation.kind {
            AnnotationKind::Step if rect.is_positive() => {
                shapes.push(Shape::circle_filled(
                    rect.center(),
                    rect.width().min(rect.height()) * 0.5,
                    color,
                ));
            }
            AnnotationKind::Callout => {
                for outline in crate::callout_style::parts(annotation) {
                    shapes.push(Shape::mesh(polygon_mesh(&outline, color)));
                }
            }
            AnnotationKind::Highlighter => {
                shapes.push(Shape::rect_filled(rect, 0.0, color));
            }
            _ => {}
        }
        match annotation.kind {
            // Effect documents use the ordered CPU preview, including all their
            // regular annotations. No placeholder may reveal the source here.
            AnnotationKind::Fill
            | AnnotationKind::Blur
            | AnnotationKind::Pixelate
            | AnnotationKind::Magnify
            | AnnotationKind::Spotlight
            | AnnotationKind::Simplify
            | AnnotationKind::Highlighter
            | AnnotationKind::MoveArea
            | AnnotationKind::DeleteArea => {}
            AnnotationKind::Stamp => {
                if visible.intersects(rect)
                    && let Some(source) = &annotation.stamp
                {
                    if let Ok(texture) = crate::stamps::texture(painter.ctx(), source) {
                        shapes.push(Shape::image(
                            texture.id(),
                            rect,
                            Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
                            Color32::from_white_alpha(annotation.color[3]),
                        ));
                    } else {
                        shapes.push(Shape::rect_stroke(
                            rect,
                            0.,
                            Stroke::new(2., Color32::RED),
                            egui::StrokeKind::Inside,
                        ));
                    }
                }
            }
            AnnotationKind::Text | AnnotationKind::Step | AnnotationKind::Callout => {
                if let Some(path) = annotation.text_style.path.as_ref().filter(|p| p.visible) {
                    let [r, g, b, a] = path.color;
                    shapes.push(Shape::line(
                        path.geometry()
                            .points
                            .into_iter()
                            .map(|p| p + pos(annotation.start).to_vec2())
                            .collect(),
                        Stroke::new(
                            f32::from(path.stroke),
                            Color32::from_rgba_unmultiplied(r, g, b, a),
                        ),
                    ));
                }
                // Both preview and export use this canonical pixel raster. Host
                // DPI, editor zoom, and UI theme cannot alter text placement.
                match text_raster(annotation) {
                    Ok(raster) if !raster.alpha.is_empty() => {
                        let rect = Rect::from_min_size(
                            text_anchor(annotation) + raster.offset,
                            egui::vec2(raster.size[0] as f32, raster.size[1] as f32),
                        );
                        if !visible.intersects(rect) {
                            continue;
                        }
                        match preview_texture(painter.ctx(), annotation, &raster) {
                            Ok(texture) => {
                                shapes.push(Shape::image(
                                    texture.id(),
                                    rect,
                                    Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
                                    Color32::from_rgba_unmultiplied(
                                        text_ink(annotation)[0],
                                        text_ink(annotation)[1],
                                        text_ink(annotation)[2],
                                        text_ink(annotation)[3],
                                    ),
                                ));
                            }
                            Err(()) => {
                                let galley = painter.layout_no_wrap(
                                    "Text preview limit reached".into(),
                                    FontId::proportional(14.0),
                                    Color32::RED,
                                );
                                shapes.push(Shape::galley(
                                    pos(annotation.start),
                                    galley,
                                    Color32::RED,
                                ));
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(_) => {
                        let galley = painter.layout_no_wrap(
                            "Text too large to preview".into(),
                            FontId::proportional(14.0),
                            Color32::RED,
                        );
                        shapes.push(Shape::galley(pos(annotation.start), galley, Color32::RED));
                    }
                }
            }
            AnnotationKind::Arrow => {
                for part in crate::arrow_style::parts(annotation) {
                    shapes.push(if part.filled {
                        Shape::convex_polygon(part.points, color, Stroke::NONE)
                    } else {
                        stroke_preview(part.points, stroke)
                    });
                }
            }
            AnnotationKind::Freehand => {
                // A single coverage raster preserves round joins and caps, and
                // applies opacity once even where a stroke crosses itself.
                // Its geometry is still transformed from original pixels.
                let bounds = annotation_bounds(annotation).expand(1.0).intersect(visible);
                if !bounds.is_positive() {
                    continue;
                }
                let rect = Rect::from_min_max(bounds.min.floor(), bounds.max.ceil());
                match ink_preview_texture(painter.ctx(), annotation, rect, ink_pixels) {
                    Ok(texture) => shapes.push(Shape::image(
                        texture.id(),
                        rect,
                        Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        color,
                    )),
                    Err(()) => {
                        let galley = painter.layout_no_wrap(
                            "Drawing preview limit reached".into(),
                            FontId::proportional(14.0),
                            Color32::RED,
                        );
                        shapes.push(Shape::galley(rect.min, galley, Color32::RED));
                    }
                }
            }
            AnnotationKind::RoundedRectangle
            | AnnotationKind::Triangle
            | AnnotationKind::Diamond
            | AnnotationKind::Pentagon
            | AnnotationKind::Hexagon
            | AnnotationKind::Star
            | AnnotationKind::Cross
            | AnnotationKind::Parallelogram
            | AnnotationKind::Cylinder
            | AnnotationKind::Prism
            | AnnotationKind::Cone
            | AnnotationKind::Sphere
            | AnnotationKind::HexagonalPrism
            | AnnotationKind::Torus
            | AnnotationKind::Tetrahedron => {
                let points = crate::shapes::outline(annotation.kind, rect);
                if annotation.fill {
                    let mesh = if annotation.kind == AnnotationKind::Torus {
                        let inner = crate::shapes::torus_hole(rect);
                        let mut mesh = egui::Mesh::default();
                        for (&outer, &inside) in points.iter().zip(&inner) {
                            mesh.colored_vertex(outer, color);
                            mesh.colored_vertex(inside, color);
                        }
                        for i in 0..points.len() {
                            let a = (i * 2) as u32;
                            let b = (((i + 1) % points.len()) * 2) as u32;
                            mesh.add_triangle(a, b, a + 1);
                            mesh.add_triangle(b, b + 1, a + 1);
                        }
                        mesh
                    } else {
                        polygon_mesh(&points, color)
                    };
                    shapes.push(Shape::mesh(mesh));
                }
                shapes.push(Shape::closed_line(points, stroke));
                let detail = crate::shapes::detail_color(
                    annotation.color,
                    annotation.fill && annotation.kind != AnnotationKind::Torus,
                );
                for path in crate::shapes::details(annotation.kind, rect) {
                    if patterned {
                        continue;
                    }
                    shapes.push(Shape::line(
                        path,
                        Stroke::new(
                            annotation.stroke_width,
                            Color32::from_rgba_unmultiplied(
                                detail[0], detail[1], detail[2], detail[3],
                            ),
                        ),
                    ));
                }
            }
            AnnotationKind::Rectangle => {
                shapes.push(Shape::rect_filled(rect, 0.0, fill));
                shapes.push(Shape::rect_stroke(
                    rect,
                    0.0,
                    stroke,
                    egui::StrokeKind::Middle,
                ));
            }
            AnnotationKind::Ellipse => {
                shapes.push(Shape::Ellipse(egui::epaint::EllipseShape {
                    center: rect.center(),
                    radius: rect.size() * 0.5,
                    fill,
                    stroke,
                }));
            }
        }
        if patterned {
            for (path, rgba) in patterned_shape_paths(annotation) {
                shapes.push(stroke_preview(
                    path,
                    Stroke::new(
                        annotation.stroke_width,
                        Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]),
                    ),
                ));
            }
        }
    }
    shapes
}

/// Flatten a copy for export. The original and editable annotations stay intact.
pub fn flatten(document: &Document) -> Result<RgbaImage, String> {
    let resolved = crate::connectors::resolved(document);
    let document = resolved.as_ref();
    crate::document::validate_document(document)?;
    if document.geometry.canvas.is_some() {
        return flatten(&crate::canvas::render_document(document)?);
    }
    if document.annotations.is_empty() && !document.geometry.effects.active() {
        return document.geometry.apply(&document.image);
    }
    let mut output = (*document.image).clone();
    composite_annotations(&mut output, &document.annotations, [1.0; 2])?;
    crate::image_effects::apply(&mut output, document.geometry.effects, [0.; 2], [1.; 2]);
    if document.geometry == Default::default() {
        Ok(output)
    } else {
        document.geometry.apply(&output)
    }
}

/// Bounded, nearest-sampled transition frame. Only this small composition runs
/// on the UI thread; the normal worker replaces it with the antialiased preview.
/// Sample expanded canvases directly, never allocate their full-resolution pixels.
pub fn interactive_preview(document: &Document, max_side: u32) -> Result<RgbaImage, String> {
    crate::document::validate_document(document)?;
    if max_side == 0 {
        return Err("Preview dimensions must be positive.".into());
    }
    let original = [document.image.width(), document.image.height()];
    let bounds = document
        .geometry
        .canvas
        .unwrap_or(crate::geometry::CanvasRect {
            x: 0,
            y: 0,
            width: original[0],
            height: original[1],
        });
    let dimensions = fit_dimensions(
        [bounds.width, bounds.height],
        max_side.min(768),
        max_side.min(768),
    );
    let crop = document.geometry.crop_rect(original);
    let mut output = RgbaImage::from_fn(dimensions[0], dimensions[1], |x, y| {
        let sx = bounds.x as i64
            + ((x as u64 * 2 + 1) * bounds.width as u64 / (dimensions[0] as u64 * 2)) as i64;
        let sy = bounds.y as i64
            + ((y as u64 * 2 + 1) * bounds.height as u64 / (dimensions[1] as u64 * 2)) as i64;
        if sx < 0
            || sy < 0
            || sx >= original[0] as i64
            || sy >= original[1] as i64
            || (document.geometry.canvas.is_some()
                && (sx < crop.x as i64
                    || sy < crop.y as i64
                    || sx >= (crop.x + crop.width) as i64
                    || sy >= (crop.y + crop.height) as i64))
        {
            image::Rgba([0; 4])
        } else {
            *document.image.get_pixel(sx as u32, sy as u32)
        }
    });
    let mut annotations = document.annotations.clone();
    if document.geometry.canvas.is_some() {
        for a in &mut annotations {
            let shift = |p: &mut Point| {
                p.x -= bounds.x as f32;
                p.y -= bounds.y as f32;
            };
            shift(&mut a.start);
            shift(&mut a.end);
            if let Some(controls) = &mut a.arrow.curve {
                for control in controls {
                    shift(control);
                }
            }
            if let Some(tail) = &mut a.tail {
                shift(tail);
            }
            for p in &mut a.points {
                shift(p);
            }
            if let Some(source) = &mut a.area_source {
                for p in source {
                    shift(p);
                }
            }
        }
    }
    composite_annotations(
        &mut output,
        &annotations,
        [
            dimensions[0] as f32 / bounds.width as f32,
            dimensions[1] as f32 / bounds.height as f32,
        ],
    )?;
    crate::image_effects::apply(
        &mut output,
        document.geometry.effects,
        [bounds.x as f32, bounds.y as f32],
        [
            dimensions[0] as f32 / bounds.width as f32,
            dimensions[1] as f32 / bounds.height as f32,
        ],
    );
    Ok(output)
}

/// Render the complete ordered stack into a bounded texture in original image
/// coordinates. Geometry remains a GPU transform, so a crop or rotation cannot
/// briefly expose unfiltered source pixels. Call on the preview worker.
pub fn privacy_preview(document: &Document, max_side: u32) -> Result<RgbaImage, String> {
    crate::document::validate_document(document)?;
    if document.geometry.canvas.is_some() {
        return privacy_preview(&crate::canvas::render_document(document)?, max_side);
    }
    if max_side == 0 {
        return Err("Preview dimensions must be positive.".into());
    }
    let max_side = max_side.min(4096);
    let original = [document.image.width(), document.image.height()];
    let dimensions = fit_dimensions(original, max_side, max_side);
    let mut output = if dimensions == original {
        (*document.image).clone()
    } else {
        sample_view(
            &ThumbnailSource {
                image: &document.image,
                crop: [0, 0, original[0], original[1]],
                turns: 0,
            },
            dimensions,
        )
    };
    let scale = [
        dimensions[0] as f32 / original[0] as f32,
        dimensions[1] as f32 / original[1] as f32,
    ];
    composite_annotations(&mut output, &document.annotations, scale)?;
    crate::image_effects::apply(&mut output, document.geometry.effects, [0.; 2], scale);
    Ok(output)
}

fn composite_annotations(
    output: &mut RgbaImage,
    annotations: &[Annotation],
    scale: [f32; 2],
) -> Result<(), String> {
    // A privacy-only document needs no shape/text buffer. In particular, avoid
    // allocating another full-resolution RGBA image merely to blur a selection.
    let mut overlay: Option<Pixmap> = None;
    let mut pending = false;
    let transform = Transform::from_scale(scale[0], scale[1]);
    for annotation in annotations.iter().filter(|a| a.visible()) {
        if annotation.kind.needs_composite() {
            if pending {
                let overlay = overlay
                    .as_mut()
                    .expect("pending annotations have an overlay");
                composite_overlay(output, overlay);
                overlay.fill(tiny_skia::Color::TRANSPARENT);
                pending = false;
            }
            if annotation.kind.is_studio() {
                crate::studio::apply(output, annotation, scale)?;
            } else if annotation.kind == AnnotationKind::Fill {
                crate::fill::apply(output, annotation, scale)?;
            } else if annotation.kind.is_area_edit() {
                crate::area_pixels::apply(output, annotation, scale)?;
            } else if scale == [1.0; 2] {
                crate::area_pixels::apply_filter(output, annotation)?;
            } else {
                let mut scaled = annotation.clone();
                scaled.start.x *= scale[0];
                scaled.start.y *= scale[1];
                scaled.end.x *= scale[0];
                scaled.end.y *= scale[1];
                // Never reduce a privacy filter below its supported minimum.
                // Rounding up makes small previews conservatively stronger.
                scaled.effect_size = (annotation.effect_size as f32 * scale[0].max(scale[1]))
                    .ceil()
                    .max(2.0) as u32;
                crate::area_pixels::apply_filter(output, &scaled)?;
            }
        } else {
            if overlay.is_none() {
                overlay =
                    Some(Pixmap::new(output.width(), output.height()).ok_or_else(|| {
                        "The image is too large to render annotations.".to_owned()
                    })?);
            }
            let overlay = overlay
                .as_mut()
                .expect("the annotation overlay was allocated");
            if annotation.kind == AnnotationKind::Stamp {
                paint_stamp(overlay, annotation, transform)?;
            } else if annotation.kind != AnnotationKind::Text {
                paint_shape(overlay, annotation, transform);
            }
            if has_text(annotation) {
                if scale == [1.0; 2] {
                    paint_text(overlay, annotation)?;
                } else {
                    // Sample the canonical source glyph raster: scaling font
                    // size and laying it out again would shift line geometry.
                    paint_text_transformed(overlay, annotation, transform)?;
                }
            }
            pending = true;
        }
    }
    if pending {
        composite_overlay(
            output,
            overlay
                .as_ref()
                .expect("pending annotations have an overlay"),
        );
    }
    Ok(())
}

/// A bounded project preview. The original pixels are sampled through a view,
/// and annotations are rendered directly into the small destination. A large
/// capture therefore never needs a second full-resolution image or overlay.
pub fn thumbnail(document: &Document, width: u32, height: u32) -> Result<RgbaImage, String> {
    crate::document::validate_document(document)?;
    if document.geometry.canvas.is_some() {
        return thumbnail(&crate::canvas::render_document(document)?, width, height);
    }
    if width == 0 || height == 0 {
        return Err("Thumbnail dimensions must be positive.".into());
    }
    if document.geometry.flip_horizontal || document.geometry.flip_vertical {
        let mut unmirrored = document.clone();
        unmirrored.geometry.flip_horizontal = false;
        unmirrored.geometry.flip_vertical = false;
        let mut pixels = thumbnail(&unmirrored, width, height)?;
        if document.geometry.flip_horizontal {
            image::imageops::flip_horizontal_in_place(&mut pixels);
        }
        if document.geometry.flip_vertical {
            image::imageops::flip_vertical_in_place(&mut pixels);
        }
        return Ok(pixels);
    }
    if document.geometry.effects.active()
        || document.geometry.has_cutouts()
        || document
            .annotations
            .iter()
            .any(|annotation| annotation.visible() && annotation.kind.needs_composite())
    {
        return privacy_thumbnail(document, width, height);
    }
    let original = [document.image.width(), document.image.height()];
    let output = document.output_size();
    let working = fit_dimensions(output, width.saturating_mul(4), height.saturating_mul(4));
    let crop = document.geometry.crop_rect(original);
    let source = ThumbnailSource {
        image: &document.image,
        crop: [crop.x, crop.y, crop.width, crop.height],
        turns: document.geometry.quarter_turns,
    };
    let sampled = image::imageops::thumbnail(&source, working[0], working[1]);
    let mut pixels = RgbaImage::from_fn(working[0], working[1], |x, y| {
        let pixel = sampled.get_pixel(x, y);
        let alpha = pixel[3];
        if alpha == 0 {
            return image::Rgba([0; 4]);
        }
        image::Rgba([
            ((pixel[0] * 255 + alpha / 2) / alpha).min(255) as u8,
            ((pixel[1] * 255 + alpha / 2) / alpha).min(255) as u8,
            ((pixel[2] * 255 + alpha / 2) / alpha).min(255) as u8,
            ((alpha + 127) / 255).min(255) as u8,
        ])
    });
    if !document.annotations.is_empty() {
        let factor = egui::vec2(
            working[0] as f32 / output[0] as f32,
            working[1] as f32 / output[1] as f32,
        );
        let map = |point| {
            let mapped = document.geometry.map_point(point, original);
            egui::vec2(mapped.x, mapped.y) * factor
        };
        let origin = map(Point { x: 0.0, y: 0.0 });
        let x = map(Point { x: 1.0, y: 0.0 }) - origin;
        let y = map(Point { x: 0.0, y: 1.0 }) - origin;
        let transform = Transform::from_row(x.x, x.y, y.x, y.y, origin.x, origin.y);
        let mut overlay = Pixmap::new(working[0], working[1])
            .ok_or_else(|| "The thumbnail is too large to render.".to_owned())?;
        for annotation in document.annotations.iter().filter(|a| a.visible()) {
            if annotation.kind == AnnotationKind::Stamp {
                paint_stamp(&mut overlay, annotation, transform)?;
            } else if annotation.kind != AnnotationKind::Text {
                paint_shape(&mut overlay, annotation, transform);
            }
            if has_text(annotation) {
                paint_text_transformed(&mut overlay, annotation, transform)?;
            }
        }
        composite_overlay(&mut pixels, &overlay);
    }
    let target = fit_dimensions(output, width, height);
    if target == working {
        return Ok(pixels);
    }
    // The final reduction also uses premultiplied samples, protecting rounded
    // captures and semi-transparent annotation edges from dark halos.
    let source = ThumbnailSource {
        image: &pixels,
        crop: [0, 0, working[0], working[1]],
        turns: 0,
    };
    let sampled = image::imageops::thumbnail(&source, target[0], target[1]);
    Ok(RgbaImage::from_fn(target[0], target[1], |x, y| {
        let pixel = sampled.get_pixel(x, y);
        let alpha = pixel[3];
        if alpha == 0 {
            return image::Rgba([0; 4]);
        }
        image::Rgba([
            ((pixel[0] * 255 + alpha / 2) / alpha).min(255) as u8,
            ((pixel[1] * 255 + alpha / 2) / alpha).min(255) as u8,
            ((pixel[2] * 255 + alpha / 2) / alpha).min(255) as u8,
            ((alpha + 127) / 255).min(255) as u8,
        ])
    }))
}

fn privacy_thumbnail(document: &Document, width: u32, height: u32) -> Result<RgbaImage, String> {
    // Apply privacy before cropping or rotating, including ordinary annotations
    // beneath each filter. Failure propagates: never fall back to the original.
    let composite = privacy_preview(document, width.max(height).saturating_mul(4).min(4096))?;
    let original = [document.image.width(), document.image.height()];
    if document.geometry.has_cutouts() {
        let [w, h] = fit_dimensions(document.output_size(), width, height);
        let output = document.output_size();
        return Ok(RgbaImage::from_fn(w, h, |x, y| {
            let p = document.geometry.unwarped().inverse_point(
                Point {
                    x: (x as f32 + 0.5) * output[0] as f32 / w as f32,
                    y: (y as f32 + 0.5) * output[1] as f32 / h as f32,
                },
                original,
            );
            *composite.get_pixel(
                (p.x * composite.width() as f32 / original[0] as f32)
                    .floor()
                    .clamp(0., (composite.width() - 1) as f32) as u32,
                (p.y * composite.height() as f32 / original[1] as f32)
                    .floor()
                    .clamp(0., (composite.height() - 1) as f32) as u32,
            )
        }));
    }
    let crop = document.geometry.crop_rect(original);
    let scale = [
        composite.width() as f64 / original[0] as f64,
        composite.height() as f64 / original[1] as f64,
    ];
    let left = (f64::from(crop.x) * scale[0]).floor() as u32;
    let top = (f64::from(crop.y) * scale[1]).floor() as u32;
    let right = (f64::from(crop.x + crop.width) * scale[0]).ceil() as u32;
    let bottom = (f64::from(crop.y + crop.height) * scale[1]).ceil() as u32;
    let source = ThumbnailSource {
        image: &composite,
        crop: [
            left,
            top,
            right.min(composite.width()) - left,
            bottom.min(composite.height()) - top,
        ],
        turns: document.geometry.quarter_turns,
    };
    Ok(sample_view(
        &source,
        fit_dimensions(document.output_size(), width, height),
    ))
}

fn sample_view(source: &ThumbnailSource<'_>, dimensions: [u32; 2]) -> RgbaImage {
    let sampled = image::imageops::thumbnail(source, dimensions[0], dimensions[1]);
    RgbaImage::from_fn(dimensions[0], dimensions[1], |x, y| {
        let pixel = sampled.get_pixel(x, y);
        let alpha = pixel[3];
        if alpha == 0 {
            return image::Rgba([0; 4]);
        }
        image::Rgba([
            ((pixel[0] * 255 + alpha / 2) / alpha).min(255) as u8,
            ((pixel[1] * 255 + alpha / 2) / alpha).min(255) as u8,
            ((pixel[2] * 255 + alpha / 2) / alpha).min(255) as u8,
            ((alpha + 127) / 255).min(255) as u8,
        ])
    })
}

fn fit_dimensions(size: [u32; 2], width: u32, height: u32) -> [u32; 2] {
    let scale = (f64::from(width) / f64::from(size[0]))
        .min(f64::from(height) / f64::from(size[1]))
        .min(1.0);
    [
        (f64::from(size[0]) * scale).round().max(1.0) as u32,
        (f64::from(size[1]) * scale).round().max(1.0) as u32,
    ]
}

struct ThumbnailSource<'a> {
    image: &'a RgbaImage,
    crop: [u32; 4],
    turns: u8,
}

impl image::GenericImageView for ThumbnailSource<'_> {
    // 32-bit values retain exact 8-bit channel * alpha products, and the image
    // library accumulates these in u64 even when a whole 100MP image is sampled.
    type Pixel = image::Rgba<u32>;

    fn dimensions(&self) -> (u32, u32) {
        if self.turns.is_multiple_of(2) {
            (self.crop[2], self.crop[3])
        } else {
            (self.crop[3], self.crop[2])
        }
    }

    fn get_pixel(&self, x: u32, y: u32) -> Self::Pixel {
        let [left, top, width, height] = self.crop;
        let (x, y) = match self.turns % 4 {
            1 => (y, height - 1 - x),
            2 => (width - 1 - x, height - 1 - y),
            3 => (width - 1 - y, x),
            _ => (x, y),
        };
        let pixel = self.image.get_pixel(left + x, top + y);
        let alpha = u32::from(pixel[3]);
        image::Rgba([
            u32::from(pixel[0]) * alpha,
            u32::from(pixel[1]) * alpha,
            u32::from(pixel[2]) * alpha,
            alpha * 255,
        ])
    }
}

fn paint_stamp(
    overlay: &mut Pixmap,
    annotation: &Annotation,
    transform: Transform,
) -> Result<(), String> {
    let source = annotation.stamp.as_ref().ok_or("Missing stamp artwork.")?;
    let rect = Rect::from_two_pos(pos(annotation.start), pos(annotation.end));
    if !rect.is_positive() {
        return Ok(());
    }
    let image = if let crate::stamps::StampSource::Svg { svg, .. } = source {
        let scale = transform
            .sx
            .hypot(transform.ky)
            .max(transform.sy.hypot(transform.kx));
        std::sync::Arc::new(crate::svg_stamp::raster(
            svg,
            (rect.width().max(rect.height()) * scale)
                .ceil()
                .clamp(16., 2048.) as u32,
        )?)
    } else {
        crate::stamps::raster(source)?
    };
    let mut pixmap = Pixmap::new(image.width(), image.height()).ok_or("Invalid stamp size.")?;
    for (dest, pixel) in pixmap.pixels_mut().iter_mut().zip(image.pixels()) {
        let a = pixel[3] as u32;
        *dest = tiny_skia::PremultipliedColorU8::from_rgba(
            ((pixel[0] as u32 * a + 127) / 255) as u8,
            ((pixel[1] as u32 * a + 127) / 255) as u8,
            ((pixel[2] as u32 * a + 127) / 255) as u8,
            a as u8,
        )
        .unwrap();
    }
    let map = transform.pre_translate(rect.min.x, rect.min.y).pre_scale(
        rect.width() / image.width() as f32,
        rect.height() / image.height() as f32,
    );
    overlay.draw_pixmap(
        0,
        0,
        pixmap.as_ref(),
        &tiny_skia::PixmapPaint {
            opacity: annotation.color[3] as f32 / 255.,
            quality: tiny_skia::FilterQuality::Bilinear,
            ..Default::default()
        },
        map,
        None,
    );
    Ok(())
}

fn paint_text_transformed(
    overlay: &mut Pixmap,
    annotation: &Annotation,
    transform: Transform,
) -> Result<(), String> {
    text_path_render::paint_path(overlay, annotation, transform);
    let raster = text_raster(annotation)?;
    if raster.alpha.is_empty() {
        return Ok(());
    }
    let rect = Rect::from_min_size(
        text_anchor(annotation) + raster.offset,
        egui::vec2(raster.size[0] as f32, raster.size[1] as f32),
    );
    let mut corners = [
        rect.left_top(),
        rect.right_top(),
        rect.left_bottom(),
        rect.right_bottom(),
    ]
    .map(|point| tiny_skia::Point::from_xy(point.x, point.y));
    transform.map_points(&mut corners);
    let mut bounds = Rect::NOTHING;
    for point in corners {
        bounds.extend_with(egui::pos2(point.x, point.y));
    }
    let Some(inverse) = transform.invert() else {
        return Err("The thumbnail transform is invalid.".into());
    };
    let left = bounds.min.x.floor().clamp(0.0, overlay.width() as f32) as u32;
    let top = bounds.min.y.floor().clamp(0.0, overlay.height() as f32) as u32;
    let right = bounds.max.x.ceil().clamp(0.0, overlay.width() as f32) as u32;
    let bottom = bounds.max.y.ceil().clamp(0.0, overlay.height() as f32) as u32;
    for y in top..bottom {
        for x in left..right {
            let mut point = tiny_skia::Point::from_xy(x as f32 + 0.5, y as f32 + 0.5);
            inverse.map_point(&mut point);
            let sample = egui::pos2(point.x, point.y) - rect.min - Vec2::splat(0.5);
            blend_text_pixel(
                overlay,
                x,
                y,
                text_ink(annotation),
                raster_alpha(&raster, sample),
            );
        }
    }
    Ok(())
}

fn composite_overlay(output: &mut RgbaImage, overlay: &Pixmap) {
    for (pixel, ink) in output.pixels_mut().zip(overlay.pixels()) {
        // Keep untouched original bytes exactly, including hidden RGB in a fully
        // transparent pixel. Premultiplying the base image would lose those bytes.
        if ink.alpha() == 0 {
            continue;
        }
        let alpha = f32::from(ink.alpha()) / 255.0;
        let remaining = (1.0 - alpha) * f32::from(pixel[3]) / 255.0;
        let out_alpha = alpha + remaining;
        for (channel, ink_channel) in [ink.red(), ink.green(), ink.blue()].into_iter().enumerate() {
            pixel[channel] = ((f32::from(ink_channel) + f32::from(pixel[channel]) * remaining)
                / out_alpha)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
        pixel[3] = (out_alpha * 255.0).round() as u8;
    }
}

fn patterned_shape_paths(annotation: &Annotation) -> Vec<(Vec<Pos2>, [u8; 4])> {
    let rect = Rect::from_two_pos(pos(annotation.start), pos(annotation.end));
    let outline = crate::shapes::outline(annotation.kind, rect);
    let mut paths: Vec<_> = crate::arrow_style::patterned_paths(
        &outline,
        true,
        annotation.arrow.pattern,
        annotation.stroke_width,
    )
    .into_iter()
    .map(|p| (p, annotation.color))
    .collect();
    let detail = crate::shapes::detail_color(
        annotation.color,
        annotation.fill && annotation.kind != AnnotationKind::Torus,
    );
    for path in crate::shapes::details(annotation.kind, rect) {
        paths.extend(
            crate::arrow_style::patterned_paths(
                &path,
                annotation.kind == AnnotationKind::Torus,
                annotation.arrow.pattern,
                annotation.stroke_width,
            )
            .into_iter()
            .map(|p| (p, detail)),
        );
    }
    paths
}

fn paint_shape(overlay: &mut Pixmap, annotation: &Annotation, transform: Transform) {
    let [r, g, b, a] = annotation.color;
    let mut paint = Paint::default();
    paint.set_color_rgba8(r, g, b, a);
    paint.anti_alias = true;
    if annotation.kind == AnnotationKind::Freehand {
        paint_freehand(overlay, annotation, &paint, transform);
        return;
    }
    let stroke = tiny_skia::Stroke {
        width: annotation.stroke_width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };
    let rect = Rect::from_two_pos(pos(annotation.start), pos(annotation.end));
    let mut builder = PathBuilder::new();
    if annotation.kind == AnnotationKind::Callout {
        for points in crate::callout_style::parts(annotation) {
            if let Some(first) = points.first() {
                builder.move_to(first.x, first.y);
                for point in points.iter().skip(1) {
                    builder.line_to(point.x, point.y);
                }
                builder.close();
            }
        }
        if let Some(path) = builder.finish() {
            overlay.fill_path(&path, &paint, FillRule::Winding, transform, None);
        }
        return;
    }
    if annotation.kind == AnnotationKind::Arrow {
        for part in crate::arrow_style::parts(annotation) {
            let mut builder = PathBuilder::new();
            if let Some(first) = part.points.first() {
                builder.move_to(first.x, first.y);
                for p in part.points.iter().skip(1) {
                    builder.line_to(p.x, p.y);
                }
                if part.filled {
                    builder.close();
                }
                if let Some(path) = builder.finish() {
                    if part.filled {
                        overlay.fill_path(&path, &paint, FillRule::Winding, transform, None);
                    } else {
                        overlay.stroke_path(&path, &paint, &stroke, transform, None);
                    }
                }
            }
        }
        return;
    }
    let Some(rect) =
        tiny_skia::Rect::from_xywh(rect.min.x, rect.min.y, rect.width(), rect.height())
    else {
        return;
    };
    let path: Option<Path> = match annotation.kind {
        AnnotationKind::RoundedRectangle
        | AnnotationKind::Triangle
        | AnnotationKind::Diamond
        | AnnotationKind::Pentagon
        | AnnotationKind::Hexagon
        | AnnotationKind::Star
        | AnnotationKind::Cross
        | AnnotationKind::Parallelogram
        | AnnotationKind::Cylinder
        | AnnotationKind::Prism
        | AnnotationKind::Cone
        | AnnotationKind::Sphere
        | AnnotationKind::HexagonalPrism
        | AnnotationKind::Torus
        | AnnotationKind::Tetrahedron => {
            let bounds = Rect::from_min_size(
                egui::pos2(rect.x(), rect.y()),
                egui::vec2(rect.width(), rect.height()),
            );
            let points = crate::shapes::outline(annotation.kind, bounds);
            if let Some(first) = points.first() {
                builder.move_to(first.x, first.y);
                for p in &points[1..] {
                    builder.line_to(p.x, p.y);
                }
                builder.close();
                if annotation.kind == AnnotationKind::Torus {
                    let hole = crate::shapes::torus_hole(bounds);
                    builder.move_to(hole[0].x, hole[0].y);
                    for p in hole.iter().skip(1).rev() {
                        builder.line_to(p.x, p.y);
                    }
                    builder.close();
                }
            }
            builder.finish()
        }
        AnnotationKind::Rectangle | AnnotationKind::Highlighter => {
            builder.push_rect(rect);
            builder.finish()
        }
        AnnotationKind::Ellipse => {
            builder.push_oval(rect);
            builder.finish()
        }
        AnnotationKind::Step => {
            let radius = rect.width().min(rect.height()) * 0.5;
            builder.push_circle(
                rect.x() + rect.width() * 0.5,
                rect.y() + rect.height() * 0.5,
                radius,
            );
            builder.finish()
        }
        AnnotationKind::Fill
        | AnnotationKind::Stamp
        | AnnotationKind::Arrow
        | AnnotationKind::Text
        | AnnotationKind::Blur
        | AnnotationKind::Pixelate
        | AnnotationKind::Magnify
        | AnnotationKind::Spotlight
        | AnnotationKind::Simplify
        | AnnotationKind::Freehand
        | AnnotationKind::MoveArea
        | AnnotationKind::DeleteArea
        | AnnotationKind::Callout => None,
    };
    if let Some(path) = path {
        if annotation.fill
            || matches!(
                annotation.kind,
                AnnotationKind::Step | AnnotationKind::Highlighter
            )
        {
            overlay.fill_path(&path, &paint, FillRule::Winding, transform, None);
        }
        if !(matches!(
            annotation.kind,
            AnnotationKind::Step | AnnotationKind::Highlighter
        ) || annotation.kind.is_shape()
            && annotation.arrow.pattern != crate::arrow_style::Pattern::Solid)
        {
            overlay.stroke_path(&path, &paint, &stroke, transform, None);
        }
    }
    if annotation.kind.is_shape() && annotation.arrow.pattern != crate::arrow_style::Pattern::Solid
    {
        for (points, rgba) in patterned_shape_paths(annotation) {
            let mut builder = PathBuilder::new();
            if let Some(first) = points.first() {
                builder.move_to(first.x, first.y);
                for p in points.iter().skip(1) {
                    builder.line_to(p.x, p.y);
                }
            }
            if let Some(path) = builder.finish() {
                let mut stroke_paint = paint.clone();
                stroke_paint.set_color_rgba8(rgba[0], rgba[1], rgba[2], rgba[3]);
                overlay.stroke_path(&path, &stroke_paint, &stroke, transform, None);
            }
        }
    }
    if annotation.kind.is_3d()
        && annotation.kind != AnnotationKind::Torus
        && annotation.arrow.pattern == crate::arrow_style::Pattern::Solid
    {
        let bounds = Rect::from_min_size(
            egui::pos2(rect.x(), rect.y()),
            egui::vec2(rect.width(), rect.height()),
        );
        let color = crate::shapes::detail_color(annotation.color, annotation.fill);
        let mut detail = Paint::default();
        detail.set_color_rgba8(color[0], color[1], color[2], color[3]);
        detail.anti_alias = true;
        for points in crate::shapes::details(annotation.kind, bounds) {
            let mut builder = PathBuilder::new();
            if let Some(first) = points.first() {
                builder.move_to(first.x, first.y);
                for p in &points[1..] {
                    builder.line_to(p.x, p.y);
                }
            }
            if let Some(path) = builder.finish() {
                overlay.stroke_path(&path, &detail, &stroke, transform, None);
            }
        }
    }
}

fn paint_freehand(
    overlay: &mut Pixmap,
    annotation: &Annotation,
    paint: &Paint<'_>,
    transform: Transform,
) {
    let Some(first) = annotation.points.first() else {
        return;
    };
    let mut builder = PathBuilder::new();
    // A click, or repeated stationary input, is a round dot. A degenerate
    // stroked path has backend-dependent cap behavior, so make it explicit.
    if annotation.points.iter().all(|point| point == first) {
        builder.push_circle(first.x, first.y, annotation.stroke_width * 0.5);
        if let Some(path) = builder.finish() {
            overlay.fill_path(&path, paint, FillRule::Winding, transform, None);
        }
        return;
    }
    builder.move_to(first.x, first.y);
    let mut previous = first;
    for point in annotation.points.iter().skip(1) {
        if point != previous {
            builder.line_to(point.x, point.y);
            previous = point;
        }
    }
    if let Some(path) = builder.finish() {
        let stroke = tiny_skia::Stroke {
            width: annotation.stroke_width,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };
        overlay.stroke_path(&path, paint, &stroke, transform, None);
    }
}

fn ink_preview_texture(
    ctx: &egui::Context,
    annotation: &Annotation,
    rect: Rect,
    pixel_budget: usize,
) -> Result<egui::TextureHandle, ()> {
    let limit = ctx.input(|input| input.max_texture_side).clamp(1, 2048) as f32;
    let scale = (limit / rect.width().max(rect.height()))
        .min((pixel_budget as f32 / rect.area()).sqrt())
        .min(1.0);
    let mut size = [
        (rect.width() * scale).floor().max(1.0) as u32,
        (rect.height() * scale).floor().max(1.0) as u32,
    ];
    // A very narrow path still needs one texel on its shorter axis. Account for
    // that rounding so even extreme aspect ratios obey the shared byte budget.
    if size[0] as usize * size[1] as usize > pixel_budget {
        let long_axis = usize::from(size[1] > size[0]);
        size[long_axis] = (pixel_budget / size[1 - long_axis] as usize).max(1) as u32;
    }
    let cache = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Arc<Mutex<InkPreviewCache>>>(egui::Id::new(
            "annotation ink previews",
        ))
        .clone()
    });
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let frame = ctx.cumulative_frame_nr();
    if let Some(index) = cache.entries.iter().position(|entry| {
        entry.points == annotation.points
            && entry.width == annotation.stroke_width
            && entry.rect == rect
            && entry.texture.size() == [size[0] as usize, size[1] as usize]
    }) {
        let mut entry = cache.entries.remove(index).expect("cache position exists");
        entry.frame = frame;
        let texture = entry.texture.clone();
        cache.entries.push_front(entry);
        return Ok(texture);
    }
    let bytes = size[0] as usize * size[1] as usize * 4
        + annotation.points.len() * std::mem::size_of::<Point>();
    while cache.bytes + bytes > MAX_PREVIEW_CACHE_BYTES
        || cache.entries.len() >= crate::document::MAX_ANNOTATIONS * 2
    {
        // Textures referenced by this frame must survive until rendering.
        let Some(index) = cache.entries.iter().rposition(|entry| entry.frame != frame) else {
            return Err(());
        };
        let entry = cache.entries.remove(index).expect("cache position exists");
        cache.bytes -= entry.bytes;
    }
    let mut overlay = Pixmap::new(size[0], size[1]).ok_or(())?;
    let sx = size[0] as f32 / rect.width();
    let sy = size[1] as f32 / rect.height();
    let transform = Transform::from_row(sx, 0.0, 0.0, sy, -rect.min.x * sx, -rect.min.y * sy);
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = true;
    paint_freehand(&mut overlay, annotation, &paint, transform);
    let image = egui::ColorImage::new(
        [size[0] as usize, size[1] as usize],
        overlay
            .pixels()
            .iter()
            .map(|pixel| Color32::from_white_alpha(pixel.alpha()))
            .collect(),
    );
    let texture = ctx.load_texture("annotation ink", image, egui::TextureOptions::LINEAR);
    cache.bytes += bytes;
    cache.entries.push_front(PreviewInk {
        points: annotation.points.clone(),
        width: annotation.stroke_width,
        rect,
        texture: texture.clone(),
        bytes,
        frame,
    });
    Ok(texture)
}

fn build_text_raster(annotation: &Annotation) -> Result<TextRaster, String> {
    FONTS.with_borrow_mut(|fonts| {
        fonts.begin_pass(TEXT_ATLAS_SIDE, TEXT_ALPHA);
        if annotation.kind == AnnotationKind::Text && annotation.text_style.path.is_some() { return text_path_render::raster(fonts, annotation); }
        let (galley, shift) = text_layout(fonts, annotation)?;
        // egui reuses occupied atlas slots if a single layout overflows. Detect
        // it before reading any texels, rather than exporting corrupted glyphs.
        if fonts.font_atlas_fill_ratio() >= 1.0 {
            return Err("This text annotation contains too many large glyphs. Reduce its font size or split it into shorter annotations.".into());
        }
        let bounds = galley.rect.union(galley.mesh_bounds).translate(shift);
        if !galley.mesh_bounds.is_positive() {
            return Ok(TextRaster { bounds, offset: Vec2::ZERO, size: [0, 0], alpha: Vec::new() });
        }
        let offset = galley.mesh_bounds.min.floor().to_vec2();
        let extent = galley.mesh_bounds.max.ceil().to_vec2() - offset;
        let size = [extent.x as usize, extent.y as usize];
        if size[0] > 32_768 || size[1] > 32_768
            || size[0].saturating_mul(size[1]) > MAX_TEXT_RASTER_PIXELS {
            return Err("This text annotation is too large to render. Reduce its font size or split it into shorter annotations.".into());
        }
        let mut alpha = vec![0_u8; size[0] * size[1]];
        let atlas = fonts.texture_atlas().image();
        for row in &galley.rows {
            if annotation.text_style.italic || annotation.text_style.underline {
                let mesh = &row.visuals.mesh;
                let shift = row.pos.to_vec2() - offset;
                for triangle in mesh.indices.chunks_exact(3) {
                    let a = mesh.vertices[triangle[0] as usize];
                    let b = mesh.vertices[triangle[1] as usize];
                    let c = mesh.vertices[triangle[2] as usize];
                    let pa = a.pos + shift; let pb = b.pos + shift; let pc = c.pos + shift;
                    let bounds = Rect::from_points(&[pa,pb,pc]);
                    let ab = pb-pa; let ac = pc-pa;
                    let cross = |u: Vec2, v: Vec2| u.x*v.y-u.y*v.x;
                    let det = cross(ab,ac);
                    if det.abs() < 0.00001 { continue; }
                    for y in (bounds.min.y.floor().max(0.) as usize)..(bounds.max.y.ceil().max(0.) as usize).min(size[1]) {
                        for x in (bounds.min.x.floor().max(0.) as usize)..(bounds.max.x.ceil().max(0.) as usize).min(size[0]) {
                            let p = egui::pos2(x as f32+0.5,y as f32+0.5)-pa;
                            let u = cross(p,ac)/det; let v = cross(ab,p)/det;
                            if u < -0.0001 || v < -0.0001 || u+v > 1.0001 { continue; }
                            let sample = a.uv + (b.uv-a.uv)*u + (c.uv-a.uv)*v - Vec2::splat(0.5);
                            let coverage = if a.uv == Pos2::ZERO && b.uv == Pos2::ZERO && c.uv == Pos2::ZERO { 255 } else { (sample_alpha(atlas,sample)*255.).round() as u8 };
                            alpha[y*size[0]+x] = alpha[y*size[0]+x].max(coverage);
                        }
                    }
                }
                continue;
            }
            let vertices = &row.visuals.mesh.vertices[row.visuals.glyph_vertex_range.clone()];
            // Plain egui glyphs are TL, TR, BL, BR quads with texel-space UVs.
            for quad in vertices.chunks_exact(4) {
                let shift = row.pos.to_vec2() - offset;
                let rect = Rect::from_min_max(quad[0].pos + shift, quad[3].pos + shift);
                let uv = Rect::from_min_max(quad[0].uv, quad[3].uv);
                let left = rect.min.x.floor().max(0.0) as usize;
                let top = rect.min.y.floor().max(0.0) as usize;
                let right = (rect.max.x.ceil() as usize).min(size[0]);
                let bottom = (rect.max.y.ceil() as usize).min(size[1]);
                for y in top..bottom {
                    for x in left..right {
                        let p = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
                        if !rect.contains(p) { continue; }
                        let fraction = (p - rect.min) / rect.size();
                        let sample = uv.min + uv.size() * fraction - Vec2::splat(0.5);
                        let coverage = sample_alpha(atlas, sample);
                        let old = f32::from(alpha[y * size[0] + x]);
                        alpha[y * size[0] + x] = (coverage * 255.0 + old * (1.0 - coverage)).round() as u8;
                    }
                }
            }
        }
        Ok(TextRaster { bounds, offset: offset + shift, size, alpha })
    })
}

fn preview_texture(
    ctx: &egui::Context,
    annotation: &Annotation,
    raster: &TextRaster,
) -> Result<egui::TextureHandle, ()> {
    let cache = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Arc<Mutex<PreviewCache>>>(egui::Id::new(
            "myimage-annotation-textures",
        ))
        .clone()
    });
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let frame = ctx.cumulative_frame_nr();
    if let Some(index) = cache
        .entries
        .iter()
        .position(|entry| entry.key.matches(annotation))
    {
        let mut entry = cache.entries.remove(index).expect("cache position exists");
        entry.frame = frame;
        let texture = entry.texture.clone();
        cache.entries.push_front(entry);
        return Ok(texture);
    }
    let limit = ctx.input(|input| input.max_texture_side).max(1);
    let (size, alpha) = if raster.size[0].max(raster.size[1]) > limit {
        let scale = limit as f32 / raster.size[0].max(raster.size[1]) as f32;
        let width = ((raster.size[0] as f32 * scale).floor() as u32).max(1);
        let height = ((raster.size[1] as f32 * scale).floor() as u32).max(1);
        let image = image::GrayImage::from_raw(
            raster.size[0] as u32,
            raster.size[1] as u32,
            raster.alpha.clone(),
        )
        .expect("text raster dimensions match its alpha buffer");
        let image =
            image::imageops::resize(&image, width, height, image::imageops::FilterType::Triangle);
        ([width as usize, height as usize], image.into_raw())
    } else {
        (raster.size, raster.alpha.clone())
    };
    let bytes = alpha.len() * 4 + annotation.text.len();
    while cache.bytes + bytes > MAX_PREVIEW_CACHE_BYTES
        || cache.entries.len() >= MAX_TEXT_CACHE_ENTRIES
    {
        // Shapes for this frame still reference their texture IDs. Never free
        // one before the renderer consumes it, even when the cache is full.
        let Some(index) = cache.entries.iter().rposition(|entry| entry.frame != frame) else {
            return Err(());
        };
        let entry = cache.entries.remove(index).expect("cache position exists");
        cache.bytes -= entry.bytes;
    }
    let image = egui::ColorImage::new(
        size,
        alpha.into_iter().map(Color32::from_white_alpha).collect(),
    );
    let texture = ctx.load_texture("annotation text", image, egui::TextureOptions::LINEAR);
    cache.bytes += bytes;
    cache.entries.push_front(PreviewText {
        key: TextKey::new(annotation),
        texture: texture.clone(),
        bytes,
        frame,
    });
    Ok(texture)
}

fn paint_text(overlay: &mut Pixmap, annotation: &Annotation) -> Result<(), String> {
    text_path_render::paint_path(overlay, annotation, Transform::identity());
    let raster = text_raster(annotation)?;
    let rect = Rect::from_min_size(
        text_anchor(annotation) + raster.offset,
        egui::vec2(raster.size[0] as f32, raster.size[1] as f32),
    );
    let left = rect.min.x.floor().max(0.0) as u32;
    let top = rect.min.y.floor().max(0.0) as u32;
    let right = rect.max.x.ceil().clamp(0.0, overlay.width() as f32) as u32;
    let bottom = rect.max.y.ceil().clamp(0.0, overlay.height() as f32) as u32;
    for y in top..bottom {
        for x in left..right {
            let p = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
            if !rect.contains(p) {
                continue;
            }
            let sample = p - rect.min - Vec2::splat(0.5);
            let coverage = raster_alpha(&raster, sample);
            blend_text_pixel(overlay, x, y, text_ink(annotation), coverage);
        }
    }
    Ok(())
}

fn raster_alpha(raster: &TextRaster, point: Vec2) -> f32 {
    let x = point.x.floor() as i32;
    let y = point.y.floor() as i32;
    let fx = point.x - point.x.floor();
    let fy = point.y - point.y.floor();
    let get = |x: i32, y: i32| {
        if x < 0 || y < 0 || x as usize >= raster.size[0] || y as usize >= raster.size[1] {
            0.0
        } else {
            f32::from(raster.alpha[y as usize * raster.size[0] + x as usize]) / 255.0
        }
    };
    let top = get(x, y) * (1.0 - fx) + get(x + 1, y) * fx;
    let bottom = get(x, y + 1) * (1.0 - fx) + get(x + 1, y + 1) * fx;
    top * (1.0 - fy) + bottom * fy
}

fn sample_alpha(atlas: &egui::ColorImage, point: Pos2) -> f32 {
    let x = point.x.floor() as i32;
    let y = point.y.floor() as i32;
    let fx = point.x - point.x.floor();
    let fy = point.y - point.y.floor();
    let get = |x: i32, y: i32| {
        if x < 0 || y < 0 || x as usize >= atlas.size[0] || y as usize >= atlas.size[1] {
            0.0
        } else {
            f32::from(atlas.pixels[y as usize * atlas.size[0] + x as usize].a()) / 255.0
        }
    };
    let top = get(x, y) * (1.0 - fx) + get(x + 1, y) * fx;
    let bottom = get(x, y + 1) * (1.0 - fx) + get(x + 1, y + 1) * fx;
    top * (1.0 - fy) + bottom * fy
}

fn blend_text_pixel(overlay: &mut Pixmap, x: u32, y: u32, color: [u8; 4], coverage: f32) {
    let alpha = (f32::from(color[3]) * coverage).round() as u8;
    if alpha == 0 {
        return;
    }
    let index = y as usize * overlay.width() as usize + x as usize;
    let destination = &mut overlay.pixels_mut()[index];
    let remaining = 255 - u32::from(alpha);
    let blend = |source: u8, old: u8| {
        ((u32::from(source) * u32::from(alpha) + u32::from(old) * remaining + 127) / 255) as u8
    };
    let a = u32::from(alpha) + (u32::from(destination.alpha()) * remaining + 127) / 255;
    *destination = tiny_skia::PremultipliedColorU8::from_rgba(
        blend(color[0], destination.red()),
        blend(color[1], destination.green()),
        blend(color[2], destination.blue()),
        a as u8,
    )
    .expect("source-over keeps color premultiplied");
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn dotted_stroke_preview_has_full_round_coverage() {
        let shape = stroke_preview(
            vec![Pos2::new(20., 30.), Pos2::new(20.01, 30.)],
            Stroke::new(6., Color32::RED),
        );
        let Shape::Circle(dot) = shape else {
            panic!("Dot must not render as a thin butt-capped line");
        };
        assert_eq!(dot.radius, 3.);
        assert_eq!(dot.fill, Color32::RED);
    }

    fn annotation(kind: AnnotationKind, start: [f32; 2], end: [f32; 2]) -> Annotation {
        Annotation {
            kind,
            start: Point {
                x: start[0],
                y: start[1],
            },
            end: Point {
                x: end[0],
                y: end[1],
            },
            color: [230, 30, 70, 255],
            stroke_width: 3.0,
            fill: false,
            text: "İstanbul ığüşöç\nArrows →".into(),
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
        }
    }

    fn document(annotations: Vec<Annotation>) -> Document {
        Document {
            image: Arc::new(RgbaImage::from_pixel(300, 200, Rgba([20, 80, 160, 128]))),
            annotations,
            geometry: Default::default(),
        }
    }

    fn freehand(points: &[[f32; 2]], width: f32) -> Annotation {
        let mut ink = annotation(AnnotationKind::Freehand, [0.0; 2], [0.0; 2]);
        ink.points = points.iter().map(|p| Point { x: p[0], y: p[1] }).collect();
        ink.stroke_width = width;
        ink.color = [40, 100, 200, 128];
        ink
    }

    #[test]
    fn three_dimensional_shapes_render_and_reopen_with_torus_hole() {
        for (kind, name) in crate::shapes::THREE_D {
            let mut a = annotation(kind, [30., 30.], [170., 170.]);
            a.fill = true;
            let mut doc = document(vec![a]);
            doc.image = Arc::new(RgbaImage::new(300, 200));
            let flat = flatten(&doc).unwrap();
            assert!(flat.pixels().filter(|p| p[3] > 0).count() > 1000, "{name}");
            if kind == AnnotationKind::Torus {
                assert_eq!(flat.get_pixel(100, 90)[3], 0);
                assert!(flat.get_pixel(100, 155)[3] > 0);
            }
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("3d.myimage");
            crate::document::save_project(&doc, &path).unwrap();
            let loaded = crate::document::load_project(&path).unwrap();
            assert_eq!(loaded.edit_state(), doc.edit_state());
            assert_eq!(flatten(&loaded).unwrap(), flat);
            let points = crate::shapes::outline(
                kind,
                Rect::from_min_max(egui::pos2(30., 30.), egui::pos2(170., 170.)),
            );
            let mesh = polygon_mesh(&points, Color32::WHITE);
            assert_eq!(mesh.indices.len(), (points.len() - 2) * 3, "{name}");
        }
    }

    #[test]
    fn polygon_mesh_and_export_agree_for_convex_and_concave_shapes() {
        for (kind, name) in crate::shapes::CHOICES {
            if !kind.is_polygon_shape() {
                continue;
            }
            let rect = Rect::from_min_max(egui::pos2(30., 30.), egui::pos2(170., 170.));
            let points = crate::shapes::outline(kind, rect);
            assert!(
                points.iter().all(|p| rect.expand(0.001).contains(*p)),
                "{name}"
            );
            let area: f32 = points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .take(points.len())
                .map(|(a, b)| a.x * b.y - a.y * b.x)
                .sum::<f32>()
                * 0.5;
            let mesh = polygon_mesh(&points, Color32::WHITE);
            let mesh_area: f32 = mesh
                .indices
                .chunks_exact(3)
                .map(|t| {
                    let a = mesh.vertices[t[0] as usize].pos;
                    let b = mesh.vertices[t[1] as usize].pos;
                    let c = mesh.vertices[t[2] as usize].pos;
                    ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)) * 0.5
                })
                .sum();
            assert!(
                area > 0. && (area - mesh_area).abs() < 0.05,
                "{name}: {area} vs {mesh_area}"
            );
            let mut a = annotation(kind, [30., 30.], [170., 170.]);
            a.fill = true;
            a.color = [40, 160, 220, 128];
            let mut doc = document(vec![a]);
            doc.image = Arc::new(RgbaImage::new(300, 200));
            let flat = flatten(&doc).unwrap();
            assert_eq!(flat.get_pixel(100, 100)[3], 128, "{name}");
            assert_eq!(flat.get_pixel(10, 10)[3], 0, "{name}");
            if matches!(kind, AnnotationKind::Star | AnnotationKind::Cross) {
                assert_eq!(flat.get_pixel(35, 35)[3], 0, "{name}");
            }
            doc.annotations[0].fill = false;
            assert_eq!(flatten(&doc).unwrap().get_pixel(100, 100)[3], 0, "{name}");
        }
    }

    #[test]
    fn freehand_dot_caps_and_bounds_follow_points_not_endpoints() {
        let dot = freehand(&[[40.0, 50.0]], 20.0);
        assert_eq!(
            annotation_bounds(&dot),
            Rect::from_min_max(egui::pos2(30.0, 40.0), egui::pos2(50.0, 60.0))
        );
        let line = freehand(&[[80.0, 50.0], [140.0, 50.0]], 20.0);
        let mut doc = document(vec![dot.clone(), line]);
        doc.image = Arc::new(RgbaImage::new(300, 200));
        let output = flatten(&doc).unwrap();
        for (x, y) in [(40, 50), (31, 50), (40, 41), (71, 50), (148, 50), (110, 50)] {
            assert_eq!(output.get_pixel(x, y)[3], 128, "{x}, {y}");
        }
        for (x, y) in [(30, 40), (49, 59), (70, 40), (149, 59), (110, 39)] {
            assert_eq!(output.get_pixel(x, y)[3], 0, "{x}, {y}");
        }
        let mut stationary = dot.clone();
        stationary.points = vec![dot.points[0]; 32];
        doc.annotations = vec![dot];
        let once = flatten(&doc).unwrap();
        doc.annotations = vec![stationary];
        assert_eq!(flatten(&doc).unwrap(), once);
    }

    #[test]
    fn freehand_joins_and_self_intersections_apply_alpha_once() {
        let ink = freehand(
            &[[30.0, 30.0], [150.0, 150.0], [150.0, 30.0], [30.0, 150.0]],
            20.0,
        );
        let mut doc = document(vec![ink.clone()]);
        doc.image = Arc::new(RgbaImage::new(300, 200));
        let output = flatten(&doc).unwrap();
        assert!(output.pixels().all(|pixel| pixel[3] <= 128));
        for (x, y) in [(90, 90), (150, 150), (150, 30)] {
            assert_eq!(output.get_pixel(x, y)[3], 128);
        }
        doc.annotations.push(ink);
        assert_eq!(flatten(&doc).unwrap().get_pixel(90, 90)[3], 192);
    }

    #[test]
    fn freehand_preview_cache_reuses_color_and_invalidates_geometry_with_bounded_rasters() {
        let ctx = egui::Context::default();
        let mut ink = freehand(&[[30.0, 30.0], [150.0, 150.0], [150.0, 30.0]], 20.0);
        let rect = annotation_bounds(&ink).expand(1.0);
        let first = ink_preview_texture(&ctx, &ink, rect, MAX_INK_PREVIEW_PIXELS as usize).unwrap();
        ink.color = [255, 0, 0, 255];
        assert_eq!(
            first.id(),
            ink_preview_texture(&ctx, &ink, rect, MAX_INK_PREVIEW_PIXELS as usize)
                .unwrap()
                .id()
        );
        ink.stroke_width = 12.0;
        assert_ne!(
            first.id(),
            ink_preview_texture(&ctx, &ink, rect, MAX_INK_PREVIEW_PIXELS as usize)
                .unwrap()
                .id()
        );
        ink.points[1].x = 120.0;
        assert_ne!(
            first.id(),
            ink_preview_texture(&ctx, &ink, rect, MAX_INK_PREVIEW_PIXELS as usize)
                .unwrap()
                .id()
        );

        let sparse = freehand(&[[-900_000.0, -900_000.0], [900_000.0, 900_000.0]], 100.0);
        let sparse_rect = annotation_bounds(&sparse);
        let texture =
            ink_preview_texture(&ctx, &sparse, sparse_rect, MAX_INK_PREVIEW_PIXELS as usize)
                .unwrap();
        assert!(texture.size()[0] <= 2048 && texture.size()[1] <= 2048);
        assert!(texture.size()[0] * texture.size()[1] <= MAX_INK_PREVIEW_PIXELS as usize);
        let narrow = freehand(&[[-900_000.0, 0.0], [900_000.0, 0.0]], 0.1);
        let narrow_rect = annotation_bounds(&narrow).expand(1.0);
        let texture = ink_preview_texture(&ctx, &narrow, narrow_rect, 100).unwrap();
        assert!(texture.size()[0] * texture.size()[1] <= 100);
        let cache = ctx
            .data(|data| {
                data.get_temp::<Arc<Mutex<InkPreviewCache>>>(egui::Id::new(
                    "annotation ink previews",
                ))
            })
            .unwrap();
        assert!(cache.lock().unwrap().bytes <= MAX_PREVIEW_CACHE_BYTES);
    }

    #[test]
    fn freehand_preview_eviction_releases_old_frames_and_keeps_current_textures() {
        let ctx = egui::Context::default();
        let rect = Rect::from_min_max(Pos2::ZERO, egui::pos2(1024.0, 1024.0));
        let mut first = None;
        for i in 0..12 {
            let ink = freehand(&[[0.0, i as f32], [1024.0, 1024.0]], 20.0);
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                let texture =
                    ink_preview_texture(ctx, &ink, rect, MAX_INK_PREVIEW_PIXELS as usize).unwrap();
                if first.is_none() {
                    first = Some(texture.id());
                }
                let cache = ctx
                    .data(|data| {
                        data.get_temp::<Arc<Mutex<InkPreviewCache>>>(egui::Id::new(
                            "annotation ink previews",
                        ))
                    })
                    .unwrap();
                let cache = cache.lock().unwrap();
                assert!(cache.bytes <= MAX_PREVIEW_CACHE_BYTES);
                assert!(
                    cache
                        .entries
                        .iter()
                        .any(|entry| entry.texture.id() == texture.id())
                );
            });
        }
        let cache = ctx
            .data(|data| {
                data.get_temp::<Arc<Mutex<InkPreviewCache>>>(egui::Id::new(
                    "annotation ink previews",
                ))
            })
            .unwrap();
        assert!(
            cache
                .lock()
                .unwrap()
                .entries
                .iter()
                .all(|entry| Some(entry.texture.id()) != first)
        );
    }

    #[test]
    fn freehand_preview_keeps_many_large_strokes_and_more_than_512_dots() {
        let ctx = egui::Context::default();
        let visible = Rect::from_min_size(Pos2::ZERO, Vec2::splat(1024.0));
        let large: Vec<_> = (0..20)
            .map(|i| {
                freehand(
                    &[[10.0, 10.0 + i as f32], [1000.0, 1000.0 - i as f32]],
                    20.0,
                )
            })
            .collect();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            // A revised pass can increase the visible count without stranding
            // the previous pass's larger textures in the current frame.
            let _ = annotation_shapes(&painter, &large[..8], visible);
            let shapes = annotation_shapes(&painter, &large, visible);
            assert_eq!(shapes.len(), large.len());
            assert!(shapes.iter().all(|shape| matches!(shape, Shape::Mesh(_))));
        });
        let dots: Vec<_> = (0..600)
            .map(|i| {
                freehand(
                    &[[10.0 + (i % 30) as f32 * 30.0, 10.0 + (i / 30) as f32 * 30.0]],
                    8.0,
                )
            })
            .collect();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            let shapes = annotation_shapes(&painter, &dots, visible);
            assert_eq!(shapes.len(), dots.len());
            assert!(shapes.iter().all(|shape| matches!(shape, Shape::Mesh(_))));
        });
        let cache = ctx
            .data(|data| {
                data.get_temp::<Arc<Mutex<InkPreviewCache>>>(egui::Id::new(
                    "annotation ink previews",
                ))
            })
            .unwrap();
        assert!(cache.lock().unwrap().bytes <= MAX_PREVIEW_CACHE_BYTES);
    }

    #[test]
    fn freehand_native_mesh_tracks_crop_rotation_resize_and_zoom() {
        use crate::geometry::{CropRect, ImageGeometry};
        let ink = freehand(&[[50.0, 60.0], [100.0, 120.0]], 20.0);
        let source = annotation_bounds(&ink).expand(1.0);
        let mut doc = document(vec![ink]);
        doc.geometry = ImageGeometry {
            effects: Default::default(),
            viewport: None,
            flip_horizontal: false,
            flip_vertical: false,
            canvas: None,
            cutouts: [None; 16],
            crop: Some(CropRect {
                x: 20,
                y: 30,
                width: 200,
                height: 140,
            }),
            quarter_turns: 1,
            resize: Some([280, 100]),
        };
        let origin = egui::pos2(25.0, 35.0);
        for scale in [0.5, 1.0, 2.0] {
            let ctx = egui::Context::default();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(2000.0))),
                    ..Default::default()
                },
                |ctx| {
                    let painter = ctx.layer_painter(egui::LayerId::background());
                    paint_document_annotations(&painter, &doc, origin, scale);
                },
            );
            let mesh = output
                .shapes
                .iter()
                .find_map(|shape| {
                    if let Shape::Mesh(mesh) = &shape.shape {
                        Some(mesh)
                    } else {
                        None
                    }
                })
                .expect("ink uses its canonical coverage texture");
            let mut expected = Rect::NOTHING;
            for point in [
                source.left_top(),
                source.right_top(),
                source.left_bottom(),
                source.right_bottom(),
            ] {
                let mapped = doc.geometry.map_point(
                    Point {
                        x: point.x,
                        y: point.y,
                    },
                    [300, 200],
                );
                expected.extend_with(origin + egui::vec2(mapped.x, mapped.y) * scale);
            }
            assert_eq!(mesh.calc_bounds(), expected);
        }
    }

    #[test]
    fn freehand_transforms_and_privacy_preserve_original_vectors() {
        use crate::geometry::{CropRect, ImageGeometry};
        let ink = freehand(&[[50.0, 50.0], [100.0, 50.0], [100.0, 130.0]], 20.0);
        let mut doc = document(vec![ink]);
        doc.image = Arc::new(RgbaImage::new(300, 200));
        let original = doc.image.clone();
        let state = doc.edit_state();
        let full = flatten(&doc).unwrap();
        let reduced = privacy_preview(&doc, 150).unwrap();
        assert_eq!(reduced.dimensions(), (150, 100));
        assert_eq!(reduced.get_pixel(35, 25)[3], 128);
        assert_eq!(reduced.get_pixel(50, 50)[3], 128);
        assert_eq!(reduced.get_pixel(70, 25)[3], 0);
        doc.geometry = ImageGeometry {
            effects: Default::default(),
            viewport: None,
            flip_horizontal: false,
            flip_vertical: false,
            canvas: None,
            cutouts: [None; 16],
            crop: Some(CropRect {
                x: 20,
                y: 30,
                width: 200,
                height: 140,
            }),
            quarter_turns: 1,
            resize: Some([280, 100]),
        };
        assert_eq!(flatten(&doc).unwrap(), doc.geometry.apply(&full).unwrap());
        let thumbnail = thumbnail(&doc, 280, 100).unwrap();
        let transformed = doc
            .geometry
            .map_point(Point { x: 75.0, y: 50.0 }, [300, 200]);
        assert_eq!(
            thumbnail.get_pixel(transformed.x as u32, transformed.y as u32)[3],
            128
        );
        let mut effect = annotation(AnnotationKind::Pixelate, [0.0, 0.0], [300.0, 200.0]);
        effect.effect_size = 24;
        let mut filtered = full;
        crate::privacy::apply(&mut filtered, &effect).unwrap();
        doc.annotations.push(effect);
        assert_eq!(
            flatten(&doc).unwrap(),
            doc.geometry.apply(&filtered).unwrap()
        );
        assert_eq!(privacy_preview(&doc, 300).unwrap(), filtered);
        assert_eq!(doc.annotations[0], state.annotations[0]);
        assert!(Arc::ptr_eq(&doc.image, &original));
    }

    #[test]
    fn flatten_keeps_original_bytes_and_dimensions_without_modifying_document() {
        let doc = document(vec![annotation(
            AnnotationKind::Rectangle,
            [20.0, 30.0],
            [80.0, 70.0],
        )]);
        let original = doc.image.clone();
        let output = flatten(&doc).unwrap();
        assert_eq!(output.dimensions(), (300, 200));
        assert_eq!(output.get_pixel(0, 0), original.get_pixel(0, 0));
        assert_eq!(output.get_pixel(50, 50), original.get_pixel(50, 50));
        assert_ne!(output.get_pixel(20, 50), original.get_pixel(20, 50));
        assert_eq!(doc.image.as_raw(), original.as_raw());
        let empty = document(vec![]);
        assert_eq!(flatten(&empty).unwrap().as_raw(), empty.image.as_raw());
    }

    #[test]
    fn shapes_use_original_coordinates_and_preserve_transparency() {
        let mut rectangle = annotation(AnnotationKind::Rectangle, [60.0, 55.0], [20.0, 25.0]);
        rectangle.fill = true;
        rectangle.color = [40, 160, 220, 128];
        let mut ellipse = annotation(AnnotationKind::Ellipse, [100.0, 20.0], [160.0, 80.0]);
        ellipse.fill = true;
        let mut doc = document(vec![
            rectangle,
            ellipse,
            annotation(AnnotationKind::Arrow, [20.0, 120.0], [90.0, 120.0]),
        ]);
        doc.image = Arc::new(RgbaImage::from_pixel(300, 200, Rgba([4, 8, 12, 0])));
        let output = flatten(&doc).unwrap();
        let center = output.get_pixel(40, 40);
        assert_eq!(center[3], 128);
        assert!((i32::from(center[0]) - 40).abs() <= 1);
        assert_eq!(output.get_pixel(130, 50)[3], 255);
        assert_eq!(output.get_pixel(100, 20)[3], 0);
        assert_eq!(output.get_pixel(50, 120)[3], 255);
        assert!(output.get_pixel(81, 117)[3] > 0);
        assert_eq!(*output.get_pixel(250, 170), Rgba([4, 8, 12, 0]));
        assert!(output.pixels().any(|pixel| pixel[3] > 0 && pixel[3] < 128));
    }

    #[test]
    fn unicode_multiline_text_renders_inside_its_layout_bounds() {
        let text = annotation(AnnotationKind::Text, [15.0, 20.0], [15.0, 20.0]);
        FONTS.with_borrow_mut(|fonts| {
            assert!(fonts.has_glyphs(&FontId::proportional(24.0), "İıĞğÜüŞşÖöÇç"));
        });
        let bounds = annotation_bounds(&text).expand(1.0);
        let mut doc = document(vec![text]);
        doc.image = Arc::new(RgbaImage::new(300, 200));
        let output = flatten(&doc).unwrap();
        let mut first_line = 0;
        let mut second_line = 0;
        for (x, y, pixel) in output.enumerate_pixels() {
            if pixel[3] > 0 {
                assert!(bounds.contains(egui::pos2(x as f32 + 0.5, y as f32 + 0.5)));
                if y < 48 {
                    first_line += 1;
                } else {
                    second_line += 1;
                }
            }
        }
        assert!(first_line > 100 && second_line > 100);
    }

    #[test]
    fn saved_project_reopens_with_identical_export_and_can_be_edited_again() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("annotations.myimage");
        let original = document(vec![
            annotation(AnnotationKind::Text, [12.0, 14.0], [12.0, 14.0]),
            annotation(AnnotationKind::Rectangle, [40.0, 90.0], [110.0, 160.0]),
            annotation(AnnotationKind::Arrow, [140.0, 140.0], [230.0, 110.0]),
        ]);
        let initial_export = flatten(&original).unwrap();
        crate::document::save_project(&original, &path).unwrap();
        let mut reopened = crate::document::load_project(&path).unwrap();
        assert_eq!(flatten(&reopened).unwrap(), initial_export);
        assert_eq!(reopened.image.as_raw(), original.image.as_raw());

        reopened.annotations[0].text = "Düzenlendi".into();
        reopened.annotations[1].start.x += 25.0;
        reopened.annotations[1].end.x += 25.0;
        crate::document::save_project(&reopened, &path).unwrap();
        let edited_export = flatten(&crate::document::load_project(&path).unwrap()).unwrap();
        assert_ne!(edited_export, initial_export);
        assert_eq!(edited_export, flatten(&reopened).unwrap());
        assert_eq!(
            edited_export.get_pixel(40, 125),
            original.image.get_pixel(40, 125)
        );
        assert_ne!(
            edited_export.get_pixel(65, 125),
            original.image.get_pixel(65, 125)
        );
    }

    #[test]
    fn text_preview_geometry_matches_export_at_different_dpi_and_zoom() {
        let text = annotation(AnnotationKind::Text, [15.25, 20.5], [15.25, 20.5]);
        let raster = text_raster(&text).unwrap();
        let origin = egui::pos2(45.0, 60.0);
        for dpi in [1.0, 2.0] {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(dpi);
            for scale in [0.5, 1.0, 2.0] {
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(2_000.0))),
                        ..Default::default()
                    },
                    |ctx| {
                        let painter = ctx.layer_painter(egui::LayerId::background());
                        paint_annotations(&painter, std::slice::from_ref(&text), origin, scale);
                    },
                );
                let mesh = output
                    .shapes
                    .iter()
                    .find_map(|shape| {
                        if let Shape::Mesh(mesh) = &shape.shape {
                            Some(mesh)
                        } else {
                            None
                        }
                    })
                    .expect("text preview uses the canonical raster texture");
                let expected = Rect::from_min_size(
                    origin + (pos(text.start).to_vec2() + raster.offset) * scale,
                    egui::vec2(raster.size[0] as f32, raster.size[1] as f32) * scale,
                );
                assert_eq!(mesh.calc_bounds(), expected);
            }
        }
        let mut doc = document(vec![text.clone()]);
        doc.image = Arc::new(RgbaImage::new(300, 200));
        let output = flatten(&doc).unwrap();
        let rectangle = Rect::from_min_size(
            pos(text.start) + raster.offset,
            egui::vec2(raster.size[0] as f32, raster.size[1] as f32),
        );
        for (x, y, pixel) in output.enumerate_pixels() {
            let point = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
            let expected = if rectangle.contains(point) {
                (raster_alpha(&raster, point - rectangle.min - Vec2::splat(0.5)) * 255.0).round()
                    as u8
            } else {
                0
            };
            assert_eq!(pixel[3], expected);
        }
    }

    #[test]
    fn transformed_preview_preserves_rotated_text_texels_and_image_uvs() {
        use crate::geometry::{CropRect, ImageGeometry};
        let text = annotation(AnnotationKind::Text, [25.0, 30.0], [25.0, 30.0]);
        let raster = text_raster(&text).unwrap();
        let text_rect = Rect::from_min_size(
            pos(text.start) + raster.offset,
            egui::vec2(raster.size[0] as f32, raster.size[1] as f32),
        );
        let mut document = document(vec![text]);
        let original = [document.image.width(), document.image.height()];
        let origin = egui::pos2(35.0, 55.0);
        for turns in 0..4 {
            document.geometry = ImageGeometry {
                effects: Default::default(),
                viewport: None,
                flip_horizontal: false,
                flip_vertical: false,
                canvas: None,
                cutouts: [None; 16],
                crop: Some(CropRect {
                    x: 10,
                    y: 15,
                    width: 250,
                    height: 160,
                }),
                quarter_turns: turns,
                resize: Some([310, 210]),
            };
            for scale in [0.5, 1.0, 2.0] {
                let ctx = egui::Context::default();
                ctx.set_pixels_per_point(2.0);
                let rect = Rect::from_min_size(origin, egui::vec2(310.0, 210.0) * scale);
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(2000.0))),
                        ..Default::default()
                    },
                    |ctx| {
                        let painter = ctx.layer_painter(egui::LayerId::background());
                        paint_document_image(&painter, egui::TextureId::User(77), &document, rect);
                        paint_document_annotations(&painter, &document, origin, scale);
                    },
                );
                let meshes: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| {
                        if let Shape::Mesh(mesh) = &shape.shape {
                            Some((shape.clip_rect, mesh))
                        } else {
                            None
                        }
                    })
                    .collect();
                assert_eq!(meshes.len(), 2);
                for (clip, mesh) in meshes {
                    assert_eq!(clip, rect);
                    assert_eq!(mesh.vertices.len(), 4);
                    for vertex in &mesh.vertices {
                        let source = if mesh.texture_id == egui::TextureId::User(77) {
                            egui::pos2(
                                vertex.uv.x * original[0] as f32,
                                vertex.uv.y * original[1] as f32,
                            )
                        } else {
                            text_rect.min + vertex.uv.to_vec2() * text_rect.size()
                        };
                        let expected = document.geometry.map_point(
                            Point {
                                x: source.x,
                                y: source.y,
                            },
                            original,
                        );
                        let expected = origin + egui::vec2(expected.x, expected.y) * scale;
                        assert!(vertex.pos.distance(expected) < 0.001);
                    }
                }
            }
        }
    }

    #[test]
    fn flatten_applies_geometry_after_annotations_without_changing_editable_state() {
        use crate::geometry::{CropRect, ImageGeometry};
        let mut document = document(vec![
            annotation(AnnotationKind::Text, [15.0, 20.0], [15.0, 20.0]),
            annotation(AnnotationKind::Arrow, [20.0, 120.0], [160.0, 90.0]),
        ]);
        let untransformed = flatten(&document).unwrap();
        document.geometry = ImageGeometry {
            effects: Default::default(),
            viewport: None,
            flip_horizontal: false,
            flip_vertical: false,
            canvas: None,
            cutouts: [None; 16],
            crop: Some(CropRect {
                x: 8,
                y: 12,
                width: 170,
                height: 130,
            }),
            quarter_turns: 1,
            resize: Some([65, 120]),
        };
        let state = document.edit_state();
        let original = document.image.clone();
        let output = flatten(&document).unwrap();
        assert_eq!(output.dimensions(), (65, 120));
        assert_eq!(output, document.geometry.apply(&untransformed).unwrap());
        assert_eq!(document.edit_state(), state);
        assert_eq!(document.image, original);
    }

    #[test]
    fn bounded_thumbnail_respects_crop_rotation_and_transparency() {
        use crate::geometry::{CropRect, ImageGeometry};
        let mut document = document(Vec::new());
        document.image = Arc::new(RgbaImage::from_fn(6, 4, |x, y| {
            Rgba([x as u8 * 30, y as u8 * 50, 120, 128])
        }));
        document.geometry = ImageGeometry {
            effects: Default::default(),
            viewport: None,
            flip_horizontal: false,
            flip_vertical: false,
            canvas: None,
            cutouts: [None; 16],
            crop: Some(CropRect {
                x: 1,
                y: 1,
                width: 4,
                height: 2,
            }),
            quarter_turns: 1,
            resize: None,
        };
        assert_eq!(
            thumbnail(&document, 384, 256).unwrap(),
            flatten(&document).unwrap()
        );
        document.image = Arc::new(RgbaImage::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba([255, 0, 0, 0])
            } else {
                Rgba([0, 0, 255, 255])
            }
        }));
        document.geometry = ImageGeometry {
            flip_horizontal: false,
            flip_vertical: false,
            canvas: None,
            resize: Some([1, 1]),
            ..Default::default()
        };
        assert_eq!(
            thumbnail(&document, 384, 256).unwrap().get_pixel(0, 0),
            &Rgba([0, 0, 255, 128])
        );
    }

    #[test]
    fn privacy_stack_filters_text_below_but_keeps_later_text_visible() {
        let mut text = annotation(AnnotationKind::Text, [24.0, 24.0], [24.0, 24.0]);
        text.text = "PRIVATE 123456".into();
        text.font_size = 28.0;
        text.color = [255; 4];
        for kind in [AnnotationKind::Blur, AnnotationKind::Pixelate] {
            let mut effect = annotation(kind, [8.0, 8.0], [292.0, 100.0]);
            effect.effect_size = 24;
            let base = document(vec![text.clone()]);
            let readable = flatten(&base).unwrap();
            let mut expected = readable.clone();
            crate::privacy::apply(&mut expected, &effect).unwrap();
            let filtered = document(vec![text.clone(), effect.clone()]);
            assert_eq!(flatten(&filtered).unwrap(), expected);
            assert_ne!(expected, readable);
            assert_eq!(privacy_preview(&filtered, 2048).unwrap(), expected);

            let above = document(vec![effect, text.clone()]);
            let above_pixels = flatten(&above).unwrap();
            // A white glyph is attenuated when filtered below, and remains
            // exactly white when painted after the filter.
            let glyph = readable
                .enumerate_pixels()
                .find(|(_, _, p)| p.0 == [255; 4])
                .unwrap();
            assert_eq!(above_pixels.get_pixel(glyph.0, glyph.1).0, [255; 4]);
            assert_ne!(expected.get_pixel(glyph.0, glyph.1).0, [255; 4]);
        }
    }

    #[test]
    fn ordered_effects_are_clipped_and_geometry_is_applied_last() {
        use crate::geometry::{CropRect, ImageGeometry};
        let mut doc = document(vec![
            annotation(AnnotationKind::Pixelate, [-20.0, 15.0], [120.0, 100.0]),
            annotation(AnnotationKind::Blur, [65.0, 60.0], [190.0, 155.0]),
        ]);
        doc.image = Arc::new(RgbaImage::from_fn(300, 200, |x, y| {
            Rgba([
                (x % 251) as u8,
                ((y * 5) % 255) as u8,
                ((x + y) % 255) as u8,
                255,
            ])
        }));
        let pixels = flatten(&doc).unwrap();
        assert_eq!(pixels.get_pixel(250, 180), doc.image.get_pixel(250, 180));
        assert_ne!(pixels.get_pixel(18, 35), doc.image.get_pixel(18, 35));
        doc.geometry = ImageGeometry {
            effects: Default::default(),
            viewport: None,
            flip_horizontal: false,
            flip_vertical: false,
            canvas: None,
            cutouts: [None; 16],
            crop: Some(CropRect {
                x: 10,
                y: 20,
                width: 220,
                height: 160,
            }),
            quarter_turns: 1,
            resize: Some([80, 110]),
        };
        let state = doc.edit_state();
        assert_eq!(privacy_preview(&doc, 2048).unwrap(), pixels);
        assert_eq!(flatten(&doc).unwrap(), doc.geometry.apply(&pixels).unwrap());
        assert_eq!(thumbnail(&doc, 384, 256).unwrap().dimensions(), (80, 110));
        assert_eq!(doc.edit_state(), state);
    }

    #[test]
    fn privacy_previews_stay_bounded_and_invalid_effects_fail_closed() {
        let mut doc = document(vec![annotation(
            AnnotationKind::Pixelate,
            [0.0, 0.0],
            [1600.0, 800.0],
        )]);
        doc.image = Arc::new(RgbaImage::from_fn(1600, 800, |x, y| {
            let tone = if (x / 4 + y / 4) % 2 == 0 { 0 } else { 255 };
            Rgba([tone, tone, tone, 255])
        }));
        let before = doc.image.clone();
        assert_eq!(privacy_preview(&doc, 200).unwrap().dimensions(), (200, 100));
        assert_eq!(thumbnail(&doc, 160, 100).unwrap().dimensions(), (160, 80));
        assert_eq!(doc.image, before);
        assert!(privacy_preview(&doc, 0).is_err());
        doc.annotations[0].effect_size = 0;
        assert!(privacy_preview(&doc, 200).is_err());
        assert!(thumbnail(&doc, 160, 100).is_err());
        assert!(flatten(&doc).is_err());
    }

    #[test]
    fn oversized_text_returns_error_instead_of_partial_or_corrupt_export() {
        let mut text = annotation(AnnotationKind::Text, [0.0, 0.0], [0.0, 0.0]);
        text.text = "W".repeat(2_000);
        text.font_size = crate::document::MAX_FONT_SIZE;
        let doc = document(vec![text]);
        assert!(flatten(&doc).unwrap_err().contains("too large to render"));
        assert_eq!(doc.annotations[0].text.len(), 2_000);
        TEXT_CACHE.with_borrow(|cache| {
            assert!(cache.bytes <= MAX_TEXT_CACHE_BYTES);
            assert!(cache.entries.len() <= MAX_TEXT_CACHE_ENTRIES);
        });
    }

    #[test]
    fn callout_outline_triangulates_every_tail_direction_without_overlap() {
        let mut callout = annotation(AnnotationKind::Callout, [100.0, 60.0], [220.0, 140.0]);
        let cross = |a: Pos2, b: Pos2| a.x * b.y - a.y * b.x;
        for point in [
            [160.0, 0.0],
            [300.0, 100.0],
            [160.0, 200.0],
            [20.0, 100.0],
            [350.0, 250.0],
            [160.0, 100.0],
        ] {
            callout.tail = Some(Point {
                x: point[0],
                y: point[1],
            });
            let outline = callout_outline(&callout);
            let expected = outline
                .iter()
                .copied()
                .zip(outline.iter().copied().cycle().skip(1))
                .map(|(a, b)| cross(a, b))
                .sum::<f32>()
                * 0.5;
            let mesh = polygon_mesh(&outline, Color32::WHITE);
            let actual = mesh
                .indices
                .chunks_exact(3)
                .map(|triangle| {
                    let [a, b, c] = [triangle[0], triangle[1], triangle[2]]
                        .map(|i| mesh.vertices[i as usize].pos);
                    (cross(a, b) + cross(b, c) + cross(c, a)) * 0.5
                })
                .sum::<f32>();
            assert!(
                (expected - actual).abs() < 0.01,
                "tail {point:?}: {expected} vs {actual}"
            );
            assert!(annotation_bounds(&callout).contains(egui::pos2(point[0], point[1])));
        }
    }

    #[test]
    fn every_callout_style_triangulates_and_respects_alpha_and_text_interior() {
        let cross = |a: Pos2, b: Pos2| a.x * b.y - a.y * b.x;
        for style in crate::callout_style::CalloutStyle::ALL {
            let mut a = crate::callout_style::sample_annotation();
            a.start = Point { x: 100., y: 80. };
            a.end = Point { x: 260., y: 180. };
            a.callout_style = style;
            a.color = [230, 66, 55, 128];
            for tip in [
                [180., 20.],
                [320., 130.],
                [180., 240.],
                [40., 130.],
                [310., 220.],
                [180., 130.],
            ] {
                a.tail = Some(Point {
                    x: tip[0],
                    y: tip[1],
                });
                for outline in crate::callout_style::parts(&a) {
                    let expected = outline
                        .iter()
                        .copied()
                        .zip(outline.iter().copied().cycle().skip(1))
                        .map(|(a, b)| cross(a, b))
                        .sum::<f32>()
                        * 0.5;
                    let mesh = polygon_mesh(&outline, Color32::WHITE);
                    let actual = mesh
                        .indices
                        .chunks_exact(3)
                        .map(|t| {
                            let [a, b, c] =
                                [t[0], t[1], t[2]].map(|i| mesh.vertices[i as usize].pos);
                            (cross(a, b) + cross(b, c) + cross(c, a)) * 0.5
                        })
                        .sum::<f32>();
                    assert!(
                        (expected - actual).abs() < 0.05,
                        "{style:?} {tip:?}: {expected} / {actual}"
                    );
                }
                let mut doc = document(vec![a.clone()]);
                doc.image = Arc::new(RgbaImage::new(400, 300));
                let output = flatten(&doc).unwrap();
                assert!(output.pixels().any(|p| p[3] > 0));
                assert!(
                    output.pixels().all(|p| p[3] <= 128),
                    "overlapping alpha for {style:?}"
                );
                let mut text = a.clone();
                text.text = "Hello there\nCallout".into();
                let metrics = editable_text_layout(&text).unwrap();
                let text_rect = Rect::from_min_size(metrics.origin, metrics.size);
                assert!(
                    crate::callout_style::text_body(&text).contains_rect(text_rect),
                    "{style:?} {text_rect:?}"
                );
            }
        }
    }

    #[test]
    fn callout_and_highlighter_alpha_are_applied_once_per_layer() {
        let mut callout = annotation(AnnotationKind::Callout, [30.0, 30.0], [150.0, 110.0]);
        callout.tail = Some(Point { x: 90.0, y: 170.0 });
        callout.text.clear();
        callout.color = [30, 100, 220, 128];
        let mut doc = document(vec![callout]);
        doc.image = Arc::new(RgbaImage::new(300, 200));
        let pixels = flatten(&doc).unwrap();
        for point in [(90, 60), (90, 105), (90, 115), (90, 140)] {
            assert_eq!(pixels.get_pixel(point.0, point.1)[3], 128);
        }
        let mut highlight = annotation(AnnotationKind::Highlighter, [10.0, 10.0], [100.0, 40.0]);
        highlight.color = [255, 220, 0, 80];
        doc.annotations = vec![highlight.clone()];
        let once = flatten(&doc).unwrap();
        assert_eq!(once.get_pixel(50, 25)[3], 80);
        assert_eq!(once.get_pixel(9, 25)[3], 0);
        doc.annotations.push(highlight);
        assert_eq!(flatten(&doc).unwrap().get_pixel(50, 25)[3], 135);
    }

    #[test]
    fn markup_text_fits_and_cache_tracks_number_and_body_dimensions() {
        let mut step = annotation(AnnotationKind::Step, [20.0, 20.0], [80.0, 80.0]);
        step.step_number = 9999;
        let first = text_raster(&step).unwrap();
        let body = Rect::from_two_pos(pos(step.start), pos(step.end));
        assert!(body.contains_rect(first.bounds.translate(text_anchor(&step).to_vec2())));
        step.step_number = 2;
        let second = text_raster(&step).unwrap();
        assert!(!Arc::ptr_eq(&first, &second));
        step.end = Point { x: 140.0, y: 140.0 };
        let third = text_raster(&step).unwrap();
        assert!(third.size[1] > second.size[1]);
        let mut callout = annotation(AnnotationKind::Callout, [220.0, 160.0], [20.0, 20.0]);
        callout.tail = Some(Point { x: 250.0, y: 190.0 });
        callout.text = "A wrapped callout with an editable tail".into();
        let raster = text_raster(&callout).unwrap();
        let body = Rect::from_two_pos(pos(callout.start), pos(callout.end));
        assert!(body.contains_rect(raster.bounds.translate(text_anchor(&callout).to_vec2())));
        assert!(raster.bounds.height() > callout.font_size);
        callout.text = "W".repeat(64_000);
        callout.end = Point { x: 218.0, y: 158.0 };
        assert!(text_raster(&callout).is_err());
    }

    #[test]
    fn markup_text_colors_and_order_match_privacy_preview_and_geometry() {
        let mut step = annotation(AnnotationKind::Step, [20.0, 20.0], [80.0, 80.0]);
        step.text_color = [0, 255, 0, 255];
        let mut callout = annotation(AnnotationKind::Callout, [100.0, 30.0], [260.0, 120.0]);
        callout.text = "Callout".into();
        callout.text_color = [0, 255, 0, 255];
        callout.tail = Some(Point { x: 280.0, y: 160.0 });
        let mut highlighter = annotation(AnnotationKind::Highlighter, [120.0, 50.0], [240.0, 90.0]);
        highlighter.color = [255, 220, 0, 80];
        let mut doc = document(vec![step, callout, highlighter]);
        let saved = doc.edit_state();
        let original = doc.image.clone();
        let unfiltered = flatten(&doc).unwrap();
        assert!(unfiltered.pixels().any(|p| p.0 == [0, 255, 0, 255]));
        assert_eq!(privacy_preview(&doc, 2048).unwrap(), unfiltered);
        let mut effect = annotation(AnnotationKind::Pixelate, [0.0, 0.0], [300.0, 200.0]);
        effect.effect_size = 16;
        let mut expected = unfiltered;
        crate::privacy::apply(&mut expected, &effect).unwrap();
        doc.annotations.push(effect);
        assert_eq!(flatten(&doc).unwrap(), expected);
        assert_eq!(privacy_preview(&doc, 2048).unwrap(), expected);
        doc.geometry.quarter_turns = 1;
        doc.geometry.resize = Some([100, 150]);
        assert_eq!(
            flatten(&doc).unwrap(),
            doc.geometry.apply(&expected).unwrap()
        );
        assert_eq!(doc.image, original);
        assert_eq!(&doc.annotations[..3], saved.annotations.as_slice());
    }
}
