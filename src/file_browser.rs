use super::*;
use crate::appearance::Icon as DialogIcon;
use crate::dialogs::{self, Dialog};
use crate::library_files::{self, Operation, Outcome};

pub(super) enum FileDialog {
    Storage,
    ChangeStorage {
        destination: PathBuf,
        move_existing: bool,
    },
    Trash(Vec<PathBuf>),
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
            LibraryAction::Storage => self.file_dialog = Some(FileDialog::Storage),
            LibraryAction::Trash(paths) => self.file_dialog = Some(FileDialog::Trash(paths)),
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
        self.busy = true;
        self.status = "Updating files…".into();
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(WorkEvent::Files(library_files::run(op)));
            ctx.request_repaint();
        });
    }
    pub(super) fn file_operation_finished(&mut self, result: Outcome) {
        self.busy = false;
        let removed_active = result
            .removed
            .iter()
            .any(|p| self.project_path.as_ref() == Some(p) || self.last_path.as_ref() == Some(p));
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
        self.library.forget_paths(&result.removed);
        self.library.forget_paths(&result.missing);
        for (old, new) in &result.renamed {
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
        if let Some(loader) = &self.image_loader {
            loader.clear();
        }
        self.library.refresh();
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
        if !result.errors.is_empty() {
            self.error = Some(result.errors.join("\n"));
        }
    }
    pub(super) fn file_dialog_ui(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.file_dialog.take() else {
            return;
        };
        let mut close = false;
        let mut operation = None;
        let mut choose_storage = false;
        let mut storage_change = None;
        let (title, icon) = match &dialog {
            FileDialog::Storage => ("Storage folder".to_owned(), Icon::OpenProject),
            FileDialog::ChangeStorage { .. } => {
                ("Change storage folder".to_owned(), Icon::OpenProject)
            }
            FileDialog::Trash(paths) => (
                format!("Move {} file(s) to Trash?", paths.len()),
                Icon::Trash,
            ),
            FileDialog::Rename { .. } => ("Rename file".to_owned(), Icon::Edit),
            FileDialog::Info(_) => ("File info".to_owned(), Icon::Info),
        };
        let shown=Dialog::new(&title, "Your MyImage library.", icon, 470.).modal(ctx,egui::Id::new("library-file-dialog"),|ui| {
            match &mut dialog {
                FileDialog::Storage => {

                    ui.label("Captures and editable projects are ordinary files in your library.");
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
                FileDialog::Trash(paths) => {

                    ui.label("You can restore them from your system Trash or Recycle Bin. Only the selected files will be moved.");
                    egui::ScrollArea::vertical().max_height(180.).show(ui,|ui| {for p in paths.iter() {ui.label(p.file_name().unwrap_or_default().to_string_lossy());}});
                    ui.add_space(8.);
                    ui.horizontal_wrapped(|ui| {
                        if dialogs::secondary(ui, "Cancel", DialogIcon::Close).clicked() {close=true;}
                        if dialogs::danger(ui, "Move to Trash", DialogIcon::Trash).clicked() {operation=Some(Operation::Trash(paths.clone()));close=true;}
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
        if let Some(op) = operation {
            self.start_file_operation(ctx, op);
        }
    }
}
