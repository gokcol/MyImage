//! Reversible styling of the composed image. The original pixels and layer stack are untouched.
use crate::{document::Document, geometry::CanvasRect};
use eframe::egui::{self, Rect};
use image::{Pixel, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

#[path = "image_effect_styles.rs"]
mod styles;
pub use styles::{ColorFilter, EffectStyles};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeStyle {
    #[default]
    None,
    Torn,
    Wave,
    Sawtooth,
    Rounded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Edges {
    pub style: EdgeStyle,
    pub depth: u16,
    /// Top, right, bottom, left.
    pub sides: [bool; 4],
}
impl Default for Edges {
    fn default() -> Self {
        Self {
            style: EdgeStyle::None,
            depth: 16,
            sides: [true; 4],
        }
    }
}
impl Edges {
    pub fn active(self) -> bool {
        self.style != EdgeStyle::None && self.depth > 0 && self.sides.iter().any(|s| *s)
    }

    // Signed distance in source pixels. Positive values lie inside the styled frame.
    // Profiles are deterministic so thumbnails, dragging previews and exports agree.
    pub fn distance(self, p: egui::Pos2, frame: Rect) -> f32 {
        let x = p.x - frame.left();
        let y = p.y - frame.top();
        let w = frame.width();
        let h = frame.height();
        let plain = x.min(w - x).min(y).min(h - y);
        if !self.active() {
            return plain;
        }
        let depth = (self.depth as f32).min(w.min(h) * 0.45);
        if self.style == EdgeStyle::Rounded {
            for (corner, enabled) in [
                (egui::pos2(depth, depth), self.sides[0] && self.sides[3]),
                (egui::pos2(w - depth, depth), self.sides[0] && self.sides[1]),
                (
                    egui::pos2(w - depth, h - depth),
                    self.sides[2] && self.sides[1],
                ),
                (egui::pos2(depth, h - depth), self.sides[2] && self.sides[3]),
            ] {
                let point = egui::pos2(x, y);
                let outside_x = if corner.x < w * 0.5 {
                    x < corner.x
                } else {
                    x > corner.x
                };
                let outside_y = if corner.y < h * 0.5 {
                    y < corner.y
                } else {
                    y > corner.y
                };
                if enabled && outside_x && outside_y {
                    return depth - point.distance(corner);
                }
            }
            return plain;
        }
        let profile = |position: f32, side: usize| {
            if !self.sides[side] {
                return 0.;
            }
            let phase = position / (depth * 2.).max(8.) + side as f32 * 0.37;
            let fraction = phase.rem_euclid(1.);
            let amount = match self.style {
                EdgeStyle::Wave => 0.5 + 0.5 * (phase * std::f32::consts::TAU).sin(),
                EdgeStyle::Sawtooth => 1. - (fraction * 2. - 1.).abs(),
                EdgeStyle::Torn => {
                    // Irregular, continuous paper edge with fine and coarse teeth.
                    let triangle = |t: f32| 1. - (t.rem_euclid(1.) * 2. - 1.).abs();
                    0.15 + 0.5 * triangle(phase * 1.73)
                        + 0.23 * triangle(phase * 4.19 + 0.31)
                        + 0.12 * triangle(phase * 9.71)
                }
                _ => 0.,
            };
            depth * amount
        };
        (y - profile(x, 0))
            .min(w - x - profile(y, 1))
            .min(h - y - profile(x, 2))
            .min(x - profile(y, 3))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageEffects {
    #[serde(default)]
    pub styles: EffectStyles,
    #[serde(default)]
    pub edges: Edges,
    pub grayscale: bool,
    pub border_width: u16,
    pub border_color: [u8; 4],
    pub shadow_blur: u16,
    pub shadow_opacity: u8,
    pub shadow_offset: i16,
    /// The image frame before styling adds transparent space around it.
    pub frame: Option<CanvasRect>,
}
impl Default for ImageEffects {
    fn default() -> Self {
        Self {
            edges: Edges::default(),
            styles: EffectStyles::default(),
            grayscale: false,
            border_width: 0,
            border_color: [38, 35, 32, 255],
            shadow_blur: 12,
            shadow_opacity: 0,
            shadow_offset: 8,
            frame: None,
        }
    }
}
impl ImageEffects {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn active(self) -> bool {
        self.grayscale
            || self.border_width > 0
            || self.shadow_opacity > 0
            || self.edges.active()
            || self.styles.active()
    }
    pub fn validate(self) -> Result<(), String> {
        self.styles.validate()?;
        if self.edges.depth > 128
            || self.border_width > 64
            || self.shadow_blur > 64
            || self.shadow_blur == 0
            || self.shadow_offset.abs_diff(0) > 64
        {
            return Err("Image effects exceed supported sizes.".into());
        }
        if let Some(f) = self.frame {
            crate::document::validate_dimensions(f.width, f.height)?;
            let limit = crate::document::MAX_IMAGE_SIDE;
            if f.x.abs_diff(0) > limit || f.y.abs_diff(0) > limit {
                return Err("Invalid image-effect frame.".into());
            }
        } else if self.border_width > 0
            || self.shadow_opacity > 0
            || self.edges.active()
            || self.styles.active()
        {
            return Err("Edges, border and shadow need an image frame.".into());
        }
        Ok(())
    }
    pub fn shift(&mut self, x: i32, y: i32) {
        if let Some(frame) = &mut self.frame {
            frame.x -= x;
            frame.y -= y;
        }
    }
}

/// Adjust effects and grow only the visible canvas as needed, in one caller-owned undo step.
pub fn update(document: &mut Document, mut effects: ImageEffects) -> Result<(), String> {
    let before = document.geometry;
    if effects.active() && effects.frame.is_none() {
        effects.frame = Some(before.view_rect([document.image.width(), document.image.height()]));
    }
    effects.validate()?;
    document.geometry.effects = effects;
    if let Some(frame) = effects.frame.filter(|_| {
        effects.border_width > 0
            || effects.shadow_opacity > 0
            || effects.styles.reflection_height > 0
    }) {
        let border = effects.border_width as f32;
        let shadow = if effects.shadow_opacity > 0 {
            effects.shadow_blur as f32 * 3. + effects.shadow_offset.unsigned_abs() as f32
        } else {
            0.
        };
        let mut rect = Rect::from_min_size(
            egui::pos2(frame.x as f32, frame.y as f32),
            egui::vec2(frame.width as f32, frame.height as f32),
        )
        .expand(border + shadow);
        if effects.styles.reflection_height > 0 {
            rect.max.y += frame.height as f32 * effects.styles.reflection_height as f32 / 100.
                + effects.styles.reflection_gap as f32;
        }
        if let Err(error) = crate::canvas::extend_to(document, rect) {
            document.geometry = before;
            return Err(error);
        }
    }
    Ok(())
}

/// Same source-space calculation at every preview/export scale. Styling follows
/// the composed artwork through subsequent crop, rotation, flip and resize.
pub fn apply(pixels: &mut RgbaImage, effects: ImageEffects, origin: [f32; 2], scale: [f32; 2]) {
    if !effects.active() {
        return;
    }
    let frame = effects.frame.map(|f| {
        Rect::from_min_size(
            egui::pos2(f.x as f32, f.y as f32),
            egui::vec2(f.width as f32, f.height as f32),
        )
    });
    for (x, y, pixel) in pixels.enumerate_pixels_mut() {
        effects.styles.color(pixel);
        if effects.grayscale {
            let gray = ((u32::from(pixel[0]) * 54
                + u32::from(pixel[1]) * 183
                + u32::from(pixel[2]) * 19
                + 128)
                / 256) as u8;
            pixel[0] = gray;
            pixel[1] = gray;
            pixel[2] = gray;
        }
        let Some(frame) = frame else {
            continue;
        };
        let p = egui::pos2(
            (x as f32 + 0.5) / scale[0] + origin[0],
            (y as f32 + 0.5) / scale[1] + origin[1],
        );
        effects.styles.fade(pixel, p, frame);
        if effects.edges.active() {
            let distance = effects.edges.distance(p, frame);
            let aa = (1. / scale[0]).max(1. / scale[1]);
            let coverage = (distance / aa + 0.5).clamp(0., 1.);
            pixel[3] = (pixel[3] as f32 * coverage).round() as u8;
            let mut behind = Rgba([0; 4]);
            if effects.shadow_opacity > 0 {
                let shadow_point = p - egui::Vec2::splat(effects.shadow_offset as f32);
                let gap = (-effects.edges.distance(shadow_point, frame)
                    - effects.border_width as f32)
                    .max(0.);
                let sigma = effects.shadow_blur as f32;
                behind[3] = ((-gap * gap / (2. * sigma * sigma)).exp()
                    * effects.shadow_opacity as f32)
                    .round() as u8;
            }
            if effects.border_width > 0 {
                let outer_coverage =
                    ((distance + effects.border_width as f32) / aa + 0.5).clamp(0., 1.);
                let mut border = effects.border_color;
                border[3] = (border[3] as f32 * (outer_coverage - coverage).max(0.)).round() as u8;
                behind.blend(&Rgba(border));
            }
            // Keep holes in the original artwork transparent, as with plain frame shadows.
            if distance > 0. {
                behind = Rgba([0; 4]);
            }
            behind.blend(pixel);
            *pixel = behind;
            continue;
        }
        let outer = frame.expand(effects.border_width as f32);
        if effects.shadow_opacity > 0 && !outer.contains(p) {
            let shadow = outer.translate(egui::vec2(
                effects.shadow_offset as f32,
                effects.shadow_offset as f32,
            ));
            let dx = (shadow.left() - p.x).max(p.x - shadow.right()).max(0.);
            let dy = (shadow.top() - p.y).max(p.y - shadow.bottom()).max(0.);
            let sigma = effects.shadow_blur as f32;
            let opacity =
                (-(dx * dx + dy * dy) / (2. * sigma * sigma)).exp() * effects.shadow_opacity as f32;
            let mut behind = Rgba([0, 0, 0, opacity.round() as u8]);
            behind.blend(pixel);
            *pixel = behind;
        }
        if effects.border_width > 0 && outer.contains(p) && !frame.contains(p) {
            pixel.blend(&Rgba(effects.border_color));
        }
    }
    if let Some(frame) = frame {
        effects.styles.compose(pixels, frame, origin, scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document::{self, EditHistory},
        geometry::CropRect,
        render,
    };
    use std::sync::Arc;

    fn fixture() -> Document {
        Document {
            image: Arc::new(RgbaImage::from_fn(120, 80, |x, y| {
                Rgba([x as u8, y as u8, 180, if x < 6 { 0 } else { 255 }])
            })),
            annotations: vec![crate::markup_tests::object(
                document::AnnotationKind::Rectangle,
                [20., 20.],
                [50., 50.],
            )],
            geometry: Default::default(),
        }
    }

    #[test]
    fn combined_styles_fade_reflection_perspective_round_trip_and_export() {
        let mut d = fixture();
        let original = d.clone();
        update(
            &mut d,
            ImageEffects {
                edges: Edges {
                    style: EdgeStyle::Sawtooth,
                    depth: 6,
                    sides: [false, true, true, false],
                },
                styles: EffectStyles {
                    filter: ColorFilter::Warm,
                    brightness: 5,
                    contrast: 10,
                    saturation: 20,
                    perspective: [18, -8],
                    reflection_height: 40,
                    reflection_opacity: 150,
                    reflection_gap: 4,
                    fade_depth: 8,
                    fade_sides: [true, false, false, false],
                },
                shadow_opacity: 70,
                border_width: 2,
                ..Default::default()
            },
        )
        .unwrap();
        let flat = render::flatten(&d).unwrap();
        assert!(d.output_size()[1] > 80);
        assert_eq!(flat, render::privacy_preview(&d, 2048).unwrap());
        assert_eq!(flat, render::interactive_preview(&d, 2048).unwrap());
        assert_eq!(flat, crate::image_clipboard::copy_pixels(&d, None).unwrap());
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("styled.myimage");
        document::save_project(&d, &project).unwrap();
        let reopened = document::load_project(&project).unwrap();
        assert_eq!(d.edit_state(), reopened.edit_state());
        assert_eq!(*original.image, *reopened.image);
        assert_eq!(original.annotations, reopened.annotations);
        let flattened = Document {
            image: Arc::new(flat),
            annotations: vec![],
            geometry: Default::default(),
        };
        for format in crate::export::ExportFormat::ALL {
            let actual = dir.path().join(format!("actual.{}", format.extension()));
            let expected = dir.path().join(format!("expected.{}", format.extension()));
            let options = crate::export::ExportOptions {
                format,
                ..Default::default()
            };
            crate::export::save(&reopened, &actual, options).unwrap();
            crate::export::save(&flattened, &expected, options).unwrap();
            assert_eq!(
                std::fs::read(actual).unwrap(),
                std::fs::read(expected).unwrap()
            );
        }
    }

    #[test]
    fn fade_and_reflection_preserve_transparency_and_have_controllable_extent() {
        let mut d = fixture();
        update(
            &mut d,
            ImageEffects {
                styles: EffectStyles {
                    fade_depth: 20,
                    fade_sides: [true, false, false, false],
                    reflection_height: 50,
                    reflection_opacity: 180,
                    reflection_gap: 4,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();
        let flat = render::flatten(&d).unwrap();
        assert_eq!(flat.dimensions(), (120, 124));
        assert!(flat.get_pixel(60, 0)[3] < 5);
        assert_eq!(flat.get_pixel(60, 30)[3], 255);
        assert_eq!(flat.get_pixel(60, 82)[3], 0);
        assert!(flat.get_pixel(60, 85)[3] > flat.get_pixel(60, 110)[3]);
        assert_eq!(flat.get_pixel(2, 85)[3], 0);
        let mut clear = d.geometry.effects;
        clear.styles = Default::default();
        update(&mut d, clear).unwrap();
        let flat = render::flatten(&d).unwrap();
        assert_eq!(flat.get_pixel(60, 0)[3], 255);
        assert_eq!(flat.get_pixel(60, 85)[3], 0);
    }

    #[test]
    fn perspective_maps_annotation_handles_and_pointer_back_to_original() {
        let mut d = fixture();
        update(
            &mut d,
            ImageEffects {
                styles: EffectStyles {
                    perspective: [25, -15],
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();
        for turns in 0..4 {
            d.geometry.quarter_turns = turns;
            d.geometry.flip_horizontal = true;
            d.geometry.resize = Some([300, 200]);
            for p in [
                document::Point { x: 20., y: 20. },
                document::Point { x: 50., y: 50. },
                document::Point { x: 119., y: 79. },
            ] {
                let shown = d.geometry.map_point(p, [120, 80]);
                let source = d.geometry.inverse_point(shown, [120, 80]);
                assert!((source.x - p.x).abs() < 0.001 && (source.y - p.y).abs() < 0.001);
            }
        }
    }

    #[test]
    fn edges_are_reversible_portable_and_match_preview_and_clipboard() {
        for style in [
            EdgeStyle::Torn,
            EdgeStyle::Wave,
            EdgeStyle::Sawtooth,
            EdgeStyle::Rounded,
        ] {
            let mut d = fixture();
            let original = d.clone();
            let before = d.edit_state();
            let mut history = EditHistory::new(before.clone());
            update(
                &mut d,
                ImageEffects {
                    edges: Edges {
                        style,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap();
            history.commit(before.clone(), &d);
            let output = render::flatten(&d).unwrap();
            assert!(
                output
                    .pixels()
                    .zip(render::flatten(&original).unwrap().pixels())
                    .any(|(a, b)| a[3] < b[3])
            );
            assert_eq!(output, render::interactive_preview(&d, 768).unwrap());
            assert_eq!(
                output,
                crate::image_clipboard::copy_pixels(&d, None).unwrap()
            );
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("edge.myimage");
            document::save_project(&d, &path).unwrap();
            let reopened = document::load_project(&path).unwrap();
            assert_eq!(reopened.edit_state(), d.edit_state());
            assert_eq!(*reopened.image, *original.image);
            assert_eq!(output, render::flatten(&reopened).unwrap());
            assert!(history.undo(&mut d));
            assert_eq!(d.edit_state(), before);
        }
    }

    #[test]
    fn edge_sides_depth_and_source_space_are_respected() {
        let frame = CanvasRect {
            x: 0,
            y: 0,
            width: 120,
            height: 80,
        };
        for style in [EdgeStyle::Torn, EdgeStyle::Wave, EdgeStyle::Sawtooth] {
            let effects = ImageEffects {
                frame: Some(frame),
                edges: Edges {
                    style,
                    depth: 20,
                    sides: [false, false, true, false],
                },
                ..Default::default()
            };
            let mut output = RgbaImage::from_pixel(120, 80, Rgba([150, 80, 20, 255]));
            apply(&mut output, effects, [0.; 2], [1.; 2]);
            assert!((0..120).all(|x| output.get_pixel(x, 0)[3] == 255));
            assert!((0..120).any(|x| output.get_pixel(x, 79)[3] == 0));
            assert_eq!(output.get_pixel(60, 40)[3], 255);
            // A source-space crop must match the corresponding part of the full render.
            let mut crop = RgbaImage::from_pixel(60, 30, Rgba([150, 80, 20, 255]));
            apply(&mut crop, effects, [30., 50.], [1.; 2]);
            assert_eq!(
                crop,
                image::imageops::crop_imm(&output, 30, 50, 60, 30).to_image()
            );
        }
        let legacy = r#"{"grayscale":false,"border_width":0,"border_color":[38,35,32,255],"shadow_blur":12,"shadow_opacity":0,"shadow_offset":8,"frame":null}"#;
        let effects: ImageEffects = serde_json::from_str(legacy).unwrap();
        assert_eq!(effects.edges, Edges::default());
        assert!(!effects.active());
    }

    #[test]
    fn border_follows_shaped_edge_and_invalid_depth_is_atomic() {
        let mut d = fixture();
        update(
            &mut d,
            ImageEffects {
                edges: Edges {
                    style: EdgeStyle::Rounded,
                    depth: 20,
                    ..Default::default()
                },
                border_width: 3,
                border_color: [250, 20, 30, 255],
                ..Default::default()
            },
        )
        .unwrap();
        let flat = render::flatten(&d).unwrap();
        let frame = d.geometry.effects.frame.unwrap();
        let canvas = d.geometry.canvas.unwrap();
        let x = (frame.x - canvas.x) as u32;
        let y = (frame.y - canvas.y) as u32;
        assert_eq!(
            flat.get_pixel(x, y)[3],
            0,
            "rounded corner must be transparent"
        );
        assert_eq!(flat.get_pixel(x + 20, y - 2).0, [250, 20, 30, 255]);
        let before = d.edit_state();
        let mut invalid = d.geometry.effects;
        invalid.edges.depth = 129;
        assert!(update(&mut d, invalid).is_err());
        assert_eq!(before, d.edit_state());
    }

    #[test]
    fn styling_preserves_original_layers_and_round_trips_with_one_undo() {
        let mut d = fixture();
        let original = d.clone();
        let before = d.edit_state();
        let mut history = EditHistory::new(before.clone());
        update(
            &mut d,
            ImageEffects {
                border_width: 4,
                shadow_opacity: 100,
                grayscale: true,
                ..Default::default()
            },
        )
        .unwrap();
        history.commit(before.clone(), &d);
        assert!(Arc::ptr_eq(&d.image, &original.image));
        assert_eq!(d.annotations, original.annotations);
        assert!(d.output_size()[0] > 120 && d.output_size()[1] > 80);
        let flat = render::flatten(&d).unwrap();
        assert_eq!(flat, render::privacy_preview(&d, 2048).unwrap());
        assert_eq!(flat, render::interactive_preview(&d, 768).unwrap());
        assert_eq!(flat, crate::image_clipboard::copy_pixels(&d, None).unwrap());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("effects.myimage");
        document::save_project(&d, &path).unwrap();
        let reopened = document::load_project(&path).unwrap();
        assert_eq!(reopened.edit_state(), d.edit_state());
        assert_eq!(*reopened.image, *original.image);
        assert_eq!(render::flatten(&reopened).unwrap(), flat);
        let flattened = Document {
            image: Arc::new(flat.clone()),
            annotations: vec![],
            geometry: Default::default(),
        };
        for format in crate::export::ExportFormat::ALL {
            let actual = dir.path().join(format!("styled.{}", format.extension()));
            let expected = dir.path().join(format!("flattened.{}", format.extension()));
            let options = crate::export::ExportOptions {
                format,
                ..Default::default()
            };
            crate::export::save(&d, &actual, options).unwrap();
            crate::export::save(&flattened, &expected, options).unwrap();
            assert_eq!(
                std::fs::read(actual).unwrap(),
                std::fs::read(expected).unwrap()
            );
        }
        assert!(history.undo(&mut d));
        assert_eq!(d.edit_state(), before);
        assert!(!history.undo(&mut d));
    }

    #[test]
    fn grayscale_preserves_alpha_and_includes_annotations_and_geometry() {
        let mut d = fixture();
        let colored = render::flatten(&d).unwrap();
        update(
            &mut d,
            ImageEffects {
                grayscale: true,
                ..Default::default()
            },
        )
        .unwrap();
        let gray = render::flatten(&d).unwrap();
        for (a, b) in colored.pixels().zip(gray.pixels()) {
            assert_eq!(a[3], b[3]);
            assert_eq!(b[0], b[1]);
            assert_eq!(b[1], b[2]);
        }
        d.geometry.crop = Some(CropRect {
            x: 10,
            y: 5,
            width: 90,
            height: 65,
        });
        d.geometry.quarter_turns = 1;
        d.geometry.flip_horizontal = true;
        d.geometry.resize = Some([130, 180]);
        let transformed = render::flatten(&d).unwrap();
        assert_eq!(transformed.dimensions(), (130, 180));
        assert_eq!(transformed, d.geometry.apply(&gray).unwrap());
        let thumbnail = render::thumbnail(&d, 65, 90).unwrap();
        assert_eq!(thumbnail.dimensions(), (65, 90));
        for p in thumbnail.pixels() {
            assert_eq!(p[0], p[1]);
            assert_eq!(p[1], p[2]);
        }
    }

    #[test]
    fn border_shadow_are_outside_frame_and_adjustments_do_not_accumulate_padding() {
        let mut d = fixture();
        let style = ImageEffects {
            border_width: 4,
            border_color: [230, 70, 30, 255],
            shadow_blur: 8,
            shadow_offset: 5,
            shadow_opacity: 120,
            ..Default::default()
        };
        update(&mut d, style).unwrap();
        let geometry = d.geometry;
        let frame = geometry.effects.frame.unwrap();
        update(&mut d, geometry.effects).unwrap();
        assert_eq!(d.geometry, geometry);
        let flat = render::flatten(&d).unwrap();
        let offset = geometry.canvas.unwrap();
        let x = (frame.x - offset.x) as u32;
        let y = (frame.y - offset.y) as u32;
        assert_eq!(flat.get_pixel(x - 2, y + 20).0, style.border_color);
        assert!(flat.get_pixel(x + frame.width + 8, y + 30)[3] > 0);
        assert_eq!(
            flat.get_pixel(x + 2, y + 20)[3],
            0,
            "transparent source content must stay transparent"
        );
        let clear = ImageEffects {
            frame: Some(frame),
            ..Default::default()
        };
        update(&mut d, clear).unwrap();
        let plain = render::flatten(&d).unwrap();
        assert_eq!(plain.get_pixel(x - 2, y + 20)[3], 0);
        let mut restored = fixture();
        restored.geometry = geometry;
        restored.geometry.effects = clear;
        assert_eq!(render::flatten(&restored).unwrap(), plain);
    }

    #[test]
    fn invalid_styles_are_atomic_and_legacy_geometry_remains_compatible() {
        let mut d = fixture();
        let before = d.edit_state();
        assert!(
            update(
                &mut d,
                ImageEffects {
                    border_width: 65,
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert_eq!(d.edit_state(), before);
        let old = r#"{"crop":null,"quarter_turns":0,"resize":null}"#;
        let geometry: crate::geometry::ImageGeometry = serde_json::from_str(old).unwrap();
        assert!(geometry.effects.is_default());
    }
}
