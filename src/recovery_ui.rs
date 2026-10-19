use super::*;
use crate::appearance::Icon as DialogIcon;
use crate::dialogs::{self, Dialog};
use crate::recovery::{self, Command, Entry, Event};

#[derive(Default)]
pub(super) struct RecoveryState {
    tx: Option<mpsc::Sender<Command>>,
    entries: Vec<Entry>,
    pub prompt: bool,
    pub ready: bool,
    stopping: bool,
    armed: bool,
    due: bool,
    pub writing: bool,
    copy_expected: bool,
    epoch: u64,
    snapshot: Option<document::EditState>,
    pub warning: Option<String>,
    pub saved_at: Option<String>,
}
impl MyImageApp {
    pub(super) fn start_recovery(&mut self, ctx: &egui::Context) {
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        match recovery::start(recovery::directory(self.demo), move |event| {
            let _ = tx.send(WorkEvent::Recovery(event));
            ctx.request_repaint();
        }) {
            Ok(tx) => self.recovery.tx = Some(tx),
            Err(error) => {
                self.recovery.ready = true;
                self.recovery.warning = Some(error);
            }
        }
    }
    fn recovery_send(&mut self, command: Command) -> bool {
        if self
            .recovery
            .tx
            .as_ref()
            .is_some_and(|tx| tx.send(command).is_ok())
        {
            true
        } else {
            self.recovery.warning =
                Some("Autosave is unavailable. Save your project manually.".into());
            self.recovery.tx = None;
            false
        }
    }
    pub(super) fn clear_recovery(&mut self) {
        self.recovery.epoch = self.recovery.epoch.wrapping_add(1);
        self.recovery.snapshot = None;
        self.recovery.copy_expected = false;
        self.recovery.saved_at = None;
        if self.recovery.tx.is_some() {
            self.recovery_send(Command::Clear(self.recovery.epoch));
        }
    }
    pub(super) fn autosave(&mut self, ctx: &egui::Context) {
        if !self.recovery.ready
            || self.recovery.stopping
            || self.recovery.tx.is_none()
            || self.quitting
        {
            return;
        }
        // A clean undo or successful manual save must not leave a stale dirty draft.
        if !self.has_unsaved_edits() {
            if self.recovery.copy_expected {
                self.clear_recovery();
            }
            return;
        }
        let Some(document) = &self.document else {
            return;
        };
        let changed = self.recovery.snapshot.as_ref().is_none_or(|state| {
            state.geometry != document.geometry || state.annotations != document.annotations
        });
        if !changed || self.recovery.writing {
            return;
        }
        if !self.recovery.due {
            if !self.recovery.armed && self.recovery_send(Command::Arm) {
                self.recovery.armed = true;
            }
            return;
        }
        if self.busy
            || self.selection.is_some()
            || self.scroll_session.is_some()
            || ctx.input(|i| i.pointer.any_down())
            || !self.editor.autosave_ready()
        {
            if !self.recovery.armed && self.recovery_send(Command::Arm) {
                self.recovery.armed = true;
            }
            return;
        }
        let document = self.document.as_ref().unwrap().clone();
        let title = self
            .project_path
            .as_ref()
            .or(self.last_path.as_ref())
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.draft_title.clone());
        let state = document.edit_state();
        if self.recovery_send(Command::Save(
            self.recovery.epoch,
            Box::new(document),
            title,
        )) {
            self.recovery.snapshot = Some(state);
            self.recovery.copy_expected = true;
            self.recovery.writing = true;
            self.recovery.due = false;
            self.recovery.armed = false;
        }
    }
    pub(super) fn stop_recovery(&mut self) -> bool {
        if self.recovery.tx.is_none() {
            return false;
        }
        if !self.recovery.stopping && self.recovery_send(Command::Stop) {
            self.recovery.stopping = true;
            self.busy = true;
            self.status = "Finishing recovery cleanup…".into();
        }
        self.recovery.stopping
    }
    pub(super) fn recover_entry(&mut self, ctx: &egui::Context, id: PathBuf) {
        if self.busy {
            return;
        }
        self.recovery.prompt = false;
        if !self.can_replace_image(ctx, PendingAction::Recover(id.clone())) {
            return;
        }
        if self.recovery_send(Command::Recover(id)) {
            self.busy = true;
            self.status = "Recovering editable project…".into();
        }
    }
    pub(super) fn recovery_event(&mut self, ctx: &egui::Context, event: Event) {
        match event {
            Event::Ready(result) => match result {
                Ok(entries) => {
                    self.recovery.ready = true;
                    self.recovery.prompt = !entries.is_empty();
                    self.recovery.entries = entries;
                    if self.recovery.prompt {
                        self.guide_open = false;
                        self.show(ctx);
                    }
                }
                Err(error) => {
                    self.recovery.ready = true;
                    self.recovery.warning = Some(error);
                    self.recovery.tx = None;
                }
            },
            Event::Tick => {
                self.recovery.armed = false;
                self.recovery.due = true;
            }
            Event::Written(epoch, result) => {
                self.recovery.writing = false;
                if epoch == self.recovery.epoch {
                    match result {
                        Ok(()) => {
                            self.recovery.warning = None;
                            self.recovery.saved_at = Some(Local::now().format("%H:%M").to_string());
                        }
                        Err(error) => {
                            self.recovery.warning = Some(error);
                            self.recovery.snapshot = None;
                            self.recovery.due = false;
                        }
                    }
                }
            }
            Event::Cleared(epoch, result) => {
                if epoch == self.recovery.epoch
                    && let Err(error) = result
                {
                    self.recovery.warning = Some(error);
                }
            }
            Event::Recovered(id, result) => {
                self.busy = false;
                match result {
                    Ok(document) => {
                        let title = self
                            .recovery
                            .entries
                            .iter()
                            .find(|e| e.id == id)
                            .map(|e| e.title.clone())
                            .unwrap_or_else(|| "Recovered image".into());
                        self.recovery.entries.retain(|e| e.id != id);
                        self.recovery.epoch = self.recovery.epoch.wrapping_add(1);
                        self.recovery.snapshot = Some(document.edit_state());
                        self.recovery.copy_expected = true;
                        self.recovery.saved_at = Some(Local::now().format("%H:%M").to_string());
                        self.install_document_state(*document, None);
                        self.last_path = None;
                        let stem = std::path::Path::new(&title)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or(&title);
                        self.draft_title = if stem.starts_with("Recovered ") {
                            stem.to_owned()
                        } else {
                            format!("Recovered {stem}")
                        };
                        self.view = View::Editor;
                        self.recovery.prompt = false;
                        self.recovery.warning = None;
                        self.status =
                            "Recovered · annotations are editable · Save project to keep your work"
                                .into();
                        self.show(ctx);
                    }
                    Err(error) => {
                        self.recovery.warning = Some(error);
                        self.recovery.prompt = true;
                    }
                }
            }
            Event::Discarded(id, result) => {
                self.busy = false;
                match result {
                    Ok(()) => {
                        self.recovery.entries.retain(|e| e.id != id);
                        self.recovery.prompt = !self.recovery.entries.is_empty();
                        self.recovery.warning = None;
                        self.status = "Recovery copy discarded · saved files are unchanged".into();
                    }
                    Err(error) => self.recovery.warning = Some(error),
                }
            }
            Event::Stopped(result) => {
                self.recovery.tx = None;
                self.recovery.stopping = false;
                self.busy = false;
                match result {
                    Ok(()) => {
                        self.quitting = true;
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                    Err(error) => {
                        self.error = Some(format!(
                            "{error}\nQuit again to leave the recovery copy on disk."
                        ))
                    }
                }
            }
        }
        if !self.busy && !self.quitting && std::mem::take(&mut self.quit_after_work) {
            self.request_quit(ctx);
        }
    }
    pub(super) fn recovery_menu(&mut self, ui: &mut egui::Ui) {
        let count = self.recovery.entries.len();
        if count > 0
            && appearance::action_button(
                ui,
                &format!("Recover unfinished work… ({count})"),
                Icon::Refresh,
                !self.busy
                    && self.pending_action.is_none()
                    && self.file_dialog.is_none()
                    && !self.editor.has_modal(),
                false,
            )
            .clicked()
        {
            self.recovery.prompt = true;
            ui.close();
        }
    }
    pub(super) fn recovery_ui(&mut self, ctx: &egui::Context) {
        if !self.recovery.prompt {
            return;
        }
        Dialog::new("Pick up where you left off", "Unfinished work from your previous session.", DialogIcon::Refresh, 490.).modal(ctx, egui::Id::new("recover_unfinished_work"), |ui| {
            ui.weak("Recover opens an editable copy. Discard removes only the recovery copy; your saved images and projects stay unchanged.");
            ui.add_space(12.);
            if let Some(error) = &self.recovery.warning {
                dialogs::notice(ui, DialogIcon::Info, "Recovery needs attention", error, true);
            }
            let entries = self.recovery.entries.clone();
            egui::ScrollArea::vertical().max_height(320.).show(ui, |ui| {
                for entry in entries {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.strong(&entry.title);
                        let time: DateTime<Local> = entry.modified.into();
                        ui.weak(format!("Autosaved {}", time.format("%d %b %Y · %H:%M")));
                        ui.horizontal_wrapped(|ui| {
                            if dialogs::button(ui, "Recover", DialogIcon::Refresh, dialogs::Action::Primary, !self.busy).clicked() {
                                self.recover_entry(ctx, entry.id.clone());
                            }
                            if dialogs::button(ui, "Discard recovery", Icon::Trash, dialogs::Action::Danger, !self.busy).clicked()
                                && self.recovery_send(Command::Discard(entry.id)) {
                                self.busy = true;
                            }
                        });
                    });
                }
            });
            ui.add_space(8.);
            if dialogs::button(ui, "Decide later", DialogIcon::Close, dialogs::Action::Secondary, !self.busy).clicked() {
                self.recovery.prompt = false;
            }
            ui.weak("You can return here from the calico App menu.");
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn app() -> (MyImageApp, egui::Context, mpsc::Receiver<Command>) {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        let (tx, rx) = mpsc::channel();
        app.recovery.tx = Some(tx);
        app.recovery.ready = true;
        (app, ctx, rx)
    }
    fn document() -> Document {
        Document {
            image: Arc::new(image::RgbaImage::new(100, 80)),
            geometry: Default::default(),
            annotations: vec![],
        }
    }
    #[test]
    fn autosave_is_delayed_deduplicated_and_never_marks_project_saved() {
        let (mut app, ctx, rx) = app();
        app.install_document(document(), None);
        assert!(matches!(rx.recv().unwrap(), Command::Clear(_)));
        app.autosave(&ctx);
        assert!(matches!(rx.recv().unwrap(), Command::Arm));
        app.autosave(&ctx);
        assert!(rx.try_recv().is_err());
        app.recovery_event(&ctx, Event::Tick);
        app.autosave(&ctx);
        let Command::Save(epoch, _, _) = rx.recv().unwrap() else {
            panic!("expected snapshot");
        };
        app.recovery_event(&ctx, Event::Written(epoch, Ok(())));
        assert!(
            app.has_unsaved_edits(),
            "autosave must not mark work permanently saved"
        );
        app.autosave(&ctx);
        assert!(
            rx.try_recv().is_err(),
            "unchanged draft must not poll or re-encode"
        );
        // Simulate a completed manual project save, using the same event as the worker.
        app.recovery_event(
            &ctx,
            Event::Written(epoch.wrapping_sub(1), Err("stale failure".into())),
        );
        assert!(app.recovery.warning.is_none());
        app.work_tx
            .send(WorkEvent::ProjectSaved(
                PathBuf::from("/tmp/saved.myimage"),
                app.document.as_ref().unwrap().edit_state(),
                Ok(()),
            ))
            .unwrap();
        app.handle_events(&ctx);
        assert!(!app.has_unsaved_edits());
        assert!(matches!(rx.recv().unwrap(), Command::Clear(_)));
        assert!(app.recovery.saved_at.is_none());
    }
    #[test]
    fn recovered_project_requires_save_as_and_retains_original_pixels_and_annotations() {
        let (mut app, ctx, _) = app();
        let id = PathBuf::from("draft-recovery");
        app.recovery.entries.push(Entry {
            id: id.clone(),
            title: "Original.myimage".into(),
            modified: SystemTime::now(),
        });
        let mut document = document();
        document.annotations.push(crate::markup_tests::object(
            document::AnnotationKind::Arrow,
            [10., 20.],
            [70., 60.],
        ));
        let expected = document.clone();
        app.recovery_event(&ctx, Event::Recovered(id, Ok(Box::new(document))));
        assert!(app.has_unsaved_edits());
        assert!(app.project_path.is_none());
        assert!(app.last_path.is_none());
        assert_eq!(
            app.document.as_ref().unwrap().edit_state(),
            expected.edit_state()
        );
        assert_eq!(app.document.as_ref().unwrap().image, expected.image);
        assert!(app.recovery.entries.is_empty());
        assert!(!app.recovery.prompt);
        assert_eq!(app.draft_title, "Recovered Original");
        assert!(app.view == View::Editor);
    }
    #[test]
    fn undo_to_clean_removes_snapshot_but_export_does_not() {
        let (mut app, ctx, rx) = app();
        app.install_document(document(), None);
        let _ = rx.recv().unwrap();
        app.last_path = Some(PathBuf::from("/tmp/original.png"));
        let original = app.document.as_ref().unwrap().edit_state();
        app.document.as_mut().unwrap().geometry.flip_horizontal = true;
        app.history.commit(original, app.document.as_ref().unwrap());
        app.recovery.due = true;
        app.autosave(&ctx);
        let Command::Save(epoch, _, _) = rx.recv().unwrap() else {
            panic!()
        };
        app.recovery_event(&ctx, Event::Written(epoch, Ok(())));
        app.work_tx
            .send(WorkEvent::Exported(
                PathBuf::from("/tmp/export.png"),
                ExportFormat::Png,
                Ok(()),
            ))
            .unwrap();
        app.handle_events(&ctx);
        assert!(app.has_unsaved_edits());
        assert!(rx.try_recv().is_err());
        app.history.undo(app.document.as_mut().unwrap());
        app.autosave(&ctx);
        assert!(matches!(rx.recv().unwrap(), Command::Clear(_)));
    }
    #[test]
    fn quit_prompts_for_dirty_work_then_waits_for_serial_recovery_cleanup() {
        let (mut app, ctx, rx) = app();
        app.install_document(document(), None);
        let _ = rx.recv().unwrap();
        app.request_quit(&ctx);
        assert!(matches!(app.pending_action, Some(PendingAction::Quit)));
        assert!(!app.quitting);
        assert!(rx.try_recv().is_err());
        app.pending_action = None;
        app.perform_action(&ctx, PendingAction::Quit, true);
        assert!(matches!(rx.recv().unwrap(), Command::Stop));
        assert!(
            !app.quitting,
            "must await any preceding autosave and its cleanup"
        );
        app.recovery_event(&ctx, Event::Stopped(Ok(())));
        assert!(app.quitting);
    }
    #[test]
    fn stale_autosave_completion_cannot_mark_replacement_document_as_protected() {
        let (mut app, ctx, rx) = app();
        app.install_document(document(), None);
        let _ = rx.recv().unwrap();
        app.recovery.due = true;
        app.autosave(&ctx);
        let Command::Save(epoch, _, _) = rx.recv().unwrap() else {
            panic!()
        };
        let mut replacement = document();
        replacement.geometry.flip_vertical = true;
        app.install_document(replacement, None);
        assert!(matches!(rx.recv().unwrap(), Command::Clear(_)));
        app.recovery_event(&ctx, Event::Written(epoch, Ok(())));
        assert!(app.recovery.saved_at.is_none());
        assert!(app.recovery.snapshot.is_none());
        app.autosave(&ctx);
        assert!(matches!(rx.recv().unwrap(), Command::Arm));
    }
}
