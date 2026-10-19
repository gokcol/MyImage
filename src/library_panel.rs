use crate::{
    appearance,
    library::{self, EntryKind, LibraryEntry, LibraryIndex},
};
use chrono::{Datelike, NaiveDate};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, RichText, Stroke, Vec2};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
};

const MAX_PENDING: usize = 16;
const MAX_TEXTURES: usize = 128;
const MAX_TEXTURE_BYTES: usize = 32 * 1024 * 1024;
const MAX_REPLIES_PER_FRAME: usize = 2;
#[path = "library_actions.rs"]
mod actions;
#[path = "library_availability.rs"]
mod availability;
#[path = "library_browser.rs"]
mod browser;
#[path = "library_selection.rs"]
mod selection;
use browser::{Collection, Location};
use selection::FileSelection;

pub enum LibraryAction {
    Open(PathBuf),
    Storage,
    Trash(Vec<PathBuf>),
    Duplicate(Vec<PathBuf>),
    Rename(PathBuf),
    Info(PathBuf),
    Reveal(PathBuf),
    RevealLibrary(PathBuf),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DateFilter {
    #[default]
    All,
    Year(i32),
    Month(i32, u32),
    Day(NaiveDate),
}

impl DateFilter {
    fn includes(self, date: NaiveDate) -> bool {
        match self {
            Self::All => true,
            Self::Year(year) => date.year() == year,
            Self::Month(year, month) => date.year() == year && date.month() == month,
            Self::Day(day) => date == day,
        }
    }
}

type Calendar = BTreeMap<i32, BTreeMap<u32, BTreeMap<NaiveDate, usize>>>;

enum Request {
    Scan(u64, PathBuf),
    CheckPaths(u64, Vec<PathBuf>),
    Thumbnail(u64, LibraryEntry),
}

enum Reply {
    Scan(u64, Result<LibraryIndex, String>),
    Missing(u64, Vec<PathBuf>),
    Thumbnail(u64, PathBuf, Result<PreparedThumbnail, String>),
}

struct PreparedThumbnail {
    image: egui::ColorImage,
    original_size: [u32; 2],
}

impl From<library::Thumbnail> for PreparedThumbnail {
    fn from(thumbnail: library::Thumbnail) -> Self {
        Self {
            image: crate::image_loading::color_image(&thumbnail.image),
            original_size: thumbnail.original_size,
        }
    }
}

struct Worker {
    requests: SyncSender<Request>,
    replies: Receiver<Reply>,
}

impl Worker {
    fn start(ctx: egui::Context, generation: Arc<AtomicU64>) -> Result<Self, String> {
        let (requests, receive) = mpsc::sync_channel(MAX_PENDING + 1);
        let (send, replies) = mpsc::channel();
        std::thread::Builder::new()
            .name("myimage-library".into())
            .spawn(move || {
                while let Ok(request) = receive.recv() {
                    let version = match &request {
                        Request::Scan(version, _)
                        | Request::Thumbnail(version, _)
                        | Request::CheckPaths(version, _) => *version,
                    };
                    if version != generation.load(Ordering::Relaxed) {
                        continue;
                    }
                    let reply = match request {
                        Request::CheckPaths(version, paths) => Reply::Missing(
                            version,
                            paths
                                .into_iter()
                                .filter(|p| crate::library_files::is_missing(p))
                                .collect(),
                        ),
                        Request::Scan(version, root) => Reply::Scan(version, library::scan(&root)),
                        Request::Thumbnail(version, entry) => {
                            let result = library::thumbnail(&entry).map(PreparedThumbnail::from);
                            if result.is_err() && crate::library_files::is_missing(&entry.path) {
                                Reply::Missing(version, vec![entry.path])
                            } else {
                                Reply::Thumbnail(version, entry.path, result)
                            }
                        }
                    };
                    if version == generation.load(Ordering::Relaxed) {
                        if send.send(reply).is_err() {
                            break;
                        }
                        // The worker wakes the UI only when there is new information.
                        ctx.request_repaint();
                    }
                }
            })
            .map_err(|error| format!("Could not start the library worker: {error}"))?;
        Ok(Self { requests, replies })
    }
}

enum Preview {
    Ready {
        texture: egui::TextureHandle,
        original_size: [u32; 2],
    },
    Failed(String),
}

struct CachedPreview {
    preview: Preview,
    bytes: usize,
    used: u64,
}

pub struct LibraryPanel {
    root: Option<PathBuf>,
    preferences: library::BrowserPreferences,
    preferences_dirty: bool,
    collection: Collection,
    back: Vec<Location>,
    forward: Vec<Location>,
    generation: Arc<AtomicU64>,
    worker: Option<Worker>,
    needs_scan: bool,
    scanning: bool,
    entries: Vec<LibraryEntry>,
    calendar: Calendar,
    filter: DateFilter,
    search: String,
    filtered: Vec<usize>,
    filter_dirty: bool,
    reset_scroll: bool,
    selection: FileSelection,
    recent_selection: FileSelection,
    pending: HashSet<PathBuf>,
    checked_paths: HashMap<PathBuf, std::time::Instant>,
    missing_paths: HashSet<PathBuf>,
    previews: HashMap<PathBuf, CachedPreview>,
    preview_bytes: usize,
    frame: u64,
    error: Option<String>,
    warnings: Vec<String>,
}

impl Default for LibraryPanel {
    fn default() -> Self {
        Self {
            root: None,
            preferences: Default::default(),
            preferences_dirty: false,
            collection: Collection::All,
            back: Vec::new(),
            forward: Vec::new(),
            generation: Arc::new(AtomicU64::new(0)),
            worker: None,
            needs_scan: true,
            scanning: false,
            entries: Vec::new(),
            calendar: Calendar::new(),
            filter: DateFilter::All,
            search: String::new(),
            filtered: Vec::new(),
            filter_dirty: true,
            reset_scroll: true,
            selection: FileSelection::default(),
            recent_selection: FileSelection::default(),
            pending: HashSet::new(),
            checked_paths: HashMap::new(),
            missing_paths: HashSet::new(),
            previews: HashMap::new(),
            preview_bytes: 0,
            frame: 0,
            error: None,
            warnings: Vec::new(),
        }
    }
}

impl Drop for LibraryPanel {
    fn drop(&mut self) {
        // Cancel queued work before the sender and receiver disconnect.
        self.generation.fetch_add(1, Ordering::Relaxed);
    }
}

impl LibraryPanel {
    pub fn recents_ui(
        &mut self,
        ui: &mut egui::Ui,
        root: &Path,
        paths: &[PathBuf],
        active: Option<&Path>,
        dirty: bool,
    ) -> Option<LibraryAction> {
        if self.root.as_deref() != Some(root) {
            self.refresh();
            self.root = Some(root.to_owned());
        }
        self.frame = self.frame.wrapping_add(1);
        self.receive(ui.ctx());
        self.request_scan(ui.ctx(), root);
        let mut entries = Vec::new();
        let mut seen = HashSet::new();
        for path in paths.iter().take(24) {
            if !self.preferences.hidden_recents.contains(path)
                && !self.missing_paths.contains(path)
                && seen.insert(path.clone())
            {
                entries.push(LibraryEntry {
                    path: path.clone(),
                    name: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    date: chrono::Local::now().date_naive(),
                    kind: if path
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("myimage"))
                    {
                        EntryKind::Project
                    } else {
                        EntryKind::Image
                    },
                    modified: None,
                    created: None,
                    bytes: 0,
                });
            }
        }
        for entry in &self.entries {
            if entries.len() >= 24 {
                break;
            }
            if !self.preferences.hidden_recents.contains(&entry.path)
                && seen.insert(entry.path.clone())
            {
                entries.push(entry.clone());
            }
        }
        let order: Vec<_> = entries.iter().map(|e| e.path.clone()).collect();
        self.check_paths(ui.ctx(), &order);
        self.recent_selection.retain(&order);
        let mut chosen = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().interact_size.y = 18.;
            ui.spacing_mut().button_padding = Vec2::new(4., 2.);
            let count = self.recent_selection.paths.len();
            ui.small(if count == 0 {
                "Ctrl/⌘-click to select · Shift-click for a range".into()
            } else {
                format!("{count} selected")
            });
            if count > 0 {
                if ui.small_button("Move to Trash…").clicked() {
                    chosen = Some(LibraryAction::Trash(self.recent_selection.list()));
                }
                if ui.small_button("Clear selection").clicked() {
                    self.recent_selection.clear();
                }
            }
        });
        egui::ScrollArea::horizontal()
            .id_salt("recent-captures-strip")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.;
                    if entries.is_empty() {
                        ui.weak(
                            "Recent captures and projects appear here as you open or save them.",
                        );
                    }
                    for entry in &entries {
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::new(112., 76.), egui::Sense::click());
                        if !ui.is_rect_visible(rect) {
                            continue;
                        }
                        let is_active = active == Some(entry.path.as_path());
                        let normal_click =
                            self.item_interaction(ui, &response, &entry.path, true, &order);
                        let selected = self.recent_selection.paths.contains(&entry.path);
                        if normal_click && !is_active {
                            chosen = Some(LibraryAction::Open(entry.path.clone()));
                        }
                        if !ui.input(|i| i.modifiers.ctrl) {
                            response.context_menu(|ui| {
                                if let Some(a) = self.file_menu(ui, &entry.path, true) {
                                    chosen = Some(a);
                                }
                            });
                        }
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::SelectableLabel,
                                ui.is_enabled(),
                                selected,
                                &entry.name,
                            )
                        });
                        ui.painter().rect_filled(
                            rect,
                            5,
                            if selected {
                                appearance::CREAM
                            } else {
                                Color32::from_gray(239)
                            },
                        );
                        let image_rect = Rect::from_min_max(
                            rect.min + Vec2::splat(4.),
                            rect.max - Vec2::new(4., 20.),
                        );
                        if let Some(cache) = self.previews.get_mut(&entry.path) {
                            cache.used = self.frame;
                            match &cache.preview {
                                Preview::Ready { texture, .. } => {
                                    let size = texture.size_vec2();
                                    let factor = (image_rect.width() / size.x)
                                        .min(image_rect.height() / size.y);
                                    crate::editor::paint_checkerboard(ui.painter(), image_rect);
                                    ui.painter().image(
                                        texture.id(),
                                        Rect::from_center_size(image_rect.center(), size * factor),
                                        Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
                                        Color32::WHITE,
                                    );
                                }
                                Preview::Failed(_) => {
                                    ui.painter().text(
                                        image_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "Unavailable",
                                        FontId::proportional(10.),
                                        appearance::MUTED,
                                    );
                                }
                            }
                        } else {
                            if ui.is_enabled() {
                                self.request_preview(entry);
                            }
                            appearance::icon(
                                ui.painter(),
                                image_rect.center(),
                                appearance::Icon::View,
                                appearance::MUTED,
                                20.,
                            );
                        }
                        if selected {
                            ui.painter().rect_stroke(
                                rect,
                                5,
                                Stroke::new(1.5, appearance::ACCENT),
                                egui::StrokeKind::Inside,
                            );
                        }
                        let label = if is_active && dirty {
                            "Unsaved changes".into()
                        } else {
                            entry.name.chars().take(17).collect::<String>()
                        };
                        ui.painter().text(
                            rect.center_bottom() - Vec2::new(0., 9.),
                            egui::Align2::CENTER_CENTER,
                            label,
                            FontId::proportional(10.),
                            appearance::INK,
                        );
                        if is_active {
                            ui.painter().circle_filled(
                                rect.right_top() + Vec2::new(-7., 7.),
                                3.,
                                appearance::ACCENT,
                            );
                        }
                        response.on_hover_text(format!(
                            "{}\nCtrl/⌘-click to select · Right-click for actions",
                            entry.path.display()
                        ));
                    }
                });
            });
        if let Some(paths) = self.recent_selection.keyboard(ui, &order) {
            chosen = Some(LibraryAction::Trash(paths));
        }
        chosen
    }
    pub fn refresh(&mut self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.needs_scan = true;
        self.scanning = false;
        self.entries.clear();
        self.calendar.clear();
        self.filtered.clear();
        self.filter_dirty = true;
        self.pending.clear();
        self.checked_paths.clear();
        self.previews.clear();
        self.preview_bytes = 0;
        self.error = None;
        self.warnings.clear();
    }

    pub fn release_textures(&mut self) {
        self.refresh();
    }

    fn receive(&mut self, ctx: &egui::Context) {
        let Some(worker) = &self.worker else {
            return;
        };
        let mut replies = Vec::new();
        let disconnected = loop {
            if replies.len() >= MAX_REPLIES_PER_FRAME {
                ctx.request_repaint();
                break false;
            }
            match worker.replies.try_recv() {
                Ok(reply) => replies.push(reply),
                Err(mpsc::TryRecvError::Empty) => break false,
                Err(mpsc::TryRecvError::Disconnected) => break true,
            }
        };
        let generation = self.generation.load(Ordering::Relaxed);
        for reply in replies {
            match reply {
                Reply::Missing(version, paths) if version == generation => {
                    self.note_missing(&paths)
                }
                Reply::Scan(version, result) if version == generation => {
                    self.scanning = false;
                    match result {
                        Ok(index) => {
                            self.calendar = build_calendar(&index.entries);
                            self.entries = index.entries;
                            self.warnings = index.warnings;
                            self.filter_dirty = true;
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                Reply::Thumbnail(version, path, result)
                    if version == generation && !self.missing_paths.contains(&path) =>
                {
                    self.pending.remove(&path);
                    let (preview, bytes) = match result {
                        Ok(thumbnail) => {
                            let bytes = thumbnail.image.pixels.len() * 4;
                            let texture = ctx.load_texture(
                                format!("library-{generation}-{}", path.display()),
                                thumbnail.image,
                                egui::TextureOptions::LINEAR,
                            );
                            (
                                Preview::Ready {
                                    texture,
                                    original_size: thumbnail.original_size,
                                },
                                bytes,
                            )
                        }
                        Err(error) => (Preview::Failed(error), 0),
                    };
                    if let Some(old) = self.previews.insert(
                        path,
                        CachedPreview {
                            preview,
                            bytes,
                            used: self.frame,
                        },
                    ) {
                        self.preview_bytes -= old.bytes;
                    }
                    self.preview_bytes += bytes;
                }
                _ => {}
            }
        }
        self.trim_previews();
        if disconnected {
            self.worker = None;
            self.scanning = false;
            self.pending.clear();
            self.error = Some("The library worker stopped. Click Refresh to retry.".into());
        }
    }

    fn trim_previews(&mut self) {
        while self.previews.len() > MAX_TEXTURES || self.preview_bytes > MAX_TEXTURE_BYTES {
            let Some(path) = self
                .previews
                .iter()
                .min_by_key(|(_, preview)| preview.used)
                .map(|(path, _)| path.clone())
            else {
                break;
            };
            if let Some(preview) = self.previews.remove(&path) {
                self.preview_bytes -= preview.bytes;
            }
        }
    }

    fn request_scan(&mut self, ctx: &egui::Context, root: &Path) {
        if !self.needs_scan {
            return;
        }
        if self.worker.is_none() {
            match Worker::start(ctx.clone(), self.generation.clone()) {
                Ok(worker) => self.worker = Some(worker),
                Err(error) => {
                    self.needs_scan = false;
                    self.error = Some(error);
                    return;
                }
            }
        }
        let request = Request::Scan(self.generation.load(Ordering::Relaxed), root.to_owned());
        match self.worker.as_ref().unwrap().requests.try_send(request) {
            Ok(()) => {
                self.needs_scan = false;
                self.scanning = true;
            }
            Err(TrySendError::Full(_)) => {
                // A previous generation is draining; retry once it has made progress.
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            Err(TrySendError::Disconnected(_)) => {
                self.worker = None;
                self.needs_scan = false;
                self.error = Some("The library worker stopped. Click Refresh to retry.".into());
            }
        }
    }

    fn request_preview(&mut self, entry: &LibraryEntry) {
        if self.scanning
            || self.pending.len() >= MAX_PENDING
            || self.pending.contains(&entry.path)
            || self.previews.contains_key(&entry.path)
        {
            return;
        }
        let Some(worker) = &self.worker else {
            return;
        };
        let request = Request::Thumbnail(self.generation.load(Ordering::Relaxed), entry.clone());
        if worker.requests.try_send(request).is_ok() {
            self.pending.insert(entry.path.clone());
        }
    }

    fn update_filter(&mut self) {
        if !self.filter_dirty {
            return;
        }
        self.filtered = filter_entries(&self.entries, self.filter, &self.search);
        self.filtered.retain(|&i| {
            self.collection
                .includes(&self.entries[i], &self.preferences)
        });
        library::sort_entries(
            &self.entries,
            &mut self.filtered,
            self.preferences.sort,
            self.preferences.descending,
        );
        self.calendar = build_calendar(
            self.entries
                .iter()
                .filter(|entry| self.collection.includes(entry, &self.preferences)),
        );

        let order: Vec<_> = self
            .filtered
            .iter()
            .map(|&i| self.entries[i].path.clone())
            .collect();
        self.selection.retain(&order);
        self.filter_dirty = false;
        self.reset_scroll = true;
    }

    fn calendar_ui(&mut self, ui: &mut egui::Ui) {
        let mut filter = self.filter;
        date_button(
            ui,
            &mut filter,
            DateFilter::All,
            "All dates",
            self.calendar
                .values()
                .flat_map(|months| months.values())
                .flat_map(|days| days.values())
                .sum(),
        );
        ui.add_space(4.0);
        let newest = self.calendar.keys().next_back().copied();
        for (&year, months) in self.calendar.iter().rev() {
            let count = months.values().flat_map(|days| days.values()).sum();
            egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                ui.make_persistent_id(("library-year", year)),
                Some(year) == newest,
            )
            .show_header(ui, |ui| {
                date_button(
                    ui,
                    &mut filter,
                    DateFilter::Year(year),
                    &year.to_string(),
                    count,
                );
            })
            .body(|ui| {
                let newest_month = months.keys().next_back().copied();
                for (&month, days) in months.iter().rev() {
                    let title = NaiveDate::from_ymd_opt(year, month, 1)
                        .unwrap()
                        .format("%B")
                        .to_string();
                    egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        ui.make_persistent_id(("library-month", year, month)),
                        Some(year) == newest && Some(month) == newest_month,
                    )
                    .show_header(ui, |ui| {
                        date_button(
                            ui,
                            &mut filter,
                            DateFilter::Month(year, month),
                            &title,
                            days.values().sum(),
                        );
                    })
                    .body(|ui| {
                        for (&day, &count) in days.iter().rev() {
                            date_button(
                                ui,
                                &mut filter,
                                DateFilter::Day(day),
                                &day.format("%d · %a").to_string(),
                                count,
                            );
                        }
                    });
                }
            });
        }
        if filter != self.filter {
            self.navigate(Location {
                collection: self.collection,
                date: filter,
            });
        }
    }
}

fn build_calendar<'a>(entries: impl IntoIterator<Item = &'a LibraryEntry>) -> Calendar {
    let mut calendar = Calendar::new();
    for entry in entries {
        *calendar
            .entry(entry.date.year())
            .or_default()
            .entry(entry.date.month())
            .or_default()
            .entry(entry.date)
            .or_default() += 1;
    }
    calendar
}

