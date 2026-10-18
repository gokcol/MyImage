use super::*;
use crate::stamps::{self, StampSource};
use std::{path::PathBuf, sync::mpsc};

fn replace_artwork(
    document: &mut Document,
    history: &mut EditHistory,
    index: usize,
    source: StampSource,
    fit_artwork: bool,
) -> Result<(), String> {
    let annotation = document
        .annotations
        .get(index)
        .ok_or("This stamp is no longer available.")?;
    if annotation.locked() {
        return Err("Unlock the stamp before replacing its artwork.".into());
    }
    let before = document.edit_state();
    if fit_artwork {
        let raster = stamps::raster(&source)?;
        let bounds = Rect::from_two_pos(point(annotation.start), point(annotation.end));
        let scale =
            (bounds.width() / raster.width() as f32).min(bounds.height() / raster.height() as f32);
        let fitted = Rect::from_center_size(
            bounds.center(),
            Vec2::new(raster.width() as f32, raster.height() as f32) * scale,
        );
        document.annotations[index].start = Point {
            x: fitted.left(),
            y: fitted.top(),
        };
        document.annotations[index].end = Point {
            x: fitted.right(),
            y: fitted.bottom(),
        };
    }
    document.annotations[index].stamp = Some(source);
    if let Err(e) = crate::document::validate_annotations(&document.annotations) {
        document.annotations = before.annotations;
        return Err(e);
    }
    history.commit(before, document);
    Ok(())
}

/// Resize in displayed-image pixels, including a rotated/resized canvas.
pub(super) fn resize_stamp(
    document: &mut Document,
    history: &mut EditHistory,
    index: usize,
    size: Vec2,
) -> Result<(), String> {
    let annotation = document
        .annotations
        .get(index)
        .ok_or("This stamp is no longer available.")?;
    if annotation.kind != AnnotationKind::Stamp || annotation.locked() {
        return Err("Select an unlocked stamp to change its size.".into());
    }
    if !size.is_finite() || size.min_elem() < 1.0 {
        return Err("Stamp dimensions must be positive pixels.".into());
    }
    let bounds = mapped_bounds(
        Rect::from_two_pos(point(annotation.start), point(annotation.end)),
        document,
    );
    let resized = Rect::from_center_size(bounds.center(), size);
    let original = original_size(document);
    let a = document
        .geometry
        .inverse_point(from_pos(resized.min), original);
    let b = document
        .geometry
        .inverse_point(from_pos(resized.max), original);
    let before = document.edit_state();
    let bounds = Rect::from_two_pos(point(a), point(b));
    document.annotations[index].start = from_pos(bounds.min);
    document.annotations[index].end = from_pos(bounds.max);
    if let Err(error) = crate::document::validate_annotations(&document.annotations) {
        document.annotations = before.annotations;
        return Err(error);
    }
    history.commit(before, document);
    Ok(())
}

