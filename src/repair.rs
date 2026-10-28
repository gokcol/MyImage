//! Bounded local repair. Surrounding colors interpolate the four outside edges;
//! cloning preserves texture when the user chooses a neighboring source.
use image::{Rgba, RgbaImage};
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Method {
    #[default]
    Surrounding,
    Left,
    Right,
    Above,
    Below,
}
impl Method {
    pub const ALL: [Self; 5] = [
        Self::Surrounding,
        Self::Left,
        Self::Right,
        Self::Above,
        Self::Below,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Surrounding => "Surrounding colors",
            Self::Left => "Clone left",
            Self::Right => "Clone right",
            Self::Above => "Clone above",
            Self::Below => "Clone below",
        }
    }
}
pub fn patch(image: &RgbaImage, rect: [u32; 4], method: Method) -> Result<RgbaImage, String> {
    let [x, y, w, h] = rect;
    if w == 0
        || h == 0
        || x.saturating_add(w) > image.width()
        || y.saturating_add(h) > image.height()
        || u64::from(w) * u64::from(h) > 4_000_000
    {
        return Err("Select a region of at most four million pixels inside the canvas.".into());
    }
    if method != Method::Surrounding {
        let (dx, dy) = match method {
            Method::Left => (-(i64::from(w) + 2), 0),
            Method::Right => (i64::from(w) + 2, 0),
            Method::Above => (0, -(i64::from(h) + 2)),
            Method::Below => (0, i64::from(h) + 2),
            _ => unreachable!(),
        };
        let (sx, sy) = (i64::from(x) + dx, i64::from(y) + dy);
        if sx < 0
            || sy < 0
            || sx + i64::from(w) > i64::from(image.width())
            || sy + i64::from(h) > i64::from(image.height())
        {
            return Err(
                "There is not enough image on that side. Choose another repair source.".into(),
            );
        }
        return Ok(image::imageops::crop_imm(image, sx as u32, sy as u32, w, h).to_image());
    }
    let sides = [x > 0, x + w < image.width(), y > 0, y + h < image.height()];
    if !sides.iter().any(|v| *v) {
        return Err("Leave some surrounding pixels outside the selection so the background can be repaired.".into());
    }
    Ok(RgbaImage::from_fn(w, h, |px, py| {
        let samples = [
            (
                image.get_pixel(x.saturating_sub(1), y + py),
                px + 1,
                sides[0],
            ),
            (
                image.get_pixel((x + w).min(image.width() - 1), y + py),
                w - px,
                sides[1],
            ),
            (
                image.get_pixel(x + px, y.saturating_sub(1)),
                py + 1,
                sides[2],
            ),
            (
                image.get_pixel(x + px, (y + h).min(image.height() - 1)),
                h - py,
                sides[3],
            ),
        ];
        let mut channels = [0.; 4];
        let mut total = 0.;
        for (p, distance, available) in samples {
            if !available {
                continue;
            }
            let weight = 1. / f64::from(distance);
            let alpha = f64::from(p[3]) / 255.;
            for c in 0..3 {
                channels[c] += f64::from(p[c]) * alpha * weight;
            }
            channels[3] += alpha * weight;
            total += weight;
        }
        let alpha = channels[3] / total;
        if alpha <= 0. {
            Rgba([0; 4])
        } else {
            Rgba([
                (channels[0] / channels[3]).round() as u8,
                (channels[1] / channels[3]).round() as u8,
                (channels[2] / channels[3]).round() as u8,
                (alpha * 255.).round() as u8,
            ])
        }
    }))
}
pub fn text_ink(original: &RgbaImage, _background: &RgbaImage) -> [u8; 4] {
    let mut bins = std::collections::BTreeMap::<[u8; 3], (u64, [u64; 3])>::new();
    for a in original.pixels().filter(|p| p[3] >= 200) {
        let entry = bins.entry([a[0] / 16, a[1] / 16, a[2] / 16]).or_default();
        entry.0 += 1;
        for c in 0..3 {
            entry.1[c] += u64::from(a[c]);
        }
    }
    let average = |(n, sum): &(u64, [u64; 3])| {
        [
            (sum[0] / n) as u8,
            (sum[1] / n) as u8,
            (sum[2] / n) as u8,
            255,
        ]
    };
    let Some(background) = bins.values().max_by_key(|v| v.0).map(average) else {
        return [20, 20, 20, 255];
    };
    bins.values()
        .filter(|v| {
            let color = average(v);
            (0..3)
                .map(|c| u32::from(color[c].abs_diff(background[c])))
                .sum::<u32>()
                >= 80
        })
        .max_by_key(|v| v.0)
        .map_or([20, 20, 20, 255], average)
}
/// A mostly uniform border is common around screenshot text. Ignore isolated
/// foreground letters touching that border instead of spreading them into streaks.
pub fn text_patch(image: &RgbaImage, rect: [u32; 4]) -> Result<RgbaImage, String> {
    let repaired = patch(image, rect, Method::Surrounding)?;
    let [x, y, w, h] = rect;
    let mut samples = Vec::new();
    if x > 0 {
        samples.extend((y..y + h).map(|py| *image.get_pixel(x - 1, py)));
    }
    if x + w < image.width() {
        samples.extend((y..y + h).map(|py| *image.get_pixel(x + w, py)));
    }
    if y > 0 {
        samples.extend((x..x + w).map(|px| *image.get_pixel(px, y - 1)));
    }
    if y + h < image.height() {
        samples.extend((x..x + w).map(|px| *image.get_pixel(px, y + h)));
    }
    let mut bins = std::collections::BTreeMap::<[u8; 4], (usize, [u64; 4])>::new();
    for p in &samples {
        let entry = bins.entry(p.0.map(|v| v / 8)).or_default();
        entry.0 += 1;
        for c in 0..4 {
            entry.1[c] += u64::from(p[c]);
        }
    }
    if let Some((count, sum)) = bins.values().max_by_key(|v| v.0)
        && count * 5 >= samples.len() * 3
    {
        return Ok(RgbaImage::from_pixel(
            w,
            h,
            Rgba(sum.map(|v| (v / (*count as u64)) as u8)),
        ));
    }
    Ok(repaired)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_colors_ignore_background_streaks_and_handle_light_text() {
        for (background, ink) in [([255; 4], [20, 30, 40, 255]), ([20, 30, 40, 255], [255; 4])] {
            let mut image = RgbaImage::from_pixel(60, 40, Rgba(background));
            for y in 14..26 {
                for x in 20..23 {
                    image.put_pixel(x, y, Rgba(ink));
                }
            }
            let original = image::imageops::crop_imm(&image, 10, 12, 40, 14).to_image();
            let repaired = text_patch(&image, [10, 12, 40, 14]).unwrap();
            assert!(repaired.pixels().all(|p| p.0 == background));
            assert_eq!(text_ink(&original, &repaired), ink);
        }
    }
    #[test]
    fn repair_uses_only_outside_pixels_and_leaves_input_untouched() {
        let mut image = RgbaImage::from_pixel(40, 30, Rgba([30, 80, 120, 255]));
        for y in 8..18 {
            for x in 10..20 {
                image.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            }
        }
        let before = image.clone();
        let output = patch(&image, [10, 8, 10, 10], Method::Surrounding).unwrap();
        assert!(output.pixels().all(|p| p.0 == [30, 80, 120, 255]));
        assert_eq!(image, before);
        assert!(patch(&image, [0, 0, 40, 30], Method::Surrounding).is_err());
        assert!(patch(&image, [0, 0, 10, 10], Method::Left).is_err());
    }
    #[test]
    fn clones_are_pixel_exact_and_transparency_never_leaks_hidden_color() {
        let image = RgbaImage::from_fn(40, 30, |x, y| Rgba([x as u8, y as u8, 100, 255]));
        assert_eq!(
            patch(&image, [20, 10, 8, 8], Method::Left).unwrap(),
            image::imageops::crop_imm(&image, 10, 10, 8, 8).to_image()
        );
        let image = RgbaImage::from_pixel(20, 20, Rgba([255, 0, 0, 0]));
        assert!(
            patch(&image, [5, 5, 10, 10], Method::Surrounding)
                .unwrap()
                .pixels()
                .all(|p| p.0 == [0; 4])
        );
    }
}
