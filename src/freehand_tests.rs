//! End-to-end persistence and export checks for editable pen paths.

use crate::{
    document::{self, Annotation, AnnotationKind, Document, Point},
    export::{self, ExportFormat, ExportOptions},
    geometry::CropRect,
    library,
    markup_tests::object,
    render,
};
use image::{Rgba, RgbaImage};
use std::{fs, io::Read, path::PathBuf, sync::Arc};

fn pen(points: &[[f32; 2]], width: f32, color: [u8; 4]) -> Annotation {
    let mut annotation = object(AnnotationKind::Freehand, points[0], *points.last().unwrap());
    annotation.points = points.iter().map(|p| Point { x: p[0], y: p[1] }).collect();
    annotation.stroke_width = width;
    annotation.color = color;
    annotation.fill = false;
    annotation
}

fn review_document() -> Document {
    let mut document = Document {
        image: Arc::new(RgbaImage::from_pixel(960, 640, Rgba([246, 248, 251, 255]))),
        annotations: Vec::new(),
        geometry: Default::default(),
    };
    for (y, size, content) in [
        (40.0, 40.0, "Make your mark"),
        (
            96.0,
            22.0,
            "Draw freely. Refine the color. Keep every stroke editable.",
        ),
        (198.0, 22.0, "Round tips and smooth turns"),
        (
            390.0,
            22.0,
            "Erase a section, then edit each remaining piece",
        ),
        (
            574.0,
            20.0,
            "Original pixels stay intact in your .myimage project.",
        ),
    ] {
        let mut label = object(AnnotationKind::Text, [48.0, y], [48.0, y]);
        label.text = content.into();
        label.font_size = size;
        label.color = [29, 38, 52, 255];
        document.annotations.push(label);
    }
    document.image = Arc::new(render::flatten(&document).unwrap());
    document.annotations.clear();
    let curve: Vec<_> = (0..=160)
        .map(|i| {
            let x = i as f32;
            [65.0 + x * 4.8, 292.0 + (x * 0.065).sin() * 38.0]
        })
        .collect();
    document
        .annotations
        .push(pen(&curve, 13.0, [0, 126, 137, 255]));
    document
        .annotations
        .push(pen(&[[875.0, 290.0]], 32.0, [230, 66, 55, 255]));
    document.annotations.push(pen(
        &[
            [65.0, 485.0],
            [175.0, 450.0],
            [290.0, 490.0],
            [390.0, 465.0],
        ],
        18.0,
        [93, 84, 204, 255],
    ));
    document.annotations.push(pen(
        &[
            [510.0, 460.0],
            [635.0, 490.0],
            [750.0, 445.0],
            [875.0, 485.0],
        ],
        18.0,
        [93, 84, 204, 255],
    ));
    document
}

#[test]
fn pen_paths_survive_exports_geometry_privacy_and_project_reediting() {
    let directory = tempfile::tempdir().unwrap();
    let mut document = review_document();
    let mut effect = object(AnnotationKind::Pixelate, [300.0, 240.0], [420.0, 350.0]);
    effect.effect_size = 20;
    document.annotations.insert(1, effect);
    document.geometry.crop = Some(CropRect {
        x: 35,
        y: 180,
        width: 890,
        height: 350,
    });
    document.geometry.quarter_turns = 3;
    document.geometry.resize = Some([280, 712]);
    let path = directory.path().join("pen.myimage");
    document::save_project(&document, &path).unwrap();
    let saved_bytes = fs::read(&path).unwrap();
    let saved_state = document.edit_state();
    let original = Arc::clone(&document.image);
    let expected = render::flatten(&document).unwrap();
    assert_eq!(expected.dimensions(), (280, 712));
    for format in ExportFormat::ALL {
        let output = directory.path().join(format!("pen.{}", format.extension()));
        export::save(
            &document,
            &output,
            ExportOptions {
                format,
                jpeg_quality: 95,
                ..Default::default()
            },
        )
        .unwrap();
        if format == ExportFormat::Pdf {
            let bytes = fs::read(output).unwrap();
            let start = bytes
                .windows(7)
                .position(|chunk| chunk == b"stream\n")
                .unwrap()
                + 7;
            let mut rgb = Vec::new();
            flate2::read::ZlibDecoder::new(&bytes[start..])
                .read_to_end(&mut rgb)
                .unwrap();
            assert_eq!(
                rgb,
                expected
                    .pixels()
                    .flat_map(|pixel| pixel.0[..3].to_vec())
                    .collect::<Vec<_>>()
            );
        } else {
            let pixels = image::open(output).unwrap().into_rgba8();
            assert_eq!(pixels.dimensions(), expected.dimensions());
            if format == ExportFormat::Jpeg {
                let error: u64 = pixels
                    .pixels()
                    .zip(expected.pixels())
                    .map(|(a, b)| {
                        (0..3)
                            .map(|channel| u64::from(a[channel].abs_diff(b[channel])))
                            .sum::<u64>()
                    })
                    .sum();
                assert!(error / (u64::from(pixels.width()) * u64::from(pixels.height()) * 3) < 8);
            } else {
                assert_eq!(pixels, expected);
            }
        }
        assert_eq!(fs::read(&path).unwrap(), saved_bytes);
        assert_eq!(document.edit_state(), saved_state);
        assert!(Arc::ptr_eq(&document.image, &original));
    }
    let entries = library::scan(directory.path()).unwrap();
    let thumbnail = library::thumbnail(
        entries
            .entries
            .iter()
            .find(|entry| entry.path == path)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(thumbnail.original_size, [280, 712]);
    assert!(thumbnail.image.width() <= 384 && thumbnail.image.height() <= 256);
    let mut reopened = document::load_project(&path).unwrap();
    assert_eq!(reopened.edit_state(), saved_state);
    assert_eq!(*reopened.image, *original);
    reopened.annotations[0].stroke_width = 24.0;
    reopened.annotations[0].color = [230, 66, 55, 255];
    reopened.annotations[0].points[20].y += 30.0;
    assert_ne!(render::flatten(&reopened).unwrap(), expected);
    document::save_project(&reopened, &path).unwrap();
    assert_eq!(
        document::load_project(&path).unwrap().edit_state(),
        reopened.edit_state()
    );
}

#[test]
#[ignore = "manual freehand image/PDF visual review fixture"]
fn freehand_review_fixture() {
    let directory = std::env::var_os("MYIMAGE_FREEHAND_REVIEW_DIR")
        .map(PathBuf::from)
        .expect("set MYIMAGE_FREEHAND_REVIEW_DIR to the review artifact directory");
    fs::create_dir_all(&directory).unwrap();
    let document = review_document();
    document::save_project(&document, &directory.join("freehand-review.myimage")).unwrap();
    document
        .image
        .save(directory.join("freehand-original.png"))
        .unwrap();
    for format in ExportFormat::ALL {
        export::save(
            &document,
            &directory.join(format!("freehand-review.{}", format.extension())),
            ExportOptions {
                format,
                ..Default::default()
            },
        )
        .unwrap();
    }
    render::thumbnail(&document, 384, 256)
        .unwrap()
        .save(directory.join("freehand-thumbnail.png"))
        .unwrap();
    render::privacy_preview(&document, 800)
        .unwrap()
        .save(directory.join("freehand-preview.png"))
        .unwrap();
}
