//! Read-only OCR of the visible selection; the clipboard changes only on Copy.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Instant;

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    job: Option<Job>,
    text: String,
    error: Option<String>,
    copied: bool,
    copy_error: Option<String>,
    found_text: bool,
    focus: bool,
}

struct Job {
    receiver: mpsc::Receiver<Result<String, String>>,
    canceled: Arc<AtomicBool>,
    image: Arc<image::RgbaImage>,
    before: EditState,
    started: Instant,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.canceled.store(true, Ordering::Release);
    }
}

impl EditorState {
    fn can_extract_text(&self) -> bool {
        self.area.is_some() && self.clipboard_job.is_none() && !self.text_extraction.open
    }

    pub(super) fn extract_text_button(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) -> bool {
        let clicked = crate::appearance::action_button(
            ui,
            "Extract text…",
            DialogIcon::TextSize,
            self.can_extract_text(),
            false,
        )
        .on_hover_text("Recognize text in the selected visible pixels, then review and copy it")
        .clicked();
        if clicked {
            self.start_text_extraction(ui.ctx(), document, history);
        }
        clicked
    }

    fn start_text_extraction(
        &mut self,
        ctx: &egui::Context,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if !self.can_extract_text() {
            return;
        }
        self.finish_gesture(document, history);
        let Some(area) = self.pixel_area(document) else {
            return;
        };
        let pixel_mask = self
            .selected
            .and_then(|i| document.annotations.get(i))
            .map(|a| a.pixel_mask.clone())
            .unwrap_or_else(|| self.pixel_mask.clone());
        let before = document.edit_state();
        let snapshot = document.clone();
        let canceled = Arc::new(AtomicBool::new(false));
        let cancel = canceled.clone();
        let (tx, receiver) = mpsc::channel();
        self.text_extraction = State {
            open: true,
            job: Some(Job {
                receiver,
                canceled,
                image: document.image.clone(),
                before,
                started: Instant::now(),
            }),
            ..Default::default()
        };
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            if cancel.load(Ordering::Acquire) {
                return;
            }
            let result =
                crate::image_clipboard::copy_pixels_masked(&snapshot, Some(area), pixel_mask)
                    .and_then(|pixels| {
                        if cancel.load(Ordering::Acquire) {
                            return Ok(String::new());
                        }
                        crate::ocr::recognize_cancellable(pixels, cancel.clone())
                    });
            if !cancel.load(Ordering::Acquire) {
                let _ = tx.send(result);
                ctx.request_repaint();
            }
        });
    }

    pub(super) fn poll_text_extraction(&mut self, document: &Document) {
        let Some(job) = &self.text_extraction.job else {
            return;
        };
        if !Arc::ptr_eq(&job.image, &document.image) || job.before != document.edit_state() {
            self.text_extraction = State::default();
            return;
        }
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                if job.started.elapsed() < crate::ocr::JOB_TIMEOUT {
                    return;
                }
                Err("Text recognition took too long. Try again, or select a smaller area.".into())
            }
            Err(_) => Err("Text recognition stopped unexpectedly. Please try again.".into()),
        };
        self.text_extraction.job = None;
        match result {
            Ok(text) => {
                self.text_extraction.found_text = !text.is_empty();
                self.text_extraction.text = text;
                self.text_extraction.focus = true;
            }
            Err(error) => self.text_extraction.error = Some(error),
        }
    }

    pub(super) fn text_extraction_dialog(
        &mut self,
        ctx: &egui::Context,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if !self.text_extraction.open {
            return;
        }
        let state = &mut self.text_extraction;
        let running = state.job.is_some();
        let mut close = false;
        let mut retry = false;
        let response = Dialog::new(
            "Extract text",
            "From your selection · processed on this computer",
            DialogIcon::TextSize,
            520.,
        )
        .modal(ctx, egui::Id::new("extract-selected-text"), |ui| {
            if running {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Reading text in your selection…");
                });
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            } else if let Some(error) = &state.error {
                ui.colored_label(dialogs::DANGER, error);
            } else if !state.found_text {
                ui.label("No text found in this selection.");
                ui.weak("Try selecting a larger, sharper area containing upright text.");
            } else {
                ui.weak("Review or edit the recognized text before copying.");
                let height = (ctx.content_rect().height() - 270.).clamp(70., 260.);
                let response = egui::ScrollArea::vertical()
                    .id_salt("recognized-text-scroll")
                    .max_height(height)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut state.text)
                                .id(egui::Id::new("recognized-text"))
                                .desired_width(f32::INFINITY)
                                .desired_rows(8)
                                .font(egui::TextStyle::Body),
                        )
                    })
                    .inner;
                if std::mem::take(&mut state.focus) {
                    response.request_focus();
                }
                if response.changed() {
                    state.copied = false;
                }
            }
            if let Some(error) = &state.copy_error {
                ui.colored_label(dialogs::DANGER, error);
            }
            dialogs::actions(ui, |ui| {
                close = dialogs::secondary(
                    ui,
                    if running { "Cancel" } else { "Close" },
                    DialogIcon::Close,
                )
                .clicked();
                if !running && (state.error.is_some() || !state.found_text) {
                    retry = dialogs::primary(ui, "Try again", DialogIcon::Refresh).clicked();
                }
                if dialogs::button(
                    ui,
                    "Copy text",
                    DialogIcon::Copy,
                    dialogs::Action::Primary,
                    !running && state.error.is_none() && !state.text.trim().is_empty(),
                )
                .clicked()
                {
                    match write_text(&state.text) {
                        Ok(()) => {
                            state.copied = true;
                            state.copy_error = None;
                        }
                        Err(error) => {
                            state.copied = false;
                            state.copy_error = Some(error);
                        }
                    }
                }
                if state.copied {
                    ui.label("Copied to clipboard");
                }
            });
        });
        if close || response.should_close() {
            self.text_extraction = State::default();
            self.focus_canvas = true;
        } else if retry {
            self.text_extraction = State::default();
            self.start_text_extraction(ctx, document, history);
        }
    }
}

