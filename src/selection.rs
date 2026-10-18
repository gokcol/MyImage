//! Display-independent selection geometry and non-destructive capture masks.
//!
//! Selection coordinates are local to the displayed screenshot, in logical
//! points. No desktop origin or platform scale factor is needed: the screenshot's
//! actual dimensions determine the conversion to image pixels.

use image::RgbaImage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaptureShape {
    #[default]
    Rectangle,
    RoundedRectangle,
    Ellipse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Convert a drag to a bounded image-pixel rectangle, including partially
/// selected pixels. Reversed drags and unequal horizontal/vertical scaling are
/// supported. An empty, fully out-of-bounds, or non-finite selection is rejected.
pub fn selection_to_pixels(
    start: [f32; 2],
    end: [f32; 2],
    view_size: [f32; 2],
    image_size: [u32; 2],
) -> Option<PixelRect> {
    if start
        .iter()
        .chain(end.iter())
        .any(|value| !value.is_finite())
        || view_size
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
        || image_size.contains(&0)
    {
        return None;
    }

    let mut lower = [0; 2];
    let mut upper = [0; 2];
    for axis in 0..2 {
        let view_extent = f64::from(view_size[axis]);
        let image_extent = f64::from(image_size[axis]);
        let a = f64::from(start[axis]).clamp(0.0, view_extent);
        let b = f64::from(end[axis]).clamp(0.0, view_extent);
        // Check before rounding: a click at a fractional pixel is still empty.
        if a == b {
            return None;
        }
        lower[axis] = (a.min(b) / view_extent * image_extent).floor() as u32;
        upper[axis] = ((a.max(b) / view_extent * image_extent).ceil() as u32).min(image_size[axis]);
    }

    Some(PixelRect {
        x: lower[0],
        y: lower[1],
        width: upper[0].checked_sub(lower[0]).filter(|size| *size > 0)?,
        height: upper[1].checked_sub(lower[1]).filter(|size| *size > 0)?,
    })
}

/// Crop without changing the source image, then multiply its existing alpha by
/// an antialiased shape mask. `corner_radius` is measured in image pixels and is
/// bounded to half the shorter side. Invalid rectangles return an error.
pub fn crop_selection(
    image: &RgbaImage,
    rect: PixelRect,
    shape: CaptureShape,
    corner_radius: u32,
) -> Result<RgbaImage, String> {
    if rect.width == 0 || rect.height == 0 {
        return Err("The capture selection is empty.".to_owned());
    }
    let right = rect.x.checked_add(rect.width);
    let bottom = rect.y.checked_add(rect.height);
    if !matches!(right, Some(right) if right <= image.width())
        || !matches!(bottom, Some(bottom) if bottom <= image.height())
    {
        return Err("The capture selection extends beyond the screenshot.".to_owned());
    }

    let mut cropped =
        image::imageops::crop_imm(image, rect.x, rect.y, rect.width, rect.height).to_image();
    match shape {
        CaptureShape::Rectangle => {}
        CaptureShape::RoundedRectangle => mask_rounded(&mut cropped, corner_radius),
        CaptureShape::Ellipse => mask_ellipse(&mut cropped),
    }
    Ok(cropped)
}

fn mask_rounded(image: &mut RgbaImage, requested_radius: u32) {
    let radius = requested_radius.min(image.width().min(image.height()) / 2);
    if radius == 0 {
        return;
    }
    let r = f64::from(radius);
    // Only the four corner squares can change. Integer radii keep mirrored
    // corners disjoint, including odd-sized and very small images.
    for y in 0..radius {
        for x in 0..radius {
            let coverage = ellipse_coverage(x, y, r, r);
            if coverage == MASK_SAMPLES {
                continue;
            }
            let opposite_x = image.width() - 1 - x;
            let opposite_y = image.height() - 1 - y;
            multiply_alpha(image, x, y, coverage);
            multiply_alpha(image, opposite_x, y, coverage);
            multiply_alpha(image, x, opposite_y, coverage);
            multiply_alpha(image, opposite_x, opposite_y, coverage);
        }
    }
}

fn mask_ellipse(image: &mut RgbaImage) {
    let radius_x = f64::from(image.width()) / 2.0;
    let radius_y = f64::from(image.height()) / 2.0;
    let half_width = image.width() / 2 + image.width() % 2;
    for y in 0..image.height() {
        let far_y = (f64::from(y) - radius_y)
            .abs()
            .max((f64::from(y) + 1.0 - radius_y).abs())
            / radius_y;
        // The middle of each row is completely covered. Visit only pixels that
        // could have transparency, and mirror the left edge onto the right.
        let inner_extent = radius_x * (1.0 - far_y * far_y).max(0.0).sqrt();
        let edge_end = ((radius_x - inner_extent).ceil() as u32).min(half_width);
        for x in 0..edge_end {
            let coverage = ellipse_coverage(x, y, radius_x, radius_y);
            multiply_alpha(image, x, y, coverage);
            let opposite_x = image.width() - 1 - x;
            if opposite_x != x {
                multiply_alpha(image, opposite_x, y, coverage);
            }
        }
    }
}

const MASK_GRID: u32 = 8;
const MASK_SAMPLES: u32 = MASK_GRID * MASK_GRID;

/// Coverage of one pixel inside an ellipse centered at (radius_x, radius_y).
/// Most pixels are classified with bounds; only edge pixels need supersampling.
fn ellipse_coverage(x: u32, y: u32, radius_x: f64, radius_y: f64) -> u32 {
    let left = (f64::from(x) - radius_x) / radius_x;
    let right = (f64::from(x) + 1.0 - radius_x) / radius_x;
    let top = (f64::from(y) - radius_y) / radius_y;
    let bottom = (f64::from(y) + 1.0 - radius_y) / radius_y;
    let nearest = |a: f64, b: f64| {
        if a <= 0.0 && b >= 0.0 {
            0.0
        } else {
            a.abs().min(b.abs())
        }
    };
    let near_x = nearest(left, right);
    let near_y = nearest(top, bottom);
    if near_x * near_x + near_y * near_y >= 1.0 {
        return 0;
    }
    let far_x = left.abs().max(right.abs());
    let far_y = top.abs().max(bottom.abs());
    if far_x * far_x + far_y * far_y <= 1.0 {
        return MASK_SAMPLES;
    }

    let mut covered = 0;
    for sample_y in 0..MASK_GRID {
        let dy = top + (f64::from(sample_y) + 0.5) / f64::from(MASK_GRID) / radius_y;
        for sample_x in 0..MASK_GRID {
            let dx = left + (f64::from(sample_x) + 0.5) / f64::from(MASK_GRID) / radius_x;
            if dx * dx + dy * dy <= 1.0 {
                covered += 1;
            }
        }
    }
    covered
}

fn multiply_alpha(image: &mut RgbaImage, x: u32, y: u32, coverage: u32) {
    let pixel = image.get_pixel_mut(x, y);
    pixel[3] = ((u32::from(pixel[3]) * coverage + MASK_SAMPLES / 2) / MASK_SAMPLES) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn full_rect(width: u32, height: u32) -> PixelRect {
        PixelRect {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    #[test]
    fn reverse_drag_and_independent_pixel_scales() {
        let rect = selection_to_pixels([80.25, 40.5], [10.25, 5.25], [100.0, 50.0], [200, 150]);
        assert_eq!(
            rect,
            Some(PixelRect {
                x: 20,
                y: 15,
                width: 141,
                height: 107
            })
        );
    }

    #[test]
    fn clamp_drag_to_all_image_edges() {
        assert_eq!(
            selection_to_pixels([-100.0, 1000.0], [1000.0, -10.0], [100.0, 50.0], [250, 100]),
            Some(full_rect(250, 100)),
        );
        assert_eq!(
            selection_to_pixels([-10.0, 1.0], [-2.0, 10.0], [100.0; 2], [200; 2]),
            None
        );
    }

    #[test]
    fn viewport_local_origin_is_not_a_desktop_coordinate() {
        // A monitor placed left of the primary display still supplies a local
        // screenshot whose top-left pixel is (0, 0).
        assert_eq!(
            selection_to_pixels([0.0, 0.0], [20.0, 10.0], [1920.0, 1080.0], [3840, 2160]),
            Some(full_rect(40, 20)),
        );
    }

    #[test]
    fn fractional_click_empty_dimensions_and_non_finite_inputs_are_rejected() {
        assert_eq!(
            selection_to_pixels([1.2; 2], [1.2; 2], [100.0; 2], [200; 2]),
            None
        );
        assert_eq!(
            selection_to_pixels([1.2, 1.0], [1.2, 5.0], [100.0; 2], [200; 2]),
            None
        );
        for extent in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                selection_to_pixels([0.0; 2], [10.0; 2], [extent, 100.0], [200; 2]),
                None
            );
        }
        assert_eq!(
            selection_to_pixels([f32::NAN, 0.0], [10.0; 2], [100.0; 2], [200; 2]),
            None
        );
        assert_eq!(
            selection_to_pixels([0.0; 2], [f32::INFINITY, 10.0], [100.0; 2], [200; 2]),
            None
        );
        assert_eq!(
            selection_to_pixels([0.0; 2], [10.0; 2], [100.0; 2], [0, 200]),
            None
        );
    }

    #[test]
    fn rectangular_crop_preserves_source_pixels_and_alpha() {
        let image = RgbaImage::from_fn(4, 3, |x, y| Rgba([x as u8, y as u8, 99, 123]));
        let rect = PixelRect {
            x: 1,
            y: 1,
            width: 2,
            height: 2,
        };
        let cropped = crop_selection(&image, rect, CaptureShape::Rectangle, 12).unwrap();
        assert_eq!(cropped.dimensions(), (2, 2));
        assert_eq!(cropped.get_pixel(0, 0), image.get_pixel(1, 1));
        assert_eq!(cropped.get_pixel(1, 1), image.get_pixel(2, 2));
        assert_eq!(image.dimensions(), (4, 3));
    }

    #[test]
    fn masks_preserve_existing_alpha_and_have_antialiased_edges() {
        for shape in [CaptureShape::RoundedRectangle, CaptureShape::Ellipse] {
            let image = RgbaImage::from_pixel(32, 20, Rgba([20, 40, 60, 127]));
            let cropped = crop_selection(&image, full_rect(32, 20), shape, 8).unwrap();
            assert_eq!(cropped.get_pixel(0, 0)[3], 0);
            assert_eq!(cropped.get_pixel(16, 10)[3], 127);
            assert!(cropped.pixels().any(|pixel| pixel[3] > 0 && pixel[3] < 127));
            for (x, y, pixel) in cropped.enumerate_pixels() {
                assert_eq!(&pixel.0[..3], &[20, 40, 60]);
                assert!(pixel[3] <= 127);
                assert_eq!(pixel[3], cropped.get_pixel(31 - x, y)[3]);
                assert_eq!(pixel[3], cropped.get_pixel(x, 19 - y)[3]);
            }
            assert_eq!(image.get_pixel(0, 0)[3], 127);
        }
    }

    #[test]
    fn rounded_radius_is_bounded_and_zero_means_rectangle() {
        let image = RgbaImage::from_pixel(11, 9, Rgba([1, 2, 3, 255]));
        let oversized = crop_selection(
            &image,
            full_rect(11, 9),
            CaptureShape::RoundedRectangle,
            u32::MAX,
        )
        .unwrap();
        let bounded =
            crop_selection(&image, full_rect(11, 9), CaptureShape::RoundedRectangle, 4).unwrap();
        let zero =
            crop_selection(&image, full_rect(11, 9), CaptureShape::RoundedRectangle, 0).unwrap();
        assert_eq!(oversized, bounded);
        assert_eq!(zero, image);
    }

    #[test]
    fn odd_and_single_pixel_masks_do_not_apply_alpha_twice() {
        let image = RgbaImage::from_pixel(1, 1, Rgba([1, 2, 3, 128]));
        let ellipse = crop_selection(&image, full_rect(1, 1), CaptureShape::Ellipse, 10).unwrap();
        let coverage = ellipse_coverage(0, 0, 0.5, 0.5);
        assert_eq!(
            ellipse.get_pixel(0, 0)[3],
            ((128 * coverage + MASK_SAMPLES / 2) / MASK_SAMPLES) as u8
        );
        let rounded =
            crop_selection(&image, full_rect(1, 1), CaptureShape::RoundedRectangle, 10).unwrap();
        assert_eq!(rounded, image);
        let odd = RgbaImage::from_pixel(7, 9, Rgba([1, 2, 3, 128]));
        let cropped = crop_selection(&odd, full_rect(7, 9), CaptureShape::Ellipse, 0).unwrap();
        assert_eq!(cropped.get_pixel(3, 4)[3], 128);
    }

    #[test]
    fn invalid_bounds_and_integer_overflow_return_errors() {
        let image = RgbaImage::new(10, 10);
        for rect in [
            full_rect(0, 1),
            full_rect(1, 0),
            full_rect(11, 1),
            full_rect(1, 11),
            PixelRect {
                x: 10,
                y: 0,
                width: 1,
                height: 1,
            },
            PixelRect {
                x: 0,
                y: 10,
                width: 1,
                height: 1,
            },
            PixelRect {
                x: u32::MAX,
                y: 0,
                width: 1,
                height: 1,
            },
            PixelRect {
                x: 0,
                y: u32::MAX,
                width: 1,
                height: 1,
            },
        ] {
            assert!(crop_selection(&image, rect, CaptureShape::Ellipse, 4).is_err());
        }
    }
}
