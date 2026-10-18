//! Reusable artwork in a native panel; inserted copies are ordinary annotations.
use super::*;
use crate::presets::{self, Change, Library, Preset};
use std::path::PathBuf;

pub(super) enum PresetDialog {
    Save {
        name: String,
        annotations: Vec<Annotation>,
        focus: bool,
    },
    Rename {
        id: u64,
        name: String,
        focus: bool,
    },
    Delete {
        id: u64,
        name: String,
    },
}

pub(super) struct PresetState {
    path: Result<PathBuf, String>,
    library: Library,
    loaded: bool,
    ready: bool,
    selected: Option<u64>,
    search: String,
    error: Option<String>,
    notice: Option<String>,
    pub dialog: Option<PresetDialog>,
}

impl Default for PresetState {
    fn default() -> Self {
        Self::new(Err("The preset library location is unavailable.".into()))
    }
}

impl PresetState {
    pub(super) fn new(path: Result<PathBuf, String>) -> Self {
        Self {
            path,
            library: Default::default(),
            loaded: false,
            ready: false,
            selected: None,
            search: String::new(),
            error: None,
            notice: None,
            dialog: None,
        }
    }
    fn reload(&mut self) {
        self.loaded = true;
        match self
            .path
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|p| presets::load(p))
        {
            Ok(library) => {
                self.library = library;
                self.ready = true;
                self.error = None;
                if self
                    .selected
                    .is_some_and(|id| !self.library.entries.iter().any(|p| p.id == id))
                {
                    self.selected = None;
                }
            }
            Err(error) => {
                self.error = Some(error);
                self.ready = false;
            }
        }
    }
    fn apply(&mut self, change: Change) -> bool {
        match self
            .path
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|p| presets::change(p, change))
        {
            Ok((library, added)) => {
                self.library = library;
                self.selected = added
                    .or(self.selected)
                    .filter(|id| self.library.entries.iter().any(|p| p.id == *id));
                self.error = None;
                self.ready = true;
                self.loaded = true;
                true
            }
            Err(error) => {
                self.error = Some(error);
                false
            }
        }
    }
}

impl EditorState {
    fn save_selection_preset(&mut self, document: &mut Document, history: &mut EditHistory) {
        self.finish_annotation_gesture(document, history);
        match annotation_clipboard::selection_snapshot(document, &self.selected_indices(document)) {
            Ok(annotations) => {
                self.presets.error = None;
                self.presets.dialog = Some(PresetDialog::Save {
                    name: String::new(),
                    annotations,
                    focus: true,
                });
            }
            Err(error) => self.presets.error = Some(error),
        }
    }

    fn insert_preset(&mut self, id: u64, document: &mut Document, history: &mut EditHistory) {
        let Some(preset) = self.presets.library.entries.iter().find(|p| p.id == id) else {
            return;
        };
        let name = preset.name.clone();
        match annotation_clipboard::prepare_insertion(preset.annotations.clone(), document) {
            Ok(annotations) => {
                let (count, protected) = self.insert_annotations(annotations, document, history);
                self.presets.error = None;
                self.clipboard_status = Some(format!(
                    "Inserted “{name}” · {count} editable annotation(s).{}",
                    if protected {
                        " Hidden or locked copies are available in Layers."
                    } else {
                        ""
                    }
                ));
            }
            Err(error) => self.presets.error = Some(error),
        }
    }

