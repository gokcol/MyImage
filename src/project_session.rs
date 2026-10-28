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

    fn workspace_canvas(
        app: &mut MyImageApp,
        ctx: &egui::Context,
        texture: egui::TextureId,
        events: Vec<egui::Event>,
    ) -> egui::Mesh {
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900., 700.))),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.workspace_ui(ui, ctx));
            },
        );
        ctx.tessellate(output.shapes, output.pixels_per_point)
            .into_iter()
            .find_map(|primitive| match primitive.primitive {
                egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == texture => Some(mesh),
                _ => None,
            })
            .expect("the original image stays painted")
    }

    #[test]
    fn project_save_keeps_canvas_appearance_and_blocks_editing() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, ctx) = app(dir.path());
        app.guide_open = false;
        app.view = View::Editor;
        app.install_document(
            Document {
                image: Arc::new(image::RgbaImage::from_pixel(
                    320,
                    240,
                    image::Rgba([60, 130, 210, 255]),
                )),
                annotations: vec![
                    serde_json::from_value(serde_json::json!({
                        "kind": "Arrow",
                        "start": {"x": 80., "y": 80.},
                        "end": {"x": 240., "y": 160.},
                        "color": [240, 40, 30, 255],
                        "stroke_width": 4.,
                        "fill": false,
                        "text": "",
                        "font_size": 24.
                    }))
                    .unwrap(),
                ],
                geometry: Default::default(),
            },
            None,
        );
        let source = load_texture(&ctx, "save canvas", &app.document.as_ref().unwrap().image);
        app.last_texture = Some(source.clone());
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let shaft_point = |mesh: &egui::Mesh| {
            let bounds = mesh.vertices.iter().fold(Rect::NOTHING, |rect, vertex| {
                rect.union(Rect::from_min_max(vertex.pos, vertex.pos))
            });
            // Source point (120, 100) is a quarter along the arrow shaft,
            // away from its endpoint and middle bend handles.
            bounds.min + bounds.size() * egui::vec2(120. / 320., 100. / 240.)
        };
        let initial = workspace_canvas(&mut app, &ctx, source.id(), vec![]);
        let select = shaft_point(&initial);
        // Select first: dragging an unselected object in Select mode creates a
        // pixel marquee. The saved baseline includes the selected inspector.
        for events in [
            vec![egui::Event::PointerMoved(select), button(select, true)],
            vec![button(select, false)],
        ] {
            let _ = workspace_canvas(&mut app, &ctx, source.id(), events);
        }
        let normal = workspace_canvas(&mut app, &ctx, source.id(), vec![]);
        assert!(normal.vertices.iter().all(|v| v.color == Color32::WHITE));
        let start = shaft_point(&normal);
        let end = start + egui::vec2(24., 16.);
        let before = app.document.as_ref().unwrap().edit_state();
        app.busy = true;
        app.project_session.saving = true;
        for events in [
            vec![egui::Event::PointerMoved(start), button(start, true)],
            vec![egui::Event::PointerMoved(end)],
            vec![button(end, false)],
        ] {
            assert_eq!(
                workspace_canvas(&mut app, &ctx, source.id(), events),
                normal
            );
            assert_eq!(app.document.as_ref().unwrap().edit_state(), before);
        }
        app.busy = false;
        app.project_session.saving = false;
        assert_eq!(
            workspace_canvas(&mut app, &ctx, source.id(), vec![]),
            normal
        );

        // Unrelated work and a dialog overlapping a save retain their visual lock.
        for saving in [false, true] {
            app.busy = true;
            app.project_session.saving = saving;
            app.about_open = saving;
            let locked = workspace_canvas(&mut app, &ctx, source.id(), vec![]);
            assert!(locked.vertices.iter().all(|v| v.color.a() < 255));
        }
        app.busy = false;
        app.project_session.saving = false;
        app.about_open = false;

        // The same pointer sequence really moves the arrow once saving finishes.
        for events in [
            vec![egui::Event::PointerMoved(start), button(start, true)],
            vec![egui::Event::PointerMoved(end)],
            vec![button(end, false)],
        ] {
            let _ = workspace_canvas(&mut app, &ctx, source.id(), events);
        }
        assert_ne!(
            app.document.as_ref().unwrap().annotations[0].start,
            before.annotations[0].start
        );
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
    #[test]
    fn rotation_and_resize_history_survive_autosave_undo_and_redo() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, ctx) = app(dir.path());
        app.autosave_project(&ctx);
        finish(&mut app, &ctx);
        let project = app.project_path.clone().unwrap();
        let original = app.document.as_ref().unwrap().edit_state();
        let pixels = app.document.as_ref().unwrap().image.clone();
        for geometry in [
            crate::geometry::ImageGeometry {
                quarter_turns: 1,
                ..Default::default()
            },
            crate::geometry::ImageGeometry {
                quarter_turns: 1,
                resize: Some([22, 33]),
                ..Default::default()
            },
        ] {
            let doc = app.document.as_mut().unwrap();
            let before = doc.edit_state();
            doc.geometry = geometry;
            app.history.commit(before, doc);
            app.autosave_project(&ctx);
            app.project_session.due = Some(Instant::now());
            app.autosave_project(&ctx);
            finish(&mut app, &ctx);
            assert_eq!(document::load_project(&project).unwrap().geometry, geometry);
            assert!(app.history.can_undo());
        }
        let edited = app.document.as_ref().unwrap().edit_state();
        for _ in 0..2 {
            assert!(app.history.undo(app.document.as_mut().unwrap()));
            app.autosave_project(&ctx);
            app.project_session.due = Some(Instant::now());
            app.autosave_project(&ctx);
            finish(&mut app, &ctx);
            assert!(app.history.can_redo());
        }
        assert_eq!(
            document::load_project(&project).unwrap().edit_state(),
            original
        );
        for _ in 0..2 {
            assert!(app.history.redo(app.document.as_mut().unwrap()));
            app.autosave_project(&ctx);
            app.project_session.due = Some(Instant::now());
            app.autosave_project(&ctx);
            finish(&mut app, &ctx);
        }
        assert_eq!(
            document::load_project(&project).unwrap().edit_state(),
            edited
        );
        assert!(Arc::ptr_eq(&pixels, &app.document.as_ref().unwrap().image));
    }
}
