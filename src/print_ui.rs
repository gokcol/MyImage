//! App-owned print lifecycle; edits and replacement remain blocked until completion.
use super::*;

impl MyImageApp {
    pub(super) fn begin_print(&mut self, ctx: &egui::Context) {
        if self.busy
            || self.document.is_none()
            || !self.workspace_unobscured()
            || self.editor.has_modal()
            || self.selection.is_some()
            || self.scroll_session.is_some()
        {
            return;
        }
        self.finish_edit();
        let document = self.document.as_ref().unwrap().clone();
        let title = self
            .project_path
            .as_ref()
            .or(self.last_path.as_ref())
            .and_then(|p| p.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.draft_title.clone());
        let sender = self.work_tx.clone();
        let worker_ctx = ctx.clone();
        match std::thread::Builder::new()
            .name("myimage-print-prepare".into())
            .spawn(move || {
                let result = crate::printing::prepare(&document, title);
                let _ = sender.send(WorkEvent::PrintPrepared(result));
                worker_ctx.request_repaint();
            }) {
            Ok(_) => {
                self.busy = true;
                self.status = "Preparing print preview…".into();
            }
            Err(error) => self.error = Some(format!("Could not prepare printing: {error}")),
        }
    }

    pub(super) fn poll_print(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if let Some(prepared) = self.print_ready.take() {
            match crate::printing::Job::start(ctx, frame, prepared) {
                Ok(job) => {
                    self.print_job = Some(job);
                    self.status = "Choose print settings…".into();
                }
                Err(error) => self.finish_print(ctx, Err(error)),
            }
        }
        if let Some(result) = self.print_job.as_ref().and_then(crate::printing::Job::poll) {
            self.print_job = None;
            self.finish_print(ctx, result);
        }
    }

    pub(super) fn finish_print(&mut self, ctx: &egui::Context, result: crate::printing::Result) {
        self.busy = false;
        match result {
            Ok(crate::printing::Outcome::Submitted) => self.status = "Print job sent".into(),
            Ok(crate::printing::Outcome::NotCompleted) => {
                self.status = "Printing cancelled or not completed".into()
            }
            Err(error) => {
                self.status = "Could not print the image".into();
                self.error = Some(error);
            }
        }
        if std::mem::take(&mut self.quit_after_work) {
            self.request_quit(ctx);
        }
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::App as _;

    fn fixture(root: &std::path::Path) -> (MyImageApp, egui::Context) {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.initialized = true;
        app.guide_open = false;
        app.view = View::Editor;
        app.settings.output_dir = root.to_owned();
        app.install_document_state(
            Document {
                image: Arc::new(image::RgbaImage::from_pixel(
                    80,
                    40,
                    image::Rgba([80, 120, 160, 255]),
                )),
                annotations: vec![],
                geometry: Default::default(),
            },
            Some(root.join("print.myimage")),
        );
        (app, ctx)
    }

    #[test]
    fn print_shortcut_prepares_once_and_cancel_or_error_preserve_edits() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, ctx) = fixture(dir.path());
        let before = app.document.as_ref().unwrap().edit_state();
        let document = app.document.as_mut().unwrap();
        document.geometry.flip_horizontal = true;
        app.history.commit(before, document);
        let expected = app.document.as_ref().unwrap().edit_state();
        let revision = app.history.revision();
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::P,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND,
                }],
                ..Default::default()
            },
            |ctx| app.update(ctx, &mut eframe::Frame::_new_kittest()),
        );
        assert!(app.busy);
        assert_eq!(app.status, "Preparing print preview…");
        app.begin_print(&ctx); // Repeated actions cannot queue another print job.
        let event = app.work_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(event, WorkEvent::PrintPrepared(Ok(_))));
        app.work_tx.send(event).unwrap();
        app.handle_events(&ctx);
        assert!(app.print_ready.take().is_some());
        assert!(app.work_rx.try_recv().is_err());
        app.finish_print(&ctx, Ok(crate::printing::Outcome::NotCompleted));
        assert!(!app.busy && app.has_unsaved_edits());
        assert_eq!(app.document.as_ref().unwrap().edit_state(), expected);
        assert_eq!(app.history.revision(), revision);
        app.busy = true;
        app.finish_print(&ctx, Err("Printer unavailable".into()));
        assert!(!app.busy);
        assert_eq!(app.error.as_deref(), Some("Printer unavailable"));
        assert_eq!(app.document.as_ref().unwrap().edit_state(), expected);
    }

    #[test]
    fn printing_respects_dialogs_and_no_document_and_reports_missing_native_window() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, ctx) = fixture(dir.path());
        for block in ["about", "updates", "save-as", "busy", "no-document"] {
            app.about_open = block == "about";
            app.updates.open = block == "updates";
            app.save_as_panel.open = block == "save-as";
            app.busy = block == "busy";
            let doc = if block == "no-document" {
                app.document.take()
            } else {
                None
            };
            app.begin_print(&ctx);
            assert!(app.work_rx.try_recv().is_err());
            assert!(app.print_ready.is_none());
            if let Some(doc) = doc {
                app.document = Some(doc);
            }
        }
        app.about_open = false;
        app.updates.open = false;
        app.save_as_panel.open = false;
        app.busy = true;
        app.print_ready =
            Some(crate::printing::prepare(app.document.as_ref().unwrap(), "Test".into()).unwrap());
        app.poll_print(&ctx, &eframe::Frame::_new_kittest());
        assert!(!app.busy);
        assert!(app.print_job.is_none());
        assert!(app.error.as_ref().unwrap().contains("window"));
    }
}
