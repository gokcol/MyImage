//! Compact inspection tools. Sampling and measuring do not edit the document;
//! explicit Apply/Add actions use the ordinary annotation history.
use super::*;
use std::sync::{Arc, mpsc};

#[derive(Default)]
pub(super) struct State {
    brush: Option<Brush>,
    pub context_source: Option<usize>,
    measurement: Option<[Point; 2]>,
    pub measure_dragging: bool,
    axis: Axis,
    sample: Option<Sample>,
    sample_job: Option<SampleJob>,
    message: Option<String>,
    preview: Option<ColorPreview>,
    preview_job: Option<PreviewJob>,
    preview_failed: bool,
    screen: crate::screen_color::Picker,
    screen_sample: bool,
}

struct ColorPreview {
    source: Arc<image::RgbaImage>,
    before: EditState,
    pixels: image::RgbaImage,
}
struct PreviewJob {
    source: Arc<image::RgbaImage>,
    before: EditState,
    receiver: mpsc::Receiver<Result<image::RgbaImage, String>>,
}

struct Brush {
    kind: AnnotationKind,
    appearance: tool_appearance::Appearance,
}

#[derive(Clone, Copy, Default, PartialEq)]
enum Axis {
    #[default]
    Free,
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Sample {
    pixel: [u32; 2],
    rgba: [u8; 4],
}

struct SampleJob {
    receiver: mpsc::Receiver<Result<Sample, String>>,
    image: Arc<image::RgbaImage>,
    before: EditState,
}

impl Sample {
    fn hex(self) -> String {
        let [r, g, b, a] = self.rgba;
        if a == 255 {
            format!("#{r:02X}{g:02X}{b:02X}")
        } else {
            format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
        }
    }
    fn rgb(self) -> String {
        let [r, g, b, a] = self.rgba;
        if a == 255 {
            format!("rgb({r}, {g}, {b})")
        } else {
            format!("rgba({r}, {g}, {b}, {:.3})", f32::from(a) / 255.)
        }
    }
}

fn compatible(source: AnnotationKind, target: AnnotationKind) -> bool {
    !source.is_area_edit() && (source == target || source.is_shape() && target.is_shape())
}

fn colorable(kind: AnnotationKind) -> bool {
    !kind.is_area_edit() && !kind.is_effect() && kind != AnnotationKind::Stamp
}

fn output_point(position: Pos2, rect: Rect, scale: f32, size: [u32; 2]) -> Point {
    let p = (position - rect.min) / scale;
    Point {
        x: p.x.clamp(0., size[0] as f32),
        y: p.y.clamp(0., size[1] as f32),
    }
}

fn sample_pixel(position: Pos2, rect: Rect, scale: f32, size: [u32; 2]) -> Option<[u32; 2]> {
    let p = (position - rect.min) / scale;
    (p.x >= 0. && p.y >= 0. && p.x < size[0] as f32 && p.y < size[1] as f32)
        .then_some([p.x.floor() as u32, p.y.floor() as u32])
}

fn sample_document(document: &Document, pixel: [u32; 2]) -> Result<Sample, String> {
    // Use the same full-resolution composition as exports, including masks,
    // annotations and privacy effects. Never sample the unprocessed source.
    let image = render::flatten(document)?;
    image
        .get_pixel_checked(pixel[0], pixel[1])
        .map(|rgba| Sample {
            pixel,
            rgba: rgba.0,
        })
        .ok_or_else(|| "Choose a pixel inside the image.".into())
}

fn measured(document: &Document, ends: [Point; 2]) -> (Vec2, f32) {
    let [a, b] = ends.map(|p| point(document.geometry.map_point(p, original_size(document))));
    let delta = b - a;
    (delta.abs(), delta.length())
}

fn px(value: f32) -> String {
    if (value - value.round()).abs() < 0.05 {
        format!("{value:.0} px")
    } else {
        format!("{value:.1} px")
    }
}

impl EditorState {
    pub(super) fn precision_active(&self) -> bool {
        self.tool == Tool::ConnectedArrow
            || self.retouch_active()
            || matches!(
                self.tool,
                Tool::StyleBrush | Tool::Measure | Tool::ColorInspector
            )
    }

    pub(super) fn start_precision_tool(
        &mut self,
        tool: Tool,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        self.finish_gesture(document, history);
        self.area = None;
        self.area_mask.clear();
        self.pixel_mask = None;
        self.precision.measure_dragging = false;
        self.precision.message = None;
        self.precision.preview_failed = false;
        if tool == Tool::StyleBrush {
            self.precision.brush = None;
        }
        if tool == Tool::Measure {
            self.precision.measurement = None;
        }
        self.tool = tool;
        self.focus_text = false;
        self.focus_canvas = true;
        self.inspector.show(InspectorPanel::Styles);
    }

    pub(super) fn stop_precision_tool(&mut self) {
        self.stop_retouch();
        self.precision.measure_dragging = false;
        self.precision.measurement = None;
        self.precision.preview = None;
        self.tool = Tool::Area;
        self.focus_canvas = true;
    }

    pub(super) fn start_style_brush(
        &mut self,
        document: &mut Document,
        history: &mut EditHistory,
        source: Option<usize>,
    ) {
        self.start_precision_tool(Tool::StyleBrush, document, history);
        if let Some(source) = source {
            self.pick_brush(source, document);
        }
    }

    fn pick_brush(&mut self, index: usize, document: &Document) {
        let Some(annotation) = document
            .annotations
            .get(index)
            .filter(|a| a.visible() && !a.kind.is_area_edit())
        else {
            self.precision.message = Some("Choose an annotation to copy its style.".into());
            return;
        };
        self.precision.brush = Some(Brush {
            kind: annotation.kind,
            appearance: tool_appearance::Appearance::from_annotation(annotation),
        });
        self.precision.message = Some(format!(
            "{} style copied. Click compatible artwork to apply it.",
            quick_styles::kind_name(annotation.kind)
        ));
    }

