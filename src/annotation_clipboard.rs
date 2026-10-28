//! A bounded, versioned native text clipboard payload. No source image pixels.
use super::*;
use crate::appearance::Icon as DialogIcon;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const PREFIX: &str = "MYIMAGE-ANNOTATIONS\n";
const VERSION: u32 = 1;
const MAX_BYTES: usize = 4 * 1024 * 1024;

pub(super) struct ClipboardJob {
    receiver: std::sync::mpsc::Receiver<Result<ClipboardResult, String>>,
    copy_started: Option<std::time::Instant>,
}
pub(super) struct CutPlan {
    image: std::sync::Arc<image::RgbaImage>,
    before: EditState,
    removal: Annotation,
}
pub(super) enum ClipboardResult {
    Copied(u32, u32),
    Cut(u32, u32, Box<CutPlan>),
    Pasted(crate::stamps::StampSource, Rect, Box<Document>),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    version: u32,
    annotations: Vec<Annotation>,
}

pub(super) fn selection_snapshot(
    document: &Document,
    indices: &[usize],
) -> Result<Vec<Annotation>, String> {
    if indices.is_empty() {
        return Err("Select annotations or a group to copy.".into());
    }
    let mut selected = indices.to_vec();
    selected.sort_unstable();
    selected.dedup();
    let mut annotations = Vec::with_capacity(selected.len());
    let mut counts = BTreeMap::new();
    for &i in &selected {
        let a = document
            .annotations
            .get(i)
            .ok_or("The selection is no longer available.")?;
        if let Some(id) = a.group_id {
            *counts.entry(id).or_insert(0) += 1;
        }
        annotations.push(a.clone());
    }
    // An individual member can be copied without dragging its unselected group
    // along. Retain inherited visibility and locks when removing that membership.
    for a in &mut annotations {
        if let Some(id) = a.group_id
            && counts[&id] == 1
            && document
                .annotations
                .iter()
                .filter(|a| a.group_id == Some(id))
                .count()
                > 1
        {
            a.layer.hidden |= a.group_layer.hidden;
            a.layer.locked |= a.group_layer.locked;
            a.group_id = None;
            a.group_layer = Default::default();
        }
    }
    if annotations
        .iter()
        .any(|a| a.kind.is_area_edit() || a.kind == AnnotationKind::Fill)
    {
        return Err("Fills and area pixel edits belong to this image. Select annotation objects to copy or save a preset.".into());
    }
    crate::document::validate_annotations(&annotations)?;
    Ok(annotations)
}

fn encode(document: &Document, indices: &[usize]) -> Result<String, String> {
    let annotations = selection_snapshot(document, indices)?;
    let json = serde_json::to_string(&Payload {
        version: VERSION,
        annotations,
    })
    .map_err(|e| format!("Cannot copy annotations: {e}"))?;
    if json.len() > MAX_BYTES {
        return Err("This selection exceeds the clipboard data limit.".into());
    }
    Ok(format!("{PREFIX}{json}"))
}

fn decode(text: &str) -> Result<Vec<Annotation>, String> {
    if text.len() > MAX_BYTES + PREFIX.len() {
        return Err("Clipboard annotation data exceeds the 4 MiB limit.".into());
    }
    let json = text.strip_prefix(PREFIX).ok_or(
        "The clipboard does not contain MyImage annotations. Copy a selection in Edit first.",
    )?;
    let payload: Payload = serde_json::from_str(json)
        .map_err(|_| "The clipboard contains invalid MyImage annotation data.")?;
    if payload.version != VERSION {
        return Err(format!(
            "Clipboard annotation version {} is not supported.",
            payload.version
        ));
    }
    if payload.annotations.is_empty() {
        return Err("The clipboard selection is empty.".into());
    }
    crate::document::validate_annotations(&payload.annotations)?;
    Ok(payload.annotations)
}

fn pasted_annotations(text: &str, destination: &Document) -> Result<Vec<Annotation>, String> {
    prepare_insertion(decode(text)?, destination)
}

pub(super) fn prepare_insertion(
    mut annotations: Vec<Annotation>,
    destination: &Document,
) -> Result<Vec<Annotation>, String> {
    if annotations.is_empty() {
        return Err("There are no annotations to insert.".into());
    }
    crate::document::validate_annotations(&annotations)?;
    if destination.annotations.len() + annotations.len() > crate::document::MAX_ANNOTATIONS {
        return Err("Inserting would exceed the 10,000 annotation limit.".into());
    }
    let mut used: BTreeSet<_> = destination
        .annotations
        .iter()
        .filter_map(|a| a.group_id)
        .collect();
    let mut groups = BTreeMap::new();
    let mut next_id = 1;
    for a in &mut annotations {
        if let Some(id) = a.group_id {
            let new_id = *groups.entry(id).or_insert_with(|| {
                while used.contains(&next_id) {
                    next_id += 1;
                }
                used.insert(next_id);
                next_id
            });
            a.group_id = Some(new_id);
        }
    }
    // Translation preserves every style, shape, sample and pointer. Center in
    // the destination crop; its image rotation/resize applies to these objects
    // just like artwork created there. Oversized selections remain editable.
    let bounds = annotations
        .iter()
        .map(render::annotation_bounds)
        .fold(Rect::NOTHING, Rect::union);
    let crop = destination.geometry.canvas_rect(original_size(destination));
    let center = Pos2::new(
        crop.x as f32 + crop.width as f32 * 0.5,
        crop.y as f32 + crop.height as f32 * 0.5,
    );
    let delta = center - bounds.center();
    for a in &mut annotations {
        *a = offset_annotation(a, delta);
    }
    crate::connectors::remap(&mut annotations, &destination.annotations);
    let mut candidate = destination.clone();
    candidate.annotations.extend(annotations.iter().cloned());
    crate::document::validate_document(&candidate)?;
    // Reserve space for image dimensions and reversible image geometry metadata.
    if serde_json::to_vec(&candidate.annotations).map_or(true, |json| {
        json.len() > crate::document::MAX_METADATA_BYTES - 1024
    }) {
        return Err("Inserting would exceed the project's annotation data limit.".into());
    }
    Ok(annotations)
}

impl EditorState {
    pub(super) fn copy_scope(&self, document: &Document) -> String {
        match self.area {
            Some(rect) => match crate::image_clipboard::copy_bounds(document, rect) {
                Ok(bounds) => format!("Selected: {} × {} px", bounds.width, bounds.height),
                Err(_) => "Selection outside image".into(),
            },
            None => "Copy: whole image".into(),
        }
    }

