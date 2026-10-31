use super::*;
use crate::appearance::Icon as DialogIcon;
use crate::dialogs::{self, Dialog};

pub(super) struct CanvasDraft {
    width: i64,
    height: i64,
    base: [u32; 2],
    relative: bool,
    anchor: usize,
}

impl CanvasDraft {
    fn new(base: [u32; 2]) -> Self {
        Self {
            width: i64::from(base[0]),
            height: i64::from(base[1]),
            base,
            relative: false,
            anchor: 4,
        }
    }

    fn set_relative(&mut self, relative: bool) {
        if self.relative == relative {
            return;
        }
        for (value, base) in [&mut self.width, &mut self.height]
            .into_iter()
            .zip(self.base)
        {
            *value = if relative {
                value.saturating_sub(i64::from(base))
            } else {
                value.saturating_add(i64::from(base))
            };
        }
        self.relative = relative;
    }

    fn target_size(&self) -> Result<[u32; 2], String> {
        let mut size = [0; 2];
        for (axis, value) in [self.width, self.height].into_iter().enumerate() {
            let pixels = if self.relative {
                value.checked_add(i64::from(self.base[axis]))
            } else {
                Some(value)
            }
            .filter(|value| (1..=i64::from(crate::document::MAX_IMAGE_SIDE)).contains(value))
            .ok_or_else(|| {
                format!(
                    "Canvas dimensions must be between 1 and {} pixels.",
                    crate::document::MAX_IMAGE_SIDE
                )
            })?;
            size[axis] = pixels as u32;
        }
        crate::document::validate_dimensions(size[0], size[1])?;
        Ok(size)
    }
}

const ANCHORS: [&str; 9] = [
    "Top left",
    "Top center",
    "Top right",
    "Center left",
    "Center",
    "Center right",
    "Bottom left",
    "Bottom center",
    "Bottom right",
];

fn placement_picker(ui: &mut egui::Ui, anchor: &mut usize) {
    egui::Grid::new("canvas-anchor")
        .spacing(Vec2::splat(3.))
        .show(ui, |ui| {
            for (i, label) in ANCHORS.iter().enumerate() {
                let selected = *anchor == i;
                let response =
                    ui.add_sized(Vec2::splat(30.), egui::Button::new("").selected(selected));
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::SelectableLabel,
                        ui.is_enabled(),
                        selected,
                        format!("Place image: {label}"),
                    )
                });
                let color = if selected {
                    crate::appearance::palette(ui.ctx()).accent
                } else {
                    ui.visuals().text_color()
                };
                if selected {
                    ui.painter().rect_stroke(
                        response.rect.shrink(9.),
                        1.,
                        Stroke::new(1.8, color),
                        StrokeKind::Inside,
                    );
                } else {
                    let direction = egui::vec2(
                        (*anchor % 3) as f32 - (i % 3) as f32,
                        (*anchor / 3) as f32 - (i / 3) as f32,
                    )
                    .normalized();
                    ui.painter().arrow(
                        response.rect.center() - direction * 5.,
                        direction * 10.,
                        Stroke::new(1.3, color),
                    );
                }
                if response.on_hover_text(*label).clicked() {
                    *anchor = i;
                }
                if i % 3 == 2 {
                    ui.end_row();
                }
            }
        });
}

impl EditorState {
    pub(super) fn open_canvas_size(&mut self, document: &mut Document, history: &mut EditHistory) {
        self.finish_gesture(document, history);
        self.canvas_size = Some(CanvasDraft::new(document.output_size()));
    }

