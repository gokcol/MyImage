//! Capture delivery is independent of the open document and its undo/recovery state.
use super::*;
use crate::capture::CaptureDestination;

#[derive(Clone)]
pub(super) struct CaptureOutput {
    pub image: Arc<image::RgbaImage>,
    pub path: Result<PathBuf, String>,
    pub destination: CaptureDestination,
    pub clipboard: Result<(), String>,
}

impl CaptureOutput {
    fn prepare(
        image: image::RgbaImage,
        folder: &std::path::Path,
        at: DateTime<Local>,
        destination: CaptureDestination,
        copy: impl FnOnce(&image::RgbaImage) -> Result<(), String>,
    ) -> Self {
        // Publish the clipboard first, so slow disk storage never delays pasting.
        // Either operation may succeed independently of the other.
        let clipboard = if destination == CaptureDestination::Clipboard {
            copy(&image)
        } else {
            Ok(())
        };
        let path = storage::save_capture(&image, folder, at);
        Self {
            image: Arc::new(image),
            path,
            destination,
            clipboard,
        }
    }
}

impl MyImageApp {
    pub(super) fn save_captured_image(
        &mut self,
        ctx: &egui::Context,
        image: image::RgbaImage,
        at: DateTime<Local>,
    ) {
        self.queue_capture_output(ctx, at, move || Ok(image));
    }

