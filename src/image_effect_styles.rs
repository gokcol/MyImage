//! Source-space color, reflection and projective presentation effects.
use crate::document::Point;
use eframe::egui::{Pos2, Rect};
use image::{Pixel, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorFilter {
    #[default]
    None,
    Sepia,
    Warm,
    Cool,
    Invert,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EffectStyles {
    pub fade_depth: u16,
    pub fade_sides: [bool; 4],
    pub filter: ColorFilter,
    pub brightness: i16,
    pub contrast: i16,
    pub saturation: i16,
    pub perspective: [i16; 2],
    pub reflection_height: u8,
    pub reflection_opacity: u8,
    pub reflection_gap: u16,
}

impl EffectStyles {
    pub fn active(self) -> bool {
        self.fade_depth > 0
            || self.filter != ColorFilter::None
            || self.brightness != 0
            || self.contrast != 0
            || self.saturation != 0
            || self.perspective != [0; 2]
            || self.reflection_height > 0
    }

    pub fn validate(self) -> Result<(), String> {
        if self.fade_depth > 512
            || self.brightness.abs_diff(0) > 100
            || self.contrast.abs_diff(0) > 100
            || self.saturation.abs_diff(0) > 100
            || self.perspective.iter().any(|v| v.abs_diff(0) > 30)
            || self.reflection_height > 100
            || self.reflection_gap > 128
        {
            return Err("Effect style exceeds supported limits.".into());
        }
        Ok(())
    }

    pub fn fade(self, pixel: &mut Rgba<u8>, p: Pos2, frame: Rect) {
        if self.fade_depth == 0 {
            return;
        }
        let mut amount: f32 = 1.;
        for (enabled, distance) in self.fade_sides.into_iter().zip([
            p.y - frame.top(),
            frame.right() - p.x,
            frame.bottom() - p.y,
            p.x - frame.left(),
        ]) {
            if enabled {
                amount = amount.min((distance / self.fade_depth as f32).clamp(0., 1.));
            }
        }
        let amount = amount * amount * (3. - 2. * amount);
        pixel[3] = (pixel[3] as f32 * amount).round() as u8;
    }

    pub fn color(self, p: &mut Rgba<u8>) {
        if self.filter == ColorFilter::None
            && self.brightness == 0
            && self.contrast == 0
            && self.saturation == 0
        {
            return;
        }
        let [r, g, b, _] = p.0.map(f32::from);
        let mut rgb = match self.filter {
            ColorFilter::None => [r, g, b],
            ColorFilter::Sepia => [
                r * 0.393 + g * 0.769 + b * 0.189,
                r * 0.349 + g * 0.686 + b * 0.168,
                r * 0.272 + g * 0.534 + b * 0.131,
            ],
            ColorFilter::Warm => [r * 1.08 + 8., g * 1.02, b * 0.88],
            ColorFilter::Cool => [r * 0.88, g * 1.02, b * 1.08 + 8.],
            ColorFilter::Invert => [255. - r, 255. - g, 255. - b],
        };
        let gray = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
        let saturation = 1. + self.saturation as f32 / 100.;
        let contrast = if self.contrast >= 0 {
            1. + self.contrast as f32 / 50.
        } else {
            1. + self.contrast as f32 / 100.
        };
        for (channel, value) in rgb.iter_mut().enumerate() {
            *value = gray + (*value - gray) * saturation;
            *value = (*value - 127.5) * contrast + 127.5 + self.brightness as f32 * 2.55;
            p[channel] = value.round().clamp(0., 255.) as u8;
        }
    }

    /// Forward/inverse homography, also used for annotation hit testing and handles.
    pub fn project(self, point: Point, frame: Rect, inverse: bool) -> Point {
        if self.perspective == [0; 2] {
            return point;
        }
        let center = frame.center();
        let half = frame.size() * 0.5;
        let x = (point.x - center.x) / half.x;
        let y = (point.y - center.y) / half.y;
        let h = self.perspective[0] as f32 / 100.;
        let v = self.perspective[1] as f32 / 100.;
        let fit = 1. - h.abs() - v.abs();
        let factor = if inverse {
            1. / (fit - h * x - v * y).max(0.01)
        } else {
            fit / (1. + h * x + v * y).max(0.01)
        };
        Point {
            x: center.x + x * factor * half.x,
            y: center.y + y * factor * half.y,
        }
    }

    pub fn compose(self, pixels: &mut RgbaImage, frame: Rect, origin: [f32; 2], scale: [f32; 2]) {
        if self.reflection_height > 0 && self.reflection_opacity > 0 {
            let source = pixels.clone();
            let height = frame.height() * self.reflection_height as f32 / 100.;
            let top = frame.bottom() + self.reflection_gap as f32;
            for (x, y, pixel) in pixels.enumerate_pixels_mut() {
                let p = Pos2::new(
                    (x as f32 + 0.5) / scale[0] + origin[0],
                    (y as f32 + 0.5) / scale[1] + origin[1],
                );
                let down = p.y - top;
                if down >= 0. && down < height && p.x >= frame.left() && p.x < frame.right() {
                    let sy = (frame.bottom() - down - origin[1]) * scale[1] - 0.5;
                    let mut reflection = sample(&source, x as f32, sy);
                    reflection[3] = (reflection[3] as f32 * self.reflection_opacity as f32 / 255.
                        * (1. - down / height).powi(2))
                    .round() as u8;
                    reflection.blend(pixel);
                    *pixel = reflection;
                }
            }
        }
        if self.perspective != [0; 2] {
            let source = pixels.clone();
            for (x, y, pixel) in pixels.enumerate_pixels_mut() {
                let p = Point {
                    x: (x as f32 + 0.5) / scale[0] + origin[0],
                    y: (y as f32 + 0.5) / scale[1] + origin[1],
                };
                let p = self.project(p, frame, true);
                *pixel = sample(
                    &source,
                    (p.x - origin[0]) * scale[0] - 0.5,
                    (p.y - origin[1]) * scale[1] - 0.5,
                );
            }
        }
    }
}

// Interpolate premultiplied channels to avoid dark fringes on transparent edges.
fn sample(source: &RgbaImage, x: f32, y: f32) -> Rgba<u8> {
    if !x.is_finite()
        || !y.is_finite()
        || x < -1.
        || y < -1.
        || x >= source.width() as f32
        || y >= source.height() as f32
    {
        return Rgba([0; 4]);
    }
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let mut sum = [0.; 4];
    for (dx, dy, weight) in [
        (0, 0, (1. - fx) * (1. - fy)),
        (1, 0, fx * (1. - fy)),
        (0, 1, (1. - fx) * fy),
        (1, 1, fx * fy),
    ] {
        let (sx, sy) = (ix + dx, iy + dy);
        if sx < 0 || sy < 0 || sx >= source.width() as i32 || sy >= source.height() as i32 {
            continue;
        }
        let p = source.get_pixel(sx as u32, sy as u32);
        let alpha = p[3] as f32 * weight;
        sum[3] += alpha;
        for c in 0..3 {
            sum[c] += p[c] as f32 * alpha;
        }
    }
    if sum[3] < 0.5 {
        return Rgba([0; 4]);
    }
    Rgba([
        (sum[0] / sum[3]).round() as u8,
        (sum[1] / sum[3]).round() as u8,
        (sum[2] / sum[3]).round() as u8,
        sum[3].round() as u8,
    ])
}
