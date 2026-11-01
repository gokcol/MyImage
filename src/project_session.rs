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
            || self.save_as_panel.open
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
    fn snagit_open_imports_editable_objects_and_autosaves_one_project() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("Snagit original.snagx");
        let bytes = crate::snagit_import::test_project_bytes();
        std::fs::write(&source, &bytes).unwrap();
        let (mut app, ctx) = app(&dir.path().join("Library"));
        app.begin_open(&ctx, source.clone(), true);
        finish(&mut app, &ctx);
        let imported = app.document.as_ref().unwrap().edit_state();
        assert_eq!(imported.annotations.len(), 1);
        assert_eq!(
            imported.annotations[0].kind,
            document::AnnotationKind::Arrow
        );
        assert!(app.project_path.is_none());
        assert!(
            app.file_dialog.is_none(),
            "Faithful imports need no warning prompt"
        );
        app.autosave_project(&ctx);
        finish(&mut app, &ctx);
        let project = app.project_path.clone().unwrap();
        assert_eq!(app.settings.recent_files, vec![project.clone()]);
        assert_eq!(
            document::load_project(&project).unwrap().edit_state(),
            imported
        );
        assert_eq!(std::fs::read(&source).unwrap(), bytes);
        assert_eq!(
            app.settings.project_sources[&crate::project_store::source_key(&source)],
            project
        );
        // Reopening the original resumes its existing MyImage project.
        let doc = app.document.as_mut().unwrap();
        let before = doc.edit_state();
        doc.annotations[0].color = [25, 100, 225, 255];
        app.history.commit(before, doc);
        app.save_project(&ctx, false);
        finish(&mut app, &ctx);
        app.begin_open(&ctx, source.clone(), true);
        finish(&mut app, &ctx);
        assert_eq!(app.project_path.as_ref(), Some(&project));
        assert_eq!(
            app.document.as_ref().unwrap().annotations[0].color,
            [25, 100, 225, 255]
        );
        assert_eq!(app.settings.recent_files, vec![project]);
        assert_eq!(std::fs::read(source).unwrap(), bytes);
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

    fn workspace_meshes(
        app: &mut MyImageApp,
        ctx: &egui::Context,
        time: Option<f64>,
        events: Vec<egui::Event>,
    ) -> Vec<egui::Mesh> {
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900., 700.))),
                time,
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.workspace_ui(ui, ctx));
            },
        );
        ctx.tessellate(output.shapes, output.pixels_per_point)
            .into_iter()
            .filter_map(|primitive| match primitive.primitive {
                egui::epaint::Primitive::Mesh(mesh) => Some(mesh),
                _ => None,
            })
            .collect()
    }

    fn workspace_canvas(
        app: &mut MyImageApp,
        ctx: &egui::Context,
        texture: egui::TextureId,
        events: Vec<egui::Event>,
    ) -> egui::Mesh {
        workspace_meshes(app, ctx, None, events)
            .into_iter()
            .find(|mesh| mesh.texture_id == texture)
            .expect("the original image stays painted")
    }

    #[test]
    fn opening_from_strip_keeps_the_whole_workspace_steady() {
        use eframe::App as _;
        let dir = tempfile::tempdir().unwrap();
        let (mut app, ctx) = app(dir.path());
        // Exercise the real update/render path without starting native windows,
        // hotkeys, capture, recovery, or the user's preferences.
        app.initialized = true;
        app.guide_open = false;
        app.view = View::Editor;
        app.settings.recents_open = true;
        let old = dir.path().join("old.myimage");
        let next = dir.path().join("next.myimage");
        for path in [&old, &next] {
            document::save_project(app.document.as_ref().unwrap(), path).unwrap();
        }
        app.project_path = Some(old.clone());
        app.settings.recent_files = vec![old.clone(), next.clone()];
        let (release, gate) = mpsc::channel();
        let tx = app.work_tx.clone();
        app.image_loader = Some(
            crate::image_loading::Loader::start(move |path, result| {
                if gate.recv().is_ok() {
                    let _ = tx.send(WorkEvent::Prepared(path, result));
                }
            })
            .unwrap(),
        );
        let draw = |app: &mut MyImageApp, events| {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1280., 900.))),
                    events,
                    ..Default::default()
                },
                |ctx| app.update(ctx, &mut eframe::Frame::_new_kittest()),
            );
            let status = egui::containers::panel::PanelState::load(&ctx, egui::Id::new("status"))
                .unwrap()
                .rect;
            output
                .shapes
                .into_iter()
                .filter(|entry| {
                    let bounds = entry.shape.visual_bounding_rect();
                    bounds.is_positive() && bounds.max.y <= status.top()
                })
                .collect::<Vec<_>>()
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        while ctx
            .tex_manager()
            .read()
            .allocated()
            .filter(|(_, meta)| meta.name.starts_with("library-"))
            .count()
            < 2
        {
            draw(&mut app, vec![]);
            assert!(Instant::now() < deadline, "both thumbnails must be ready");
            std::thread::sleep(Duration::from_millis(5));
        }
        for _ in 0..4 {
            draw(&mut app, vec![]);
        }
        let normal = draw(&mut app, vec![]);
        assert!(
            normal.len() > 50,
            "exercise the header, ribbon, inspector, canvas and strip"
        );
        assert_eq!(
            draw(&mut app, vec![]),
            normal,
            "idle controls should be stable"
        );
        // Save-before-switch has the same temporary input lock. In particular,
        // unavailable Undo/Redo controls must not brighten during that lock.
        app.busy = true;
        app.project_session.saving = true;
        assert_eq!(
            draw(&mut app, vec![]),
            normal,
            "saving must keep the workspace steady"
        );
        app.busy = false;
        app.project_session.saving = false;
        app.library_action(&ctx, LibraryAction::Open(next.clone()));
        assert!(app.busy && app.opening_image);
        let loading = draw(&mut app, vec![]);
        assert_eq!(
            loading.len(),
            normal.len(),
            "opening changed workspace paint geometry"
        );
        for (index, (actual, expected)) in loading.iter().zip(&normal).enumerate() {
            assert!(
                actual == expected,
                "workspace shape {index} changed while opening: {actual:?} versus {expected:?}"
            );
        }
        // A ribbon click and a browse key must not navigate during loading.
        let home = normal
            .iter()
            .find_map(|entry| match &entry.shape {
                egui::Shape::Text(text) if text.galley.job.text == "Home" => {
                    Some(text.pos + egui::vec2(4., 4.))
                }
                _ => None,
            })
            .unwrap();
        for pressed in [true, false] {
            draw(
                &mut app,
                vec![
                    egui::Event::PointerMoved(home),
                    egui::Event::PointerButton {
                        pos: home,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        draw(
            &mut app,
            vec![
                egui::Event::PointerGone,
                egui::Event::Key {
                    key: egui::Key::ArrowRight,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(app.view == View::Editor);
        assert_eq!(app.project_path.as_ref(), Some(&old));
        release.send(()).unwrap();
        finish(&mut app, &ctx);
        assert_eq!(app.project_path, Some(next));
    }

    #[test]
    fn opening_a_project_keeps_the_current_canvas_steady_until_the_replacement_is_ready() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, ctx) = app(dir.path());
        app.guide_open = false;
        app.view = View::Editor;
        let old_path = dir.path().join("old.myimage");
        let next_path = dir.path().join("next.myimage");
        let doc = Document {
            image: Arc::new(image::RgbaImage::from_pixel(
                320,
                240,
                image::Rgba([60, 130, 210, 255]),
            )),
            annotations: vec![crate::markup_tests::object(
                document::AnnotationKind::Arrow,
                [80., 80.],
                [240., 160.],
            )],
            geometry: Default::default(),
        };
        document::save_project(&doc, &old_path).unwrap();
        app.install_document_state(doc, Some(old_path.clone()));
        let source = load_texture(
            &ctx,
            "opening baseline",
            &app.document.as_ref().unwrap().image,
        );
        app.last_texture = Some(source.clone());
        app.settings.recent_files = vec![old_path.clone(), next_path.clone()];
        let target = Document {
            image: Arc::new(image::RgbaImage::from_fn(64, 48, |x, y| {
                image::Rgba([x as u8 * 3, y as u8 * 4, 80, 255])
            })),
            annotations: vec![crate::markup_tests::object(
                document::AnnotationKind::Pixelate,
                [0., 0.],
                [64., 48.],
            )],
            geometry: Default::default(),
        };
        document::save_project(&target, &next_path).unwrap();
        let (release, gate) = mpsc::channel();
        let tx = app.work_tx.clone();
        app.image_loader = Some(
            crate::image_loading::Loader::start(move |path, result| {
                if gate.recv().is_ok() {
                    let _ = tx.send(WorkEvent::Prepared(path, result));
                }
            })
            .unwrap(),
        );
        for _ in 0..3 {
            let _ = workspace_canvas(&mut app, &ctx, source.id(), vec![]);
        }
        let normal = workspace_canvas(&mut app, &ctx, source.id(), vec![]);
        assert!(
            normal
                .vertices
                .iter()
                .all(|vertex| vertex.color == Color32::WHITE)
        );
        let old_state = app.document.as_ref().unwrap().edit_state();
        app.library_action(&ctx, LibraryAction::Open(next_path.clone()));
        assert!(app.busy);
        assert!(!app.project_session.saving);
        let position = normal.vertices[0].pos + egui::vec2(40., 40.);
        for pressed in [true, false] {
            let events = vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            assert_eq!(
                workspace_canvas(&mut app, &ctx, source.id(), events),
                normal,
                "Loading must not fade, blank, or shift the old image"
            );
            assert_eq!(app.document.as_ref().unwrap().edit_state(), old_state);
            assert_eq!(app.project_path.as_ref(), Some(&old_path));
        }
        release.send(()).unwrap();
        finish(&mut app, &ctx);
        assert_eq!(app.project_path.as_ref(), Some(&next_path));
        assert_eq!(
            app.document.as_ref().unwrap().edit_state(),
            target.edit_state()
        );
        let composition = app
            .privacy_preview
            .texture(&ctx, app.document.as_ref().unwrap())
            .unwrap()
            .expect("Prepared privacy pixels must be ready on the first frame");
        let ready = ctx.input(|i| i.time) + 1.;
        for (elapsed, old_alpha) in [(0., Some(255)), (0.06, Some(128)), (0.13, None)] {
            let meshes = workspace_meshes(&mut app, &ctx, Some(ready + elapsed), vec![]);
            let painted = meshes
                .iter()
                .find(|mesh| mesh.texture_id == composition.id())
                .expect("the protected composition is painted from the first ready frame");
            assert!(painted.vertices.iter().all(|v| v.color == Color32::WHITE));
            assert!(
                !meshes
                    .iter()
                    .any(|mesh| mesh.texture_id == app.last_texture.as_ref().unwrap().id()),
                "the target's unprocessed source must never flash during a transition"
            );
            let previous = meshes.iter().find(|mesh| mesh.texture_id == source.id());
            if let Some(alpha) = old_alpha {
                let previous = previous.expect("old image dissolves over the prepared target");
                assert_eq!(
                    previous
                        .vertices
                        .iter()
                        .map(|v| (v.pos, v.uv))
                        .collect::<Vec<_>>(),
                    normal
                        .vertices
                        .iter()
                        .map(|v| (v.pos, v.uv))
                        .collect::<Vec<_>>()
                );
                assert!(
                    previous
                        .vertices
                        .iter()
                        .all(|v| v.color.a().abs_diff(alpha) <= 1)
                );
            } else {
                assert!(
                    previous.is_none(),
                    "old pixels are released at the end of the transition"
                );
            }
        }
        assert!(!app.has_unsaved_edits());
        assert_eq!(app.settings.recent_files, [old_path, next_path]);
    }

    #[test]
    fn opening_keeps_the_recent_strip_bright_and_failed_opens_restore_input() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, ctx) = app(dir.path());
        app.guide_open = false;
        app.view = View::Editor;
        app.settings.recents_open = true;
        let old = dir.path().join("old.myimage");
        let next = dir.path().join("next.myimage");
        for path in [&old, &next] {
            document::save_project(app.document.as_ref().unwrap(), path).unwrap();
        }
        app.project_path = Some(old.clone());
        app.settings.recent_files = vec![old.clone(), next];
        let draw = |app: &mut MyImageApp| {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900., 700.))),
                    ..Default::default()
                },
                |ctx| {
                    app.recents_panel(ctx);
                    egui::CentralPanel::default().show(ctx, |ui| app.workspace_ui(ui, ctx));
                },
            );
            let thumbnails: Vec<_> = ctx
                .tex_manager()
                .read()
                .allocated()
                .filter(|(_, meta)| meta.name.starts_with("library-"))
                .map(|(id, _)| *id)
                .collect();
            ctx.tessellate(output.shapes, output.pixels_per_point)
                .into_iter()
                .filter_map(|p| match p.primitive {
                    egui::epaint::Primitive::Mesh(mesh)
                        if thumbnails.contains(&mesh.texture_id) =>
                    {
                        Some(mesh)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        while draw(&mut app).len() < 2 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let normal = draw(&mut app);
        assert_eq!(normal.len(), 2);
        assert!(
            normal
                .iter()
                .all(|m| m.vertices.iter().all(|v| v.color == Color32::WHITE))
        );
        // Gate delivery so both the genuine waiting frame and the error path are exercised.
        let (release, gate) = mpsc::channel();
        let tx = app.work_tx.clone();
        app.image_loader = Some(
            crate::image_loading::Loader::start(move |path, result| {
                if gate.recv().is_ok() {
                    let _ = tx.send(WorkEvent::Prepared(path, result));
                }
            })
            .unwrap(),
        );
        let broken = dir.path().join("broken.myimage");
        std::fs::write(&broken, b"invalid project").unwrap();
        let previous = app.document.as_ref().unwrap().edit_state();
        app.begin_open(&ctx, broken, true);
        assert!(app.busy && app.opening_image);
        assert_eq!(
            draw(&mut app),
            normal,
            "opening must not dim or reorder thumbnails"
        );
        release.send(()).unwrap();
        finish(&mut app, &ctx);
        assert!(!app.opening_image);
        assert!(app.error.is_some());
        assert_eq!(app.project_path, Some(old));
        assert_eq!(app.document.as_ref().unwrap().edit_state(), previous);
        assert_eq!(draw(&mut app), normal);
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
