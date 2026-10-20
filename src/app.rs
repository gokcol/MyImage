use crate::appearance::Icon as DialogIcon;
use crate::dialogs::{self, Dialog};
#[path = "app_menu.rs"]
mod app_menu;
#[path = "capture_launcher.rs"]
mod capture_launcher;
#[path = "capture_output.rs"]
mod capture_output;
#[path = "capture_session.rs"]
mod capture_session;
use crate::{
    appearance::{self, Icon},
    capture::{self, DisplayInfo},
    desktop::{DesktopEvent, DesktopIntegration},
    document::{self, Document, EditHistory},
    editor::EditorState,
    export::{self, ExportFormat},
    export_panel::ExportPanel,
    library_panel::{LibraryAction, LibraryPanel},
    privacy_preview::PrivacyPreview,
    render,
    selection::{self, CaptureShape},
    storage::{self, Settings},
};
use capture_output::CaptureOutput;
use capture_session::{CapturedFrame, ScrollSession};
use chrono::{DateTime, Local};
use eframe::egui::{
    self, Color32, Pos2, Rect, Stroke, Vec2, ViewportCommand, viewport::WindowLevel,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

enum WorkEvent {
    Recovery(crate::recovery::Event),
    Displays(Result<Vec<DisplayInfo>, String>),
    Captured(
        u64,
        DisplayInfo,
        DateTime<Local>,
        Result<CapturedFrame, String>,
    ),
    WindowCaptured(
        u64,
        u32,
        Option<[f32; 4]>,
        DateTime<Local>,
        Result<image::RgbaImage, String>,
    ),
    ScrollSection(
        u64,
        Box<crate::scroll_capture::Stitcher>,
        Result<bool, String>,
        bool,
    ),
    Saved(Arc<image::RgbaImage>, Result<PathBuf, String>, bool),
    CaptureFinished(Result<CaptureOutput, String>),
    #[cfg(test)]
    Opened(PathBuf, bool, Result<Document, String>),
    MissingOpen(PathBuf),
    Prepared(
        PathBuf,
        Result<crate::image_loading::PreparedDocument, String>,
    ),
    ProjectSaved(PathBuf, document::EditState, Result<(), String>),
    Exported(PathBuf, ExportFormat, Result<(), String>),
    ClipboardImage(Result<image::RgbaImage, String>),
    FileCopied(PathBuf, Result<[u32; 2], String>),
    Files(crate::library_files::Outcome),
    StoragePrepared(Result<crate::storage_move::PreparedMove, String>),
    StorageProgress(usize, usize),
    StorageFinished(usize, Vec<String>),
}

#[path = "file_browser.rs"]
mod file_browser;
#[path = "recent_history.rs"]
mod recent_history;
#[path = "recovery_ui.rs"]
mod recovery_ui;
use file_browser::FileDialog;

#[derive(Clone)]
enum PendingAction {
    Recover(PathBuf),
    OpenCapture(CaptureOutput),
    Open,
    OpenProject,
    Create(crate::templates::Spec),
    OpenPath { path: PathBuf, edit: bool },
    Quit,
    Files(crate::library_files::Operation),
}

struct Selection {
    display: DisplayInfo,
    image: image::RgbaImage,
    texture: egui::TextureHandle,
    captured_at: DateTime<Local>,
    drag: capture_session::RegionDrag,
    multi: capture_session::MultiRegions,
    mode: capture::CaptureMode,
    return_hidden: bool,
    placement_frames: u8,
    windows: Vec<capture::WindowInfo>,
    window_id: Option<u32>,
    hover_cycle: usize,
    magnifier: bool,
    magnifier_preview: crate::capture_magnifier::CaptureMagnifier,
    pointer_ready: bool,
}

/// Keep geometry in desktop units, independent of the capture window's DPI:
/// physical pixels on Windows and Quartz points on macOS.
#[derive(Clone, Copy)]
struct WindowPlacement {
    outer_position: Pos2,
    inner_size: Vec2,
    maximized: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum View {
    Home,
    Capture,
    Library,
    Viewer,
    Editor,
    Create,
}

pub struct MyImageApp {
    about_open: bool,
    appearance_open: bool,
    guide_open: bool,
    source_textures: crate::texture_cache::SourceTextures,
    recovery: recovery_ui::RecoveryState,
    settings: Settings,
    startup: crate::startup::Preference,
    #[cfg(target_os = "macos")]
    _termination_guard: Option<crate::desktop::TerminationGuard>,
    desktop: Option<DesktopIntegration>,
    capture_launcher: Option<capture_launcher::Launcher>,
    desktop_rx: mpsc::Receiver<DesktopEvent>,
    desktop_tx: mpsc::Sender<DesktopEvent>,
    work_rx: mpsc::Receiver<WorkEvent>,
    work_tx: mpsc::Sender<WorkEvent>,
    hidden: Arc<AtomicBool>,
    initialized: bool,
    background_start: bool,
    quitting: bool,
    quit_after_work: bool,
    pending_action: Option<PendingAction>,
    file_dialog: Option<FileDialog>,
    after_save: Option<PendingAction>,
    allow_discard: bool,
    displays: Vec<DisplayInfo>,
    shape: CaptureShape,
    view: View,
    generation: u64,
    pending_capture: Option<bool>,
    capture_cancel: Option<Arc<AtomicBool>>,
    capture_copy_retry: Option<CaptureOutput>,
    selection: Option<Selection>,
    scroll_session: Option<ScrollSession>,
    busy: bool,
    error: Option<String>,
    status: String,
    last_image: Option<Arc<image::RgbaImage>>,
    last_texture: Option<egui::TextureHandle>,
    app_icon: egui::TextureHandle,
    calico: crate::calico_play::CalicoPlay,
    last_path: Option<PathBuf>,
    document: Option<Document>,
    project_path: Option<PathBuf>,
    history: EditHistory,
    recent_revision: u64,
    editor: EditorState,
    library: LibraryPanel,
    image_loader: Option<crate::image_loading::Loader>,
    create_panel: crate::templates::CreatePanel,
    draft_title: String,
    export_panel: ExportPanel,
    privacy_preview: PrivacyPreview,
    zoom: f32,
    fit: bool,
    editor_placement: Option<WindowPlacement>,
    restore_frames_left: u8,
    demo: bool,
}

impl MyImageApp {
    fn paste_new_image(&mut self, ctx: &egui::Context) {
        if self.busy || self.document.is_some() {
            return;
        }
        self.busy = true;
        self.status = "Pasting image…".into();
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(WorkEvent::ClipboardImage(
                crate::image_clipboard::read_image(),
            ));
            ctx.request_repaint();
        });
    }
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        background: bool,
        hidden: Arc<AtomicBool>,
    ) -> Self {
        let demo = std::env::args().any(|arg| arg == "--demo")
            || std::env::var_os("MYIMAGE_DEMO").is_some();
        Self::with_context(&cc.egui_ctx, background, hidden, demo)
    }

    fn with_context(
        ctx: &egui::Context,
        background: bool,
        hidden: Arc<AtomicBool>,
        demo: bool,
    ) -> Self {
        let (desktop_tx, desktop_rx) = mpsc::channel();
        let (work_tx, work_rx) = mpsc::channel();
        let (mut settings, error) = match if demo {
            Ok(Settings::default())
        } else {
            storage::load_settings()
        } {
            Ok(value) => (value, None),
            Err(error) => (Settings::default(), Some(error)),
        };
        if demo {
            settings.output_dir = std::env::temp_dir().join("MyImage-demo");
        }
        let shape = match settings.shape.as_str() {
            "rounded" => CaptureShape::RoundedRectangle,
            "ellipse" => CaptureShape::Ellipse,
            _ => CaptureShape::Rectangle,
        };
        settings.recent_files.truncate(24);
        appearance::apply(ctx);
        appearance::set_finish(ctx, settings.gui_finish);
        crate::icon_style::set(ctx, settings.icon_style);
        ctx.set_zoom_factor(settings.ui_text_size.factor());
        ctx.options_mut(|options| options.zoom_with_keyboard = false);
        let app_icon = load_texture(
            ctx,
            "app-menu-calico",
            &image::load_from_memory(include_bytes!(
                "../assets/branding/myimage-app-icon-256.png"
            ))
            .expect("bundled app icon is valid PNG")
            .into_rgba8(),
        );
        let library = LibraryPanel::with_preferences(settings.library.clone());
        let mut editor = EditorState::with_presets(storage::preset_library_path(demo));
        editor.set_toolbar(
            settings.editor_toolbar.clone(),
            settings.toolbar_layout_version == 0,
        );
        Self {
            about_open: false,
            appearance_open: false,
            guide_open: !background && settings.guide_version < crate::onboarding::VERSION,
            source_textures: Default::default(),
            recovery: Default::default(),
            settings,
            startup: Default::default(),
            #[cfg(target_os = "macos")]
            _termination_guard: None,
            desktop: None,
            capture_launcher: None,
            desktop_rx,
            desktop_tx,
            work_rx,
            work_tx,
            hidden,
            initialized: false,
            background_start: background,
            quitting: false,
            quit_after_work: false,
            pending_action: None,
            file_dialog: None,
            after_save: None,
            allow_discard: false,
            displays: Vec::new(),
            shape,
            view: View::Home,
            generation: 0,
            pending_capture: None,
            capture_cancel: None,
            capture_copy_retry: None,
            selection: None,
            scroll_session: None,
            busy: false,
            error,
            status: "Ready to edit".into(),
            last_image: None,
            last_texture: None,
            app_icon,
            calico: Default::default(),
            last_path: None,
            document: None,
            project_path: None,
            history: EditHistory::new(document::EditState::default()),
            recent_revision: 0,
            editor,
            library,
            image_loader: None,
            create_panel: Default::default(),
            draft_title: "Untitled image".into(),
            export_panel: ExportPanel::default(),
            privacy_preview: PrivacyPreview::default(),
            zoom: 1.0,
            fit: true,
            editor_placement: None,
            restore_frames_left: 0,
            demo,
        }
    }

    fn remember_recent(&mut self, path: PathBuf) {
        self.library.unhide_recent(&path);
        storage::remember_recent(&mut self.settings, path);
        self.persist_settings();
    }
    fn open_project(&mut self, ctx: &egui::Context) {
        if self.busy
            || self.export_panel.open
            || self.editor.has_modal()
            || !self.can_replace_image(ctx, PendingAction::OpenProject)
        {
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Open editable project")
            .add_filter("MyImage project", &["myimage"])
            .pick_file()
        {
            self.begin_open(ctx, path, true);
        }
    }
    fn create_image(&mut self, ctx: &egui::Context, spec: crate::templates::Spec) {
        if self.busy
            || self.editor.has_modal()
            || self.export_panel.open
            || !self.can_replace_image(ctx, PendingAction::Create(spec.clone()))
        {
            return;
        }
        match crate::templates::build(&spec) {
            Ok(document) => {
                self.install_document(document, None);
                self.last_path = None;
                self.draft_title = if spec.title.is_empty() {
                    "Untitled image".into()
                } else {
                    spec.title
                };
                self.view = View::Editor;
                self.status = "Image created · placeholders and text remain editable".into();
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn refresh_displays(&self, ctx: &egui::Context) {
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(WorkEvent::Displays(capture::displays()));
            ctx.request_repaint();
        });
    }

    fn persist_settings(&mut self) {
        if self.demo {
            return;
        }
        self.settings.shape = match self.shape {
            CaptureShape::Rectangle => "rectangle",
            CaptureShape::RoundedRectangle => "rounded",
            CaptureShape::Ellipse => "ellipse",
        }
        .into();
        if let Err(error) = storage::save_settings(&self.settings) {
            self.error = Some(error);
        }
    }

    fn choose_library_folder(&mut self) {
        if let Some(folder) = rfd::FileDialog::new()
            .set_title("Choose storage folder for captures and projects")
            .set_directory(&self.settings.output_dir)
            .pick_folder()
            && folder != self.settings.output_dir
        {
            self.file_dialog = Some(FileDialog::ChangeStorage {
                destination: folder,
                move_existing: true,
            });
        }
    }

    fn hide(&mut self, ctx: &egui::Context) {
        self.calico.stop();
        if self.desktop.is_none() {
            return;
        }
        self.finish_edit();
        self.source_textures.clear();
        self.export_panel.open = false;
        self.remember_window(ctx);
        self.last_texture = None;
        self.privacy_preview.clear();
        self.library.release_textures();
        if let Some(loader) = &self.image_loader {
            loader.clear();
        }
        self.hidden.store(true, Ordering::Relaxed);
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
    }

    fn show(&mut self, ctx: &egui::Context) {
        if let Some(desktop) = &self.desktop {
            desktop.reveal_application();
        }
        self.hidden.store(false, Ordering::Relaxed);
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    fn begin_capture(&mut self, ctx: &egui::Context) {
        self.close_capture_launcher(ctx);
        self.calico.stop();
        if self.scroll_session.is_some() {
            self.capture_scroll_section(ctx);
            return;
        }
        if self.pending_capture.is_some()
            || self.selection.is_some()
            || self.busy
            || self.recovery.prompt
            || self.about_open
            || self.appearance_open
            || self.guide_open
            || self.pending_action.is_some()
            || self.export_panel.open
            || self.editor.has_modal()
        {
            return;
        }
        // Capture itself does not replace the open project. Ask about unsaved
        // edits only if the completed capture is actually sent to Editor.
        self.finish_edit();
        let Some(display) = self
            .displays
            .iter()
            .find(|d| Some(d.id) == self.settings.display_id)
            .or_else(|| self.displays.iter().find(|d| d.is_primary))
            .or(self.displays.first())
            .cloned()
        else {
            self.error = Some(
                "No display is available. Connect a display and choose Refresh displays.".into(),
            );
            self.show(ctx);
            return;
        };
        // A preflight permission hint can be stale after a local rebuild.
        // ScreenCaptureKit enforces access on this explicit capture action and
        // returns a real permission error; there is no partial-image fallback.
        self.error = None;
        let was_hidden = self.hidden.load(Ordering::Relaxed);
        self.remember_window(ctx);
        self.pending_capture = Some(was_hidden);
        self.generation += 1;
        let generation = self.generation;
        let cancel = Arc::new(AtomicBool::new(false));
        self.capture_cancel = Some(cancel.clone());
        self.hidden.store(true, Ordering::Relaxed);
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        let delay = self.settings.delay_seconds;
        let demo = self.demo;
        let pointer = self.settings.include_pointer;
        let mode = self.settings.capture_mode;
        self.shape = CaptureShape::Rectangle;
        log::info!("Capture requested on display {}", display.id);
        std::thread::spawn(move || {
            // Let the compositor remove our window and a tray menu before capture.
            // This worker exists only for a requested capture; no idle polling.
            std::thread::sleep(Duration::from_millis(250) + Duration::from_secs(delay.into()));
            if cancel.load(Ordering::Acquire) {
                return;
            }
            let at = Local::now();
            let result = if demo {
                Ok(CapturedFrame {
                    image: if mode == capture::CaptureMode::Scrolling {
                        image::RgbaImage::from_fn(1600, 1000, capture_session::demo_scroll_pixel)
                    } else {
                        demo_image()
                    },
                    windows: if matches!(
                        mode,
                        capture::CaptureMode::Region
                            | capture::CaptureMode::MultiRegion
                            | capture::CaptureMode::Window
                            | capture::CaptureMode::Scrolling
                    ) {
                        vec![capture::WindowInfo {
                            id: 0,
                            name: "Demo window".into(),
                            x: display.x + (display.width / 8) as i32,
                            y: display.y + (display.height / 8) as i32,
                            width: display.width * 3 / 4,
                            height: display.height * 3 / 4,
                        }]
                    } else {
                        vec![]
                    },
                })
            } else {
                capture::capture_display(display.id, pointer).and_then(|image| {
                    let windows = if matches!(
                        mode,
                        capture::CaptureMode::Region
                            | capture::CaptureMode::MultiRegion
                            | capture::CaptureMode::Window
                            | capture::CaptureMode::Scrolling
                    ) {
                        match capture::windows() {
                            Ok(w) => w,
                            Err(_)
                                if matches!(
                                    mode,
                                    capture::CaptureMode::Region
                                        | capture::CaptureMode::MultiRegion
                                ) =>
                            {
                                Vec::new()
                            }
                            Err(e) => return Err(e),
                        }
                    } else {
                        vec![]
                    };
                    if matches!(
                        mode,
                        capture::CaptureMode::Window | capture::CaptureMode::Scrolling
                    ) && windows.is_empty()
                    {
                        return Err("No capturable window is available on this desktop.".into());
                    }
                    Ok(CapturedFrame { image, windows })
                })
            };
            let _ = tx.send(WorkEvent::Captured(generation, display, at, result));
            ctx.request_repaint();
        });
    }

    fn desktop_units_per_point(ctx: &egui::Context) -> f32 {
        if cfg!(target_os = "windows") {
            ctx.pixels_per_point()
        } else {
            ctx.zoom_factor()
        }
    }

    fn remember_window(&mut self, ctx: &egui::Context) {
        // Never replace the editor's geometry with the full-display overlay,
        // including frames in which its asynchronous restoration is underway.
        if self.selection.is_some()
            || self.scroll_session.is_some()
            || self.pending_capture.is_some()
            || self.restore_frames_left > 0
            || (self.hidden.load(Ordering::Relaxed) && self.editor_placement.is_some())
        {
            return;
        }
        let geometry = ctx.input(|input| {
            let viewport = input.viewport();
            if viewport.minimized == Some(true) {
                return None;
            }
            Some((
                viewport.outer_rect?,
                viewport.inner_rect?,
                viewport.maximized.unwrap_or(false),
            ))
        });
        let Some((outer, inner, maximized)) = geometry else {
            return;
        };
        if !outer.is_finite() || !inner.is_finite() || inner.width() <= 0.0 || inner.height() <= 0.0
        {
            return;
        }
        if maximized && let Some(placement) = &mut self.editor_placement {
            // Preserve the last normal bounds so unmaximizing after a capture
            // returns to the user's original window size as well.
            placement.maximized = true;
        } else {
            let units_per_point = Self::desktop_units_per_point(ctx);
            self.editor_placement = Some(WindowPlacement {
                outer_position: outer.min * units_per_point,
                inner_size: inner.size() * units_per_point,
                maximized,
            });
        }
    }

    fn apply_window_restoration(&mut self, ctx: &egui::Context) {
        if self.restore_frames_left == 0 {
            return;
        }
        if self.restore_frames_left == 1 {
            // Apply the minimum after moving back, using the editor display's
            // DPI instead of imposing the capture display's physical minimum.
            ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(Vec2::new(980.0, 640.0)));
        }
        if let Some(placement) = self.editor_placement {
            let units_per_point = Self::desktop_units_per_point(ctx);
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(
                placement.inner_size / units_per_point,
            ));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(
                placement.outer_position / units_per_point,
            ));
            if self.restore_frames_left == 1 && placement.maximized {
                ctx.send_viewport_cmd(ViewportCommand::Maximized(true));
            }
        } else {
            // Only used if the platform did not report initial window geometry.
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(1200.0, 800.0)));
        }
        self.restore_frames_left -= 1;
        // The extra pass also lets native geometry settle after the last
        // command. Explicit wakeups continue working when returning to tray.
        ctx.request_repaint();
    }

    fn restore_window(&mut self, ctx: &egui::Context, hide: bool) {
        // Change native styles once per transition. On macOS even querying
        // maximization of a borderless window temporarily adds a title bar;
        // repeated Maximized(false) requests can change its native geometry.
        if cfg!(target_os = "windows") {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(false));
        }
        ctx.send_viewport_cmd(ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(ViewportCommand::Resizable(true));
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::Normal));
        self.restore_frames_left = 4;
        self.apply_window_restoration(ctx);
        if hide {
            self.hide(ctx);
        } else {
            self.show(ctx);
        }
    }

    fn cancel_selection(&mut self, ctx: &egui::Context) {
        if let Some(selection) = self.selection.take() {
            self.restore_window(ctx, selection.return_hidden);
            self.status = "Capture canceled".into();
        }
    }

    fn accept_selection(&mut self, ctx: &egui::Context, rect: selection::PixelRect) {
        let Some(selection) = self.selection.take() else {
            return;
        };
        if self.settings.capture_mode == capture::CaptureMode::Scrolling {
            self.start_scroll_session(ctx, selection, rect);
            return;
        }
        let shape = self.shape;
        let radius = self.settings.corner_radius;
        self.queue_capture_output(ctx, selection.captured_at, move || {
            selection::crop_selection(&selection.image, rect, shape, radius)
        });
    }

    fn open_image(&mut self, ctx: &egui::Context) {
        if self.busy || self.export_panel.open || self.editor.has_modal() {
            return;
        }
        if !self.can_replace_image(ctx, PendingAction::Open) {
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Images and editable projects",
                &[
                    "myimage", "png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "gif",
                ],
            )
            .add_filter("MyImage projects", &["myimage"])
            .add_filter(
                "Images",
                &["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "gif"],
            )
            .pick_file()
        else {
            return;
        };
        self.begin_open(ctx, path, true);
    }

    fn open_path(&mut self, ctx: &egui::Context, path: PathBuf, edit: bool) {
        if self.busy
            || self.export_panel.open
            || self.editor.has_modal()
            || !self.can_replace_image(
                ctx,
                PendingAction::OpenPath {
                    path: path.clone(),
                    edit,
                },
            )
        {
            return;
        }
        self.begin_open(ctx, path, edit);
    }

    fn begin_open(&mut self, ctx: &egui::Context, path: PathBuf, _edit: bool) {
        if self.image_loader.is_none() {
            let tx = self.work_tx.clone();
            let worker_ctx = ctx.clone();
            match crate::image_loading::Loader::start(move |path, result| {
                let event = if result.is_err() && crate::library_files::is_missing(&path) {
                    WorkEvent::MissingOpen(path)
                } else {
                    WorkEvent::Prepared(path, result)
                };
                let _ = tx.send(event);
                worker_ctx.request_repaint();
            }) {
                Ok(loader) => self.image_loader = Some(loader),
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            }
        }
        let side = ctx.input(|i| i.max_texture_side).max(1) as u32;
        match self.image_loader.as_ref().unwrap().open(path, side) {
            Ok(()) => {
                self.busy = true;
                self.status = "Opening image…".into();
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn finish_edit(&mut self) {
        if let Some(document) = &mut self.document {
            self.editor.finish_gesture(document, &mut self.history);
        }
    }

    fn install_document(&mut self, document: Document, project_path: Option<PathBuf>) {
        self.clear_recovery();
        self.install_document_state(document, project_path);
    }

    fn install_document_state(&mut self, document: Document, project_path: Option<PathBuf>) {
        self.last_image = Some(Arc::clone(&document.image));
        self.last_texture = None;
        self.privacy_preview.clear();
        self.history = EditHistory::new(document.edit_state());
        self.recent_revision = self.history.revision();
        self.document = Some(document);
        self.project_path = project_path;
        self.editor.reset();
        self.fit = true;
    }

    fn save_project(&mut self, ctx: &egui::Context, save_as: bool) -> bool {
        if self.busy || self.document.is_none() || self.export_panel.open || self.editor.has_modal()
        {
            return false;
        }
        self.finish_edit();
        let path = if !save_as && self.project_path.is_some() {
            self.project_path.clone().unwrap()
        } else {
            let source = self.project_path.as_ref().or(self.last_path.as_ref());
            let directory =
                storage::project_save_directory(&self.settings, self.project_path.as_deref());
            if let Err(error) = std::fs::create_dir_all(&directory) {
                self.error = Some(format!(
                    "Cannot create project folder {}: {error}",
                    directory.display()
                ));
                return false;
            }
            let stem = source
                .and_then(|p| p.file_stem())
                .and_then(|s| s.to_str())
                .unwrap_or(&self.draft_title);
            let Some(path) = rfd::FileDialog::new()
                .set_directory(directory)
                .set_file_name(format!("{stem}.myimage"))
                .add_filter("Editable MyImage project", &["myimage"])
                .save_file()
            else {
                return false;
            };
            match checked_save_path(path, "myimage") {
                Ok(path) => path,
                Err(error) => {
                    self.error = Some(error);
                    return false;
                }
            }
        };
        let document = self.document.as_ref().unwrap().clone();
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        self.busy = true;
        self.status = "Saving editable project…".into();
        std::thread::spawn(move || {
            let result = document::save_project(&document, &path);
            let _ = tx.send(WorkEvent::ProjectSaved(path, document.edit_state(), result));
            ctx.request_repaint();
        });
        true
    }

    fn begin_export(&mut self) {
        if self.busy
            || self.document.is_none()
            || self.recovery.prompt
            || self.about_open
            || self.appearance_open
            || self.guide_open
            || self.pending_action.is_some()
            || self.editor.has_modal()
        {
            return;
        }
        self.finish_edit();
        self.export_panel.begin();
    }

    fn export_copy(&mut self, ctx: &egui::Context) {
        if self.busy || self.document.is_none() {
            return;
        }
        self.finish_edit();
        self.export_panel.error = None;
        let options = self.export_panel.options;
        let format = options.format;
        let source = self.project_path.as_ref().or(self.last_path.as_ref());
        let directory = source
            .and_then(|p| p.parent())
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| self.settings.output_dir.clone());
        let stem = source
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled");
        let Some(path) = rfd::FileDialog::new()
            .set_title(format!("Export {} copy", format.label()))
            .set_directory(directory)
            .set_file_name(format!("{stem}-annotated.{}", format.extension()))
            .add_filter(format.label(), format.extensions())
            .save_file()
        else {
            return;
        };
        let path = match export::checked_export_path(path, format) {
            Ok(path) => path,
            Err(error) => {
                self.export_panel.error = Some(error);
                return;
            }
        };
        let document = self.document.as_ref().unwrap().clone();
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        self.export_panel.open = false;
        self.busy = true;
        self.status = format!("Exporting {}…", format.label());
        std::thread::spawn(move || {
            let result = export::save(&document, &path, options);
            let _ = tx.send(WorkEvent::Exported(path, format, result));
            ctx.request_repaint();
        });
    }

    fn perform_action(&mut self, ctx: &egui::Context, action: PendingAction, discard: bool) {
        self.allow_discard = discard;
        match action {
            PendingAction::Recover(id) => self.recover_entry(ctx, id),
            PendingAction::OpenCapture(output) => self.open_captured_image(ctx, output),
            PendingAction::Open => self.open_image(ctx),
            PendingAction::OpenProject => self.open_project(ctx),
            PendingAction::Create(spec) => self.create_image(ctx, spec),
            PendingAction::OpenPath { path, edit } => self.open_path(ctx, path, edit),
            PendingAction::Quit => self.request_quit(ctx),
            PendingAction::Files(op) => self.start_file_operation(ctx, op),
        }
        self.allow_discard = false;
    }

    fn handle_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.desktop_rx.try_recv() {
            match event {
                DesktopEvent::Capture => self.begin_capture(ctx),
                DesktopEvent::CapturePanel => self.toggle_capture_launcher(ctx),
                DesktopEvent::Show | DesktopEvent::CaptureSettings | DesktopEvent::Library => {
                    self.close_capture_launcher(ctx);
                    if self.scroll_session.is_some() {
                        if !self.busy {
                            self.show(ctx);
                        }
                        continue;
                    }
                    if self.selection.is_some() {
                        self.cancel_selection(ctx);
                    }
                    self.generation += 1;
                    if self.pending_capture.take().is_some() {
                        self.restore_window(ctx, false);
                    }
                    if let Some(cancel) = self.capture_cancel.take() {
                        cancel.store(true, Ordering::Release);
                    }
                    if !self.busy
                        && !self.guide_open
                        && !self.about_open
                        && !self.appearance_open
                        && !self.recovery.prompt
                        && self.pending_action.is_none()
                        && self.file_dialog.is_none()
                        && !self.export_panel.open
                        && !self.editor.has_modal()
                    {
                        match event {
                            DesktopEvent::Show => {
                                self.finish_edit();
                                self.view = View::Editor;
                            }
                            DesktopEvent::CaptureSettings => {
                                self.finish_edit();
                                self.view = View::Capture;
                            }
                            DesktopEvent::Library => {
                                self.finish_edit();
                                self.view = View::Library;
                                self.library.refresh();
                            }
                            _ => {}
                        }
                    }
                    self.show(ctx);
                }
                DesktopEvent::Quit => {
                    self.request_quit(ctx);
                }
            }
        }
        while let Ok(event) = self.work_rx.try_recv() {
            match event {
                WorkEvent::Recovery(event) => self.recovery_event(ctx, event),
                WorkEvent::StoragePrepared(result) => {
                    self.storage_prepared(ctx, result);
                    if !self.busy && std::mem::take(&mut self.quit_after_work) {
                        self.request_quit(ctx);
                    }
                }
                WorkEvent::StorageProgress(done, total) => {
                    if !self.quit_after_work {
                        self.status = format!("Moving library · verified {done} of {total} files…");
                    }
                }
                WorkEvent::StorageFinished(count, warnings) => {
                    self.busy = false;
                    self.status = if count == 0 {
                        "Storage folder updated".into()
                    } else {
                        format!("Storage updated · {count} files available in Library and Recents")
                    };
                    if !warnings.is_empty() {
                        self.error = Some(format!(
                            "Storage switched successfully. Some originals were retained:\n{}",
                            warnings.join("\n")
                        ));
                    }
                    if std::mem::take(&mut self.quit_after_work) {
                        self.request_quit(ctx);
                    }
                }
                WorkEvent::Files(result) => {
                    self.file_operation_finished(result);
                    if std::mem::take(&mut self.quit_after_work) {
                        self.request_quit(ctx);
                    }
                }
                WorkEvent::ClipboardImage(result) => {
                    self.busy = false;
                    match result {
                        Ok(image) => {
                            self.install_document(
                                Document {
                                    image: Arc::new(image),
                                    annotations: vec![],
                                    geometry: Default::default(),
                                },
                                None,
                            );
                            self.last_path = None;
                            self.draft_title = "Pasted image".into();
                            self.view = View::Editor;
                            self.status = "Image pasted. Save a project to keep your edits.".into();
                            self.error = None;
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                WorkEvent::FileCopied(path, result) => {
                    self.busy = false;
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    match result {
                        Ok([width, height]) => {
                            self.error = None;
                            self.status =
                                format!("Copied to clipboard · {name} · {width} × {height} px");
                        }
                        Err(error) => {
                            self.status = "Copy failed".into();
                            self.error = Some(format!("Could not copy {name}: {error}"));
                        }
                    }
                    if std::mem::take(&mut self.quit_after_work) {
                        self.request_quit(ctx);
                    }
                }
                WorkEvent::Displays(result) => match result {
                    Ok(displays) => {
                        self.displays = displays;
                        if !self
                            .displays
                            .iter()
                            .any(|d| Some(d.id) == self.settings.display_id)
                        {
                            self.settings.display_id = self
                                .displays
                                .iter()
                                .find(|d| d.is_primary)
                                .or(self.displays.first())
                                .map(|d| d.id);
                        }
                    }
                    Err(error) => self.error = Some(error),
                },
                WorkEvent::Captured(generation, display, captured_at, result) => {
                    if generation != self.generation {
                        continue;
                    }
                    let Some(return_hidden) = self.pending_capture.take() else {
                        continue;
                    };
                    self.capture_cancel = None;
                    match result {
                        Ok(frame) => {
                            let image = frame.image;
                            if self.settings.capture_mode == capture::CaptureMode::FullScreen {
                                self.save_captured_image(ctx, image, captured_at);
                                continue;
                            }
                            let texture = load_texture(ctx, "selection", &image);
                            self.selection = Some(Selection {
                                display,
                                image,
                                texture,
                                captured_at,
                                drag: Default::default(),
                                multi: Default::default(),
                                mode: self.settings.capture_mode,
                                return_hidden,
                                placement_frames: 0,
                                windows: frame.windows,
                                window_id: None,
                                hover_cycle: 0,
                                magnifier: true,
                                magnifier_preview: Default::default(),
                                pointer_ready: false,
                            });
                            self.show(ctx);
                        }
                        Err(error) => {
                            self.error = Some(error);
                            self.view = View::Capture;
                            self.show(ctx);
                        }
                    }
                }
                WorkEvent::WindowCaptured(generation, id, crop, at, result) => {
                    if generation != self.generation || self.pending_capture.take().is_none() {
                        continue;
                    }
                    self.capture_cancel = None;
                    match result {
                        Ok(image)
                            if self.settings.capture_mode == capture::CaptureMode::Scrolling =>
                        {
                            self.start_window_scroll(ctx, id, crop, image, at)
                        }
                        Ok(image) => self.save_captured_image(ctx, image, at),
                        Err(error) => {
                            self.restore_window(ctx, false);
                            self.error = Some(error);
                            self.view = View::Capture;
                        }
                    }
                }
                WorkEvent::ScrollSection(generation, stitcher, result, final_sample) => {
                    if generation != self.generation {
                        continue;
                    }
                    self.busy = false;
                    let mut finish = false;
                    let mut show = false;
                    let mut collect_final = false;
                    let failed = result.is_err();
                    if let Some(session) = &mut self.scroll_session {
                        let auto = session.window_id.is_some();
                        match result {
                            Ok(true) => {
                                session.message =
                                    "Section added. Keep scrolling down, or choose Finish.".into();
                                session.texture = None;
                            }
                            Ok(false) => {
                                if !session.paused {
                                    session.message=if auto {"Watching the selected window. Scroll down steadily; sections are collected automatically."} else {"No new content. Scroll down before capturing the next section."}.into();
                                }
                            }
                            Err(error) => {
                                session.message = error;
                                session.paused = auto;
                            }
                        }
                        session.stitcher = Some(*stitcher);
                        session.next_capture =
                            std::time::Instant::now() + Duration::from_millis(200);
                        if final_sample && failed {
                            session.finish_requested = false;
                            session.message.push_str(
                                " Resume to retry, or Finish to keep the accepted sections.",
                            );
                        }
                        finish = session.finish_requested && (final_sample || !auto);
                        collect_final = session.finish_requested && !finish;
                        show = !auto;
                    }
                    if finish {
                        self.finish_scroll_capture(ctx);
                    } else if collect_final {
                        self.capture_scroll_section(ctx);
                    } else if show {
                        self.show(ctx);
                    }
                }
                WorkEvent::CaptureFinished(output) => self.finish_capture_output(ctx, output),
                WorkEvent::Saved(image, path, replace_document) => {
                    self.busy = false;
                    if replace_document {
                        self.install_document(
                            Document {
                                image,
                                annotations: Vec::new(),
                                geometry: Default::default(),
                            },
                            None,
                        );
                    }
                    self.last_texture = None;
                    self.fit = true;
                    match path {
                        Ok(path) => {
                            self.library.refresh();
                            self.status = "Capture saved".into();
                            self.remember_recent(path.clone());
                            self.last_path = Some(path);
                            self.error = None;
                        }
                        Err(error) => {
                            self.last_path = None;
                            self.status = "Capture is in memory — save failed".into();
                            self.error = Some(error);
                        }
                    }
                    if std::mem::take(&mut self.quit_after_work) {
                        self.request_quit(ctx);
                    }
                }
                WorkEvent::MissingOpen(path) => {
                    self.busy = false;
                    self.settings.recent_files.retain(|p| p != &path);
                    self.library.forget_paths(&[path]);
                    self.sync_library_references();
                    self.library.refresh();
                    self.error = None;
                    self.status =
                        "That file is no longer on disk. Its missing entry was removed.".into();
                    if std::mem::take(&mut self.quit_after_work) {
                        self.request_quit(ctx);
                    }
                }
                WorkEvent::Prepared(path, result) => {
                    self.busy = false;
                    match result {
                        Ok(prepared) => {
                            let project_path = is_project_path(&path).then(|| path.clone());
                            self.install_document(prepared.document, project_path);
                            self.last_texture =
                                Some(self.source_textures.get(ctx, prepared.source));
                            if let Some(composition) = prepared.composition {
                                self.privacy_preview.seed(
                                    ctx,
                                    self.document.as_ref().unwrap(),
                                    prepared.max_side,
                                    composition,
                                );
                            }
                            self.remember_opened_recent(path.clone());
                            self.last_path = Some(path);
                            self.view = View::Editor;
                            self.status = "Image opened".into();
                            self.error = None;
                        }
                        Err(error) => {
                            self.error = Some(error);
                            self.status = "Could not open the file".into();
                        }
                    }
                    if std::mem::take(&mut self.quit_after_work) {
                        self.request_quit(ctx);
                    }
                }
                #[cfg(test)]
                WorkEvent::Opened(path, _edit, result) => {
                    self.busy = false;
                    match result {
                        Ok(document) => {
                            let project_path = is_project_path(&path).then(|| path.clone());
                            self.install_document(document, project_path);
                            self.remember_opened_recent(path.clone());
                            self.last_path = Some(path);
                            self.view = View::Editor;
                            self.fit = true;
                            self.status = "Image opened".into();
                            self.error = None;
                        }
                        Err(error) => {
                            self.error = Some(error);
                            self.status = "Could not open the file".into();
                        }
                    }
                    if std::mem::take(&mut self.quit_after_work) {
                        self.request_quit(ctx);
                    }
                }
                WorkEvent::ProjectSaved(path, saved_state, result) => {
                    self.busy = false;
                    match result {
                        Ok(()) => {
                            self.library.refresh();
                            self.history.mark_saved(&saved_state);
                            if self.last_path.is_none() {
                                self.last_path = Some(path.clone());
                            }
                            self.remember_recent(path.clone());
                            self.project_path = Some(path);
                            if !self.has_unsaved_edits() {
                                self.clear_recovery();
                            }
                            self.status =
                                "Project saved • image edits and annotations remain editable"
                                    .into();
                            self.error = None;
                            if let Some(action) = self.after_save.take() {
                                self.perform_action(ctx, action, false);
                            }
                        }
                        Err(error) => {
                            self.error = Some(error);
                            self.status =
                                "Project save failed • your edits are still in memory".into();
                            self.pending_action = self.after_save.take();
                        }
                    }
                    if std::mem::take(&mut self.quit_after_work) {
                        self.request_quit(ctx);
                    }
                }
                WorkEvent::Exported(path, format, result) => {
                    self.busy = false;
                    match result {
                        Ok(()) => {
                            self.library.refresh();
                            self.remember_recent(path.clone());
                            self.status =
                                format!("{} exported to {}", format.label(), path.display());
                            self.error = None;
                        }
                        Err(error) => {
                            self.error = Some(error);
                            self.status = format!("{} export failed", format.label());
                        }
                    }
                    if std::mem::take(&mut self.quit_after_work) {
                        self.request_quit(ctx);
                    }
                }
            }
        }
    }

    fn has_unsaved_capture(&self) -> bool {
        self.last_image.is_some() && self.last_path.is_none()
    }

    fn has_unsaved_edits(&self) -> bool {
        self.has_unsaved_capture()
            || self
                .document
                .as_ref()
                .is_some_and(|document| self.history.is_dirty(document))
    }

    fn can_replace_image(&mut self, ctx: &egui::Context, action: PendingAction) -> bool {
        self.finish_edit();
        if self.allow_discard {
            return true;
        }
        if self.has_unsaved_capture() || self.has_unsaved_edits() {
            self.pending_action = Some(action);
            self.show(ctx);
            false
        } else {
            true
        }
    }

    fn request_quit(&mut self, ctx: &egui::Context) {
        self.close_capture_launcher(ctx);
        if let Some(session) = &mut self.scroll_session {
            session.message = "Finish or cancel the scrolling capture before quitting.".into();
            if !self.busy {
                self.show(ctx);
            }
            return;
        }
        self.export_panel.open = false;
        if self.busy {
            self.quit_after_work = true;
            self.status = "Finishing the current file operation before quitting…".into();
            return;
        }
        if !self.can_replace_image(ctx, PendingAction::Quit) {
            return;
        }
        if let Some(cancel) = self.capture_cancel.take() {
            cancel.store(true, Ordering::Release);
        }
        if self.stop_recovery() {
            return;
        }
        self.quitting = true;
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    fn capture_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        use crate::capture_panel::{self, Action, Availability};
        let response = capture_panel::ui(
            ui,
            &mut self.settings,
            &mut self.shape,
            &self.displays,
            Availability {
                capture: !self.displays.is_empty() && !self.busy,
                background: self.desktop.is_some(),
                permission_needed: cfg!(target_os = "macos")
                    && !self.demo
                    && !capture::screen_capture_permission(),
                demo: self.demo,
            },
            Some(self.app_icon.id()),
        );
        if response.changed {
            self.persist_settings();
        }
        if self.capture_copy_retry.is_some() {
            ui.horizontal(|ui| {
                if appearance::action_button(ui, "Retry copy", Icon::Copy, !self.busy, true)
                    .clicked()
                {
                    self.retry_capture_copy(ctx);
                }
                if appearance::action_button(
                    ui,
                    "Open capture in Editor",
                    Icon::Edit,
                    !self.busy,
                    false,
                )
                .clicked()
                    && let Some(output) = self.capture_copy_retry.clone()
                {
                    self.open_captured_image(ctx, output);
                }
            });
        }
        match response.action {
            Some(Action::Capture) => self.begin_capture(ctx),
            Some(Action::Panel) => self.toggle_capture_launcher(ctx),
            Some(Action::RefreshDisplays) => self.refresh_displays(ctx),
            Some(Action::ChooseFolder) => self.choose_library_folder(),
            Some(Action::Library) => {
                self.view = View::Library;
                self.library.refresh();
            }
            Some(Action::Background) => self.hide(ctx),
            Some(Action::Permission) => {
                let granted = capture::request_screen_capture_permission();
                self.status = if granted { "Screen capture permission granted" } else {
                    "Enable MyImage in macOS Screen & System Audio Recording settings, then reopen if requested."
                }.into();
            }
            None => {}
        }
    }

    fn viewer_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.document.is_some(),
                    appearance::primary(ui.ctx(), "Edit & annotate"),
                )
                .clicked()
            {
                self.view = View::Editor;
            }
            if ui.button("Fit").clicked() {
                self.fit = true;
            }
            if ui.button("100%").clicked() {
                self.fit = false;
                self.zoom = 1.0;
            }
            if crate::appearance::zoom_slider(ui, &mut self.zoom, 0.1..=4.0).changed() {
                self.fit = false;
            }
            ui.weak(format!("{:.0}%", self.zoom * 100.));
        });
        let Some(image) = &self.last_image else {
            ui.add_space(50.0);
            ui.heading("View an image");
            ui.label("Open an existing image, or make your first capture.");
            return;
        };
        if self.last_texture.is_none() {
            self.last_texture = Some(load_texture(ctx, "viewer", image));
        }
        let output_size = self
            .document
            .as_ref()
            .map_or([image.width(), image.height()], Document::output_size);
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("{} × {} pixels", output_size[0], output_size[1]));
            if let Some(path) = &self.last_path {
                ui.weak(path.display().to_string());
            }
        });
        if self.last_path.is_none() && !self.busy && ui.button("Retry saving capture").clicked() {
            let image = Arc::clone(image);
            let tx = self.work_tx.clone();
            let ctx = ctx.clone();
            let root = self.settings.output_dir.clone();
            self.busy = true;
            std::thread::spawn(move || {
                let path = storage::save_capture(&image, &root, Local::now());
                let _ = tx.send(WorkEvent::Saved(image, path, false));
                ctx.request_repaint();
            });
        }
        let size = Vec2::new(output_size[0] as f32, output_size[1] as f32);
        let available = ui.available_size() - Vec2::splat(14.0);
        if self.fit {
            self.zoom = (available.x / size.x)
                .min(available.y / size.y)
                .clamp(0.01, 1.0);
        }
        let pinch_offset = crate::canvas_zoom::prepare(
            ui,
            "viewer-canvas-scroll",
            &mut self.zoom,
            &mut self.fit,
            false,
            true,
        );
        let mut scroll = egui::ScrollArea::both().id_salt("viewer-canvas-scroll");
        if let Some(offset) = pinch_offset {
            scroll = scroll.scroll_offset(offset);
        }
        scroll
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, visible| {
                let (rect, _) = ui.allocate_exact_size(size * self.zoom, egui::Sense::hover());
                crate::canvas_zoom::remember(
                    ui,
                    "viewer-canvas-scroll",
                    rect,
                    Rect::from_min_size(rect.min + visible.min.to_vec2(), visible.size())
                        .intersect(ui.clip_rect()),
                );
                let painter = ui.painter();
                let cell = 16.0;
                let clipped = rect.intersect(ui.clip_rect());
                let min_x = ((clipped.min.x - rect.min.x) / cell).floor().max(0.0) as u32;
                let max_x = ((clipped.max.x - rect.min.x) / cell).ceil().max(0.0) as u32;
                let min_y = ((clipped.min.y - rect.min.y) / cell).floor().max(0.0) as u32;
                let max_y = ((clipped.max.y - rect.min.y) / cell).ceil().max(0.0) as u32;
                for y in min_y..max_y {
                    for x in min_x..max_x {
                        let square = Rect::from_min_size(
                            rect.min + Vec2::new(x as f32 * cell, y as f32 * cell),
                            Vec2::splat(cell),
                        )
                        .intersect(rect);
                        painter.rect_filled(
                            square,
                            0,
                            Color32::from_gray(if (x + y) % 2 == 0 { 54 } else { 69 }),
                        );
                    }
                }
                if let Some(texture) = &self.last_texture {
                    if let Some(document) = &self.document {
                        let painter = painter.with_clip_rect(rect.intersect(ui.clip_rect()));
                        if document.geometry.canvas.is_some()
                            || (document.geometry.has_cutouts()
                                || document.geometry.effects.active())
                            || document
                                .annotations
                                .iter()
                                .any(|a| a.visible() && a.kind.needs_composite())
                        {
                            match self.privacy_preview.texture(ctx, document) {
                                Ok(Some(processed)) => render::paint_document_image(
                                    &painter,
                                    processed.id(),
                                    document,
                                    rect,
                                ),
                                state => {
                                    painter.rect_filled(
                                        rect,
                                        0,
                                        appearance::palette(ui.ctx()).paper,
                                    );
                                    let message = match state {
                                        Err(error) => {
                                            format!("Privacy preview unavailable: {error}")
                                        }
                                        _ => "Updating privacy preview…".into(),
                                    };
                                    painter.text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        message,
                                        egui::FontId::proportional(14.0),
                                        appearance::palette(ui.ctx()).muted,
                                    );
                                }
                            }
                        } else {
                            self.privacy_preview.clear();
                            render::paint_document_image(&painter, texture.id(), document, rect);
                            render::paint_document_annotations(
                                &painter, document, rect.min, self.zoom,
                            );
                        }
                    } else {
                        painter.image(
                            texture.id(),
                            rect,
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                }
            });
    }

    fn library_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let action = self.library.ui(ui, &self.settings.output_dir);
        self.sync_library_references();
        if let Some(action) = action {
            self.library_action(ctx, action);
        }
    }
    fn home_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if ui.is_enabled()
            && !self.busy
            && ui.input_mut(|input| {
                let paste = input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Paste(_)));
                input
                    .events
                    .retain(|event| !matches!(event, egui::Event::Paste(_)));
                paste
            })
        {
            self.paste_new_image(ctx);
        }
        let top_space = (ui.available_height() * 0.07).clamp(8., 32.);
        egui::ScrollArea::vertical()
            .id_salt("home_start")
            .show(ui, |ui| {
                ui.add_space(top_space);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new(
                            "Open a recent image, capture your screen, or create from a template.",
                        )
                        .size(18.)
                        .color(appearance::palette(ui.ctx()).ink),
                    );
                    ui.add_space(22.);
                    let columns = if ui.available_width() >= 600. { 2 } else { 1 };
                    let gap = 14.;
                    let width = ((ui.available_width() - gap).min(606.) / columns as f32).min(296.);
                    let actions = [
                        (
                            "Open image",
                            "Browse your files",
                            Icon::Open,
                            appearance::palette(ui.ctx()).navy,
                        ),
                        (
                            "Capture",
                            "Region, window or screen",
                            Icon::Capture,
                            appearance::palette(ui.ctx()).ginger,
                        ),
                        (
                            "Paste image",
                            "From your clipboard",
                            Icon::Copy,
                            Color32::WHITE,
                        ),
                        (
                            "Create",
                            "Start from a template",
                            Icon::Create,
                            appearance::palette(ui.ctx()).cream,
                        ),
                    ];
                    ui.allocate_ui_with_layout(
                        egui::vec2(width * columns as f32 + gap * (columns - 1) as f32, 0.),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
                            for (row, chips) in actions.chunks(columns).enumerate() {
                                ui.horizontal(|ui| {
                                    for (col, &(title, hint, icon, color)) in
                                        chips.iter().enumerate()
                                    {
                                        if appearance::start_chip(
                                            ui,
                                            title,
                                            hint,
                                            icon,
                                            color,
                                            width,
                                            matches!(icon, Icon::Open)
                                                .then_some(self.app_icon.id()),
                                        )
                                        .clicked()
                                        {
                                            match row * columns + col {
                                                0 => self.open_image(ctx),
                                                1 => self.view = View::Capture,
                                                2 => self.paste_new_image(ctx),
                                                _ => self.view = View::Create,
                                            }
                                        }
                                    }
                                });
                            }
                        },
                    );
                });
                if self.document.is_some() {
                    ui.add_space(18.);
                    ui.vertical_centered(|ui| {
                        if appearance::action_button(
                            ui,
                            "Continue editing",
                            Icon::Edit,
                            true,
                            false,
                        )
                        .clicked()
                        {
                            self.view = View::Editor;
                        }
                    });
                }
            });
    }

    fn editor_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.document.is_none() && ui.is_enabled() && !self.busy {
            let paste = ui.input_mut(|i| {
                let paste = i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)));
                i.events.retain(|e| !matches!(e, egui::Event::Paste(_)));
                paste
            });
            if paste {
                self.paste_new_image(ctx);
            }
        }
        let Some(document) = &mut self.document else {
            self.home_ui(ui, ctx);
            return;
        };
        if self.last_texture.is_none() {
            self.last_texture = Some(load_texture(ctx, "editor", &document.image));
        }
        self.editor.ui(
            ui,
            document,
            &mut self.history,
            self.last_texture.as_ref().unwrap(),
            &mut self.privacy_preview,
        );
    }
}