    pub(super) fn canvas_size_dialog(
        &mut self,
        ctx: &egui::Context,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let Some(draft) = &mut self.canvas_size else {
            return;
        };
        let mut apply = false;
        let mut cancel = false;
        let modal = Dialog::new(
            "Resize canvas",
            "Change the canvas without scaling your artwork.",
            DialogIcon::Resize,
            360.,
        )
        .modal(ctx, egui::Id::new("Canvas size"), |ui| {
            ui.weak(format!("Current: {} × {} px", draft.base[0], draft.base[1]));
            let mut relative = draft.relative;
            if ui
                .checkbox(&mut relative, "Use relative dimensions")
                .changed()
            {
                draft.set_relative(relative);
            }
            egui::Grid::new("canvas-size-fields")
                .spacing(Vec2::new(16., 8.))
                .show(ui, |ui| {
                    let range = if draft.relative {
                        -(crate::document::MAX_IMAGE_SIDE as i64)
                            ..=crate::document::MAX_IMAGE_SIDE as i64
                    } else {
                        1..=crate::document::MAX_IMAGE_SIDE as i64
                    };
                    for (name, value) in
                        [("Width", &mut draft.width), ("Height", &mut draft.height)]
                    {
                        ui.label(name);
                        let field = ui.add(
                            egui::DragValue::new(value)
                                .range(range.clone())
                                .speed(1)
                                .suffix(" px"),
                        );
                        field.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::DragValue,
                                ui.is_enabled(),
                                format!("Canvas {name}"),
                            )
                        });
                        ui.end_row();
                    }
                });
            if draft.relative {
                ui.small("Positive values add space; negative values trim it.");
            }
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label("Placement");
                    ui.small(ANCHORS[draft.anchor]);
                });
                placement_picker(ui, &mut draft.anchor);
            });
            let validation = draft.target_size().and_then(|size| {
                let mut candidate = document.clone();
                resize_canvas(&mut candidate, size, draft.anchor)?;
                Ok(size)
            });
            match &validation {
                Ok(size) => {
                    ui.strong(format!("New canvas: {} × {} px", size[0], size[1]));
                }
                Err(error) => {
                    dialogs::notice(ui, DialogIcon::Info, "Check dimensions", error, true);
                }
            }
            ui.small("New space is transparent. Hidden content remains in your project.");
            dialogs::actions(ui, |ui| {
                cancel = dialogs::secondary(ui, "Cancel", DialogIcon::Close).clicked();
                apply = dialogs::button(
                    ui,
                    "Resize canvas",
                    DialogIcon::Resize,
                    dialogs::Action::Primary,
                    validation.is_ok(),
                )
                .clicked();
            });
        });
        if cancel || modal.should_close() {
            self.canvas_size = None;
            return;
        }
        if apply {
            let before = document.edit_state();
            let result = draft
                .target_size()
                .and_then(|size| resize_canvas(document, size, draft.anchor));
            match result {
                Ok(_) => {
                    history.commit(before, document);
                    // Changing the canvas does not change the user's view mode.
                    self.canvas_size = None;
                    self.focus_canvas = true;
                    self.image_error = None;
                    self.confirm_resize(document);
                }
                Err(error) => self.image_error = Some(error),
            }
        }
    }
}

