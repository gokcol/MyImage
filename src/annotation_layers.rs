//! Layer controls share the document's annotation stack and undo history.
use super::*;
use crate::document::LayerSettings;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum LayerTarget {
    Annotation(usize),
    Group(u64),
}

pub(super) struct RenameDraft {
    target: LayerTarget,
    name: String,
    focus: bool,
}

pub(super) fn editable(document: &Document, indices: &[usize]) -> bool {
    !indices.is_empty()
        && indices
            .iter()
            .all(|&i| document.annotations.get(i).is_some_and(|a| !a.locked()))
}

impl LayerTarget {
    fn indices(self, document: &Document) -> Vec<usize> {
        match self {
            Self::Annotation(i) => (i < document.annotations.len())
                .then_some(i)
                .into_iter()
                .collect(),
            Self::Group(id) => document
                .annotations
                .iter()
                .enumerate()
                .filter_map(|(i, a)| (a.group_id == Some(id)).then_some(i))
                .collect(),
        }
    }

    fn settings(self, document: &Document) -> Option<&LayerSettings> {
        let first = *self.indices(document).first()?;
        let a = &document.annotations[first];
        Some(match self {
            Self::Annotation(_) => &a.layer,
            Self::Group(_) => &a.group_layer,
        })
    }

    fn label(self, document: &Document) -> String {
        if let Some(settings) = self.settings(document)
            && !settings.name.is_empty()
        {
            return settings.name.clone();
        }
        match self {
            Self::Group(id) => format!("Group {id}"),
            Self::Annotation(i) => {
                let a = &document.annotations[i];
                let kind = match a.kind {
                    AnnotationKind::RoundedRectangle
                    | AnnotationKind::Triangle
                    | AnnotationKind::Diamond
                    | AnnotationKind::Pentagon
                    | AnnotationKind::Hexagon
                    | AnnotationKind::Star
                    | AnnotationKind::Cross
                    | AnnotationKind::Parallelogram
                    | AnnotationKind::Cylinder
                    | AnnotationKind::Prism
                    | AnnotationKind::Cone
                    | AnnotationKind::Sphere
                    | AnnotationKind::HexagonalPrism
                    | AnnotationKind::Torus
                    | AnnotationKind::Tetrahedron => crate::shapes::name(a.kind),
                    AnnotationKind::Text => "Text",
                    AnnotationKind::Stamp => "Stamp",
                    AnnotationKind::Fill => "Fill",
                    AnnotationKind::MoveArea => "Moved area",
                    AnnotationKind::DeleteArea => "Deleted area",
                    AnnotationKind::Arrow => "Arrow",
                    AnnotationKind::Rectangle => "Rectangle",
                    AnnotationKind::Ellipse => "Ellipse",
                    AnnotationKind::Magnify => "Magnify",
                    AnnotationKind::Spotlight => "Spotlight",
                    AnnotationKind::Simplify => "Simplify",
                    AnnotationKind::Blur => "Blur",
                    AnnotationKind::Pixelate => "Pixelate",
                    AnnotationKind::Step => "Step",
                    AnnotationKind::Callout => "Callout",
                    AnnotationKind::Highlighter => "Highlight",
                    AnnotationKind::Freehand => "Pen stroke",
                };
                format!("{kind} {}", i + 1)
            }
        }
    }
}

impl EditorState {
    pub(super) fn can_edit_selection(&self, document: &Document) -> bool {
        editable(document, &self.selected_indices(document))
    }

    fn selected_layer(&self, document: &Document) -> Option<LayerTarget> {
        let indices = self.selected_indices(document);
        if let Some(target) = self.layer_target
            && target.indices(document) == indices
        {
            return Some(target);
        }
        let first = *indices.first()?;
        if let Some(id) = document.annotations[first].group_id {
            let target = LayerTarget::Group(id);
            if target.indices(document) == indices {
                return Some(target);
            }
        }
        (indices.len() == 1).then_some(LayerTarget::Annotation(first))
    }