    pub(super) fn presets_panel(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if !self.presets.loaded {
            self.presets.reload();
        }
        ui.spacing_mut().item_spacing = Vec2::new(6., 6.);
        ui.spacing_mut().button_padding = Vec2::new(8., 6.);
        ui.horizontal(|ui| {
            ui.strong("Annotation presets");
            ui.weak(self.presets.library.entries.len().to_string());
            if ui.small_button("Refresh").clicked() {
                self.presets.reload();
            }
        });
        ui.weak("Save artwork once. Reuse editable copies in any image.");
        if ui
            .add_enabled(
                self.presets.ready && !self.selected_indices(document).is_empty(),
                egui::Button::new("Save selection as preset…"),
            )
            .on_hover_text("Save selected annotations, groups, styles and layer settings")
            .clicked()
        {
            self.save_selection_preset(document, history);
        }
        if let Some(notice) = &self.presets.notice {
            ui.weak(notice);
        }
        if let Some(error) = &self.presets.error {
            ui.colored_label(Color32::from_rgb(170, 35, 45), error);
        }
        ui.add(
            egui::TextEdit::singleline(&mut self.presets.search)
                .hint_text("Search presets…")
                .desired_width(f32::INFINITY),
        );
        let selected = self
            .presets
            .selected
            .and_then(|id| self.presets.library.entries.iter().find(|p| p.id == id))
            .map(|p| (p.id, p.name.clone()));
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.presets.ready && selected.is_some(),
                    egui::Button::new("Insert preset"),
                )
                .clicked()
            {
                self.insert_preset(selected.as_ref().unwrap().0, document, history);
            }
            if ui
                .add_enabled(
                    self.presets.ready && selected.is_some(),
                    egui::Button::new("Rename…"),
                )
                .clicked()
            {
                let p = selected.as_ref().unwrap();
                self.presets.error = None;
                self.presets.dialog = Some(PresetDialog::Rename {
                    id: p.0,
                    name: p.1.clone(),
                    focus: true,
                });
            }
            if ui
                .add_enabled(
                    self.presets.ready && selected.is_some(),
                    egui::Button::new("Delete…"),
                )
                .clicked()
            {
                let p = selected.as_ref().unwrap();
                self.presets.error = None;
                self.presets.dialog = Some(PresetDialog::Delete {
                    id: p.0,
                    name: p.1.clone(),
                });
            }
        });
        ui.separator();
        let query = self.presets.search.trim().to_lowercase();
        let mut found = false;
        for preset in &self.presets.library.entries {
            if !preset.name.to_lowercase().contains(&query) {
                continue;
            }
            found = true;
            ui.push_id(("preset", preset.id), |ui| {
                if ui
                    .add_sized(
                        [ui.available_width(), 30.],
                        egui::Button::selectable(
                            self.presets.selected == Some(preset.id),
                            &preset.name,
                        )
                        .truncate(),
                    )
                    .on_hover_text(&preset.name)
                    .clicked()
                {
                    self.presets.selected = Some(preset.id);
                }
                let groups: std::collections::BTreeSet<_> = preset
                    .annotations
                    .iter()
                    .filter_map(|a| a.group_id)
                    .collect();
                ui.small(format!(
                    "{} annotation{} · {} group{}",
                    preset.annotations.len(),
                    if preset.annotations.len() == 1 {
                        ""
                    } else {
                        "s"
                    },
                    groups.len(),
                    if groups.len() == 1 { "" } else { "s" }
                ));
                let hidden = preset.annotations.iter().filter(|a| !a.visible()).count();
                let locked = preset.annotations.iter().filter(|a| a.locked()).count();
                if hidden + locked > 0 {
                    ui.weak(format!("{hidden} hidden · {locked} locked"));
                }
                preset_preview(ui, preset);
                ui.add_space(6.);
            });
        }
        if !found {
            ui.weak(if self.presets.library.entries.is_empty() {
                "Select artwork in the image or Layers, then save your first preset."
            } else {
                "No matching presets."
            });
        }
        ui.separator();
        ui.weak("Inserted copies keep their sizes and styles. Changes to an image never change the preset.");
        if let Ok(path) = &self.presets.path {
            ui.small("Saved on this computer")
                .on_hover_text(path.display().to_string());
        }
    }

    pub(super) fn preset_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.presets.dialog else {
            return;
        };
        let mut apply = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("annotation-preset-dialog")).show(ctx, |ui| {
            ui.set_width(360.);
            match dialog {
                PresetDialog::Save { name, focus, .. }
                | PresetDialog::Rename { name, focus, .. } => {
                    ui.heading("Name your preset");
                    ui.add_space(10.);
                    let response = ui.add(
                        egui::TextEdit::singleline(name)
                            .char_limit(64)
                            .hint_text("For example: Review callout")
                            .desired_width(f32::INFINITY),
                    );
                    if *focus {
                        response.request_focus();
                        *focus = false;
                    }
                    let valid = presets::validate_name(name).is_ok();
                    ui.weak("The library retains an independent editable snapshot.");
                    ui.horizontal(|ui| {
                        cancel = ui.button("Cancel").clicked();
                        apply = ui
                            .add_enabled(valid, egui::Button::new("Save preset"))
                            .clicked();
                    });
                    apply |= valid && ui.input(|i| i.key_pressed(Key::Enter));
                }
                PresetDialog::Delete { name, .. } => {
                    ui.heading("Delete preset?");
                    ui.label(format!("Remove “{name}” from your library?"));
                    ui.weak("Copies already inserted into images remain unchanged.");
                    ui.horizontal(|ui| {
                        cancel = ui.button("Cancel").clicked();
                        apply = ui.button("Delete preset").clicked();
                    });
                }
            }
            if let Some(error) = &self.presets.error {
                ui.colored_label(Color32::from_rgb(170, 35, 45), error);
            }
        });
        if apply {
            let change = match self.presets.dialog.as_ref().unwrap() {
                PresetDialog::Save {
                    name, annotations, ..
                } => Change::Add {
                    name: name.clone(),
                    annotations: annotations.clone(),
                },
                PresetDialog::Rename { id, name, .. } => Change::Rename {
                    id: *id,
                    name: name.clone(),
                },
                PresetDialog::Delete { id, .. } => Change::Delete { id: *id },
            };
            if self.presets.apply(change) {
                self.presets.dialog = None;
                self.presets.search.clear();
                self.presets.notice = Some("Preset library saved.".into());
            }
        } else if cancel || modal.should_close() {
            self.presets.dialog = None;
        }
    }
}

