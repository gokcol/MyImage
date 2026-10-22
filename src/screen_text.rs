//! Ephemeral screen OCR. It never creates a project or replaces the editor.
use super::*;
use std::time::Instant;

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    job: Option<Job>,
    pixels: Option<Arc<image::RgbaImage>>,
    text: String,
    error: Option<String>,
    focus: bool,
}

struct Job {
    receiver: mpsc::Receiver<Event>,
    cancel: Arc<AtomicBool>,
    started: Instant,
}
enum Event {
    Prepared(Arc<image::RgbaImage>),
    Finished(Result<String, String>),
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("screen-text-result")
}

impl State {
    fn start(
        &mut self,
        ctx: &egui::Context,
        prepare: impl FnOnce() -> Result<image::RgbaImage, String> + Send + 'static,
    ) {
        self.open = true;
        self.error = None;
        self.text.clear();
        self.focus = true;
        let cancel = Arc::new(AtomicBool::new(false));
        let canceled = cancel.clone();
        let (tx, receiver) = mpsc::channel();
        self.job = Some(Job {
            receiver,
            cancel,
            started: Instant::now(),
        });
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = prepare().and_then(|pixels| {
                if canceled.load(Ordering::Acquire) {
                    return Err("Text extraction canceled.".into());
                }
                let pixels = Arc::new(pixels);
                let _ = tx.send(Event::Prepared(pixels.clone()));
                crate::ocr::recognize_cancellable((*pixels).clone(), canceled.clone())
            });
            if !canceled.load(Ordering::Acquire) {
                let _ = tx.send(Event::Finished(result));
                ctx.request_repaint();
            }
        });
    }

    fn poll(&mut self) -> Option<Result<String, String>> {
        let job = self.job.as_ref()?;
        loop {
            match job.receiver.try_recv() {
                Ok(Event::Prepared(pixels)) => self.pixels = Some(pixels),
                Ok(Event::Finished(result)) => {
                    self.job = None;
                    return Some(result);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.job = None;
                    return Some(Err("Text recognition stopped. Please try again.".into()));
                }
                Err(mpsc::TryRecvError::Empty) => break,
            }
        }
        if job.started.elapsed() >= crate::ocr::JOB_TIMEOUT + Duration::from_secs(2) {
            self.job = None;
            return Some(Err(
                "Text recognition took too long. Please try again.".into()
            ));
        }
        None
    }
}

impl MyImageApp {
    pub(super) fn queue_screen_text(
        &mut self,
        ctx: &egui::Context,
        prepare: impl FnOnce() -> Result<image::RgbaImage, String> + Send + 'static,
    ) {
        self.error = None;
        self.status = "Reading screen text…".into();
        self.restore_window(ctx, self.desktop.is_some());
        self.screen_text = State::default();
        self.screen_text.start(ctx, prepare);
        ctx.request_repaint();
    }

    fn finish_screen_text(
        &mut self,
        ctx: &egui::Context,
        result: Result<String, String>,
        copy: impl FnOnce(&str) -> Result<(), String>,
    ) {
        match result {
            Ok(text) if text.trim().is_empty() => {
                self.screen_text.error =
                    Some("No text found. Select a sharper area containing upright text.".into());
            }
            Ok(text) => {
                // Keep recognized text for Retry copy if another app locks the clipboard.
                self.screen_text.text = text;
                match copy(&self.screen_text.text) {
                    Ok(()) => {
                        self.close_screen_text(ctx);
                        self.status = "Text copied to clipboard · Ready to paste".into();
                        if self.hidden.load(Ordering::Relaxed)
                            && let Some(desktop) = &self.desktop
                        {
                            desktop.capture_feedback(&self.status);
                        }
                    }
                    Err(error) => self.screen_text.error = Some(error),
                }
            }
            Err(error) => self.screen_text.error = Some(error),
        }
    }

    pub(super) fn close_screen_text(&mut self, ctx: &egui::Context) {
        self.screen_text = State::default();
        ctx.send_viewport_cmd_to(viewport_id(), ViewportCommand::Visible(false));
        ctx.request_repaint();
    }

    pub(super) fn screen_text_ui(&mut self, ctx: &egui::Context) {
        if !self.screen_text.open {
            return;
        }
        let mut close = false;
        let mut retry = false;
        let mut choose_again = false;
        let state = &mut self.screen_text;
        let running = state.job.is_some();
        let builder = egui::ViewportBuilder::default()
            .with_title("Extract screen text")
            .with_inner_size([390., 190.])
            .with_resizable(false)
            .with_taskbar(false)
            .with_window_level(WindowLevel::AlwaysOnTop);
        ctx.show_viewport_immediate(viewport_id(), builder, |child, _| {
            if std::mem::take(&mut state.focus) {
                child.send_viewport_cmd(ViewportCommand::Focus);
            }
            close |=
                child.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape));
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(appearance::palette(child).paper)
                        .inner_margin(16),
                )
                .show(child, |ui| {
                    ui.heading("Extract screen text");
                    ui.weak("On this computer · no image saved");
                    ui.add_space(8.);
                    if running {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Reading your selected region…");
                        });
                        ctx.request_repaint_after(Duration::from_millis(100));
                        child.request_repaint_after(Duration::from_millis(100));
                    } else if let Some(error) = &state.error {
                        egui::ScrollArea::vertical().max_height(64.).show(ui, |ui| {
                            ui.colored_label(dialogs::DANGER, error);
                        });
                    }
                    ui.add_space(8.);
                    ui.horizontal(|ui| {
                        close |= ui
                            .button(if running { "Cancel" } else { "Close" })
                            .clicked();
                        if !running {
                            if !state.text.is_empty() {
                                retry |= ui.button("Retry copy").clicked();
                            } else if state.pixels.is_some() {
                                retry |= ui.button("Retry").clicked();
                            }
                            choose_again |= ui.button("Select another region").clicked();
                        }
                    });
                });
        });
        if close || choose_again {
            self.close_screen_text(ctx);
            self.status = "Text capture canceled".into();
            if choose_again {
                self.settings.capture_mode = capture::CaptureMode::Text;
                self.persist_settings();
                self.begin_capture(ctx);
            }
        } else if retry {
            if self.screen_text.text.is_empty() {
                if let Some(pixels) = self.screen_text.pixels.clone() {
                    self.screen_text.start(ctx, move || Ok((*pixels).clone()));
                }
            } else {
                let text = self.screen_text.text.clone();
                self.finish_screen_text(ctx, Ok(text), copy_text);
            }
        } else if let Some(result) = self.screen_text.poll() {
            // Process this frame's Cancel/Escape before a ready result can
            // overwrite the clipboard.
            self.finish_screen_text(ctx, result, copy_text);
        }
    }
}