    fn change_layer(
        &mut self,
        target: LayerTarget,
        settings: LayerSettings,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let before = document.edit_state();
        for i in target.indices(document) {
            match target {
                LayerTarget::Annotation(_) => document.annotations[i].layer = settings.clone(),
                LayerTarget::Group(_) => document.annotations[i].group_layer = settings.clone(),
            }
        }
        if let Err(error) = crate::document::validate_document(document) {
            document.annotations = before.annotations;
            self.image_error = Some(error);
            return;
        }
        history.commit(before, document);
    }

    pub(super) fn layers_panel(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
        ui.spacing_mut().button_padding = Vec2::new(8.0, 6.0);
        ui.horizontal(|ui| {
            ui.strong("Layers");
            ui.weak(format!("{} annotations", document.annotations.len()));
        });
        ui.weak("Top layers appear in front. Select a group or one of its members.");
        ui.add_space(8.0);
        if let Some(target) = self.selected_layer(document) {
            ui.horizontal(|ui| {
                if ui.button("Rename…").clicked() {
                    self.rename_layer = Some(RenameDraft {
                        target,
                        name: target.settings(document).unwrap().name.clone(),
                        focus: true,
                    });
                }
                let mut settings = target.settings(document).unwrap().clone();
                let mut visible = !settings.hidden;
                let changed = ui.checkbox(&mut visible, "Show").changed()
                    | ui.checkbox(&mut settings.locked, "Lock").changed();
                if changed {
                    settings.hidden = !visible;
                    self.change_layer(target, settings, document, history);
                }
            });
            if let LayerTarget::Annotation(i) = target {
                let a = &document.annotations[i];
                if a.group_layer.hidden {
                    ui.weak("Hidden by its group.");
                }
                if a.group_layer.locked {
                    ui.weak("Locked by its group. Unlock the group first.");
                }
            }
        } else {
            ui.weak("Select one layer or a complete group to rename, show or lock it.");
        }
        ui.add_enabled_ui(self.can_edit_selection(document), |ui| {
            ui.horizontal(|ui| {
                if ui.button("To back").clicked() {
                    self.reorder_extreme(document, history, false);
                }
                if ui.button("Lower").clicked() {
                    self.reorder(document, history, false);
                }
                if ui.button("Raise").clicked() {
                    self.reorder(document, history, true);
                }
                if ui.button("To front").clicked() {
                    self.reorder_extreme(document, history, true);
                }
            });
        });
        ui.separator();
        if document.annotations.is_empty() {
            ui.weak("Add text, shapes or a pen stroke to begin.");
        }
        let mut seen = BTreeSet::new();
        for i in (0..document.annotations.len()).rev() {
            if let Some(id) = document.annotations[i].group_id {
                if !seen.insert(id) {
                    continue;
                }
                let target = LayerTarget::Group(id);
                self.layer_row(ui, target, document);
                egui::CollapsingHeader::new(format!("{} members", target.indices(document).len()))
                    .id_salt(("layer-group", id))
                    .default_open(true)
                    .show(ui, |ui| {
                        for child in target.indices(document).into_iter().rev() {
                            self.layer_row(ui, LayerTarget::Annotation(child), document);
                        }
                    });
            } else {
                self.layer_row(ui, LayerTarget::Annotation(i), document);
            }
        }
        ui.add_space(8.0);
        ui.separator();
        self.selection_controls(ui, document, history);
        if ui
            .add_enabled(
                self.can_edit_selection(document),
                egui::Button::new("Delete selected"),
            )
            .clicked()
        {
            self.delete(document, history);
        }
        ui.weak("Hidden layers stay in the project but are excluded from exports. Locking protects artwork; names and visibility remain adjustable.");
    }

