//! Pixel-accurate capture loupe, independent of the background preview's GPU size.
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

const ZOOM: f32 = 4.;
const APPROXIMATE_SIDE: f32 = 112.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Sample {
    pixel: [u32; 2],
    size: [usize; 2],
}

impl Sample {
    fn at(pointer: Pos2, bounds: Rect, image_size: [u32; 2]) -> Option<Self> {
        if !bounds.is_finite()
            || !bounds.is_positive()
            || !pointer.is_finite()
            || !bounds.contains(pointer)
            || image_size.contains(&0)
        {
            return None;
        }
        let ratios = [
            image_size[0] as f32 / bounds.width(),
            image_size[1] as f32 / bounds.height(),
        ];
        let point = pointer - bounds.min;
        Some(Self {
            pixel: [
                (point.x * ratios[0])
                    .floor()
                    .min((image_size[0] - 1) as f32) as u32,
                (point.y * ratios[1])
                    .floor()
                    .min((image_size[1] - 1) as f32) as u32,
            ],
            // An odd sample has one unambiguous center pixel. The display size
            // below derives from this count, preserving exactly 4x at any DPI.
            size: ratios
                .map(|r| ((APPROXIMATE_SIDE * r / ZOOM).round().clamp(1., 1023.) as usize) | 1),
        })
    }

    fn pixels(self, image: &image::RgbaImage) -> egui::ColorImage {
        let mut tile = egui::ColorImage::filled(self.size, Color32::TRANSPARENT);
        let left = i64::from(self.pixel[0]) - self.size[0] as i64 / 2;
        let top = i64::from(self.pixel[1]) - self.size[1] as i64 / 2;
        for y in 0..self.size[1] {
            for x in 0..self.size[0] {
                let source_x = left + x as i64;
                let source_y = top + y as i64;
                if source_x >= 0
                    && source_y >= 0
                    && source_x < i64::from(image.width())
                    && source_y < i64::from(image.height())
                {
                    let [r, g, b, a] = image.get_pixel(source_x as u32, source_y as u32).0;
                    tile.pixels[y * self.size[0] + x] = Color32::from_rgba_unmultiplied(r, g, b, a);
                }
            }
        }
        tile
    }

    fn display_size(self, bounds: Rect, image_size: [u32; 2]) -> Vec2 {
        Vec2::new(
            self.size[0] as f32 * bounds.width() / image_size[0] as f32 * ZOOM,
            self.size[1] as f32 * bounds.height() / image_size[1] as f32 * ZOOM,
        )
    }
}

#[derive(Default)]
pub struct CaptureMagnifier {
    sample: Option<Sample>,
    texture: Option<egui::TextureHandle>,
}

impl CaptureMagnifier {
    pub fn paint(
        &mut self,
        painter: &egui::Painter,
        pointer: Pos2,
        bounds: Rect,
        controls: Rect,
        image: &image::RgbaImage,
    ) {
        let image_size = [image.width(), image.height()];
        let Some(sample) = Sample::at(pointer, bounds, image_size) else {
            return;
        };
        if self.sample != Some(sample) {
            let pixels = sample.pixels(image);
            if let Some(texture) = &mut self.texture {
                texture.set(pixels, egui::TextureOptions::NEAREST);
            } else {
                self.texture = Some(painter.ctx().load_texture(
                    "capture-magnifier",
                    pixels,
                    egui::TextureOptions::NEAREST,
                ));
            }
            self.sample = Some(sample);
        }
        let Some(texture) = &self.texture else {
            return;
        };
        let display_size = sample.display_size(bounds, image_size);
        let card_size = display_size + Vec2::new(8., 29.);
        let available = Rect::from_min_max(
            bounds.min + Vec2::splat(8.),
            Pos2::new(
                bounds.right() - 8.,
                controls.top().min(bounds.bottom()) - 8.,
            ),
        );
        let card = place_card(pointer, card_size, available);
        let lens = Rect::from_min_size(card.min + Vec2::splat(4.), display_size);
        painter.rect_filled(card, 7, crate::appearance::palette(painter.ctx()).navy);
        painter.image(
            texture.id(),
            lens,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
            Color32::WHITE,
        );
        let center = lens.center();
        // Leave the center pixel visible, with four short contrast marks around it.
        for direction in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
            let segment = [center + direction * 4., center + direction * 12.];
            painter.line_segment(segment, Stroke::new(3., Color32::from_black_alpha(190)));
            painter.line_segment(segment, Stroke::new(1., Color32::WHITE));
        }
        painter.text(
            Pos2::new(card.center().x, lens.bottom() + 12.),
            egui::Align2::CENTER_CENTER,
            format!("4×  ·  {}, {}", sample.pixel[0], sample.pixel[1]),
            egui::FontId::proportional(10.),
            Color32::WHITE,
        );
    }
}

fn place_card(pointer: Pos2, size: Vec2, available: Rect) -> Rect {
    let mut top_left = pointer + Vec2::splat(24.);
    if top_left.x + size.x > available.right() {
        top_left.x = pointer.x - size.x - 24.;
    }
    if top_left.y + size.y > available.bottom() {
        top_left.y = pointer.y - size.y - 24.;
    }
    top_left.x = top_left.x.clamp(
        available.left(),
        (available.right() - size.x).max(available.left()),
    );
    top_left.y = top_left.y.clamp(
        available.top(),
        (available.bottom() - size.y).max(available.top()),
    );
    Rect::from_min_size(top_left, size)
}

