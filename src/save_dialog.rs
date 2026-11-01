//! Native Save As dialogs must yield to the application's event loop. In
//! particular, NSSavePanel::runModal can re-enter winit on macOS and abort the
//! process before the PDF/image writer ever starts.

use super::*;
use crate::export::ExportOptions;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Wake, Waker},
};

#[derive(Clone, Copy)]
pub(super) enum SaveKind {
    Project,
    Image(ExportOptions),
}

pub(super) struct PendingSave {
    destination: Pin<Box<dyn Future<Output = Option<PathBuf>> + Send>>,
    document: Document,
    kind: SaveKind,
    previous_status: String,
}

struct RepaintOnReady(egui::Context);

impl Wake for RepaintOnReady {
    fn wake(self: Arc<Self>) {
        self.0.request_repaint();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.request_repaint();
    }
}

impl MyImageApp {
    pub(super) fn choose_save_destination(
        &mut self,
        ctx: &egui::Context,
        dialog: rfd::AsyncFileDialog,
        kind: SaveKind,
    ) {
        // Create the native sheet on the main thread, then poll without blocking.
        // Do not move its creation to a worker or block_on it inside App::update.
        let destination = dialog.save_file();
        self.wait_for_save_destination(ctx, kind, async move {
            destination.await.map(|file| file.path().to_owned())
        });
    }

    fn wait_for_save_destination(
        &mut self,
        ctx: &egui::Context,
        kind: SaveKind,
        destination: impl Future<Output = Option<PathBuf>> + Send + 'static,
    ) {
        self.save_dialog = Some(PendingSave {
            destination: Box::pin(destination),
            document: self.document.as_ref().unwrap().clone(),
            kind,
            previous_status: self.status.clone(),
        });
        self.save_as_panel.open = false;
        self.busy = true;
        self.status = "Choose where to save the file…".into();
        // Register the future's repaint waker on the next frame, including when
        // the native dialog has already completed. No idle polling is needed.
        ctx.request_repaint();
    }