pub(super) struct StampState {
    path: Result<PathBuf, String>,
    library: stamps::Library,
    loaded: bool,
    category: String,
    search: String,
    pub(super) source: StampSource,
    pub(super) size: f32,
    opacity: f32,
    linked_size: bool,
    designer: Option<crate::stamp_designer::Designer>,
    new_category: String,
    adding_category: bool,
    import_target: Option<usize>,
    error: Option<String>,
    pending: Option<mpsc::Receiver<Result<Option<StampSource>, String>>>,
    drag_candidate: Option<(StampSource, Pos2)>,
}
impl Default for StampState {
    fn default() -> Self {
        Self {
            path: Err("Signature library folder is unavailable.".into()),
            library: Default::default(),
            loaded: false,
            category: "Symbols".into(),
            search: String::new(),
            source: StampSource::Builtin { id: "2705".into() },
            size: 96.,
            opacity: 100.,
            linked_size: true,
            designer: None,
            new_category: String::new(),
            adding_category: false,
            import_target: None,
            error: None,
            pending: None,
            drag_candidate: None,
        }
    }
}
impl StampState {
    fn custom_destination(&self) -> String {
        if self.category == "Signatures"
            || self
                .library
                .categories
                .iter()
                .any(|c| c.name == self.category)
        {
            self.category.clone()
        } else {
            "My stamps".into()
        }
    }
    fn save_source(
        &mut self,
        category: &str,
        source: StampSource,
        old: Option<&StampSource>,
    ) -> Result<stamps::Library, String> {
        let path = self.path.as_ref().map_err(Clone::clone)?;
        if category != "Signatures" && !self.library.categories.iter().any(|c| c.name == category) {
            self.library = stamps::create_category(path, category)?;
        }
        stamps::put_in(path, category, old, source)
    }
    pub(super) fn is_importing(&self) -> bool {
        self.pending.is_some() || self.designer.is_some()
    }
    pub(super) fn new(path: Result<PathBuf, String>) -> Self {
        Self {
            path,
            ..Default::default()
        }
    }
}
impl EditorState {
    pub(super) fn stamp_designer_window(
        &mut self,
        ctx: &egui::Context,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let Some(mut draft) = self.stamps.designer.take() else {
            return;
        };
        let mut categories: Vec<String> = self
            .stamps
            .library
            .categories
            .iter()
            .map(|c| c.name.clone())
            .collect();
        categories.push("Signatures".into());
        if !categories.contains(&draft.category) {
            categories.push(draft.category.clone());
        }
        match draft.show(ctx, &categories) {
            Some(crate::stamp_designer::Action::Cancel) => {}
            Some(crate::stamp_designer::Action::Save(source)) => {
                if let Some(index) = draft.target {
                    let mut candidate = document.clone();
                    let mut check_history = EditHistory::new(candidate.edit_state());
                    if let Err(error) = replace_artwork(
                        &mut candidate,
                        &mut check_history,
                        index,
                        source.clone(),
                        false,
                    ) {
                        draft.set_error(error);
                        self.stamps.designer = Some(draft);
                        return;
                    }
                }
                let saved = self.stamps.save_source(
                    &draft.category,
                    source.clone(),
                    draft.original.as_ref(),
                );
                match saved {
                    Ok(lib) => {
                        self.stamps.library = lib;
                        if let Some(index) = draft.target
                            && document.annotations.get(index).is_some_and(|a| !a.locked())
                        {
                            if let Err(error) =
                                replace_artwork(document, history, index, source.clone(), false)
                            {
                                draft.set_error(error);
                                self.stamps.designer = Some(draft);
                                return;
                            }
                        } else {
                            self.clear_selection();
                            self.tool = Tool::Stamp;
                        }
                        self.stamps.source = source;
                        self.stamps.category = draft.category;
                        self.stamps.search.clear();
                        self.stamps.error = None;
                    }
                    Err(e) => {
                        draft.set_error(e);
                        self.stamps.designer = Some(draft);
                    }
                }
            }
            None => self.stamps.designer = Some(draft),
        }
    }
    pub(super) fn sync_stamp_selection(&mut self, annotation: &Annotation) {
        if let Some(source) = &annotation.stamp {
            self.stamps.source = source.clone();
            self.stamps.category = match source {
                StampSource::Builtin { id } => stamps::BUILTINS
                    .iter()
                    .find(|s| s.id == id)
                    .map_or("Symbols", |s| s.category),
                StampSource::Image { .. }
                | StampSource::Raster { .. }
                | StampSource::Svg { .. } => self
                    .stamps
                    .library
                    .categories
                    .iter()
                    .find(|c| c.stamps.contains(source))
                    .map_or("Signatures", |c| c.name.as_str()),
            }
            .into();
            self.stamps.search.clear();
        }
    }
    pub(super) fn stamp_panel(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if !self.stamps.loaded {
            match self
                .stamps
                .path
                .as_ref()
                .map_err(Clone::clone)
                .and_then(|p| stamps::load(p))
            {
                Ok(library) => self.stamps.library = library,
                Err(error) => self.stamps.error = Some(error),
            }
            self.stamps.loaded = true;
        }
        if let Some(receiver) = &self.stamps.pending {
            match receiver.try_recv() {
                Ok(result) => {
                    self.stamps.pending = None;
                    match result {
                        Ok(Some(source)) => {
                            if let Some(index) = self.stamps.import_target.take() {
                                if let Err(error) =
                                    replace_artwork(document, history, index, source.clone(), true)
                                {
                                    self.stamps.error = Some(error);
                                } else {
                                    self.stamps.error = None;
                                }
                                self.select(Some(index), document);
                            } else {
                                let category = self.stamps.custom_destination();
                                let saved =
                                    self.stamps.save_source(&category, source.clone(), None);
                                match saved {
                                    Ok(library) => {
                                        self.stamps.library = library;
                                        self.stamps.source = source;
                                        self.stamps.category = category;
                                        self.stamps.error = None;
                                    }
                                    Err(error) => {
                                        self.stamps.import_target = None;
                                        self.stamps.error = Some(error);
                                    }
                                }
                            }
                        }
                        Ok(None) => {
                            self.stamps.import_target = None;
                        }
                        Err(error) => {
                            self.stamps.import_target = None;
                            self.stamps.error = Some(error);
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.stamps.pending = None;
                    self.stamps.import_target = None;
                    self.stamps.error = Some("Signature import stopped. Please try again.".into());
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(100));
                }
            }
        }
        ui.horizontal(|ui| {
            ui.strong("Stamps");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("+ Add", |ui| {
                    if ui
                        .add_enabled(
                            self.stamps.pending.is_none(),
                            egui::Button::new("Import image / SVG…"),
                        )
                        .clicked()
                    {
                        self.import_stamp(ui.ctx(), None);
                        ui.close();
                    }
                    if ui.button("Design SVG…").clicked() {
                        self.stamps.designer = Some(crate::stamp_designer::Designer::new(
                            None,
                            None,
                            self.stamps.custom_destination(),
                        ));
                        ui.close();
                    }
                    if ui.button("New category…").clicked() {
                        self.stamps.adding_category = true;
                        ui.close();
                    }
                    if self.stamps.category == "Signatures"
                        && !matches!(self.stamps.source, StampSource::Builtin { .. })
                        && !self.stamps.library.signatures.contains(&self.stamps.source)
                        && ui.button("Save this signature to collection").clicked()
                    {
                        match self
                            .stamps
                            .path
                            .as_ref()
                            .map_err(Clone::clone)
                            .and_then(|p| stamps::add(p, self.stamps.source.clone()))
                        {
                            Ok(library) => self.stamps.library = library,
                            Err(error) => self.stamps.error = Some(error),
                        }
                        ui.close();
                    }
                    ui.separator();
                    ui.small("Drag to place · Double-click to insert");
                    ui.hyperlink_to(
                        "Artwork: Twemoji · CC BY 4.0",
                        "https://github.com/jdecked/twemoji",
                    );
                });
            });
        });
        egui::ComboBox::from_id_salt("stamp-category")
            .width(ui.available_width())
            .selected_text(&self.stamps.category)
            .show_ui(ui, |ui| {
                let categories: Vec<String> = stamps::CATEGORIES
                    .iter()
                    .map(|s| (*s).to_string())
                    .chain(
                        self.stamps
                            .library
                            .categories
                            .iter()
                            .map(|c| c.name.clone()),
                    )
                    .collect();
                for category in categories {
                    if ui
                        .selectable_value(&mut self.stamps.category, category.clone(), &category)
                        .clicked()
                    {
                        self.stamps.search.clear();
                    }
                }
            });
        ui.add(
            egui::TextEdit::singleline(&mut self.stamps.search)
                .hint_text("Search stamps…")
                .desired_width(f32::INFINITY),
        );
        if self.stamps.adding_category {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.stamps.new_category)
                        .hint_text("Category name")
                        .char_limit(40)
                        .desired_width(155.),
                );
                if ui.button("Add").clicked() {
                    let name = self.stamps.new_category.trim().to_string();
                    match self
                        .stamps
                        .path
                        .as_ref()
                        .map_err(Clone::clone)
                        .and_then(|p| stamps::create_category(p, &name))
                    {
                        Ok(lib) => {
                            self.stamps.library = lib;
                            self.stamps.category = name;
                            self.stamps.new_category.clear();
                            self.stamps.adding_category = false;
                            self.stamps.error = None;
                        }
                        Err(e) => self.stamps.error = Some(e),
                    }
                }
            });
        }
        let search = self.stamps.search.to_lowercase();
        let sources: Vec<StampSource> = if self.stamps.category == "Signatures" {
            let mut entries = self.stamps.library.signatures.clone();
            if matches!(
                self.stamps.source,
                StampSource::Image { .. } | StampSource::Svg { .. }
            ) && !entries.contains(&self.stamps.source)
            {
                entries.push(self.stamps.source.clone());
            }
            entries
        } else if let Some(category) = self
            .stamps
            .library
            .categories
            .iter()
            .find(|c| c.name == self.stamps.category)
        {
            category.stamps.clone()
        } else {
            stamps::BUILTINS
                .iter()
                .filter(|s| s.category == self.stamps.category)
                .map(|s| StampSource::Builtin { id: s.id.into() })
                .collect()
        }
        .into_iter()
        .filter(|s| s.name().to_lowercase().contains(&search))
        .collect();
        // Keep the initial tile hit until movement starts a drag. The press and
        // release may be coalesced together or arrive in two adjacent frames.
        let press = ui.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    ..
                } => Some(*pos),
                _ => None,
            })
        });
        if press.is_some() || !ui.is_enabled() {
            self.stamps.drag_candidate = None;
        }
        // Only the gallery scrolls. Reserve a stable, always-visible property shelf.
        let gallery_height = (ui.available_height() - 192.).max(56.);
        egui::ScrollArea::vertical()
            .id_salt(("stamp-gallery-scroll", &self.stamps.category))
            .max_height(gallery_height)
            .min_scrolled_height(gallery_height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if sources.is_empty() {
                    ui.small("No stamps here yet. Use + Add to import or design one.");
                }
                let cell_width = (ui.available_width() - 16.) / 5.;
                egui::Grid::new("stamp-gallery")
                    .num_columns(5)
                    .spacing([4., 4.])
                    .show(ui, |ui| {
                        for (i, source) in sources.iter().enumerate() {
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(cell_width, cell_width + 10.),
                                Sense::click_and_drag(),
                            );
                            let selected = *source == self.stamps.source;
                            response.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::SelectableLabel,
                                    ui.is_enabled(),
                                    selected,
                                    source.name(),
                                )
                            });
                            let fill = if selected {
                                Color32::from_rgb(222, 245, 244)
                            } else if response.hovered() {
                                Color32::from_rgb(243, 246, 250)
                            } else {
                                Color32::TRANSPARENT
                            };
                            ui.painter().rect_filled(rect, 7., fill);
                            if selected {
                                ui.painter().rect_stroke(
                                    rect,
                                    7.,
                                    Stroke::new(1., Color32::from_rgb(0, 137, 146)),
                                    StrokeKind::Inside,
                                );
                            }
                            if ui.is_rect_visible(rect)
                                && let Ok(texture) = stamps::texture(ui.ctx(), source)
                            {
                                let bounds = Rect::from_min_size(
                                    rect.min + Vec2::splat(7.),
                                    egui::vec2(cell_width - 14., cell_width - 14.),
                                );
                                let size = texture.size_vec2();
                                let scale = (bounds.width() / size.x).min(bounds.height() / size.y);
                                ui.painter().image(
                                    texture.id(),
                                    Rect::from_center_size(bounds.center(), size * scale),
                                    Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
                                    Color32::WHITE,
                                );
                            }
                            let label: String = source.name().chars().take(9).collect();
                            ui.painter().text(
                                egui::pos2(rect.center().x, rect.bottom() - 10.),
                                egui::Align2::CENTER_CENTER,
                                label,
                                egui::FontId::proportional(9.),
                                Color32::from_rgb(73, 85, 101),
                            );
                            response.dnd_set_drag_payload(source.clone());
                            if ui.is_enabled()
                                && let Some(start) = press
                                && rect.intersect(ui.clip_rect()).contains(start)
                                && ui.ctx().layer_id_at(start) == Some(ui.layer_id())
                            {
                                self.stamps.drag_candidate = Some((source.clone(), start));
                            }
                            let response = response.on_hover_text(format!("{}\nDrag onto the image · Double-click to insert · Click to choose", source.name()));
                            if response.double_clicked() {
                                self.insert_stamp_at_center(source.clone(), document, history);
                            } else if response.clicked() {
                                self.stamps.source = source.clone();
                                self.tool = Tool::Stamp;
                                self.clear_selection();
                                self.area = None;
                                self.area_mask.clear();
                            }
                            if i % 5 == 4 {
                                ui.end_row();
                            }
                        }
                    });
            });
        if let Some((source, start)) = self.stamps.drag_candidate.take() {
            let cancelled = !ui.is_enabled()
                || ui.input(|i| {
                    i.key_pressed(Key::Escape)
                        || !(i.pointer.primary_down()
                            || i.pointer.button_released(PointerButton::Primary))
                });
            if !cancelled && !egui::DragAndDrop::has_payload_of_type::<StampSource>(ui.ctx()) {
                let end = ui.input(|i| i.pointer.interact_pos());
                if let Some(end) = end
                    && start.distance(end) > ui.ctx().options(|o| o.input_options.max_click_dist)
                {
                    egui::DragAndDrop::set_payload(ui.ctx(), source);
                } else if ui.input(|i| i.pointer.primary_down()) {
                    self.stamps.drag_candidate = Some((source, start));
                }
            }
        }
        ui.separator();
        self.stamp_properties(ui, document, history);
    }

    fn import_stamp(&mut self, ctx: &egui::Context, target: Option<usize>) {
        self.stamps.import_target = target;
        let (send, receive) = mpsc::channel();
        self.stamps.pending = Some(receive);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = rfd::FileDialog::new()
                .set_title("Stamp artwork")
                .add_filter(
                    "Images / SVG",
                    &[
                        "svg", "png", "jpg", "jpeg", "webp", "tif", "tiff", "bmp", "gif",
                    ],
                )
                .pick_file()
                .map(|p| stamps::import(&p))
                .transpose();
            let _ = send.send(result);
            ctx.request_repaint();
        });
    }

    fn stamp_properties(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let indices = self.selected_indices(document);
        let selected = self.selected.filter(|&i| {
            indices.len() == 1 && document.annotations[i].kind == AnnotationKind::Stamp
        });
        let source = selected
            .and_then(|i| document.annotations[i].stamp.clone())
            .unwrap_or_else(|| self.stamps.source.clone());
        ui.horizontal(|ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(if indices.len() > 1 {
                        format!("{} annotations selected", indices.len())
                    } else {
                        source.name().to_string()
                    })
                    .strong(),
                )
                .truncate(),
            );
        });
        if indices.len() > 1 {
            ui.small("Select one stamp to edit its size and opacity.");
        } else {
            ui.add_enabled_ui(
                selected.is_none() || self.can_edit_selection(document),
                |ui| {
                    if let Some(index) = selected {
                        let a = &document.annotations[index];
                        let mut size = mapped_bounds(
                            Rect::from_two_pos(point(a.start), point(a.end)),
                            document,
                        )
                        .size();
                        let old_size = size;
                        let mut changed = false;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.;
                            ui.spacing_mut().text_edit_width = 53.;
                            ui.label("W");
                            let width = ui
                                .add(
                                    egui::DragValue::new(&mut size.x)
                                        .range(1.0..=32768.)
                                        .max_decimals(1),
                                )
                                .on_hover_text("Width in image pixels");
                            if width.changed() && self.stamps.linked_size {
                                size.y = old_size.y * size.x / old_size.x.max(0.001);
                            }
                            ui.label("H");
                            let height = ui
                                .add(
                                    egui::DragValue::new(&mut size.y)
                                        .range(1.0..=32768.)
                                        .max_decimals(1),
                                )
                                .on_hover_text("Height in image pixels");
                            if height.changed() && self.stamps.linked_size {
                                size.x = old_size.x * size.y / old_size.y.max(0.001);
                            }
                            changed = width.changed() || height.changed();
                            if ui
                                .selectable_label(self.stamps.linked_size, "Link")
                                .on_hover_text("Keep width and height proportional")
                                .clicked()
                            {
                                self.stamps.linked_size = !self.stamps.linked_size;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Scale");
                            for (label, factor) in [("−", 0.8), ("+", 1.25)] {
                                if ui
                                    .button(label)
                                    .on_hover_text(if factor < 1. {
                                        "20% smaller"
                                    } else {
                                        "25% larger"
                                    })
                                    .clicked()
                                {
                                    size = old_size * factor;
                                    changed = true;
                                }
                            }
                            ui.small("Drag corners to resize");
                        });
                        if changed && let Err(error) = resize_stamp(document, history, index, size)
                        {
                            self.stamps.error = Some(error);
                        }
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("Size");
                            ui.spacing_mut().slider_width = 110.;
                            crate::appearance::value_slider(
                                ui,
                                &mut self.stamps.size,
                                16.0..=1024.0,
                                "Stamp size",
                                " px",
                            );
                        });
                    }
                    let mut opacity = selected.map_or(self.stamps.opacity, |i| {
                        document.annotations[i].color[3] as f32 / 255. * 100.
                    });
                    ui.horizontal(|ui| {
                        ui.label("Opacity");
                        if crate::appearance::value_slider(
                            ui,
                            &mut opacity,
                            0.0..=100.0,
                            "Stamp opacity",
                            "%",
                        )
                        .changed()
                        {
                            if let Some(index) = selected {
                                let before = document.edit_state();
                                document.annotations[index].color[3] =
                                    (opacity * 2.55).round() as u8;
                                self.record_properties(before, document, history);
                                self.color = document.annotations[index].color;
                            } else {
                                self.stamps.opacity = opacity;
                            }
                        }
                    });
                },
            );
        }
        ui.horizontal(|ui| {
            ui.add_enabled_ui(
                selected.is_none() || self.can_edit_selection(document),
                |ui| {
                    if let Some(index) = selected
                        && ui
                            .add_enabled(
                                self.stamps.pending.is_none(),
                                egui::Button::new("Replace…"),
                            )
                            .clicked()
                    {
                        self.import_stamp(ui.ctx(), Some(index));
                    }
                    if indices.len() <= 1
                        && matches!(source, StampSource::Svg { .. })
                        && ui.button("Edit SVG…").clicked()
                    {
                        self.stamps.designer = Some(crate::stamp_designer::Designer::new(
                            Some(source.clone()),
                            selected,
                            self.stamps.custom_destination(),
                        ));
                    }
                },
            );
            if !indices.is_empty() {
                ui.menu_button("More", |ui| {
                    self.privacy_actions(ui, document, history);
                    self.selection_controls(ui, document, history);
                });
            }
        });
        ui.horizontal(|ui| {
            if self.stamps.pending.is_some() {
                ui.small("Importing…");
            } else if let Some(error) = &self.stamps.error {
                ui.colored_label(Color32::DARK_RED, "Stamp needs attention")
                    .on_hover_text(error);
            } else if selected.is_some() && !self.can_edit_selection(document) {
                ui.small("Locked · Unlock in Layers to edit");
            } else {
                ui.small("Drag to image · Double-click to insert");
            }
        });
    }
    fn insert_stamp_at_center(
        &mut self,
        source: StampSource,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let crop = document.geometry.crop_rect(original_size(document));
        self.stamps.source = source;
        self.place_stamp(
            Point {
                x: crop.x as f32 + crop.width as f32 * 0.5,
                y: crop.y as f32 + crop.height as f32 * 0.5,
            },
            document,
            history,
        );
    }

    pub(super) fn place_stamp(
        &mut self,
        point: Point,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let source = self.stamps.source.clone();
        let image = match stamps::raster(&source) {
            Ok(image) => image,
            Err(error) => {
                self.image_error = Some(error);
                return;
            }
        };
        let mut annotation = self.new_annotation(AnnotationKind::Stamp, point);
        let scale = self.stamps.size / image.width().max(image.height()) as f32;
        let size = egui::vec2(image.width() as f32, image.height() as f32) * scale;
        let bounds = Rect::from_center_size(super::point(point), size);
        annotation.start = from_pos(bounds.min);
        annotation.end = from_pos(bounds.max);
        annotation.color = [255, 255, 255, (self.stamps.opacity * 2.55).round() as u8];
        annotation.text.clear();
        annotation.layer.name = source.name().into();
        annotation.stamp = Some(source);
        let before = document.edit_state();
        document.annotations.push(annotation);
        if let Err(error) = crate::document::validate_annotations(&document.annotations) {
            document.annotations.pop();
            self.image_error = Some(error);
            return;
        }
        history.commit(before, document);
        self.tool = Tool::Area;
        self.select(Some(document.annotations.len() - 1), document);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn displayed_stamp_dimensions_preserve_center_artwork_and_undo_after_rotation() {
        let mut document = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(600, 400)),
            annotations: vec![],
            geometry: ImageGeometry::default(),
        };
        let mut history = EditHistory::new(document.edit_state());
        let mut editor = EditorState::default();
        editor.stamps.opacity = 45.;
        editor.place_stamp(Point { x: 250., y: 200. }, &mut document, &mut history);
        assert_eq!(document.annotations[0].color[3], 115);
        document.geometry.quarter_turns = 1;
        document.geometry.resize = Some([200, 300]);
        let before = document.edit_state();
        let bounds = |d: &Document| {
            mapped_bounds(
                Rect::from_two_pos(point(d.annotations[0].start), point(d.annotations[0].end)),
                d,
            )
        };
        let center = bounds(&document).center();
        resize_stamp(&mut document, &mut history, 0, Vec2::new(120., 80.)).unwrap();
        assert_eq!(bounds(&document).size(), Vec2::new(120., 80.));
        assert_eq!(bounds(&document).center(), center);
        assert_eq!(document.annotations[0].stamp, before.annotations[0].stamp);
        assert!(history.undo(&mut document));
        assert_eq!(document.edit_state(), before);
        document.annotations[0].layer.locked = true;
        let locked = document.edit_state();
        assert!(resize_stamp(&mut document, &mut history, 0, Vec2::splat(20.)).is_err());
        assert_eq!(document.edit_state(), locked);
    }

    #[test]
    fn compact_gallery_keeps_selected_properties_visible_at_minimum_workspace_height() {
        let ctx = egui::Context::default();
        crate::appearance::apply(&ctx);
        let mut editor = EditorState::default();
        editor.stamps.loaded = true;
        let mut document = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(600, 400)),
            annotations: vec![],
            geometry: ImageGeometry::default(),
        };
        let mut history = EditHistory::new(document.edit_state());
        editor.place_stamp(Point { x: 100., y: 100. }, &mut document, &mut history);
        let mut render = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(280., 380.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(6., 5.);
                        ui.spacing_mut().button_padding = Vec2::new(7., 4.);
                        ui.spacing_mut().interact_size.y = 24.;
                        editor.stamp_panel(ui, &mut document, &mut history);
                    });
                },
            )
        };
        render(vec![]);
        let before = render(vec![]);
        let text_rect = |output: &egui::FullOutput, label: &str| {
            output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.job.text == label => {
                        let rect = Rect::from_min_size(t.pos, t.galley.size());
                        assert!(
                            s.clip_rect.contains_rect(rect),
                            "{label} clipped at {rect:?}"
                        );
                        Some(rect)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("Missing {label}"))
        };
        let opacity = text_rect(&before, "Opacity");
        for label in ["W", "H", "Replace…", "More"] {
            text_rect(&before, label);
        }
        let after = render(vec![
            egui::Event::PointerMoved(Pos2::new(100., 140.)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0., -300.),
                modifiers: Modifiers::NONE,
            },
        ]);
        assert_eq!(text_rect(&after, "Opacity"), opacity);
    }

    #[test]
    fn gallery_pointer_drag_reaches_canvas_and_double_click_inserts_once() {
        for source in [
            StampSource::Builtin { id: "26a0".into() },
            StampSource::Svg {
                name: "Warning".into(),
                svg: std::sync::Arc::new(crate::svg_stamp::STARTER.into()),
                prompt: "Test".into(),
            },
        ] {
            for timing in 0..3 {
                let ctx = egui::Context::default();
                crate::appearance::apply(&ctx);
                let texture = ctx.load_texture(
                    "test",
                    egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
                    Default::default(),
                );
                let mut editor = EditorState {
                    tool: Tool::Stamp,
                    ..Default::default()
                };
                editor.stamps.loaded = true;
                editor.stamps.category = "Test".into();
                editor
                    .stamps
                    .library
                    .categories
                    .push(stamps::CustomCategory {
                        name: "Test".into(),
                        stamps: vec![source.clone()],
                    });
                let mut document = Document {
                    image: std::sync::Arc::new(image::RgbaImage::new(600, 400)),
                    annotations: vec![],
                    geometry: ImageGeometry::default(),
                };
                let mut history = EditHistory::new(document.edit_state());
                let mut time = 0.;
                let mut frame = |editor: &mut EditorState,
                                 document: &mut Document,
                                 history: &mut EditHistory,
                                 events| {
                    time += 0.03;
                    ctx.run(
                        egui::RawInput {
                            time: Some(time),
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(1000., 800.),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| {
                                editor.workspace_ui(
                                    ui,
                                    document,
                                    history,
                                    &texture,
                                    &mut Default::default(),
                                );
                            });
                        },
                    )
                };
                frame(&mut editor, &mut document, &mut history, vec![]);
                let output = frame(&mut editor, &mut document, &mut history, vec![]);
                let start = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == "Warning" => {
                            Some(text.pos + Vec2::new(text.galley.size().x * 0.5, -25.))
                        }
                        _ => None,
                    })
                    .unwrap();
                let end = Pos2::new(400., 350.);
                let button = |pos, pressed| egui::Event::PointerButton {
                    pos,
                    pressed,
                    button: PointerButton::Primary,
                    modifiers: Modifiers::NONE,
                };
                if timing == 0 {
                    frame(
                        &mut editor,
                        &mut document,
                        &mut history,
                        vec![
                            egui::Event::PointerMoved(start),
                            button(start, true),
                            egui::Event::PointerMoved(end),
                            button(end, false),
                        ],
                    );
                } else {
                    frame(
                        &mut editor,
                        &mut document,
                        &mut history,
                        vec![egui::Event::PointerMoved(start), button(start, true)],
                    );
                    if timing == 1 {
                        frame(
                            &mut editor,
                            &mut document,
                            &mut history,
                            vec![egui::Event::PointerMoved(end), button(end, false)],
                        );
                    } else {
                        frame(
                            &mut editor,
                            &mut document,
                            &mut history,
                            vec![egui::Event::PointerMoved(start + Vec2::new(-20., 0.))],
                        );
                        assert!(
                            egui::DragAndDrop::has_payload_of_type::<StampSource>(&ctx),
                            "gallery did not begin a drag"
                        );
                        frame(
                            &mut editor,
                            &mut document,
                            &mut history,
                            vec![egui::Event::PointerMoved(end)],
                        );
                        frame(
                            &mut editor,
                            &mut document,
                            &mut history,
                            vec![button(end, false)],
                        );
                    }
                }
                assert_eq!(document.annotations.len(), 1, "timing={timing}");
                assert_eq!(document.annotations[0].stamp, Some(source.clone()));
                assert!(history.undo(&mut document));
                assert!(!history.can_undo());
                editor.tool = Tool::Stamp;
                frame(&mut editor, &mut document, &mut history, vec![]);
                for _ in 0..2 {
                    frame(
                        &mut editor,
                        &mut document,
                        &mut history,
                        vec![egui::Event::PointerMoved(start), button(start, true)],
                    );
                    frame(
                        &mut editor,
                        &mut document,
                        &mut history,
                        vec![button(start, false)],
                    );
                }
                assert_eq!(
                    document.annotations.len(),
                    1,
                    "double click inserted more or fewer than one stamp"
                );
            }
        }
    }

    #[test]
    fn double_click_insertion_uses_cropped_image_center_and_preserves_svg_for_undo() {
        let mut editor = EditorState::default();
        let mut document = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(800, 600)),
            annotations: vec![],
            geometry: ImageGeometry {
                viewport: None,
                flip_horizontal: false,
                flip_vertical: false,
                canvas: None,
                cutouts: [None; 16],
                crop: Some(CropRect {
                    x: 100,
                    y: 200,
                    width: 300,
                    height: 200,
                }),
                quarter_turns: 1,
                resize: Some([100, 150]),
            },
        };
        let before = document.edit_state();
        let mut history = EditHistory::new(before.clone());
        let source = StampSource::Svg {
            name: "Approved".into(),
            svg: std::sync::Arc::new(crate::svg_stamp::STARTER.into()),
            prompt: "Approval".into(),
        };
        editor.insert_stamp_at_center(source.clone(), &mut document, &mut history);
        let stamp = &document.annotations[0];
        assert_eq!(stamp.stamp, Some(source));
        assert_eq!((stamp.start.x + stamp.end.x) * 0.5, 250.);
        assert_eq!((stamp.start.y + stamp.end.y) * 0.5, 300.);
        assert_eq!(editor.selected, Some(0));
        assert!(history.undo(&mut document));
        assert_eq!(document.edit_state(), before);
        assert!(!history.can_undo());
        assert!(history.redo(&mut document));
        assert_eq!(document.annotations.len(), 1);
    }

    #[test]
    fn replacing_template_artwork_preserves_aspect_ratio_and_undo() {
        let mut d = crate::templates::build(&crate::templates::Spec {
            kind: crate::templates::Kind::BeforeAfter,
            title: "Compare".into(),
            size: [1600, 1000],
            background: [255; 4],
        })
        .unwrap();
        let index = d
            .annotations
            .iter()
            .position(|a| a.kind == AnnotationKind::Stamp)
            .unwrap();
        let original = d.edit_state();
        let mut h = EditHistory::new(original.clone());
        let source = StampSource::Svg {
            name: "Approval".into(),
            svg: std::sync::Arc::new(crate::svg_stamp::STARTER.into()),
            prompt: "Teal approval seal".into(),
        };
        replace_artwork(&mut d, &mut h, index, source.clone(), true).unwrap();
        let bounds = Rect::from_two_pos(
            point(d.annotations[index].start),
            point(d.annotations[index].end),
        );
        assert_eq!(bounds.width(), bounds.height());
        assert_eq!(d.annotations[index].stamp, Some(source.clone()));
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), original);
        d.annotations[index].layer.locked = true;
        assert!(replace_artwork(&mut d, &mut h, index, source, false).is_err());
    }

    #[test]
    fn stamps_place_resize_copy_group_and_undo_without_losing_artwork() {
        let mut e = EditorState::default();
        let mut d = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(400, 300)),
            annotations: vec![],
            geometry: Default::default(),
        };
        let mut h = EditHistory::new(d.edit_state());
        e.place_stamp(Point { x: 100., y: 100. }, &mut d, &mut h);
        assert_eq!(d.annotations.len(), 1);
        assert!(has_extent(&d.annotations[0]));
        let source = d.annotations[0].stamp.clone();
        let snapshot = annotation_clipboard::selection_snapshot(&d, &[0]).unwrap();
        let inserted = annotation_clipboard::prepare_insertion(snapshot, &d).unwrap();
        assert_eq!(inserted[0].stamp, source);
        e.place_stamp(Point { x: 240., y: 100. }, &mut d, &mut h);
        e.set_selection(vec![0, 1], &d);
        e.group_selection(&mut d, &mut h);
        assert_eq!(d.annotations[0].group_id, d.annotations[1].group_id);
        h.undo(&mut d);
        assert!(d.annotations[0].group_id.is_none());
        h.undo(&mut d);
        assert_eq!(d.annotations.len(), 1);
        h.undo(&mut d);
        assert!(d.annotations.is_empty());
    }
}
