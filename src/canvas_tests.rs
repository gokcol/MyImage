use crate::{
    canvas,
    document::{self, AnnotationKind, Document, EditHistory, Point},
    geometry::CropRect,
    render,
};
use eframe::egui::{self, Rect};
use image::{Rgba, RgbaImage};
use std::sync::Arc;

fn document() -> Document {
    Document {
        image: Arc::new(RgbaImage::from_fn(20, 16, |x, y| {
            Rgba([x as u8, y as u8, 80, 255])
        })),
        annotations: vec![],
        geometry: Default::default(),
    }
}
fn region(a: [f32; 2], b: [f32; 2]) -> Rect {
    Rect::from_min_max(egui::pos2(a[0], a[1]), egui::pos2(b[0], b[1]))
}

#[test]
fn expanded_canvas_preserves_original_pixels_transparency_layers_and_undo() {
    let mut d = document();
    let original = d.image.clone();
    let before = d.edit_state();
    let mut h = EditHistory::new(before.clone());
    canvas::extend_to(&mut d, region([-5., -3.], [31., 25.])).unwrap();
    let mut stamp = crate::markup_tests::object(AnnotationKind::Stamp, [22., 5.], [30., 12.]);
    stamp.color = [255; 4];
    stamp.stamp = Some(
        crate::stamps::StampSource::from_pixels(&RgbaImage::from_pixel(
            8,
            7,
            Rgba([210, 10, 40, 255]),
        ))
        .unwrap(),
    );
    d.annotations.push(stamp);
    h.commit(before.clone(), &d);
    let flat = render::flatten(&d).unwrap();
    assert_eq!(flat.dimensions(), (36, 28));
    for y in 0..16 {
        for x in 0..20 {
            assert_eq!(flat.get_pixel(x + 5, y + 3), original.get_pixel(x, y));
        }
    }
    assert_eq!(flat.get_pixel(0, 0)[3], 0);
    assert_eq!(flat.get_pixel(27, 8).0, [210, 10, 40, 255]);
    assert!(Arc::ptr_eq(&d.image, &original));
    assert_eq!(render::thumbnail(&d, 36, 28).unwrap(), flat);
    assert_eq!(crate::image_clipboard::copy_pixels(&d, None).unwrap(), flat);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("expanded.myimage");
    document::save_project(&d, &path).unwrap();
    let mut reopened = document::load_project(&path).unwrap();
    assert_eq!(reopened.edit_state(), d.edit_state());
    assert_eq!(reopened.image, original);
    assert_eq!(render::flatten(&reopened).unwrap(), flat);
    reopened.annotations[0].end.x += 1.;
    assert_ne!(render::flatten(&reopened).unwrap(), flat);
    assert!(h.undo(&mut d));
    assert_eq!(d.edit_state(), before);
    assert!(h.redo(&mut d));
    assert_eq!(render::flatten(&d).unwrap(), flat);
}

#[test]
fn expanded_canvas_geometry_roundtrips_rotations_cutouts_and_resize_without_revealing_crop() {
    for turn in 0..4 {
        let mut d = document();
        d.geometry.crop = Some(CropRect {
            x: 3,
            y: 2,
            width: 12,
            height: 10,
        });
        d.geometry.quarter_turns = turn;
        d.geometry.cutouts[0] = Some(crate::geometry::Cutout {
            axis: 0,
            start: 6,
            end: 8,
        });
        let rotated = d.geometry.rotated_size([20, 16]);
        d.geometry.resize = Some([rotated[0] * 2, rotated[1] * 2]);
        canvas::extend_to(&mut d, region([-2., -3.], [24., 20.])).unwrap();
        let working = canvas::render_document(&d).unwrap();
        let c = d.geometry.canvas.unwrap();
        for p in [
            Point { x: -1., y: -2. },
            Point { x: 4., y: 5. },
            Point { x: 21., y: 18. },
        ] {
            let mapped = d.geometry.map_point(p, [20, 16]);
            let inverse = d.geometry.inverse_point(mapped, [20, 16]);
            assert!((inverse.x - p.x).abs() < 0.001 && (inverse.y - p.y).abs() < 0.001);
            assert_eq!(
                mapped,
                working.geometry.map_point(
                    Point {
                        x: p.x - c.x as f32,
                        y: p.y - c.y as f32
                    },
                    [c.width, c.height]
                )
            );
        }
        let flat = render::flatten(&d).unwrap();
        assert_eq!(flat.dimensions(), (d.output_size()[0], d.output_size()[1]));
        let hidden = d.geometry.map_point(Point { x: 0., y: 0. }, [20, 16]);
        assert_eq!(flat.get_pixel(hidden.x as u32, hidden.y as u32)[3], 0);
        assert_eq!(
            working
                .geometry
                .apply(&render::privacy_preview(&d, 4096).unwrap())
                .unwrap(),
            flat
        );
    }
}

