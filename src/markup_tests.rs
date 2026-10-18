//! Cross-format checks for portable, editable markup documents.

use crate::{
    document::{self, Annotation, AnnotationKind, Document, Point},
    export::{self, ExportFormat, ExportOptions},
    geometry::CropRect,
    library, render,
};
use image::{Rgba, RgbaImage};
use std::{fs, io::Read, path::PathBuf, sync::Arc};

pub(crate) fn object(kind: AnnotationKind, start: [f32; 2], end: [f32; 2]) -> Annotation {
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
        color: [0, 125, 137, 255],
        stroke_width: 3.0,
        fill: true,
        text: String::new(),
        font_size: 28.0,
        effect_size: 12,
        step_number: 1,
        tail: (kind == AnnotationKind::Callout).then_some(Point {
            x: end[0],
            y: end[1] + 40.0,
        }),
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

fn text(x: f32, y: f32, size: f32, label: &str) -> Annotation {
    let mut annotation = object(AnnotationKind::Text, [x, y], [x, y]);
    annotation.text = label.into();
    annotation.font_size = size;
    annotation.color = [29, 38, 52, 255];
    annotation
}

fn review_document() -> Document {
    let mut document = Document {
        image: Arc::new(RgbaImage::from_pixel(1000, 660, Rgba([246, 248, 251, 255]))),
        annotations: vec![
            text(48.0, 42.0, 40.0, "A clearer story, one step at a time"),
            text(
                48.0,
                103.0,
                22.0,
                "Numbered steps, callouts, and highlights stay editable.",
            ),
            text(145.0, 216.0, 28.0, "Capture a moment"),
            text(145.0, 326.0, 28.0, "Explain what matters"),
            text(
                145.0,
                436.0,
                28.0,
                "Highlight the detail and share your result",
            ),
            text(
                48.0,
                580.0,
                21.0,
                "Original pixels remain in .myimage. Export creates a finished copy.",
            ),
        ],
        geometry: Default::default(),
    };
    document.image = Arc::new(render::flatten(&document).unwrap());
    document.annotations.clear();
    for index in 0..3 {
        let y = 200.0 + index as f32 * 110.0;
        let mut step = object(AnnotationKind::Step, [56.0, y], [116.0, y + 60.0]);
        step.step_number = index + 1;
        step.color = [230, 66, 55, 255];
        document.annotations.push(step);
    }
    let mut callout = object(AnnotationKind::Callout, [565.0, 190.0], [945.0, 370.0]);
    callout.tail = Some(Point { x: 480.0, y: 340.0 });
    callout.text = "Move the pointer.\nEdit the text.\nKeep the context.".into();
    document.annotations.push(callout);
    let mut highlight = object(AnnotationKind::Highlighter, [138.0, 429.0], [852.0, 477.0]);
    highlight.color = [255, 220, 0, 90];
    document.annotations.push(highlight);
    document
}

#[test]
fn markup_exports_and_library_preserve_editable_objects_with_geometry_and_privacy() {
    let directory = tempfile::tempdir().unwrap();
    let mut document = review_document();
    let mut effect = object(AnnotationKind::Pixelate, [565.0, 210.0], [945.0, 265.0]);
    effect.effect_size = 16;
    document.annotations.push(effect);
    document.geometry.crop = Some(CropRect {
        x: 35,
        y: 160,
        width: 930,
        height: 355,
    });
    document.geometry.quarter_turns = 1;
    document.geometry.resize = Some([284, 744]);
    let project = directory.path().join("markup.myimage");
    document::save_project(&document, &project).unwrap();
    let saved_bytes = fs::read(&project).unwrap();
    let saved_state = document.edit_state();
    let original = Arc::clone(&document.image);
    let expected = render::flatten(&document).unwrap();
    assert_eq!(expected.dimensions(), (284, 744));
    for format in ExportFormat::ALL {
        let path = directory
            .path()
            .join(format!("markup.{}", format.extension()));
        export::save(
            &document,
            &path,
            ExportOptions {
                format,
                jpeg_quality: 95,
                ..Default::default()
            },
        )
        .unwrap();
        if format == ExportFormat::Pdf {
            let bytes = fs::read(path).unwrap();
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
            let output = image::open(path).unwrap().into_rgba8();
            assert_eq!(output.dimensions(), expected.dimensions());
            if format == ExportFormat::Jpeg {
                let total: u64 = output
                    .pixels()
                    .zip(expected.pixels())
                    .map(|(a, b)| (0..3).map(|i| u64::from(a[i].abs_diff(b[i]))).sum::<u64>())
                    .sum();
                assert!(total / (u64::from(output.width()) * u64::from(output.height()) * 3) < 8);
            } else {
                assert_eq!(output, expected);
            }
        }
        assert_eq!(fs::read(&project).unwrap(), saved_bytes);
        assert_eq!(document.edit_state(), saved_state);
        assert!(Arc::ptr_eq(&document.image, &original));
    }
    let index = library::scan(directory.path()).unwrap();
    let entry = index
        .entries
        .iter()
        .find(|entry| entry.path == project)
        .unwrap();
    let thumbnail = library::thumbnail(entry).unwrap();
    assert_eq!(thumbnail.original_size, [284, 744]);
    assert!(thumbnail.image.width() <= 384 && thumbnail.image.height() <= 256);
    let mut reopened = document::load_project(&project).unwrap();
    assert_eq!(reopened.edit_state(), saved_state);
    assert_eq!(*reopened.image, *original);
    reopened.annotations[0].step_number = 7;
    reopened.annotations[3].text = "Still editable".into();
    reopened.annotations[3].tail = Some(Point { x: 520.0, y: 400.0 });
    reopened.annotations[4].color[3] = 150;
    assert_ne!(render::flatten(&reopened).unwrap(), expected);
    document::save_project(&reopened, &project).unwrap();
    assert_eq!(
        document::load_project(&project).unwrap().edit_state(),
        reopened.edit_state()
    );
}

#[test]
#[ignore = "manual markup image/PDF visual review fixture"]
fn markup_review_fixture() {
    let directory = std::env::var_os("MYIMAGE_MARKUP_REVIEW_DIR")
        .map(PathBuf::from)
        .expect("set MYIMAGE_MARKUP_REVIEW_DIR to the review artifact directory");
    fs::create_dir_all(&directory).unwrap();
    let document = review_document();
    document::save_project(&document, &directory.join("markup-review.myimage")).unwrap();
    document
        .image
        .save(directory.join("markup-original.png"))
        .unwrap();
    for format in ExportFormat::ALL {
        export::save(
            &document,
            &directory.join(format!("markup-review.{}", format.extension())),
            ExportOptions {
                format,
                ..Default::default()
            },
        )
        .unwrap();
    }
    render::thumbnail(&document, 384, 256)
        .unwrap()
        .save(directory.join("markup-thumbnail.png"))
        .unwrap();
    render::privacy_preview(&document, 800)
        .unwrap()
        .save(directory.join("markup-preview.png"))
        .unwrap();
}