fn preset_preview(ui: &mut egui::Ui, preset: &Preset) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 72.), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect_filled(rect, 4., Color32::from_rgb(235, 239, 244));
    if preset
        .annotations
        .iter()
        .any(|a| a.visible() && a.kind.is_effect())
    {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Blur / pixelate preset",
            egui::FontId::proportional(13.),
            Color32::from_rgb(65, 78, 92),
        );
        return;
    }
    let bounds = preset
        .annotations
        .iter()
        .filter(|a| a.visible())
        .map(render::annotation_bounds)
        .fold(Rect::NOTHING, Rect::union);
    if !bounds.is_finite() {
        return;
    }
    let scale = ((rect.width() - 16.) / bounds.width().max(1.))
        .min((rect.height() - 16.) / bounds.height().max(1.))
        .min(1.);
    let origin = rect.center() - bounds.center().to_vec2() * scale;
    render::paint_annotations(&painter, &preset.annotations, origin, scale);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{document, presets::tests::artwork};

    fn blank() -> Document {
        Document {
            image: std::sync::Arc::new(image::RgbaImage::from_pixel(
                800,
                600,
                image::Rgba([245, 248, 252, 255]),
            )),
            annotations: vec![],
            geometry: Default::default(),
        }
    }

    #[test]
    fn saved_snapshot_survives_switch_restart_and_independent_editable_project_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("presets.json");
        let mut e = EditorState::with_presets(Ok(path.clone()));
        let mut source = blank();
        source.annotations = artwork();
        let mut h = EditHistory::new(source.edit_state());
        e.set_selection((0..10).collect(), &source);
        e.save_selection_preset(&mut source, &mut h);
        assert!(!h.can_undo() && !h.is_dirty(&source));
        let Some(PresetDialog::Save { annotations, .. }) = e.presets.dialog.take() else {
            panic!("save dialog")
        };
        assert!(e.presets.apply(Change::Add {
            name: "Review kit".into(),
            annotations
        }));
        source.annotations[0].text = "Changed in source".into();
        e.reset();
        assert_eq!(e.presets.library.entries[0].annotations, artwork());
        let mut e = EditorState::with_presets(Ok(path.clone()));
        e.presets.reload();
        let mut destination = blank();
        let original = destination.edit_state();
        let pixels = destination.image.clone();
        let mut h = EditHistory::new(original.clone());
        e.presets_open = true;
        e.insert_preset(1, &mut destination, &mut h);
        assert_eq!(destination.annotations.len(), 10);
        assert_eq!(
            e.selected_indices(&destination),
            (0..10).collect::<Vec<_>>()
        );
        assert!(e.layers_open && !e.presets_open);
        let first = destination.annotations.clone();
        e.insert_preset(1, &mut destination, &mut h);
        assert_ne!(
            destination.annotations[0].group_id,
            destination.annotations[10].group_id
        );
        assert_eq!(&destination.annotations[..10], &first);
        assert!(h.undo(&mut destination));
        assert_eq!(destination.annotations, first);
        assert!(h.undo(&mut destination));
        assert_eq!(destination.edit_state(), original);
        assert!(!h.undo(&mut destination));
        assert!(h.redo(&mut destination));
        let project = dir.path().join("destination.myimage");
        document::save_project(&destination, &project).unwrap();
        let mut reopened = document::load_project(&project).unwrap();
        assert_eq!(reopened.annotations, first);
        assert_eq!(reopened.image, pixels);
        reopened.annotations[0].text = "Edited copy".into();
        reopened.annotations[4].layer.locked = false;
        document::save_project(&reopened, &project).unwrap();
        assert_eq!(
            document::load_project(&project).unwrap().annotations,
            reopened.annotations
        );
        assert_eq!(
            presets::load(&path).unwrap().entries[0].annotations,
            artwork()
        );
        presets::change(&path, Change::Delete { id: 1 }).unwrap();
        assert_eq!(
            document::load_project(&project).unwrap().annotations,
            reopened.annotations
        );
    }

    #[test]
    fn failed_library_write_keeps_cache_and_document_and_allows_retry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("presets.json");
        let mut e = EditorState::with_presets(Ok(path.clone()));
        e.presets.reload();
        assert!(e.presets.apply(Change::Add {
            name: "Original".into(),
            annotations: artwork()
        }));
        let original = e.presets.library.clone();
        std::fs::write(&path, b"broken").unwrap();
        assert!(!e.presets.apply(Change::Delete { id: 1 }));
        assert_eq!(e.presets.library, original);
        assert!(e.presets.error.is_some());
        e.presets.reload();
        assert!(!e.presets.ready);
        assert_eq!(std::fs::read(&path).unwrap(), b"broken");
        std::fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
        e.presets.reload();
        assert!(e.presets.ready && e.presets.error.is_none());
    }

    #[test]
    fn invalid_insertion_is_atomic_and_single_member_snapshot_normalizes_group_flags() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = EditorState::with_presets(Ok(dir.path().join("presets.json")));
        let mut source = blank();
        source.annotations = artwork();
        for a in &mut source.annotations[..3] {
            a.group_layer.hidden = true;
            a.group_layer.locked = true;
        }
        let selected = annotation_clipboard::selection_snapshot(&source, &[0]).unwrap();
        assert_eq!(selected[0].group_id, None);
        assert!(selected[0].layer.hidden && selected[0].layer.locked);
        assert!(e.presets.apply(Change::Add {
            name: "Member".into(),
            annotations: selected
        }));
        let mut d = blank();
        d.annotations = vec![artwork()[3].clone(); 10_000];
        let original = d.edit_state();
        let mut h = EditHistory::new(original.clone());
        e.insert_preset(1, &mut d, &mut h);
        assert_eq!(d.edit_state(), original);
        assert!(!h.can_undo());
        assert!(e.presets.error.is_some());
    }
    fn frame(
        e: &mut EditorState,
        d: &mut Document,
        h: &mut EditHistory,
        ctx: &egui::Context,
        texture: &egui::TextureHandle,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280., 1000.))),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| e.ui(ui, d, h, texture, &mut Default::default()));
            },
        )
    }

    fn click(
        label: &str,
        e: &mut EditorState,
        d: &mut Document,
        h: &mut EditHistory,
        ctx: &egui::Context,
        texture: &egui::TextureHandle,
    ) -> egui::FullOutput {
        if matches!(label, "Copy" | "Paste" | "Undo" | "Redo") {
            click("Image actions", e, d, h, ctx, texture);
        }
        let output = frame(e, d, h, ctx, texture, vec![]);
        let at = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.job.text == label => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("Missing button {label}"));
        frame(
            e,
            d,
            h,
            ctx,
            texture,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        frame(
            e,
            d,
            h,
            ctx,
            texture,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ],
        )
    }

    #[test]
    fn native_panel_buttons_save_insert_search_rename_cancel_and_delete() {
        let ctx = egui::Context::default();
        crate::appearance::apply(&ctx);
        let texture = ctx.load_texture(
            "preset UI",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("presets.json");
        let mut e = EditorState::with_presets(Ok(path.clone()));
        let mut d = blank();
        d.annotations = artwork()[..3].to_vec();
        let initial = d.edit_state();
        let mut h = EditHistory::new(initial.clone());
        e.set_selection(vec![0, 1, 2], &d);
        click("Presets", &mut e, &mut d, &mut h, &ctx, &texture);
        click(
            "Save selection as preset…",
            &mut e,
            &mut d,
            &mut h,
            &ctx,
            &texture,
        );
        assert!(e.has_modal());
        frame(&mut e, &mut d, &mut h, &ctx, &texture, vec![]);
        frame(
            &mut e,
            &mut d,
            &mut h,
            &ctx,
            &texture,
            vec![egui::Event::Text("Review kit".into())],
        );
        let out = click("Save preset", &mut e, &mut d, &mut h, &ctx, &texture);
        assert!(
            !out.platform_output
                .commands
                .iter()
                .any(|c| matches!(c, egui::OutputCommand::CopyText(_)))
        );
        assert!(!e.has_modal());
        assert!(!h.can_undo());
        assert_eq!(d.edit_state(), initial);
        assert_eq!(presets::load(&path).unwrap().entries[0].name, "Review kit");
        e.reset();
        d = blank();
        h = EditHistory::new(d.edit_state());
        click("Presets", &mut e, &mut d, &mut h, &ctx, &texture);
        click("Review kit", &mut e, &mut d, &mut h, &ctx, &texture);
        click("Insert preset", &mut e, &mut d, &mut h, &ctx, &texture);
        assert_eq!(d.annotations.len(), 3);
        click("Undo", &mut e, &mut d, &mut h, &ctx, &texture);
        assert!(d.annotations.is_empty());
        click("Redo", &mut e, &mut d, &mut h, &ctx, &texture);
        assert_eq!(d.annotations.len(), 3);
        click("Rename…", &mut e, &mut d, &mut h, &ctx, &texture);
        if let Some(PresetDialog::Rename { name, .. }) = &mut e.presets.dialog {
            *name = "Renamed kit".into();
        } else {
            panic!("rename dialog");
        }
        click("Save preset", &mut e, &mut d, &mut h, &ctx, &texture);
        assert_eq!(presets::load(&path).unwrap().entries[0].name, "Renamed kit");
        e.presets.search = "nothing-matches".into();
        let out = frame(&mut e, &mut d, &mut h, &ctx, &texture, vec![]);
        assert!(out.shapes.iter().any(|s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.job.text == "No matching presets.")));
        e.presets.search.clear();
        click("Delete…", &mut e, &mut d, &mut h, &ctx, &texture);
        click("Cancel", &mut e, &mut d, &mut h, &ctx, &texture);
        assert_eq!(presets::load(&path).unwrap().entries.len(), 1);
        click("Delete…", &mut e, &mut d, &mut h, &ctx, &texture);
        click("Delete preset", &mut e, &mut d, &mut h, &ctx, &texture);
        assert!(presets::load(&path).unwrap().entries.is_empty());
        assert_eq!(d.annotations.len(), 3);
    }
}
