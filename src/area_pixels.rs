//! Reversible area operations sample the composition below them.
use crate::document::{Annotation, AnnotationKind, Point};
use image::{Rgba, RgbaImage};

fn bounds(a: Point, b: Point, scale: [f32; 2]) -> [i64; 4] {
    [
        (a.x * scale[0]).floor() as i64,
        (a.y * scale[1]).floor() as i64,
        (b.x * scale[0]).ceil() as i64,
        (b.y * scale[1]).ceil() as i64,
    ]
}
fn clipped(rect: [i64; 4], image: &RgbaImage) -> [u32; 4] {
    [
        rect[0].clamp(0, image.width() as i64) as u32,
        rect[1].clamp(0, image.height() as i64) as u32,
        rect[2].clamp(0, image.width() as i64) as u32,
        rect[3].clamp(0, image.height() as i64) as u32,
    ]
}
/// Even/odd scan conversion supports concave and self-crossing lassos. Work is
/// bounded by path points per row, rather than path points per image pixel.
pub(crate) fn spans(
    mask: &[Point],
    rect: [i64; 4],
    y: u32,
    left: u32,
    right: u32,
) -> Vec<(u32, u32)> {
    if mask.is_empty() {
        return vec![(left, right)];
    }
    let py = (y as f64 + 0.5 - rect[1] as f64) / (rect[3] - rect[1]).max(1) as f64;
    let mut crossings = Vec::new();
    for (a, b) in mask
        .iter()
        .zip(mask.iter().cycle().skip(1))
        .take(mask.len())
    {
        if (a.y as f64 > py) != (b.y as f64 > py) {
            let x = a.x as f64
                + (b.x as f64 - a.x as f64) * (py - a.y as f64) / (b.y as f64 - a.y as f64);
            crossings.push((rect[0] as f64 + x * (rect[2] - rect[0]) as f64 - 0.5).ceil() as i64);
        }
    }
    crossings.sort_unstable();
    crossings
        .chunks_exact(2)
        .map(|p| {
            (
                p[0].clamp(left as i64, right as i64) as u32,
                p[1].clamp(left as i64, right as i64) as u32,
            )
        })
        .filter(|(a, b)| b > a)
        .collect()
}
/// Intersect a lasso with a saved wand mask. Runs preserve holes and disconnected islands.
pub(crate) fn masked_spans(
    edit: &Annotation,
    rect: [i64; 4],
    y: u32,
    left: u32,
    right: u32,
) -> Vec<(u32, u32)> {
    let base = spans(&edit.area_mask, rect, y, left, right);
    let Some(mask) = &edit.pixel_mask else {
        return base;
    };
    let ny = (y as f64 + 0.5 - rect[1] as f64) / (rect[3] - rect[1]).max(1) as f64;
    if !(0.0..1.).contains(&ny) {
        return vec![];
    }
    let row = (ny * mask.height as f64).floor() as u32;
    let first = mask.runs.partition_point(|r| r[0] < row);
    let mut out = Vec::new();
    for &[_, l, r] in mask.runs[first..].iter().take_while(|r| r[0] == row) {
        let map = |x: u32| {
            (rect[0] as f64 + x as f64 / mask.width as f64 * (rect[2] - rect[0]) as f64 - 0.5)
                .ceil()
                .clamp(left as f64, right as f64) as u32
        };
        let (l, r) = (map(l), map(r));
        for &(a, b) in &base {
            let (a, b) = (a.max(l), b.min(r));
            if b > a {
                out.push((a, b));
            }
        }
    }
    out
}
fn clear(image: &mut RgbaImage, rect: [i64; 4], edit: &Annotation) {
    let [x, y, r, b] = clipped(rect, image);
    for py in y..b {
        for (left, right) in masked_spans(edit, rect, py, x, r) {
            for px in left..right {
                image.put_pixel(px, py, Rgba([0; 4]));
            }
        }
    }
}
pub fn apply(image: &mut RgbaImage, edit: &Annotation, scale: [f32; 2]) -> Result<(), String> {
    let destination = bounds(edit.start, edit.end, scale);
    if edit.kind == AnnotationKind::DeleteArea {
        clear(image, destination, edit);
        return Ok(());
    }
    let Some([a, b]) = edit.area_source else {
        return Err("Area move has no source.".into());
    };
    let source = bounds(a, b, scale);
    let [x, y, r, b] = clipped(source, image);
    if r <= x || b <= y || destination[2] <= destination[0] || destination[3] <= destination[1] {
        return Ok(());
    }
    // Snapshot before clearing so overlapping moves never smear the source.
    let pixels = image::imageops::crop_imm(image, x, y, r - x, b - y).to_image();
    clear(image, source, edit);
    let [dx, dy, dr, db] = clipped(destination, image);
    for py in dy..db {
        for (left, right) in masked_spans(edit, destination, py, dx, dr) {
            for px in left..right {
                let sx = source[0]
                    + ((px as i64 - destination[0]) * (source[2] - source[0]))
                        / (destination[2] - destination[0]);
                let sy = source[1]
                    + ((py as i64 - destination[1]) * (source[3] - source[1]))
                        / (destination[3] - destination[1]);
                if sx >= x as i64 && sx < r as i64 && sy >= y as i64 && sy < b as i64 {
                    image.put_pixel(
                        px,
                        py,
                        *pixels.get_pixel((sx - x as i64) as u32, (sy - y as i64) as u32),
                    );
                }
            }
        }
    }
    Ok(())
}
pub fn apply_filter(image: &mut RgbaImage, edit: &Annotation) -> Result<(), String> {
    if edit.area_mask.is_empty() && edit.pixel_mask.is_none() {
        return crate::privacy::apply(image, edit);
    }
    let rect = bounds(edit.start, edit.end, [1.; 2]);
    let [x, y, r, b] = clipped(rect, image);
    if r <= x || b <= y {
        return Ok(());
    }
    let original = image::imageops::crop_imm(image, x, y, r - x, b - y).to_image();
    crate::privacy::apply(image, edit)?;
    for py in y..b {
        let mut previous = x;
        for (left, right) in masked_spans(edit, rect, py, x, r) {
            for px in previous..left {
                image.put_pixel(px, py, *original.get_pixel(px - x, py - y));
            }
            previous = right;
        }
        for px in previous..r {
            image.put_pixel(px, py, *original.get_pixel(px - x, py - y));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn edit(kind: AnnotationKind) -> Annotation {
        Annotation {
            kind,
            start: Point { x: 2., y: 2. },
            end: Point { x: 12., y: 12. },
            color: [0; 4],
            stroke_width: 1.,
            fill: false,
            text: String::new(),
            font_size: 24.,
            effect_size: 4,
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
            arrow: Default::default(),
            studio: Default::default(),
        }
    }
    fn pixels() -> RgbaImage {
        RgbaImage::from_fn(20, 20, |x, y| {
            Rgba([((x + y) % 2 * 240) as u8, x as u8, y as u8, 255])
        })
    }
    #[test]
    fn overlapping_moves_copy_before_clearing_and_roundtrip_original_and_geometry() {
        let original = std::sync::Arc::new(pixels());
        let mut a = edit(AnnotationKind::MoveArea);
        a.area_source = Some([a.start, a.end]);
        a.start.x += 4.;
        a.end.x += 4.;
        let mut doc = crate::document::Document {
            image: original.clone(),
            annotations: vec![a],
            geometry: Default::default(),
        };
        let flat = crate::render::flatten(&doc).unwrap();
        assert_eq!(flat.get_pixel(7, 5), original.get_pixel(3, 5));
        assert_eq!(flat.get_pixel(3, 5)[3], 0);
        doc.geometry.quarter_turns = 1;
        doc.geometry.resize = Some([40, 20]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("area.myimage");
        crate::document::save_project(&doc, &path).unwrap();
        let loaded = crate::document::load_project(&path).unwrap();
        assert_eq!(loaded.edit_state(), doc.edit_state());
        assert_eq!(loaded.image, original);
        assert_eq!(
            crate::render::flatten(&loaded).unwrap(),
            doc.geometry.apply(&flat).unwrap()
        );
        assert_eq!(crate::render::privacy_preview(&loaded, 20).unwrap(), flat);
        assert!(crate::render::thumbnail(&loaded, 10, 10).is_ok());
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[8..12].copy_from_slice(&8u32.to_le_bytes());
        std::fs::write(&path, bytes).unwrap();
        assert!(
            crate::document::load_project(&path)
                .unwrap_err()
                .contains("version 9")
        );
    }
    #[test]
    fn lasso_blur_and_pixelate_do_not_change_pixels_outside_the_path() {
        for kind in [AnnotationKind::Blur, AnnotationKind::Pixelate] {
            let original = pixels();
            let mut image = original.clone();
            let mut a = edit(kind);
            a.area_mask = vec![
                Point { x: 0., y: 0. },
                Point { x: 1., y: 0. },
                Point { x: 0., y: 1. },
            ];
            apply_filter(&mut image, &a).unwrap();
            assert_eq!(image.get_pixel(11, 11), original.get_pixel(11, 11));
            assert_ne!(image.get_pixel(4, 4), original.get_pixel(4, 4));
        }
    }
    #[test]
    fn masked_move_preserves_pixels_outside_source_and_destination_silhouettes() {
        let original = pixels();
        let mut image = original.clone();
        let mut a = edit(AnnotationKind::MoveArea);
        a.area_source = Some([a.start, a.end]);
        a.start.x += 5.;
        a.end.x += 5.;
        a.area_mask = vec![
            Point { x: 0., y: 0. },
            Point { x: 1., y: 0. },
            Point { x: 0., y: 1. },
        ];
        apply(&mut image, &a, [1.; 2]).unwrap();
        assert_eq!(image.get_pixel(3, 3)[3], 0);
        assert_eq!(image.get_pixel(8, 3), original.get_pixel(3, 3));
        assert_eq!(image.get_pixel(11, 11), original.get_pixel(11, 11));
        assert_eq!(image.get_pixel(16, 11), original.get_pixel(16, 11));
    }
}