/// A fixed 13×13 pixel grid makes the exact source pixel visible even at fit-to-window zoom.
pub fn paint_color(
    painter: &egui::Painter,
    pointer: Pos2,
    bounds: Rect,
    viewport: Rect,
    image: &image::RgbaImage,
) {
    let Some(sample) = Sample::at(pointer, bounds, [image.width(), image.height()]) else {
        return;
    };
    let card = place_card(pointer, Vec2::new(120., 145.), viewport.shrink(6.));
    let lens = Rect::from_min_size(card.min + Vec2::new(8., 7.), Vec2::splat(104.));
    painter.rect_filled(card, 7., crate::appearance::palette(painter.ctx()).navy);
    for y in 0..13 {
        for x in 0..13 {
            let cell = Rect::from_min_size(
                lens.min + Vec2::new(x as f32 * 8., y as f32 * 8.),
                Vec2::splat(8.),
            );
            painter.rect_filled(
                cell,
                0.,
                if (x + y) % 2 == 0 {
                    Color32::WHITE
                } else {
                    Color32::from_gray(205)
                },
            );
            let sx = i64::from(sample.pixel[0]) + x - 6;
            let sy = i64::from(sample.pixel[1]) + y - 6;
            if sx >= 0
                && sy >= 0
                && let Some(p) = image.get_pixel_checked(sx as u32, sy as u32)
            {
                painter.rect_filled(
                    cell,
                    0.,
                    Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]),
                );
            }
        }
    }
    let center = Rect::from_center_size(lens.center(), Vec2::splat(8.));
    painter.rect_stroke(
        center.expand(1.),
        0.,
        Stroke::new(3., Color32::BLACK),
        egui::StrokeKind::Outside,
    );
    painter.rect_stroke(
        center.expand(1.),
        0.,
        Stroke::new(1., Color32::WHITE),
        egui::StrokeKind::Outside,
    );
    let [r, g, b, _] = image.get_pixel(sample.pixel[0], sample.pixel[1]).0;
    painter.text(
        Pos2::new(card.center().x, lens.bottom() + 10.),
        egui::Align2::CENTER_CENTER,
        format!("#{r:02X}{g:02X}{b:02X}"),
        egui::FontId::monospace(11.),
        Color32::WHITE,
    );
    painter.text(
        Pos2::new(card.center().x, lens.bottom() + 24.),
        egui::Align2::CENTER_CENTER,
        format!("{}, {}", sample.pixel[0], sample.pixel[1]),
        egui::FontId::proportional(10.),
        Color32::WHITE,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_pixel_matches_capture_coordinates_at_retina_and_shifted_origins() {
        let image = image::RgbaImage::from_fn(400, 200, |x, y| {
            image::Rgba([(x % 256) as u8, y as u8, (x / 256) as u8, 255])
        });
        let bounds = Rect::from_min_size(Pos2::new(-100., 70.), Vec2::new(200., 100.));
        let sample = Sample::at(Pos2::new(55.7, 94.2), bounds, [400, 200]).unwrap();
        assert_eq!(sample.pixel, [311, 48]);
        let tile = sample.pixels(&image);
        let center = (sample.size[1] / 2) * sample.size[0] + sample.size[0] / 2;
        assert_eq!(tile.pixels[center], Color32::from_rgb(55, 48, 1));
    }

    #[test]
    fn zoom_is_four_times_the_displayed_image_at_multiple_scales() {
        for scale in [1., 1.25, 1.5, 2., 3.] {
            let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(1600., 900.) / scale);
            let sample = Sample::at(bounds.center(), bounds, [1600, 900]).unwrap();
            let lens = sample.display_size(bounds, [1600, 900]);
            assert!((lens.x / sample.size[0] as f32 * scale - 4.).abs() < 0.001);
            assert!((lens.y / sample.size[1] as f32 * scale - 4.).abs() < 0.001);
        }
    }

    #[test]
    fn screen_edges_are_padded_without_shifting_the_center_or_smearing_pixels() {
        let image = image::RgbaImage::from_pixel(100, 100, image::Rgba([32, 64, 128, 255]));
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.));
        for point in [bounds.min, bounds.max] {
            let sample = Sample::at(point, bounds, [100, 100]).unwrap();
            let tile = sample.pixels(&image);
            assert_eq!(
                sample.pixel,
                if point == bounds.min {
                    [0, 0]
                } else {
                    [99, 99]
                }
            );
            assert_eq!(
                tile.pixels[tile.pixels.len() / 2],
                Color32::from_rgb(32, 64, 128)
            );
            assert!(tile.pixels.contains(&Color32::TRANSPARENT));
        }
    }

    #[test]
    fn loupe_flips_away_from_edges_and_stays_above_controls() {
        let available = Rect::from_min_max(Pos2::new(8., 8.), Pos2::new(992., 600.));
        for pointer in [
            Pos2::new(20., 20.),
            Pos2::new(980., 580.),
            Pos2::new(20., 650.),
        ] {
            let card = place_card(pointer, Vec2::new(120., 144.), available);
            assert!(available.contains_rect(card));
            assert!(!card.contains(pointer));
        }
    }
}
