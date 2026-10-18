//! Adjustable privacy effects are applied to a composited copy of original pixels.
//!
//! Only pixels inside the clipped selection are read or written. Blur uses three
//! separable box passes, with a sliding sum and one reusable scanline: its runtime
//! is linear in selected pixels and its scratch memory is bounded by one image
//! side, independent of blur radius. Every average weights RGB by alpha, so hidden
//! color in transparent pixels cannot bleed into an opaque result.

use crate::document::{self, Annotation, AnnotationKind};
use image::{Rgba, RgbaImage};

#[derive(Clone, Copy)]
struct Region {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

/// Apply one blur or pixelation object to an existing composited image.
/// Invalid dimensions, parameters, or allocation failure leave the image intact.
pub fn apply(image: &mut RgbaImage, annotation: &Annotation) -> Result<(), String> {
    document::validate_dimensions(image.width(), image.height())?;
    document::validate_annotation(annotation)?;
    if !annotation.kind.is_effect() {
        return Err("Only blur and pixelation objects can be applied as privacy effects.".into());
    }
    if annotation.start.x == annotation.end.x || annotation.start.y == annotation.end.y {
        return Ok(());
    }
    let region = Region {
        left: annotation
            .start
            .x
            .min(annotation.end.x)
            .floor()
            .clamp(0.0, image.width() as f32) as u32,
        top: annotation
            .start
            .y
            .min(annotation.end.y)
            .floor()
            .clamp(0.0, image.height() as f32) as u32,
        right: annotation
            .start
            .x
            .max(annotation.end.x)
            .ceil()
            .clamp(0.0, image.width() as f32) as u32,
        bottom: annotation
            .start
            .y
            .max(annotation.end.y)
            .ceil()
            .clamp(0.0, image.height() as f32) as u32,
    };
    if region.left >= region.right || region.top >= region.bottom {
        return Ok(());
    }
    match annotation.kind {
        AnnotationKind::Blur => blur(image, region, annotation.effect_size)?,
        AnnotationKind::Pixelate => pixelate(image, region, annotation.effect_size),
        _ => unreachable!("effect kind was validated above"),
    }
    Ok(())
}

fn weighted(pixel: Rgba<u8>) -> [u64; 4] {
    let alpha = u64::from(pixel[3]);
    [
        u64::from(pixel[0]) * alpha,
        u64::from(pixel[1]) * alpha,
        u64::from(pixel[2]) * alpha,
        alpha,
    ]
}

fn add(sum: &mut [u64; 4], pixel: Rgba<u8>) {
    for (sum, value) in sum.iter_mut().zip(weighted(pixel)) {
        *sum += value;
    }
}

fn remove(sum: &mut [u64; 4], pixel: Rgba<u8>) {
    for (sum, value) in sum.iter_mut().zip(weighted(pixel)) {
        *sum -= value;
    }
}

fn average(sum: [u64; 4], count: u64) -> Rgba<u8> {
    let alpha = (sum[3] + count / 2) / count;
    if alpha == 0 {
        return Rgba([0, 0, 0, 0]);
    }
    Rgba([
        ((sum[0] + sum[3] / 2) / sum[3]) as u8,
        ((sum[1] + sum[3] / 2) / sum[3]) as u8,
        ((sum[2] + sum[3] / 2) / sum[3]) as u8,
        alpha as u8,
    ])
}

fn pixelate(image: &mut RgbaImage, region: Region, block: u32) {
    for top in (region.top..region.bottom).step_by(block as usize) {
        let bottom = (top + block).min(region.bottom);
        for left in (region.left..region.right).step_by(block as usize) {
            let right = (left + block).min(region.right);
            let mut sum = [0; 4];
            for y in top..bottom {
                for x in left..right {
                    add(&mut sum, *image.get_pixel(x, y));
                }
            }
            let color = average(sum, u64::from(right - left) * u64::from(bottom - top));
            for y in top..bottom {
                for x in left..right {
                    image.put_pixel(x, y, color);
                }
            }
        }
    }
}

fn blur(image: &mut RgbaImage, region: Region, radius: u32) -> Result<(), String> {
    let width = (region.right - region.left) as usize;
    let height = (region.bottom - region.top) as usize;
    let mut line = Vec::new();
    line.try_reserve_exact(width.max(height))
        .map_err(|_| "There is not enough memory to render this blur.".to_string())?;
    line.resize(width.max(height), Rgba([0; 4]));
    for _ in 0..3 {
        for y in region.top..region.bottom {
            for (index, pixel) in line[..width].iter_mut().enumerate() {
                *pixel = *image.get_pixel(region.left + index as u32, y);
            }
            blur_line(&line[..width], radius as usize, |index, pixel| {
                image.put_pixel(region.left + index as u32, y, pixel);
            });
        }
        for x in region.left..region.right {
            for (index, pixel) in line[..height].iter_mut().enumerate() {
                *pixel = *image.get_pixel(x, region.top + index as u32);
            }
            blur_line(&line[..height], radius as usize, |index, pixel| {
                image.put_pixel(x, region.top + index as u32, pixel);
            });
        }
    }
    Ok(())
}

/// The window is clipped to the selected region at its boundary. Since the
/// source scanline is immutable, overlapping windows never reuse blurred output.
fn blur_line(line: &[Rgba<u8>], radius: usize, mut write: impl FnMut(usize, Rgba<u8>)) {
    let mut sum = [0; 4];
    let mut left = 0;
    let mut right = (radius + 1).min(line.len());
    for &pixel in &line[..right] {
        add(&mut sum, pixel);
    }
    for index in 0..line.len() {
        write(index, average(sum, (right - left) as u64));
        let next_left = (index + 1).saturating_sub(radius);
        let next_right = (index + radius + 2).min(line.len());
        while left < next_left {
            remove(&mut sum, line[left]);
            left += 1;
        }
        while right < next_right {
            add(&mut sum, line[right]);
            right += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{MAX_EFFECT_SIZE, Point};

    fn effect(kind: AnnotationKind, start: [f32; 2], end: [f32; 2], size: u32) -> Annotation {
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
            color: [0, 0, 0, 255],
            stroke_width: 2.0,
            fill: false,
            text: String::new(),
            font_size: 24.0,
            effect_size: size,
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
            pixel_mask: None,
            arrow: Default::default(),
            studio: Default::default(),
            points: Vec::new(),
        }
    }

    fn pattern(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| {
            let value = if (x + y) % 2 == 0 { 0 } else { 255 };
            Rgba([value, ((x * 31 + y * 47) % 256) as u8, 255 - value, 255])
        })
    }

    #[test]
    fn effects_change_only_selected_pixels_and_obscure_high_frequency_detail() {
        let original = pattern(31, 23);
        for kind in [AnnotationKind::Blur, AnnotationKind::Pixelate] {
            let mut output = original.clone();
            apply(&mut output, &effect(kind, [4.4, 5.7], [27.2, 20.1], 4)).unwrap();
            for (x, y, pixel) in output.enumerate_pixels() {
                if !(4..28).contains(&x) || !(5..21).contains(&y) {
                    assert_eq!(pixel, original.get_pixel(x, y));
                }
            }
            assert_ne!(output, original);
            // Every full blur/pixelation block mixes both checkerboard colors.
            for y in 8..16 {
                for x in 8..24 {
                    assert!((80..=180).contains(&output.get_pixel(x, y)[0]));
                }
            }
        }
    }

    #[test]
    fn transparent_hidden_rgb_is_removed_and_cannot_contaminate_visible_pixels() {
        for kind in [AnnotationKind::Blur, AnnotationKind::Pixelate] {
            let annotation = effect(kind, [1.0, 1.0], [3.0, 3.0], 16);
            let mut transparent = RgbaImage::from_pixel(4, 4, Rgba([40, 150, 250, 0]));
            apply(&mut transparent, &annotation).unwrap();
            assert_eq!(*transparent.get_pixel(1, 1), Rgba([0, 0, 0, 0]));
            assert_eq!(*transparent.get_pixel(0, 0), Rgba([40, 150, 250, 0]));
            let mut mixed = RgbaImage::from_pixel(4, 4, Rgba([0, 0, 255, 0]));
            mixed.put_pixel(1, 1, Rgba([255, 0, 0, 255]));
            apply(&mut mixed, &annotation).unwrap();
            for y in 1..3 {
                for x in 1..3 {
                    assert_eq!(*mixed.get_pixel(x, y), Rgba([255, 0, 0, 64]));
                }
            }
        }
    }

    #[test]
    fn reversed_offcanvas_and_tiny_regions_are_clipped_safely() {
        for kind in [AnnotationKind::Blur, AnnotationKind::Pixelate] {
            let original = pattern(7, 5);
            let mut forward = original.clone();
            let mut reverse = original.clone();
            apply(&mut forward, &effect(kind, [-5.0, -4.0], [4.2, 3.2], 128)).unwrap();
            apply(&mut reverse, &effect(kind, [4.2, 3.2], [-5.0, -4.0], 128)).unwrap();
            assert_eq!(forward, reverse);
            assert_ne!(forward, original);
            let before = forward.clone();
            apply(
                &mut forward,
                &effect(kind, [-10.0, -8.0], [-1.0, -2.0], 128),
            )
            .unwrap();
            apply(&mut forward, &effect(kind, [3.0, 2.0], [3.0, 2.0], 128)).unwrap();
            assert_eq!(forward, before);
            let mut tiny = RgbaImage::from_pixel(1, 1, Rgba([50, 100, 150, 255]));
            apply(&mut tiny, &effect(kind, [0.0, 0.0], [1.0, 1.0], 128)).unwrap();
            assert_eq!(*tiny.get_pixel(0, 0), Rgba([50, 100, 150, 255]));
        }
    }

    #[test]
    fn invalid_parameters_do_not_partially_modify_pixels() {
        let original = pattern(7, 5);
        for kind in [AnnotationKind::Blur, AnnotationKind::Pixelate] {
            for size in [0, 1, MAX_EFFECT_SIZE + 1, u32::MAX] {
                let mut image = original.clone();
                assert!(apply(&mut image, &effect(kind, [0.0, 0.0], [7.0, 5.0], size)).is_err());
                assert_eq!(image, original);
            }
            for coordinate in [f32::NAN, f32::INFINITY, 1_000_001.0] {
                let mut image = original.clone();
                assert!(
                    apply(&mut image, &effect(kind, [coordinate, 0.0], [7.0, 5.0], 16)).is_err()
                );
                assert_eq!(image, original);
            }
        }
        let mut image = original.clone();
        assert!(
            apply(
                &mut image,
                &effect(AnnotationKind::Rectangle, [0.0, 0.0], [7.0, 5.0], 16)
            )
            .is_err()
        );
        assert_eq!(image, original);
    }

    #[test]
    fn effect_order_and_strength_change_composite_without_changing_source() {
        let original = pattern(31, 23);
        let blur = effect(AnnotationKind::Blur, [3.0, 2.0], [29.0, 19.0], 2);
        let pixelate = effect(AnnotationKind::Pixelate, [5.0, 4.0], [23.0, 21.0], 7);
        let mut first = original.clone();
        apply(&mut first, &blur).unwrap();
        apply(&mut first, &pixelate).unwrap();
        let mut second = original.clone();
        apply(&mut second, &pixelate).unwrap();
        apply(&mut second, &blur).unwrap();
        assert_ne!(first, second);
        let mut stronger = original.clone();
        let mut adjusted = blur.clone();
        adjusted.effect_size = 11;
        apply(&mut stronger, &adjusted).unwrap();
        let mut weaker = original.clone();
        apply(&mut weaker, &blur).unwrap();
        assert_ne!(stronger, weaker);
        assert_eq!(original, pattern(31, 23));
    }

    #[test]
    fn scanline_sliding_sum_matches_reference_alpha_weighted_average() {
        let line: Vec<_> = (0..19)
            .map(|index| Rgba([index * 11, 240 - index * 7, index * 3, index * 13]))
            .collect();
        for radius in [2, 5, 128] {
            let mut actual = Vec::new();
            blur_line(&line, radius, |_, pixel| actual.push(pixel));
            for (index, output) in actual.iter().enumerate() {
                let left = index.saturating_sub(radius);
                let right = (index + radius + 1).min(line.len());
                let mut sum = [0; 4];
                for &pixel in &line[left..right] {
                    add(&mut sum, pixel);
                }
                assert_eq!(*output, average(sum, (right - left) as u64));
            }
        }
    }
}