    pub(super) fn poll_save_destination(&mut self, ctx: &egui::Context) {
        let Some(mut pending) = self.save_dialog.take() else {
            return;
        };
        let waker = Waker::from(Arc::new(RepaintOnReady(ctx.clone())));
        let Poll::Ready(path) = pending
            .destination
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
        else {
            self.save_dialog = Some(pending);
            return;
        };
        self.busy = false;
        self.status = pending.previous_status;
        self.save_as_panel.open = true;
        if let Some(path) = path {
            let checked = match pending.kind {
                SaveKind::Project => checked_save_path(path, "myimage"),
                SaveKind::Image(options) => export::checked_export_path(path, options.format),
            };
            match checked {
                Err(error) => match pending.kind {
                    SaveKind::Project => self.error = Some(error),
                    SaveKind::Image(_) => self.save_as_panel.error = Some(error),
                },
                Ok(path) => {
                    self.save_as_panel.open = false;
                    match pending.kind {
                        SaveKind::Project => self.write_project(ctx, path, pending.document),
                        SaveKind::Image(options) => {
                            let tx = self.work_tx.clone();
                            let ctx = ctx.clone();
                            self.busy = true;
                            self.status = format!("Saving {} copy…", options.format.label());
                            std::thread::spawn(move || {
                                let result = export::save(&pending.document, &path, options);
                                let _ = tx.send(WorkEvent::Exported(path, options.format, result));
                                ctx.request_repaint();
                            });
                        }
                    }
                }
            }
        }
        if !self.busy && std::mem::take(&mut self.quit_after_work) {
            self.request_quit(ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct DialogReply {
        path: Option<Option<PathBuf>>,
        waker: Option<Waker>,
    }
    struct DeferredDialog(Arc<Mutex<DialogReply>>);
    impl Future for DeferredDialog {
        type Output = Option<PathBuf>;
        fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
            let mut reply = self.0.lock().unwrap();
            if let Some(path) = reply.path.take() {
                Poll::Ready(path)
            } else {
                reply.waker = Some(ctx.waker().clone());
                Poll::Pending
            }
        }
    }

    fn fixture() -> (MyImageApp, egui::Context) {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.install_document_state(
            Document {
                image: Arc::new(image::RgbaImage::from_pixel(
                    96,
                    64,
                    image::Rgba([40, 80, 120, 200]),
                )),
                geometry: Default::default(),
                annotations: vec![crate::markup_tests::object(
                    document::AnnotationKind::Arrow,
                    [8., 12.],
                    [70., 50.],
                )],
            },
            Some(PathBuf::from("original.myimage")),
        );
        let doc = app.document.as_mut().unwrap();
        let before = doc.edit_state();
        doc.geometry.flip_horizontal = true;
        app.history.commit(before, doc);
        app.settings.recent_files = vec![PathBuf::from("original.myimage")];
        app.status = "Editing project".into();
        (app, ctx)
    }

    fn finish_worker(app: &mut MyImageApp, ctx: &egui::Context) {
        let event = app.work_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        app.work_tx.send(event).unwrap();
        app.handle_events(ctx);
        assert!(!app.busy);
    }

    #[test]
    fn delayed_pdf_destination_yields_and_wakes_then_saves_all_page_sizes_without_changing_project()
    {
        for page in crate::pdf_export::PdfPageSize::ALL {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("copy.pdf");
            let (mut app, ctx) = fixture();
            let state = app.document.as_ref().unwrap().edit_state();
            let revision = app.history.revision();
            let original = app.project_path.clone();
            let recents = app.settings.recent_files.clone();
            let options = ExportOptions {
                format: ExportFormat::Pdf,
                pdf_page_size: page,
                ..Default::default()
            };
            app.save_as_panel.options = options;
            let reply = Arc::new(Mutex::new(DialogReply::default()));
            app.wait_for_save_destination(
                &ctx,
                SaveKind::Image(options),
                DeferredDialog(reply.clone()),
            );
            // Pending native dialogs must return immediately, allowing unrelated
            // events to run, and must not start any writer or autosave yet.
            app.work_tx
                .send(WorkEvent::Displays(Err("display probe completed".into())))
                .unwrap();
            app.handle_events(&ctx);
            assert!(
                app.error
                    .as_ref()
                    .unwrap()
                    .contains("display probe completed")
            );
            assert!(app.save_dialog.is_some());
            assert!(app.busy);
            assert!(!app.save_as_panel.open);
            app.autosave_project(&ctx);
            assert!(app.work_rx.try_recv().is_err());
            assert!(!path.exists());
            let wake = {
                let mut reply = reply.lock().unwrap();
                reply.path = Some(Some(path.clone()));
                reply.waker.take().expect("registered completion wakeup")
            };
            wake.wake();
            app.poll_save_destination(&ctx);
            assert!(app.save_dialog.is_none());
            assert!(app.busy);
            finish_worker(&mut app, &ctx);
            assert!(app.error.is_none());
            let bytes = std::fs::read(&path).unwrap();
            assert!(bytes.starts_with(b"%PDF-1.7"));
            assert!(bytes.ends_with(b"%%EOF"));
            assert_eq!(app.document.as_ref().unwrap().edit_state(), state);
            assert_eq!(app.history.revision(), revision);
            assert_eq!(app.project_path, original);
            assert_eq!(app.settings.recent_files, recents);
            assert!(app.has_unsaved_edits());
            assert_eq!(app.settings.image_save.pdf_page_size, page);
        }
    }

    #[test]
    fn cancel_and_invalid_destination_restore_options_and_keep_dirty_project_intact() {
        let dir = tempfile::tempdir().unwrap();
        let occupied = dir.path().join("occupied");
        for kind in [
            SaveKind::Project,
            SaveKind::Image(ExportOptions {
                format: ExportFormat::Pdf,
                ..Default::default()
            }),
        ] {
            let extension = match kind {
                SaveKind::Project => "myimage",
                SaveKind::Image(_) => "pdf",
            };
            std::fs::write(occupied.with_extension(extension), b"existing file").unwrap();
            for choice in [
                None,
                Some(dir.path().join("wrong.xyz")),
                Some(occupied.clone()),
            ] {
                let (mut app, ctx) = fixture();
                let state = app.document.as_ref().unwrap().edit_state();
                let canceled = choice.is_none();
                app.wait_for_save_destination(&ctx, kind, std::future::ready(choice));
                app.handle_events(&ctx);
                assert!(!app.busy);
                assert!(app.save_dialog.is_none());
                assert!(app.save_as_panel.open);
                assert!(!app.project_session.saving);
                assert_eq!(app.status, "Editing project");
                assert_eq!(app.document.as_ref().unwrap().edit_state(), state);
                assert!(app.has_unsaved_edits());
                assert!(app.work_rx.try_recv().is_err());
                assert_eq!(
                    app.error.is_some() || app.save_as_panel.error.is_some(),
                    !canceled
                );
                assert_eq!(
                    std::fs::read(occupied.with_extension(extension)).unwrap(),
                    b"existing file"
                );
            }
        }
    }

    #[test]
    fn asynchronous_project_save_marks_only_the_completed_snapshot_saved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.myimage");
        let (mut app, ctx) = fixture();
        let document = app.document.as_ref().unwrap().clone();
        let revision = app.history.revision();
        app.wait_for_save_destination(
            &ctx,
            SaveKind::Project,
            std::future::ready(Some(path.clone())),
        );
        app.poll_save_destination(&ctx);
        assert!(app.project_session.saving);
        assert!(app.has_unsaved_edits());
        finish_worker(&mut app, &ctx);
        assert!(app.error.is_none());
        assert!(!app.has_unsaved_edits());
        assert_eq!(app.project_path.as_ref(), Some(&path));
        assert_eq!(app.history.revision(), revision);
        let loaded = document::load_project(&path).unwrap();
        assert_eq!(loaded.edit_state(), document.edit_state());
        assert_eq!(loaded.image, document.image);
    }

    #[test]
    fn pdf_write_failure_reports_an_error_and_preserves_the_project() {
        let dir = tempfile::tempdir().unwrap();
        // A directory cannot be replaced by the atomic output file.
        let path = dir.path().join("occupied.pdf");
        std::fs::create_dir(&path).unwrap();
        let (mut app, ctx) = fixture();
        let state = app.document.as_ref().unwrap().edit_state();
        let options = ExportOptions {
            format: ExportFormat::Pdf,
            ..Default::default()
        };
        app.wait_for_save_destination(
            &ctx,
            SaveKind::Image(options),
            std::future::ready(Some(path.clone())),
        );
        app.poll_save_destination(&ctx);
        finish_worker(&mut app, &ctx);
        assert!(app.error.is_some());
        assert!(app.status.contains("Could not save PDF"));
        assert!(path.is_dir());
        assert_eq!(app.document.as_ref().unwrap().edit_state(), state);
        assert!(app.has_unsaved_edits());
    }
}
