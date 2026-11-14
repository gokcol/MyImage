//! Opening a thumbnail keeps its place; completed edits promote the active file.
use super::*;

impl MyImageApp {
    pub(super) fn recents_panel(&mut self, ctx: &egui::Context) {
        if matches!(self.view, View::Home | View::Editor | View::Viewer) {
            egui::TopBottomPanel::bottom("recents")
                .frame(egui::Frame::NONE.fill(Color32::WHITE).inner_margin(4))
                .show(ctx, |ui| {
                    ui.spacing_mut().item_spacing.y = 3.;
                    let unobscured = self.workspace_unobscured() && !self.editor.has_modal();
                    if unobscured {
                        self.recents_keyboard(ctx);
                    }
                    let enabled = !self.busy && unobscured;
                    let keep_appearance = !self.workspace_has_dialog();
                    let was_open = self.settings.recents_open;
                    let mut chosen = self.library.recents_header(
                        ui,
                        &mut self.settings.recents_open,
                        enabled,
                        keep_appearance,
                        |ui| {
                            if self.view == View::Editor
                                && let Some(document) = &mut self.document
                            {
                                appearance::input_scope(ui, enabled, keep_appearance, |ui| {
                                    self.editor.image_controls(ui, document, &mut self.history);
                                });
                            }
                        },
                    );
                    if was_open != self.settings.recents_open {
                        self.persist_settings();
                    }
                    if self.settings.recents_open {
                        let dirty = self.has_unsaved_edits();
                        let active = self.project_path.as_deref().or(self.last_path.as_deref());
                        chosen = appearance::input_scope(ui, enabled, keep_appearance, |ui| {
                            self.library.recents_ui(
                                ui,
                                &self.settings.output_dir,
                                &self.settings.recent_files,
                                active,
                                dirty,
                            )
                        })
                        .inner
                        .or(chosen);
                        self.sync_library_references();
                    }
                    if let Some(action) = chosen {
                        self.library_action(ctx, action);
                    }
                });
            self.prefetch_recent(ctx);
        }
    }

    pub(super) fn remember_browse_direction(&mut self, next: &std::path::Path) {
        let paths = self.library.recent_paths(&self.settings.recent_files);
        let active = self.project_path.as_ref().or(self.last_path.as_ref());
        if let (Some(previous), Some(next)) = (
            paths.iter().position(|p| Some(p) == active),
            paths.iter().position(|p| p == next),
        ) && previous != next
        {
            self.recent_browse_forward = next > previous;
        }
    }

    fn prefetch_recent(&self, ctx: &egui::Context) {
        if self.busy
            || !self.settings.recents_open
            || !matches!(self.view, View::Editor | View::Viewer)
            || !self.workspace_unobscured()
            || self.editor.has_modal()
            || self.hidden.load(Ordering::Relaxed)
        {
            return;
        }
        let Some(loader) = &self.image_loader else {
            return;
        };
        let paths = self.library.recent_paths(&self.settings.recent_files);
        let active = self.project_path.as_ref().or(self.last_path.as_ref());
        let Some(index) = paths.iter().position(|p| Some(p) == active) else {
            return;
        };
        let next = paths.get(index + 1);
        let previous = index.checked_sub(1).and_then(|i| paths.get(i));
        let candidate = if self.recent_browse_forward {
            next.or(previous)
        } else {
            previous.or(next)
        };
        if let Some(path) = candidate {
            loader.prefetch(
                path.clone(),
                ctx.input(|i| i.max_texture_side).max(1) as u32,
            );
        }
    }

