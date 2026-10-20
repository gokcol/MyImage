//! Reversible image geometry. Annotation coordinates always belong to the original image.

use crate::document::{Point, validate_dimensions};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// An expanded viewport in original-image coordinates. The image itself is
/// unchanged; space outside its crop is transparent and can contain annotations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanvasRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cutout {
    /// 0 removes columns; 1 removes rows, in original-image coordinates.
    pub axis: u8,
    pub start: u32,
    pub end: u32,
}
fn is_false(value: &bool) -> bool {
    !*value
}

fn no_cutouts(cuts: &[Option<Cutout>; 16]) -> bool {
    cuts.iter().all(Option::is_none)
}

/// Crop original pixels, remove strips, rotate clockwise, mirror, then resize to the requested output size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageGeometry {
    #[serde(
        default,
        skip_serializing_if = "crate::image_effects::ImageEffects::is_default"
    )]
    pub effects: crate::image_effects::ImageEffects,
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_horizontal: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_vertical: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas: Option<CanvasRect>,
    /// Visible canvas window; content outside remains in the composition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<CanvasRect>,
    #[serde(deserialize_with = "required_option")]
    pub crop: Option<CropRect>,
    pub quarter_turns: u8,
    #[serde(deserialize_with = "required_option")]
    pub resize: Option<[u32; 2]>,
    #[serde(default, skip_serializing_if = "no_cutouts")]
    pub cutouts: [Option<Cutout>; 16],
}

// Explicit null is valid, but missing v2 geometry fields are not an identity edit.
fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

impl ImageGeometry {
    pub fn has_cutouts(self) -> bool {
        !no_cutouts(&self.cutouts)
    }
    pub fn canvas_rect(self, original: [u32; 2]) -> CanvasRect {
        self.canvas.unwrap_or_else(|| {
            let c = self.crop_rect(original);
            CanvasRect {
                x: c.x as i32,
                y: c.y as i32,
                width: c.width,
                height: c.height,
            }
        })
    }

    pub fn view_rect(self, original: [u32; 2]) -> CanvasRect {
        self.viewport.unwrap_or_else(|| self.canvas_rect(original))
    }

