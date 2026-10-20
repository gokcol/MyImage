//! Native raster clipboard exchange, with full-resolution composition and alpha.
use crate::{
    document::{Document, Point},
    stamps::StampSource,
};
use eframe::egui::{self, Pos2, Rect};
use image::RgbaImage;
use std::sync::atomic::{AtomicBool, Ordering};

static PASTE_REQUESTED: AtomicBool = AtomicBool::new(false);
pub const IMAGE_PASTE: &str = "\0MYIMAGE-NATIVE-IMAGE-PASTE\0";
pub fn request_paste() {
    PASTE_REQUESTED.store(true, Ordering::Release);
}
pub fn complete_paste_input(ctx: &egui::Context, input: &mut egui::RawInput) {
    if PASTE_REQUESTED.swap(false, Ordering::AcqRel)
        && !input
            .events
            .iter()
            .any(|e| matches!(e, egui::Event::Paste(_)))
        && ctx
            .memory(|m| m.focused())
            .is_none_or(|id| egui::text_edit::TextEditState::load(ctx, id).is_none())
    {
        // egui-winit emits no Paste event for an image-only system clipboard.
        input.events.push(egui::Event::Paste(IMAGE_PASTE.into()));
    }
}

pub fn read_image() -> Result<RgbaImage, String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Cannot access the clipboard: {e}"))?;
    let image = clipboard
        .get_image()
        .map_err(|_| "The clipboard has no image. Copy an image or area first.".to_string())?;
    let width = u32::try_from(image.width).map_err(|_| "Clipboard image is too wide.")?;
    let height = u32::try_from(image.height).map_err(|_| "Clipboard image is too tall.")?;
    crate::document::validate_dimensions(width, height)?;
    if u64::from(width) * u64::from(height) > 64_000_000 {
        return Err("Clipboard images are limited to 64 megapixels.".into());
    }
    RgbaImage::from_raw(width, height, image.bytes.into_owned())
        .ok_or("Invalid clipboard image pixels.".into())
}