impl eframe::App for MyImageApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // The resident launcher's transparent viewport needs true alpha around
        // its rounded shell. Other windows paint their opaque panel surfaces.
        [0.; 4]
    }
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        crate::image_clipboard::complete_paste_input(ctx, input);
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.calico.cancel_on_escape(ctx);
        if self.restore_frames_left > 0 {
            self.apply_window_restoration(ctx);
        } else {
            // Record normal geometry before the first --background hide and
            // before processing a hotkey that replaces this window with capture.
            self.remember_window(ctx);
        }
        if !self.initialized {
            self.initialized = true;
            self.start_recovery(ctx);
            #[cfg(target_os = "macos")]
            match crate::desktop::TerminationGuard::install(ctx.clone(), self.desktop_tx.clone()) {
                Ok(guard) => self._termination_guard = Some(guard),
                Err(error) => {
                    self.error = Some(format!("Native quit protection unavailable: {error}"))
                }
            }
            match DesktopIntegration::new(ctx.clone(), self.desktop_tx.clone()) {
                Ok(desktop) => self.desktop = Some(desktop),
                Err(error) => {
                    self.error = Some(format!("Background integration unavailable: {error}"))
                }
            }
            self.refresh_displays(ctx);
            if self.background_start && self.desktop.is_some() {
                self.hide(ctx);
            }
        }
        self.handle_events(ctx);
        self.promote_edited_recent(ctx);
        self.autosave(ctx);
        if let Some(desktop) = &mut self.desktop {
            desktop.update_capture_menu(
                crate::capture_panel::menu_summary(&self.settings, self.shape, &self.displays),
                !self.displays.is_empty()
                    && !self.busy
                    && self.pending_capture.is_none()
                    && self.selection.is_none()
                    && !self.guide_open
                    && !self.about_open
                    && !self.appearance_open
                    && !self.recovery.prompt
                    && self.pending_action.is_none()
                    && self.file_dialog.is_none()
                    && !self.export_panel.open
                    && !self.editor.has_modal(),
            );
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            if let Some(session) = &mut self.scroll_session {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                session.message =
                    "Use Finish to keep this capture, or Cancel to discard it.".into();
            } else if self.selection.is_some() {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.cancel_selection(ctx);
            } else if self.desktop.is_some() {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.hide(ctx);
            } else {
                self.request_quit(ctx);
                if !self.quitting {
                    ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                }
            }
        }
        if !self.quitting {
            self.capture_launcher_ui(ctx);
        }
        if self.quitting || self.hidden.load(Ordering::Relaxed) {
            return;
        }
        if self.scroll_session.is_some() {
            self.scroll_session_ui(ctx);
            crate::pointer_style::paint(ctx, self.settings.pointer_style);
            return;
        }
        if self.selection.is_some() {
            self.selection_ui(ctx);
            crate::pointer_style::paint(ctx, self.settings.pointer_style);
            return;
        }
        if !self.busy
            && !self.guide_open
            && !self.about_open
            && !self.appearance_open
            && !self.recovery.prompt
            && self.pending_action.is_none()
            && self.file_dialog.is_none()
            && !self.export_panel.open
            && !self.editor.has_modal()
        {
            let save = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
            let save_as = egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::S,
            );
            if ctx.input_mut(|i| i.consume_shortcut(&save_as)) {
                self.save_project(ctx, true);
            } else if ctx.input_mut(|i| i.consume_shortcut(&save)) {
                self.save_project(ctx, false);
            }
        }
        egui::TopBottomPanel::top("document_bar")
            .frame(egui::Frame::NONE.fill(Color32::WHITE).inner_margin(10))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        let title = if self.view == View::Home {
                            "Home".into()
                        } else if self.view == View::Capture {
                            "Capture".into()
                        } else if self.view == View::Create {
                            "Create".into()
                        } else if self.view == View::Library {
                            "Your image library".into()
                        } else {
                            self.project_path
                                .as_ref()
                                .or(self.last_path.as_ref())
                                .and_then(|p| p.file_name())
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_else(|| {
                                    if self.document.is_some() {
                                        self.draft_title.clone()
                                    } else {
                                        "Editor".into()
                                    }
                                })
                        };
                        ui.set_max_width((ui.available_width() - 640.0).max(150.0));
                        ui.add(
                            egui::Label::new(egui::RichText::new(title).strong().size(16.0))
                                .truncate(),
                        );
                        ui.label(
                            egui::RichText::new(if self.view == View::Home {
                                "Capture. Annotate. Make it clear."
                            } else if self.view == View::Capture {
                                "Set up here. Capture from anywhere."
                            } else if self.view == View::Create {
                                "A head start for your next visual guide."
                            } else if self.view == View::Library && self.has_unsaved_edits() {
                                "Your current image has unsaved changes in Edit"
                            } else if self.view == View::Library {
                                "Every capture. Ready when you need it."
                            } else if self.view == View::Viewer {
                                "View only · Choose Editor to annotate"
                            } else if self.has_unsaved_edits() {
                                "Unsaved changes"
                            } else if self.project_path.is_some() {
                                "Editable project · Saved"
                            } else {
                                "Capture. Annotate. Make it clear."
                            })
                            .size(12.0)
                            .color(appearance::palette(ui.ctx()).muted),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_enabled_ui(
                            !self.busy
                                && !self.guide_open
                                && !self.about_open
                                && !self.appearance_open
                                && !self.recovery.prompt
                                && self.pending_action.is_none()
                                && self.file_dialog.is_none()
                                && !self.export_panel.open
                                && !self.editor.has_modal(),
                            |ui| {
                                let has_doc = self.document.is_some();
                                if matches!(self.view, View::Viewer | View::Editor) {
                                    if appearance::action_button(
                                        ui,
                                        "Save project",
                                        Icon::Save,
                                        has_doc,
                                        true,
                                    )
                                    .clicked()
                                    {
                                        self.save_project(ctx, false);
                                    }
                                    if appearance::action_button(
                                        ui,
                                        "Export…",
                                        Icon::Export,
                                        has_doc,
                                        false,
                                    )
                                    .clicked()
                                    {
                                        self.begin_export();
                                    }
                                    if self.view == View::Editor {
                                        ui.add_enabled_ui(has_doc, |ui| {
                                            ui.menu_button("Image ▾", |ui| {
                                                if let Some(document) = &mut self.document {
                                                    self.editor.image_menu(
                                                        ui,
                                                        document,
                                                        &mut self.history,
                                                    );
                                                }
                                            })
                                            .response
                                            .on_hover_text(
                                                "Resize, extend canvas, rotate, flip and crop",
                                            );
                                        });
                                    }
                                    if appearance::action_button(
                                        ui,
                                        "Save as…",
                                        Icon::SaveAs,
                                        has_doc,
                                        false,
                                    )
                                    .clicked()
                                    {
                                        self.save_project(ctx, true);
                                    }
                                }
                                if appearance::action_button(
                                    ui,
                                    "Open project…",
                                    Icon::OpenProject,
                                    true,
                                    false,
                                )
                                .clicked()
                                {
                                    self.open_project(ctx);
                                }
                                if appearance::action_button(ui, "Open…", Icon::Open, true, false)
                                    .clicked()
                                {
                                    self.open_image(ctx);
                                }
                            },
                        );
                    });
                });
            });
        let previous_view = self.view;
        egui::TopBottomPanel::top("workspace_toolbar")
            .frame(egui::Frame::NONE.fill(Color32::WHITE).inner_margin(6))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(3., 3.);
                ui.add_enabled_ui(
                    !self.busy
                        && !self.guide_open
                        && !self.about_open
                        && !self.appearance_open
                        && !self.recovery.prompt
                        && self.pending_action.is_none()
                        && self.file_dialog.is_none()
                        && !self.export_panel.open
                        && !self.editor.has_modal(),
                    |ui| {
                        ui.horizontal(|ui| {
                            for (view, label, icon) in [
                                (View::Home, "Home", Icon::Home),
                                (View::Editor, "Editor", Icon::Edit),
                                (View::Library, "Library", Icon::Library),
                                (View::Capture, "Capture", Icon::Capture),
                                (View::Create, "Create", Icon::Create),
                            ] {
                                if appearance::nav_button(
                                    ui,
                                    label,
                                    icon,
                                    self.view == view
                                        || (self.view == View::Viewer && view == View::Editor),
                                )
                                .clicked()
                                {
                                    self.view = view;
                                }
                            }
                            if self.view == View::Editor {
                                ui.add_space(12.);
                                ui.separator();
                                ui.add_space(12.);
                                if let Some(document) = &mut self.document {
                                    self.editor.toolbar(ui, document, &mut self.history);
                                }
                            }
                        });
                    },
                );
            });
        if let Some(pins) = self.editor.take_toolbar_change() {
            self.settings.editor_toolbar = pins;
            self.settings.toolbar_layout_version = self.settings.toolbar_layout_version.max(1);
            self.persist_settings();
        }
        if previous_view != self.view {
            self.finish_edit();
            if self.view == View::Library {
                self.library.refresh();
            }
        }
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.busy {
                    ui.spinner();
                }
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let menu = ui
                        .scope(|ui| {
                            ui.spacing_mut().button_padding = egui::vec2(4., 3.);
                            egui::containers::menu::MenuButton::from_button(
                                egui::Button::image(
                                    egui::Image::new((self.app_icon.id(), egui::vec2(28., 28.)))
                                        .alt_text("App menu"),
                                )
                                .min_size(egui::vec2(36., 34.))
                                .corner_radius(9),
                            )
                            .config(app_menu::config(ctx))
                            .ui(ui, |ui| self.app_menu_contents(ui, ctx))
                        })
                        .inner;
                    menu.0.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            ui.is_enabled(),
                            "App menu",
                        )
                    });
                    menu.0.on_hover_text("App menu");
                    if let Some(warning) = &self.recovery.warning {
                        ui.colored_label(appearance::palette(ui.ctx()).ginger, "Autosave needs attention").on_hover_text(warning);
                    } else if self.recovery.writing {
                        ui.weak("Autosaving…");
                    } else if let Some(time) = &self.recovery.saved_at {
                        ui.weak(format!("Autosaved {time}")).on_hover_text("Recovery copy saved. Use Save project for your permanent editable file.");
                    }
                });
            });
        });
        if matches!(self.view, View::Home | View::Editor | View::Viewer) {
            egui::TopBottomPanel::bottom("recents")
                .frame(egui::Frame::NONE.fill(Color32::WHITE).inner_margin(4))
                .show(ctx, |ui| {
                    ui.spacing_mut().item_spacing.y = 3.;
                    let enabled = !self.busy
                        && !self.guide_open
                        && !self.about_open
                        && !self.appearance_open
                        && !self.recovery.prompt
                        && self.pending_action.is_none()
                        && self.file_dialog.is_none()
                        && !self.export_panel.open
                        && !self.editor.has_modal();
                    let was_open = self.settings.recents_open;
                    let mut chosen = self.library.recents_header(
                        ui,
                        &mut self.settings.recents_open,
                        enabled,
                        |ui| {
                            if self.view == View::Editor
                                && let Some(document) = &mut self.document
                            {
                                ui.add_enabled_ui(enabled, |ui| {
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
                        chosen = ui
                            .add_enabled_ui(enabled, |ui| {
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
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(error) = &self.error {
                dialogs::notice(ui, DialogIcon::Info, "MyImage needs attention", error, true);
                if dialogs::secondary(ui, "Dismiss message", DialogIcon::Close).clicked() {
                    self.error = None;
                }
            }
            if let Some(error) = self
                .desktop
                .as_ref()
                .and_then(|desktop| desktop.hotkey_error())
                .map(str::to_string)
            {
                dialogs::notice(
                    ui,
                    DialogIcon::Capture,
                    "Capture shortcut unavailable",
                    &error,
                    true,
                );
                if dialogs::secondary(ui, "Retry unavailable shortcuts", DialogIcon::Refresh)
                    .clicked()
                    && let Some(desktop) = &mut self.desktop
                {
                    let _ = desktop.retry_hotkey();
                }
            }
            ui.add_enabled_ui(
                !self.busy
                    && !self.guide_open
                    && !self.about_open
                    && !self.appearance_open
                    && !self.recovery.prompt
                    && self.pending_action.is_none()
                    && self.file_dialog.is_none()
                    && !self.export_panel.open,
                |ui| match self.view {
                    View::Home => self.home_ui(ui, ctx),
                    View::Capture => self.capture_ui(ui, ctx),
                    View::Library => self.library_ui(ui, ctx),
                    View::Viewer => self.viewer_ui(ui, ctx),
                    View::Editor => self.editor_ui(ui, ctx),
                    View::Create => {
                        if let Some(spec) = self.create_panel.ui(ui) {
                            self.create_image(ctx, spec);
                        }
                    }
                },
            );
        });
        if let Some(document) = &self.document {
            let size = document.output_size();
            let has_privacy = document
                .annotations
                .iter()
                .any(|a| a.visible() && a.kind.is_effect());
            if self.export_panel.show(ctx, size, has_privacy) {
                self.export_copy(ctx);
            }
        }
        self.file_dialog_ui(ctx);
        self.recovery_ui(ctx);
        self.about_ui(ctx);
        self.appearance_ui(ctx);
        if self.recovery.ready
            && !self.recovery.prompt
            && self.pending_action.is_none()
            && self.file_dialog.is_none()
            && crate::onboarding::show(ctx, &mut self.guide_open, &self.app_icon)
        {
            self.settings.guide_version = crate::onboarding::VERSION;
            self.persist_settings();
        }
        if self.pending_action.is_none()
            && self.file_dialog.is_none()
            && !self.export_panel.open
            && !self.editor.has_modal()
        {
            self.calico.paint(ctx);
        } else {
            self.calico.stop();
        }
        if let Some(action) = self.pending_action.clone() {
            let prompt = Dialog::new("Save your changes?", "Keep your work ready for next time.", DialogIcon::Save, 470.).modal(ctx, egui::Id::new("unsaved_document"), |ui| {
                ui.label(
                    "Save an editable project to keep the original image, image edits, and every annotation.",
                );
                if self.has_unsaved_capture() {
                    ui.label("This image has no saved copy yet.");
                }
                dialogs::actions(ui, |ui| {
                    if dialogs::secondary(ui, "Keep editing", DialogIcon::Back).clicked() {
                        self.pending_action = None;
                    }
                    if dialogs::danger(ui, "Discard changes", DialogIcon::Trash).clicked() {
                        self.pending_action = None;
                        self.perform_action(ctx, action.clone(), true);
                    }
                    if dialogs::primary(ui, "Save and continue", DialogIcon::Save).clicked()
                        && self.save_project(ctx, false)
                    {
                        self.pending_action = None;
                        self.after_save = Some(action);
                    }
                });
            });
            if prompt.should_close() {
                self.pending_action = None;
            }
        }
        self.promote_edited_recent(ctx);
        self.autosave(ctx);
        crate::pointer_style::paint(ctx, self.settings.pointer_style);
    }
}

fn load_texture(ctx: &egui::Context, name: &str, image: &image::RgbaImage) -> egui::TextureHandle {
    // Keep the original pixels for cropping/saving, but never exceed the GPU's
    // texture limit when opening a large image or spanning a high-DPI display.
    let limit = ctx.input(|input| input.max_texture_side).max(1) as u32;
    let preview;
    let image = if image.width() > limit || image.height() > limit {
        let scale = limit as f64 / image.width().max(image.height()) as f64;
        preview = image::imageops::resize(
            image,
            (image.width() as f64 * scale).round().max(1.0) as u32,
            (image.height() as f64 * scale).round().max(1.0) as u32,
            image::imageops::FilterType::Triangle,
        );
        &preview
    } else {
        image
    };
    ctx.load_texture(
        name,
        egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        ),
        egui::TextureOptions::LINEAR,
    )
}

fn demo_image() -> image::RgbaImage {
    image::RgbaImage::from_fn(1600, 1000, |x, y| {
        if x % 100 < 2 || y % 100 < 2 {
            image::Rgba([235, 240, 245, 255])
        } else {
            image::Rgba([
                (x * 180 / 1600 + 30) as u8,
                (y * 150 / 1000 + 35) as u8,
                125,
                255,
            ])
        }
    })
}

fn is_project_path(path: &std::path::Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("myimage"))
}

fn checked_save_path(mut path: PathBuf, extension: &str) -> Result<PathBuf, String> {
    if let Some(actual) = path.extension() {
        if !actual.eq_ignore_ascii_case(extension) {
            return Err(format!(
                "Please use a .{extension} filename for this file type."
            ));
        }
    } else {
        path.set_extension(extension);
        if path.exists() {
            return Err(format!(
                "{} already exists. Choose its full filename in the save dialog to confirm replacement.",
                path.display()
            ));
        }
    }
    Ok(path)
}

#[cfg(test)]
mod loading_performance {
    use super::*;

    #[test]
    #[ignore = "manual opening performance measurement"]
    fn measure_opening_pipeline() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.png");
        let pixels = image::RgbaImage::from_fn(3200, 2000, |x, y| {
            image::Rgba([(x % 251) as u8, (y % 241) as u8, ((x + y) % 239) as u8, 255])
        });
        pixels.save(&path).unwrap();
        let ctx = egui::Context::default();
        for side in [8192, 2048] {
            let _ = ctx.run(
                egui::RawInput {
                    max_texture_side: Some(side),
                    ..Default::default()
                },
                |_| {},
            );
            let start = std::time::Instant::now();
            let pixels = image::ImageReader::open(&path)
                .unwrap()
                .decode()
                .unwrap()
                .into_rgba8();
            let decoded = start.elapsed();
            let texture = load_texture(&ctx, "benchmark", &pixels);
            eprintln!(
                "opening baseline side={side}: decode={decoded:?}, UI texture preparation={:?}, total={:?}",
                start.elapsed() - decoded,
                start.elapsed()
            );
            let _ = std::hint::black_box(texture);
            let mut cache = crate::image_loading::LoadCache::default();
            let mut textures = crate::texture_cache::SourceTextures::default();
            for attempt in 0..3 {
                let start = std::time::Instant::now();
                let prepared = cache.open(&path, side as u32).unwrap();
                let background = start.elapsed();
                let texture = textures.get(&ctx, prepared.source);
                eprintln!(
                    "opening prepared side={side} attempt={attempt}: background={background:?}, UI enqueue={:?}, total={:?}",
                    start.elapsed() - background,
                    start.elapsed()
                );
                let _ = std::hint::black_box(texture);
            }
        }
    }
}