    pub fn kept_intervals(self, original: [u32; 2], axis: usize) -> Vec<(i32, i32)> {
        let c = self.view_rect(original);
        let (start, end) = if axis == 0 {
            (c.x, c.x + c.width as i32)
        } else {
            (c.y, c.y + c.height as i32)
        };
        let mut cuts: Vec<_> = self
            .cutouts
            .iter()
            .flatten()
            .filter(|c| c.axis as usize == axis)
            .map(|c| ((c.start as i32).max(start), (c.end as i32).min(end)))
            .filter(|(a, b)| b > a)
            .collect();
        cuts.sort_unstable();
        let mut result = Vec::new();
        let mut at = start;
        for (l, r) in cuts {
            if l > at {
                result.push((at, l));
            }
            at = at.max(r);
        }
        if at < end {
            result.push((at, end));
        }
        result
    }
    pub fn compact_size(self, original: [u32; 2]) -> [u32; 2] {
        if !self.has_cutouts() {
            let c = self.view_rect(original);
            return [c.width, c.height];
        }
        [0, 1].map(|axis| {
            self.kept_intervals(original, axis)
                .iter()
                .map(|(a, b)| (b - a) as u32)
                .sum()
        })
    }
    fn compact_axis(self, value: f32, original: [u32; 2], axis: usize) -> f32 {
        if !self.has_cutouts() {
            let c = self.view_rect(original);
            return value - if axis == 0 { c.x as f32 } else { c.y as f32 };
        }
        let intervals = self.kept_intervals(original, axis);
        let start = intervals.first().map_or(0, |v| v.0) as f32;
        if value < start {
            return value - start;
        }
        let mut out = 0.;
        for (a, b) in intervals {
            if value < a as f32 {
                return out;
            }
            if value <= b as f32 {
                return out + value - a as f32;
            }
            out += (b - a) as f32;
        }
        let c = self.view_rect(original);
        let end = if axis == 0 {
            c.x + c.width as i32
        } else {
            c.y + c.height as i32
        };
        out + (value - end as f32).max(0.)
    }
    fn expand_axis(self, value: f32, original: [u32; 2], axis: usize) -> f32 {
        if !self.has_cutouts() {
            let c = self.view_rect(original);
            return value + if axis == 0 { c.x as f32 } else { c.y as f32 };
        }
        let intervals = self.kept_intervals(original, axis);
        let mut left = value;
        for &(a, b) in &intervals {
            if left < (b - a) as f32 {
                return a as f32 + left;
            }
            left -= (b - a) as f32;
        }
        intervals.last().map_or(0., |v| v.1 as f32) + left
    }
    pub fn validate(self, original: [u32; 2]) -> Result<(), String> {
        validate_dimensions(original[0], original[1])?;
        self.effects.validate()?;
        if self.quarter_turns > 3 {
            return Err("The image rotation must be 0, 90, 180, or 270 degrees.".into());
        }
        let crop = self.crop_rect(original);
        if let Some(canvas) = self.canvas {
            validate_dimensions(canvas.width, canvas.height)?;
            let right = i64::from(canvas.x) + i64::from(canvas.width);
            let bottom = i64::from(canvas.y) + i64::from(canvas.height);
            if canvas.x.abs_diff(0) > crate::document::MAX_IMAGE_SIDE
                || canvas.y.abs_diff(0) > crate::document::MAX_IMAGE_SIDE
                || right > i64::from(crate::document::MAX_IMAGE_SIDE) * 2
                || bottom > i64::from(crate::document::MAX_IMAGE_SIDE) * 2
                || i64::from(canvas.x) > i64::from(crop.x)
                || i64::from(canvas.y) > i64::from(crop.y)
                || right < i64::from(crop.x) + i64::from(crop.width)
                || bottom < i64::from(crop.y) + i64::from(crop.height)
            {
                return Err("The expanded canvas must contain the image crop and stay within supported dimensions.".into());
            }
        }
        if let Some(view) = self.viewport {
            validate_dimensions(view.width, view.height)?;
            let bounds = self.canvas_rect(original);
            if self.canvas.is_none()
                || view.x < bounds.x
                || view.y < bounds.y
                || i64::from(view.x) + i64::from(view.width)
                    > i64::from(bounds.x) + i64::from(bounds.width)
                || i64::from(view.y) + i64::from(view.height)
                    > i64::from(bounds.y) + i64::from(bounds.height)
            {
                return Err("The visible canvas must stay inside the retained composition.".into());
            }
        }
        if crop.width == 0
            || crop.height == 0
            || crop
                .x
                .checked_add(crop.width)
                .is_none_or(|end| end > original[0])
            || crop
                .y
                .checked_add(crop.height)
                .is_none_or(|end| end > original[1])
        {
            return Err("The crop must contain pixels within the original image.".into());
        }
        for c in self.cutouts.iter().flatten() {
            if c.axis > 1 || c.start >= c.end || c.end > original[c.axis.min(1) as usize] {
                return Err("Invalid cut-out strip.".into());
            }
        }
        if self.compact_size(original).contains(&0) {
            return Err("A cut out must leave at least one row and column.".into());
        }
        if let Some([width, height]) = self.resize {
            validate_dimensions(width, height)?;
        }
        Ok(())
    }

    pub fn crop_rect(self, original: [u32; 2]) -> CropRect {
        self.crop.unwrap_or(CropRect {
            x: 0,
            y: 0,
            width: original[0],
            height: original[1],
        })
    }

    pub fn rotated_size(self, original: [u32; 2]) -> [u32; 2] {
        let [w, h] = self.compact_size(original);
        if self.quarter_turns.is_multiple_of(2) {
            [w, h]
        } else {
            [h, w]
        }
    }

    pub fn output_size(self, original: [u32; 2]) -> [u32; 2] {
        self.resize.unwrap_or_else(|| self.rotated_size(original))
    }

    /// Texture and canvas bounds are already in effect presentation space.
    pub fn unwarped(mut self) -> Self {
        self.effects.styles.perspective = [0; 2];
        self
    }

    fn project_point(self, point: Point, inverse: bool) -> Point {
        let Some(f) = self.effects.frame else {
            return point;
        };
        self.effects.styles.project(
            point,
            eframe::egui::Rect::from_min_size(
                eframe::egui::pos2(f.x as f32, f.y as f32),
                eframe::egui::vec2(f.width as f32, f.height as f32),
            ),
            inverse,
        )
    }