fn filter_entries(entries: &[LibraryEntry], filter: DateFilter, search: &str) -> Vec<usize> {
    let search = search.trim().to_lowercase();
    entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            (filter.includes(entry.date)
                && (search.is_empty() || entry.name.to_lowercase().contains(&search)))
            .then_some(index)
        })
        .collect()
}

fn date_button(
    ui: &mut egui::Ui,
    filter: &mut DateFilter,
    value: DateFilter,
    name: &str,
    count: usize,
) {
    if browser::navigation_row(ui, name, count, *filter == value, None).clicked() {
        *filter = value;
    }
}

fn clipped_label(ui: &egui::Ui, position: Pos2, width: f32, text: &str, size: f32, color: Color32) {
    let mut job = egui::text::LayoutJob::simple_singleline(
        text.to_owned(),
        FontId::proportional(size),
        color,
    );
    job.wrap.max_width = width;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(position, galley, color);
}

fn empty_state(ui: &mut egui::Ui, title: &str, description: &str) {
    let rect = ui.available_rect_before_wrap();
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(Rect::from_center_size(
            rect.center(),
            Vec2::new(rect.width().min(440.0), 140.0),
        )),
        |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(16.0);
                ui.label(RichText::new(title).size(22.0).strong());
                ui.add_space(8.0);
                ui.label(RichText::new(description).color(appearance::MUTED));
            });
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, year: i32, month: u32, day: u32) -> LibraryEntry {
        LibraryEntry {
            path: PathBuf::from(name),
            name: name.into(),
            date: NaiveDate::from_ymd_opt(year, month, day).unwrap(),
            kind: EntryKind::Image,
            modified: None,
            created: None,
            bytes: 0,
        }
    }

    #[test]
    fn calendar_and_filters_keep_year_month_and_search_boundaries() {
        let entries = vec![
            entry("Capture.PNG", 2026, 10, 17),
            entry("notes.myimage", 2026, 10, 17),
            entry("capture.jpg", 2026, 9, 17),
            entry("capture.png", 2025, 10, 17),
        ];
        let calendar = build_calendar(&entries);
        assert_eq!(calendar[&2026][&10][&entries[0].date], 2);
        assert_eq!(
            filter_entries(&entries, DateFilter::Year(2026), ""),
            [0, 1, 2]
        );
        assert_eq!(
            filter_entries(&entries, DateFilter::Month(2026, 10), " CAPTURE "),
            [0]
        );
        assert_eq!(
            filter_entries(&entries, DateFilter::Day(entries[0].date), "notes"),
            [1]
        );
        assert!(filter_entries(&entries, DateFilter::Month(2025, 9), "").is_empty());
    }

    #[test]
    fn filtering_clears_hidden_selection_and_refresh_cancels_old_generation() {
        let mut panel = LibraryPanel::default();
        panel.entries = vec![
            entry("one.png", 2026, 10, 17),
            entry("two.png", 2025, 10, 17),
        ];
        panel.selection.paths.insert(PathBuf::from("two.png"));
        panel.filter = DateFilter::Year(2026);
        panel.update_filter();
        assert!(panel.selection.paths.is_empty());
        panel.pending.insert(PathBuf::from("one.png"));
        let previous = panel.generation.load(Ordering::Relaxed);
        panel.refresh();
        assert_ne!(panel.generation.load(Ordering::Relaxed), previous);
        assert!(panel.pending.is_empty());
        assert!(panel.entries.is_empty());
        assert!(panel.needs_scan);
    }

    #[test]
    fn old_thumbnail_reply_cannot_replace_current_generation() {
        let mut panel = LibraryPanel::default();
        let (requests, _request_receiver) = mpsc::sync_channel(MAX_PENDING + 1);
        let (replies, receiver) = mpsc::channel();
        panel.worker = Some(Worker {
            requests,
            replies: receiver,
        });
        panel.refresh();
        let current = panel.generation.load(Ordering::Relaxed);
        let path = PathBuf::from("same-path.png");
        panel.pending.insert(path.clone());
        replies
            .send(Reply::Thumbnail(
                current - 1,
                path.clone(),
                Err("old failure".into()),
            ))
            .unwrap();
        replies
            .send(Reply::Scan(current - 1, Err("old scan".into())))
            .unwrap();
        panel.receive(&egui::Context::default());
        assert!(panel.pending.contains(&path));
        assert!(panel.previews.is_empty());
        assert!(panel.error.is_none());
        replies
            .send(Reply::Thumbnail(
                current,
                path.clone(),
                Err("current failure".into()),
            ))
            .unwrap();
        panel.receive(&egui::Context::default());
        assert!(!panel.pending.contains(&path));
        assert!(
            matches!(&panel.previews[&path].preview, Preview::Failed(message) if message == "current failure")
        );
    }

    #[test]
    fn thumbnail_uploads_are_spread_across_frames() {
        let mut panel = LibraryPanel::default();
        let (requests, _receiver) = mpsc::sync_channel(MAX_PENDING + 1);
        let (sender, replies) = mpsc::channel();
        panel.worker = Some(Worker { requests, replies });
        for index in 0..5 {
            sender
                .send(Reply::Thumbnail(
                    0,
                    PathBuf::from(format!("{index}.png")),
                    Ok(PreparedThumbnail {
                        image: egui::ColorImage::filled([16, 16], Color32::RED),
                        original_size: [64, 64],
                    }),
                ))
                .unwrap();
        }
        let ctx = egui::Context::default();
        panel.receive(&ctx);
        assert_eq!(panel.previews.len(), 2);
        panel.receive(&ctx);
        assert_eq!(panel.previews.len(), 4);
        panel.receive(&ctx);
        assert_eq!(panel.previews.len(), 5);
    }

    #[test]
    fn thumbnail_work_and_cache_stay_bounded() {
        let mut panel = LibraryPanel::default();
        let (requests, receiver) = mpsc::sync_channel(MAX_PENDING + 1);
        let (_reply_sender, replies) = mpsc::channel();
        panel.worker = Some(Worker { requests, replies });
        for index in 0..1000 {
            panel.request_preview(&entry(&format!("{index}.png"), 2026, 10, 17));
        }
        assert_eq!(panel.pending.len(), MAX_PENDING);
        assert_eq!(receiver.try_iter().count(), MAX_PENDING);
        for index in 0..200 {
            panel.previews.insert(
                PathBuf::from(format!("{index}.png")),
                CachedPreview {
                    preview: Preview::Failed("unavailable".into()),
                    bytes: 0,
                    used: index,
                },
            );
        }
        panel.trim_previews();
        assert_eq!(panel.previews.len(), MAX_TEXTURES);
        assert!(!panel.previews.contains_key(Path::new("0.png")));
        assert!(panel.previews.contains_key(Path::new("199.png")));
    }

    #[test]
    fn populated_library_renders_calendar_and_gallery_at_small_and_large_sizes() {
        let ctx = egui::Context::default();
        let root = Path::new("library-fixture");
        let (requests, request_receiver) = mpsc::sync_channel(MAX_PENDING + 1);
        let (reply_sender, replies) = mpsc::channel();
        let mut panel = LibraryPanel::default();
        panel.root = Some(root.to_owned());
        panel.worker = Some(Worker { requests, replies });
        panel.needs_scan = false;
        panel.scanning = true;
        let mut project = entry("editable.myimage", 2026, 10, 17);
        project.kind = EntryKind::Project;
        reply_sender
            .send(Reply::Scan(
                0,
                Ok(LibraryIndex {
                    entries: vec![
                        project,
                        entry("recent.png", 2026, 10, 17),
                        entry("earlier.jpg", 2026, 9, 5),
                        entry("old.webp", 2025, 12, 31),
                    ],
                    warnings: Vec::new(),
                }),
            ))
            .unwrap();

        // These sizes represent the available panel area after the application's
        // navigation and document bar at its minimum and initial window sizes.
        for size in [Vec2::new(820.0, 500.0), Vec2::new(1040.0, 660.0)] {
            for filter in [
                DateFilter::All,
                DateFilter::Year(2026),
                DateFilter::Month(2026, 10),
                DateFilter::Day(NaiveDate::from_ymd_opt(2026, 10, 17).unwrap()),
            ] {
                for (view, tile_size) in [
                    (library::GalleryView::Grid, library::TileSize::Small),
                    (library::GalleryView::Grid, library::TileSize::Medium),
                    (library::GalleryView::Grid, library::TileSize::Large),
                    (library::GalleryView::List, library::TileSize::Medium),
                ] {
                    panel.preferences.view = view;
                    panel.preferences.tile_size = tile_size;
                    panel.filter = filter;
                    panel.filter_dirty = true;
                    let output = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                            ..Default::default()
                        },
                        |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| {
                                assert!(panel.ui(ui, root).is_none());
                                assert!(ui.min_rect().right() <= size.x);
                                assert!(
                                    ui.min_rect().bottom() <= size.y,
                                    "bounds {:?} in {:?}, {:?}, {:?}",
                                    ui.min_rect(),
                                    size,
                                    view,
                                    tile_size
                                );
                            });
                        },
                    );
                    assert!(!output.shapes.is_empty());
                    assert!(!panel.filtered.is_empty());
                }
            }
        }
        assert!(!panel.scanning);
        assert_eq!(panel.calendar.len(), 2);
        assert!(
            request_receiver
                .try_iter()
                .any(|request| matches!(request, Request::Thumbnail(_, _)))
        );
    }
}
