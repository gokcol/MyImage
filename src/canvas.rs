//! Transparent canvas growth without replacing original pixels or flattening layers.
use crate::{
    document::{Document, Point},
    geometry::{CanvasRect, ImageGeometry},
};
use eframe::egui::{self, Rect};
use image::RgbaImage;

/// Run flood selection in a source-coordinate composition, including transparent
/// canvas margins. Returned mask coordinates remain local to the annotation.
pub fn fill_region(
    document: &Document,
    annotation: &crate::document::Annotation,
    seed: Point,
    tolerance: u8,
    global: bool,
) -> Result<crate::fill::FillRegion, String> {
    let mut source = if document.geometry.canvas.is_some() {
        render_document(document)?
    } else {
        document.clone()
    };
    let mut annotation = annotation.clone();
    let mut seed = seed;
    if let Some(bounds) = document.geometry.canvas {
        for p in [&mut annotation.start, &mut annotation.end, &mut seed] {
            p.x -= bounds.x as f32;
            p.y -= bounds.y as f32;
        }
    }
    source.geometry = Default::default();
    let pixels = crate::render::flatten(&source)?;
    crate::fill::build(&pixels, &annotation, seed, tolerance, global)
}

pub fn prepare_image(
    image: &RgbaImage,
    geometry: ImageGeometry,
) -> Result<(RgbaImage, ImageGeometry), String> {
    let original = [image.width(), image.height()];
    geometry.validate(original)?;
    let bounds = geometry.canvas_rect(original);
    let crop = geometry.crop_rect(original);
    let mut pixels = RgbaImage::new(bounds.width, bounds.height);
    for y in crop.y..crop.y + crop.height {
        for x in crop.x..crop.x + crop.width {
            pixels.put_pixel(
                (x as i32 - bounds.x) as u32,
                (y as i32 - bounds.y) as u32,
                *image.get_pixel(x, y),
            );
        }
    }
    let mut transformed = geometry;
    transformed.canvas = None;
    transformed.effects.shift(bounds.x, bounds.y);
    transformed.crop = geometry.viewport.map(|v| crate::geometry::CropRect {
        x: (v.x - bounds.x) as u32,
        y: (v.y - bounds.y) as u32,
        width: v.width,
        height: v.height,
    });
    transformed.viewport = None;
    for cut in &mut transformed.cutouts {
        *cut = cut.and_then(|mut c| {
            let (offset, size) = if c.axis == 0 {
                (bounds.x, bounds.width)
            } else {
                (bounds.y, bounds.height)
            };
            c.start = (i64::from(c.start) - i64::from(offset)).clamp(0, i64::from(size)) as u32;
            c.end = (i64::from(c.end) - i64::from(offset)).clamp(0, i64::from(size)) as u32;
            (c.end > c.start).then_some(c)
        });
    }
    Ok((pixels, transformed))
}

pub fn render_document(document: &Document) -> Result<Document, String> {
    let bounds = document
        .geometry
        .canvas_rect([document.image.width(), document.image.height()]);
    let (pixels, geometry) = prepare_image(&document.image, document.geometry)?;
    let shift = |p: &mut Point| {
        p.x -= bounds.x as f32;
        p.y -= bounds.y as f32;
    };
    let mut annotations = document.annotations.clone();
    for a in &mut annotations {
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
    Ok(Document {
        image: std::sync::Arc::new(pixels),
        annotations,
        geometry,
    })
}

/// Extend to include source-coordinate bounds, maintaining the current display scale.
pub fn extend_to(document: &mut Document, rect: Rect) -> Result<bool, String> {
    if !rect.is_finite() || !rect.is_positive() {
        return Err("Invalid canvas bounds.".into());
    }
    let original = [document.image.width(), document.image.height()];
    let old = document.geometry;
    if old.viewport.is_some() {
        let b = old.view_rect(original);
        let visible = Rect::from_min_size(
            egui::pos2(b.x as f32, b.y as f32),
            egui::vec2(b.width as f32, b.height as f32),
        );
        return if visible.contains_rect(rect) {
            Ok(false)
        } else {
            set_view_rect(document, visible.union(rect))
        };
    }
    let b = old.canvas_rect(original);
    let current = Rect::from_min_size(
        egui::pos2(b.x as f32, b.y as f32),
        egui::vec2(b.width as f32, b.height as f32),
    );
    if current.contains_rect(rect) {
        return Ok(false);
    }
    let union = current.union(rect);
    let min = union.min.floor();
    let max = union.max.ceil();
    let mut next = old;
    next.canvas = Some(CanvasRect {
        x: min.x as i32,
        y: min.y as i32,
        width: (max.x - min.x) as u32,
        height: (max.y - min.y) as u32,
    });
    next.validate(original)?;
    if let Some(size) = old.resize {
        let before = old.rotated_size(original);
        let after = next.rotated_size(original);
        next.resize = Some([0, 1].map(|i| {
            (f64::from(size[i]) * f64::from(after[i]) / f64::from(before[i]))
                .round()
                .max(1.) as u32
        }));
    }
    next.validate(original)?;
    document.geometry = next;
    Ok(true)
}

pub fn extend_for_annotations(document: &mut Document, indices: &[usize]) -> Result<bool, String> {
    let bounds = indices
        .iter()
        .filter_map(|&i| document.annotations.get(i))
        .map(crate::render::annotation_bounds)
        .fold(Rect::NOTHING, Rect::union);
    if bounds.is_positive() {
        extend_to(document, bounds)
    } else {
        Ok(false)
    }
}

/// Resize the visible window while retaining the original crop and every layer.
pub fn set_view_rect(document: &mut Document, rect: Rect) -> Result<bool, String> {
    if !rect.is_finite() || !rect.is_positive() {
        return Err("Invalid canvas size.".into());
    }
    let original = [document.image.width(), document.image.height()];
    let old = document.geometry;
    let min = rect.min.floor();
    let max = rect.max.ceil();
    let view = CanvasRect {
        x: min.x as i32,
        y: min.y as i32,
        width: (max.x - min.x) as u32,
        height: (max.y - min.y) as u32,
    };
    let b = old.canvas_rect(original);
    let retained = Rect::from_min_size(
        egui::pos2(b.x as f32, b.y as f32),
        egui::vec2(b.width as f32, b.height as f32),
    )
    .union(Rect::from_min_max(min, max));
    let mut next = old;
    next.canvas = Some(CanvasRect {
        x: retained.min.x as i32,
        y: retained.min.y as i32,
        width: retained.width() as u32,
        height: retained.height() as u32,
    });
    next.viewport = Some(view);
    if let Some(size) = old.resize {
        let before = old.rotated_size(original);
        let after = next.rotated_size(original);
        next.resize = Some([0, 1].map(|i| {
            (f64::from(size[i]) * f64::from(after[i]) / f64::from(before[i]))
                .round()
                .max(1.) as u32
        }));
    }
    next.validate(original)?;
    document.geometry = next;
    Ok(next != old)
}