    pub(super) fn queue_capture_output(
        &mut self,
        ctx: &egui::Context,
        at: DateTime<Local>,
        prepare: impl FnOnce() -> Result<image::RgbaImage, String> + Send + 'static,
    ) {
        self.busy = true;
        self.error = None;
        self.status = match self.settings.capture_destination {
            CaptureDestination::Editor => "Saving capture…",
            CaptureDestination::Clipboard => "Copying capture…",
        }
        .into();
        // Never briefly display the Editor between the selection overlay and
        // clipboard completion. The tray remains available to reopen MyImage.
        self.restore_window(ctx, self.desktop.is_some());
        let folder = self.settings.output_dir.clone();
        let destination = self.settings.capture_destination;
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = prepare().map(|image| {
                CaptureOutput::prepare(
                    image,
                    &folder,
                    at,
                    destination,
                    crate::image_clipboard::write_image,
                )
            });
            let _ = tx.send(WorkEvent::CaptureFinished(result));
            ctx.request_repaint();
        });
    }

    pub(super) fn finish_capture_output(
        &mut self,
        ctx: &egui::Context,
        result: Result<CaptureOutput, String>,
    ) {
        self.busy = false;
        match result {
            Err(error) => {
                self.error = Some(error);
                self.view = View::Capture;
                self.restore_window(ctx, false);
            }
            Ok(output) => {
                if let Ok(path) = &output.path {
                    self.library.refresh();
                    self.remember_recent(path.clone());
                }
                if output.destination == CaptureDestination::Editor {
                    self.open_captured_image(ctx, output);
                } else if let Err(error) = &output.clipboard {
                    self.error = Some(format!(
                        "{error}. Your capture is retained. Retry copy or open it in Editor below."
                    ));
                    self.capture_copy_retry = Some(output);
                    self.view = View::Capture;
                    self.status = "Capture ready · Clipboard copy failed".into();
                    self.show(ctx);
                } else {
                    self.capture_copy_retry = None;
                    self.status = format!(
                        "Copied to clipboard · {} × {} px · Ready to paste",
                        output.image.width(),
                        output.image.height(),
                    );
                    self.error = output.path.err().map(|error| {
                        format!("Capture copied, but the Library copy could not be saved: {error}")
                    });
                    if let Some(desktop) = &mut self.desktop {
                        desktop.capture_feedback(&self.status);
                    }
                    // Keep document, paths, view, undo and recovery completely intact.
                    if self.desktop.is_none() {
                        self.show(ctx);
                    }
                }
            }
        }
        if std::mem::take(&mut self.quit_after_work) {
            self.request_quit(ctx);
        }
    }

    pub(super) fn open_captured_image(&mut self, ctx: &egui::Context, output: CaptureOutput) {
        if !self.can_replace_image(ctx, PendingAction::OpenCapture(output.clone())) {
            return;
        }
        self.capture_copy_retry = None;
        self.install_document(
            Document {
                image: output.image,
                annotations: Vec::new(),
                geometry: Default::default(),
            },
            None,
        );
        self.last_texture = None;
        self.fit = true;
        match output.path {
            Ok(path) => {
                self.last_path = Some(path);
                self.status = "Capture saved".into();
                self.error = None;
            }
            Err(error) => {
                self.last_path = None;
                self.status = "Capture is in memory — save failed".into();
                self.error = Some(error);
            }
        }
        self.view = View::Editor;
        self.show(ctx);
    }

    pub(super) fn retry_capture_copy(&mut self, ctx: &egui::Context) {
        let Some(mut output) = self.capture_copy_retry.take() else {
            return;
        };
        self.busy = true;
        self.error = None;
        self.status = "Copying capture…".into();
        if self.desktop.is_some() {
            self.hide(ctx);
        }
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            output.clipboard = crate::image_clipboard::write_image(&output.image);
            let _ = tx.send(WorkEvent::CaptureFinished(Ok(output)));
            ctx.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixels() -> image::RgbaImage {
        image::RgbaImage::from_fn(83, 51, |x, y| {
            image::Rgba([x as u8, y as u8, 177, if x < 3 { 0 } else { 255 }])
        })
    }

    fn dirty_editor(ctx: &egui::Context) -> MyImageApp {
        let mut app = MyImageApp::with_context(ctx, false, Arc::new(AtomicBool::new(false)), true);
        let doc = Document {
            image: Arc::new(image::RgbaImage::new(30, 20)),
            annotations: Vec::new(),
            geometry: Default::default(),
        };
        app.install_document_state(doc, Some(PathBuf::from("Working.myimage")));
        app.last_path = Some(PathBuf::from("Working.myimage"));
        app.view = View::Editor;
        let doc = app.document.as_mut().unwrap();
        let before = doc.edit_state();
        doc.geometry.flip_horizontal = true;
        app.history.commit(before, doc);
        assert!(app.has_unsaved_edits());
        app
    }

    #[test]
    fn destinations_round_trip_and_legacy_settings_keep_editor_default() {
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.capture_destination, CaptureDestination::Editor);
        let settings = Settings {
            capture_destination: CaptureDestination::Clipboard,
            ..settings
        };
        let reopened: Settings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(reopened.capture_destination, CaptureDestination::Clipboard);
    }

    #[test]
    fn clipboard_gets_exact_pixels_before_library_save_and_editor_does_not_copy() {
        let dir = tempfile::tempdir().unwrap();
        let expected = pixels();
        let output = CaptureOutput::prepare(
            expected.clone(),
            dir.path(),
            Local::now(),
            CaptureDestination::Clipboard,
            |image| {
                assert_eq!(*image, expected);
                assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
                Ok(())
            },
        );
        assert!(output.clipboard.is_ok());
        assert_eq!(
            image::open(output.path.unwrap()).unwrap().into_rgba8(),
            expected
        );
        let output = CaptureOutput::prepare(
            pixels(),
            dir.path(),
            Local::now(),
            CaptureDestination::Editor,
            |_| panic!("Editor capture must not overwrite the user's clipboard"),
        );
        assert!(output.path.is_ok());
    }

    #[test]
    fn clipboard_completion_preserves_dirty_document_paths_and_undo_even_if_disk_fails() {
        let ctx = egui::Context::default();
        let mut app = dirty_editor(&ctx);
        let old_doc = app.document.clone().unwrap();
        let revision = app.history.revision();
        let dir = tempfile::tempdir().unwrap();
        let blocked = dir.path().join("not-a-folder");
        std::fs::write(&blocked, b"occupied").unwrap();
        let output = CaptureOutput::prepare(
            pixels(),
            &blocked,
            Local::now(),
            CaptureDestination::Clipboard,
            |_| Ok(()),
        );
        assert!(output.path.is_err());
        app.work_tx
            .send(WorkEvent::CaptureFinished(Ok(output)))
            .unwrap();
        app.handle_events(&ctx);
        assert!(Arc::ptr_eq(
            &app.document.as_ref().unwrap().image,
            &old_doc.image
        ));
        assert_eq!(
            app.document.as_ref().unwrap().edit_state(),
            old_doc.edit_state()
        );
        assert_eq!(app.history.revision(), revision);
        assert_eq!(app.project_path, Some(PathBuf::from("Working.myimage")));
        assert_eq!(app.last_path, app.project_path);
        assert!(app.view == View::Editor);
        assert!(app.pending_action.is_none());
        assert!(app.status.starts_with("Copied to clipboard · 83 × 51"));
        assert!(app.error.as_deref().unwrap().contains("Library copy"));
        assert!(app.history.undo(app.document.as_mut().unwrap()));
        assert!(!app.document.as_ref().unwrap().geometry.flip_horizontal);
    }

    #[test]
    fn failed_clipboard_retains_capture_and_successful_retry_preserves_original() {
        let ctx = egui::Context::default();
        let mut app = dirty_editor(&ctx);
        let original = app.document.clone().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let output = CaptureOutput::prepare(
            pixels(),
            dir.path(),
            Local::now(),
            CaptureDestination::Clipboard,
            |_| Err("Clipboard busy".into()),
        );
        let path = output.path.clone().unwrap();
        app.finish_capture_output(&ctx, Ok(output));
        assert!(app.view == View::Capture);
        assert!(app.error.as_deref().unwrap().contains("Clipboard busy"));
        assert_eq!(*app.capture_copy_retry.as_ref().unwrap().image, pixels());
        assert!(app.settings.recent_files.contains(&path));
        assert_eq!(
            app.document.as_ref().unwrap().edit_state(),
            original.edit_state()
        );
        let mut retry = app.capture_copy_retry.take().unwrap();
        retry.clipboard = Ok(());
        app.finish_capture_output(&ctx, Ok(retry));
        assert!(app.capture_copy_retry.is_none());
        assert!(app.error.is_none());
        assert!(app.status.starts_with("Copied to clipboard"));
        assert_eq!(
            app.document.as_ref().unwrap().edit_state(),
            original.edit_state()
        );
    }

    #[test]
    fn editor_capture_defers_replacement_until_unsaved_work_is_resolved() {
        let ctx = egui::Context::default();
        let mut app = dirty_editor(&ctx);
        let original = app.document.clone().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let output = CaptureOutput::prepare(
            pixels(),
            dir.path(),
            Local::now(),
            CaptureDestination::Editor,
            |_| unreachable!(),
        );
        app.finish_capture_output(&ctx, Ok(output));
        assert_eq!(
            app.document.as_ref().unwrap().edit_state(),
            original.edit_state()
        );
        let action = app
            .pending_action
            .take()
            .expect("Save prompt before replacing dirty project");
        assert!(matches!(action, PendingAction::OpenCapture(_)));
        app.perform_action(&ctx, action, true);
        assert_eq!(*app.document.as_ref().unwrap().image, pixels());
        assert!(app.project_path.is_none());
        assert!(app.last_path.as_ref().unwrap().is_file());
        assert!(!app.has_unsaved_edits());
    }
}
