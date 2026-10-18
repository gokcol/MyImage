//! Groups are editable metadata; all output paths retain the same stack composition.
use crate::{
    document::{self, AnnotationKind, Document},
    export::{self, ExportFormat, ExportOptions},
    library,
    markup_tests::object,
    render,
};
use image::{Rgba, RgbaImage};
use std::{fs, path::PathBuf, sync::Arc};

fn fixture() -> Document {
    let mut d = Document {
        image: Arc::new(RgbaImage::from_pixel(960, 640, Rgba([246, 248, 251, 255]))),
        annotations: vec![],
        geometry: Default::default(),
    };
    for (y, size, text) in [
        (40.0, 40.0, "Better together"),
        (
            100.0,
            22.0,
            "Select, group, and duplicate. Every annotation stays editable.",
        ),
        (
            570.0,
            20.0,
            "Shift-click to select more. Alt/Option-click to edit one member.",
        ),
    ] {
        let mut a = object(AnnotationKind::Text, [48.0, y], [48.0, y]);
        a.text = text.into();
        a.font_size = size;
        a.color = [29, 38, 52, 255];
        d.annotations.push(a);
    }
    d.image = Arc::new(render::flatten(&d).unwrap());
    d.annotations.clear();
    let mut panel = object(AnnotationKind::Rectangle, [80.0, 210.0], [410.0, 440.0]);
    panel.color = [218, 239, 237, 255];
    panel.group_id = Some(1);
    d.annotations.push(panel);
    let mut step = object(AnnotationKind::Step, [110.0, 245.0], [166.0, 301.0]);
    step.color = [0, 125, 137, 255];
    step.group_id = Some(1);
    d.annotations.push(step);
    let mut label = object(AnnotationKind::Text, [110.0, 340.0], [110.0, 340.0]);
    label.text = "Capture a moment".into();
    label.font_size = 26.0;
    label.color = [29, 38, 52, 255];
    label.group_id = Some(1);
    d.annotations.push(label);
    let mut callout = object(AnnotationKind::Callout, [555.0, 240.0], [875.0, 425.0]);
    callout.text = "Move the whole group.\nRefine one detail.".into();
    callout.group_id = Some(2);
    d.annotations.push(callout);
    let mut arrow = object(AnnotationKind::Arrow, [465.0, 320.0], [535.0, 320.0]);
    arrow.color = [230, 66, 55, 255];
    arrow.stroke_width = 6.0;
    arrow.group_id = Some(2);
    d.annotations.push(arrow);
    d
}

#[test]
fn groups_preserve_output_in_all_formats_and_reopen_as_editable_members() {
    let directory = tempfile::tempdir().unwrap();
    let mut grouped = fixture();
    grouped.geometry.crop = Some(crate::geometry::CropRect {
        x: 50,
        y: 180,
        width: 860,
        height: 320,
    });
    grouped.geometry.quarter_turns = 1;
    grouped.geometry.resize = Some([256, 688]);
    let mut ungrouped = grouped.clone();
    for a in &mut ungrouped.annotations {
        a.group_id = None;
    }
    assert_eq!(
        render::flatten(&grouped).unwrap(),
        render::flatten(&ungrouped).unwrap()
    );
    assert_eq!(
        render::thumbnail(&grouped, 384, 256).unwrap(),
        render::thumbnail(&ungrouped, 384, 256).unwrap()
    );
    let path = directory.path().join("groups.myimage");
    document::save_project(&grouped, &path).unwrap();
    let bytes = fs::read(&path).unwrap();
    for format in ExportFormat::ALL {
        let a = directory
            .path()
            .join(format!("grouped.{}", format.extension()));
        let b = directory
            .path()
            .join(format!("ungrouped.{}", format.extension()));
        let options = ExportOptions {
            format,
            ..Default::default()
        };
        export::save(&grouped, &a, options).unwrap();
        export::save(&ungrouped, &b, options).unwrap();
        assert_eq!(fs::read(a).unwrap(), fs::read(b).unwrap());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    let scanned = library::scan(directory.path()).unwrap();
    let entry = scanned
        .entries
        .iter()
        .find(|entry| entry.path == path)
        .unwrap();
    let thumb = library::thumbnail(entry).unwrap();
    assert_eq!(thumb.original_size, [256, 688]);
    let mut loaded = document::load_project(&path).unwrap();
    assert_eq!(loaded.edit_state(), grouped.edit_state());
    assert_eq!(*loaded.image, *grouped.image);
    loaded.annotations[2].text = "Still editable".into();
    loaded.annotations[1].step_number = 9;
    document::save_project(&loaded, &path).unwrap();
    assert_eq!(
        document::load_project(&path).unwrap().edit_state(),
        loaded.edit_state()
    );
    assert_eq!(loaded.annotations[2].group_id, Some(1));
}

#[test]
#[ignore = "manual group project and output review fixture"]
fn group_review_fixture() {
    let p = std::env::var_os("MYIMAGE_GROUP_REVIEW_DIR")
        .map(PathBuf::from)
        .expect("set MYIMAGE_GROUP_REVIEW_DIR");
    fs::create_dir_all(&p).unwrap();
    let d = fixture();
    document::save_project(&d, &p.join("groups-review.myimage")).unwrap();
    d.image.save(p.join("groups-original.png")).unwrap();
    for format in ExportFormat::ALL {
        export::save(
            &d,
            &p.join(format!("groups-review.{}", format.extension())),
            ExportOptions {
                format,
                ..Default::default()
            },
        )
        .unwrap();
    }
    render::thumbnail(&d, 384, 256)
        .unwrap()
        .save(p.join("groups-thumbnail.png"))
        .unwrap();
}