    /// Map pixel-edge coordinates, including coordinates outside the current crop.
    pub fn map_point(self, point: Point, original: [u32; 2]) -> Point {
        let point = self.project_point(point, false);
        let [width, height] = self.compact_size(original);
        let (x, y) = (
            self.compact_axis(point.x, original, 0),
            self.compact_axis(point.y, original, 1),
        );
        let (x, y) = match self.quarter_turns {
            1 => (height as f32 - y, x),
            2 => (width as f32 - x, height as f32 - y),
            3 => (y, width as f32 - x),
            _ => (x, y),
        };
        let rotated = self.rotated_size(original);
        let output = self.output_size(original);
        let x = if self.flip_horizontal {
            rotated[0] as f32 - x
        } else {
            x
        };
        let y = if self.flip_vertical {
            rotated[1] as f32 - y
        } else {
            y
        };
        Point {
            x: x * output[0] as f32 / rotated[0] as f32,
            y: y * output[1] as f32 / rotated[1] as f32,
        }
    }

    pub fn inverse_point(self, point: Point, original: [u32; 2]) -> Point {
        let [width, height] = self.compact_size(original);
        let rotated = self.rotated_size(original);
        let output = self.output_size(original);
        let (x, y) = (
            point.x * rotated[0] as f32 / output[0] as f32,
            point.y * rotated[1] as f32 / output[1] as f32,
        );
        let x = if self.flip_horizontal {
            rotated[0] as f32 - x
        } else {
            x
        };
        let y = if self.flip_vertical {
            rotated[1] as f32 - y
        } else {
            y
        };
        let (x, y) = match self.quarter_turns {
            1 => (y, height as f32 - x),
            2 => (width as f32 - x, height as f32 - y),
            3 => (width as f32 - y, x),
            _ => (x, y),
        };
        self.project_point(
            Point {
                x: self.expand_axis(x, original, 0),
                y: self.expand_axis(y, original, 1),
            },
            true,
        )
    }

    /// Produce presentation pixels without changing the original image or annotations.
    pub fn apply(self, original: &RgbaImage) -> Result<RgbaImage, String> {
        let dimensions = [original.width(), original.height()];
        self.validate(dimensions)?;
        if self.canvas.is_some() {
            let (pixels, geometry) = crate::canvas::prepare_image(original, self)?;
            return geometry.apply(&pixels);
        }
        let xs: Vec<i32> = self
            .kept_intervals(dimensions, 0)
            .into_iter()
            .flat_map(|(a, b)| a..b)
            .collect();
        let ys: Vec<i32> = self
            .kept_intervals(dimensions, 1)
            .into_iter()
            .flat_map(|(a, b)| a..b)
            .collect();
        let [w, h] = self.rotated_size(dimensions);
        let rotated = RgbaImage::from_fn(w, h, |x, y| {
            let x = if self.flip_horizontal { w - 1 - x } else { x };
            let y = if self.flip_vertical { h - 1 - y } else { y };
            let (x, y) = match self.quarter_turns {
                1 => (y, ys.len() as u32 - 1 - x),
                2 => (xs.len() as u32 - 1 - x, ys.len() as u32 - 1 - y),
                3 => (xs.len() as u32 - 1 - y, x),
                _ => (x, y),
            };
            *original.get_pixel(xs[x as usize] as u32, ys[y as usize] as u32)
        });
        let [width, height] = self.output_size(dimensions);
        if rotated.dimensions() == (width, height) {
            return Ok(rotated);
        }
        Ok(resize_premultiplied(&rotated, width, height))
    }
}