    fn layer_row(&mut self, ui: &mut egui::Ui, target: LayerTarget, document: &Document) {
        let indices = target.indices(document);
        let selection = self.selected_indices(document);
        let selected = !indices.is_empty() && indices.iter().all(|i| selection.contains(i));
        let settings = target.settings(document).unwrap();
        let label = target.label(document);
        ui.push_id(target, |ui| {
            let response = ui.add_sized(
                [ui.available_width(), 32.0],
                egui::Button::selectable(selected, &label).truncate(),
            );
            response.clone().on_hover_text(&label);
            if response.clicked() {
                self.tool = Tool::Select;
                self.choose_annotation(
                    indices.first().copied(),
                    ui.input(|i| i.modifiers.shift),
                    matches!(target, LayerTarget::Annotation(_)),
                    document,
                );
                self.layer_target = Some(target);
                self.focus_canvas = true;
            }
            let (visible, locked) = match target {
                LayerTarget::Annotation(i) => (
                    document.annotations[i].visible(),
                    document.annotations[i].locked(),
                ),
                LayerTarget::Group(_) => (!settings.hidden, settings.locked),
            };
            let order = match target {
                LayerTarget::Annotation(i) => format!("#{}", i + 1),
                LayerTarget::Group(_) => "Group".into(),
            };
            ui.small(format!(
                "{order}  ·  {}  ·  {}",
                if visible { "Visible" } else { "Hidden" },
                if locked { "Locked" } else { "Unlocked" }
            ));
            ui.add_space(4.0);
        });
    }

    fn reorder_extreme(&mut self, document: &mut Document, history: &mut EditHistory, front: bool) {
        let indices = self.selected_indices(document);
        if !editable(document, &indices) {
            return;
        }
        let before = document.edit_state();
        let (selected, rest): (Vec<_>, Vec<_>) = document
            .annotations
            .iter()
            .cloned()
            .enumerate()
            .partition(|(i, _)| indices.binary_search(i).is_ok());
        let start = if front { rest.len() } else { 0 };
        let count = selected.len();
        document.annotations = if front {
            rest.into_iter().chain(selected)
        } else {
            selected.into_iter().chain(rest)
        }
        .map(|(_, a)| a)
        .collect();
        self.set_selection((start..start + count).collect(), document);
        history.commit(before, document);
        self.focus_canvas = true;
    }

