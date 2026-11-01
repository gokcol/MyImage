use super::*;
use crate::appearance::Icon as DialogIcon;
use crate::dialogs::{self, Dialog};
use crate::library_files::{self, Operation, Outcome};

pub(super) enum FileDialog {
    ImportNotes(Vec<String>),
    Storage,
    ChangeStorage {
        destination: PathBuf,
        move_existing: bool,
    },
    Trash {
        paths: Vec<PathBuf>,
        skip_confirmation: bool,
    },
    Rename {
        path: PathBuf,
        name: String,
        error: Option<String>,
    },
    Info(PathBuf),
}
impl MyImageApp {
    pub(super) fn sync_library_references(&mut self) {
        let missing = self.library.take_missing_paths();
        if !missing.is_empty() {
            self.settings.recent_files.retain(|p| !missing.contains(p));
        }
        let preferences = self.library.take_preferences_change();
        let changed = preferences.is_some() || !missing.is_empty();
        if let Some(preferences) = preferences {
            self.settings.library = preferences;
        }
        if changed {
            self.persist_settings();
        }
        // The open in-memory document is intentionally untouched, including edits.
    }

    fn begin_storage_change(
        &mut self,
        ctx: &egui::Context,
        destination: PathBuf,
        move_existing: bool,
    ) {
        if self.busy {
            return;
        }
        self.finish_edit();
        self.busy = true;
        self.error = None;
        self.status = if move_existing {
            "Preparing to move captures and projects…"
        } else {
            "Updating storage…"
        }
        .into();
        let source = self.settings.output_dir.clone();
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = crate::storage_move::prepare(
                &source,
                &destination,
                move_existing,
                |done, total| {
                    let _ = tx.send(WorkEvent::StorageProgress(done, total));
                    ctx.request_repaint();
                },
            );
            let _ = tx.send(WorkEvent::StoragePrepared(result));
            ctx.request_repaint();
        });
    }

    pub(super) fn storage_prepared(
        &mut self,
        ctx: &egui::Context,
        result: Result<crate::storage_move::PreparedMove, String>,
    ) {
        let prepared = match result {
            Ok(prepared) => prepared,
            Err(error) => {
                self.busy = false;
                self.status = "Storage unchanged".into();
                self.error = Some(error);
                return;
            }
        };
        if let Some(preferences) = self.library.take_preferences_change() {
            self.settings.library = preferences;
        }
        let next = prepared.settings(&self.settings);
        // Persist references before any originals can be retired, including on restart.
        if !self.demo
            && let Err(error) = storage::save_settings(&next)
        {
            self.busy = false;
            self.status = "Storage unchanged".into();
            self.error = Some(format!(
                "Could not save storage preferences: {error}\nAll originals are intact; verified copies remain in {}.",
                prepared.destination.display()
            ));
            return;
        }
        for path in [&mut self.project_path, &mut self.last_path]
            .into_iter()
            .flatten()
        {
            prepared.remap(path);
        }
        self.settings = next;
        self.library = LibraryPanel::with_preferences(self.settings.library.clone());
        if let Some(loader) = &self.image_loader {
            loader.clear();
        }
        let count = prepared.paths.len();
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        self.status = "Storage updated · finishing move…".into();
        std::thread::spawn(move || {
            let warnings = crate::storage_move::retire_originals(prepared);
            let _ = tx.send(WorkEvent::StorageFinished(count, warnings));
            ctx.request_repaint();
        });
    }

    pub(super) fn library_action(&mut self, ctx: &egui::Context, action: LibraryAction) {
        match action {
            LibraryAction::Open(p) => self.open_path(ctx, p, true),
            LibraryAction::Copy(path) => self.copy_library_image(ctx, path),
            LibraryAction::Storage => self.file_dialog = Some(FileDialog::Storage),
            LibraryAction::Trash(paths) => {
                if self.settings.confirm_trash {
                    self.file_dialog = Some(FileDialog::Trash {
                        paths,
                        skip_confirmation: false,
                    });
                } else {
                    self.start_file_operation(ctx, Operation::Trash(paths));
                }
            }
            LibraryAction::Duplicate(paths) => {
                self.start_file_operation(ctx, Operation::Duplicate(paths))
            }
            LibraryAction::Rename(path) => {
                let name = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                self.file_dialog = Some(FileDialog::Rename {
                    path,
                    name,
                    error: None,
                });
            }
            LibraryAction::Info(path) => self.file_dialog = Some(FileDialog::Info(path)),
            LibraryAction::Reveal(path) => {
                if let Err(e) = library_files::reveal(&path) {
                    self.error = Some(e);
                }
            }
            LibraryAction::RevealLibrary(path) => {
                if !path.starts_with(&self.settings.output_dir) {
                    self.error=Some("This file is outside your library folder. Use Reveal in Finder/Explorer to locate it.".into());
                } else {
                    self.view = View::Library;
                    self.library.reveal_in_library(path);
                }
            }
        }
    }
    fn copy_library_image(&mut self, ctx: &egui::Context, path: PathBuf) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        self.status = "Copying image…".into();
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = crate::image_clipboard::file_pixels(&path).and_then(|pixels| {
                crate::image_clipboard::write_image(&pixels)?;
                Ok([pixels.width(), pixels.height()])
            });
            let event = if result.is_err() && library_files::is_missing(&path) {
                WorkEvent::MissingOpen(path)
            } else {
                WorkEvent::FileCopied(path, result)
            };
            let _ = tx.send(event);
            ctx.request_repaint();
        });
    }

    pub(super) fn start_file_operation(&mut self, ctx: &egui::Context, op: Operation) {
        if self.busy {
            return;
        }
        let active = op
            .paths()
            .iter()
            .any(|p| self.project_path.as_ref() == Some(p) || self.last_path.as_ref() == Some(p));
        if active && !self.can_replace_image(ctx, PendingAction::Files(op.clone())) {
            return;
        }
        let op = if let Operation::Trash(mut paths) = op {
            let companions = crate::project_store::owned_companions(&self.settings, &paths);
            for source in companions {
                if !paths.contains(&source) {
                    paths.push(source);
                }
            }
            Operation::Trash(paths)
        } else {
            op
        };
        self.busy = true;
        self.status = "Updating files…".into();
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(WorkEvent::Files(library_files::run(op)));
            ctx.request_repaint();
        });
    }
    pub(super) fn file_operation_finished(&mut self, ctx: &egui::Context, result: Outcome) {
        self.busy = false;
        let removed_active = result
            .removed
            .iter()
            .any(|p| self.project_path.as_ref() == Some(p) || self.last_path.as_ref() == Some(p));
        let removed: Vec<_> = result
            .removed
            .iter()
            .chain(&result.missing)
            .cloned()
            .collect();
        self.remaining_recents_after_trash =
            if removed_active && self.view == View::Editor && !self.quit_after_work {
                self.project_path
                    .as_ref()
                    .or(self.last_path.as_ref())
                    .map(|active| self.recents_after_trash(active, &removed))
            } else {
                None
            };
        if removed_active {
            self.clear_recovery();
            self.document = None;
            self.project_path = None;
            self.last_path = None;
            self.last_image = None;
            self.last_texture = None;
            self.history = EditHistory::new(document::EditState::default());
            self.editor.reset();
            self.privacy_preview.clear();
        }
        self.settings
            .recent_files
            .retain(|p| !result.removed.contains(p) && !result.missing.contains(p));
        self.library.files_removed(&removed);
        for (old, new) in &result.renamed {
            for target in self.settings.project_sources.values_mut() {
                if target == old {
                    *target = new.clone();
                }
            }
            for p in &mut self.settings.recent_files {
                if p == old {
                    *p = new.clone();
                }
            }
            if self.project_path.as_ref() == Some(old) {
                self.project_path = Some(new.clone());
            }
            if self.last_path.as_ref() == Some(old) {
                self.last_path = Some(new.clone());
            }
            self.library.renamed_path(old, new);
        }
        for p in &result.created {
            storage::remember_recent(&mut self.settings, p.clone());
        }
        if let Some(p) = self.library.take_preferences_change() {
            self.settings.library = p;
        }
        self.persist_settings();
        if !result.created.is_empty() || !result.renamed.is_empty() {
            if let Some(loader) = &self.image_loader {
                loader.clear();
            }
            self.library.refresh();
        }
        self.status = if !result.removed.is_empty() {
            format!("Moved {} file(s) to Trash", result.removed.len())
        } else if !result.created.is_empty() {
            format!("Duplicated {} file(s)", result.created.len())
        } else if !result.renamed.is_empty() {
            "File renamed".into()
        } else if !result.missing.is_empty() {
            format!(
                "Removed {} missing file(s) from Library and Recents",
                result.missing.len()
            )
        } else {
            "No files changed".into()
        };
        self.error = (!result.errors.is_empty()).then(|| result.errors.join("\n"));
        self.open_next_recent_after_trash(ctx);
    }
    pub(super) fn file_dialog_ui(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.file_dialog.take() else {
            return;
        };
        let mut close = false;
        let mut operation = None;
        let mut confirm_trash = None;
        let mut choose_storage = false;
        let mut storage_change = None;
        let (title, icon) = match &dialog {
            FileDialog::ImportNotes(_) => ("Snagit import notes".to_owned(), Icon::Info),
            FileDialog::Storage => ("Storage folder".to_owned(), Icon::OpenProject),
            FileDialog::ChangeStorage { .. } => {
                ("Change storage folder".to_owned(), Icon::OpenProject)
            }
            FileDialog::Trash { paths, .. } => (
                format!("Move {} file(s) to Trash?", paths.len()),
                Icon::Trash,
            ),
            FileDialog::Rename { .. } => ("Rename file".to_owned(), Icon::Edit),
            FileDialog::Info(_) => ("File info".to_owned(), Icon::Info),
        };
        let subtitle = if matches!(&dialog, FileDialog::ImportNotes(_)) {
            "Your original .snagx file is unchanged."
        } else {
            "Your MyImage library."
        };
        let shown=Dialog::new(&title, subtitle, icon, 470.).modal(ctx,egui::Id::new("library-file-dialog"),|ui| {
            match &mut dialog {
                FileDialog::ImportNotes(notes) => {
                    egui::ScrollArea::vertical().max_height(260.).show(ui, |ui| {
                        for note in notes { ui.label(note.as_str()); ui.add_space(6.); }
                    });
                    ui.weak("Continue to edit and save the imported content as a MyImage project.");
                    if dialogs::primary(ui, "Continue", DialogIcon::Check).clicked() { close = true; }
                }
                FileDialog::Storage => {

                    ui.label("Each project contains its original image and editable changes. PNG/JPEG files are created only when you export.");
                    ui.add_space(8.);
                    ui.strong("Library & captures");
                    ui.label(self.settings.output_dir.display().to_string());
                    ui.strong("New .myimage projects");
                    let projects = storage::projects_folder(&self.settings);
                    ui.label(projects.display().to_string());
                    ui.horizontal_wrapped(|ui| {
                        for (label, folder) in [("Open library folder", self.settings.output_dir.clone()), ("Open projects folder", projects)] {
                            if dialogs::secondary(ui, label, Icon::Open).clicked()
                                && let Err(error) = library_files::open_folder(&folder) {
                                    self.error = Some(error);
                            }
                        }
                    });
                    ui.separator();
                    ui.label("Change storage for future captures and projects, with the option to move your existing library too.");
                    ui.horizontal_wrapped(|ui| {
                        if dialogs::primary(ui, "Change storage folder…", Icon::OpenProject).clicked() {
                            choose_storage = true;
                        }
                        if dialogs::secondary(ui, "Done", DialogIcon::Check).clicked() { close = true; }
                    });
                }
                FileDialog::ChangeStorage { destination, move_existing } => {

                    ui.strong("From"); ui.label(self.settings.output_dir.display().to_string());
                    ui.strong("To"); ui.label(destination.display().to_string());
                    ui.add_space(8.);
                    ui.checkbox(move_existing, "Move existing captures and projects");
                    if *move_existing {
                        ui.label("Move images and .myimage projects from the current library. Date folders, favorites and Recents follow automatically. Conflicting filenames get a new name.");
                        ui.weak("Files outside this library, linked folders and other file types stay where they are. Open projects keep their unsaved edits.");
                    } else {
                        ui.label("Use this folder for new captures and projects only. Existing files and their Recents stay at their current locations.");
                    }
                    ui.horizontal_wrapped(|ui| {
                        if dialogs::secondary(ui, "Cancel", DialogIcon::Close).clicked() { close = true; }
                        if dialogs::primary(ui, if *move_existing { "Move and switch" } else { "Use folder" }, DialogIcon::OpenProject).clicked() {
                            storage_change = Some((destination.clone(), *move_existing)); close = true;
                        }
                    });
                }
                FileDialog::Trash { paths, skip_confirmation } => {

                    ui.label("You can restore them from your system Trash or Recycle Bin. The selected projects, their embedded images and edits, and any linked legacy captures in MyImage storage will be moved.");
                    egui::ScrollArea::vertical().max_height(180.).show(ui,|ui| {for p in paths.iter() {ui.label(p.file_name().unwrap_or_default().to_string_lossy());}});
                    ui.add_space(8.);
                    ui.checkbox(skip_confirmation, "Don’t ask again");
                    ui.weak("You can turn Trash confirmations back on in Settings.");
                    ui.add_space(8.);
                    ui.horizontal_wrapped(|ui| {
                        if dialogs::secondary(ui, "Cancel", DialogIcon::Close).clicked() {close=true;}
                        if dialogs::danger(ui, "Move to Trash", DialogIcon::Trash).clicked() {
                            confirm_trash = Some(!*skip_confirmation);
                            operation=Some(Operation::Trash(paths.clone()));
                            close=true;
                        }
                    });
                }
                FileDialog::Rename{path,name,error} => {

                    ui.label("Name"); ui.text_edit_singleline(name);
                    ui.weak(format!("The .{} extension is kept.",path.extension().unwrap_or_default().to_string_lossy()));
                    if let Some(e)=error.as_ref() { dialogs::notice(ui, DialogIcon::Info, "Name unavailable", e, true); }
                    ui.horizontal_wrapped(|ui| {
                        if dialogs::secondary(ui, "Cancel", DialogIcon::Close).clicked() {close=true;}
                        if dialogs::primary(ui, "Rename", DialogIcon::Check).clicked() {
                            match library_files::rename_target(path,name) {
                                Ok(target) if target!=*path && target.exists()=>*error=Some("A file with this name already exists.".into()),
                                Ok(_)=>{operation=Some(Operation::Rename(path.clone(),name.clone()));close=true;},
                                Err(e)=>*error=Some(e),
                            }
                        }
                    });
                }
                FileDialog::Info(path) => {

                    ui.strong(path.file_name().unwrap_or_default().to_string_lossy());
                    ui.label(path.to_string_lossy());
                    match std::fs::metadata(&path) {
                        Ok(m)=>{
                            ui.label(format!("Size: {} bytes",m.len()));
                            for (label,date) in [("Created",m.created()),("Modified",m.modified())] {
                                if let Ok(date)=date {let date: chrono::DateTime<chrono::Local>=date.into();ui.label(format!("{label}: {}",date.format("%Y-%m-%d %H:%M:%S")));}
                            }
                            ui.weak(if path.extension().is_some_and(|e|e.eq_ignore_ascii_case("myimage")) {"Editable project · original image and annotations"} else {"Image file"});
                        }
                        Err(e)=>{ui.label(e.to_string());}
                    }
                    if dialogs::primary(ui, "Done", DialogIcon::Check).clicked(){close=true;}
                }
            }
        });
        if shown.should_close() {
            close = true;
        }
        if !close {
            self.file_dialog = Some(dialog);
        }
        if choose_storage {
            self.choose_library_folder();
        }
        if let Some((destination, move_existing)) = storage_change {
            self.begin_storage_change(ctx, destination, move_existing);
        }
        if let Some(confirm) = confirm_trash
            && self.settings.confirm_trash != confirm
        {
            self.settings.confirm_trash = confirm;
            self.persist_settings();
        }
        if let Some(op) = operation {
            self.start_file_operation(ctx, op);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        app: &mut MyImageApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 800.))),
                events,
                ..Default::default()
            },
            |ctx| {
                app.file_dialog_ui(ctx);
                app.appearance_ui(ctx);
            },
        )
    }

    fn click(app: &mut MyImageApp, ctx: &egui::Context, label: &str) {
        for _ in 0..3 {
            frame(app, ctx, vec![]);
        }
        let output = frame(app, ctx, vec![]);
        let position = output
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape {
                    (text.galley.job.text == label)
                        .then(|| Rect::from_min_size(text.pos, text.galley.size()).center())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("Missing {label}"));
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }

    #[test]
    fn trash_checkbox_commits_only_on_confirm_and_settings_can_restore_the_prompt() {
        let dir = tempfile::tempdir().unwrap();
        // Missing paths exercise the worker without moving any real files to Trash.
        let missing = dir.path().join("already-removed.myimage");
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.library_action(&ctx, LibraryAction::Trash(vec![missing.clone()]));
        click(&mut app, &ctx, "Don’t ask again");
        assert!(app.settings.confirm_trash);
        click(&mut app, &ctx, "Cancel");
        assert!(app.settings.confirm_trash);
        assert!(app.file_dialog.is_none());
        assert!(!app.busy);

        app.library_action(&ctx, LibraryAction::Trash(vec![missing.clone()]));
        click(&mut app, &ctx, "Don’t ask again");
        click(&mut app, &ctx, "Move to Trash");
        assert!(!app.settings.confirm_trash);
        assert!(app.file_dialog.is_none());
        let WorkEvent::Files(result) = app.work_rx.recv_timeout(Duration::from_secs(5)).unwrap()
        else {
            panic!("Expected completed file operation");
        };
        assert_eq!(result.missing.as_slice(), std::slice::from_ref(&missing));
        app.file_operation_finished(&ctx, result);

        app.library_action(&ctx, LibraryAction::Trash(vec![missing.clone()]));
        assert!(app.file_dialog.is_none());
        let WorkEvent::Files(result) = app.work_rx.recv_timeout(Duration::from_secs(5)).unwrap()
        else {
            panic!("Expected operation without confirmation");
        };
        app.file_operation_finished(&ctx, result);
        app.appearance_open = true;
        click(&mut app, &ctx, "Ask before moving files to Trash");
        assert!(app.settings.confirm_trash);
        app.appearance_open = false;
        app.library_action(&ctx, LibraryAction::Trash(vec![missing]));
        assert!(matches!(app.file_dialog, Some(FileDialog::Trash { .. })));
    }
}