/// Triangle filtering with a wider footprint when downsampling. Keep only a few
/// floating-point scanlines, avoiding full-image RGBA32F intermediate buffers.
fn resize_premultiplied(source: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    // Filter the shrinking axis first when aspect ratio changes substantially.
    // A virtual transpose avoids both an extra image buffer and a huge expanded
    // intermediate workload (for example a tall, thin image resized very wide).
    let transpose = u64::from(width) * u64::from(source.height())
        > u64::from(height) * u64::from(source.width());
    let (input_width, input_height, output_width, output_height) = if transpose {
        (source.height(), source.width(), height, width)
    } else {
        (source.width(), source.height(), width, height)
    };
    let horizontal = filter_taps(input_width, output_width);
    let vertical = filter_taps(input_height, output_height);
    let mut output = RgbaImage::new(width, height);
    let mut rows: VecDeque<(u32, Vec<[f32; 4]>)> = VecDeque::new();
    let mut accumulated = vec![[0.0_f32; 4]; output_width as usize];
    for (y, taps) in vertical.iter().enumerate() {
        accumulated.fill([0.0; 4]);
        for &(source_y, y_weight) in taps {
            let row_index = match rows.iter().position(|(cached_y, _)| *cached_y == source_y) {
                Some(index) => index,
                None => {
                    if rows.len() == 4 {
                        rows.pop_front();
                    }
                    let row = horizontal
                        .iter()
                        .map(|x_taps| {
                            let mut pixel = [0.0_f32; 4];
                            for &(source_x, x_weight) in x_taps {
                                let rgba = if transpose {
                                    source.get_pixel(source_y, source_x).0
                                } else {
                                    source.get_pixel(source_x, source_y).0
                                };
                                let alpha = rgba[3] as f32 / 255.0;
                                for channel in 0..3 {
                                    pixel[channel] += rgba[channel] as f32 * alpha * x_weight;
                                }
                                pixel[3] += rgba[3] as f32 * x_weight;
                            }
                            pixel
                        })
                        .collect();
                    rows.push_back((source_y, row));
                    rows.len() - 1
                }
            };
            for (accumulated, filtered) in accumulated.iter_mut().zip(&rows[row_index].1) {
                for channel in 0..4 {
                    accumulated[channel] += filtered[channel] * y_weight;
                }
            }
        }
        for (x, pixel) in accumulated.iter().enumerate() {
            let mut rgba = [0_u8; 4];
            rgba[3] = pixel[3].round().clamp(0.0, 255.0) as u8;
            if rgba[3] != 0 {
                for channel in 0..3 {
                    rgba[channel] = (pixel[channel] * 255.0 / pixel[3])
                        .round()
                        .clamp(0.0, 255.0) as u8;
                }
            }
            if transpose {
                output.put_pixel(y as u32, x as u32, Rgba(rgba));
            } else {
                output.put_pixel(x as u32, y as u32, Rgba(rgba));
            }
        }
    }
    output
}