/// Decode the saved file at full resolution without opening it in the Editor.
/// Projects use the same composition as exports, including annotations and effects.
pub fn file_pixels(path: &std::path::Path) -> Result<RgbaImage, String> {
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("myimage"))
    {
        crate::render::flatten(&crate::document::load_project(path)?)
    } else {
        image::ImageReader::open(path)
            .map_err(|error| error.to_string())?
            .with_guessed_format()
            .map_err(|error| error.to_string())?
            .decode()
            .map(|image| image.into_rgba8())
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
pub fn copy_pixels(
    document: &Document,
    area: Option<(Rect, Vec<Point>)>,
) -> Result<RgbaImage, String> {
    copy_pixels_masked(document, area, None)
}
/// Pixel-edge rounding shared by clipboard output and the visible copy-scope label.
pub fn copy_bounds(document: &Document, rect: Rect) -> Result<crate::geometry::CropRect, String> {
    let size = [document.image.width(), document.image.height()];
    let [width, height] = document.output_size();
    let map = |p: Pos2| {
        let p = document.geometry.map_point(Point { x: p.x, y: p.y }, size);
        Pos2::new(p.x, p.y)
    };
    let mapped = Rect::from_two_pos(map(rect.min), map(rect.max));
    let left = mapped.left().floor().max(0.) as u32;
    let top = mapped.top().floor().max(0.) as u32;
    let right = (mapped.right().ceil().max(0.) as u32).min(width);
    let bottom = (mapped.bottom().ceil().max(0.) as u32).min(height);
    if right <= left || bottom <= top {
        return Err("The selected area is outside the visible image.".into());
    }
    Ok(crate::geometry::CropRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

pub fn copy_pixels_masked(
    document: &Document,
    area: Option<(Rect, Vec<Point>)>,
    pixel_mask: Option<crate::fill::FillRegion>,
) -> Result<RgbaImage, String> {
    let flat = crate::render::flatten(document)?;
    let Some((rect, mask)) = area else {
        return Ok(flat);
    };
    let size = [document.image.width(), document.image.height()];
    let crop = copy_bounds(document, rect)?;
    let (left, top) = (crop.x, crop.y);
    let mut result =
        image::imageops::crop_imm(&flat, left, top, crop.width, crop.height).to_image();
    if !mask.is_empty() || pixel_mask.is_some() {
        let base = document.geometry.inverse_point(
            Point {
                x: left as f32,
                y: top as f32,
            },
            size,
        );
        let xs: Vec<_> = (0..result.width())
            .map(|x| {
                document.geometry.inverse_point(
                    Point {
                        x: (left + x) as f32 + 0.5,
                        y: top as f32,
                    },
                    size,
                )
            })
            .collect();
        let ys: Vec<_> = (0..result.height())
            .map(|y| {
                document.geometry.inverse_point(
                    Point {
                        x: left as f32,
                        y: (top + y) as f32 + 0.5,
                    },
                    size,
                )
            })
            .collect();
        for y in 0..result.height() {
            for x in 0..result.width() {
                let p = Point {
                    x: xs[x as usize].x + ys[y as usize].x - base.x,
                    y: xs[x as usize].y + ys[y as usize].y - base.y,
                };
                let p = egui::pos2(
                    (p.x - rect.left()) / rect.width(),
                    (p.y - rect.top()) / rect.height(),
                );
                let inside_lasso =
                    mask.is_empty() || crate::studio::polygon_contains(&mask, p.x, p.y);
                if !inside_lasso || pixel_mask.as_ref().is_some_and(|m| !m.contains(p.x, p.y)) {
                    result.put_pixel(x, y, image::Rgba([0; 4]));
                }
            }
        }
    }
    Ok(result)
}
#[cfg_attr(test, allow(dead_code))]
pub fn write_image(image: &RgbaImage) -> Result<(), String> {
    arboard::Clipboard::new()
        .map_err(|e| e.to_string())?
        .set_image(arboard::ImageData {
            width: image.width() as usize,
            height: image.height() as usize,
            bytes: std::borrow::Cow::Borrowed(image.as_raw()),
        })
        .map_err(|e| format!("Cannot copy image: {e}"))
}
pub fn pasted_raster(
    mut image: RgbaImage,
    document: &Document,
) -> Result<(StampSource, Rect), String> {
    let original = [document.image.width(), document.image.height()];
    let [w, h] = document.geometry.output_size(original);
    let rect = Rect::from_center_size(
        egui::pos2(w as f32 / 2., h as f32 / 2.),
        egui::vec2(image.width() as f32, image.height() as f32),
    );
    let inverse = |p: Pos2| {
        let p = document
            .geometry
            .inverse_point(Point { x: p.x, y: p.y }, original);
        egui::pos2(p.x, p.y)
    };
    let bounds = Rect::from_two_pos(inverse(rect.min), inverse(rect.max));
    if document.geometry.flip_horizontal {
        image::imageops::flip_horizontal_in_place(&mut image);
    }
    if document.geometry.flip_vertical {
        image::imageops::flip_vertical_in_place(&mut image);
    }
    let pixels = match document.geometry.quarter_turns {
        1 => image::imageops::rotate270(&image),
        2 => image::imageops::rotate180(&image),
        3 => image::imageops::rotate90(&image),
        _ => image,
    };
    Ok((StampSource::from_pixels(&pixels)?, bounds))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document::{AnnotationKind, EditHistory},
        geometry::{CropRect, ImageGeometry},
    };
    use std::sync::Arc;
    fn document() -> Document {
        Document {
            image: Arc::new(RgbaImage::from_fn(16, 12, |x, y| {
                image::Rgba([x as u8 * 10, y as u8 * 10, 40, 255])
            })),
            annotations: vec![],
            geometry: Default::default(),
        }
    }
    #[test]
    fn file_copy_preserves_raster_alpha_and_composes_saved_project_edits() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("transparent.png");
        let pixels = RgbaImage::from_fn(32, 24, |x, y| {
            image::Rgba([x as u8, y as u8, 80, if x < 8 { 0 } else { 200 }])
        });
        pixels.save(&png).unwrap();
        assert_eq!(file_pixels(&png).unwrap(), pixels);

        let mut doc = Document {
            image: Arc::new(pixels),
            annotations: vec![crate::markup_tests::object(
                AnnotationKind::Rectangle,
                [10., 5.],
                [25., 18.],
            )],
            geometry: Default::default(),
        };
        doc.annotations[0].fill = true;
        doc.annotations[0].color = [255, 0, 0, 255];
        doc.geometry.quarter_turns = 1;
        doc.geometry.effects.grayscale = true;
        let project = dir.path().join("annotated.MYIMAGE");
        crate::document::save_project(&doc, &project).unwrap();
        let before = std::fs::read(&project).unwrap();
        let copied = file_pixels(&project).unwrap();
        assert_eq!(copied, crate::render::flatten(&doc).unwrap());
        assert_eq!(copied.dimensions(), (24, 32));
        assert_eq!(std::fs::read(&project).unwrap(), before);
        assert_eq!(
            crate::document::load_project(&project)
                .unwrap()
                .edit_state(),
            doc.edit_state()
        );
    }

    #[test]
    fn file_copy_reports_missing_or_corrupt_sources_without_fallback_pixels() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.png");
        assert!(file_pixels(&path).is_err());
        std::fs::write(&path, b"not an image").unwrap();
        assert!(file_pixels(&path).is_err());
        let project = dir.path().join("broken.myimage");
        std::fs::write(&project, b"not a project").unwrap();
        assert!(file_pixels(&project).is_err());
    }

    #[test]
    fn copied_pixels_include_visible_edits_and_display_geometry() {
        let mut d = document();
        let mut a = crate::markup_tests::object(AnnotationKind::Rectangle, [1., 1.], [8., 8.]);
        a.fill = true;
        a.color = [255, 0, 0, 255];
        d.annotations.push(a);
        d.geometry = ImageGeometry {
            effects: Default::default(),
            viewport: None,
            flip_horizontal: false,
            flip_vertical: false,
            canvas: None,
            cutouts: [None; 16],
            crop: Some(CropRect {
                x: 2,
                y: 1,
                width: 12,
                height: 10,
            }),
            quarter_turns: 1,
            resize: Some([20, 24]),
        };
        let original = d.edit_state();
        let full = crate::render::flatten(&d).unwrap();
        assert_eq!(copy_pixels(&d, None).unwrap(), full);
        let area = copy_pixels(
            &d,
            Some((
                Rect::from_min_max(egui::pos2(4., 3.), egui::pos2(8., 7.)),
                vec![],
            )),
        )
        .unwrap();
        assert_eq!(
            area,
            image::imageops::crop_imm(&full, 8, 4, 8, 8).to_image()
        );
        assert_eq!(original, d.edit_state());
    }
    #[test]
    fn lasso_copy_preserves_transparency_and_clips_to_canvas() {
        let d = document();
        let rect = Rect::from_min_max(egui::pos2(2., 2.), egui::pos2(10., 10.));
        let mask = vec![
            Point { x: 0., y: 0. },
            Point { x: 1., y: 0. },
            Point { x: 0., y: 1. },
        ];
        let pixels = copy_pixels(&d, Some((rect, mask))).unwrap();
        assert_eq!(pixels.dimensions(), (8, 8));
        assert_eq!(pixels.get_pixel(0, 0), d.image.get_pixel(2, 2));
        assert_eq!(pixels.get_pixel(7, 7).0, [0; 4]);
        let outside = Rect::from_min_max(egui::pos2(-4., -3.), egui::pos2(5., 6.));
        assert_eq!(
            copy_pixels(&d, Some((outside, vec![])))
                .unwrap()
                .dimensions(),
            (5, 6)
        );
        assert!(
            copy_pixels(
                &d,
                Some((outside.translate(egui::vec2(100., 100.)), vec![]))
            )
            .is_err()
        );
    }
    #[test]
    fn pasted_layers_keep_orientation_alpha_project_data_and_undo() {
        let source = RgbaImage::from_fn(4, 2, |x, y| {
            image::Rgba([20 + x as u8 * 40, 30 + y as u8 * 80, 200, 255])
        });
        for turn in 0..4 {
            let mut d = document();
            d.geometry.quarter_turns = turn;
            let mut history = EditHistory::new(d.edit_state());
            let before = d.edit_state();
            let (raster, rect) = pasted_raster(source.clone(), &d).unwrap();
            let mut a = crate::markup_tests::object(
                AnnotationKind::Stamp,
                [rect.left(), rect.top()],
                [rect.right(), rect.bottom()],
            );
            a.stamp = Some(raster);
            a.color = [255; 4];
            d.annotations.push(a);
            history.commit(before, &d);
            let rendered = crate::render::flatten(&d).unwrap();
            let x = rendered.width() / 2 - 2;
            let y = rendered.height() / 2 - 1;
            assert_eq!(
                image::imageops::crop_imm(&rendered, x, y, 4, 2).to_image(),
                source,
                "rotation {turn}"
            );
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("paste.myimage");
            crate::document::save_project(&d, &path).unwrap();
            let reopened = crate::document::load_project(&path).unwrap();
            assert_eq!(reopened.annotations, d.annotations);
            assert_eq!(crate::render::flatten(&reopened).unwrap(), rendered);
            let mut bytes = std::fs::read(&path).unwrap();
            bytes[8..12].copy_from_slice(&12_u32.to_le_bytes());
            std::fs::write(&path, bytes).unwrap();
            assert!(
                crate::document::load_project(&path)
                    .unwrap_err()
                    .contains("version 13")
            );
            history.undo(&mut d);
            assert!(d.annotations.is_empty());
            history.redo(&mut d);
            assert_eq!(d.annotations.len(), 1);
        }
    }
    #[test]
    fn image_only_paste_is_injected_once_without_duplicate_text_events() {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        request_paste();
        complete_paste_input(&ctx, &mut input);
        assert_eq!(input.events, vec![egui::Event::Paste(IMAGE_PASTE.into())]);
        complete_paste_input(&ctx, &mut input);
        assert_eq!(input.events.len(), 1);
        input.events = vec![egui::Event::Paste("text".into())];
        request_paste();
        complete_paste_input(&ctx, &mut input);
        assert_eq!(input.events.len(), 1);
    }
}