    fn brush_click(
        &mut self,
        index: usize,
        document: &mut Document,
        history: &mut EditHistory,
        repick: bool,
    ) {
        if self.precision.brush.is_none() || repick {
            self.pick_brush(index, document);
            return;
        }
        let brush = self.precision.brush.as_ref().unwrap();
        let Some(target) = document.annotations.get(index) else {
            return;
        };
        if target.locked() || !target.visible() {
            self.precision.message =
                Some("Unlock the target in Layers before applying a style.".into());
            return;
        }
        if !compatible(brush.kind, target.kind) {
            self.precision.message = Some(format!(
                "Choose another {}{}.",
                quick_styles::kind_name(brush.kind),
                if brush.kind.is_shape() {
                    " or shape"
                } else {
                    " annotation"
                }
            ));
            return;
        }
        let before = document.edit_state();
        brush
            .appearance
            .apply_to_annotation(&mut document.annotations[index]);
        if let Err(error) = crate::document::validate_document(document) {
            document.annotations = before.annotations;
            self.precision.message = Some(error);
            return;
        }
        history.commit(before, document);
        self.select(Some(index), document);
        self.remember_appearance(document.annotations[index].kind);
        self.precision.message =
            Some("Style applied. Click another target, or press Escape to finish.".into());
    }

