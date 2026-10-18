use crate::{
    document::{self, AnnotationKind, Document},
    markup_tests::object,
    render,
    stamps::{self, StampSource},
};
use image::{Rgba, RgbaImage};
use std::sync::Arc;
fn doc(source: StampSource) -> Document {
    let mut a = object(AnnotationKind::Stamp, [20., 20.], [116., 116.]);
    a.stamp = Some(source);
    a.color = [255; 4];
    Document {
        image: Arc::new(RgbaImage::from_pixel(180, 150, Rgba([245, 245, 245, 255]))),
        annotations: vec![a],
        geometry: Default::default(),
    }
}
#[test]
fn expanded_categories_have_unique_renderable_stamps() {
    let mut ids = std::collections::HashSet::new();
    for category in stamps::CATEGORIES.iter().filter(|&&c| c != "Signatures") {
        let entries: Vec<_> = stamps::BUILTINS
            .iter()
            .filter(|s| s.category == *category)
            .collect();
        assert_eq!(entries.len(), if *category == "Business" { 40 } else { 30 });
        for entry in entries {
            assert!(ids.insert(entry.id));
            let source = StampSource::Builtin {
                id: entry.id.into(),
            };
            source.validate().unwrap();
            let image = stamps::raster(&source).unwrap();
            assert_eq!(image.dimensions(), (512, 512));
            assert!(image.pixels().any(|p| p[3] != 0));
            assert!(image.pixels().any(|p| p[3] == 0));
        }
    }
    assert_eq!(ids.len(), 160);
}
#[test]
fn signature_is_portable_without_original_file_and_supports_presets_exports() {
    let tmp = tempfile::tempdir().unwrap();
    let source_path = tmp.path().join("Test signature.png");
    RgbaImage::from_fn(80, 24, |x, y| {
        if (x / 8 + y / 4) % 2 == 0 {
            Rgba([25, 40, 100, 180])
        } else {
            Rgba([0; 4])
        }
    })
    .save(&source_path)
    .unwrap();
    let source = stamps::import(&source_path).unwrap();
    let library_path = tmp.path().join("signatures.json");
    stamps::add(&library_path, source.clone()).unwrap();
    assert_eq!(
        stamps::load(&library_path).unwrap().signatures,
        vec![source.clone()]
    );
    let d = doc(source);
    let path = tmp.path().join("portable.myimage");
    document::save_project(&d, &path).unwrap();
    std::fs::remove_file(source_path).unwrap();
    std::fs::remove_file(library_path).unwrap();
    let reopened = document::load_project(&path).unwrap();
    assert_eq!(reopened.annotations, d.annotations);
    assert_eq!(*reopened.image, *d.image);
    assert_eq!(
        render::flatten(&reopened).unwrap(),
        render::flatten(&d).unwrap()
    );
    crate::presets::change(
        &tmp.path().join("presets.json"),
        crate::presets::Change::Add {
            name: "Signature".into(),
            annotations: d.annotations.clone(),
        },
    )
    .unwrap();
    assert_eq!(
        crate::presets::load(&tmp.path().join("presets.json"))
            .unwrap()
            .entries[0]
            .annotations,
        d.annotations
    );
    for format in [
        crate::export::ExportFormat::Png,
        crate::export::ExportFormat::Jpeg,
        crate::export::ExportFormat::WebP,
        crate::export::ExportFormat::Tiff,
        crate::export::ExportFormat::Pdf,
    ] {
        let path = tmp.path().join(format!("stamp.{}", format.extension()));
        crate::export::save(
            &reopened,
            &path,
            crate::export::ExportOptions {
                format,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(std::fs::metadata(path).unwrap().len() > 100);
    }
}
#[test]
fn stamp_stack_opacity_geometry_and_visibility_are_consistent() {
    let mut d = doc(StampSource::Builtin { id: "1f600".into() });
    let original = d.image.clone();
    let flat = render::flatten(&d).unwrap();
    assert_ne!(&flat, &*original);
    assert_eq!(flat.get_pixel(20, 20), original.get_pixel(20, 20));
    d.annotations[0].layer.hidden = true;
    assert_eq!(render::flatten(&d).unwrap(), *original);
    d.annotations[0].layer.hidden = false;
    d.annotations[0].color[3] = 0;
    assert_eq!(render::flatten(&d).unwrap(), *original);
    d.annotations[0].color[3] = 255;
    for turns in 0..4 {
        d.geometry.quarter_turns = turns;
        let flat = render::flatten(&d).unwrap();
        let thumb = render::thumbnail(&d, 180, 180).unwrap();
        assert_eq!(thumb.dimensions(), flat.dimensions());
    }
    let mut blur = object(AnnotationKind::Pixelate, [20., 20.], [116., 116.]);
    blur.effect_size = 24;
    d.annotations.push(blur);
    assert_ne!(
        render::privacy_preview(&d, 180).unwrap(),
        render::thumbnail(&doc(StampSource::Builtin { id: "1f600".into() }), 180, 180).unwrap()
    );
    assert_eq!(*d.image, *original);
}
#[test]
fn stamp_validation_rejects_missing_unknown_corrupt_and_old_project_versions() {
    let mut d = doc(StampSource::Builtin { id: "2705".into() });
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("stamp.myimage");
    document::save_project(&d, &path).unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[8..12].copy_from_slice(&9u32.to_le_bytes());
    std::fs::write(&path, bytes).unwrap();
    assert!(
        document::load_project(&path)
            .unwrap_err()
            .contains("version 10")
    );
    for source in [
        None,
        Some(StampSource::Builtin {
            id: "unknown".into(),
        }),
        Some(StampSource::Image {
            name: "Signature".into(),
            png: Arc::new("invalid".into()),
        }),
    ] {
        d.annotations[0].stamp = source;
        assert!(document::validate_annotations(&d.annotations).is_err());
    }
}
#[test]
fn formatted_text_has_distinct_rendering_and_portable_properties() {
    let mut d = doc(StampSource::Builtin { id: "2705".into() });
    let mut text = object(AnnotationKind::Text, [10., 20.], [10., 20.]);
    text.text = "Hello world\nshort".into();
    text.font_size = 22.;
    d.annotations = vec![text];
    let plain = render::flatten(&d).unwrap();
    for style in [
        crate::text_style::TextStyle {
            italic: true,
            ..Default::default()
        },
        crate::text_style::TextStyle {
            underline: true,
            ..Default::default()
        },
        crate::text_style::TextStyle {
            monospace: true,
            ..Default::default()
        },
        crate::text_style::TextStyle {
            alignment: crate::text_style::Alignment::Right,
            ..Default::default()
        },
    ] {
        d.annotations[0].text_style = style;
        assert_ne!(render::flatten(&d).unwrap(), plain);
    }
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("text.myimage");
    document::save_project(&d, &path).unwrap();
    let read = document::load_project(&path).unwrap();
    assert_eq!(d.annotations, read.annotations);
    assert_eq!(
        render::flatten(&d).unwrap(),
        render::flatten(&read).unwrap()
    );
}
#[test]
#[ignore = "Creates a disposable native review fixture"]
fn stamp_review_fixture() {
    let dir = std::path::Path::new("/private/tmp/myimage-stamp-review");
    std::fs::create_dir_all(dir).unwrap();
    let mut d = Document {
        image: Arc::new(RgbaImage::from_pixel(1000, 650, Rgba([248, 250, 253, 255]))),
        annotations: vec![],
        geometry: Default::default(),
    };
    let mut title = object(AnnotationKind::Text, [64., 55.], [64., 55.]);
    title.text = "Say it with a stamp".into();
    title.font_size = 42.;
    title.color = [27, 44, 66, 255];
    d.annotations.push(title);
    let mut note = object(AnnotationKind::Text, [66., 117.], [66., 117.]);
    note.text = "160 built-in stamps. Your signatures. Editable text.".into();
    note.font_size = 22.;
    note.color = [93, 111, 132, 255];
    d.annotations.push(note);
    for (i, id) in ["2705", "1f680", "1f3c6", "1f4a1", "1f604", "1f389"]
        .iter()
        .enumerate()
    {
        let x = 70. + (i % 3) as f32 * 295.;
        let y = 220. + (i / 3) as f32 * 210.;
        let mut a = object(AnnotationKind::Stamp, [x, y], [x + 120., y + 120.]);
        a.stamp = Some(StampSource::Builtin { id: (*id).into() });
        a.color = [255; 4];
        d.annotations.push(a);
    }
    document::save_project(&d, &dir.join("stamps-and-text.myimage")).unwrap();
    render::flatten(&d)
        .unwrap()
        .save(dir.join("stamps-and-text.png"))
        .unwrap();
    RgbaImage::from_fn(240, 70, |x, y| {
        if (10..230).contains(&x) && (y as f32 - (35. + (x as f32 / 13.).sin() * 20.)).abs() < 2. {
            Rgba([30, 45, 75, 255])
        } else {
            Rgba([0; 4])
        }
    })
    .save(dir.join("Sample signature.png"))
    .unwrap();
}