fn filter_taps(input: u32, output: u32) -> Vec<Vec<(u32, f32)>> {
    let scale = input as f32 / output as f32;
    let support = scale.max(1.0);
    (0..output)
        .map(|index| {
            let center = (index as f32 + 0.5) * scale;
            let start = ((center - support).floor() as i64).max(0) as u32;
            let end = ((center + support).ceil() as u32).min(input);
            let mut taps: Vec<_> = (start..end)
                .filter_map(|sample| {
                    let weight = (1.0 - ((sample as f32 + 0.5 - center) / support).abs()).max(0.0);
                    (weight > 0.0).then_some((sample, weight))
                })
                .collect();
            let weight_sum: f32 = taps.iter().map(|(_, weight)| weight).sum();
            for (_, weight) in &mut taps {
                *weight /= weight_sum;
            }
            taps
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarter_turns_map_crop_corners_and_round_trip_outside_points() {
        let expected = [
            [(0.0, 0.0), (80.0, 60.0)],
            [(60.0, 0.0), (0.0, 80.0)],
            [(80.0, 60.0), (0.0, 0.0)],
            [(0.0, 80.0), (60.0, 0.0)],
        ];
        for quarter_turns in 0..4 {
            let mut geometry = ImageGeometry {
                effects: Default::default(),
                viewport: None,
                flip_horizontal: false,
                flip_vertical: false,
                canvas: None,
                cutouts: [None; 16],
                crop: Some(CropRect {
                    x: 10,
                    y: 20,
                    width: 80,
                    height: 60,
                }),
                quarter_turns,
                resize: None,
            };
            for (point, (x, y)) in [Point { x: 10.0, y: 20.0 }, Point { x: 90.0, y: 80.0 }]
                .into_iter()
                .zip(expected[quarter_turns as usize])
            {
                assert_eq!(geometry.map_point(point, [100, 100]), Point { x, y });
            }
            geometry.resize = Some([137, 91]);
            for point in [
                Point { x: 0.0, y: 0.0 },
                Point { x: 47.25, y: 32.75 },
                Point { x: 100.0, y: 100.0 },
            ] {
                let inverse =
                    geometry.inverse_point(geometry.map_point(point, [100, 100]), [100, 100]);
                assert!((inverse.x - point.x).abs() < 0.0001);
                assert!((inverse.y - point.y).abs() < 0.0001);
            }
        }
    }

    #[test]
    fn exact_crop_and_rotation_keep_pixel_values_and_source_unchanged() {
        let original = RgbaImage::from_fn(4, 3, |x, y| {
            Rgba([(y * 4 + x) as u8, 13, 210, (x * 50) as u8])
        });
        let before = original.clone();
        let mut geometry = ImageGeometry {
            flip_horizontal: false,
            flip_vertical: false,
            canvas: None,
            crop: Some(CropRect {
                x: 1,
                y: 1,
                width: 3,
                height: 2,
            }),
            ..Default::default()
        };
        let expected = [
            vec![5, 6, 7, 9, 10, 11],
            vec![9, 5, 10, 6, 11, 7],
            vec![11, 10, 9, 7, 6, 5],
            vec![7, 11, 6, 10, 5, 9],
        ];
        for quarter_turns in 0..4 {
            geometry.quarter_turns = quarter_turns;
            let output = geometry.apply(&original).unwrap();
            assert_eq!(
                output.pixels().map(|pixel| pixel[0]).collect::<Vec<_>>(),
                expected[quarter_turns as usize]
            );
            assert_eq!(
                output.dimensions(),
                if quarter_turns.is_multiple_of(2) {
                    (3, 2)
                } else {
                    (2, 3)
                }
            );
        }
        assert_eq!(original, before);
        assert_eq!(ImageGeometry::default().apply(&original).unwrap(), original);
    }

    #[test]
    fn resize_does_not_bleed_hidden_rgb_into_transparent_edges() {
        let original = RgbaImage::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 255, 0, 0])
            }
        });
        let geometry = ImageGeometry {
            flip_horizontal: false,
            flip_vertical: false,
            canvas: None,
            resize: Some([1, 1]),
            ..Default::default()
        };
        assert_eq!(
            geometry.apply(&original).unwrap().get_pixel(0, 0).0,
            [255, 0, 0, 128]
        );
        let geometry = ImageGeometry {
            flip_horizontal: false,
            flip_vertical: false,
            canvas: None,
            resize: Some([5, 3]),
            ..Default::default()
        };
        for pixel in geometry
            .apply(&original)
            .unwrap()
            .pixels()
            .filter(|pixel| pixel[3] != 0)
        {
            assert_eq!(&pixel.0[..3], &[255, 0, 0]);
        }
        let portrait = image::imageops::rotate90(&original);
        let geometry = ImageGeometry {
            flip_horizontal: false,
            flip_vertical: false,
            canvas: None,
            resize: Some([17, 1]),
            ..Default::default()
        };
        let resized = geometry.apply(&portrait).unwrap();
        assert_eq!(resized.dimensions(), (17, 1));
        assert!(resized.pixels().all(|pixel| pixel.0 == [255, 0, 0, 128]));
    }

    #[test]
    fn rejects_empty_outside_overflow_and_excessive_geometry_before_allocation() {
        for crop in [
            CropRect {
                x: 0,
                y: 0,
                width: 0,
                height: 1,
            },
            CropRect {
                x: 90,
                y: 0,
                width: 11,
                height: 1,
            },
            CropRect {
                x: u32::MAX,
                y: 0,
                width: 2,
                height: 1,
            },
        ] {
            assert!(
                ImageGeometry {
                    flip_horizontal: false,
                    flip_vertical: false,
                    canvas: None,
                    crop: Some(crop),
                    ..Default::default()
                }
                .validate([100, 100])
                .is_err()
            );
        }
        for resize in [[0, 1], [1, 32_769], [20_000, 20_000]] {
            assert!(
                ImageGeometry {
                    flip_horizontal: false,
                    flip_vertical: false,
                    canvas: None,
                    resize: Some(resize),
                    ..Default::default()
                }
                .validate([100, 100])
                .is_err()
            );
        }
        assert!(
            ImageGeometry {
                flip_horizontal: false,
                flip_vertical: false,
                canvas: None,
                quarter_turns: 4,
                ..Default::default()
            }
            .validate([100, 100])
            .is_err()
        );
        assert!(ImageGeometry::default().validate([0, 10]).is_err());
    }
}