    pub(super) fn precision_canvas_input(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        rect: Rect,
        scale: f32,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if !self.precision_active() || !ui.is_enabled() {
            return;
        }
        if self.retouch_active() {
            self.retouch_input(ui, response, rect, scale, document);
            return;
        }
        if self.tool == Tool::ConnectedArrow {
            if response.clicked()
                && let Some(p) = response.interact_pointer_pos()
            {
                self.connector_click(
                    object_point(p, rect, scale, document),
                    scale,
                    document,
                    history,
                );
            }
            return;
        }
        let events = ui.input(|input| input.events.clone());
        for event in events {
            match event {
                egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    ..
                } if self.tool == Tool::Measure
                    && rect.intersect(ui.clip_rect()).contains(pos)
                    && ui.ctx().layer_id_at(pos) == Some(ui.layer_id()) =>
                {
                    response.request_focus();
                    let p = object_point(pos, rect, scale, document);
                    self.precision.measurement = Some([p, p]);
                    self.precision.measure_dragging = true;
                    self.precision.message = None;
                }
                egui::Event::PointerMoved(pos)
                | egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    ..
                } if self.tool == Tool::Measure && self.precision.measure_dragging => {
                    self.update_measurement(
                        output_point(pos, rect, scale, document.output_size()),
                        ui.input(|i| i.modifiers.shift),
                        document,
                    );
                }
                _ => {}
            }
        }
        if !ui.input(|i| i.pointer.primary_down()) {
            self.precision.measure_dragging = false;
        }
        if response.clicked_by(PointerButton::Primary)
            && let Some(position) = response.interact_pointer_pos()
        {
            response.request_focus();
            if self.tool == Tool::StyleBrush {
                if let Some(index) = hit_document_annotation(
                    document,
                    object_point(position, rect, scale, document),
                    scale,
                ) {
                    self.brush_click(index, document, history, ui.input(|i| i.modifiers.alt));
                } else {
                    self.precision.message = Some(
                        "Click an annotation. Hold Option/Alt to copy a different style.".into(),
                    );
                }
            } else if self.tool == Tool::ColorInspector
                && let Some(pixel) = sample_pixel(position, rect, scale, document.output_size())
            {
                self.start_color_sample(ui.ctx(), document, pixel);
            }
        }
    }

    fn update_measurement(&mut self, mut end: Point, constrain: bool, document: &Document) {
        let Some(ends) = &mut self.precision.measurement else {
            return;
        };
        let start = document
            .geometry
            .map_point(ends[0], original_size(document));
        let horizontal = self.precision.axis == Axis::Horizontal
            || (self.precision.axis == Axis::Free
                && constrain
                && (end.x - start.x).abs() >= (end.y - start.y).abs());
        let vertical = self.precision.axis == Axis::Vertical
            || (self.precision.axis == Axis::Free && constrain && !horizontal);
        if horizontal {
            end.y = start.y;
        }
        if vertical {
            end.x = start.x;
        }
        ends[1] = document
            .geometry
            .inverse_point(end, original_size(document));
    }

    fn add_dimension(&mut self, document: &mut Document, history: &mut EditHistory) {
        let Some(ends) = self.precision.measurement else {
            return;
        };
        let (_, length) = measured(document, ends);
        if length < 1. || self.precision.measure_dragging {
            return;
        }
        if document.annotations.len() + 2 > crate::document::MAX_ANNOTATIONS {
            self.precision.message = Some("The project has reached its annotation limit.".into());
            return;
        }
        let before = document.edit_state();
        let first = document.annotations.len();
        let group = (1..)
            .find(|id| !document.annotations.iter().any(|a| a.group_id == Some(*id)))
            .unwrap();
        let color = [35, 112, 225, 255];
        let mut line = self.new_annotation(AnnotationKind::Arrow, ends[0]);
        line.end = ends[1];
        line.color = color;
        line.stroke_width = 3.;
        line.arrow = crate::arrow_style::ArrowStyle {
            start: crate::arrow_style::Endpoint::Bar,
            end: crate::arrow_style::Endpoint::Bar,
            ..Default::default()
        };
        line.group_id = Some(group);
        line.group_layer.name = "Measurement".into();
        line.layer.name = "Dimension line".into();
        let mid = from_pos(point(ends[0]).lerp(point(ends[1]), 0.5));
        let mut label = self.new_annotation(AnnotationKind::Text, mid);
        label.text = px(length);
        label.color = color;
        label.font_size = 32.;
        label.text_style = Default::default();
        let size = render::annotation_bounds(&label).size();
        // Keep the caption editable and separate from its dimension line.
        label.start = from_pos(point(mid) - egui::vec2(size.x / 2., size.y + 6.));
        label.end = label.start;
        label.group_id = Some(group);
        label.group_layer = line.group_layer.clone();
        label.layer.name = "Dimension label".into();
        document.annotations.extend([line, label]);
        let indices = [first, first + 1];
        if let Err(error) = crate::canvas::extend_for_annotations(document, &indices)
            .and_then(|_| crate::document::validate_document(document))
        {
            document.annotations = before.annotations;
            document.geometry = before.geometry;
            self.precision.message = Some(error);
            return;
        }
        history.commit(before, document);
        self.stop_precision_tool();
        self.set_selection(indices.to_vec(), document);
    }

    fn start_color_sample(&mut self, ctx: &egui::Context, document: &Document, pixel: [u32; 2]) {
        self.precision.screen_sample = false;
        if let Some(preview) = &self.precision.preview
            && Arc::ptr_eq(&preview.source, &document.image)
            && preview.before == document.edit_state()
            && let Some(rgba) = preview.pixels.get_pixel_checked(pixel[0], pixel[1])
        {
            self.precision.sample = Some(Sample {
                pixel,
                rgba: rgba.0,
            });
            self.precision.message = None;
            return;
        }
        if self.precision.sample_job.is_some() {
            return;
        }
        let (tx, receiver) = mpsc::channel();
        let snapshot = document.clone();
        let context = ctx.clone();
        let job = SampleJob {
            receiver,
            image: Arc::clone(&document.image),
            before: document.edit_state(),
        };
        match std::thread::Builder::new()
            .name("color-inspector".into())
            .spawn(move || {
                let _ = tx.send(sample_document(&snapshot, pixel));
                context.request_repaint();
            }) {
            Ok(_) => {
                self.precision.sample_job = Some(job);
                self.precision.sample = None;
                self.precision.message = None;
            }
            Err(error) => {
                self.precision.message = Some(format!("Could not sample the color: {error}"))
            }
        }
    }

    pub(super) fn poll_color_sample(&mut self, ctx: &egui::Context, document: &Document) {
        if let Some(result) = self.precision.screen.poll(ctx) {
            match result {
                Ok(Some(rgba)) => {
                    self.precision.sample = Some(Sample {
                        pixel: [0, 0],
                        rgba,
                    });
                    self.precision.screen_sample = true;
                    self.precision.message = None;
                }
                Ok(None) => {
                    self.precision.message = Some("Screen color selection canceled.".into())
                }
                Err(error) => self.precision.message = Some(error),
            }
        }
        self.poll_color_preview(ctx, document);
        let Some(job) = &self.precision.sample_job else {
            return;
        };
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
                return;
            }
            Err(_) => Err("Color sampling stopped. Click the image to try again.".into()),
        };
        let current =
            Arc::ptr_eq(&job.image, &document.image) && job.before == document.edit_state();
        self.precision.sample_job = None;
        if !current {
            self.precision.message =
                Some("The image changed. Click it again to sample the current color.".into());
            return;
        }
        match result {
            Ok(sample) => {
                self.precision.sample = Some(sample);
                self.precision.message = None;
            }
            Err(error) => self.precision.message = Some(error),
        }
    }

    fn poll_color_preview(&mut self, ctx: &egui::Context, document: &Document) {
        if self.tool != Tool::ColorInspector {
            if self.precision.preview_job.as_ref().is_some_and(|job| {
                !matches!(job.receiver.try_recv(), Err(mpsc::TryRecvError::Empty))
            }) {
                self.precision.preview_job = None;
            }
            return;
        }
        if self.precision.preview.as_ref().is_some_and(|p| {
            !Arc::ptr_eq(&p.source, &document.image) || p.before != document.edit_state()
        }) {
            self.precision.preview = None;
            self.precision.preview_failed = false;
        }
        if let Some(job) = &self.precision.preview_job {
            match job.receiver.try_recv() {
                Ok(result) => {
                    let job = self.precision.preview_job.take().unwrap();
                    if Arc::ptr_eq(&job.source, &document.image)
                        && job.before == document.edit_state()
                    {
                        match result {
                            Ok(pixels) => {
                                self.precision.preview = Some(ColorPreview {
                                    source: job.source,
                                    before: job.before,
                                    pixels,
                                })
                            }
                            Err(error) => {
                                self.precision.preview_failed = true;
                                self.precision.message = Some(error);
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(50));
                    return;
                }
                Err(_) => {
                    self.precision.preview_job = None;
                    self.precision.preview_failed = true;
                }
            }
        }
        if self.precision.preview.is_none() && !self.precision.preview_failed {
            let snapshot = document.clone();
            let (tx, receiver) = mpsc::channel();
            let context = ctx.clone();
            let job = PreviewJob {
                source: Arc::clone(&document.image),
                before: document.edit_state(),
                receiver,
            };
            match std::thread::Builder::new()
                .name("color-loupe".into())
                .spawn(move || {
                    let _ = tx.send(render::flatten(&snapshot));
                    context.request_repaint();
                }) {
                Ok(_) => self.precision.preview_job = Some(job),
                Err(error) => {
                    self.precision.preview_failed = true;
                    self.precision.message = Some(error.to_string());
                }
            }
        }
    }

    fn apply_sample(&mut self, document: &mut Document, history: &mut EditHistory, text: bool) {
        let Some(sample) = self.precision.sample else {
            return;
        };
        let indices = self.selected_indices(document);
        if indices.is_empty() || !self.can_edit_selection(document) {
            return;
        }
        let before = document.edit_state();
        for &index in &indices {
            let annotation = &mut document.annotations[index];
            if text {
                if matches!(
                    annotation.kind,
                    AnnotationKind::Callout | AnnotationKind::Step
                ) {
                    annotation.text_color = sample.rgba;
                }
            } else if colorable(annotation.kind) {
                annotation.color = sample.rgba;
            }
        }
        history.commit(before, document);
        self.set_selection(indices.clone(), document);
        if indices.len() == 1 {
            let kind = document.annotations[indices[0]].kind;
            self.remember_appearance(kind);
            if kind == AnnotationKind::Fill {
                self.remember_sampled_fill(sample.rgba);
            }
        }
        self.precision.message = Some("Sampled color applied.".into());
    }

    fn use_sample_with(&mut self, tool: Tool, document: &mut Document, history: &mut EditHistory) {
        let Some(sample) = self.precision.sample else {
            return;
        };
        self.clear_selection();
        self.area = None;
        self.area_mask.clear();
        self.pixel_mask = None;
        self.choose_tool(tool, document, history);
        self.color = sample.rgba;
        if tool == Tool::Fill {
            self.remember_sampled_fill(sample.rgba);
        } else if let Some(kind) = tool.kind() {
            self.remember_appearance(kind);
        }
    }

    pub(super) fn precision_panel(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if self.tool == Tool::ConnectedArrow {
            self.connector_panel(ui);
            return;
        }
        if self.retouch_active() {
            self.retouch_panel(ui, document, history);
            return;
        }
        match self.tool {
            Tool::StyleBrush => {
                ui.heading("Style Brush");
                if let Some(brush) = &self.precision.brush {
                    ui.label(format!(
                        "{} style ready",
                        quick_styles::kind_name(brush.kind)
                    ));
                    ui.small("Click compatible annotations to apply. Option/Alt-click to copy another style.");
                    if ui.button("Pick another style").clicked() {
                        self.precision.brush = None;
                        self.precision.message = None;
                    }
                } else {
                    ui.label("Click an annotation to copy its appearance.");
                    ui.small("Shapes can share styles. Other tools match their own kind. Text, positions and arrow bends stay intact.");
                }
            }
            Tool::Measure => {
                ui.heading("Measure");
                ui.small("Drag between two points. Shift locks to a horizontal or vertical line.");
                ui.horizontal(|ui| {
                    for (axis, label) in [
                        (Axis::Free, "Free"),
                        (Axis::Horizontal, "Horizontal"),
                        (Axis::Vertical, "Vertical"),
                    ] {
                        if ui
                            .selectable_value(&mut self.precision.axis, axis, label)
                            .changed()
                        {
                            self.precision.measurement = None;
                        }
                    }
                });
                if let Some(ends) = self.precision.measurement {
                    let (delta, length) = measured(document, ends);
                    ui.label(egui::RichText::new(px(length)).size(24.).strong());
                    ui.label(format!("Width {} · Height {}", px(delta.x), px(delta.y)));
                    ui.weak("Measured in output image pixels.");
                    ui.add_space(4.);
                    if ui
                        .add_enabled(
                            length >= 1. && !self.precision.measure_dragging,
                            egui::Button::new("Add dimension annotation"),
                        )
                        .clicked()
                    {
                        self.add_dimension(document, history);
                    }
                    if ui.button("Copy measurement").clicked() {
                        ui.ctx().copy_text(format!(
                            "{} (width {}, height {})",
                            px(length),
                            px(delta.x),
                            px(delta.y)
                        ));
                    }
                    ui.small("The added label records this measurement. Its line and text remain editable together or separately.");
                    if ui.button("Clear measurement").clicked() {
                        self.precision.measurement = None;
                    }
                }
            }
            Tool::ColorInspector => {
                ui.heading("Color Inspector");
                ui.small("Move over the image for a magnified pixel view, then click to pick. Includes annotations and effects.");
                if ui
                    .add_enabled(
                        !self.precision.screen.active(),
                        egui::Button::new("Pick color anywhere on screen…"),
                    )
                    .clicked()
                    && let Err(error) = self.precision.screen.start(ui.ctx())
                {
                    self.precision.message = Some(error);
                }
                if self.precision.screen.active() {
                    ui.small("Point anywhere and click. Escape cancels.");
                }
                if self.precision.sample_job.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Sampling color…");
                    });
                }
                if let Some(sample) = self.precision.sample {
                    let color = Color32::from_rgba_unmultiplied(
                        sample.rgba[0],
                        sample.rgba[1],
                        sample.rgba[2],
                        sample.rgba[3],
                    );
                    let (rect, _) = ui
                        .allocate_exact_size(egui::vec2(ui.available_width(), 38.), Sense::hover());
                    for row in 0..4 {
                        for col in 0..32 {
                            let cell = Rect::from_min_size(
                                rect.min + egui::vec2(col as f32 * 10., row as f32 * 10.),
                                Vec2::splat(10.),
                            )
                            .intersect(rect);
                            if cell.is_positive() {
                                ui.painter().rect_filled(
                                    cell,
                                    0.,
                                    if (row + col) % 2 == 0 {
                                        Color32::WHITE
                                    } else {
                                        Color32::from_gray(210)
                                    },
                                );
                            }
                        }
                    }
                    ui.painter().rect_filled(rect, 0., color);
                    ui.label(egui::RichText::new(sample.hex()).monospace().size(20.));
                    ui.label(egui::RichText::new(sample.rgb()).monospace());
                    ui.small(if self.precision.screen_sample {
                        "Screen color · sRGB".into()
                    } else {
                        format!(
                            "Pixel {}, {} · Alpha {:.0}%",
                            sample.pixel[0],
                            sample.pixel[1],
                            f32::from(sample.rgba[3]) / 2.55
                        )
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Copy HEX").clicked() {
                            ui.ctx().copy_text(sample.hex());
                        }
                        if ui.button("Copy RGB").clicked() {
                            ui.ctx().copy_text(sample.rgb());
                        }
                    });
                    ui.separator();
                    let selected = self.selected_indices(document);
                    let editable = !selected.is_empty() && self.can_edit_selection(document);
                    let can_color = editable
                        && selected
                            .iter()
                            .any(|&i| colorable(document.annotations[i].kind));
                    if ui
                        .add_enabled(can_color, egui::Button::new("Apply to selected color"))
                        .clicked()
                    {
                        self.apply_sample(document, history, false);
                    }
                    let can_text = editable
                        && selected.iter().any(|&i| {
                            matches!(
                                document.annotations[i].kind,
                                AnnotationKind::Step | AnnotationKind::Callout
                            )
                        });
                    if can_text && ui.button("Apply to selected text color").clicked() {
                        self.apply_sample(document, history, true);
                    }
                    ui.menu_button("Use with drawing tool…", |ui| {
                        for (tool, label, _) in self
                            .tools()
                            .into_iter()
                            .filter(|(tool, _, _)| tool.kind().is_some_and(colorable))
                        {
                            if ui.button(label).clicked() {
                                self.use_sample_with(tool, document, history);
                                ui.close();
                            }
                        }
                    });
                }
            }
            _ => return,
        }
        if let Some(message) = &self.precision.message {
            ui.add_space(6.);
            ui.label(message);
        }
        ui.add_space(8.);
        if ui
            .button("Done")
            .on_hover_text("Return to Select (Escape)")
            .clicked()
        {
            self.stop_precision_tool();
        }
    }

    pub(super) fn paint_precision_overlay(
        &mut self,
        ui: &egui::Ui,
        rect: Rect,
        scale: f32,
        document: &Document,
    ) {
        if self.retouch_active() {
            self.paint_retouch(ui, rect, scale, document);
            return;
        }
        if self.tool == Tool::ColorInspector
            && let Some(preview) = &self.precision.preview
            && let Some(pointer) = ui.input(|i| i.pointer.hover_pos())
            && rect.intersect(ui.clip_rect()).contains(pointer)
            && ui.ctx().layer_id_at(pointer) == Some(ui.layer_id())
        {
            let painter = ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                egui::Id::new("color-inspector-loupe"),
            ));
            crate::capture_magnifier::paint_color(
                &painter,
                pointer,
                rect,
                ui.clip_rect(),
                &preview.pixels,
            );
        }
        if self.tool != Tool::Measure {
            return;
        }
        let Some(ends) = self.precision.measurement else {
            return;
        };
        let [a, b] = ends.map(|p| {
            rect.min
                + point(document.geometry.map_point(p, original_size(document))).to_vec2() * scale
        });
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        let accent = crate::appearance::palette(ui.ctx()).accent;
        painter.line_segment([a, b], Stroke::new(4., Color32::WHITE));
        painter.line_segment([a, b], Stroke::new(2., accent));
        let normal = if a.distance(b) > 0.01 {
            (b - a).normalized().rot90() * 6.
        } else {
            egui::vec2(0., 6.)
        };
        for p in [a, b] {
            painter.line_segment([p - normal, p + normal], Stroke::new(2., accent));
            painter.circle_filled(p, 3., accent);
        }
        let (_, length) = measured(document, ends);
        let galley =
            painter.layout_no_wrap(px(length), egui::FontId::proportional(14.), Color32::WHITE);
        let bounds = Rect::from_center_size(
            a.lerp(b, 0.5) - egui::vec2(0., 17.),
            galley.size() + egui::vec2(12., 6.),
        );
        painter.rect_filled(bounds, 4., accent);
        painter.galley(bounds.min + egui::vec2(6., 3.), galley, Color32::WHITE);
    }
}

