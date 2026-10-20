//! Opening a thumbnail keeps its place; completed edits promote the active file.
use super::*;

impl MyImageApp {
    pub(super) fn remember_opened_recent(&mut self, path: PathBuf) {
        let visible = self.library.recent_paths(&self.settings.recent_files);
        self.library.unhide_recent(&path);
        storage::remember_opened_recent(&mut self.settings, path, &visible);
        self.persist_settings();
    }

    pub(super) fn promote_edited_recent(&mut self, ctx: &egui::Context) {
        if self.busy || !self.editor.autosave_ready() || ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        let revision = self.history.revision();
        if revision == self.recent_revision {
            return;
        }
        self.recent_revision = revision;
        if self.document.as_ref().is_some_and(|d| self.history.is_dirty(d))
            && let Some(path) = self.project_path.as_ref().or(self.last_path.as_ref())
            // Never resurrect an entry removed by the availability guard or user.
            && self.settings.recent_files.contains(path)
        {
            self.remember_recent(path.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> Document {
        Document {
            image: Arc::new(image::RgbaImage::new(100, 80)),
            geometry: Default::default(),
            annotations: vec![],
        }
    }

    #[test]
    fn copying_a_recent_keeps_the_open_project_undo_history_and_order_intact() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        let paths: Vec<PathBuf> = ["active.myimage", "copied.png"].map(PathBuf::from).into();
        app.install_document_state(document(), Some(paths[0].clone()));
        app.last_path = Some(paths[0].clone());
        app.settings.recent_files = paths.clone();
        app.view = View::Editor;
        let doc = app.document.as_mut().unwrap();
        let before = doc.edit_state();
        doc.geometry.flip_horizontal = true;
        app.history.commit(before, doc);
        let edited = doc.edit_state();
        let revision = app.history.revision();
        let source = doc.image.clone();
        for result in [Ok([320, 240]), Err("Clipboard unavailable".into())] {
            let succeeded = result.is_ok();
            app.busy = true;
            app.work_tx
                .send(WorkEvent::FileCopied(paths[1].clone(), result))
                .unwrap();
            app.handle_events(&ctx);
            assert!(!app.busy);
            assert_eq!(app.error.is_none(), succeeded);
            if succeeded {
                assert!(app.status.contains("Copied to clipboard"));
            }
            assert_eq!(app.settings.recent_files, paths);
            assert_eq!(app.project_path.as_ref(), Some(&paths[0]));
            assert_eq!(app.last_path.as_ref(), Some(&paths[0]));
            assert!(matches!(app.view, View::Editor));
            assert_eq!(app.history.revision(), revision);
            let doc = app.document.as_ref().unwrap();
            assert_eq!(doc.edit_state(), edited);
            assert!(Arc::ptr_eq(&doc.image, &source));
            assert!(app.history.is_dirty(doc));
        }
    }

    #[test]
    fn opening_and_browsing_keep_recents_stable_but_completed_edits_promote_once() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        let paths: Vec<PathBuf> = ["new.png", "middle.myimage", "old.png"]
            .into_iter()
            .map(PathBuf::from)
            .collect();
        app.settings.recent_files = paths.clone();
        app.work_tx
            .send(WorkEvent::Opened(paths[1].clone(), true, Ok(document())))
            .unwrap();
        app.handle_events(&ctx);
        app.promote_edited_recent(&ctx);
        assert_eq!(app.settings.recent_files, paths);

        app.work_tx
            .send(WorkEvent::Prepared(
                paths[1].clone(),
                Ok(crate::image_loading::PreparedDocument {
                    document: document(),
                    source: Arc::new(egui::ColorImage::new([1, 1], vec![Color32::TRANSPARENT])),
                    composition: None,
                    max_side: 100,
                }),
            ))
            .unwrap();
        app.handle_events(&ctx);
        app.promote_edited_recent(&ctx);
        assert_eq!(app.settings.recent_files, paths);

        // View changes and a no-op property gesture are not document changes.
        app.zoom = 2.;
        app.fit = false;
        let d = app.document.as_mut().unwrap();
        app.history.commit(d.edit_state(), d);
        app.promote_edited_recent(&ctx);
        assert_eq!(app.settings.recent_files, paths);

        let d = app.document.as_mut().unwrap();
        let before = d.edit_state();
        d.geometry.flip_horizontal = true;
        app.history.commit(before, d);
        // A completed gesture is promoted once, after releasing the pointer.
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::PointerButton {
                    pos: egui::pos2(20., 20.),
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |_| {},
        );
        app.promote_edited_recent(&ctx);
        assert_eq!(app.settings.recent_files, paths);
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::PointerButton {
                    pos: egui::pos2(20., 20.),
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |_| {},
        );
        app.promote_edited_recent(&ctx);
        assert_eq!(
            app.settings.recent_files,
            vec![paths[1].clone(), paths[0].clone(), paths[2].clone()]
        );
        let export = PathBuf::from("export.png");
        app.remember_recent(export.clone());
        app.promote_edited_recent(&ctx);
        assert_eq!(
            app.settings.recent_files[0], export,
            "An unchanged dirty project must not reclaim first place every frame"
        );

        let d = app.document.as_mut().unwrap();
        assert!(app.history.undo(d));
        app.promote_edited_recent(&ctx);
        assert_eq!(app.settings.recent_files[0], export);
        assert!(app.history.redo(app.document.as_mut().unwrap()));
        app.promote_edited_recent(&ctx);
        assert_eq!(app.settings.recent_files[0], paths[1]);

        app.settings.recent_files.retain(|p| p != &paths[1]);
        assert!(app.history.undo(app.document.as_mut().unwrap()));
        assert!(app.history.redo(app.document.as_mut().unwrap()));
        app.promote_edited_recent(&ctx);
        assert!(!app.settings.recent_files.contains(&paths[1]));
    }
}