fn write_text(text: &str) -> Result<(), String> {
    #[cfg(not(test))]
    {
        arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.set_text(text.to_owned()))
            .map_err(|error| format!("Cannot copy text: {error}"))
    }
    #[cfg(test)]
    {
        let _ = text;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> Document {
        Document {
            image: Arc::new(image::RgbaImage::from_pixel(
                400,
                200,
                image::Rgba([255; 4]),
            )),
            annotations: vec![],
            geometry: Default::default(),
        }
    }

    fn pending(
        editor: &mut EditorState,
        document: &Document,
    ) -> mpsc::Sender<Result<String, String>> {
        let (tx, receiver) = mpsc::channel();
        editor.text_extraction = State {
            open: true,
            job: Some(Job {
                receiver,
                canceled: Arc::new(AtomicBool::new(false)),
                image: document.image.clone(),
                before: document.edit_state(),
                started: Instant::now(),
            }),
            ..Default::default()
        };
        tx
    }

    #[test]
    fn result_is_reviewed_without_editing_the_project_or_copying_automatically() {
        let document = document();
        let before = document.edit_state();
        let mut editor = EditorState::default();
        let tx = pending(&mut editor, &document);
        assert!(editor.has_modal());
        tx.send(Ok("Merhaba İstanbul\nSecond line 12345".into()))
            .unwrap();
        editor.poll_text_extraction(&document);
        assert!(editor.text_extraction.open);
        assert!(editor.text_extraction.job.is_none());
        assert_eq!(
            editor.text_extraction.text,
            "Merhaba İstanbul\nSecond line 12345"
        );
        assert!(!editor.text_extraction.copied);
        assert_eq!(document.edit_state(), before);
    }

    #[test]
    fn switching_projects_cancels_and_discards_an_in_flight_result() {
        let document = document();
        let mut editor = EditorState::default();
        let tx = pending(&mut editor, &document);
        let canceled = editor
            .text_extraction
            .job
            .as_ref()
            .unwrap()
            .canceled
            .clone();
        tx.send(Ok("Old project text".into())).unwrap();
        let different_document = Document {
            image: Arc::new((*document.image).clone()),
            ..document
        };
        editor.poll_text_extraction(&different_document);
        assert!(!editor.text_extraction.open);
        assert!(editor.text_extraction.text.is_empty());
        assert!(canceled.load(Ordering::Acquire));
    }

    #[test]
    fn empty_and_failed_results_do_not_enable_copy_or_change_document() {
        let document = document();
        let mut editor = EditorState::default();
        let tx = pending(&mut editor, &document);
        tx.send(Ok(String::new())).unwrap();
        editor.poll_text_extraction(&document);
        assert!(!editor.text_extraction.found_text);
        assert!(!editor.text_extraction.copied);
        let tx = pending(&mut editor, &document);
        tx.send(Err("Language unavailable".into())).unwrap();
        editor.poll_text_extraction(&document);
        assert_eq!(
            editor.text_extraction.error.as_deref(),
            Some("Language unavailable")
        );
        assert!(document.annotations.is_empty());
    }

    #[test]
    fn a_worker_that_never_replies_times_out_and_releases_its_pending_receiver() {
        let document = document();
        let before = document.edit_state();
        let mut editor = EditorState {
            area: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(200., 100.))),
            ..Default::default()
        };
        let selection = editor.area;
        let tx = pending(&mut editor, &document);
        let job = editor.text_extraction.job.as_mut().unwrap();
        let canceled = job.canceled.clone();
        job.started = Instant::now() - crate::ocr::JOB_TIMEOUT;

        editor.poll_text_extraction(&document);

        assert!(editor.text_extraction.open);
        assert!(editor.text_extraction.job.is_none());
        assert!(
            editor
                .text_extraction
                .error
                .as_deref()
                .unwrap()
                .contains("took too long")
        );
        assert!(canceled.load(Ordering::Acquire));
        assert!(
            tx.send(Ok("Late text must not replace the timeout".into()))
                .is_err()
        );
        assert_eq!(editor.area, selection);
        assert_eq!(document.edit_state(), before);
        assert!(!editor.text_extraction.copied);
    }

    #[test]
    fn a_queued_result_is_kept_when_the_next_ui_frame_arrives_after_the_deadline() {
        let document = document();
        let before = document.edit_state();
        let mut editor = EditorState::default();
        let tx = pending(&mut editor, &document);
        let text = "A recognized line\n".repeat(200);
        tx.send(Ok(text.clone())).unwrap();
        editor.text_extraction.job.as_mut().unwrap().started =
            Instant::now() - crate::ocr::JOB_TIMEOUT;

        editor.poll_text_extraction(&document);
        editor.poll_text_extraction(&document);

        assert_eq!(editor.text_extraction.text, text);
        assert!(editor.text_extraction.error.is_none());
        assert!(editor.text_extraction.found_text);
        assert!(!editor.text_extraction.copied);
        assert_eq!(document.edit_state(), before);
    }

    #[test]
    fn timed_out_and_empty_results_offer_retry_without_dismissing_the_selection() {
        for error in [Some("Text recognition took too long.".to_string()), None] {
            let ctx = egui::Context::default();
            crate::appearance::apply(&ctx);
            let mut document = document();
            let before = document.edit_state();
            let mut history = EditHistory::new(before.clone());
            let mut editor = EditorState {
                area: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(200., 100.))),
                text_extraction: State {
                    open: true,
                    error,
                    ..Default::default()
                },
                ..Default::default()
            };
            let selection = editor.area;
            let mut retry_visible = false;
            for _ in 0..3 {
                let frame = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(560., 360.))),
                        ..Default::default()
                    },
                    |ctx| editor.text_extraction_dialog(ctx, &mut document, &mut history),
                );
                retry_visible |= frame.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == "Try again")
                });
            }
            assert!(retry_visible);
            assert!(editor.text_extraction.open);
            assert_eq!(editor.area, selection);
            assert_eq!(document.edit_state(), before);
            assert!(!history.can_undo());
        }
    }

    #[test]
    fn escape_cancels_a_pending_worker_and_rejects_late_results() {
        let ctx = egui::Context::default();
        crate::appearance::apply(&ctx);
        let mut document = document();
        let before = document.edit_state();
        let mut history = EditHistory::new(before.clone());
        let mut editor = EditorState {
            area: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(200., 100.))),
            ..Default::default()
        };
        let selection = editor.area;
        let tx = pending(&mut editor, &document);
        let canceled = editor
            .text_extraction
            .job
            .as_ref()
            .unwrap()
            .canceled
            .clone();
        for frame in 0..4 {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(700., 460.))),
                ..Default::default()
            };
            if frame == 3 {
                input.events.push(egui::Event::Key {
                    key: Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                });
            }
            let _ = ctx.run(input, |ctx| {
                editor.text_extraction_dialog(ctx, &mut document, &mut history)
            });
        }
        assert!(!editor.text_extraction.open);
        assert!(canceled.load(Ordering::Acquire));
        assert!(tx.send(Ok("Canceled text".into())).is_err());
        assert_eq!(editor.area, selection);
        assert_eq!(document.edit_state(), before);
        assert!(!history.can_undo());
    }

    #[test]
    fn review_fits_small_windows_and_escape_preserves_selected_pixels() {
        let ctx = egui::Context::default();
        crate::appearance::apply(&ctx);
        let mut document = document();
        let mut history = EditHistory::new(document.edit_state());
        let mut editor = EditorState {
            area: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(200., 100.))),
            text_extraction: State {
                open: true,
                text: "Line of recognized text\n".repeat(100),
                found_text: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let selection = editor.area;
        for frame in 0..4 {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(700., 460.))),
                ..Default::default()
            };
            if frame == 3 {
                input.events.push(egui::Event::Key {
                    key: Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                });
            }
            let _ = ctx.run(input, |ctx| {
                editor.text_extraction_dialog(ctx, &mut document, &mut history)
            });
        }
        assert!(!editor.text_extraction.open);
        assert_eq!(editor.area, selection);
    }

    #[test]
    #[ignore = "exercises native OCR through the selected-region worker and creates a UI fixture"]
    fn selected_region_reads_only_visible_selected_text() {
        let image = crate::svg_stamp::raster(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="360">
            <rect width="1600" height="360" fill="white"/>
            <g font-family="sans-serif" font-size="36" fill="black">
              <text x="40" y="100">Selected region text</text>
              <text x="40" y="190">Second line 12345</text>
              <text x="900" y="100">Outside selection</text>
            </g></svg>"##,
            1600,
        )
        .unwrap();
        let mut document = Document {
            image: Arc::new(image),
            annotations: vec![],
            geometry: Default::default(),
        };
        if let Some(root) = std::env::var_os("MYIMAGE_OCR_REVIEW_DIR") {
            let root = std::path::PathBuf::from(root);
            std::fs::create_dir_all(&root).unwrap();
            document
                .image
                .save(root.join("ocr-selection-fixture.png"))
                .unwrap();
            crate::document::save_project(&document, &root.join("OCR selection review.myimage"))
                .unwrap();
        }
        let before = document.edit_state();
        let ctx = egui::Context::default();
        let mut editor = EditorState {
            area: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(780., 300.))),
            ..Default::default()
        };
        let mut history = EditHistory::new(before.clone());
        editor.start_text_extraction(&ctx, &mut document, &mut history);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while editor.text_extraction.job.is_some() {
            assert!(std::time::Instant::now() < deadline, "OCR worker timed out");
            std::thread::sleep(std::time::Duration::from_millis(20));
            editor.poll_text_extraction(&document);
        }
        assert!(
            editor.text_extraction.error.is_none(),
            "{:?}",
            editor.text_extraction.error
        );
        let text = &editor.text_extraction.text;
        assert!(text.contains("Selected region text"), "{text:?}");
        assert!(text.contains("Second line 12345"), "{text:?}");
        assert!(!text.contains("Outside"), "{text:?}");
        assert!(!editor.text_extraction.copied);
        assert_eq!(document.edit_state(), before);
        assert!(!history.can_undo());
        eprintln!("Selected-region OCR result: {text:?}");
    }
}
