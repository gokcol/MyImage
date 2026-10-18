use crate::{
    document::{self, AnnotationKind, Document, EditHistory, Point},
    fill,
    markup_tests::object,
    render,
};
use image::{Rgba, RgbaImage};
use std::sync::Arc;
fn canvas() -> RgbaImage {
    RgbaImage::from_fn(40, 30, |x, y| {
        if x == 20
            || ((5..=15).contains(&x) && (y == 5 || y == 20))
            || ((5..=20).contains(&y) && (x == 5 || x == 15))
        {
            Rgba([0, 0, 0, 255])
        } else {
            Rgba([255; 4])
        }
    })
}
fn filled(image: RgbaImage, seed: Point, tolerance: u8, global: bool) -> Document {
    let mut a = object(
        AnnotationKind::Fill,
        [0., 0.],
        [image.width() as f32, image.height() as f32],
    );
    a.color = [25, 120, 200, 255];
    a.fill_region = Some(fill::build(&image, &a, seed, tolerance, global).unwrap());
    Document {
        image: Arc::new(image),
        annotations: vec![a],
        geometry: Default::default(),
    }
}
#[test]
fn recalculating_a_partly_off_canvas_mask_keeps_its_coordinate_frame() {
    let image = RgbaImage::from_pixel(40, 30, Rgba([255; 4]));
    let mut a = object(AnnotationKind::Fill, [-5., -5.], [15., 20.]);
    a.color = [20, 90, 170, 255];
    a.area_mask = vec![
        Point { x: 0., y: 0. },
        Point { x: 1., y: 0. },
        Point { x: 0., y: 1. },
    ];
    a.fill_region = Some(fill::build(&image, &a, Point { x: 2., y: 2. }, 0, true).unwrap());
    let region = a.fill_region.as_ref().unwrap();
    assert_eq!((region.width, region.height), (20, 25));
    let mut out = image.clone();
    fill::apply(&mut out, &a, [1.; 2]).unwrap();
    assert_eq!(out.get_pixel(2, 2), &Rgba(a.color));
    assert_eq!(out.get_pixel(12, 10), image.get_pixel(12, 10));
    assert_eq!(out.get_pixel(16, 2), image.get_pixel(16, 2));
}
#[test]
fn connected_fill_respects_solid_lines_and_four_connectivity() {
    let d = filled(canvas(), Point { x: 10., y: 10. }, 15, false);
    let out = render::flatten(&d).unwrap();
    assert_eq!(out.get_pixel(10, 10), &Rgba([25, 120, 200, 255]));
    for (x, y) in [(4, 10), (16, 10), (10, 4), (10, 21), (30, 10), (5, 10)] {
        assert_eq!(out.get_pixel(x, y), d.image.get_pixel(x, y));
    }
    let image = RgbaImage::from_fn(3, 3, |x, y| {
        if x == y {
            Rgba([255; 4])
        } else {
            Rgba([0, 0, 0, 255])
        }
    });
    let d = filled(image, Point { x: 0., y: 0. }, 0, false);
    let out = render::flatten(&d).unwrap();
    assert_eq!(out.get_pixel(1, 1), &Rgba([255; 4]));
}
#[test]
fn global_fill_and_tolerance_are_explicit_and_seed_relative() {
    let d = filled(canvas(), Point { x: 10., y: 10. }, 0, true);
    let out = render::flatten(&d).unwrap();
    assert_eq!(out.get_pixel(30, 10), &Rgba([25, 120, 200, 255]));
    assert_eq!(out.get_pixel(20, 10), &Rgba([0, 0, 0, 255]));
    let image = RgbaImage::from_fn(256, 1, |x, _| Rgba([x as u8, x as u8, x as u8, 255]));
    let d = filled(image.clone(), Point { x: 0., y: 0. }, 10, false);
    let out = render::flatten(&d).unwrap();
    assert_eq!(out.get_pixel(25, 0), &Rgba([25, 120, 200, 255]));
    assert_eq!(out.get_pixel(26, 0), image.get_pixel(26, 0));
    let d = filled(image, Point { x: 0., y: 0. }, 0, false);
    assert_eq!(
        d.annotations[0]
            .fill_region
            .as_ref()
            .unwrap()
            .runs
            .as_slice(),
        &[[0, 0, 1]]
    );
}
#[test]
fn rectangle_lasso_and_outside_clicks_constrain_even_global_fill() {
    let image = canvas();
    let mut a = object(AnnotationKind::Fill, [1., 1.], [19., 25.]);
    a.color = [200, 20, 10, 255];
    a.fill_region = Some(fill::build(&image, &a, Point { x: 2., y: 2. }, 0, true).unwrap());
    let mut out = image.clone();
    fill::apply(&mut out, &a, [1.; 2]).unwrap();
    assert_eq!(out.get_pixel(0, 0), image.get_pixel(0, 0));
    assert_eq!(out.get_pixel(30, 10), image.get_pixel(30, 10));
    a.area_mask = vec![
        Point { x: 0., y: 0. },
        Point { x: 1., y: 0. },
        Point { x: 0., y: 1. },
    ];
    assert!(fill::build(&image, &a, Point { x: 18., y: 24. }, 0, true).is_err());
    a.fill_region = Some(fill::build(&image, &a, Point { x: 2., y: 2. }, 0, true).unwrap());
    out = image.clone();
    fill::apply(&mut out, &a, [1.; 2]).unwrap();
    assert_eq!(out.get_pixel(18, 24), image.get_pixel(18, 24));
    assert_eq!(out.get_pixel(2, 2), &Rgba(a.color));
}
#[test]
fn transparent_and_partial_alpha_replace_pixels_and_hide_restores_original() {
    let mut d = filled(canvas(), Point { x: 10., y: 10. }, 15, false);
    d.annotations[0].color = [255, 0, 0, 0];
    let out = render::flatten(&d).unwrap();
    assert_eq!(out.get_pixel(10, 10), &Rgba([0; 4]));
    assert_eq!(out.get_pixel(5, 10), d.image.get_pixel(5, 10));
    d.annotations[0].color = [255, 20, 10, 128];
    assert_eq!(
        render::flatten(&d).unwrap().get_pixel(10, 10),
        &Rgba([255, 20, 10, 128])
    );
    d.annotations[0].layer.hidden = true;
    assert_eq!(render::flatten(&d).unwrap(), *d.image);
    let image = RgbaImage::from_fn(10, 10, |x, y| Rgba([x as u8, y as u8, 99, 0]));
    let d = filled(image, Point { x: 5., y: 5. }, 0, false);
    assert_eq!(
        render::flatten(&d).unwrap().get_pixel(9, 9),
        &Rgba([25, 120, 200, 255])
    );
}
#[test]
fn previews_do_not_flood_across_downsampled_one_pixel_boundaries() {
    let image = RgbaImage::from_fn(4000, 100, |x, _| {
        if x == 2001 {
            Rgba([0, 0, 0, 255])
        } else {
            Rgba([255; 4])
        }
    });
    let d = filled(image, Point { x: 100., y: 50. }, 15, false);
    let preview = render::privacy_preview(&d, 500).unwrap();
    assert_eq!(preview.get_pixel(400, 5), &Rgba([255; 4]));
    assert_eq!(preview.get_pixel(10, 5), &Rgba([25, 120, 200, 255]));
    let thumb = render::thumbnail(&d, 300, 300).unwrap();
    assert_eq!(thumb.get_pixel(260, 3), &Rgba([255; 4]));
}
#[test]
fn fill_projects_exports_geometry_and_undo_keep_original_pixels() {
    let mut d = filled(canvas(), Point { x: 10., y: 10. }, 15, false);
    let original = d.image.clone();
    let mut h = EditHistory::new(d.edit_state());
    let before = d.edit_state();
    d.annotations[0].color = [0; 4];
    h.commit(before, &d);
    h.undo(&mut d);
    assert_eq!(d.annotations[0].color[3], 255);
    h.redo(&mut d);
    assert_eq!(d.annotations[0].color[3], 0);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fill.myimage");
    document::save_project(&d, &path).unwrap();
    let read = document::load_project(&path).unwrap();
    assert_eq!(read.edit_state(), d.edit_state());
    assert_eq!(*read.image, *original);
    for format in crate::export::ExportFormat::ALL {
        let path = dir.path().join(format!("fill.{}", format.extension()));
        crate::export::save(
            &d,
            &path,
            crate::export::ExportOptions {
                format,
                ..Default::default()
            },
        )
        .unwrap();
        if matches!(
            format,
            crate::export::ExportFormat::Png
                | crate::export::ExportFormat::WebP
                | crate::export::ExportFormat::Tiff
        ) {
            assert_eq!(
                image::open(path).unwrap().to_rgba8().get_pixel(10, 10),
                &Rgba([0; 4])
            );
        }
    }
    for turns in 0..4 {
        d.geometry.quarter_turns = turns;
        let out = render::flatten(&d).unwrap();
        assert!(out.pixels().any(|p| p[3] == 0));
        assert_eq!(*d.image, *original);
    }
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[8..12].copy_from_slice(&10u32.to_le_bytes());
    std::fs::write(&path, bytes).unwrap();
    assert!(
        document::load_project(&path)
            .unwrap_err()
            .contains("version 11")
    );
}
#[test]
fn corrupt_fill_masks_are_rejected_before_use() {
    let mut d = filled(canvas(), Point { x: 10., y: 10. }, 15, false);
    for runs in [
        vec![],
        vec![[0, 0, 41]],
        vec![[30, 0, 1]],
        vec![[0, 1, 1]],
        vec![[1, 0, 2], [0, 0, 2]],
        vec![[0, 0, 2], [0, 1, 3]],
    ] {
        d.annotations[0].fill_region.as_mut().unwrap().runs = Arc::new(runs);
        assert!(document::validate_document(&d).is_err());
    }
}
#[test]
#[ignore = "Creates an isolated Fill UI review fixture"]
fn fill_review_fixture() {
    let dir = std::path::Path::new("/private/tmp/myimage-fill-review");
    std::fs::create_dir_all(dir).unwrap();
    let image = RgbaImage::from_fn(900, 560, |x, y| {
        if (((70..=400).contains(&x)) && ((100..104).contains(&y) || (420..424).contains(&y)))
            || (((100..=423).contains(&y)) && ((70..74).contains(&x) || (397..401).contains(&x)))
            || ((480..484).contains(&x))
        {
            Rgba([35, 45, 60, 255])
        } else {
            Rgba([255; 4])
        }
    });
    let mut d = Document {
        image: Arc::new(image),
        annotations: vec![],
        geometry: Default::default(),
    };
    let mut title = object(AnnotationKind::Text, [70., 35.], [70., 35.]);
    title.text = "Fill inside solid boundaries".into();
    title.font_size = 28.;
    title.color = [35, 45, 60, 255];
    d.annotations.push(title);
    document::save_project(&d, &dir.join("fill-review.myimage")).unwrap();
}
