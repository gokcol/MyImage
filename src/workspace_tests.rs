use crate::{document, render, stamps, svg_stamp, templates};
use std::sync::Arc;

fn svg(name: &str) -> stamps::StampSource {
    stamps::StampSource::Svg {
        name: name.into(),
        svg: Arc::new(svg_stamp::STARTER.into()),
        prompt: "A teal approval seal".into(),
    }
}

#[test]
fn svg_import_rejects_active_linked_and_unbounded_content() {
    let good = svg_stamp::raster(svg_stamp::STARTER, 128).unwrap();
    assert_eq!(good.dimensions(), (128, 128));
    assert_eq!(good.get_pixel(0, 0)[3], 0);
    assert!(good.pixels().any(|p| p[3] > 0));
    for content in [
        "<script>alert(1)</script>",
        "<image href='file:///tmp/private.png'/>",
        "<use href='#loop' id='loop'/>",
        "<foreignObject/>",
        "<rect onclick='x()'/>",
        "<rect fill='url(https://example.com/a)'/>",
        "<rect style='fill:red'/>",
        "<filter/>",
    ] {
        assert!(
            svg_stamp::tree(&format!(
                "<svg xmlns='http://www.w3.org/2000/svg'>{content}</svg>"
            ))
            .is_err(),
            "{content}"
        );
    }
    assert!(svg_stamp::tree(&format!("<svg>{}</svg>", "<g/>".repeat(5000))).is_err());
    assert!(
        svg_stamp::tree("<!DOCTYPE svg [<!ENTITY x SYSTEM 'file:///tmp/private'>]><svg>&x;</svg>")
            .is_err()
    );
    assert!(svg_stamp::tree("<svg width='9000' height='10'/>").is_err());
    let wrapped = format!("```svg\n{}\n```", svg_stamp::STARTER);
    assert_eq!(svg_stamp::clean(&wrapped), svg_stamp::STARTER);
}

#[test]
fn custom_categories_and_edited_svg_persist_without_replacing_other_collections() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stamps.json");
    std::fs::write(&path, r#"{"signatures":[]}"#).unwrap();
    stamps::create_category(&path, "Approvals").unwrap();
    assert!(stamps::create_category(&path, "approvals").is_err());
    assert!(stamps::create_category(&path, "Business").is_err());
    let original = svg("Approved");
    stamps::add_to(&path, "Approvals", original.clone()).unwrap();
    let changed = svg("Reviewed");
    stamps::put_in(&path, "Approvals", Some(&original), changed.clone()).unwrap();
    let lib = stamps::load(&path).unwrap();
    assert_eq!(lib.categories[0].stamps, vec![changed]);
    assert!(lib.signatures.is_empty());
    stamps::add(&path, original.clone()).unwrap();
    assert_eq!(stamps::load(&path).unwrap().signatures, vec![original]);
}

#[test]
fn templates_and_svg_artwork_round_trip_with_originals_and_editable_layers() {
    let dir = tempfile::tempdir().unwrap();
    for kind in templates::KINDS {
        let spec = templates::Spec {
            kind,
            title: "A visual guide".into(),
            size: [1600, 1400],
            background: [255; 4],
        };
        let doc = templates::build(&spec).unwrap();
        let path = dir.path().join(format!("{kind:?}.myimage"));
        document::save_project(&doc, &path).unwrap();
        let loaded = document::load_project(&path).unwrap();
        assert_eq!(loaded.annotations, doc.annotations);
        assert_eq!(loaded.image, doc.image);
        let preview = render::thumbnail(&loaded, 320, 240).unwrap();
        assert!(preview.width() <= 320);
        if kind != templates::Kind::Blank {
            assert!(
                loaded
                    .annotations
                    .iter()
                    .any(|a| a.kind == document::AnnotationKind::Text)
            );
            assert!(
                loaded
                    .annotations
                    .iter()
                    .any(|a| matches!(a.stamp, Some(stamps::StampSource::Svg { .. })))
            );
            let mut old = std::fs::read(&path).unwrap();
            old[8..12].copy_from_slice(&11u32.to_le_bytes());
            std::fs::write(&path, old).unwrap();
            assert!(
                document::load_project(&path)
                    .unwrap_err()
                    .contains("version 12")
            );
        }
    }
}

#[test]
fn svg_text_and_alpha_export_from_editable_source() {
    let source=stamps::StampSource::Svg{name:"Text seal".into(),svg:Arc::new("<svg xmlns='http://www.w3.org/2000/svg' width='512' height='512'><text x='20' y='260' font-size='100' font-family='sans-serif' fill='#008992'>OK</text></svg>".into()),prompt:String::new()};
    let raster = stamps::raster(&source).unwrap();
    assert!(raster.pixels().any(|p| p[3] > 0));
    let mut a =
        crate::markup_tests::object(document::AnnotationKind::Stamp, [0., 0.], [400., 400.]);
    a.color = [255; 4];
    a.stamp = Some(source.clone());
    let doc = document::Document {
        image: Arc::new(image::RgbaImage::new(400, 400)),
        annotations: vec![a],
        geometry: Default::default(),
    };
    let flat = render::flatten(&doc).unwrap();
    assert!(flat.pixels().any(|p| p[3] > 0));
    assert_eq!(flat.get_pixel(0, 0)[3], 0);
    let dir = tempfile::tempdir().unwrap();
    for format in crate::export::ExportFormat::ALL {
        let path = dir.path().join(format!("svg.{}", format.extension()));
        crate::export::save(
            &doc,
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
fn recents_are_deduplicated_bounded_and_serialized() {
    let mut settings = crate::storage::Settings::default();
    for i in 0..30 {
        crate::storage::remember_recent(&mut settings, format!("/tmp/capture-{i}.png").into());
    }
    crate::storage::remember_recent(&mut settings, "/tmp/capture-10.png".into());
    assert_eq!(settings.recent_files.len(), 24);
    assert_eq!(
        settings.recent_files[0].to_str(),
        Some("/tmp/capture-10.png")
    );
    let round: crate::storage::Settings =
        serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
    assert_eq!(round.recent_files, settings.recent_files);
    assert!(round.recents_open);
    let old: crate::storage::Settings = serde_json::from_str("{}").unwrap();
    assert!(old.recent_files.is_empty());
    assert!(old.recents_open);
}

#[test]
#[ignore = "Create isolated workspace review assets"]
fn workspace_review_fixture() {
    let dir = std::path::Path::new("/private/tmp/myimage-workspace-review");
    std::fs::create_dir_all(dir).unwrap();
    let doc = templates::build(&templates::Spec {
        kind: templates::Kind::BeforeAfter,
        title: "Before and after".into(),
        size: [1600, 1000],
        background: [255; 4],
    })
    .unwrap();
    document::save_project(&doc, &dir.join("template.myimage")).unwrap();
    render::flatten(&doc)
        .unwrap()
        .save(dir.join("template.png"))
        .unwrap();
    std::fs::write(dir.join("approval.svg"), svg_stamp::STARTER).unwrap();
}
