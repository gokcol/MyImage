//! Reversible focus effects. All sampling uses the composition below the effect.
use crate::document::{Annotation, AnnotationKind};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    pub zoom: f32,
    pub ellipse: bool,
    pub dim: u8,
    pub blur: u32,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            zoom: 2.,
            ellipse: true,
            dim: 60,
            blur: 0,
        }
    }
}
impl Options {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
    pub fn validate(&self) -> Result<(), String> {
        if !self.zoom.is_finite()
            || !(1.0..=8.).contains(&self.zoom)
            || self.dim > 100
            || self.blur > 128
        {
            Err("Invalid magnification or spotlight settings.".into())
        } else {
            Ok(())
        }
    }
}
pub fn polygon_contains(points: &[crate::document::Point], x: f32, y: f32) -> bool {
    let mut inside = false;
    for (a, b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        if (a.y > y) != (b.y > y) && x < (b.x - a.x) * (y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}
pub fn contains(a: &Annotation, x: f32, y: f32) -> bool {
    let l = a.start.x.min(a.end.x);
    let t = a.start.y.min(a.end.y);
    let w = (a.end.x - a.start.x).abs();
    let h = (a.end.y - a.start.y).abs();
    if w <= 0. || h <= 0. {
        return false;
    }
    let nx = (x - l) / w;
    let ny = (y - t) / h;
    (0.0..1.).contains(&nx)
        && (0.0..1.).contains(&ny)
        && (!a.studio.ellipse || (nx * 2. - 1.).powi(2) + (ny * 2. - 1.).powi(2) <= 1.)
        && a.pixel_mask.as_ref().is_none_or(|m| m.contains(nx, ny))
        && (a.area_mask.is_empty() || polygon_contains(&a.area_mask, nx, ny))
}
fn blend(dst: &mut Rgba<u8>, color: [u8; 4]) {
    let a = color[3] as f32 / 255.;
    let da = dst[3] as f32 / 255.;
    let out = a + da * (1. - a);
    if out <= 0. {
        *dst = Rgba([0; 4]);
        return;
    }
    for c in 0..3 {
        dst[c] = ((color[c] as f32 * a + dst[c] as f32 * da * (1. - a)) / out).round() as u8;
    }
    dst[3] = (out * 255.).round() as u8;
}
pub fn apply(
    image: &mut RgbaImage,
    annotation: &Annotation,
    scale: [f32; 2],
) -> Result<(), String> {
    let mut a = annotation.clone();
    a.start.x *= scale[0];
    a.start.y *= scale[1];
    a.end.x *= scale[0];
    a.end.y *= scale[1];
    let l = a.start.x.min(a.end.x);
    let t = a.start.y.min(a.end.y);
    let r = a.start.x.max(a.end.x);
    let b = a.start.y.max(a.end.y);
    if r - l < 0.01 || b - t < 0.01 {
        return Ok(());
    }
    match a.kind {
        AnnotationKind::Magnify => {
            // Snapshot only the source rectangle; never sample already enlarged pixels.
            let x = l.floor().clamp(0., image.width() as f32) as u32;
            let y = t.floor().clamp(0., image.height() as f32) as u32;
            let right = r.ceil().clamp(0., image.width() as f32) as u32;
            let bottom = b.ceil().clamp(0., image.height() as f32) as u32;
            if right <= x || bottom <= y {
                return Ok(());
            }
            let source = image::imageops::crop_imm(image, x, y, right - x, bottom - y).to_image();
            let cx = (l + r) / 2.;
            let cy = (t + b) / 2.;
            let border = annotation.stroke_width * scale[0].min(scale[1]);
            for py in y..bottom {
                for px in x..right {
                    let fx = px as f32 + 0.5;
                    let fy = py as f32 + 0.5;
                    if !contains(&a, fx, fy) {
                        continue;
                    }
                    let sx = (cx + (fx - cx) / a.studio.zoom)
                        .floor()
                        .clamp(x as f32, (right - 1) as f32) as u32;
                    let sy = (cy + (fy - cy) / a.studio.zoom)
                        .floor()
                        .clamp(y as f32, (bottom - 1) as f32) as u32;
                    let mut pixel = *source.get_pixel(sx - x, sy - y);
                    let edge = if a.studio.ellipse {
                        let d = (((fx - cx) / ((r - l) / 2.)).powi(2)
                            + ((fy - cy) / ((b - t) / 2.)).powi(2))
                        .sqrt();
                        (1. - d) * (r - l).min(b - t) / 2. < border
                    } else {
                        (fx - l).min(r - fx).min(fy - t).min(b - fy) < border
                    };
                    if edge {
                        blend(&mut pixel, a.color);
                    }
                    image.put_pixel(px, py, pixel);
                }
            }
        }
        AnnotationKind::Spotlight => {
            let original = if a.studio.blur > 0 {
                Some(image.clone())
            } else {
                None
            };
            if a.studio.blur > 0 {
                let mut blur = a.clone();
                blur.kind = AnnotationKind::Blur;
                blur.studio = Default::default();
                blur.pixel_mask = None;
                blur.start = crate::document::Point { x: 0., y: 0. };
                blur.end = crate::document::Point {
                    x: image.width() as f32,
                    y: image.height() as f32,
                };
                blur.effect_size = (a.studio.blur as f32 * scale[0].max(scale[1]))
                    .round()
                    .clamp(2., 128.) as u32;
                crate::privacy::apply(image, &blur)?;
            }
            for y in 0..image.height() {
                for x in 0..image.width() {
                    if contains(&a, x as f32 + 0.5, y as f32 + 0.5) {
                        if let Some(source) = &original {
                            image.put_pixel(x, y, *source.get_pixel(x, y));
                        }
                    } else {
                        let p = image.get_pixel_mut(x, y);
                        for c in 0..3 {
                            p[c] = (u32::from(p[c]) * (100 - u32::from(a.studio.dim)) / 100) as u8;
                        }
                    }
                }
            }
        }
        AnnotationKind::Simplify => {
            for y in t.floor().max(0.) as u32..b.ceil().clamp(0., image.height() as f32) as u32 {
                for x in l.floor().max(0.) as u32..r.ceil().clamp(0., image.width() as f32) as u32 {
                    if contains(&a, x as f32 + 0.5, y as f32 + 0.5) {
                        blend(image.get_pixel_mut(x, y), a.color);
                    }
                }
            }
        }
        _ => return Err("Unsupported focus effect.".into()),
    }
    Ok(())
}

/// Identify horizontal runs of contrast on a uniform background. The result is
/// a set of editable blocks, deliberately bounded for large screenshots.
pub fn simplify_regions(image: &RgbaImage, sensitivity: u8) -> Vec<[u32; 4]> {
    let max = 1200.;
    let scale = (max / image.width().max(image.height()) as f32).min(1.);
    let small = image::imageops::resize(
        image,
        (image.width() as f32 * scale).round().max(1.) as u32,
        (image.height() as f32 * scale).round().max(1.) as u32,
        image::imageops::FilterType::Triangle,
    );
    let mut regions: Vec<[u32; 4]> = Vec::new();
    let mut counts = std::collections::BTreeMap::new();
    for p in small.pixels().filter(|p| p[3] > 128) {
        *counts
            .entry([p[0] / 16, p[1] / 16, p[2] / 16])
            .or_insert(0usize) += 1;
    }
    let bg = counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(c, _)| c.map(|v| v as i16 * 16 + 8))
        .unwrap_or([255; 3]);
    for y in 0..small.height() {
        let threshold = 100 - (sensitivity as i16 * 3 / 4);
        let fg = |x| {
            let p = small.get_pixel(x, y);
            p[3] > 128
                && (0..3)
                    .map(|c| (p[c] as i16 - bg[c]).abs())
                    .max()
                    .unwrap_or(0)
                    > threshold
        };
        let mut x = 0;
        while x < small.width() {
            if !fg(x) {
                x += 1;
                continue;
            }
            let start = x;
            let mut end = x + 1;
            let mut gap = 0;
            while x < small.width() && gap < 6 {
                if fg(x) {
                    end = x + 1;
                    gap = 0;
                } else {
                    gap += 1;
                }
                x += 1;
            }
            if end - start < 3 || end - start > small.width() * 3 / 4 {
                continue;
            }
            if let Some(last) =
                regions.iter_mut().rev().take(80).find(|a| {
                    a[3] + 1 >= y && a[0] <= end + 3 && start <= a[2] + 3 && y - a[1] < 38
                })
            {
                last[0] = last[0].min(start);
                last[2] = last[2].max(end);
                last[3] = y + 1;
            } else if regions.len() < 1000 {
                regions.push([start, y, end, y + 1]);
            }
        }
    }
    regions
        .into_iter()
        .filter(|a| a[3] - a[1] >= 3 && a[3] - a[1] <= 38 && a[2] - a[0] >= 8)
        .take(400)
        .map(|a| a.map(|v| (v as f32 / scale).round() as u32))
        .collect()
}