#[test]
fn expanded_canvas_supports_fill_and_clear_in_new_space() {
    let mut d = document();
    canvas::extend_to(&mut d, region([-10., -10.], [30., 26.])).unwrap();
    let mut a = crate::markup_tests::object(AnnotationKind::Fill, [-10., -10.], [30., 26.]);
    a.color = [220, 30, 40, 255];
    a.fill_region = Some(canvas::fill_region(&d, &a, Point { x: -5., y: -5. }, 0, false).unwrap());
    d.annotations.push(a);
    let flat = render::flatten(&d).unwrap();
    assert_eq!(flat.get_pixel(2, 2).0, [220, 30, 40, 255]);
    assert_eq!(flat.get_pixel(12, 12), d.image.get_pixel(2, 2));
    d.annotations.push(crate::markup_tests::object(
        AnnotationKind::DeleteArea,
        [-8., -8.],
        [-3., -3.],
    ));
    assert_eq!(render::flatten(&d).unwrap().get_pixel(4, 4)[3], 0);
}

#[test]
fn oversized_canvas_fails_without_editing() {
    let mut d = document();
    let before = d.edit_state();
    for bounds in [
        region([-1e9, 0.], [20., 16.]),
        region([0., 0.], [32768., 32768.]),
    ] {
        assert!(canvas::extend_to(&mut d, bounds).is_err());
        assert_eq!(d.edit_state(), before);
    }
}

#[test]
fn whole_image_flips_map_copy_thumbnail_and_project_in_every_orientation() {
    for turn in 0..4 {
        for horizontal in [false, true] {
            for vertical in [false, true] {
                let mut d = document();
                d.annotations.push(crate::markup_tests::object(
                    AnnotationKind::Rectangle,
                    [2., 3.],
                    [8., 9.],
                ));
                canvas::extend_to(&mut d, region([-4., -2.], [26., 22.])).unwrap();
                d.geometry.quarter_turns = turn;
                let original = d.image.clone();
                let mut expected = render::flatten(&d).unwrap();
                let mut expected_thumbnail =
                    render::thumbnail(&d, expected.width(), expected.height()).unwrap();
                if horizontal {
                    image::imageops::flip_horizontal_in_place(&mut expected);
                    image::imageops::flip_horizontal_in_place(&mut expected_thumbnail);
                }
                if vertical {
                    image::imageops::flip_vertical_in_place(&mut expected);
                    image::imageops::flip_vertical_in_place(&mut expected_thumbnail);
                }
                d.geometry.flip_horizontal = horizontal;
                d.geometry.flip_vertical = vertical;
                let flat = render::flatten(&d).unwrap();
                assert_eq!(flat, expected);
                assert_eq!(
                    render::thumbnail(&d, flat.width(), flat.height()).unwrap(),
                    expected_thumbnail
                );
                assert_eq!(crate::image_clipboard::copy_pixels(&d, None).unwrap(), flat);
                for point in [
                    Point { x: -3., y: -1. },
                    Point { x: 5., y: 7. },
                    Point { x: 23., y: 19. },
                ] {
                    let mapped = d.geometry.map_point(point, [20, 16]);
                    assert_eq!(d.geometry.inverse_point(mapped, [20, 16]), point);
                }
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("flip.myimage");
                document::save_project(&d, &path).unwrap();
                let reopened = document::load_project(&path).unwrap();
                assert_eq!(reopened.edit_state(), d.edit_state());
                assert_eq!(reopened.image, original);
                assert_eq!(render::flatten(&reopened).unwrap(), flat);
                let mut old = std::fs::read(&path).unwrap();
                old[8..12].copy_from_slice(&14_u32.to_le_bytes());
                std::fs::write(&path, old).unwrap();
                assert!(
                    document::load_project(&path)
                        .unwrap_err()
                        .contains("version 15")
                );
            }
        }
    }
}

#[test]
fn pasted_image_stays_upright_on_flipped_rotated_canvas() {
    let source = RgbaImage::from_fn(4, 2, |x, y| Rgba([x as u8 * 50, y as u8 * 100, 80, 255]));
    for turn in 0..4 {
        for flips in 0..4 {
            let mut d = document();
            d.geometry.quarter_turns = turn;
            d.geometry.flip_horizontal = flips & 1 != 0;
            d.geometry.flip_vertical = flips & 2 != 0;
            let (stamp, rect) = crate::image_clipboard::pasted_raster(source.clone(), &d).unwrap();
            let mut a = crate::markup_tests::object(
                AnnotationKind::Stamp,
                [rect.left(), rect.top()],
                [rect.right(), rect.bottom()],
            );
            a.color = [255; 4];
            a.stamp = Some(stamp);
            d.annotations.push(a);
            let flat = render::flatten(&d).unwrap();
            assert_eq!(
                image::imageops::crop_imm(&flat, flat.width() / 2 - 2, flat.height() / 2 - 1, 4, 2)
                    .to_image(),
                source
            );
        }
    }
}