fn copy_text(text: &str) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text.to_owned()))
        .map_err(|error| format!("Could not copy text: {error}"))
}

pub(super) fn demo_image() -> image::RgbaImage {
    crate::svg_stamp::raster(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="1000">
        <rect width="1600" height="1000" fill="white"/>
        <g font-family="sans-serif" font-size="48" fill="#172b4d">
          <text x="100" y="320">Selected screen text</text>
          <text x="100" y="420">Merhaba İstanbul 12345</text>
          <text x="100" y="520">Ready to paste</text>
          <text x="1050" y="320">Outside region</text>
        </g></svg>"##,
        1600,
    )
    .expect("bundled text capture demonstration must render")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_only_completion_preserves_project_history_and_library_and_can_retry_copy() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        let dir = tempfile::tempdir().unwrap();
        app.settings.output_dir = dir.path().to_owned();
        app.install_document_state(
            Document {
                image: Arc::new(image::RgbaImage::new(20, 10)),
                annotations: Vec::new(),
                geometry: Default::default(),
            },
            Some(PathBuf::from("Existing.myimage")),
        );
        app.view = View::Editor;
        let doc = app.document.as_mut().unwrap();
        let before = doc.edit_state();
        doc.geometry.flip_horizontal = true;
        app.history.commit(before, doc);
        let before = app.document.as_ref().unwrap().edit_state();
        let revision = app.history.revision();
        let recent = app.settings.recent_files.clone();
        app.screen_text.open = true;
        app.finish_screen_text(&ctx, Ok("Merhaba İstanbul\nSecond line".into()), |_| {
            Err("Clipboard busy".into())
        });
        assert_eq!(app.screen_text.text, "Merhaba İstanbul\nSecond line");
        assert!(
            app.screen_text
                .error
                .as_deref()
                .unwrap()
                .contains("Clipboard busy")
        );
        let text = app.screen_text.text.clone();
        app.finish_screen_text(&ctx, Ok(text), |text| {
            assert_eq!(text, "Merhaba İstanbul\nSecond line");
            Ok(())
        });
        assert!(!app.screen_text.open);
        assert!(app.status.starts_with("Text copied to clipboard"));
        assert_eq!(app.document.as_ref().unwrap().edit_state(), before);
        assert_eq!(app.history.revision(), revision);
        assert_eq!(app.project_path, Some(PathBuf::from("Existing.myimage")));
        assert_eq!(app.settings.recent_files, recent);
        assert!(app.view == View::Editor);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn empty_errors_and_canceled_jobs_never_write_the_clipboard() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.screen_text.open = true;
        for result in [Ok(" \n".into()), Err("Native OCR unavailable".into())] {
            app.finish_screen_text(&ctx, result, |_| panic!("must preserve previous clipboard"));
            assert!(app.screen_text.error.is_some());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, receiver) = mpsc::channel();
        app.screen_text.job = Some(Job {
            receiver,
            cancel: cancel.clone(),
            started: Instant::now(),
        });
        app.close_screen_text(&ctx);
        assert!(cancel.load(Ordering::Acquire));
        assert!(tx.send(Event::Finished(Ok("late result".into()))).is_err());
        assert!(!app.screen_text.open);
    }

    #[test]
    fn stalled_job_times_out_and_text_mode_round_trips_without_changing_destination() {
        let (tx, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let mut state = State {
            open: true,
            job: Some(Job {
                receiver,
                cancel: cancel.clone(),
                started: Instant::now() - crate::ocr::JOB_TIMEOUT - Duration::from_secs(3),
            }),
            ..Default::default()
        };
        assert!(state.poll().unwrap().unwrap_err().contains("too long"));
        assert!(state.job.is_none());
        assert!(cancel.load(Ordering::Acquire));
        drop(tx);
        let settings = Settings {
            capture_mode: capture::CaptureMode::Text,
            capture_destination: capture::CaptureDestination::Editor,
            ..Default::default()
        };
        let restored: Settings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(restored.capture_mode, capture::CaptureMode::Text);
        assert_eq!(
            restored.capture_destination,
            capture::CaptureDestination::Editor
        );
    }
}