pub(super) fn paint_icon(
    painter: &egui::Painter,
    center: Pos2,
    tool: Tool,
    color: Color32,
    stroke: Stroke,
) {
    match tool {
        Tool::StyleBrush => {
            painter.rect_stroke(
                Rect::from_center_size(center + egui::vec2(0., -5.), egui::vec2(16., 8.)),
                2.,
                stroke,
                StrokeKind::Inside,
            );
            painter.add(egui::Shape::line(
                vec![
                    center + egui::vec2(8., -5.),
                    center + egui::vec2(11., -5.),
                    center + egui::vec2(11., 2.),
                    center + egui::vec2(0., 2.),
                    center + egui::vec2(0., 8.),
                ],
                stroke,
            ));
            painter.rect_filled(
                Rect::from_center_size(center + egui::vec2(0., 8.), egui::vec2(4., 7.)),
                1.,
                color,
            );
        }
        Tool::Measure => {
            painter.rect_stroke(
                Rect::from_center_size(center, egui::vec2(22., 12.)),
                1.,
                stroke,
                StrokeKind::Inside,
            );
            for (x, length) in [(-7., 4.), (-3., 6.), (1., 4.), (5., 6.)] {
                painter.line_segment(
                    [
                        center + egui::vec2(x, -6.),
                        center + egui::vec2(x, -6. + length),
                    ],
                    stroke,
                );
            }
        }
        Tool::ColorInspector => {
            painter.add(egui::Shape::closed_line(
                vec![
                    center + egui::vec2(-8., 8.),
                    center + egui::vec2(-6., 2.),
                    center + egui::vec2(4., -8.),
                    center + egui::vec2(8., -4.),
                    center + egui::vec2(-2., 6.),
                ],
                stroke,
            ));
            painter.line_segment(
                [center + egui::vec2(0., -7.), center + egui::vec2(7., 0.)],
                stroke,
            );
            painter.circle_filled(center + egui::vec2(8., 7.), 3., color);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> Document {
        Document {
            image: Arc::new(image::RgbaImage::from_pixel(
                600,
                400,
                image::Rgba([240, 241, 242, 255]),
            )),
            annotations: vec![],
            geometry: Default::default(),
        }
    }

    fn object(kind: AnnotationKind, start: [f32; 2], end: [f32; 2]) -> Annotation {
        let editor = EditorState::default();
        let mut annotation = editor.new_annotation(
            kind,
            Point {
                x: start[0],
                y: start[1],
            },
        );
        annotation.end = Point {
            x: end[0],
            y: end[1],
        };
        annotation
    }

    #[test]
    fn brush_copies_appearance_preserving_target_content_geometry_groups_and_undo() {
        for kind in [
            AnnotationKind::Arrow,
            AnnotationKind::Rectangle,
            AnnotationKind::Text,
            AnnotationKind::Callout,
            AnnotationKind::Step,
            AnnotationKind::Magnify,
            AnnotationKind::Blur,
        ] {
            let mut d = document();
            let mut source = object(kind, [50., 80.], [150., 140.]);
            source.color = [23, 44, 160, 180];
            source.stroke_width = 7.;
            source.fill = true;
            source.text = "source content must not travel".into();
            source.font_size = 40.;
            source.text_style.italic = true;
            source.text_color = [12, 34, 56, 255];
            source.effect_size = 32;
            if kind == AnnotationKind::Arrow {
                source.arrow.pattern = crate::arrow_style::Pattern::Dashed;
                source.arrow.start = crate::arrow_style::Endpoint::Circle;
                source.arrow.curve = Some([Point { x: 75., y: 25. }, Point { x: 125., y: 25. }]);
            }
            if kind.is_studio() {
                source.studio.zoom = 4.;
            }
            let mut target = object(kind, [250., 150.], [350., 210.]);
            target.text = "keep target content".into();
            target.step_number = 12;
            target.group_id = Some(14);
            target.layer.name = "Target".into();
            if kind == AnnotationKind::Arrow {
                target.arrow.curve = Some([Point { x: 275., y: 100. }, Point { x: 325., y: 100. }]);
                target.arrow.elbow = true;
            }
            if kind.is_studio() {
                target.studio.source_offset = [0.5, 0.75];
            }
            let saved = target.clone();
            d.annotations = vec![source.clone(), target];
            let before = d.edit_state();
            let mut h = EditHistory::new(before.clone());
            let mut e = EditorState::default();
            e.start_style_brush(&mut d, &mut h, Some(0));
            assert_eq!(d.edit_state(), before);
            assert!(!h.can_undo());
            e.brush_click(1, &mut d, &mut h, false);
            let result = &d.annotations[1];
            assert_eq!(result.start, saved.start);
            assert_eq!(result.end, saved.end);
            assert_eq!(result.text, saved.text);
            assert_eq!(result.step_number, saved.step_number);
            assert_eq!(result.tail, saved.tail);
            assert_eq!(result.group_id, saved.group_id);
            assert_eq!(result.layer, saved.layer);
            assert_eq!(result.arrow.curve, saved.arrow.curve);
            assert_eq!(result.arrow.elbow, saved.arrow.elbow);
            assert_eq!(result.studio.source_offset, saved.studio.source_offset);
            if kind.is_effect() {
                assert_eq!(result.effect_size, source.effect_size);
            } else {
                assert_eq!(result.color, source.color);
            }
            if matches!(kind, AnnotationKind::Text | AnnotationKind::Callout) {
                assert_eq!(result.text_style, source.text_style);
                assert_eq!(result.font_size, source.font_size);
            }
            crate::document::validate_document(&d).unwrap();
            assert!(h.undo(&mut d));
            assert_eq!(d.edit_state(), before);
            assert!(h.redo(&mut d));
        }
    }

    #[test]
    fn brush_rejects_locked_incompatible_and_hidden_targets_and_can_repick() {
        let mut d = document();
        let mut locked = object(AnnotationKind::Arrow, [100., 100.], [200., 200.]);
        locked.group_layer.locked = true;
        let mut hidden = locked.clone();
        hidden.group_layer.locked = false;
        hidden.layer.hidden = true;
        d.annotations = vec![
            object(AnnotationKind::Arrow, [20., 30.], [60., 70.]),
            locked,
            object(AnnotationKind::Text, [80., 30.], [80., 30.]),
            hidden,
        ];
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState::default();
        e.start_style_brush(&mut d, &mut h, Some(0));
        for index in [1, 2, 3] {
            e.brush_click(index, &mut d, &mut h, false);
        }
        assert_eq!(d.edit_state(), before);
        assert!(!h.can_undo());
        e.brush_click(2, &mut d, &mut h, true);
        assert_eq!(
            e.precision.brush.as_ref().unwrap().kind,
            AnnotationKind::Text
        );
        assert!(compatible(
            AnnotationKind::Rectangle,
            AnnotationKind::Ellipse
        ));
        assert!(!compatible(
            AnnotationKind::MoveArea,
            AnnotationKind::MoveArea
        ));
    }

    #[test]
    fn measure_uses_output_pixels_after_crop_rotation_flip_resize_and_axis_lock() {
        let mut d = document();
        d.geometry.crop = Some(crate::geometry::CropRect {
            x: 20,
            y: 30,
            width: 400,
            height: 300,
        });
        d.geometry.quarter_turns = 1;
        d.geometry.flip_horizontal = true;
        d.geometry.resize = Some([900, 200]);
        let ends = [Point { x: 60., y: 80. }, Point { x: 260., y: 180. }];
        let (delta, length) = measured(&d, ends);
        assert_eq!(delta, egui::vec2(300., 100.));
        assert!((length - 316.22777).abs() < 0.001);
        let mut e = EditorState::default();
        e.precision.measurement = Some(ends);
        e.precision.axis = Axis::Horizontal;
        e.update_measurement(Point { x: 450., y: 120. }, false, &d);
        let (delta, _) = measured(&d, e.precision.measurement.unwrap());
        assert!(delta.y < 0.001);
        e.precision.axis = Axis::Vertical;
        e.update_measurement(Point { x: 450., y: 120. }, false, &d);
        let (delta, _) = measured(&d, e.precision.measurement.unwrap());
        assert!(delta.x < 0.001);
        let rect = Rect::from_min_size(egui::pos2(50., 70.), egui::vec2(450., 100.));
        assert_eq!(
            output_point(egui::pos2(200., 100.), rect, 0.5, [900, 200]),
            Point { x: 300., y: 60. }
        );
    }

    #[test]
    fn dimension_annotation_is_one_undo_step_and_survives_project_round_trip() {
        let mut d = document();
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState {
            tool: Tool::Measure,
            ..Default::default()
        };
        e.precision.measurement = Some([Point { x: 120., y: 150. }, Point { x: 220., y: 150. }]);
        assert!(!h.can_undo());
        e.add_dimension(&mut d, &mut h);
        assert_eq!(d.annotations.len(), 2);
        assert_eq!(d.annotations[1].text, "100 px");
        assert_eq!(d.annotations[0].group_id, d.annotations[1].group_id);
        assert_eq!(e.selected_indices(&d), vec![0, 1]);
        assert_eq!(
            d.annotations[0].arrow.start,
            crate::arrow_style::Endpoint::Bar
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dimension.myimage");
        crate::document::save_project(&d, &path).unwrap();
        let loaded = crate::document::load_project(&path).unwrap();
        assert_eq!(loaded.edit_state(), d.edit_state());
        assert_ne!(render::flatten(&d).unwrap(), *d.image);
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), before);
        assert!(!h.can_undo());
        assert!(h.redo(&mut d));
        assert_eq!(d.annotations[1].text, "100 px");
    }

    #[test]
    fn color_samples_composition_not_source_and_maps_zoomed_pixels_exactly() {
        let mut d = document();
        let mut cover = object(AnnotationKind::Rectangle, [40., 40.], [90., 90.]);
        cover.color = [20, 80, 200, 255];
        cover.fill = true;
        d.annotations.push(cover);
        d.geometry.crop = Some(crate::geometry::CropRect {
            x: 20,
            y: 20,
            width: 200,
            height: 160,
        });
        d.geometry.quarter_turns = 1;
        d.geometry.flip_horizontal = true;
        d.geometry.resize = Some([320, 400]);
        let p = d
            .geometry
            .map_point(Point { x: 60., y: 60. }, original_size(&d));
        let pixel = [p.x as u32, p.y as u32];
        assert_eq!(sample_document(&d, pixel).unwrap().rgba, [20, 80, 200, 255]);
        let rect = Rect::from_min_size(egui::pos2(30., 50.), egui::vec2(160., 200.));
        assert_eq!(
            sample_pixel(
                rect.min + egui::vec2(pixel[0] as f32 * 0.5 + 0.2, pixel[1] as f32 * 0.5 + 0.2),
                rect,
                0.5,
                d.output_size()
            ),
            Some(pixel)
        );
        assert_eq!(
            sample_pixel(rect.right_bottom(), rect, 0.5, d.output_size()),
            None
        );
        let transparent = Sample {
            pixel: [0, 0],
            rgba: [10, 20, 30, 128],
        };
        assert_eq!(transparent.hex(), "#0A141E80");
        assert_eq!(transparent.rgb(), "rgba(10, 20, 30, 0.502)");
    }

    #[test]
    fn color_sampling_includes_privacy_effects_and_rejects_stale_results() {
        let mut d = document();
        d.image = Arc::new(image::RgbaImage::from_fn(600, 400, |x, y| {
            image::Rgba([if (x + y) % 2 == 0 { 0 } else { 255 }, 20, 30, 255])
        }));
        let mut effect = object(AnnotationKind::Pixelate, [0., 0.], [100., 100.]);
        effect.effect_size = 16;
        d.annotations.push(effect);
        let sample = sample_document(&d, [20, 20]).unwrap();
        assert_ne!(sample.rgba, d.image.get_pixel(20, 20).0);
        let (tx, receiver) = mpsc::channel();
        let mut e = EditorState::default();
        e.precision.sample_job = Some(SampleJob {
            receiver,
            image: Arc::clone(&d.image),
            before: d.edit_state(),
        });
        tx.send(Ok(sample)).unwrap();
        d.annotations[0].effect_size = 32;
        e.poll_color_sample(&egui::Context::default(), &d);
        assert!(e.precision.sample.is_none());
        assert!(e.precision.sample_job.is_none());
        assert!(
            e.precision
                .message
                .as_ref()
                .unwrap()
                .contains("image changed")
        );
    }

    #[test]
    fn sampled_color_applies_only_color_with_undo_lock_checks_and_remembered_defaults() {
        let mut d = document();
        d.annotations
            .push(object(AnnotationKind::Arrow, [50., 70.], [200., 100.]));
        d.annotations[0].arrow.curve = Some([Point { x: 80., y: 20. }, Point { x: 150., y: 20. }]);
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState::default();
        e.select(Some(0), &d);
        e.start_precision_tool(Tool::ColorInspector, &mut d, &mut h);
        e.precision.sample = Some(Sample {
            pixel: [0, 0],
            rgba: [1, 33, 77, 180],
        });
        e.apply_sample(&mut d, &mut h, false);
        let mut expected = before.annotations[0].clone();
        expected.color = [1, 33, 77, 180];
        assert_eq!(d.annotations[0], expected);
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), before);
        d.annotations[0].layer.locked = true;
        let locked = d.edit_state();
        e.apply_sample(&mut d, &mut h, false);
        assert_eq!(d.edit_state(), locked);
        e.use_sample_with(Tool::Arrow, &mut d, &mut h);
        assert_eq!(d.edit_state(), locked);
        assert_eq!(e.color, [1, 33, 77, 180]);
        e.activate_tool(Tool::Text);
        e.activate_tool(Tool::Arrow);
        assert_eq!(e.color, [1, 33, 77, 180]);
    }

    struct Workspace {
        ctx: egui::Context,
        editor: EditorState,
        document: Document,
        history: EditHistory,
        texture: egui::TextureHandle,
        preview: crate::privacy_preview::PrivacyPreview,
        time: f64,
    }
    impl Workspace {
        fn new() -> Self {
            let ctx = egui::Context::default();
            crate::appearance::apply(&ctx);
            ctx.enable_accesskit();
            let texture = ctx.load_texture(
                "precision-source",
                egui::ColorImage::filled([1, 1], Color32::WHITE),
                Default::default(),
            );
            let document = document();
            let history = EditHistory::new(document.edit_state());
            Self {
                ctx,
                editor: EditorState::default(),
                document,
                history,
                texture,
                preview: Default::default(),
                time: 0.,
            }
        }
        fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
            self.time += 0.03;
            self.ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000., 680.))),
                    time: Some(self.time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::TopBottomPanel::top("precision-ribbon").show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            self.editor
                                .toolbar(ui, &mut self.document, &mut self.history)
                        });
                    });
                    egui::CentralPanel::default().show(ctx, |ui| {
                        self.editor.ui(
                            ui,
                            &mut self.document,
                            &mut self.history,
                            &self.texture,
                            &mut self.preview,
                        )
                    });
                },
            )
        }
        fn settle(&mut self) -> egui::FullOutput {
            self.frame(vec![]);
            self.frame(vec![]);
            self.frame(vec![])
        }
        fn click(&mut self, label: &str) {
            let output = self.settle();
            let nodes = output.platform_output.accesskit_update.unwrap().nodes;
            let r = nodes
                .iter()
                .find_map(|(_, n)| (n.label() == Some(label)).then(|| n.bounds()).flatten())
                .unwrap_or_else(|| panic!("Missing {label}"));
            let p = egui::pos2(((r.x0 + r.x1) / 2.) as f32, ((r.y0 + r.y1) / 2.) as f32);
            self.pointer(p, PointerButton::Primary);
            self.settle();
        }
        fn pointer(&mut self, pos: Pos2, button: PointerButton) {
            for pressed in [true, false] {
                self.frame(vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ]);
            }
        }
        fn rect(&mut self) -> Rect {
            let output = self.settle();
            self.ctx
                .tessellate(output.shapes, output.pixels_per_point)
                .into_iter()
                .filter_map(|p| match p.primitive {
                    egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == self.texture.id() => {
                        Some(mesh.vertices)
                    }
                    _ => None,
                })
                .flatten()
                .map(|v| v.pos)
                .fold(Rect::NOTHING, |r, p| r.union(Rect::from_min_max(p, p)))
        }
        fn key_escape(&mut self) {
            self.frame(vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: Some(Key::Escape),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }]);
        }
    }

    #[test]
    fn tools_open_from_overflow_keep_canvas_stable_and_escape_returns_to_select() {
        let mut w = Workspace::new();
        let initial = w.document.edit_state();
        for (label, tool) in [
            ("Style Brush", Tool::StyleBrush),
            ("Measure", Tool::Measure),
            ("Color Inspector", Tool::ColorInspector),
        ] {
            assert!(!w.editor.toolbar_pins.iter().any(|p| p == label));
            let rect = w.rect();
            w.click("More tools");
            w.click(label);
            assert_eq!(w.editor.tool, tool);
            assert_eq!(w.rect(), rect);
            assert!(w.editor.inspector.preferences.expanded);
            w.key_escape();
            assert_eq!(w.editor.tool, Tool::Area);
            assert_eq!(w.document.edit_state(), initial);
            assert!(!w.history.can_undo());
        }
    }

    #[test]
    fn measure_real_pointer_drag_does_not_draw_or_resize_and_add_is_explicit() {
        let mut w = Workspace::new();
        let before = w.document.edit_state();
        w.click("More tools");
        w.click("Measure");
        let rect = w.rect();
        let a = rect.min + egui::vec2(80., 100.);
        let b = a + egui::vec2(150., 0.);
        w.frame(vec![
            egui::Event::PointerMoved(a),
            egui::Event::PointerButton {
                pos: a,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            egui::Event::PointerMoved(b),
            egui::Event::PointerButton {
                pos: b,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ]);
        assert_eq!(w.document.edit_state(), before);
        assert!(!w.history.can_undo());
        assert!(w.editor.gesture.is_none());
        assert!(w.editor.inspector.preferences.expanded);
        assert_eq!(w.rect(), rect);
        let (_, length) = measured(&w.document, w.editor.precision.measurement.unwrap());
        assert!((length - 150. / w.editor.zoom).abs() < 0.01);
        w.click("Add dimension annotation");
        assert_eq!(w.document.annotations.len(), 2);
        assert!(w.history.undo(&mut w.document));
        assert_eq!(w.document.edit_state(), before);
    }

    #[test]
    fn right_click_brush_uses_clicked_object_and_click_applies_without_moving_target() {
        let mut w = Workspace::new();
        let mut source = object(AnnotationKind::Rectangle, [70., 90.], [150., 170.]);
        source.fill = true;
        source.color = [30, 180, 70, 255];
        let mut target = object(AnnotationKind::Ellipse, [250., 90.], [330., 170.]);
        target.fill = true;
        w.document.annotations = vec![source, target];
        let before = w.document.edit_state();
        w.history = EditHistory::new(before.clone());
        w.editor.select(Some(1), &w.document);
        let rect = w.rect();
        let scale = w.editor.zoom;
        w.pointer(
            rect.min + egui::vec2(110., 130.) * scale,
            PointerButton::Secondary,
        );
        w.click("Style Brush");
        assert_eq!(w.editor.tool, Tool::StyleBrush);
        assert_eq!(w.document.edit_state(), before);
        w.pointer(
            rect.min + egui::vec2(290., 130.) * scale,
            PointerButton::Primary,
        );
        assert_eq!(w.document.annotations[1].color, [30, 180, 70, 255]);
        assert_eq!(w.document.annotations[1].start, before.annotations[1].start);
        assert_eq!(w.document.annotations.len(), 2);
        assert!(w.editor.gesture.is_none());
        assert!(w.history.undo(&mut w.document));
        assert_eq!(w.document.edit_state(), before);
    }
    #[test]
    fn color_copy_buttons_emit_exact_values_without_editing_the_document() {
        let mut w = Workspace::new();
        w.click("More tools");
        w.click("Color Inspector");
        w.editor.precision.sample = Some(Sample {
            pixel: [12, 34],
            rgba: [10, 20, 30, 128],
        });
        let before = w.document.edit_state();
        for (label, expected) in [
            ("Copy HEX", "#0A141E80"),
            ("Copy RGB", "rgba(10, 20, 30, 0.502)"),
        ] {
            let output = w.settle();
            let bounds = output
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .iter()
                .find_map(|(_, n)| (n.label() == Some(label)).then(|| n.bounds()).flatten())
                .unwrap();
            let pos = egui::pos2(
                ((bounds.x0 + bounds.x1) / 2.) as f32,
                ((bounds.y0 + bounds.y1) / 2.) as f32,
            );
            w.frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ]);
            let output = w.frame(vec![egui::Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }]);
            assert!(
                output
                    .platform_output
                    .commands
                    .iter()
                    .any(|c| matches!(c, egui::OutputCommand::CopyText(text) if text == expected))
            );
        }
        assert_eq!(w.document.edit_state(), before);
        assert!(!w.history.can_undo());
    }

    #[test]
    fn cancel_measurement_mid_drag_leaves_no_drawing_after_release() {
        let mut w = Workspace::new();
        let before = w.document.edit_state();
        w.click("More tools");
        w.click("Measure");
        let rect = w.rect();
        let pos = rect.min + egui::vec2(80., 100.);
        w.frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ]);
        assert!(w.editor.precision.measure_dragging);
        w.key_escape();
        assert_eq!(w.editor.tool, Tool::Area);
        assert!(!w.editor.precision.measure_dragging);
        w.frame(vec![egui::Event::PointerButton {
            pos: pos + egui::vec2(100., 50.),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
        assert_eq!(w.document.edit_state(), before);
        assert!(!w.history.can_undo());
        assert!(w.editor.gesture.is_none());
        assert!(w.editor.area_drag.is_none());
    }
}