    fn recents_keyboard(&mut self, ctx: &egui::Context) {
        if self.busy
            || !self.settings.recents_open
            || !matches!(self.view, View::Editor | View::Viewer)
            || self.guide_open
            || self.about_open
            || self.updates.open
            || self.appearance_open
            || self.recovery.prompt
            || self.pending_action.is_some()
            || self.file_dialog.is_some()
            || self.save_as_panel.open
            || self.editor.has_modal()
            || !self.editor.autosave_ready()
            || egui::Popup::is_any_open(ctx)
            || ctx.input(|input| !input.focused || input.pointer.any_down())
        {
            return;
        }
        let Some(document) = &self.document else {
            return;
        };
        let keyboard_scope = self.library.recents_have_focus(ctx)
            || if self.view == View::Editor {
                self.editor.can_browse_recents(ctx, document)
            } else {
                ctx.memory(|memory| memory.focused().is_none())
            };
        if !keyboard_scope {
            return;
        }
        let Some(key) = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key: key @ (egui::Key::ArrowLeft | egui::Key::ArrowRight),
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.is_none() => Some(*key),
                _ => None,
            })
        }) else {
            return;
        };
        let visible = self.library.recent_paths(&self.settings.recent_files);
        let active = self.project_path.as_ref().or(self.last_path.as_ref());
        let Some(index) = visible.iter().position(|path| Some(path) == active) else {
            return;
        };
        ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, key));
        let next = if key == egui::Key::ArrowRight {
            visible.get(index + 1)
        } else {
            index.checked_sub(1).and_then(|index| visible.get(index))
        };
        if let Some(path) = next.cloned() {
            self.library.navigate_recent_to(path.clone());
            self.library_action(ctx, LibraryAction::Open(path));
        }
    }

    pub(super) fn recents_after_trash(
        &self,
        active: &std::path::Path,
        removed: &[PathBuf],
    ) -> std::collections::VecDeque<PathBuf> {
        let visible = self.library.recent_paths(&self.settings.recent_files);
        let pivot = visible.iter().position(|path| path == active);
        let (before, after) = match pivot {
            Some(index) => (&visible[..index], &visible[index + 1..]),
            None => (&visible[..0], visible.as_slice()),
        };
        // Follow the visible strip: next to the right, then nearest to the left.
        // Exclude every removed/missing item in a batch, including companions.
        after
            .iter()
            .chain(before.iter().rev())
            .filter(|path| !removed.contains(path))
            .cloned()
            .collect()
    }

    pub(super) fn open_next_recent_after_trash(&mut self, ctx: &egui::Context) {
        if self.quit_after_work {
            self.remaining_recents_after_trash = None;
            return;
        }
        let Some(remaining) = &mut self.remaining_recents_after_trash else {
            return;
        };
        if let Some(path) = remaining.pop_front() {
            self.begin_open(ctx, path, true);
            if !self.busy {
                self.remaining_recents_after_trash = None;
            }
        } else {
            self.remaining_recents_after_trash = None;
        }
    }

    pub(super) fn remember_opened_recent(&mut self, path: PathBuf) {
        let visible = self.library.recent_paths(&self.settings.recent_files);
        self.library.unhide_recent(&path);
        if storage::remember_opened_recent(&mut self.settings, path, &visible) {
            self.persist_settings();
        }
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

    fn finish_recent_open(app: &mut MyImageApp, ctx: &egui::Context) {
        while app.busy {
            let event = app.work_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            app.work_tx.send(event).unwrap();
            app.handle_events(ctx);
        }
    }

    fn strip_app() -> (tempfile::TempDir, MyImageApp, egui::Context, Vec<PathBuf>) {
        let dir = tempfile::tempdir().unwrap();
        let paths: Vec<_> = (0..12)
            .map(|i| dir.path().join(format!("Image-{i:02}.myimage")))
            .collect();
        for path in &paths {
            document::save_project(&document(), path).unwrap();
        }
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.settings.output_dir = dir.path().to_owned();
        app.settings.recent_files = paths.clone();
        app.settings.recents_open = true;
        app.guide_open = false;
        app.view = View::Editor;
        app.install_document_state(
            document::load_project(&paths[0]).unwrap(),
            Some(paths[0].clone()),
        );
        (dir, app, ctx, paths)
    }

    #[test]
    fn strip_prefetch_follows_direction_and_skips_hidden_projects_without_editing_history() {
        let (_dir, mut app, ctx, paths) = strip_app();
        app.begin_open(&ctx, paths[2].clone(), true);
        finish_recent_open(&mut app, &ctx);
        app.prefetch_recent(&ctx);
        let (candidate, _) = app
            .image_loader
            .as_ref()
            .unwrap()
            .warmed
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(candidate, std::fs::canonicalize(&paths[3]).unwrap());
        let order = app.settings.recent_files.clone();
        let state = app.document.as_ref().unwrap().edit_state();
        assert!(!app.busy && !app.has_unsaved_edits());
        assert_eq!(app.project_path.as_ref(), Some(&paths[2]));
        assert_eq!(app.document.as_ref().unwrap().edit_state(), state);
        app.begin_open(&ctx, paths[3].clone(), true);
        finish_recent_open(&mut app, &ctx);
        app.begin_open(&ctx, paths[2].clone(), true);
        finish_recent_open(&mut app, &ctx);
        assert!(!app.recent_browse_forward);
        app.settings.library.hidden_recents.insert(paths[1].clone());
        app.library = LibraryPanel::with_preferences(app.settings.library.clone());
        app.prefetch_recent(&ctx);
        let (candidate, _) = app
            .image_loader
            .as_ref()
            .unwrap()
            .warmed
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(candidate, std::fs::canonicalize(&paths[0]).unwrap());
        assert_eq!(app.settings.recent_files, order);
        assert_eq!(app.project_path.as_ref(), Some(&paths[2]));
        assert!(!app.busy && !app.has_unsaved_edits());
    }

    fn strip_frame(
        app: &mut MyImageApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let time = ctx.input(|input| input.time) + 0.2;
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900., 700.))),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| {
                app.recents_panel(ctx);
                egui::CentralPanel::default().show(ctx, |ui| app.workspace_ui(ui, ctx));
            },
        )
    }

    fn arrow(key: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
        [true, false]
            .into_iter()
            .map(|pressed| egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            })
            .collect()
    }

    fn strip_label(output: &egui::FullOutput, path: &std::path::Path) -> Option<Pos2> {
        let name = path.file_name().unwrap().to_string_lossy();
        output.shapes.iter().find_map(|entry| {
            if let egui::Shape::Text(text) = &entry.shape {
                let center = text.pos + text.galley.size() * 0.5;
                (text.galley.job.text == name && entry.clip_rect.contains(center)).then_some(center)
            } else {
                None
            }
        })
    }

    fn click_strip_point(app: &mut MyImageApp, ctx: &egui::Context, pos: Pos2) {
        for pressed in [true, false] {
            strip_frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }

    #[test]
    fn strip_arrows_open_neighbors_keep_order_and_scroll_without_wrapping() {
        let (_dir, mut app, ctx, paths) = strip_app();
        app.settings.library.hidden_recents.insert(paths[1].clone());
        let visible: Vec<_> = paths.iter().filter(|p| **p != paths[1]).cloned().collect();
        app.library = LibraryPanel::with_preferences(app.settings.library.clone());
        for _ in 0..3 {
            strip_frame(&mut app, &ctx, vec![]);
        }
        let output = strip_frame(&mut app, &ctx, vec![]);
        click_strip_point(&mut app, &ctx, strip_label(&output, &paths[0]).unwrap());
        strip_frame(&mut app, &ctx, vec![]);
        assert!(app.library.recents_have_focus(&ctx));
        strip_frame(
            &mut app,
            &ctx,
            arrow(egui::Key::ArrowLeft, egui::Modifiers::NONE),
        );
        assert!(!app.busy, "Left stays at the first thumbnail");
        for path in &paths[2..] {
            strip_frame(
                &mut app,
                &ctx,
                arrow(egui::Key::ArrowRight, egui::Modifiers::NONE),
            );
            assert!(app.busy && app.opening_image);
            finish_recent_open(&mut app, &ctx);
            for _ in 0..3 {
                strip_frame(&mut app, &ctx, vec![]);
            }
            let output = strip_frame(&mut app, &ctx, vec![]);
            assert_eq!(app.project_path.as_ref(), Some(path));
            assert!(
                strip_label(&output, path).is_some(),
                "active thumbnail must scroll into view: {path:?}"
            );
            assert!(app.library.recents_have_focus(&ctx));
            assert_eq!(
                app.library.recent_paths(&app.settings.recent_files),
                visible
            );
        }
        strip_frame(
            &mut app,
            &ctx,
            arrow(egui::Key::ArrowRight, egui::Modifiers::NONE),
        );
        assert!(!app.busy, "Right stays at the last thumbnail");
        strip_frame(
            &mut app,
            &ctx,
            arrow(egui::Key::ArrowLeft, egui::Modifiers::NONE),
        );
        finish_recent_open(&mut app, &ctx);
        assert_eq!(app.project_path.as_ref(), Some(&paths[10]));
        assert_eq!(
            app.library.recent_paths(&app.settings.recent_files),
            visible
        );
    }

    #[test]
    fn strip_arrows_save_edits_before_opening_and_leave_other_key_scopes_alone() {
        let (_dir, mut app, ctx, paths) = strip_app();
        for _ in 0..3 {
            strip_frame(&mut app, &ctx, vec![]);
        }
        let output = strip_frame(&mut app, &ctx, vec![]);
        click_strip_point(&mut app, &ctx, strip_label(&output, &paths[0]).unwrap());
        let doc = app.document.as_mut().unwrap();
        let before = doc.edit_state();
        doc.geometry.flip_horizontal = true;
        app.history.commit(before, doc);
        strip_frame(
            &mut app,
            &ctx,
            arrow(egui::Key::ArrowRight, egui::Modifiers::NONE),
        );
        assert!(
            app.project_session.saving && app.busy,
            "save before replacing the edited project"
        );
        assert_eq!(app.project_path.as_ref(), Some(&paths[0]));
        finish_recent_open(&mut app, &ctx);
        assert_eq!(app.project_path.as_ref(), Some(&paths[1]));
        assert!(
            document::load_project(&paths[0])
                .unwrap()
                .geometry
                .flip_horizontal
        );
        assert!(!app.has_unsaved_edits());
        strip_frame(&mut app, &ctx, vec![]);
        for modifiers in [
            egui::Modifiers::SHIFT,
            egui::Modifiers::CTRL,
            egui::Modifiers::ALT,
            egui::Modifiers::COMMAND,
        ] {
            strip_frame(&mut app, &ctx, arrow(egui::Key::ArrowRight, modifiers));
            assert!(!app.busy);
            assert_eq!(app.project_path.as_ref(), Some(&paths[1]));
        }
        for blocked in ["closed", "busy", "dialog", "control", "popup"] {
            app.settings.recents_open = blocked != "closed";
            app.busy = blocked == "busy";
            app.about_open = blocked == "dialog";
            if blocked == "control" {
                ctx.memory_mut(|m| m.request_focus(egui::Id::new("focused-editor-control")));
            }
            if blocked == "popup" {
                egui::Popup::open_id(&ctx, egui::Id::new("open-strip-menu"));
            }
            strip_frame(
                &mut app,
                &ctx,
                arrow(egui::Key::ArrowRight, egui::Modifiers::NONE),
            );
            assert!(!app.opening_image, "no project navigation while {blocked}");
            assert_eq!(app.project_path.as_ref(), Some(&paths[1]));
            app.busy = false;
            app.about_open = false;
            app.settings.recents_open = true;
            egui::Popup::close_all(&ctx);
            if let Some(id) = ctx.memory(|m| m.focused()) {
                ctx.memory_mut(|m| m.surrender_focus(id));
            }
        }
    }

    #[test]
    fn strip_arrows_preserve_annotation_nudging_and_browse_from_an_empty_canvas() {
        let (_dir, mut app, ctx, paths) = strip_app();
        let mut doc = document();
        doc.annotations.push(crate::markup_tests::object(
            document::AnnotationKind::Arrow,
            [20., 20.],
            [80., 60.],
        ));
        document::save_project(&doc, &paths[0]).unwrap();
        app.install_document_state(doc, Some(paths[0].clone()));
        for _ in 0..3 {
            strip_frame(&mut app, &ctx, vec![]);
        }
        let output = strip_frame(&mut app, &ctx, vec![]);
        let texture = app.last_texture.as_ref().unwrap().id();
        let image_rect = ctx
            .tessellate(output.shapes, output.pixels_per_point)
            .into_iter()
            .find_map(|p| match p.primitive {
                egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == texture => {
                    Some(mesh.calc_bounds())
                }
                _ => None,
            })
            .unwrap();
        click_strip_point(&mut app, &ctx, image_rect.center());
        let before = app.document.as_ref().unwrap().annotations[0].clone();
        strip_frame(
            &mut app,
            &ctx,
            arrow(egui::Key::ArrowRight, egui::Modifiers::NONE),
        );
        assert!(
            !app.busy,
            "selected annotations must nudge instead of opening a file"
        );
        let after = &app.document.as_ref().unwrap().annotations[0];
        assert_eq!(after.start.x, before.start.x + 1.);
        assert_eq!(after.end.x, before.end.x + 1.);
        assert_eq!(app.project_path.as_ref(), Some(&paths[0]));
        // Clicking empty canvas clears the selection while keeping canvas focus.
        click_strip_point(
            &mut app,
            &ctx,
            image_rect.min + image_rect.size() * egui::vec2(0.1, 0.8),
        );
        strip_frame(
            &mut app,
            &ctx,
            arrow(egui::Key::ArrowRight, egui::Modifiers::NONE),
        );
        assert!(app.busy);
        finish_recent_open(&mut app, &ctx);
        assert_eq!(app.project_path.as_ref(), Some(&paths[1]));
        assert_eq!(
            document::load_project(&paths[0]).unwrap().annotations[0]
                .start
                .x,
            before.start.x + 1.
        );
    }

    #[test]
    fn deleting_an_open_recent_advances_right_then_left_without_reordering_the_strip() {
        for (active, deleted, expected) in [
            (0, vec![0], Some(1)),
            (1, vec![1], Some(2)),
            (3, vec![3], Some(2)),
            (1, vec![0, 1, 2], Some(3)),
            (2, vec![2, 3], Some(1)),
            (1, vec![0, 1, 2, 3], None),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let paths: Vec<_> = (0..4)
                .map(|i| dir.path().join(format!("{i}.myimage")))
                .collect();
            for (i, path) in paths.iter().enumerate() {
                let mut doc = document();
                doc.image = Arc::new(image::RgbaImage::from_pixel(
                    4,
                    4,
                    image::Rgba([i as u8, 50, 100, 255]),
                ));
                document::save_project(&doc, path).unwrap();
            }
            let ctx = egui::Context::default();
            let mut app =
                MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
            app.install_document_state(
                document::load_project(&paths[active]).unwrap(),
                Some(paths[active].clone()),
            );
            app.last_path = Some(paths[active].clone());
            app.settings.recent_files = paths.clone();
            app.view = View::Editor;
            let removed: Vec<_> = deleted.iter().map(|&i| paths[i].clone()).collect();
            for path in &removed {
                std::fs::remove_file(path).unwrap();
            }
            app.file_operation_finished(
                &ctx,
                crate::library_files::Outcome {
                    removed: removed.clone(),
                    ..Default::default()
                },
            );
            assert!(matches!(app.view, View::Editor));
            assert!(app.document.is_none());
            assert_eq!(app.busy, expected.is_some());
            finish_recent_open(&mut app, &ctx);
            assert!(matches!(app.view, View::Editor));
            assert_eq!(app.project_path.as_ref(), expected.map(|i| &paths[i]));
            if let Some(index) = expected {
                assert_eq!(
                    app.document.as_ref().unwrap().image.get_pixel(0, 0)[0],
                    index as u8
                );
                assert!(app.last_texture.is_some());
            }
            assert!(!app.has_unsaved_edits());
            assert!(app.remaining_recents_after_trash.is_none());
            let remaining: Vec<_> = paths
                .iter()
                .filter(|path| !removed.contains(path))
                .cloned()
                .collect();
            assert_eq!(
                app.library.recent_paths(&app.settings.recent_files),
                remaining
            );
            app.autosave_project(&ctx);
            assert!(!app.busy);
            assert!(removed.iter().all(|path| !path.exists()));
        }
    }

    #[test]
    fn next_recent_skips_hidden_and_newly_missing_files_and_keeps_partial_delete_errors() {
        let dir = tempfile::tempdir().unwrap();
        let paths: Vec<_> = ["deleted", "hidden", "missing", "next"]
            .iter()
            .map(|name| dir.path().join(format!("{name}.myimage")))
            .collect();
        document::save_project(&document(), &paths[1]).unwrap();
        document::save_project(&document(), &paths[3]).unwrap();
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.install_document_state(document(), Some(paths[0].clone()));
        app.settings.recent_files = paths.clone();
        app.settings.library.hidden_recents.insert(paths[1].clone());
        app.library = LibraryPanel::with_preferences(app.settings.library.clone());
        app.view = View::Editor;
        app.file_operation_finished(
            &ctx,
            crate::library_files::Outcome {
                removed: vec![paths[0].clone()],
                errors: vec!["A different selected file could not be moved to Trash".into()],
                ..Default::default()
            },
        );
        finish_recent_open(&mut app, &ctx);
        assert_eq!(app.project_path.as_ref(), Some(&paths[3]));
        assert!(!app.settings.recent_files.contains(&paths[2]));
        assert_eq!(
            app.library
                .recent_paths(&app.settings.recent_files)
                .as_slice(),
            std::slice::from_ref(&paths[3])
        );
        assert!(
            app.error
                .as_ref()
                .unwrap()
                .contains("could not be moved to Trash")
        );
        assert!(paths[1].exists());
        assert!(app.remaining_recents_after_trash.is_none());
    }

    #[test]
    fn failed_delete_or_pending_quit_does_not_open_a_different_recent() {
        let ctx = egui::Context::default();
        for quitting in [false, true] {
            let mut app =
                MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
            let active = PathBuf::from("active.myimage");
            app.install_document_state(document(), Some(active.clone()));
            app.settings.recent_files = vec![active.clone(), PathBuf::from("next.myimage")];
            app.view = View::Editor;
            app.quit_after_work = quitting;
            app.file_operation_finished(
                &ctx,
                if quitting {
                    crate::library_files::Outcome {
                        removed: vec![active.clone()],
                        ..Default::default()
                    }
                } else {
                    crate::library_files::Outcome {
                        errors: vec!["Permission denied".into()],
                        ..Default::default()
                    }
                },
            );
            assert!(!app.busy);
            assert!(app.remaining_recents_after_trash.is_none());
            assert!(app.work_rx.try_recv().is_err());
            if !quitting {
                assert_eq!(app.project_path.as_ref(), Some(&active));
                assert!(app.document.is_some());
                assert_eq!(app.error.as_deref(), Some("Permission denied"));
            }
        }
    }

    #[test]
    fn a_corrupt_next_recent_reports_the_open_failure_without_removing_that_file() {
        let dir = tempfile::tempdir().unwrap();
        let removed = dir.path().join("deleted.myimage");
        let corrupt = dir.path().join("corrupt.myimage");
        std::fs::write(&corrupt, b"not a project").unwrap();
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.install_document_state(document(), Some(removed.clone()));
        app.settings.recent_files = vec![removed.clone(), corrupt.clone()];
        app.view = View::Editor;
        app.file_operation_finished(
            &ctx,
            crate::library_files::Outcome {
                removed: vec![removed],
                ..Default::default()
            },
        );
        finish_recent_open(&mut app, &ctx);
        assert!(app.error.is_some());
        assert!(matches!(app.view, View::Editor));
        assert!(app.remaining_recents_after_trash.is_none());
        assert!(app.document.is_none());
        assert!(app.settings.recent_files.contains(&corrupt));
        assert_eq!(std::fs::read(&corrupt).unwrap(), b"not a project");
    }

    #[test]
    fn deleting_the_last_open_project_keeps_editor_visible_and_does_not_recreate_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let removed = dir.path().join("deleted.myimage");
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.guide_open = false;
        app.install_document_state(document(), Some(removed.clone()));
        app.last_path = Some(removed.clone());
        app.settings.recent_files = vec![removed.clone()];
        app.view = View::Editor;
        // Simulate a completed Trash operation; no real user files are moved.
        app.file_operation_finished(
            &ctx,
            crate::library_files::Outcome {
                removed: vec![removed.clone()],
                ..Default::default()
            },
        );
        assert!(matches!(app.view, View::Editor));
        assert!(app.document.is_none());
        assert!(app.project_path.is_none());
        assert!(app.last_image.is_none());
        assert!(!app.has_unsaved_edits());
        app.autosave_project(&ctx);
        assert!(!app.busy);
        assert!(app.work_rx.try_recv().is_err());
        assert!(!removed.exists());
        assert!(app.settings.recent_files.is_empty());
        for busy in [false, true] {
            app.busy = busy;
            app.status = "Opening image…".into();
            for _ in 0..3 {
                let frame = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 800.))),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| app.workspace_ui(ui, ctx));
                    },
                );
                let text: String = frame
                    .shapes
                    .iter()
                    .filter_map(|shape| {
                        if let egui::Shape::Text(text) = &shape.shape {
                            Some(text.galley.job.text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                assert!(!text.contains("Open a recent image, capture your screen"));
                assert!(!text.contains("Start from a template"));
                assert!(text.contains(if busy {
                    "Opening image…"
                } else {
                    "Select an image from Recents"
                }));
            }
        }
    }

    #[test]
    fn deleting_the_open_project_from_library_keeps_library_and_remaining_recents() {
        let removed = PathBuf::from("deleted.myimage");
        let retained = PathBuf::from("retained.myimage");
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.install_document_state(document(), Some(removed.clone()));
        app.settings.recent_files = vec![removed.clone(), retained.clone()];
        app.view = View::Library;
        app.file_operation_finished(
            &ctx,
            crate::library_files::Outcome {
                removed: vec![removed],
                ..Default::default()
            },
        );
        assert!(matches!(app.view, View::Library));
        assert!(app.document.is_none());
        assert!(!app.busy);
        assert_eq!(app.settings.recent_files, vec![retained]);
    }

    #[test]
    fn skipping_trash_confirmation_still_saves_the_active_projects_unsaved_edits_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("active.myimage");
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.install_document_state(document(), Some(path.clone()));
        app.settings.confirm_trash = false;
        let doc = app.document.as_mut().unwrap();
        let before = doc.edit_state();
        doc.geometry.flip_horizontal = true;
        app.history.commit(before, doc);
        let edited = doc.edit_state();
        app.library_action(&ctx, LibraryAction::Trash(vec![path.clone()]));
        assert!(app.file_dialog.is_none());
        assert!(
            matches!(&app.after_save, Some(PendingAction::Files(crate::library_files::Operation::Trash(paths))) if paths.as_slice() == std::slice::from_ref(&path))
        );
        let WorkEvent::ProjectSaved(saved, state, result) =
            app.work_rx.recv_timeout(Duration::from_secs(5)).unwrap()
        else {
            panic!("The save must complete before any Trash operation");
        };
        assert_eq!(saved, path);
        assert_eq!(state, edited);
        result.unwrap();
        assert!(path.is_file());
        assert_eq!(document::load_project(&path).unwrap().edit_state(), edited);
        assert!(app.document.is_some());
    }

    #[test]
    fn trashing_other_recents_preserves_the_open_image_history_and_failed_entries() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        let paths: Vec<PathBuf> = [
            "active.myimage",
            "removed.myimage",
            "failed.myimage",
            "missing.myimage",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect();
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
        let image = doc.image.clone();
        app.file_operation_finished(
            &ctx,
            crate::library_files::Outcome {
                removed: vec![paths[1].clone()],
                missing: vec![paths[3].clone()],
                errors: vec!["failed.myimage: permission denied".into()],
                ..Default::default()
            },
        );
        assert_eq!(
            app.settings.recent_files,
            [paths[0].clone(), paths[2].clone()]
        );
        assert_eq!(app.project_path.as_ref(), Some(&paths[0]));
        assert_eq!(app.last_path.as_ref(), Some(&paths[0]));
        assert!(matches!(app.view, View::Editor));
        assert_eq!(app.history.revision(), revision);
        let doc = app.document.as_ref().unwrap();
        assert_eq!(doc.edit_state(), edited);
        assert!(Arc::ptr_eq(&doc.image, &image));
        assert!(app.history.is_dirty(doc));
        assert!(app.error.as_ref().unwrap().contains("permission denied"));
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
        let paths: Vec<PathBuf> = ["new.myimage", "middle.myimage", "old.myimage"]
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
                    warnings: Vec::new(),
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
