//! Autosave the canonical project and flush it before navigation or quit.
use super::*;
use std::time::Instant;

#[derive(Default)]
pub(super) struct ProjectSession {
    pub(super) saving: bool,
    revision: u64,
    due: Option<Instant>,
    failed_revision: Option<u64>,
}

impl MyImageApp {
    pub(super) fn start_project_migration(&mut self, ctx: &egui::Context) {
        if self.settings.project_library_version >= 1 {
            return;
        }
        self.busy = true;
        self.status = "Organizing your library into editable projects…".into();
        let settings = self.settings.clone();
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = crate::project_store::migrate(&settings);
            let _ = tx.send(WorkEvent::ProjectLibraryReady(result));
            ctx.request_repaint();
        });
    }

    pub(super) fn project_migration_finished(
        &mut self,
        ctx: &egui::Context,
        result: Result<crate::project_store::Migration, String>,
    ) {
        self.busy = false;
        match result {
            Err(error) => {
                self.error = Some(format!(
                    "Library conversion needs attention: {error}. Original files are intact."
                ))
            }
            Ok(result) => {
                crate::project_store::remap_settings(&mut self.settings, &result.links);
                self.settings.project_library_version = u8::from(result.warnings.is_empty());
                self.library = LibraryPanel::with_preferences(self.settings.library.clone());
                let persisted = if self.demo {
                    Ok(())
                } else {
                    storage::save_settings(&self.settings)
                };
                if let Err(error) = persisted {
                    self.error = Some(format!(
                        "Could not save library associations: {error}. Original files are intact."
                    ));
                } else {
                    self.status = "Library ready · one editable project per image".into();
                    self.error = (!result.warnings.is_empty()).then(|| {
                        format!(
                            "Some images could not be converted; originals retained:\n{}",
                            result.warnings.join("\n")
                        )
                    });
                    let settings = self.settings.clone();
                    let tx = self.work_tx.clone();
                    let ctx = ctx.clone();
                    // Preferences are durable before any verified old captures go to Trash.
                    self.busy = true;
                    std::thread::spawn(move || {
                        let warnings = crate::project_store::retire_verified_captures(&settings);
                        let _ = tx.send(WorkEvent::LegacyCapturesRetired(warnings));
                        ctx.request_repaint();
                    });
                }
            }
        }
        if !self.busy && std::mem::take(&mut self.quit_after_work) {
            self.request_quit(ctx);
        }
    }

    pub(super) fn autosave_project(&mut self, ctx: &egui::Context) {
        if self.busy
            || self.quitting
            || self.document.is_none()
            || self.pending_action.is_some()
            || self.selection.is_some()
            || self.scroll_session.is_some()
            || self.export_panel.open
            || self.file_dialog.is_some()
            || !self.editor.autosave_ready()
            || ctx.input(|i| i.pointer.any_down())
        {
            return;
        }
        if !self.has_unsaved_edits() {
            self.project_session.due = None;
            return;
        }
        let revision = self.history.revision();
        if self.project_session.failed_revision == Some(revision) {
            return;
        }
        let now = Instant::now();
        if self.project_path.is_none() {
            self.save_project(ctx, false);
            return;
        }
        if self.project_session.due.is_none() || self.project_session.revision != revision {
            self.project_session.revision = revision;
            self.project_session.due = Some(now + Duration::from_millis(900));
        }
        let due = self.project_session.due.unwrap();
        if now >= due {
            self.save_project(ctx, false);
        } else {
            ctx.request_repaint_after(due - now);
        }
    }

    pub(super) fn project_save_failed(&mut self) {
        self.project_session.saving = false;
        self.project_session.failed_revision = Some(self.history.revision());
        self.project_session.due = None;
    }

    pub(super) fn project_saved(&mut self, path: &std::path::Path) {
        if self.project_path.is_none()
            && let Some(source) = &self.last_path
            && !crate::project_store::is_project(source)
        {
            let mut links = self.settings.project_sources.clone();
            links.insert(crate::project_store::source_key(source), path.to_owned());
            crate::project_store::remap_settings(&mut self.settings, &links);
            self.library = LibraryPanel::with_preferences(self.settings.library.clone());
        }
        self.project_session = Default::default();
        self.last_path = Some(path.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app(root: &std::path::Path) -> (MyImageApp, egui::Context) {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.settings.output_dir = root.to_owned();
        app.install_document(
            Document {
                image: Arc::new(image::RgbaImage::new(30, 20)),
                annotations: vec![],
                geometry: Default::default(),
            },
            None,
        );
        (app, ctx)
    }
    fn finish(app: &mut MyImageApp, ctx: &egui::Context) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.busy && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
            app.handle_events(ctx);
        }
        assert!(!app.busy);
    }
    #[test]
    fn import_autosaves_one_project_reopens_it_and_export_stays_out_of_recents() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, ctx) = app(dir.path());
        let source = dir.path().join("original.png");
        app.document.as_ref().unwrap().image.save(&source).unwrap();
        app.last_path = Some(source.clone());
        app.settings.recent_files = vec![source.clone()];
        app.autosave_project(&ctx);
        assert!(app.project_session.saving);
        finish(&mut app, &ctx);
        let project = app.project_path.clone().unwrap();
        assert_eq!(app.settings.recent_files, vec![project.clone()]);
        assert_eq!(
            app.settings.project_sources[&crate::project_store::source_key(&source)],
            project
        );
        let doc = app.document.as_mut().unwrap();
        let before = doc.edit_state();
        doc.geometry.flip_horizontal = true;
        app.history.commit(before, doc);
        app.autosave_project(&ctx);
        assert!(!app.busy, "changes are debounced");
        app.project_session.due = Some(Instant::now());
        app.autosave_project(&ctx);
        assert!(app.project_session.saving);
        finish(&mut app, &ctx);
        assert!(
            document::load_project(&project)
                .unwrap()
                .geometry
                .flip_horizontal
        );
        assert!(!app.has_unsaved_edits());
        assert!(!app.project_session.saving);
        assert_eq!(app.settings.recent_files, vec![project.clone()]);
        // Undo remains editable, and its result is flushed when switching away.
        assert!(app.history.undo(app.document.as_mut().unwrap()));
        assert!(!app.can_replace_image(
            &ctx,
            PendingAction::OpenPath {
                path: source.clone(),
                edit: true
            }
        ));
        finish(&mut app, &ctx);
        assert_eq!(app.project_path, Some(project.clone()));
        assert!(!app.document.as_ref().unwrap().geometry.flip_horizontal);
        app.work_tx
            .send(WorkEvent::Exported(
                dir.path().join("export.png"),
                ExportFormat::Png,
                Ok(()),
            ))
            .unwrap();
        app.handle_events(&ctx);
        assert_eq!(app.settings.recent_files, vec![project]);
        assert!(source.is_file());
    }
    #[test]
    fn canvas_resize_autosave_keeps_recent_thumbnails_visible() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, ctx) = app(dir.path());
        app.autosave_project(&ctx);
        finish(&mut app, &ctx);
        let project = app.project_path.clone().unwrap();
        let other = dir.path().join("other.myimage");
        document::save_project(app.document.as_ref().unwrap(), &other).unwrap();
        app.settings.recent_files.push(other.clone());

        let draw_recents = |app: &mut MyImageApp| {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    app.library.recents_ui(
                        ui,
                        &app.settings.output_dir,
                        &app.settings.recent_files,
                        app.project_path.as_deref(),
                        false,
                    );
                });
            });
        };
        let thumbnails = || {
            ctx.tex_manager()
                .read()
                .allocated()
                .filter(|(_, meta)| meta.name.starts_with("library-"))
                .map(|(id, meta)| (meta.name.clone(), *id))
                .collect::<std::collections::HashMap<_, _>>()
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        while thumbnails().len() < 2 && Instant::now() < deadline {
            draw_recents(&mut app);
            std::thread::sleep(Duration::from_millis(10));
        }
        let before = thumbnails();
        assert_eq!(
            before.len(),
            2,
            "both previews must be warm before resizing"
        );

        let doc = app.document.as_mut().unwrap();
        let state = doc.edit_state();
        doc.geometry.resize = Some([60, 20]);
        app.history.commit(state, doc);
        app.autosave_project(&ctx);
        app.project_session.due = Some(Instant::now());
        app.autosave_project(&ctx);
        finish(&mut app, &ctx);
        assert_eq!(
            document::load_project(&project).unwrap().output_size(),
            [60, 20]
        );
        assert!(!app.has_unsaved_edits());
        assert_eq!(
            thumbnails(),
            before,
            "saving must not clear the Recents strip"
        );

        draw_recents(&mut app);
        let other_name = before
            .keys()
            .find(|name| name.ends_with(other.to_str().unwrap()))
            .unwrap();
        assert_eq!(thumbnails().get(other_name), before.get(other_name));
    }

    #[test]
    fn failed_autosave_preserves_document_and_stops_retries_until_edit_or_manual_save() {
        let dir = tempfile::tempdir().unwrap();
        let blocked = dir.path().join("file");
        std::fs::write(&blocked, b"occupied").unwrap();
        let (mut app, ctx) = app(&blocked);
        app.autosave_project(&ctx);
        finish(&mut app, &ctx);
        assert!(app.has_unsaved_edits());
        assert!(app.error.is_some());
        assert!(!app.project_session.saving);
        assert!(app.project_path.is_none());
        app.autosave_project(&ctx);
        assert!(!app.busy);
        app.settings.output_dir = dir.path().join("valid");
        assert!(app.save_project(&ctx, false));
        finish(&mut app, &ctx);
        assert!(!app.has_unsaved_edits());
        assert!(app.project_path.unwrap().is_file());
    }
    #[test]
    fn recovered_or_replaced_document_does_not_inherit_a_failed_save() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, ctx) = app(dir.path());
        app.project_save_failed();
        assert_eq!(app.project_session.failed_revision, Some(0));
        let recovered = app.document.clone().unwrap();
        app.install_document_state(recovered, None);
        app.last_path = None;
        app.autosave_project(&ctx);
        finish(&mut app, &ctx);
        assert!(!app.has_unsaved_edits());
        assert!(app.project_path.unwrap().is_file());
    }

    #[test]
    fn quitting_flushes_the_latest_edit_without_a_save_dialog() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, ctx) = app(dir.path());
        app.document.as_mut().unwrap().geometry.flip_vertical = true;
        app.request_quit(&ctx);
        assert!(app.pending_action.is_none());
        assert!(app.busy);
        assert!(!app.quitting);
        finish(&mut app, &ctx);
        assert!(app.quitting);
        assert!(
            document::load_project(&app.project_path.unwrap())
                .unwrap()
                .geometry
                .flip_vertical
        );
    }
}