    pub(super) fn pixel_area(&self, document: &Document) -> Option<(Rect, Vec<Point>)> {
        self.area.map(|rect| {
            let mask = self
                .selected
                .and_then(|i| document.annotations.get(i))
                .filter(|a| a.kind.needs_composite())
                .map(|a| a.area_mask.clone())
                .unwrap_or_else(|| self.area_mask.clone());
            (rect, mask)
        })
    }
    pub(super) fn copy_image(&mut self, ctx: &egui::Context, document: &Document, whole: bool) {
        self.start_image_copy(ctx, document, whole, None);
    }

    pub(super) fn cut_selection(
        &mut self,
        ctx: &egui::Context,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if self.clipboard_job.is_some() {
            return;
        }
        self.copy_confirmation = None;
        self.clipboard_status = None;
        self.finish_area_drag(document, history);
        let Some((rect, mask)) = self.pixel_area(document) else {
            self.image_error = Some(
                "Select an area before cutting. Use Select all to cut the whole image.".into(),
            );
            return;
        };
        if self.selected.is_some() && !self.can_edit_selection(document) {
            self.image_error = Some("This selection is locked. Unlock it before cutting.".into());
            return;
        }
        let mut removal = self.new_annotation(AnnotationKind::DeleteArea, from_pos(rect.min));
        removal.end = from_pos(rect.max);
        removal.area_mask = mask;
        removal.pixel_mask = self
            .selected
            .and_then(|i| document.annotations.get(i))
            .map(|a| a.pixel_mask.clone())
            .unwrap_or_else(|| self.pixel_mask.clone());
        removal.layer.name = "Cut area".into();
        let mut candidate = document.clone();
        candidate.annotations.push(removal.clone());
        if let Err(error) = crate::document::validate_document(&candidate) {
            self.image_error = Some(error);
            return;
        }
        let plan = CutPlan {
            image: document.image.clone(),
            before: document.edit_state(),
            removal,
        };
        self.start_image_copy(ctx, document, false, Some(Box::new(plan)));
    }

