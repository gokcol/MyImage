//! Preview-first repairs and screenshot text replacement, committed as ordinary
//! editable annotations. Original pixels and undo history remain intact.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
#[derive(Default)]
pub(super) struct State {
    region: Option<Rect>,
    drag_start: Option<Point>,
    method: crate::repair::Method,
    job: Option<Job>,
    ready: Option<Prepared>,
    text: String,
    font: f32,
    ink: [u8; 4],
    original: bool,
    message: Option<String>,
    snapshot: Option<(Arc<image::RgbaImage>, EditState)>,
}
struct Job {
    image: Arc<image::RgbaImage>,
    before: EditState,
    tool: Tool,
    receiver: mpsc::Receiver<Result<Prepared, String>>,
    cancel: Arc<AtomicBool>,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
struct Prepared {
    region: Rect,
    stamp: crate::stamps::StampSource,
    text: Option<String>,
    ink: [u8; 4],
    font: f32,
}
impl EditorState {
    pub(super) fn retouch_active(&self) -> bool {
        matches!(
            self.tool,
            Tool::Repair | Tool::ScreenshotText | Tool::Redact
        )
    }
    pub(super) fn start_retouch(
        &mut self,
        tool: Tool,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let region = self.selection_region(document);
        self.finish_gesture(document, history);
        self.clear_selection();
        self.area = None;
        self.area_mask.clear();
        self.pixel_mask = None;
        self.retouch = State {
            region,
            ..Default::default()
        };
        self.tool = tool;
        self.focus_canvas = true;
        self.inspector.show(InspectorPanel::Styles);
    }
    pub(super) fn stop_retouch(&mut self) {
        self.retouch = Default::default();
    }
    pub(super) fn retouch_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        rect: Rect,
        scale: f32,
        document: &Document,
    ) {
        if self.retouch.job.is_some() {
            return;
        }
        for event in ui.input(|i| i.events.clone()) {
            match event {
                egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    ..
                } if rect.intersect(ui.clip_rect()).contains(pos)
                    && ui.ctx().layer_id_at(pos) == Some(ui.layer_id()) =>
                {
                    response.request_focus();
                    let start = object_point(pos, rect, scale, document);
                    self.retouch.drag_start = Some(start);
                    self.retouch.region = None;
                    self.retouch.ready = None;
                    self.retouch.message = None;
                }
                egui::Event::PointerMoved(pos)
                | egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    ..
                } if self.retouch.drag_start.is_some() => {
                    let start = point(self.retouch.drag_start.unwrap());
                    let end = point(object_point(rect.clamp(pos), rect, scale, document));
                    let bounds = Rect::from_two_pos(start, end);
                    self.retouch.region =
                        (bounds.width() >= 2. && bounds.height() >= 2.).then_some(bounds);
                }
                _ => {}
            }
        }
        if !ui.input(|i| i.pointer.primary_down()) {
            self.retouch.drag_start = None;
        }
    }
    fn begin_retouch_preview(&mut self, ctx: &egui::Context, document: &Document) {
        let Some(region) = self.retouch.region else {
            return;
        };
        if self.retouch.job.is_some() {
            return;
        }
        self.retouch.snapshot = Some((document.image.clone(), document.edit_state()));
        let snapshot = document.clone();
        let tool = self.tool;
        let method = self.retouch.method;
        let (tx, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let canceled = cancel.clone();
        let context = ctx.clone();
        let job = Job {
            image: document.image.clone(),
            before: document.edit_state(),
            tool,
            receiver,
            cancel,
        };
        match std::thread::Builder::new()
            .name("image-repair".into())
            .spawn(move || {
                let result = prepare(
                    &snapshot,
                    region,
                    method,
                    tool == Tool::ScreenshotText,
                    canceled,
                );
                let _ = tx.send(result);
                context.request_repaint();
            }) {
            Ok(_) => {
                self.retouch.job = Some(job);
                self.retouch.ready = None;
                self.retouch.message = None;
            }
            Err(e) => self.retouch.message = Some(format!("Could not start repair: {e}")),
        }
    }
    pub(super) fn poll_retouch(&mut self, ctx: &egui::Context, document: &Document) {
        if !self.retouch_active()
            || self
                .retouch
                .snapshot
                .as_ref()
                .is_some_and(|(image, before)| {
                    !Arc::ptr_eq(image, &document.image) || *before != document.edit_state()
                })
        {
            self.retouch.job = None;
            self.retouch.ready = None;
            self.retouch.snapshot = None;
            return;
        }
        let Some(job) = &self.retouch.job else {
            return;
        };
        if job.tool != self.tool
            || !Arc::ptr_eq(&job.image, &document.image)
            || job.before != document.edit_state()
        {
            self.retouch.job = None;
            self.retouch.ready = None;
            return;
        }
        let result = match job.receiver.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(60));
                return;
            }
            Err(_) => Err("Repair stopped. Please try again.".into()),
        };
        self.retouch.job = None;
        match result {
            Ok(prepared) => {
                self.retouch.region = Some(prepared.region);
                self.retouch.text = prepared.text.clone().unwrap_or_default();
                self.retouch.ink = prepared.ink;
                self.retouch.font = prepared.font;
                self.retouch.ready = Some(prepared);
                self.retouch.original = false;
            }
            Err(e) => self.retouch.message = Some(e),
        }
    }
    fn retouch_objects(&self) -> Vec<Annotation> {
        let Some(prepared) = &self.retouch.ready else {
            return vec![];
        };
        let mut patch = self.new_annotation(AnnotationKind::Stamp, from_pos(prepared.region.min));
        patch.end = from_pos(prepared.region.max);
        patch.stamp = Some(prepared.stamp.clone());
        patch.color = [255; 4];
        patch.layer.name = if self.tool == Tool::ScreenshotText {
            "Text background repair"
        } else {
            "Repair"
        }
        .into();
        let mut result = vec![patch];
        if self.tool == Tool::ScreenshotText && !self.retouch.text.trim().is_empty() {
            let mut text = self.new_annotation(AnnotationKind::Text, from_pos(prepared.region.min));
            text.text = self.retouch.text.clone();
            text.font_size = self.retouch.font;
            text.text_style = Default::default();
            text.color = self.retouch.ink;
            text.layer.name = "Replacement text".into();
            result.push(text);
        }
        result
    }
    fn apply_retouch(&mut self, document: &mut Document, history: &mut EditHistory) {
        let before = document.edit_state();
        let first = document.annotations.len();
        let mut objects = self.retouch_objects();
        if self.tool == Tool::Redact {
            let Some(region) = self.retouch.region else {
                return;
            };
            let mut a =
                self.new_annotation(AnnotationKind::Rectangle, from_pos(region.min.floor()));
            a.end = from_pos(region.max.ceil());
            a.color = [0, 0, 0, 255];
            a.fill = true;
            a.stroke_width = 1.;
            a.layer.name = "Redaction".into();
            objects = vec![a];
        }
        if objects.len() > 1 {
            let used = document
                .annotations
                .iter()
                .filter_map(|a| a.group_id)
                .collect();
            let group = fresh_group(&used);
            for a in &mut objects {
                a.group_id = Some(group);
            }
        }
        document.annotations.extend(objects);
        let indices = (first..document.annotations.len()).collect::<Vec<_>>();
        if let Err(error) = crate::canvas::extend_for_annotations(document, &indices)
            .and_then(|_| crate::document::validate_document(document))
        {
            document.annotations = before.annotations;
            document.geometry = before.geometry;
            self.retouch.message = Some(error);
            return;
        }
        history.commit(before, document);
        self.retouch.ready = None;
        self.retouch.region = None;
        if self.tool == Tool::Redact {
            self.retouch.message = Some(
                "Opaque cover added. Draw another region, or copy the flattened image.".into(),
            );
        } else {
            self.tool = Tool::Area;
            self.set_selection(indices, document);
        }
    }
    pub(super) fn retouch_panel(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        ui.heading(match self.tool {
            Tool::Repair => "Smart Erase / Repair",
            Tool::ScreenshotText => "Edit Screenshot Text",
            _ => "Redact for Sharing",
        });
        ui.small(if self.tool == Tool::ScreenshotText {
            "Drag around one line or block of text. Leave a small margin of clean background."
        } else {
            "Drag a rectangle over the area you want to change."
        });
        if self.tool == Tool::Redact {
            ui.small("Adds an opaque black cover. Share a flattened image; editable projects retain the original pixels.");
            if ui
                .add_enabled(
                    self.retouch.region.is_some(),
                    egui::Button::new("Add opaque redaction"),
                )
                .clicked()
            {
                self.apply_retouch(document, history);
            }
            if ui
                .add_enabled(
                    self.clipboard_job.is_none(),
                    egui::Button::new("Copy flattened image"),
                )
                .clicked()
            {
                self.copy_image(ui.ctx(), document, true);
            }
            self.copy_feedback(ui);
        } else {
            let before = self.retouch.method;
            egui::ComboBox::from_id_salt("repair-source")
                .selected_text(self.retouch.method.name())
                .show_ui(ui, |ui| {
                    for method in crate::repair::Method::ALL {
                        ui.selectable_value(&mut self.retouch.method, method, method.name());
                    }
                });
            if before != self.retouch.method {
                self.retouch.ready = None;
            }
            ui.small("Surrounding colors works best on plain backgrounds. Clone a nearby area to retain texture.");
            if self.retouch.job.is_some() {
                ui.spinner();
                ui.label(if self.tool == Tool::ScreenshotText {
                    "Reading text locally…"
                } else {
                    "Preparing repair…"
                });
                if ui.button("Cancel processing").clicked() {
                    self.retouch.job = None;
                }
            } else if ui
                .add_enabled(
                    self.retouch.region.is_some(),
                    egui::Button::new(if self.tool == Tool::ScreenshotText {
                        "Read text & preview"
                    } else {
                        "Preview repair"
                    }),
                )
                .clicked()
            {
                self.begin_retouch_preview(ui.ctx(), document);
            }
            if self.retouch.ready.is_some() {
                if self.tool == Tool::ScreenshotText {
                    ui.label("Replacement text");
                    ui.add(
                        egui::TextEdit::multiline(&mut self.retouch.text)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY)
                            .char_limit(16_384),
                    );
                    object_properties::slider(
                        ui,
                        &mut self.retouch.font,
                        6.0..=256.,
                        "Font size",
                        " px",
                    );
                    object_properties::palette(ui, &mut self.retouch.ink);
                    ui.small(
                        "Font and color are estimated. The replacement remains editable Text.",
                    );
                }
                ui.checkbox(&mut self.retouch.original, "Show original");
                if ui.button("Apply").clicked() {
                    self.apply_retouch(document, history);
                }
            }
        }
        if let Some(message) = &self.retouch.message {
            ui.label(message);
        }
        ui.add_space(8.);
        if ui.button("Done").clicked() {
            self.stop_retouch();
            self.tool = Tool::Area;
        }
    }
    pub(super) fn retouch_preview(&self, document: &Document) -> Option<Document> {
        if !self.retouch_active() || self.retouch.ready.is_none() || self.retouch.original {
            return None;
        }
        let mut preview = document.clone();
        preview.annotations.extend(self.retouch_objects());
        Some(preview)
    }
    pub(super) fn paint_retouch(&self, ui: &egui::Ui, rect: Rect, scale: f32, document: &Document) {
        if let Some(region) = self.retouch.region {
            let bounds = mapped_bounds(region, document);
            let screen = Rect::from_min_max(
                rect.min + bounds.min.to_vec2() * scale,
                rect.min + bounds.max.to_vec2() * scale,
            );
            ui.painter().rect_stroke(
                screen,
                0.,
                Stroke::new(1.5, crate::appearance::palette(ui.ctx()).accent),
                StrokeKind::Outside,
            );
        }
    }
}
fn prepare(
    document: &Document,
    region: Rect,
    method: crate::repair::Method,
    ocr: bool,
    cancel: Arc<AtomicBool>,
) -> Result<Prepared, String> {
    let bounds = document.geometry.canvas_rect(original_size(document));
    let mut source = crate::canvas::render_document(document)?;
    source.geometry = Default::default();
    let image = render::flatten(&source)?;
    let origin = egui::vec2(bounds.x as f32, bounds.y as f32);
    let selected = region.translate(-origin).intersect(Rect::from_min_size(
        Pos2::ZERO,
        egui::vec2(image.width() as f32, image.height() as f32),
    ));
    if !selected.is_positive() {
        return Err("Select pixels inside the image.".into());
    }
    let min = selected.min.floor();
    let max = selected.max.ceil();
    let [x, y, w, h] = [
        min.x as u32,
        min.y as u32,
        (max.x - min.x) as u32,
        (max.y - min.y) as u32,
    ];
    if cancel.load(Ordering::Acquire) {
        return Err("Repair canceled.".into());
    }
    let pixels = if ocr && method == crate::repair::Method::Surrounding {
        crate::repair::text_patch(&image, [x, y, w, h])?
    } else {
        crate::repair::patch(&image, [x, y, w, h], method)?
    };
    let original = image::imageops::crop_imm(&image, x, y, w, h).to_image();
    let ink = crate::repair::text_ink(&original, &pixels);
    let text = if ocr {
        Some(crate::ocr::recognize_cancellable(original, cancel)?)
    } else {
        None
    };
    let lines = text.as_ref().map_or(1, |t| t.lines().count().max(1));
    let font = (h as f32 / lines as f32 * 0.8).clamp(6., 256.);
    let mut stamp = crate::stamps::StampSource::from_pixels(&pixels)?;
    if let crate::stamps::StampSource::Raster { name, .. } = &mut stamp {
        *name = "Repaired background".into();
    }
    Ok(Prepared {
        region: Rect::from_min_max(min + origin, max + origin),
        stamp,
        text,
        ink,
        font,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn document() -> Document {
        Document {
            image: Arc::new(image::RgbaImage::from_fn(120, 100, |x, y| {
                image::Rgba(if (30..70).contains(&x) && (30..50).contains(&y) {
                    [255, 0, 0, 255]
                } else {
                    [220, 230, 240, 255]
                })
            })),
            annotations: vec![],
            geometry: Default::default(),
        }
    }
    #[test]
    fn repair_is_previewed_and_committed_as_one_reversible_edit() {
        let mut d = document();
        let before = d.edit_state();
        let pixels = d.image.clone();
        let mut e = EditorState::default();
        let mut history = EditHistory::new(before.clone());
        e.start_retouch(Tool::Repair, &mut d, &mut history);
        e.retouch.ready = Some(
            prepare(
                &d,
                Rect::from_min_max(egui::pos2(30., 30.), egui::pos2(70., 50.)),
                crate::repair::Method::Surrounding,
                false,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap(),
        );
        assert_eq!(d.edit_state(), before);
        assert!(!history.can_undo());
        assert_eq!(
            render::flatten(&e.retouch_preview(&d).unwrap())
                .unwrap()
                .get_pixel(50, 40)
                .0,
            [220, 230, 240, 255]
        );
        e.apply_retouch(&mut d, &mut history);
        assert_eq!(d.image, pixels);
        let repaired = render::flatten(&d).unwrap();
        assert_eq!(repaired.get_pixel(50, 40).0, [220, 230, 240, 255]);
        assert!(history.undo(&mut d));
        assert_eq!(d.edit_state(), before);
        assert!(history.redo(&mut d));
        assert_eq!(render::flatten(&d).unwrap(), repaired);
    }
    #[test]
    fn replacement_text_is_editable_and_grouped_with_its_repaired_background() {
        let mut d = document();
        let mut e = EditorState::default();
        let mut history = EditHistory::new(d.edit_state());
        e.start_retouch(Tool::ScreenshotText, &mut d, &mut history);
        e.retouch.ready = Some(
            prepare(
                &d,
                Rect::from_min_max(egui::pos2(30., 30.), egui::pos2(70., 50.)),
                crate::repair::Method::Surrounding,
                false,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap(),
        );
        e.retouch.text = "Updated".into();
        e.retouch.font = 14.;
        e.retouch.ink = [10, 20, 30, 255];
        e.apply_retouch(&mut d, &mut history);
        assert_eq!(d.annotations.len(), 2);
        assert_eq!(d.annotations[1].kind, AnnotationKind::Text);
        assert_eq!(d.annotations[1].text, "Updated");
        assert_eq!(d.annotations[0].group_id, d.annotations[1].group_id);
        assert!(d.annotations[0].group_id.is_some());
        assert!(history.undo(&mut d));
        assert!(d.annotations.is_empty());
    }
    #[test]
    fn redaction_copy_contains_only_composited_pixels_and_can_be_undone() {
        let mut d = document();
        let mut e = EditorState::default();
        let before = d.edit_state();
        let mut history = EditHistory::new(before.clone());
        e.start_retouch(Tool::Redact, &mut d, &mut history);
        e.retouch.region = Some(Rect::from_min_max(
            egui::pos2(29.5, 29.5),
            egui::pos2(70.5, 50.5),
        ));
        e.apply_retouch(&mut d, &mut history);
        let pixels = crate::image_clipboard::copy_pixels(&d, None).unwrap();
        for y in 30..50 {
            for x in 30..70 {
                assert_eq!(pixels.get_pixel(x, y).0, [0, 0, 0, 255]);
            }
        }
        assert_eq!(d.image.get_pixel(50, 40).0, [255, 0, 0, 255]);
        assert!(history.undo(&mut d));
        assert_eq!(d.edit_state(), before);
    }
}