    pub(super) fn layer_rename_dialog(
        &mut self,
        ctx: &egui::Context,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let Some(draft) = &mut self.rename_layer else {
            return;
        };
        let mut apply = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("rename-layer")).show(ctx, |ui| {
            ui.set_width(330.0);
            ui.heading("Rename layer");
            ui.add_space(10.0);
            let response = ui.add(
                egui::TextEdit::singleline(&mut draft.name)
                    .char_limit(64)
                    .desired_width(f32::INFINITY)
                    .hint_text("Layer name"),
            );
            if draft.focus {
                response.request_focus();
                draft.focus = false;
            }
            ui.weak("Leave blank to restore the automatic name.");
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                cancel = ui.button("Cancel").clicked();
                apply = ui.button("Rename").clicked();
            });
            apply |= ui.input(|i| i.key_pressed(Key::Enter));
        });
        if apply {
            let draft = self.rename_layer.take().unwrap();
            if let Some(mut settings) = draft.target.settings(document).cloned() {
                settings.name = draft.name.trim().to_owned();
                self.change_layer(draft.target, settings, document, history);
            }
        } else if cancel || modal.should_close() {
            self.rename_layer = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{document, export, markup_tests::object};
    use std::sync::Arc;

    fn fixture() -> Document {
        let mut d = Document {
            image: Arc::new(image::RgbaImage::from_pixel(
                800,
                600,
                image::Rgba([246, 248, 251, 255]),
            )),
            geometry: Default::default(),
            annotations: vec![
                object(AnnotationKind::Rectangle, [60., 90.], [160., 160.]),
                object(AnnotationKind::Ellipse, [260., 180.], [400., 280.]),
                object(AnnotationKind::Arrow, [610., 350.], [730., 440.]),
            ],
        };
        d.annotations[0].group_id = Some(1);
        d.annotations[1].group_id = Some(1);
        for (i, a) in d.annotations.iter_mut().enumerate() {
            a.layer.name = format!("Artwork {}", i + 1);
            a.color = [[0, 125, 137, 255], [230, 66, 55, 255], [62, 100, 220, 255]][i];
        }
        d
    }

    #[test]
    fn group_settings_preserve_member_flags_and_undo_atomically() {
        let mut d = fixture();
        d.annotations[0].layer.hidden = true;
        d.annotations[0].layer.locked = true;
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState::default();
        let target = LayerTarget::Group(1);
        let mut settings = LayerSettings {
            name: "Review notes".into(),
            hidden: true,
            locked: true,
        };
        e.change_layer(target, settings.clone(), &mut d, &mut h);
        assert!(
            d.annotations[..2]
                .iter()
                .all(|a| !a.visible() && a.locked())
        );
        assert_eq!(d.annotations[0].group_layer, d.annotations[1].group_layer);
        settings.hidden = false;
        settings.locked = false;
        e.change_layer(target, settings, &mut d, &mut h);
        assert!(!d.annotations[0].visible());
        assert!(d.annotations[0].locked());
        assert!(d.annotations[1].visible());
        assert!(!d.annotations[1].locked());
        assert!(h.undo(&mut d));
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), before);
        assert!(h.redo(&mut d));
        assert_eq!(d.annotations[0].group_layer.name, "Review notes");
    }

    #[test]
    fn hidden_layers_and_effects_are_absent_from_every_output() {
        let mut d = fixture();
        for kind in [
            AnnotationKind::Text,
            AnnotationKind::Blur,
            AnnotationKind::Pixelate,
            AnnotationKind::Freehand,
            AnnotationKind::Callout,
            AnnotationKind::Step,
            AnnotationKind::Highlighter,
        ] {
            let mut a = object(kind, [100., 100.], [400., 300.]);
            if kind == AnnotationKind::Freehand {
                a.points = vec![a.start, a.end];
            }
            if kind == AnnotationKind::Text {
                a.text = "Hidden text".into();
            }
            a.layer.hidden = true;
            d.annotations.push(a);
        }
        for a in &mut d.annotations[..2] {
            a.group_layer.hidden = true;
        }
        let mut visible = d.clone();
        visible.annotations.retain(Annotation::visible);
        assert_eq!(
            render::flatten(&d).unwrap(),
            render::flatten(&visible).unwrap()
        );
        assert_eq!(
            render::thumbnail(&d, 200, 150).unwrap(),
            render::thumbnail(&visible, 200, 150).unwrap()
        );
        assert_eq!(
            render::privacy_preview(&d, 200).unwrap(),
            render::privacy_preview(&visible, 200).unwrap()
        );
        let dir = tempfile::tempdir().unwrap();
        for format in export::ExportFormat::ALL {
            let a = dir.path().join(format!("layers.{}", format.extension()));
            let b = dir.path().join(format!("visible.{}", format.extension()));
            let options = export::ExportOptions {
                format,
                ..Default::default()
            };
            export::save(&d, &a, options).unwrap();
            export::save(&visible, &b, options).unwrap();
            assert_eq!(std::fs::read(a).unwrap(), std::fs::read(b).unwrap());
        }
        let path = dir.path().join("layers.myimage");
        document::save_project(&d, &path).unwrap();
        let loaded = document::load_project(&path).unwrap();
        assert_eq!(loaded.edit_state(), d.edit_state());
        assert_eq!(loaded.image, d.image);
    }

    #[test]
    fn locked_selection_rejects_geometry_style_delete_layout_and_group_changes() {
        let mut d = fixture();
        d.annotations[0].layer.locked = true;
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState::default();
        e.set_selection(vec![0, 1, 2], &d);
        e.delete(&mut d, &mut h);
        e.reorder(&mut d, &mut h, true);
        e.reorder_extreme(&mut d, &mut h, false);
        e.group_selection(&mut d, &mut h);
        e.ungroup_selection(&mut d, &mut h);
        e.apply_layout(
            annotation_layout::LayoutAction::Align { axis: 0, anchor: 0 },
            &mut d,
            &mut h,
        );
        for kind in [
            GestureKind::Move(0),
            GestureKind::Resize(0, Handle::BottomRight),
            GestureKind::MoveMany(vec![0, 1]),
            GestureKind::ResizeMany(vec![0, 1], Handle::BottomRight),
        ] {
            e.gesture = Some(Gesture {
                kind,
                before: before.clone(),
                origin: Point { x: 0., y: 0. },
            });
            e.update_gesture(Point { x: 300., y: 300. }, &mut d);
            e.update_snapped_gesture(Point { x: 350., y: 350. }, &mut d, false);
            assert_eq!(d.edit_state(), before);
        }
        e.gesture = None;
        e.select(Some(0), &d);
        e.color = [1; 4];
        e.apply_inspector(&mut d, &mut h);
        assert_eq!(d.edit_state(), before);
        assert!(!h.undo(&mut d));
        assert_eq!(
            hit_document_annotation(&d, Point { x: 100., y: 120. }, 1.),
            None
        );
        // Clicking an unlocked member selects its group without starting a destructive gesture.
        e.clear_selection();
        e.begin_selection_gesture(Point { x: 330., y: 230. }, 1., &mut d, false, false);
        assert!(e.gesture.is_none());
        assert_eq!(e.selected_indices(&d), vec![0, 1]);
    }

    #[test]
    fn eraser_preserves_hidden_and_locked_strokes_and_their_metadata() {
        let mut strokes = Vec::new();
        for i in 0..3 {
            let mut a = object(AnnotationKind::Freehand, [0., 50.], [100., 50.]);
            a.points = vec![a.start, a.end];
            a.layer.name = format!("Stroke {i}");
            a.layer.hidden = i == 0;
            a.layer.locked = i == 1;
            strokes.push(a);
        }
        let result = freehand::erase(
            &strokes,
            Point { x: 50., y: 0. },
            Point { x: 50., y: 100. },
            10.,
        )
        .unwrap()
        .unwrap();
        assert_eq!(&result[..2], &strokes[..2]);
        assert_eq!(result.len(), 4);
        assert!(result[2..].iter().all(|a| a.layer.name == "Stroke 2"));
    }

    #[test]
    fn ungroup_and_duplicate_keep_visibility_and_valid_group_metadata() {
        let mut d = fixture();
        for a in &mut d.annotations[..2] {
            a.group_layer.hidden = true;
            a.group_layer.name = "Hidden group".into();
        }
        let mut h = EditHistory::new(d.edit_state());
        let mut e = EditorState::default();
        e.set_selection(vec![0, 1], &d);
        e.duplicate_selection(&mut d, &mut h);
        assert_ne!(d.annotations[3].group_id, d.annotations[0].group_id);
        assert_eq!(d.annotations[3].group_layer, d.annotations[0].group_layer);
        e.select(Some(0), &d);
        e.duplicate_selection(&mut d, &mut h);
        assert!(d.annotations[5].layer.hidden);
        assert!(d.annotations[5].group_id.is_none());
        assert_eq!(d.annotations[5].group_layer, LayerSettings::default());
        e.select(Some(0), &d);
        e.ungroup_selection(&mut d, &mut h);
        assert!(
            d.annotations[..2]
                .iter()
                .all(|a| a.group_id.is_none() && a.layer.hidden)
        );
        document::validate_document(&d).unwrap();
    }

    #[test]
    fn stack_reordering_keeps_group_members_editable_and_undo_is_one_step() {
        let mut d = fixture();
        let original = d.edit_state();
        let mut h = EditHistory::new(original.clone());
        let mut e = EditorState::default();
        e.set_selection(vec![0, 1], &d);
        e.reorder_extreme(&mut d, &mut h, true);
        assert_eq!(d.annotations[0].layer.name, "Artwork 3");
        assert_eq!(e.selected_indices(&d), vec![1, 2]);
        assert!(d.annotations[1..].iter().all(|a| a.group_id == Some(1)));
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), original);
        assert!(!h.undo(&mut d));
        e.set_selection(vec![2], &d);
        e.reorder_extreme(&mut d, &mut h, false);
        assert_eq!(d.annotations[0].layer.name, "Artwork 3");
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), original);
    }

    #[test]
    fn invalid_rename_is_atomic_and_does_not_add_undo() {
        let mut d = fixture();
        let original = d.edit_state();
        let mut h = EditHistory::new(original.clone());
        let mut e = EditorState::default();
        for name in ["invalid\nname".into(), "x".repeat(257)] {
            e.change_layer(
                LayerTarget::Group(1),
                LayerSettings {
                    name,
                    ..Default::default()
                },
                &mut d,
                &mut h,
            );
            assert_eq!(d.edit_state(), original);
            assert!(!h.undo(&mut d));
        }
    }

    fn ui_frame(
        e: &mut EditorState,
        d: &mut Document,
        h: &mut EditHistory,
        ctx: &egui::Context,
        texture: &egui::TextureHandle,
        input: egui::RawInput,
    ) -> egui::FullOutput {
        ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                e.ui(ui, d, h, texture, &mut Default::default());
            });
        })
    }

    fn click_label(
        label: &str,
        e: &mut EditorState,
        d: &mut Document,
        h: &mut EditHistory,
        ctx: &egui::Context,
        texture: &egui::TextureHandle,
    ) {
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 1000.0))),
            ..Default::default()
        };
        let frame = ui_frame(e, d, h, ctx, texture, input());
        let at = frame
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == label => {
                    assert!(
                        shape
                            .clip_rect
                            .contains_rect(Rect::from_min_size(text.pos, text.galley.size())),
                        "Clipped control: {label}"
                    );
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("Missing visible control: {label}"));
        for pressed in [true, false] {
            let mut input = input();
            input.events = vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ];
            ui_frame(e, d, h, ctx, texture, input);
        }
    }

    #[test]
    fn actual_layers_controls_toggle_rename_reorder_and_preserve_undo() {
        let ctx = egui::Context::default();
        crate::appearance::apply(&ctx);
        let texture = ctx.load_texture(
            "layers UI",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut d = fixture();
        let original = d.edit_state();
        let mut h = EditHistory::new(original.clone());
        let mut e = EditorState::default();
        click_label("Layers", &mut e, &mut d, &mut h, &ctx, &texture);
        assert!(e.layers_open);
        click_label("Group 1", &mut e, &mut d, &mut h, &ctx, &texture);
        assert_eq!(e.selected_indices(&d), vec![0, 1]);
        click_label("Show", &mut e, &mut d, &mut h, &ctx, &texture);
        assert!(
            d.annotations[..2].iter().all(|a| !a.visible()),
            "selection={:?} error={:?} layers={:?}",
            e.selected_indices(&d),
            e.image_error,
            d.annotations
                .iter()
                .map(|a| (&a.layer, &a.group_layer))
                .collect::<Vec<_>>()
        );
        click_label("Lock", &mut e, &mut d, &mut h, &ctx, &texture);
        assert!(d.annotations[..2].iter().all(Annotation::locked));
        click_label("Rename…", &mut e, &mut d, &mut h, &ctx, &texture);
        assert!(e.has_modal());
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280., 1000.))),
            events: vec![egui::Event::Text("Important notes".into())],
            ..Default::default()
        };
        ui_frame(&mut e, &mut d, &mut h, &ctx, &texture, input);
        click_label("Rename", &mut e, &mut d, &mut h, &ctx, &texture);
        assert!(!e.has_modal());
        assert_eq!(d.annotations[0].group_layer.name, "Important notes");
        click_label("Lock", &mut e, &mut d, &mut h, &ctx, &texture);
        click_label("To front", &mut e, &mut d, &mut h, &ctx, &texture);
        assert_eq!(d.annotations[0].layer.name, "Artwork 3");
        for _ in 0..5 {
            assert!(h.undo(&mut d));
        }
        assert_eq!(d.edit_state(), original);
        assert!(!h.undo(&mut d));
    }

    #[test]
    #[ignore = "manual native layer review fixture"]
    fn layers_review_fixture() {
        let dir = std::path::PathBuf::from(std::env::var("MYIMAGE_LAYERS_REVIEW_DIR").unwrap());
        std::fs::create_dir_all(&dir).unwrap();
        let mut d = fixture();
        for a in &mut d.annotations[..2] {
            a.group_layer.name = "Design notes".into();
        }
        document::save_project(&d, &dir.join("layers-review.myimage")).unwrap();
    }
}