    fn start_image_copy(
        &mut self,
        ctx: &egui::Context,
        document: &Document,
        whole: bool,
        cut: Option<Box<CutPlan>>,
    ) {
        if self.clipboard_job.is_some() {
            return;
        }
        let area = if whole {
            None
        } else {
            self.pixel_area(document)
        };
        let pixel_mask = if whole {
            None
        } else {
            self.selected
                .and_then(|i| document.annotations.get(i))
                .map(|a| a.pixel_mask.clone())
                .unwrap_or_else(|| self.pixel_mask.clone())
        };
        let document = document.clone();
        let ctx = ctx.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.clipboard_job = Some(ClipboardJob {
            receiver: rx,
            copy_started: Some(std::time::Instant::now()),
        });
        self.copy_confirmation = None;
        self.image_error = None;
        self.clipboard_status = Some("Copying image…".into());
        std::thread::spawn(move || {
            // Unit tests inspect the raster route without modifying the OS clipboard.
            #[cfg_attr(test, allow(clippy::bind_instead_of_map))]
            let result = crate::image_clipboard::copy_pixels_masked(&document, area, pixel_mask)
                .and_then(|pixels| {
                    #[cfg(not(test))]
                    crate::image_clipboard::write_image(&pixels)?;
                    Ok(match cut {
                        Some(plan) => ClipboardResult::Cut(pixels.width(), pixels.height(), plan),
                        None => ClipboardResult::Copied(pixels.width(), pixels.height()),
                    })
                });
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    fn paste_image(&mut self, ctx: &egui::Context, document: &Document) {
        if self.clipboard_job.is_some() {
            return;
        }
        let document = document.clone();
        let ctx = ctx.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.clipboard_job = Some(ClipboardJob {
            receiver: rx,
            copy_started: None,
        });
        self.copy_confirmation = None;
        self.clipboard_status = Some("Pasting image…".into());
        std::thread::spawn(move || {
            let result = crate::image_clipboard::read_image()
                .and_then(|pixels| crate::image_clipboard::pasted_raster(pixels, &document))
                .map(|(source, rect)| ClipboardResult::Pasted(source, rect, Box::new(document)));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    pub(super) fn poll_clipboard(&mut self, document: &mut Document, history: &mut EditHistory) {
        let Some(result) =
            self.clipboard_job
                .as_ref()
                .and_then(|job| match job.receiver.try_recv() {
                    Ok(result) => Some(result),
                    Err(std::sync::mpsc::TryRecvError::Empty) => None,
                    Err(_) => Some(Err(
                        "Clipboard operation stopped unexpectedly. Try again.".into()
                    )),
                })
        else {
            return;
        };
        self.clipboard_job = None;
        let result = result.and_then(|result| match result {
            ClipboardResult::Copied(w,h) => {
                self.confirm_copy(format!("{w} × {h} px"));
                Ok(format!("Copied to clipboard · {w} × {h} px"))
            },
            ClipboardResult::Cut(w,h,plan) => {
                if !std::sync::Arc::ptr_eq(&plan.image, &document.image) || plan.before != document.edit_state() {
                    return Err("Copied to clipboard, but the image changed before Cut finished. Nothing was removed; select the area and try again.".into());
                }
                document.annotations.push(plan.removal);
                history.commit(plan.before, document);
                let focus = self.focus_canvas;
                self.deselect_pixels();
                self.focus_canvas = focus;
                self.confirm_copy(format!("{w} × {h} px · Cut to transparency"));
                Ok(format!("Copied to clipboard · {w} × {h} px. Cut area is transparent. Undo restores it."))
            },
            ClipboardResult::Pasted(source,rect,before) => {
                if !std::sync::Arc::ptr_eq(&before.image, &document.image) || before.edit_state() != document.edit_state() {
                    return Err("The image changed while reading the clipboard. Paste again to place it in the current image.".into());
                }
                let mut annotation = self.new_annotation(AnnotationKind::Stamp, from_pos(rect.min));
                annotation.end = from_pos(rect.max);
                annotation.color = [255;4];
                annotation.layer.name = "Pasted image".into();
                annotation.stamp = Some(source);
                let mut candidate = document.clone();
                candidate.annotations.push(annotation.clone());
                crate::document::validate_document(&candidate)?;
                let (count, _) = self.insert_annotations(vec![annotation],document,history);
                if count == 0 { return Err(self.image_error.clone().unwrap_or_else(|| "Cannot paste this image.".into())); }
                self.area = None;
                self.area_mask.clear();
                self.pixel_mask = None;
                Ok("Pasted image layer. Move, resize or change opacity; Save project keeps it editable.".into())
            }
        });
        match result {
            Ok(message) => {
                self.clipboard_status = Some(message);
                self.image_error = None;
            }
            Err(message) => {
                self.image_error = Some(message);
                self.clipboard_status = None;
            }
        }
    }
    fn copy_current(&mut self, ctx: &egui::Context, document: &Document) {
        self.copy_image(ctx, document, false);
    }
    fn confirm_copy(&mut self, detail: String) {
        self.copy_confirmation = Some((
            detail,
            std::time::Instant::now() + std::time::Duration::from_secs(4),
        ));
    }

    pub(super) fn copy_feedback(&mut self, ui: &mut egui::Ui) {
        let now = std::time::Instant::now();
        if self
            .copy_confirmation
            .as_ref()
            .is_some_and(|(_, until)| now >= *until)
        {
            self.copy_confirmation = None;
        }
        let mut heading = None;
        let mut detail = None;
        if let Some((text, until)) = &self.copy_confirmation {
            heading = Some("Copied to clipboard");
            detail = Some(text.as_str());
            ui.ctx()
                .request_repaint_after(until.saturating_duration_since(now));
        } else if let Some(started) = self.clipboard_job.as_ref().and_then(|job| job.copy_started) {
            let delay = std::time::Duration::from_millis(200);
            if now.duration_since(started) >= delay {
                heading = Some("Copying to clipboard…");
            } else {
                ui.ctx()
                    .request_repaint_after(delay - now.duration_since(started));
            }
        }
        // Reserve the same final toolbar slot even when empty. Feedback never
        // dims the editor, changes the canvas size, or steals keyboard focus.
        let width = ui.available_width().clamp(0., 260.);
        ui.allocate_ui_with_layout(
            egui::vec2(width, 38.),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                if width >= 40.
                    && let Some(heading) = heading
                {
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(width, 36.), egui::Sense::hover());
                    ui.painter().rect(
                        rect.shrink(1.),
                        6,
                        crate::appearance::palette(ui.ctx()).navy,
                        egui::Stroke::NONE,
                        egui::StrokeKind::Inside,
                    );
                    crate::appearance::icon(
                        ui.painter(),
                        rect.left_center() + egui::vec2(15., 0.),
                        if detail.is_some() {
                            DialogIcon::Check
                        } else {
                            DialogIcon::Copy
                        },
                        crate::appearance::palette(ui.ctx()).ginger,
                        15.,
                    );
                    let painter = ui
                        .painter()
                        .with_clip_rect(rect.shrink(4.).intersect(ui.clip_rect()));
                    painter.text(
                        rect.left_top() + egui::vec2(29., 5.),
                        egui::Align2::LEFT_TOP,
                        heading,
                        egui::FontId::proportional(12.),
                        Color32::WHITE,
                    );
                    if let Some(detail) = detail {
                        painter.text(
                            rect.left_top() + egui::vec2(29., 20.),
                            egui::Align2::LEFT_TOP,
                            detail,
                            egui::FontId::proportional(11.),
                            Color32::from_gray(205),
                        );
                    }
                    response.on_hover_text(format!(
                        "{heading}{}",
                        detail.map_or(String::new(), |s| format!(" · {s}"))
                    ));
                }
            },
        );
    }
    fn copy_annotations(&mut self, ctx: &egui::Context, document: &Document) {
        let indices = self.selected_indices(document);
        match encode(document, &indices) {
            Ok(text) => {
                ctx.copy_text(text);
                self.confirm_copy(format!("{} editable annotation(s)", indices.len()));
                self.clipboard_status = Some(format!(
                    "Copied {} annotation(s). Open another image and choose Paste.",
                    indices.len()
                ));
                self.image_error = None;
            }
            Err(error) => {
                self.image_error = Some(error);
                self.clipboard_status = None;
            }
        }
    }

    fn paste_annotations(
        &mut self,
        text: &str,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let annotations = match pasted_annotations(text, document) {
            Ok(annotations) => annotations,
            Err(error) => {
                self.image_error = Some(error);
                self.clipboard_status = None;
                return;
            }
        };
        let (count, protected) = self.insert_annotations(annotations, document, history);
        self.clipboard_status = Some(format!(
            "Pasted {count} editable annotation(s).{}",
            if protected {
                " Hidden and locked items are available in Layers."
            } else {
                ""
            }
        ));
    }

    pub(super) fn insert_annotations(
        &mut self,
        annotations: Vec<Annotation>,
        document: &mut Document,
        history: &mut EditHistory,
    ) -> (usize, bool) {
        self.finish_gesture(document, history);
        let before = document.edit_state();
        let first = document.annotations.len();
        let count = annotations.len();
        let protected = annotations.iter().any(|a| a.locked() || !a.visible());
        let expand = annotations.iter().any(|a| a.kind == AnnotationKind::Stamp);
        let mut candidate = document.clone();
        candidate.annotations.extend(annotations);
        let check = if expand {
            crate::canvas::extend_for_annotations(
                &mut candidate,
                &(first..first + count).collect::<Vec<_>>(),
            )
            .map(|_| ())
        } else {
            Ok(())
        }
        .and_then(|_| crate::document::validate_document(&candidate));
        if let Err(error) = check {
            self.image_error = Some(error);
            return (0, false);
        }
        document.annotations = candidate.annotations;
        document.geometry = candidate.geometry;
        history.commit(before, document);
        self.set_selection((first..first + count).collect(), document);
        self.tool = Tool::Select;
        self.focus_canvas = true;
        if protected {
            self.inspector.show(InspectorPanel::Layers);
        }
        self.image_error = None;
        (count, protected)
    }

    pub(super) fn clipboard_events(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if self.gesture.is_some()
            || self.area_drag.is_some()
            || self.has_modal()
            || !ui.is_enabled()
        {
            return;
        }
        let events = ui.input_mut(|i| {
            let mut clipboard = Vec::new();
            i.events.retain(|event| {
                if matches!(
                    event,
                    egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_)
                ) {
                    clipboard.push(event.clone());
                    false
                } else {
                    true
                }
            });
            // Also accept the user's explicit Control shortcuts on macOS;
            // Command remains the platform default. Text fields never reach here.
            #[cfg(target_os = "macos")]
            for (key, event) in [(Key::C, egui::Event::Copy), (Key::X, egui::Event::Cut)] {
                if i.consume_key(Modifiers::CTRL, key) && !clipboard.contains(&event) {
                    clipboard.push(event);
                }
            }
            clipboard
        });
        for event in events {
            match event {
                egui::Event::Copy => self.copy_current(ui.ctx(), document),
                egui::Event::Cut => self.cut_selection(ui.ctx(), document, history),
                egui::Event::Paste(text) if text.starts_with(PREFIX) => {
                    self.paste_annotations(&text, document, history)
                }
                egui::Event::Paste(_) => self.paste_image(ui.ctx(), document),
                _ => unreachable!(),
            }
        }
    }

    pub(super) fn clipboard_buttons(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if canvas_menu::item(
            ui,
            "Copy",
            canvas_menu::MenuIcon::Glyph(inspector_widgets::Glyph::Copy),
            true,
        )
        .on_hover_text(
            "Copy the selected area, or whole image when no area is selected (Command/Ctrl + C)",
        )
        .clicked()
        {
            self.finish_annotation_gesture(document, history);
            self.copy_current(ui.ctx(), document);
            self.focus_canvas = true;
            ui.close();
        }
        if self.extract_text_button(ui, document, history) {
            ui.close();
        }
        if self.selected.is_some()
            && self.selected_indices(document).iter().all(|&i| {
                !document.annotations[i].kind.is_area_edit()
                    && document.annotations[i].kind != AnnotationKind::Fill
            })
            && canvas_menu::item(
                ui,
                "Copy editable annotations",
                canvas_menu::MenuIcon::App(DialogIcon::Copy),
                true,
            )
            .on_hover_text(
                "Copy selected objects and groups with their styles for another MyImage document",
            )
            .clicked()
        {
            self.copy_annotations(ui.ctx(), document);
            ui.close();
        }
        if canvas_menu::item(
            ui,
            "Cut",
            canvas_menu::MenuIcon::Glyph(inspector_widgets::Glyph::Cut),
            self.area.is_some(),
        )
        .on_hover_text(
            "Copy the selected pixels, then clear that area to transparency (Command/Ctrl + X)",
        )
        .clicked()
        {
            self.cut_selection(ui.ctx(), document, history);
            ui.close();
        }
        if canvas_menu::item(ui, "Paste", canvas_menu::MenuIcon::Paste, true)
            .on_hover_text(
                "Paste an image as a movable layer, or paste editable MyImage annotations (Command/Ctrl + V)",
            )
            .clicked()
        {
            self.finish_gesture(document, history);
            self.focus_canvas = true;
            ui.close();
            crate::image_clipboard::request_paste();
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{document, geometry::CropRect, markup_tests::object};
    use std::sync::Arc;

    #[test]
    fn pending_paste_does_not_dim_editor_and_stale_delivery_never_changes_image() {
        let mut d = blank();
        let mut h = EditHistory::new(d.edit_state());
        let mut e = EditorState::default();
        let (tx, rx) = std::sync::mpsc::channel();
        e.clipboard_job = Some(ClipboardJob {
            receiver: rx,
            copy_started: None,
        });
        assert!(!e.has_modal(), "Clipboard reads must not dim the editor");
        let (source, rect) = crate::image_clipboard::pasted_raster(
            image::RgbaImage::from_pixel(20, 10, image::Rgba([255, 0, 0, 255])),
            &d,
        )
        .unwrap();
        let before = d.clone();
        d.geometry.quarter_turns = 1;
        let changed = d.edit_state();
        tx.send(Ok(ClipboardResult::Pasted(source, rect, Box::new(before))))
            .unwrap();
        e.poll_clipboard(&mut d, &mut h);
        assert_eq!(d.edit_state(), changed);
        assert!(e.image_error.as_deref().unwrap().contains("Paste again"));
        assert!(!h.can_undo());
    }

    fn finish_clipboard(e: &mut EditorState, d: &mut Document, h: &mut EditHistory) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while e.clipboard_job.is_some() {
            e.poll_clipboard(d, h);
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
    }

    #[test]
    fn copy_keeps_editor_enabled_and_confirms_only_after_success() {
        let (tx, receiver) = std::sync::mpsc::channel();
        let mut e = EditorState {
            clipboard_job: Some(ClipboardJob {
                receiver,
                copy_started: Some(std::time::Instant::now()),
            }),
            ..Default::default()
        };
        let mut d = blank();
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        assert!(!e.has_modal(), "Copy must not disable/dim the workspace");
        e.poll_clipboard(&mut d, &mut h);
        assert!(e.copy_confirmation.is_none());
        tx.send(Ok(ClipboardResult::Copied(37, 23))).unwrap();
        e.poll_clipboard(&mut d, &mut h);
        assert_eq!(
            e.clipboard_status.as_deref(),
            Some("Copied to clipboard · 37 × 23 px")
        );
        assert_eq!(e.copy_confirmation.as_ref().unwrap().0, "37 × 23 px");
        assert_eq!(d.edit_state(), before);
        assert!(!h.can_undo());
        let (tx, receiver) = std::sync::mpsc::channel();
        e.copy_confirmation = None;
        e.clipboard_job = Some(ClipboardJob {
            receiver,
            copy_started: Some(std::time::Instant::now()),
        });
        tx.send(Err("Clipboard unavailable".into())).unwrap();
        e.poll_clipboard(&mut d, &mut h);
        assert!(e.copy_confirmation.is_none());
        assert!(e.clipboard_status.is_none());
        assert_eq!(e.image_error.as_deref(), Some("Clipboard unavailable"));
    }

    #[test]
    fn cut_copies_before_clearing_masks_and_is_one_portable_undoable_edit() {
        for masked in [false, true] {
            let ctx = egui::Context::default();
            let mut d = blank();
            let pixels = d.image.clone();
            let before = d.edit_state();
            let mut h = EditHistory::new(before.clone());
            let mut e = EditorState {
                area: Some(Rect::from_min_size(
                    egui::pos2(20., 30.),
                    egui::vec2(40., 20.),
                )),
                area_mask: if masked {
                    vec![
                        Point { x: 0., y: 0. },
                        Point { x: 1., y: 0. },
                        Point { x: 0., y: 1. },
                    ]
                } else {
                    vec![]
                },
                pixel_mask: masked.then(|| crate::fill::FillRegion {
                    seed: Point { x: 0.1, y: 0.1 },
                    tolerance: 0,
                    global: true,
                    width: 40,
                    height: 20,
                    runs: Arc::new((0..20).map(|y| [y, 0, 30]).collect()),
                }),
                ..Default::default()
            };
            let copied = crate::image_clipboard::copy_pixels_masked(
                &d,
                e.pixel_area(&d),
                e.pixel_mask.clone(),
            )
            .unwrap();
            e.cut_selection(&ctx, &mut d, &mut h);
            assert!(!e.has_modal());
            assert_eq!(
                d.edit_state(),
                before,
                "Never clear before clipboard success"
            );
            finish_clipboard(&mut e, &mut d, &mut h);
            assert!(e.image_error.is_none(), "{:?}", e.image_error);
            assert!(
                e.clipboard_status
                    .as_deref()
                    .unwrap()
                    .contains("40 × 20 px")
            );
            assert!(e.area.is_none() && e.selected.is_none());
            assert!(e.area_mask.is_empty() && e.pixel_mask.is_none());
            assert!(Arc::ptr_eq(&d.image, &pixels));
            assert_eq!(d.annotations.len(), 1);
            let flat = render::flatten(&d).unwrap();
            for y in 0..600 {
                for x in 0..800 {
                    let removed = (20..60).contains(&x)
                        && (30..50).contains(&y)
                        && copied.get_pixel(x - 20, y - 30)[3] != 0;
                    assert_eq!(flat.get_pixel(x, y)[3], if removed { 0 } else { 255 });
                }
            }
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("cut.myimage");
            document::save_project(&d, &path).unwrap();
            let reopened = document::load_project(&path).unwrap();
            assert_eq!(reopened.edit_state(), d.edit_state());
            assert_eq!(reopened.image, pixels);
            assert_eq!(render::flatten(&reopened).unwrap(), flat);
            assert!(h.undo(&mut d));
            assert_eq!(d.edit_state(), before);
            assert!(!h.can_undo());
            assert!(h.redo(&mut d));
            assert_eq!(render::flatten(&d).unwrap(), flat);
        }
    }

    #[test]
    fn cut_requires_selection_and_does_not_remove_pixels_after_failure_or_intervening_edits() {
        let ctx = egui::Context::default();
        let mut d = blank();
        let mut h = EditHistory::new(d.edit_state());
        let mut e = EditorState::default();
        e.cut_selection(&ctx, &mut d, &mut h);
        assert!(e.clipboard_job.is_none());
        assert!(e.image_error.as_deref().unwrap().contains("Select an area"));
        e.select_all_image(&mut d, &mut h);
        e.cut_selection(&ctx, &mut d, &mut h);
        d.geometry.quarter_turns = 1;
        finish_clipboard(&mut e, &mut d, &mut h);
        assert!(d.annotations.is_empty());
        assert!(
            e.image_error
                .as_deref()
                .unwrap()
                .contains("Nothing was removed")
        );
        assert!(!h.can_undo());
        e.cut_selection(&ctx, &mut d, &mut h);
        let (tx, receiver) = std::sync::mpsc::channel();
        e.clipboard_job.as_mut().unwrap().receiver = receiver;
        tx.send(Err("Clipboard write failed".into())).unwrap();
        finish_clipboard(&mut e, &mut d, &mut h);
        assert!(d.annotations.is_empty());
        assert!(!h.can_undo());
    }

    #[test]
    fn cut_shortcut_uses_the_selected_region() {
        let ctx = egui::Context::default();
        let texture = ctx.load_texture(
            "cut shortcut",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut d = blank();
        let mut h = EditHistory::new(d.edit_state());
        let mut e = EditorState {
            area: Some(Rect::from_min_size(
                egui::pos2(40., 50.),
                egui::vec2(70., 80.),
            )),
            ..Default::default()
        };
        frame(
            &mut e,
            &mut d,
            &mut h,
            &ctx,
            &texture,
            vec![egui::Event::Cut],
        );
        finish_clipboard(&mut e, &mut d, &mut h);
        assert_eq!(d.annotations.len(), 1);
        assert_eq!(d.annotations[0].kind, AnnotationKind::DeleteArea);
        assert!(
            e.clipboard_status
                .as_deref()
                .unwrap()
                .contains("70 × 80 px")
        );
        assert!(e.area.is_none());
        assert!(e.selected.is_none());
    }

    #[test]
    fn typed_selection_dimension_is_used_by_copy_without_pressing_enter() {
        let ctx = egui::Context::default();
        let texture = ctx.load_texture(
            "typed selection",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut d = blank();
        let mut h = EditHistory::new(d.edit_state());
        let mut e = EditorState {
            inspector: inspector_panel::State::pinned(InspectorPanel::Styles),
            area: Some(Rect::from_min_size(
                egui::pos2(20., 30.),
                egui::vec2(300., 200.),
            )),
            ..Default::default()
        };
        let center = |output: &egui::FullOutput, label: &str| {
            output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.job.text == label => {
                        Some(t.pos + t.galley.size() * 0.5)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("Missing control: {label}"))
        };
        let output = frame(&mut e, &mut d, &mut h, &ctx, &texture, vec![]);
        let at = center(&output, "300 px");
        for pressed in [true, false] {
            frame(
                &mut e,
                &mut d,
                &mut h,
                &ctx,
                &texture,
                vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::PointerButton {
                        pos: at,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        frame(
            &mut e,
            &mut d,
            &mut h,
            &ctx,
            &texture,
            vec![key(Key::A, Modifiers::COMMAND)],
        );
        let output = frame(
            &mut e,
            &mut d,
            &mut h,
            &ctx,
            &texture,
            vec![egui::Event::Text("120".into())],
        );
        assert_eq!(
            crate::image_clipboard::copy_bounds(&d, e.area.unwrap())
                .unwrap()
                .width,
            120
        );
        let at = center(&output, "Copy");
        for pressed in [true, false] {
            frame(
                &mut e,
                &mut d,
                &mut h,
                &ctx,
                &texture,
                vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::PointerButton {
                        pos: at,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        finish_clipboard(&mut e, &mut d, &mut h);
        assert_eq!(
            e.clipboard_status.as_deref(),
            Some("Copied to clipboard · 120 × 200 px")
        );
        assert!(!h.can_undo());
    }

    fn blank() -> Document {
        Document {
            image: Arc::new(image::RgbaImage::from_pixel(
                800,
                600,
                image::Rgba([245, 248, 252, 255]),
            )),
            annotations: Vec::new(),
            geometry: Default::default(),
        }
    }

    fn fixture() -> Document {
        let mut d = blank();
        for (i, kind) in [
            AnnotationKind::Text,
            AnnotationKind::Arrow,
            AnnotationKind::Rectangle,
            AnnotationKind::Ellipse,
            AnnotationKind::Step,
            AnnotationKind::Callout,
            AnnotationKind::Highlighter,
            AnnotationKind::Freehand,
            AnnotationKind::Blur,
            AnnotationKind::Pixelate,
        ]
        .into_iter()
        .enumerate()
        {
            let x = 30. + (i % 5) as f32 * 100.;
            let y = 40. + (i / 5) as f32 * 160.;
            let mut a = object(kind, [x, y], [x + 60., y + 55.]);
            a.layer.name = format!("Annotation {}", i + 1);
            if kind == AnnotationKind::Arrow {
                a.arrow.curve = Some([
                    Point {
                        x: x + 20.,
                        y: y - 20.,
                    },
                    Point {
                        x: x + 40.,
                        y: y + 75.,
                    },
                ]);
            }
            if kind == AnnotationKind::Text || kind == AnnotationKind::Callout {
                a.text = "Editable ✓".into();
            }
            if kind == AnnotationKind::Freehand {
                a.points = vec![
                    a.start,
                    Point {
                        x: x + 25.,
                        y: y + 10.,
                    },
                    a.end,
                ];
            }
            if i < 3 {
                a.group_id = Some(1);
                a.group_layer.name = "Team notes".into();
            }
            if i == 4 {
                a.layer.locked = true;
            }
            if i == 7 {
                a.layer.hidden = true;
            }
            d.annotations.push(a);
        }
        d
    }

    #[test]
    fn raster_copy_uses_only_an_explicit_pixel_selection() {
        let mut d = blank();
        d.annotations
            .push(object(AnnotationKind::Magnify, [20., 20.], [100., 100.]));
        let mut e = EditorState::default();
        e.select(Some(0), &d);
        assert!(
            e.pixel_area(&d).is_none(),
            "An object selection must still copy the whole image"
        );
        let rect = Rect::from_min_max(egui::pos2(10., 15.), egui::pos2(70., 90.));
        e.area = Some(rect);
        assert_eq!(e.pixel_area(&d).unwrap().0, rect);
        assert_eq!(
            crate::image_clipboard::copy_pixels(&d, None)
                .unwrap()
                .dimensions(),
            (800, 600)
        );
        assert_eq!(
            crate::image_clipboard::copy_pixels(&d, e.pixel_area(&d))
                .unwrap()
                .dimensions(),
            (60, 75)
        );
    }

    #[test]
    fn small_annotations_can_be_inserted_above_a_large_pasted_image() {
        let image = image::RgbaImage::from_fn(800, 1600, |x, y| {
            let n = x
                .wrapping_mul(1973)
                .wrapping_add(y.wrapping_mul(9277))
                .wrapping_mul(26699);
            image::Rgba([
                (n % 251) as u8,
                ((n >> 8) % 251) as u8,
                ((n >> 16) % 251) as u8,
                255,
            ])
        });
        let source = crate::stamps::StampSource::from_pixels(&image).unwrap();
        assert!(source.stored_bytes() > MAX_BYTES);
        let mut destination = blank();
        let mut raster = object(AnnotationKind::Stamp, [0., 0.], [100., 200.]);
        raster.stamp = Some(source);
        destination.annotations.push(raster);
        let arrow = object(AnnotationKind::Arrow, [10., 10.], [50., 50.]);
        assert!(prepare_insertion(vec![arrow], &destination).is_ok());
    }

    #[test]
    fn new_shape_groups_survive_clipboard_and_preset_storage() {
        let mut source = blank();
        source.annotations = crate::shapes::CHOICES
            .iter()
            .chain(crate::shapes::THREE_D.iter())
            .map(|(kind, name)| {
                let mut a = object(*kind, [20., 30.], [100., 110.]);
                a.fill = true;
                a.group_id = Some(42);
                a.group_layer.name = "Shapes".into();
                a.layer.name = (*name).into();
                a
            })
            .collect();
        let text = encode(&source, &(0..17).collect::<Vec<_>>()).unwrap();
        assert_eq!(decode(&text).unwrap(), source.annotations);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shapes.json");
        crate::presets::change(
            &path,
            crate::presets::Change::Add {
                name: "Shape kit".into(),
                annotations: decode(&text).unwrap(),
            },
        )
        .unwrap();
        let stored = crate::presets::load(&path).unwrap();
        assert_eq!(stored.entries[0].annotations, source.annotations);
        let inserted = prepare_insertion(stored.entries[0].annotations.clone(), &blank()).unwrap();
        assert_eq!(inserted.len(), 17);
        for (a, b) in inserted.iter().zip(&source.annotations) {
            assert_eq!(a.kind, b.kind);
            assert_eq!(a.fill, b.fill);
            assert_eq!(a.color, b.color);
            assert_eq!(a.layer, b.layer);
        }
    }

    #[test]
    fn svg_source_and_prompt_survive_copy_paste() {
        let mut doc = blank();
        let mut stamp = object(AnnotationKind::Stamp, [20., 20.], [180., 180.]);
        stamp.stamp = Some(crate::stamps::StampSource::Svg {
            name: "Approval".into(),
            svg: Arc::new(crate::svg_stamp::STARTER.into()),
            prompt: "A teal approval seal".into(),
        });
        doc.annotations.push(stamp.clone());
        let text = encode(&doc, &[0]).unwrap();
        let pasted = pasted_annotations(&text, &blank()).unwrap();
        assert_eq!(pasted[0].stamp, stamp.stamp);
    }

    #[test]
    fn copy_is_an_independent_snapshot_with_every_editable_style_and_no_pixels() {
        let mut source = fixture();
        let original = source.edit_state();
        let text = encode(&source, &(0..10).collect::<Vec<_>>()).unwrap();
        assert_eq!(decode(&text).unwrap(), source.annotations);
        assert!(!text.contains("image") && !text.contains("geometry"));
        source.annotations[0].text = "Changed after copy".into();
        source.annotations.clear();
        assert_eq!(decode(&text).unwrap(), original.annotations);
    }

    #[test]
    fn paste_preserves_styles_points_tails_and_flags_with_fresh_groups_each_time() {
        let source = fixture();
        let text = encode(&source, &(0..10).collect::<Vec<_>>()).unwrap();
        let mut d = fixture();
        let original = d.edit_state();
        let mut e = EditorState::default();
        let mut h = EditHistory::new(original.clone());
        e.paste_annotations(&text, &mut d, &mut h);
        let delta = point(d.annotations[10].start) - point(source.annotations[0].start);
        for i in 0..10 {
            let mut expected = offset_annotation(&source.annotations[i], delta);
            if expected.group_id.is_some() {
                expected.group_id = Some(2);
            }
            assert_eq!(d.annotations[10 + i], expected);
        }
        assert!(e.inspector.is(InspectorPanel::Layers));
        assert_eq!(e.selected_indices(&d), (10..20).collect::<Vec<_>>());
        e.paste_annotations(&text, &mut d, &mut h);
        assert_eq!(d.annotations[20].group_id, Some(3));
        assert_eq!(&d.annotations[..10], &original.annotations);
        assert!(h.undo(&mut d));
        assert_eq!(d.annotations.len(), 20);
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), original);
        assert!(!h.undo(&mut d));
        assert!(h.redo(&mut d));
        assert_eq!(d.annotations.len(), 20);
    }

    #[test]
    fn individual_members_copy_without_unselected_siblings_and_retain_inherited_flags() {
        let mut d = fixture();
        for a in &mut d.annotations[..3] {
            a.group_layer.hidden = true;
            a.group_layer.locked = true;
        }
        let one = decode(&encode(&d, &[1]).unwrap()).unwrap();
        assert_eq!(one.len(), 1);
        assert!(one[0].group_id.is_none());
        assert!(!one[0].visible());
        assert!(one[0].locked());
        assert_eq!(one[0].layer.name, "Annotation 2");
        let subset = decode(&encode(&d, &[2, 0, 2]).unwrap()).unwrap();
        assert_eq!(subset.len(), 2);
        assert_eq!(subset[0], d.annotations[0]);
        assert_eq!(subset[1], d.annotations[2]);
        // A complete group that happens to contain only one surviving object stays a group.
        d.annotations.remove(2);
        d.annotations.remove(1);
        assert_eq!(
            decode(&encode(&d, &[0]).unwrap()).unwrap()[0].group_id,
            Some(1)
        );
    }

    #[test]
    fn centered_in_each_destination_crop_without_changing_pixel_sizes() {
        let source = fixture();
        let text = encode(&source, &[1, 2]).unwrap();
        for turns in 0..4 {
            let mut d = blank();
            d.geometry = ImageGeometry {
                effects: Default::default(),
                viewport: None,
                flip_horizontal: false,
                flip_vertical: false,
                canvas: None,
                cutouts: [None; 16],
                crop: Some(CropRect {
                    x: 200,
                    y: 150,
                    width: 300,
                    height: 200,
                }),
                quarter_turns: turns,
                resize: Some([180, 400]),
            };
            let pasted = pasted_annotations(&text, &d).unwrap();
            let bounds = pasted
                .iter()
                .map(render::annotation_bounds)
                .fold(Rect::NOTHING, Rect::union);
            let center = d
                .geometry
                .map_point(from_pos(bounds.center()), original_size(&d));
            assert!((center.x - 90.).abs() < 0.01 && (center.y - 200.).abs() < 0.01);
            assert_eq!(pasted[0].stroke_width, source.annotations[1].stroke_width);
            assert_eq!(
                point(pasted[0].end) - point(pasted[0].start),
                point(source.annotations[1].end) - point(source.annotations[1].start)
            );
        }
    }

    #[test]
    fn invalid_foreign_oversized_and_inconsistent_payloads_leave_destination_and_undo_untouched() {
        let source = fixture();
        let valid = encode(&source, &[0, 1, 2]).unwrap();
        let mut d = blank();
        let original = d.edit_state();
        let mut e = EditorState::default();
        let mut h = EditHistory::new(original.clone());
        let mut malformed =
            serde_json::from_str::<serde_json::Value>(valid.strip_prefix(PREFIX).unwrap()).unwrap();
        malformed["annotations"][0]["group_layer"]["name"] = "Mismatch".into();
        let inputs = vec![
            "ordinary text".into(),
            format!("{PREFIX}{{"),
            valid.replacen("\"version\":1", "\"version\":999", 1),
            format!("{PREFIX}{{\"version\":1,\"annotations\":[]}}"),
            "x".repeat(MAX_BYTES + PREFIX.len() + 1),
            format!("{PREFIX}{malformed}"),
        ];
        for text in inputs {
            e.paste_annotations(&text, &mut d, &mut h);
            assert!(e.image_error.is_some());
            assert_eq!(d.edit_state(), original);
            assert!(!h.undo(&mut d));
        }
    }

    #[test]
    fn paste_respects_combined_annotation_and_stroke_budgets() {
        let source = fixture();
        let mut d = blank();
        d.annotations = vec![source.annotations[3].clone(); document::MAX_ANNOTATIONS];
        assert!(
            pasted_annotations(&encode(&source, &[3]).unwrap(), &d)
                .unwrap_err()
                .contains("10,000")
        );
        let mut stroke = source.annotations[7].clone();
        stroke.points = vec![stroke.start; document::MAX_STROKE_POINTS];
        d.annotations =
            vec![stroke; document::MAX_TOTAL_STROKE_POINTS / document::MAX_STROKE_POINTS];
        assert!(
            pasted_annotations(&encode(&source, &[7]).unwrap(), &d)
                .unwrap_err()
                .contains("freehand points")
        );
    }

    #[test]
    fn pasted_objects_save_reopen_and_remain_independently_editable() {
        let source = fixture();
        let text = encode(&source, &(0..10).collect::<Vec<_>>()).unwrap();
        let mut d = blank();
        let pixels = d.image.clone();
        let mut e = EditorState::default();
        let mut h = EditHistory::new(d.edit_state());
        e.reset();
        e.paste_annotations(&text, &mut d, &mut h);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pasted.myimage");
        document::save_project(&d, &path).unwrap();
        let mut reopened = document::load_project(&path).unwrap();
        assert_eq!(reopened.annotations, d.annotations);
        assert_eq!(reopened.image, pixels);
        reopened.annotations[0].text = "Changed in destination".into();
        reopened.annotations[4].layer.locked = false;
        document::save_project(&reopened, &path).unwrap();
        assert_eq!(
            document::load_project(&path).unwrap().annotations,
            reopened.annotations
        );
        assert_eq!(source.annotations[0].text, "Editable ✓");
        assert!(render::flatten(&reopened).is_ok());
    }

    fn frame(
        e: &mut EditorState,
        d: &mut Document,
        h: &mut EditHistory,
        ctx: &egui::Context,
        texture: &egui::TextureHandle,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.enable_accesskit();
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280., 1000.))),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::TopBottomPanel::bottom("test-image-controls").show(ctx, |ui| {
                    ui.horizontal(|ui| e.image_controls(ui, d, h));
                });
                egui::CentralPanel::default()
                    .show(ctx, |ui| e.ui(ui, d, h, texture, &mut Default::default()));
            },
        )
    }

    fn key(key: Key, modifiers: Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn select_all_shortcut_and_escape_work_without_prior_canvas_focus() {
        let ctx = egui::Context::default();
        let texture = ctx.load_texture(
            "select all",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut d = fixture();
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState::default();
        e.set_selection(vec![0, 1], &d);
        frame(
            &mut e,
            &mut d,
            &mut h,
            &ctx,
            &texture,
            vec![key(Key::A, Modifiers::COMMAND)],
        );
        assert_eq!(
            e.area,
            Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.)))
        );
        assert!(e.selected_indices(&d).is_empty());
        frame(
            &mut e,
            &mut d,
            &mut h,
            &ctx,
            &texture,
            vec![egui::Event::Copy],
        );
        let result = e
            .clipboard_job
            .take()
            .unwrap()
            .receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert!(matches!(result, ClipboardResult::Copied(800, 600)));
        frame(
            &mut e,
            &mut d,
            &mut h,
            &ctx,
            &texture,
            vec![key(Key::Escape, Modifiers::NONE)],
        );
        assert!(e.area.is_none());
        assert!(e.area_mask.is_empty());
        assert_eq!(d.edit_state(), before);
        assert!(!h.can_undo());
    }

    #[test]
    fn select_all_in_text_field_keeps_native_text_selection() {
        let ctx = egui::Context::default();
        let texture = ctx.load_texture(
            "text select all",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut d = blank();
        let mut h = EditHistory::new(d.edit_state());
        let mut e = EditorState::default();
        let mut text = String::from("Select this text");
        for events in [
            vec![],
            vec![key(Key::A, Modifiers::COMMAND)],
            vec![egui::Event::Text("Replaced".into())],
        ] {
            let _ = ctx.run(
                egui::RawInput {
                    events,
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280., 1000.))),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.add(egui::TextEdit::singleline(&mut text).id_salt("select-all-text"))
                            .request_focus();
                        e.ui(ui, &mut d, &mut h, &texture, &mut Default::default());
                    });
                },
            );
        }
        assert_eq!(text, "Replaced");
        assert!(e.area.is_none());
        assert!(!h.can_undo());
    }

    fn click(
        label: &str,
        e: &mut EditorState,
        d: &mut Document,
        h: &mut EditHistory,
        ctx: &egui::Context,
        texture: &egui::TextureHandle,
    ) -> egui::FullOutput {
        if matches!(
            label,
            "Copy" | "Copy editable annotations" | "Paste" | "Undo" | "Redo"
        ) {
            click("Image tools · crop, resize, rotate", e, d, h, ctx, texture);
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
            .or_else(|| {
                output
                    .platform_output
                    .accesskit_update
                    .as_ref()
                    .and_then(|update| {
                        update.nodes.iter().find_map(|(_, n)| {
                            (n.label() == Some(label)).then(|| n.bounds()).flatten()
                        })
                    })
                    .map(|r| Pos2::new(((r.x0 + r.x1) * 0.5) as f32, ((r.y0 + r.y1) * 0.5) as f32))
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
    fn real_copy_paste_buttons_and_native_events_work_after_switching_images() {
        let ctx = egui::Context::default();
        crate::appearance::apply(&ctx);
        let texture = ctx.load_texture(
            "clipboard UI",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut source = fixture();
        let original = source.edit_state();
        let mut h = EditHistory::new(original.clone());
        let mut e = EditorState::default();
        e.set_selection(vec![0, 1, 2], &source);
        let output = click(
            "Copy editable annotations",
            &mut e,
            &mut source,
            &mut h,
            &ctx,
            &texture,
        );
        let text = output
            .platform_output
            .commands
            .iter()
            .find_map(|command| match command {
                egui::OutputCommand::CopyText(text) => Some(text.clone()),
                _ => None,
            })
            .expect("Copy writes a native clipboard payload");
        assert_eq!(source.edit_state(), original);
        assert!(!h.can_undo());
        let mut destination = blank();
        e.reset();
        h = EditHistory::new(destination.edit_state());
        let request = click("Paste", &mut e, &mut destination, &mut h, &ctx, &texture);
        assert!(request.viewport_output.values().any(|v| {
            v.commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::RequestPaste))
        }));
        frame(
            &mut e,
            &mut destination,
            &mut h,
            &ctx,
            &texture,
            vec![egui::Event::Paste(text.clone())],
        );
        assert_eq!(destination.annotations.len(), 3);
        assert_eq!(e.selected_indices(&destination), vec![0, 1, 2]);
        assert_eq!(destination.annotations[0].group_layer.name, "Team notes");
        let copy = frame(
            &mut e,
            &mut destination,
            &mut h,
            &ctx,
            &texture,
            vec![egui::Event::Copy],
        );
        assert!(
            copy.platform_output
                .commands
                .iter()
                .all(|c| !matches!(c, egui::OutputCommand::CopyText(_)))
        );
        assert!(e.clipboard_job.is_some());
        e.clipboard_job = None; // Native raster delivery is exercised in app QA.
        click("Undo", &mut e, &mut destination, &mut h, &ctx, &texture);
        assert!(destination.annotations.is_empty());
        click("Redo", &mut e, &mut destination, &mut h, &ctx, &texture);
        assert_eq!(destination.annotations.len(), 3);
        // No canvas focus is required in a newly installed destination.
        e.reset();
        ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
        frame(
            &mut e,
            &mut destination,
            &mut h,
            &ctx,
            &texture,
            vec![egui::Event::Paste(text)],
        );
        assert_eq!(destination.annotations.len(), 6);
        assert_ne!(
            destination.annotations[0].group_id,
            destination.annotations[3].group_id
        );
    }

    #[test]
    fn text_field_paste_and_modal_input_do_not_paste_annotations() {
        let ctx = egui::Context::default();
        crate::appearance::apply(&ctx);
        let texture = ctx.load_texture(
            "text focus",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut d = fixture();
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState::default();
        let text = encode(&d, &[0, 1, 2]).unwrap();
        let mut field = String::new();
        for events in [
            vec![],
            vec![egui::Event::Paste("ordinary text".into())],
            vec![egui::Event::Paste(text.clone())],
        ] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280., 1000.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut field).id_salt("clipboard-text-test"),
                        )
                        .request_focus();
                        e.ui(ui, &mut d, &mut h, &texture, &mut Default::default());
                    });
                },
            );
        }
        assert!(field.contains("ordinary text"));
        assert!(field.contains("MYIMAGE-ANNOTATIONS"));
        assert_eq!(d.edit_state(), before);
        assert!(!h.can_undo());
        assert!(e.image_error.is_none());
        e.resize = Some(ResizeDraft {
            size: [800, 600],
            ratio: 4. / 3.,
            locked: true,
        });
        frame(
            &mut e,
            &mut d,
            &mut h,
            &ctx,
            &texture,
            vec![egui::Event::Paste(text)],
        );
        assert_eq!(d.edit_state(), before);
    }

    #[test]
    #[ignore = "manual native clipboard review fixtures"]
    fn clipboard_review_fixtures() {
        let dir = std::path::PathBuf::from(std::env::var("MYIMAGE_CLIPBOARD_REVIEW_DIR").unwrap());
        std::fs::create_dir_all(&dir).unwrap();
        let mut source = fixture();
        source.annotations.truncate(3);
        document::save_project(&source, &dir.join("copy-source.myimage")).unwrap();
        document::save_project(&blank(), &dir.join("paste-destination.myimage")).unwrap();
    }
}