fn resize_canvas(document: &mut Document, size: [u32; 2], anchor: usize) -> Result<bool, String> {
    crate::document::validate_dimensions(size[0], size[1])?;
    if anchor >= ANCHORS.len() {
        return Err("Choose a valid canvas placement.".into());
    }
    let current = document.output_size();
    if size == current {
        return Ok(false);
    }
    let left = (size[0] as f32 - current[0] as f32) * (anchor % 3) as f32 * 0.5;
    let top = (size[1] as f32 - current[1] as f32) * (anchor / 3) as f32 * 0.5;
    let inverse = |x, y| {
        point(
            document
                .geometry
                .unwarped()
                .inverse_point(Point { x, y }, original_size(document)),
        )
    };
    let bounds = Rect::from_two_pos(
        inverse(-left, -top),
        inverse(size[0] as f32 - left, size[1] as f32 - top),
    );
    let before = document.geometry;
    crate::canvas::set_view_rect(document, bounds)?;
    if document.output_size() != size {
        document.geometry.resize = Some(size);
    }
    Ok(before != document.geometry)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Document {
        Document {
            image: std::sync::Arc::new(image::RgbaImage::from_fn(100, 80, |x, y| {
                image::Rgba([x as u8, y as u8, 90, 255])
            })),
            annotations: vec![crate::markup_tests::object(
                AnnotationKind::Rectangle,
                [20., 20.],
                [40., 40.],
            )],
            geometry: Default::default(),
        }
    }

    #[test]
    fn relative_canvas_dimensions_toggle_without_drift_and_validate_the_final_size() {
        let mut draft = CanvasDraft::new([100, 80]);
        assert_eq!(draft.anchor, 4);
        draft.set_relative(true);
        assert_eq!([draft.width, draft.height], [0, 0]);
        draft.width = -20;
        draft.height = 30;
        assert_eq!(draft.target_size().unwrap(), [80, 110]);
        for _ in 0..12 {
            draft.set_relative(false);
            assert_eq!([draft.width, draft.height], [80, 110]);
            draft.set_relative(true);
            assert_eq!(draft.target_size().unwrap(), [80, 110]);
        }
        for invalid in [-101, -100, i64::MAX] {
            draft.width = invalid;
            assert!(draft.target_size().is_err());
        }
        draft.set_relative(false);
        draft.width = 32_768;
        draft.height = 32_768;
        assert!(
            draft.target_size().is_err(),
            "pixel budget must be validated too"
        );
    }

    #[test]
    fn all_nine_canvas_placements_keep_artwork_size_through_rotation_and_flips() {
        for turns in 0..4 {
            for flip in [false, true] {
                for anchor in 0..9 {
                    let mut document = fixture();
                    document.geometry.quarter_turns = turns;
                    document.geometry.flip_horizontal = flip;
                    let source = document.image.clone();
                    let layers = document.annotations.clone();
                    let original = crate::render::flatten(&document).unwrap();
                    let size = document.output_size();
                    resize_canvas(&mut document, [size[0] + 20, size[1] + 16], anchor).unwrap();
                    let grown = crate::render::flatten(&document).unwrap();
                    let x = 10 * (anchor as u32 % 3);
                    let y = 8 * (anchor as u32 / 3);
                    assert_eq!(
                        image::imageops::crop_imm(&grown, x, y, size[0], size[1]).to_image(),
                        original
                    );
                    assert_eq!(
                        grown.pixels().filter(|p| p[3] != 0).count(),
                        (size[0] * size[1]) as usize
                    );
                    assert!(std::sync::Arc::ptr_eq(&document.image, &source));
                    assert_eq!(document.annotations, layers);
                    resize_canvas(&mut document, size, anchor).unwrap();
                    assert_eq!(crate::render::flatten(&document).unwrap(), original);
                }
            }
        }
    }

    #[test]
    fn perspective_canvas_resize_uses_presentation_bounds_without_stretching_artwork() {
        for turns in 0..4 {
            for anchor in [0, 4, 8] {
                let mut document = fixture();
                crate::image_effects::update(
                    &mut document,
                    crate::image_effects::ImageEffects {
                        styles: crate::image_effects::EffectStyles {
                            perspective: [25, -15],
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                )
                .unwrap();
                document.geometry.quarter_turns = turns;
                document.geometry.flip_horizontal = true;
                let before = document.edit_state();
                let original = crate::render::flatten(&document).unwrap();
                let size = document.output_size();
                assert!(!resize_canvas(&mut document, size, anchor).unwrap());
                assert_eq!(
                    document.edit_state(),
                    before,
                    "unchanged dimensions must be a no-op"
                );
                resize_canvas(&mut document, [size[0] + 20, size[1] + 16], anchor).unwrap();
                let grown = crate::render::flatten(&document).unwrap();
                let x = 10 * (anchor as u32 % 3);
                let y = 8 * (anchor as u32 / 3);
                assert_eq!(
                    image::imageops::crop_imm(&grown, x, y, size[0], size[1]).to_image(),
                    original
                );
                resize_canvas(&mut document, size, anchor).unwrap();
                assert_eq!(crate::render::flatten(&document).unwrap(), original);
                let smaller = [size[0] / 2, size[1] / 2];
                resize_canvas(&mut document, smaller, anchor).unwrap();
                let x = (size[0] - smaller[0]) * (anchor as u32 % 3) / 2;
                let y = (size[1] - smaller[1]) * (anchor as u32 / 3) / 2;
                assert_eq!(
                    crate::render::flatten(&document).unwrap(),
                    image::imageops::crop_imm(&original, x, y, smaller[0], smaller[1]).to_image()
                );
                assert_eq!(document.annotations, before.annotations);
                assert_eq!(document.geometry.effects, before.geometry.effects);
            }
        }
    }

    #[test]
    fn image_units_are_relative_to_the_opening_size_and_switch_without_rounding() {
        let mut draft = ResizeDraft::new([333, 211]);
        draft.units = ResizeUnits::Percent;
        assert_eq!([draft.value(0), draft.value(1)], [100., 100.]);
        draft.set_value(0, 50.);
        assert_eq!(draft.size, [167, 106]);
        let half = draft.size;
        for _ in 0..20 {
            draft.units = ResizeUnits::Pixels;
            assert_eq!(draft.value(0), 167.);
            draft.units = ResizeUnits::Percent;
            assert_eq!(
                draft.size, half,
                "unit changes must not re-round pixel dimensions"
            );
        }
        draft.set_value(0, 100.);
        assert_eq!(
            draft.size,
            [333, 211],
            "100% refers to the dialog's opening output"
        );
        draft.locked = false;
        draft.set_value(1, 200.);
        assert_eq!(draft.size, [333, 422]);
        draft.set_value(0, f64::NAN);
        assert_eq!(draft.size, [333, 422]);
        draft.locked = true;
        draft.ratio = draft.size[0] as f64 / draft.size[1] as f64;
        draft.set_value(0, 50.);
        assert_eq!(draft.size, [167, 212]);
    }

    struct DialogWorkspace {
        ctx: egui::Context,
        editor: EditorState,
        document: Document,
        history: EditHistory,
        time: f64,
    }

    impl DialogWorkspace {
        fn new() -> Self {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            crate::appearance::apply(&ctx);
            let document = fixture();
            let history = EditHistory::new(document.edit_state());
            Self {
                ctx,
                editor: EditorState::default(),
                document,
                history,
                time: 0.,
            }
        }
        fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
            self.time += 0.03;
            self.ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900., 740.))),
                    time: Some(self.time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |_ui| {});
                    self.editor
                        .resize_dialog(ctx, &mut self.document, &mut self.history);
                    self.editor
                        .canvas_size_dialog(ctx, &mut self.document, &mut self.history);
                },
            )
        }
        fn settle(&mut self) -> egui::FullOutput {
            self.frame(vec![]);
            self.frame(vec![]);
            self.frame(vec![])
        }
        fn click(&mut self, label: &str) {
            let output = self.settle();
            let at = output
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .iter()
                .find_map(|(_, node)| {
                    (node.label() == Some(label))
                        .then(|| node.bounds())
                        .flatten()
                })
                .map(|bounds| {
                    egui::pos2(
                        ((bounds.x0 + bounds.x1) * 0.5) as f32,
                        ((bounds.y0 + bounds.y1) * 0.5) as f32,
                    )
                })
                .unwrap_or_else(|| panic!("Missing dialog control: {label}"));
            for pressed in [true, false] {
                self.frame(vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::PointerButton {
                        pos: at,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ]);
            }
            self.settle();
        }
    }

    #[test]
    fn canvas_dialog_relative_apply_is_one_undo_preserves_view_and_cancel_keeps_document() {
        for fit in [false, true] {
            let mut w = DialogWorkspace::new();
            w.editor.fit = fit;
            w.editor.zoom = 0.5;
            let before = w.document.edit_state();
            w.editor.open_canvas_size(&mut w.document, &mut w.history);
            w.click("Use relative dimensions");
            let draft = w.editor.canvas_size.as_mut().unwrap();
            assert!(draft.relative);
            draft.width = 40;
            draft.height = -20;
            w.click("Place image: Bottom right");
            assert_eq!(w.document.edit_state(), before);
            w.click("Resize canvas");
            assert_eq!(w.document.output_size(), [140, 60]);
            assert!(w.editor.canvas_size.is_none());
            assert_eq!(w.editor.fit, fit);
            assert_eq!(w.editor.zoom, 0.5);
            assert!(w.history.undo(&mut w.document));
            assert_eq!(w.document.edit_state(), before);
            assert!(!w.history.can_undo());
            assert!(w.history.redo(&mut w.document));
            w.editor.open_canvas_size(&mut w.document, &mut w.history);
            let saved = w.document.edit_state();
            w.editor.canvas_size.as_mut().unwrap().width = 240;
            w.click("Cancel");
            assert!(w.editor.canvas_size.is_none());
            assert_eq!(w.document.edit_state(), saved);
            w.editor.open_canvas_size(&mut w.document, &mut w.history);
            w.settle();
            w.frame(vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }]);
            assert!(w.editor.canvas_size.is_none());
            assert_eq!(w.document.edit_state(), saved);
        }
    }

    #[test]
    fn resize_presets_use_opening_output_and_original_remains_a_separate_reset() {
        let mut w = DialogWorkspace::new();
        w.document.image = std::sync::Arc::new(image::RgbaImage::new(400, 200));
        w.document.annotations.clear();
        w.document.geometry.resize = Some([200, 100]);
        w.history = EditHistory::new(w.document.edit_state());
        let before = w.document.edit_state();
        w.editor.open_resize(&mut w.document, &mut w.history);
        w.click("50%");
        assert_eq!(w.editor.resize.as_ref().unwrap().size, [100, 50]);
        assert_eq!(
            w.document.edit_state(),
            before,
            "a preset only changes the draft"
        );
        w.click("Set to original");
        assert_eq!(w.editor.resize.as_ref().unwrap().size, [400, 200]);
        assert_eq!(w.document.edit_state(), before);
        w.click("25%");
        assert_eq!(
            w.editor.resize.as_ref().unwrap().size,
            [50, 25],
            "presets retain the opening baseline after other draft changes"
        );
        w.click("Cancel");
        assert_eq!(w.document.edit_state(), before);
        assert!(!w.history.can_undo());
        w.editor.open_resize(&mut w.document, &mut w.history);
        w.click("50%");
        w.click("Apply resize");
        assert_eq!(w.document.output_size(), [100, 50]);
        assert!(w.history.undo(&mut w.document));
        assert_eq!(w.document.edit_state(), before);
        assert!(!w.history.can_undo());
    }

    #[test]
    fn image_percent_dialog_commits_only_on_apply_and_preserves_view_mode() {
        for fit in [false, true] {
            let mut w = DialogWorkspace::new();
            w.editor.fit = fit;
            w.editor.zoom = 0.5;
            let before = w.document.edit_state();
            w.editor.open_resize(&mut w.document, &mut w.history);
            w.click("Image units");
            w.click("Percent");
            let draft = w.editor.resize.as_mut().unwrap();
            assert_eq!(draft.units, ResizeUnits::Percent);
            draft.set_value(0, 50.);
            w.settle();
            w.click("Image units");
            w.click("Pixels");
            assert_eq!(w.editor.resize.as_ref().unwrap().size, [50, 40]);
            assert_eq!(w.document.edit_state(), before);
            w.click("Apply resize");
            assert_eq!(w.document.output_size(), [50, 40]);
            assert_eq!(w.editor.fit, fit);
            assert_eq!(w.editor.zoom, 0.5);
            assert!(w.history.undo(&mut w.document));
            assert_eq!(w.document.edit_state(), before);
            assert!(!w.history.can_undo());
            w.editor.open_resize(&mut w.document, &mut w.history);
            w.editor.resize.as_mut().unwrap().size = [40, 40];
            w.click("Cancel");
            assert!(w.editor.resize.is_none());
            assert_eq!(w.document.edit_state(), before);
        }
    }

    #[test]
    fn shrink_and_reexpand_canvas_preserves_off_canvas_pixels_layers_and_projects() {
        for turns in 0..4 {
            for anchor in [0, 4, 8] {
                let mut d = Document {
                    image: std::sync::Arc::new(image::RgbaImage::from_fn(100, 80, |x, y| {
                        image::Rgba([x as u8, y as u8, 90, 255])
                    })),
                    annotations: vec![crate::markup_tests::object(
                        AnnotationKind::Rectangle,
                        [70., 50.],
                        [90., 70.],
                    )],
                    geometry: ImageGeometry {
                        quarter_turns: turns,
                        flip_horizontal: true,
                        ..Default::default()
                    },
                };
                let size = d.output_size();
                let before = d.edit_state();
                let original = crate::render::flatten(&d).unwrap();
                let mut h = EditHistory::new(before.clone());
                resize_canvas(&mut d, [size[0] / 2, size[1] / 2], anchor).unwrap();
                h.commit(before.clone(), &d);
                assert_eq!(d.output_size(), [size[0] / 2, size[1] / 2]);
                assert_eq!(d.annotations, before.annotations);
                let flat = crate::render::flatten(&d).unwrap();
                let x = (size[0] - flat.width()) * (anchor as u32 % 3) / 2;
                let y = (size[1] - flat.height()) * (anchor as u32 / 3) / 2;
                assert_eq!(
                    flat,
                    image::imageops::crop_imm(&original, x, y, flat.width(), flat.height())
                        .to_image()
                );
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("hidden-content.myimage");
                crate::document::save_project(&d, &path).unwrap();
                let mut loaded = crate::document::load_project(&path).unwrap();
                resize_canvas(&mut loaded, size, anchor).unwrap();
                assert_eq!(crate::render::flatten(&loaded).unwrap(), original);
                assert_eq!(loaded.annotations, before.annotations);
                assert!(h.undo(&mut d));
                assert_eq!(d.edit_state(), before);
            }
        }
    }

    #[test]
    fn shrunk_canvas_keeps_moved_pixels_even_when_their_source_is_hidden() {
        let mut d = Document {
            image: std::sync::Arc::new(image::RgbaImage::from_fn(100, 80, |x, y| {
                image::Rgba([x as u8, y as u8, 80, 255])
            })),
            annotations: vec![],
            geometry: Default::default(),
        };
        let mut a = crate::markup_tests::object(AnnotationKind::MoveArea, [10., 10.], [30., 30.]);
        a.area_source = Some([Point { x: 70., y: 50. }, Point { x: 90., y: 70. }]);
        d.annotations.push(a);
        let full = crate::render::flatten(&d).unwrap();
        resize_canvas(&mut d, [50, 40], 0).unwrap();
        assert_eq!(
            crate::render::flatten(&d).unwrap(),
            image::imageops::crop_imm(&full, 0, 0, 50, 40).to_image()
        );
        crate::canvas::extend_to(
            &mut d,
            Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(100., 80.)),
        )
        .unwrap();
        assert_eq!(d.output_size(), [100, 80]);
        assert_eq!(crate::render::flatten(&d).unwrap(), full);
    }

    #[test]
    fn pasted_images_expand_at_native_size_and_drag_growth_is_one_undo() {
        let mut d = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(80, 60)),
            annotations: vec![],
            geometry: Default::default(),
        };
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState::default();
        let (source, rect) = crate::image_clipboard::pasted_raster(
            image::RgbaImage::from_pixel(120, 90, image::Rgba([255, 0, 0, 255])),
            &d,
        )
        .unwrap();
        assert_eq!(rect.size(), egui::vec2(120., 90.));
        let mut a = e.new_annotation(AnnotationKind::Stamp, from_pos(rect.min));
        a.end = from_pos(rect.max);
        a.stamp = Some(source);
        e.insert_annotations(vec![a], &mut d, &mut h);
        assert_eq!(d.output_size(), [120, 90]);
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), before);
        assert!(h.redo(&mut d));
        let pasted = d.edit_state();
        e.select(Some(0), &d);
        let origin = from_pos(crate::render::annotation_bounds(&d.annotations[0]).center());
        e.begin_selection_gesture(origin, 1., &mut d, false, false);
        e.update_snapped_gesture(
            Point {
                x: origin.x + 100.,
                y: origin.y,
            },
            &mut d,
            true,
        );
        e.finish_annotation_gesture(&mut d, &mut h);
        assert_eq!(d.output_size(), [220, 90]);
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), pasted);
    }
    #[test]
    fn whole_canvas_selection_and_flips_follow_display_axes_and_undo() {
        let mut d = Document {
            image: std::sync::Arc::new(image::RgbaImage::from_fn(20, 16, |x, y| {
                image::Rgba([x as u8, y as u8, 100, 255])
            })),
            annotations: vec![crate::markup_tests::object(
                AnnotationKind::Rectangle,
                [2., 3.],
                [8., 9.],
            )],
            geometry: Default::default(),
        };
        resize_canvas(&mut d, [40, 30], 4).unwrap();
        let mut e = EditorState::default();
        let mut h = EditHistory::new(d.edit_state());
        e.select_all_image(&mut d, &mut h);
        let rect = crate::image_clipboard::copy_bounds(&d, e.area.unwrap()).unwrap();
        assert_eq!([rect.x, rect.y, rect.width, rect.height], [0, 0, 40, 30]);
        let source = d.image.clone();
        let before = d.edit_state();
        let pixels = crate::render::flatten(&d).unwrap();
        e.flip(&mut d, &mut h, true);
        assert_eq!(
            crate::render::flatten(&d).unwrap(),
            image::imageops::flip_horizontal(&pixels)
        );
        e.rotate(&mut d, &mut h, true);
        assert_eq!(
            crate::render::flatten(&d).unwrap(),
            image::imageops::rotate90(&image::imageops::flip_horizontal(&pixels))
        );
        assert!(std::sync::Arc::ptr_eq(&d.image, &source));
        assert_eq!(d.annotations, before.annotations);
        h.undo(&mut d);
        h.undo(&mut d);
        assert_eq!(d.edit_state(), before);
    }

    #[test]
    fn manual_canvas_size_adds_transparency_without_resizing_original() {
        for anchor in [0, 4, 8] {
            let mut d = Document {
                image: std::sync::Arc::new(image::RgbaImage::from_pixel(
                    80,
                    60,
                    image::Rgba([40, 90, 120, 255]),
                )),
                annotations: vec![],
                geometry: Default::default(),
            };
            resize_canvas(&mut d, [160, 120], anchor).unwrap();
            assert_eq!(d.output_size(), [160, 120]);
            let flat = crate::render::flatten(&d).unwrap();
            assert_eq!(flat.pixels().filter(|p| p[3] > 0).count(), 80 * 60);
        }
    }
}
