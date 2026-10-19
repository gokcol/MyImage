use crate::appearance::Icon as DialogIcon;
use crate::dialogs::{self, Dialog};
use crate::{
    document::{Annotation, AnnotationKind, Document, EditHistory, EditState, Point},
    geometry::{CropRect, ImageGeometry},
    render,
};
#[path = "annotation_clipboard.rs"]
mod annotation_clipboard;
#[path = "annotation_layers.rs"]
mod annotation_layers;
#[path = "annotation_layout.rs"]
mod annotation_layout;
#[path = "annotation_presets.rs"]
mod annotation_presets;
#[path = "annotation_selection.rs"]
mod annotation_selection;
#[path = "area_selection.rs"]
mod area_selection;
#[path = "canvas_handles.rs"]
mod canvas_handles;
#[path = "canvas_tools.rs"]
mod canvas_tools;
#[path = "object_canvas.rs"]
mod object_canvas;
use object_canvas::object_point;
#[path = "editor_toolbar.rs"]
mod editor_toolbar;
#[path = "editor_tools.rs"]
mod editor_tools;
#[path = "fill_tool.rs"]
mod fill_tool;
#[path = "freehand.rs"]
mod freehand;
#[path = "object_properties.rs"]
mod object_properties;
#[path = "quick_styles.rs"]
mod quick_styles;
#[path = "selection_size.rs"]
mod selection_size;
#[path = "shape_picker.rs"]
mod shape_picker;
#[path = "stamp_panel.rs"]
mod stamp_panel;
#[path = "text_editor.rs"]
mod text_editor;
use annotation_selection::*;

use eframe::egui::{
    self, Color32, CursorIcon, Key, Modifiers, PointerButton, Pos2, Rect, Sense, Stroke,
    StrokeKind, Vec2,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Tool {
    #[default]
    Select,
    Area,
    Crop,
    CutOut,
    Wand,
    Magnify,
    Spotlight,
    Simplify,
    Text,
    Stamp,
    Fill,
    Arrow,
    Rectangle,
    Ellipse,
    Shape(AnnotationKind),
    Blur,
    Pixelate,
    Step,
    Callout,
    Highlighter,
    Pen,
    Eraser,
}

fn shape_tool(kind: AnnotationKind) -> Tool {
    match kind {
        AnnotationKind::Rectangle => Tool::Rectangle,
        AnnotationKind::Ellipse => Tool::Ellipse,
        _ => Tool::Shape(kind),
    }
}

impl Tool {
    fn kind(self) -> Option<AnnotationKind> {
        match self {
            Self::Select | Self::Area | Self::Crop | Self::CutOut | Self::Wand | Self::Eraser => {
                None
            }
            Self::Magnify => Some(AnnotationKind::Magnify),
            Self::Spotlight => Some(AnnotationKind::Spotlight),
            Self::Simplify => Some(AnnotationKind::Simplify),
            Self::Fill => Some(AnnotationKind::Fill),
            Self::Stamp => Some(AnnotationKind::Stamp),
            Self::Text => Some(AnnotationKind::Text),
            Self::Arrow => Some(AnnotationKind::Arrow),
            Self::Rectangle => Some(AnnotationKind::Rectangle),
            Self::Ellipse => Some(AnnotationKind::Ellipse),
            Self::Shape(kind) => Some(kind),
            Self::Blur => Some(AnnotationKind::Blur),
            Self::Pixelate => Some(AnnotationKind::Pixelate),
            Self::Step => Some(AnnotationKind::Step),
            Self::Callout => Some(AnnotationKind::Callout),
            Self::Highlighter => Some(AnnotationKind::Highlighter),
            Self::Pen => Some(AnnotationKind::Freehand),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Handle {
    ArrowStart,
    ArrowEnd,
    ArrowBend,
    ArrowCurvePoint(usize),
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Left,
    Right,
    Top,
    Bottom,
    TextSize,
    CalloutTail,
}

// On-curve adjustment handles, between the endpoints and the center bend handle.
const ARROW_CURVE_PARAMETERS: [f32; 2] = [0.25, 0.75];

enum GestureKind {
    Draw(usize),
    Move(usize),
    MoveMany(Vec<usize>),
    ResizeMany(Vec<usize>, Handle),
    Marquee { current: Point, initial: Vec<usize> },
    Resize(usize, Handle),
    Erase { last: Point },
}

struct Gesture {
    kind: GestureKind,
    before: EditState,
    origin: Point,
    preview_geometry: Option<crate::geometry::ImageGeometry>,
}

#[derive(Clone)]
struct CropDraft {
    rect: CropRect,
    drag_start: Option<Point>,
}

#[derive(Clone)]
struct ResizeDraft {
    size: [u32; 2],
    ratio: f64,
    locked: bool,
}

impl ResizeDraft {
    fn constrain_axis(&mut self, axis: usize) {
        let ratio = if axis == 0 {
            self.ratio
        } else {
            1.0 / self.ratio
        };
        // Bound the edited dimension too, so the partner never hits a limit
        // while the edited value continues changing the aspect ratio.
        let minimum = ratio.ceil().clamp(1.0, 32_768.0) as u32;
        let maximum = (32_768.0 * ratio).floor().clamp(minimum as f64, 32_768.0) as u32;
        self.size[axis] = self.size[axis].clamp(minimum, maximum);
        self.size[1 - axis] = (self.size[axis] as f64 / ratio)
            .round()
            .clamp(1.0, 32_768.0) as u32;
    }
}

pub fn default_toolbar() -> Vec<String> {
    [
        "Select", "Arrow", "Text", "Callout", "Shapes", "Stamps", "Fill",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

pub struct EditorState {
    tool: Tool,
    shape_kind: AnnotationKind,
    shapes_3d: bool,
    area: Option<Rect>,
    area_mask: Vec<Point>,
    pixel_mask: Option<crate::fill::FillRegion>,
    studio: crate::studio::Options,
    extra_tools: editor_tools::State,
    toolbar_pins: Vec<String>,
    toolbar_changed: bool,
    toolbar_customizer: bool,
    lasso: bool,
    area_drag: Option<area_selection::AreaDrag>,
    move_pixels: bool,
    area_aspect_ratio: Option<f64>,
    selected: Option<usize>,
    additional: Vec<usize>,
    focus_canvas: bool,
    object_press: bool,
    snapping: bool,
    snap_guides: Vec<annotation_layout::SnapGuide>,
    color: [u8; 4],
    stroke_width: f32,
    arrow: crate::arrow_style::ArrowStyle,
    fill: bool,
    text: String,
    font_size: f32,
    effect_size: u32,
    step_number: u32,
    step_diameter: f32,
    text_color: [u8; 4],
    shape_color: [u8; 4],
    step_color: [u8; 4],
    callout_color: [u8; 4],
    highlighter_color: [u8; 4],
    pen_color: [u8; 4],
    pen_width: f32,
    shape_width: f32,
    eraser_diameter: f32,
    plain_text: String,
    callout_text: String,
    zoom: f32,
    fit: bool,
    gesture: Option<Gesture>,
    focus_text: bool,
    text_editor_generation: u64,
    crop: Option<CropDraft>,
    resize: Option<ResizeDraft>,
    canvas_size: Option<canvas_tools::CanvasDraft>,
    canvas_resize: Option<canvas_handles::CanvasResize>,
    layers_open: bool,
    presets_open: bool,
    presets: annotation_presets::PresetState,
    stamps: stamp_panel::StampState,
    fill_tool: fill_tool::FillState,
    text_dialog: Option<text_editor::TextDraft>,
    text_style: crate::text_style::TextStyle,
    layer_target: Option<annotation_layers::LayerTarget>,
    rename_layer: Option<annotation_layers::RenameDraft>,
    image_error: Option<String>,
    clipboard_status: Option<String>,
    clipboard_job: Option<annotation_clipboard::ClipboardJob>,
    copy_confirmation: Option<(String, std::time::Instant)>,
    property_before: Option<EditState>,
    property_interacting: bool,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            tool: Tool::Select,
            shape_kind: AnnotationKind::Rectangle,
            shapes_3d: false,
            area: None,
            area_mask: Vec::new(),
            pixel_mask: None,
            arrow: Default::default(),
            studio: Default::default(),
            extra_tools: Default::default(),
            toolbar_pins: default_toolbar(),
            toolbar_changed: false,
            toolbar_customizer: false,

            lasso: false,
            area_drag: None,
            move_pixels: true,
            area_aspect_ratio: None,
            selected: None,
            additional: Vec::new(),
            focus_canvas: false,
            object_press: false,
            snapping: true,
            snap_guides: Vec::new(),
            color: [230, 66, 55, 255],
            stroke_width: 4.0,
            fill: false,
            text: "Text".into(),
            font_size: 32.0,
            effect_size: 16,
            step_number: 1,
            step_diameter: 48.0,
            text_color: [255; 4],
            shape_color: [230, 66, 55, 255],
            step_color: [230, 66, 55, 255],
            callout_color: [16, 119, 108, 255],
            highlighter_color: [255, 220, 0, 80],
            pen_color: [35, 112, 225, 255],
            pen_width: 8.0,
            shape_width: 4.0,
            eraser_diameter: 32.0,
            plain_text: "Text".into(),
            callout_text: "Callout".into(),
            zoom: 1.0,
            fit: true,
            gesture: None,
            focus_text: false,
            text_editor_generation: 0,
            crop: None,
            resize: None,
            canvas_size: None,
            canvas_resize: None,
            layers_open: false,
            presets_open: false,
            presets: Default::default(),
            stamps: Default::default(),
            fill_tool: Default::default(),
            text_dialog: None,
            text_style: Default::default(),
            layer_target: None,
            rename_layer: None,
            image_error: None,
            clipboard_status: None,
            clipboard_job: None,
            copy_confirmation: None,
            property_before: None,
            property_interacting: false,
        }
    }
}

impl EditorState {
    pub fn with_presets(path: Result<std::path::PathBuf, String>) -> Self {
        Self {
            tool: Tool::Area,
            stamps: stamp_panel::StampState::new(
                path.clone().map(|p| p.with_file_name("signatures.json")),
            ),
            presets: annotation_presets::PresetState::new(path),
            ..Default::default()
        }
    }

    pub fn reset(&mut self) {
        let pins = std::mem::take(&mut self.toolbar_pins);
        let stamps = std::mem::take(&mut self.stamps);
        let mut presets = std::mem::take(&mut self.presets);
        presets.dialog = None;
        let generation = self.text_editor_generation.wrapping_add(1);
        *self = Self::default();
        self.tool = Tool::Area;
        self.text_editor_generation = generation;
        self.presets = presets;
        self.stamps = stamps;
        self.toolbar_pins = pins;
    }

    /// Complete annotation gestures and discard unapplied image-tool previews
    /// before saving, hiding, exporting, or switching to another workspace.
    pub fn finish_gesture(&mut self, document: &mut Document, history: &mut EditHistory) {
        self.finish_canvas_resize(document, history);
        self.finish_properties(document, history);
        self.finish_text_editor(document, history, true);
        self.finish_area_drag(document, history);
        self.finish_annotation_gesture(document, history);
        self.cancel_image_tools();
    }

    pub fn autosave_ready(&self) -> bool {
        self.gesture.is_none()
            && self.canvas_resize.is_none()
            && self.area_drag.is_none()
            && !self.extra_tools.busy()
            && !self.fill_tool.busy()
    }

    pub fn has_modal(&self) -> bool {
        self.extra_tools.busy()
            || self.toolbar_customizer
            || self.fill_tool.busy()
            || self.text_dialog.is_some()
            || self.stamps.is_importing()
            || self.resize.is_some()
            || self.canvas_size.is_some()
            || self.rename_layer.is_some()
            || self.presets.dialog.is_some()
    }

    pub fn cancel_image_tools(&mut self) {
        self.canvas_resize = None;
        self.crop = None;
        self.resize = None;
        self.canvas_size = None;
        self.image_error = None;
        if self.tool == Tool::Crop {
            self.tool = Tool::Select;
        }
    }

    fn finish_annotation_gesture(&mut self, document: &mut Document, history: &mut EditHistory) {
        self.snap_guides.clear();
        let Some(gesture) = self.gesture.take() else {
            return;
        };
        if let GestureKind::Draw(index) = gesture.kind
            && document
                .annotations
                .get(index)
                .is_some_and(|a| !has_extent(a))
        {
            document.annotations = gesture.before.annotations;
            document.geometry = gesture.before.geometry;
            self.clear_selection();
            return;
        }
        let callout = match gesture.kind {
            GestureKind::Draw(index)
                if document.annotations[index].kind == AnnotationKind::Callout =>
            {
                Some(index)
            }
            _ => None,
        };
        let indices = gesture.changed_objects(document);
        if let Err(error) = crate::canvas::extend_for_annotations(document, &indices) {
            document.annotations = gesture.before.annotations;
            document.geometry = gesture.before.geometry;
            self.image_error = Some(error);
            return;
        }
        history.commit(gesture.before, document);
        if let Some(index) = callout {
            self.tool = Tool::Select;
            self.select(Some(index), document);
            self.focus_text = true;
        }
    }

    fn remember_appearance(&mut self, kind: AnnotationKind) {
        match kind {
            AnnotationKind::Step => self.step_color = self.color,
            AnnotationKind::Callout => {
                self.callout_color = self.color;
                self.callout_text.clone_from(&self.text);
            }
            AnnotationKind::Highlighter => self.highlighter_color = self.color,
            AnnotationKind::Freehand => {
                self.pen_color = self.color;
                self.pen_width = self.stroke_width;
            }
            AnnotationKind::Blur | AnnotationKind::Pixelate => {}
            _ => {
                self.shape_color = self.color;
                self.shape_width = self.stroke_width;
                if kind == AnnotationKind::Text {
                    self.plain_text.clone_from(&self.text);
                }
            }
        }
    }

    fn activate_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.color = match tool {
            Tool::Step => self.step_color,
            Tool::Callout => self.callout_color,
            Tool::Highlighter => self.highlighter_color,
            Tool::Pen => self.pen_color,
            Tool::Select | Tool::Area | Tool::Crop | Tool::Blur | Tool::Pixelate | Tool::Eraser => {
                self.color
            }
            _ => self.shape_color,
        };
        self.stroke_width = if tool == Tool::Pen {
            self.pen_width
        } else {
            self.shape_width
        };
        match tool {
            Tool::Callout => self.text.clone_from(&self.callout_text),
            Tool::Text => self.text.clone_from(&self.plain_text),
            _ => {}
        }
        if matches!(tool, Tool::Text | Tool::Callout) {
            self.text_editor_generation = self.text_editor_generation.wrapping_add(1);
        }
        self.image_error = None;
    }

    fn stamp_step(&mut self, at: Point, document: &mut Document, history: &mut EditHistory) {
        let Some(number) = next_step_number(document) else {
            self.image_error = Some(format!(
                "Step numbers stop at {}. Lower the highest marker number before adding another.",
                crate::document::MAX_STEP_NUMBER
            ));
            return;
        };
        let before = document.edit_state();
        let mut annotation = self.new_annotation(AnnotationKind::Step, at);
        let bounds = step_bounds(at, self.step_diameter);
        annotation.start = from_pos(bounds.min);
        annotation.end = from_pos(bounds.max);
        annotation.step_number = number;
        document.annotations.push(annotation);
        if !self.commit_object_placement(before, document, history) {
            return;
        }
        self.select(Some(document.annotations.len() - 1), document);
        self.image_error = None;
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
        texture: &egui::TextureHandle,
        preview: &mut crate::privacy_preview::PrivacyPreview,
    ) {
        self.property_interacting = ui.input(|i| i.pointer.primary_down());
        if !self.property_interacting {
            self.finish_properties(document, history);
        }
        if self.tool == Tool::Select {
            self.tool = Tool::Area;
        }
        if self.focus_text
            && self.text_dialog.is_none()
            && let Some(index) = self.selected
        {
            self.open_text_editor(index, document.edit_state(), document);
        }
        self.poll_extra_tools(ui.ctx(), document, history);
        self.poll_fill(ui.ctx(), document, history);
        let clipboard_was_busy = self.clipboard_job.is_some();
        self.poll_clipboard(document, history);
        if clipboard_was_busy && self.clipboard_job.is_none() {
            // The app toolbar precedes the canvas in this frame; repaint once
            // so the completed clipboard confirmation appears there immediately.
            ui.ctx().request_repaint();
        }
        let modal_open = self.has_modal();
        let text_input_focused = ui
            .ctx()
            .memory(|m| m.focused())
            .is_some_and(|id| egui::text_edit::TextEditState::load(ui.ctx(), id).is_some());
        if !modal_open && !text_input_focused && self.canvas_resize.is_none() {
            if self.gesture.is_none()
                && self.area_drag.is_none()
                && ui.is_enabled()
                && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::A))
            {
                self.select_all_image(document, history);
            }
            // Escape may surrender egui's canvas focus before canvas() runs.
            // Popups and active drags still own their own cancellation.
            if self.area.is_some()
                && self.gesture.is_none()
                && self.area_drag.is_none()
                && !egui::Popup::is_any_open(ui.ctx())
                && ui.is_enabled()
                && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
            {
                self.deselect_pixels();
            }
            self.clipboard_events(ui, document, history);
        }
        ui.add_enabled_ui(!modal_open, |ui| {
            self.workspace_ui(ui, document, history, texture, preview);
        });
        self.customize_toolbar(ui.ctx());
        self.resize_dialog(ui.ctx(), document, history);
        self.canvas_size_dialog(ui.ctx(), document, history);
        self.layer_rename_dialog(ui.ctx(), document, history);
        self.preset_dialog(ui.ctx());
        self.text_editor_window(ui.ctx(), document, history);
        self.stamp_designer_window(ui.ctx(), document, history);
    }

    fn workspace_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
        texture: &egui::TextureHandle,
        preview: &mut crate::privacy_preview::PrivacyPreview,
    ) {
        if self
            .selected
            .is_some_and(|i| i >= document.annotations.len())
        {
            self.clear_selection();
        }
        egui::SidePanel::right("annotation-inspector")
            .resizable(false)
            .exact_width(280.)
            .frame(
                egui::Frame::side_top_panel(ui.style())
                    .fill(Color32::WHITE)
                    .inner_margin(10),
            )
            .show_inside(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6., 5.);
                ui.spacing_mut().button_padding = Vec2::new(7., 4.);
                ui.spacing_mut().interact_size.y = 24.;
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().button_padding = Vec2::new(6., 6.);
                    ui.spacing_mut().item_spacing.x = 4.;
                    if ui
                        .selectable_label(!self.layers_open && !self.presets_open, "Styles")
                        .clicked()
                    {
                        self.layers_open = false;
                        self.presets_open = false;
                    }
                    if ui
                        .selectable_label(self.layers_open && !self.presets_open, "Layers")
                        .clicked()
                    {
                        self.layers_open = true;
                        self.presets_open = false;
                    }
                    if ui.selectable_label(self.presets_open, "Presets").clicked() {
                        self.cancel_image_tools();
                        self.presets_open = true;
                    }
                });
                ui.separator();
                if !(self.extra_panel_active(document)
                    || self.tool == Tool::Crop
                    || self.presets_open
                    || self.layers_open
                    || (self.tool == Tool::Area && self.selected.is_none()))
                {
                    self.shape_picker(ui, document, history);
                    self.quick_styles(ui, document, history);
                }
                if !self.layers_open
                    && !self.presets_open
                    && self.tool != Tool::Crop
                    && self.style_kind(document) == Some(AnnotationKind::Stamp)
                {
                    self.stamp_panel(ui, document, history);
                    return;
                }
                let footer = !self.layers_open
                    && !self.presets_open
                    && self.tool != Tool::Crop
                    && (self.selected.is_some() || self.tool.kind().is_some())
                    && !self
                        .style_kind(document)
                        .is_some_and(|k| k.is_area_edit() || k == AnnotationKind::Fill);
                // Let the footer claim its real height before laying out the
                // scroll area. A guessed reserve lets selection controls enlarge
                // the whole workspace, refitting the canvas under a held pointer.
                ui.allocate_ui_with_layout(
                    ui.available_size(),
                    egui::Layout::bottom_up(egui::Align::Min),
                    |ui| {
                        if footer {
                            self.selection_footer(ui, document, history);
                            ui.separator();
                        }
                        ui.allocate_ui_with_layout(
                            ui.available_size(),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                egui::ScrollArea::vertical()
                                    .id_salt("annotation-inspector-scroll")
                                    .max_height(ui.available_height().max(0.))
                                    .min_scrolled_height(0.)
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        if !self.layers_open
                                            && !self.presets_open
                                            && self.extra_panel_active(document)
                                        {
                                            self.extra_tools_panel(ui, document, history);
                                        } else if !self.layers_open
                                            && !self.presets_open
                                            && (self.tool == Tool::Fill
                                                || self.style_kind(document)
                                                    == Some(AnnotationKind::Fill))
                                        {
                                            self.fill_panel(ui, document, history);
                                        } else if self.tool == Tool::Area
                                            && self.additional.is_empty()
                                            && (self.selected.is_none()
                                                || self.selected.is_some_and(|i| {
                                                    document.annotations[i].kind.is_area_edit()
                                                }))
                                            && !self.layers_open
                                            && !self.presets_open
                                        {
                                            self.area_inspector(ui, document, history);
                                        } else if self.tool == Tool::Crop {
                                            self.crop_inspector(ui, document, history);
                                        } else if self.presets_open {
                                            self.presets_panel(ui, document, history);
                                        } else if self.layers_open {
                                            self.layers_panel(ui, document, history);
                                        } else if self.style_kind(document)
                                            == Some(AnnotationKind::Stamp)
                                        {
                                            self.stamp_panel(ui, document, history);
                                        } else {
                                            self.inspector(ui, document, history);
                                        }
                                    });
                            },
                        );
                    },
                );
            });
        let hint = match self.tool {
            Tool::Wand => "Click a color to select matching pixels • Adjust tolerance on the right",
            Tool::CutOut => "Drag across a strip • Choose horizontal or vertical • Apply cut out",
            Tool::Magnify => "Drag a lens • Adjust zoom, shape and border on the right",
            Tool::Spotlight => "Drag over the focus area • Dim or blur the surroundings",
            Tool::Simplify => {
                "Cover details with editable blocks • Auto simplify finds text-like rows"
            }
            Tool::Area => {
                "Drag inside to move the boundary • Drag edges or corners to resize • Move pixels is an explicit option"
            }
            Tool::Select => "Click annotations to edit • Shift-click to select more",
            Tool::Crop => {
                "Drag a crop area • Apply to keep it • Original pixels stay in your project"
            }
            Tool::Fill => {
                "Click a background to fill • Solid edges stop the fill • Select an area first to limit it"
            }
            Tool::Stamp => "Choose a stamp on the right • Click to place • Drag corners to resize",
            Tool::Text => "Click to write text • Double-click existing text to edit",
            Tool::Arrow => {
                "Drag to draw • Click an arrow to select • Drag its center handle to bend"
            }
            Tool::Pen => "Draw with the pen • Select a stroke later to change color or thickness",
            Tool::Eraser => "Drag to erase parts of pen strokes • Undo restores the whole gesture",
            Tool::Rectangle => "Drag to draw a rectangle",
            Tool::Ellipse => "Drag to draw an ellipse",
            Tool::Shape(_) => "Choose a shape in the right panel, then drag to draw",
            Tool::Blur => "Drag to blur an area • Select it later to adjust strength or size",
            Tool::Pixelate => "Drag to pixelate an area • Select it later to adjust block size",
            Tool::Step => {
                "Click to add numbered steps • Select a marker to adjust its number or size"
            }
            Tool::Callout => "Drag a bubble • Edit its text • Drag the pointer handle to aim it",
            Tool::Highlighter => {
                "Drag to highlight an area • Adjust color and opacity in the inspector"
            }
        };
        let hint = if self.draws_objects() {
            format!(
                "{hint} • Click/drag existing objects to edit • Hold Alt/Option to draw over them"
            )
        } else {
            hint.to_owned()
        };
        egui::TopBottomPanel::bottom("editor-canvas-status")
            .exact_height(24.)
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(4, 2)))
            .show_inside(ui, |ui| {
                ui.spacing_mut().button_padding = Vec2::new(4., 1.);
                ui.spacing_mut().interact_size.y = 18.;
                ui.horizontal(|ui| {
                    ui.menu_button("Image actions", |ui| {
                        self.canvas_menu(ui, document, history)
                    })
                    .response
                    .on_hover_text("Right-click the image for these actions");
                    let [width, height] = document.output_size();
                    ui.small(format!("{width} × {height} px · {:.0}%", self.zoom * 100.))
                        .on_hover_text(hint);
                    ui.separator();
                    ui.small(self.copy_scope(document))
                        .on_hover_text("Copy includes the selected pixels and visible edits. Select all: Command/Ctrl+A. Deselect: Escape.");
                    if let Some(error) = &self.image_error {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(error)
                                    .small()
                                    .color(Color32::from_rgb(170, 35, 45)),
                            )
                            .truncate(),
                        )
                        .on_hover_text(error);
                    } else if let Some(status) = &self.clipboard_status {
                        ui.add(egui::Label::new(egui::RichText::new(status).small()).truncate())
                            .on_hover_text(status);
                    }
                });
            });

        egui::Frame::new()
            .fill(Color32::from_rgb(36, 41, 48))
            .inner_margin(12)
            .corner_radius(8)
            .show(ui, |ui| {
                self.canvas(ui, document, history, texture, preview)
            });
    }

    pub fn image_menu(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if ui
            .button("Canvas size…")
            .on_hover_text("Add transparent canvas space around the image")
            .clicked()
        {
            self.open_canvas_size(document, history);
            ui.close();
        }
        ui.set_min_width(190.);
        ui.spacing_mut().button_padding = Vec2::new(8., 4.);
        ui.spacing_mut().item_spacing.y = 3.;
        ui.spacing_mut().interact_size.y = 24.;
        if ui
            .selectable_label(self.tool == Tool::Crop, "Crop")
            .on_hover_text("Trim the canvas without losing original pixels")
            .clicked()
        {
            ui.close();
            self.finish_gesture(document, history);
            self.layers_open = false;
            self.presets_open = false;
            self.tool = Tool::Crop;
            self.clear_selection();
            self.crop = Some(CropDraft {
                rect: document.geometry.crop_rect(original_size(document)),
                drag_start: None,
            });
        }
        if ui
            .button("Resize image…")
            .on_hover_text("Set the output pixel dimensions")
            .clicked()
        {
            ui.close();
            self.finish_gesture(document, history);
            let size = document.output_size();
            self.resize = Some(ResizeDraft {
                size,
                ratio: size[0] as f64 / size[1] as f64,
                locked: true,
            });
        }
        if ui.button("Rotate left").clicked() {
            ui.close();
            self.rotate(document, history, false);
        }
        if ui.button("Rotate right").clicked() {
            ui.close();
            self.rotate(document, history, true);
        }
        if ui.button("Rotate 180°").clicked() {
            self.finish_gesture(document, history);
            let mut geometry = document.geometry;
            geometry.quarter_turns = (geometry.quarter_turns + 2) % 4;
            self.apply_geometry(document, history, geometry);
            ui.close();
        }
        ui.separator();
        if ui.button("Flip horizontal").clicked() {
            self.flip(document, history, true);
            ui.close();
        }
        if ui.button("Flip vertical").clicked() {
            self.flip(document, history, false);
            ui.close();
        }
        ui.separator();
        if ui.add_enabled(document.geometry != ImageGeometry::default(), egui::Button::new("Reset image edits"))
                .on_hover_text("Restore the original image dimensions, crop, and orientation; annotations are kept").clicked()
            {
                self.finish_gesture(document, history);
                self.apply_geometry(document, history, ImageGeometry::default());
                ui.close();
            }
    }

    fn canvas_menu(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if self.area.is_some() || self.selected.is_some() {
            let pixels = self.area.is_some();
            let enabled = self.selected.is_none() || self.can_edit_selection(document);
            let label = if pixels {
                "Delete selected area"
            } else {
                "Delete selected layers"
            };
            if crate::appearance::action_button(
                ui,
                label,
                crate::appearance::Icon::Trash,
                enabled,
                false,
            )
            .on_hover_text(if pixels {
                "Clear these pixels to transparency as an editable layer (Delete / Backspace)"
            } else {
                "Remove selected annotations or groups; keep underlying layers (Delete / Backspace)"
            })
            .clicked()
            {
                self.delete_current_selection(document, history);
                ui.close();
            }
            ui.separator();
        }
        self.image_menu(ui, document, history);
        ui.separator();
        self.edit_actions(ui, document, history);
        self.pixel_selection_commands(ui, document, history);
        if ui
            .add_enabled(
                !document.annotations.is_empty(),
                egui::Button::new("Select all annotations"),
            )
            .clicked()
        {
            self.finish_gesture(document, history);
            self.tool = Tool::Select;
            self.layers_open = false;
            self.presets_open = false;
            self.set_selection((0..document.annotations.len()).collect(), document);
            self.focus_canvas = true;
            ui.close();
        }
        ui.separator();
        if ui.button("Fit image").clicked() {
            self.fit = true;
            ui.close();
        }
        if ui.button("Actual size (100%)").clicked() {
            self.fit = false;
            self.zoom = 1.;
            ui.close();
        }
        ui.horizontal(|ui| {
            ui.label("Zoom");
            if crate::appearance::zoom_slider(ui, &mut self.zoom, 0.05..=8.).changed() {
                self.fit = false;
            }
            ui.small(format!("{:.0}%", self.zoom * 100.));
        });
    }

    fn edit_actions(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if ui
            .add_enabled(history.can_undo(), egui::Button::new("Undo"))
            .on_hover_text("Undo the last change (Command/Ctrl + Z)")
            .clicked()
        {
            self.finish_gesture(document, history);
            history.undo(document);
            self.clear_selection();
            self.area = None;
            ui.close();
        }
        if ui
            .add_enabled(history.can_redo(), egui::Button::new("Redo"))
            .on_hover_text("Redo (Command/Ctrl + Shift + Z)")
            .clicked()
        {
            self.finish_gesture(document, history);
            history.redo(document);
            self.clear_selection();
            self.area = None;
            ui.close();
        }
        ui.separator();
        self.clipboard_buttons(ui, document, history);
    }

    fn apply_geometry(
        &mut self,
        document: &mut Document,
        history: &mut EditHistory,
        geometry: ImageGeometry,
    ) -> bool {
        if let Err(error) = geometry.validate(original_size(document)) {
            self.image_error = Some(error);
            return false;
        }
        let before = document.edit_state();
        document.geometry = geometry;
        history.commit(before, document);
        self.fit = true;
        self.image_error = None;
        true
    }

    fn rotate(&mut self, document: &mut Document, history: &mut EditHistory, clockwise: bool) {
        self.finish_gesture(document, history);
        let mut geometry = document.geometry;
        geometry.quarter_turns = (geometry.quarter_turns + if clockwise { 1 } else { 3 }) % 4;
        std::mem::swap(&mut geometry.flip_horizontal, &mut geometry.flip_vertical);
        if let Some(size) = &mut geometry.resize {
            size.swap(0, 1);
        }
        self.apply_geometry(document, history, geometry);
    }

    fn flip(&mut self, document: &mut Document, history: &mut EditHistory, horizontal: bool) {
        self.finish_gesture(document, history);
        let mut geometry = document.geometry;
        if horizontal {
            geometry.flip_horizontal = !geometry.flip_horizontal;
        } else {
            geometry.flip_vertical = !geometry.flip_vertical;
        }
        self.apply_geometry(document, history, geometry);
    }

    fn apply_crop(&mut self, document: &mut Document, history: &mut EditHistory) {
        let Some(crop) = &self.crop else { return };
        let geometry = with_crop(document, Some(crop.rect));
        if self.apply_geometry(document, history, geometry) {
            self.crop = None;
            self.tool = Tool::Select;
        }
    }

    fn crop_inspector(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        ui.heading("Crop image");
        ui.add_space(8.0);
        ui.label("Drag an area on the image, or enter an exact crop below.");
        ui.add_space(12.0);
        let original = original_size(document);
        let Some(crop) = &mut self.crop else { return };
        ui.weak("Original image pixels");
        egui::Grid::new("crop-dimensions")
            .spacing(Vec2::new(12.0, 8.0))
            .show(ui, |ui| {
                ui.label("Left");
                ui.add(
                    egui::DragValue::new(&mut crop.rect.x)
                        .range(0..=original[0] - 1)
                        .speed(1),
                );
                ui.end_row();
                ui.label("Top");
                ui.add(
                    egui::DragValue::new(&mut crop.rect.y)
                        .range(0..=original[1] - 1)
                        .speed(1),
                );
                ui.end_row();
                crop.rect.width = crop.rect.width.min(original[0] - crop.rect.x).max(1);
                crop.rect.height = crop.rect.height.min(original[1] - crop.rect.y).max(1);
                ui.label("Width");
                ui.add(
                    egui::DragValue::new(&mut crop.rect.width)
                        .range(1..=original[0] - crop.rect.x)
                        .speed(1),
                );
                ui.end_row();
                ui.label("Height");
                ui.add(
                    egui::DragValue::new(&mut crop.rect.height)
                        .range(1..=original[1] - crop.rect.y)
                        .speed(1),
                );
                ui.end_row();
            });
        ui.add_space(14.0);
        let preview = with_crop(document, Some(crop.rect)).output_size(original);
        ui.strong(format!("{} × {} px output", preview[0], preview[1]));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button("Apply crop").clicked() {
                self.apply_crop(document, history);
            }
            if ui.button("Cancel").clicked() {
                self.cancel_image_tools();
            }
        });
        ui.add_space(8.0);
        if ui
            .add_enabled(
                document.geometry.crop.is_some(),
                egui::Button::new("Restore full image"),
            )
            .on_hover_text("Remove the crop while keeping rotation and output scale")
            .clicked()
        {
            let geometry = with_crop(document, None);
            if self.apply_geometry(document, history, geometry) {
                self.cancel_image_tools();
            }
        }
        ui.add_space(14.0);
        ui.weak("Your original image and all annotations remain in the project, including content outside the crop.");
    }

    fn resize_dialog(
        &mut self,
        ctx: &egui::Context,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let Some(draft) = &mut self.resize else {
            return;
        };
        let mut apply = false;
        let mut cancel = false;
        let mut candidate = document.geometry;
        let original = original_size(document);
        let modal = Dialog::new("Resize image", "Scale the image and its editable annotations.", DialogIcon::Resize, 420.).modal(ctx, egui::Id::new("resize-image"), |ui| {
            ui.label(format!("Original: {} × {} px", original[0], original[1]));
            ui.label(format!("Current output: {} × {} px", document.output_size()[0], document.output_size()[1]));
            ui.add_space(16.0);
            egui::Grid::new("resize-dimensions").spacing(Vec2::new(16.0, 12.0)).show(ui, |ui| {
                ui.label("Width");
                if ui.add(egui::DragValue::new(&mut draft.size[0]).range(1..=32_768).speed(1).suffix(" px")).changed() && draft.locked {
                    draft.constrain_axis(0);
                }
                ui.end_row();
                ui.label("Height");
                if ui.add(egui::DragValue::new(&mut draft.size[1]).range(1..=32_768).speed(1).suffix(" px")).changed() && draft.locked {
                    draft.constrain_axis(1);
                }
                ui.end_row();
            });
            ui.add_space(8.0);
            if ui.checkbox(&mut draft.locked, "Lock aspect ratio").changed() && draft.locked {
                draft.ratio = draft.size[0] as f64 / draft.size[1] as f64;
            }
            if ui.small_button("Use unscaled size").clicked() {
                draft.size = document.geometry.rotated_size(original);
                draft.ratio = draft.size[0] as f64 / draft.size[1] as f64;
            }
            candidate.resize = (draft.size != candidate.rotated_size(original)).then_some(draft.size);
            let validation = candidate.validate(original);
            ui.add_space(14.0);
            ui.label("The original pixels stay in your project. Annotations scale with the image and remain editable.");
            if let Err(error) = &validation {
                ui.add_space(8.0);
                dialogs::notice(ui, DialogIcon::Info, "Check dimensions", error, true);
            }
            ui.add_space(16.0);
            dialogs::actions(ui, |ui| {
                cancel = dialogs::secondary(ui, "Cancel", DialogIcon::Close).clicked();
                apply = dialogs::button(ui, "Apply resize", DialogIcon::Resize, dialogs::Action::Primary, validation.is_ok()).clicked();
            });
        });
        if apply && self.apply_geometry(document, history, candidate) {
            self.resize = None;
        } else if cancel || modal.should_close() {
            self.resize = None;
            self.image_error = None;
        }
    }

    fn inspector(&mut self, ui: &mut egui::Ui, document: &mut Document, history: &mut EditHistory) {
        if self
            .selected
            .and_then(|i| document.annotations.get(i))
            .is_some_and(|a| a.kind.is_area_edit())
        {
            self.area_inspector(ui, document, history);
            return;
        }
        let selected_count = self.selected_indices(document).len();
        if selected_count > 0 && !self.can_edit_selection(document) {
            ui.strong("Selection is locked");
            ui.label("Unlock it in Layers to change its appearance or position.");
        } else if selected_count > 1 {
            ui.strong(format!("{selected_count} annotations selected"));
            ui.small("Drag to move together. Drag a corner to resize.");
        } else if self.tool == Tool::Eraser && self.selected.is_none() {
            ui.strong("Eraser");
            object_properties::slider(
                ui,
                &mut self.eraser_diameter,
                1.0..=512.0,
                "Diameter",
                " px",
            );
            ui.small("Erase pen strokes. Original pixels stay intact.");
        } else if let Some(kind) = self.style_kind(document) {
            ui.horizontal(|ui| {
                ui.strong(format!("{} properties", quick_styles::kind_name(kind)));
                if self.selected.is_some() {
                    ui.weak("Selected");
                }
            });
            ui.add_space(4.);
            if self.object_controls(ui, kind) {
                self.apply_inspector(document, history);
            }
            if matches!(kind, AnnotationKind::Text | AnnotationKind::Callout) {
                if let Some(index) = self.selected {
                    if ui.button("Edit text…").clicked() {
                        self.finish_properties(document, history);
                        self.open_text_editor(index, document.edit_state(), document);
                    }
                } else {
                    ui.small("Click the image to write text.");
                }
            }
        } else {
            ui.strong("Select an annotation");
            ui.small("Click artwork to adjust it here or with the floating controls.");
        }
    }

    fn apply_inspector(&mut self, document: &mut Document, history: &mut EditHistory) {
        if self.selected.is_some() && !self.can_edit_selection(document) {
            return;
        }
        if self.selected_indices(document).len() > 1 {
            return;
        }
        let kind = self
            .selected
            .and_then(|index| document.annotations.get(index))
            .map(|a| a.kind)
            .or(self.tool.kind());
        if let Some(kind) = kind {
            self.remember_appearance(kind);
        }
        let Some(index) = self.selected else { return };
        let before = document.edit_state();
        if let Some(annotation) = document.annotations.get_mut(index) {
            if annotation.kind == AnnotationKind::Arrow {
                annotation.arrow = self.arrow.clone();
            }
            if annotation.kind.is_studio() {
                annotation.studio = self.studio.clone();
            }
            if matches!(
                annotation.kind,
                AnnotationKind::Text | AnnotationKind::Callout
            ) {
                annotation.text_style = self.text_style.clone();
            }
            if annotation.kind.is_effect() {
                annotation.effect_size = self.effect_size;
            } else {
                annotation.color = self.color;
                match annotation.kind {
                    AnnotationKind::Step => {
                        annotation.step_number = self.step_number;
                        annotation.text_color = self.text_color;
                        let current = interaction_bounds(annotation);
                        if self.step_diameter != current.width() {
                            let bounds =
                                step_bounds(from_pos(current.center()), self.step_diameter);
                            annotation.start = from_pos(bounds.min);
                            annotation.end = from_pos(bounds.max);
                            self.step_diameter = bounds.width();
                        }
                    }
                    AnnotationKind::Callout => {
                        annotation.text.clone_from(&self.text);
                        annotation.font_size = self.font_size;
                        annotation.text_color = self.text_color;
                    }
                    AnnotationKind::Text => {
                        annotation.text.clone_from(&self.text);
                        annotation.font_size = self.font_size;
                    }
                    AnnotationKind::Highlighter => {}
                    _ => {
                        annotation.stroke_width = self.stroke_width;
                        annotation.fill = self.fill;
                    }
                }
            }
        }
        if let Some(a) = document.annotations.get(index) {
            self.sync_fill_selection(a);
        }
        self.record_properties(before, document, history);
    }

    fn canvas(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
        texture: &egui::TextureHandle,
        preview: &mut crate::privacy_preview::PrivacyPreview,
    ) {
        let viewport = ui.available_size().max(Vec2::splat(1.0));
        let [width, height] = document.output_size();
        let image_size = Vec2::new(width as f32, height as f32);
        if self.fit {
            self.zoom = canvas_handles::fit_scale(viewport, image_size);
        }
        let pinch_offset = crate::canvas_zoom::prepare_with_gutter(
            ui,
            "annotation-canvas-scroll",
            &mut self.zoom,
            &mut self.fit,
            true,
            self.gesture.is_none() && self.area_drag.is_none() && self.canvas_resize.is_none(),
            canvas_handles::GUTTER,
        );
        let scale = self.zoom;
        let image_size = image_size * scale;
        let mut scroll = egui::ScrollArea::both().id_salt("annotation-canvas-scroll");
        if let Some(offset) = pinch_offset {
            scroll = scroll.scroll_offset(offset);
        }
        scroll
            .auto_shrink([false, false])
            .scroll_source(egui::scroll_area::ScrollSource {
                drag: false,
                ..Default::default()
            })
            .show_viewport(ui, |ui, visible| {
                let (space, _) = ui.allocate_exact_size(
                    visible
                        .size()
                        .max(image_size + Vec2::splat(canvas_handles::GUTTER * 2.)),
                    Sense::hover(),
                );
                let rect =
                    Rect::from_min_size(space.min + (space.size() - image_size) * 0.5, image_size);
                crate::canvas_zoom::remember(
                    ui,
                    "annotation-canvas-scroll",
                    rect,
                    Rect::from_min_size(space.min + visible.min.to_vec2(), visible.size())
                        .intersect(ui.clip_rect()),
                );
                let (canvas_resizing, painted_resize) =
                    self.canvas_resize_ui(ui, document, history, rect, scale, viewport);
                if painted_resize {
                    return;
                }
                let stamp_payload =
                    egui::DragAndDrop::has_payload_of_type::<crate::stamps::StampSource>(ui.ctx());
                let draw_in_margin = self.draws_objects() || stamp_payload;
                let hit_rect = if draw_in_margin {
                    space
                } else if self.selected.is_some()
                    || (self.tool == Tool::Area && self.area_bounds(document).is_some())
                {
                    rect.expand(8.)
                } else {
                    rect
                };
                // The margin owns focus independently: using the image response
                // for an outside click loses focus again on release.
                let margin_response = ui.interact(
                    space,
                    ui.id().with("annotation-canvas-margin"),
                    Sense::click(),
                );
                let response = ui.interact(
                    hit_rect,
                    ui.id().with("annotation-canvas"),
                    Sense::click_and_drag(),
                );
                if self.focus_canvas {
                    response.request_focus();
                    self.focus_canvas = false;
                }
                // Read the original press event: egui clears press_origin on
                // release, so a quick press/move/release in one frame would
                // otherwise never begin a gesture. Clip and layer checks keep
                // inspector controls, scrollbars, and popups from drawing.
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
                if (ui.is_enabled() && !canvas_resizing)
                    && self.selected.is_some()
                    && let Some(position) = press
                    && space.intersect(ui.clip_rect()).contains(position)
                    && !hit_rect.contains(position)
                    && ui.ctx().layer_id_at(position) == Some(ui.layer_id())
                {
                    margin_response.request_focus();
                    self.finish_annotation_gesture(document, history);
                    self.clear_selection();
                    self.object_press = false;
                }
                if (ui.is_enabled() && !canvas_resizing)
                    && !matches!(
                        self.tool,
                        Tool::Fill | Tool::Wand | Tool::Pen | Tool::Eraser
                    )
                    && let Some(position) = press
                    && hit_rect.intersect(ui.clip_rect()).contains(position)
                    && (draw_in_margin
                        || rect.contains(position)
                        || self.object_target(
                            object_point(position, rect, scale, document),
                            scale,
                            document,
                        )
                        || self.over_area_handle(
                            image_point(position, rect, scale, document),
                            document,
                        ))
                    && ui.ctx().layer_id_at(position) == Some(ui.layer_id())
                {
                    response.request_focus();
                    self.finish_annotation_gesture(document, history);
                    let point = if self.draws_objects()
                        || self.tool == Tool::Select
                        || (self.tool == Tool::Area && self.area.is_none())
                    {
                        object_point(position, rect, scale, document)
                    } else {
                        image_point(position, rect, scale, document)
                    };
                    if self.tool == Tool::Crop {
                        if let Some(crop) = &mut self.crop {
                            crop.drag_start = Some(point);
                            crop.rect = crop_between(point, point, document);
                        }
                    } else if matches!(self.tool, Tool::Area | Tool::CutOut) {
                        let modifiers = ui.input(|i| i.modifiers);
                        if self.area.is_none()
                            && (modifiers.shift
                                || self.selected.is_some_and(|i| {
                                    (!document.annotations[i].kind.needs_composite()
                                        || document.annotations[i].kind.is_studio()
                                        || !self.additional.is_empty())
                                        && (hit_document_handle(
                                            &document.annotations[i],
                                            point,
                                            scale,
                                            document,
                                        )
                                        .is_some()
                                            || hit_document_annotation(document, point, scale)
                                                .is_some_and(|hit| {
                                                    self.selected_indices(document).contains(&hit)
                                                }))
                                }))
                        {
                            self.begin_selection_gesture(
                                point,
                                scale,
                                document,
                                modifiers.shift,
                                modifiers.alt,
                            );
                        } else {
                            self.begin_area_drag(point, document);
                        }
                    } else if self.tool == Tool::Select {
                        let modifiers = ui.input(|i| i.modifiers);
                        self.begin_selection_gesture(
                            point,
                            scale,
                            document,
                            modifiers.shift,
                            modifiers.alt,
                        );
                    } else {
                        let m = ui.input(|i| i.modifiers);
                        self.begin_tool_gesture(point, scale, document, m.shift, m.alt);
                    }
                }

                // Keep every pointer sample, including a complete quick scribble
                // in one frame. Reading only interact_pos would discard corners.
                if (ui.is_enabled() && !canvas_resizing)
                    && matches!(self.tool, Tool::Pen | Tool::Eraser)
                {
                    let events = ui.input(|i| i.events.clone());
                    for event in events {
                        match event {
                            egui::Event::PointerButton {
                                pos,
                                button: PointerButton::Primary,
                                pressed: true,
                                ..
                            } if hit_rect.intersect(ui.clip_rect()).contains(pos)
                                && ui.ctx().layer_id_at(pos) == Some(ui.layer_id()) =>
                            {
                                response.request_focus();
                                self.finish_annotation_gesture(document, history);
                                let m = ui.input(|i| i.modifiers);
                                self.begin_tool_gesture(
                                    object_point(pos, rect, scale, document),
                                    scale,
                                    document,
                                    m.shift,
                                    m.alt,
                                );
                            }
                            egui::Event::PointerMoved(pos) if self.gesture.is_some() => {
                                self.update_gesture(
                                    object_point(pos, rect, scale, document),
                                    document,
                                );
                            }
                            egui::Event::PointerButton {
                                pos,
                                button: PointerButton::Primary,
                                pressed: false,
                                ..
                            } if self.gesture.is_some() => {
                                self.update_gesture(
                                    object_point(pos, rect, scale, document),
                                    document,
                                );
                                self.finish_annotation_gesture(document, history);
                                self.reconcile_selection(document);
                            }
                            _ => {}
                        }
                    }
                }

                let mut stamp_dropped = false;
                if (ui.is_enabled() && !canvas_resizing)
                    && let Some(position) = ui.input(|i| i.pointer.interact_pos())
                    && hit_rect.intersect(ui.clip_rect()).contains(position)
                    && let Some(source) =
                        response.dnd_release_payload::<crate::stamps::StampSource>()
                {
                    self.stamps.source = (*source).clone();
                    self.place_stamp(
                        object_point(position, rect, scale, document),
                        document,
                        history,
                    );
                    response.request_focus();
                    stamp_dropped = true;
                }

                if !stamp_dropped && response.clicked_by(PointerButton::Primary) {
                    response.request_focus();
                    if let Some(position) = response.interact_pointer_pos() {
                        let point = object_point(position, rect, scale, document);
                        if self.tool == Tool::Text && !self.object_press {
                            let before = document.edit_state();
                            let mut annotation = self.new_annotation(AnnotationKind::Text, point);
                            if annotation.text.is_empty() {
                                annotation.text = "Text".into();
                            }
                            document.annotations.push(annotation);
                            self.tool = Tool::Area;
                            self.open_text_editor(document.annotations.len() - 1, before, document);
                        } else if self.tool == Tool::Wand {
                            self.place_wand(ui.ctx(), point, document);
                        } else if self.tool == Tool::Step && !self.object_press {
                            self.stamp_step(point, document, history);
                        } else if self.tool == Tool::Fill {
                            self.place_fill(ui.ctx(), point, document);
                        } else if self.tool == Tool::Stamp && !self.object_press {
                            self.place_stamp(point, document, history);
                        } else if response.double_clicked()
                            && self.selected_indices(document).len() == 1
                            && let Some(index) = self.selected
                        {
                            self.open_text_editor(index, document.edit_state(), document);
                        }
                    }
                }
                if self.tool == Tool::Crop
                    && (ui.is_enabled() && !canvas_resizing)
                    && let Some(crop) = &mut self.crop
                {
                    if let Some(start) = crop.drag_start
                        && let Some(position) = ui.input(|i| i.pointer.interact_pos())
                    {
                        crop.rect = crop_between(
                            start,
                            image_point(position, rect, scale, document),
                            document,
                        );
                    }
                    if !ui.input(|i| i.pointer.button_down(PointerButton::Primary)) {
                        crop.drag_start = None;
                    }
                }
                if self.area_drag.is_some() && (ui.is_enabled() && !canvas_resizing) {
                    for position in ui.input(|i| {
                        i.events
                            .iter()
                            .filter_map(|event| match event {
                                egui::Event::PointerMoved(p)
                                | egui::Event::PointerButton {
                                    pos: p,
                                    button: PointerButton::Primary,
                                    pressed: false,
                                    ..
                                } => Some(*p),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                    }) {
                        self.update_area_drag(
                            self.area_drag_point(position, rect, scale, document),
                            document,
                        );
                    }
                    if let Some(position) = ui.input(|i| i.pointer.interact_pos()) {
                        self.update_area_drag(
                            self.area_drag_point(position, rect, scale, document),
                            document,
                        );
                    }
                    if !ui.input(|i| i.pointer.button_down(PointerButton::Primary)) {
                        self.finish_area_drag(document, history);
                    }
                }
                if self.gesture.is_some()
                    && !matches!(self.tool, Tool::Pen | Tool::Eraser)
                    && (ui.is_enabled() && !canvas_resizing)
                    && let Some(position) = ui.input(|i| i.pointer.interact_pos())
                {
                    let point = object_point(position, rect, scale, document);
                    self.update_snapped_gesture(point, document, ui.input(|i| i.modifiers.alt));
                }
                if self.gesture.is_some()
                    && !ui.input(|i| i.pointer.button_down(PointerButton::Primary))
                {
                    self.finish_annotation_gesture(document, history);
                    self.reconcile_selection(document);
                }
                // egui may surrender focus on Escape before this response is
                // inspected. A gesture still owns cancellation until release.
                if (response.has_focus()
                    || margin_response.has_focus()
                    || self.gesture.is_some()
                    || self.area_drag.is_some())
                    && (ui.is_enabled() && !canvas_resizing)
                {
                    self.keyboard(ui, document, history);
                }
                // A completed pixel/raster move can enlarge the canvas in this
                // frame. Paint with its new dimensions immediately, never squeeze
                // the new composition into the old canvas for one frame.
                let (rect, scale) = if document.output_size() != [width, height] {
                    let [w, h] = document.output_size();
                    let size = egui::vec2(w as f32, h as f32);
                    if self.fit {
                        self.zoom = canvas_handles::fit_scale(viewport, size);
                    }
                    let size = size * self.zoom;
                    ui.ctx().request_repaint();
                    (
                        Rect::from_min_size(
                            space.min
                                + (visible
                                    .size()
                                    .max(size + Vec2::splat(canvas_handles::GUTTER * 2.))
                                    - size)
                                    * 0.5,
                            size,
                        ),
                        self.zoom,
                    )
                } else {
                    (rect, scale)
                };
                // Only the presentation grows while dragging. The real geometry
                // and selection coordinates remain stable until release/cancel.
                // Invalid growth is rejected by finish_area_drag; keep the current
                // canvas visible in the meantime rather than show a blank frame.
                let object_expansion = self
                    .object_canvas_preview(document, rect, scale)
                    .ok()
                    .flatten();
                let expansion = self
                    .area_drag_preview(document, rect, scale)
                    .ok()
                    .flatten()
                    .or(object_expansion);
                let display_document = expansion.as_ref().map_or(&*document, |(doc, _)| doc);
                let live_fit = self.fit && expansion.is_some();
                let (paint_rect, display_scale) = if live_fit {
                    let [w, h] = display_document.output_size();
                    let size = egui::vec2(w as f32, h as f32);
                    let zoom = canvas_handles::fit_scale(viewport, size);
                    (Rect::from_center_size(rect.center(), size * zoom), zoom)
                } else {
                    (
                        expansion.as_ref().map_or(rect, |(_, bounds)| *bounds),
                        scale,
                    )
                };
                let stable_frame = self
                    .area_drag_texture_frame(document, rect, scale, ui.clip_rect())
                    .ok()
                    .flatten()
                    .or_else(|| {
                        self.object_canvas_texture_frame(document, rect, scale, ui.clip_rect())
                            .ok()
                            .flatten()
                    });
                let (paint_document, _) = stable_frame
                    .as_ref()
                    .or(expansion.as_ref())
                    .map_or((&*document, rect), |(doc, bounds)| (doc, *bounds));
                let mut presentation = paint_document.clone();
                if stable_frame.is_some() {
                    // Sample the fixed retained canvas, but display only the current
                    // visible window at its live-fit size. UVs keep their fixed grid.
                    presentation.geometry.canvas =
                        Some(paint_document.geometry.canvas_rect(original_size(document)));
                    presentation.geometry.viewport =
                        Some(display_document.geometry.view_rect(original_size(document)));
                    presentation.geometry.resize = display_document.geometry.resize;
                }
                let painter = ui
                    .painter()
                    .with_clip_rect(paint_rect.intersect(ui.clip_rect()));
                // Process edits before asking for a texture, so a newly added or
                // moved privacy region can never reveal the unprocessed image
                // for one frame while its replacement preview is being built.
                if paint_document.geometry.canvas.is_some()
                    || paint_document.geometry.has_cutouts()
                    || paint_document
                        .annotations
                        .iter()
                        .any(|annotation| annotation.visible() && annotation.kind.needs_composite())
                {
                    let image = if self.moving_area_pixels(document) || self.gesture.is_some() {
                        preview.drag_texture(ui.ctx(), paint_document)
                    } else {
                        preview.continuous_texture(ui.ctx(), paint_document)
                    };
                    match image {
                        Ok(Some(texture)) => {
                            paint_checkerboard_at(&painter, paint_rect, rect.min);
                            render::paint_document_image(
                                &painter,
                                texture.id(),
                                &presentation,
                                paint_rect,
                            );
                        }
                        result => {
                            painter.rect_filled(paint_rect, 0, Color32::from_rgb(36, 41, 48));
                            let status = match result {
                                Ok(None) => "Updating image preview…".to_owned(),
                                Err(error) => format!("Image preview unavailable: {error}"),
                                Ok(Some(_)) => unreachable!(),
                            };
                            painter.text(
                                paint_rect.intersect(ui.clip_rect()).center(),
                                egui::Align2::CENTER_CENTER,
                                status,
                                egui::FontId::proportional(14.0),
                                Color32::from_gray(220),
                            );
                        }
                    }
                } else {
                    paint_checkerboard_at(&painter, paint_rect, rect.min);
                    render::paint_document_image(&painter, texture.id(), &presentation, paint_rect);
                    render::paint_document_annotations(&painter, document, rect.min, scale);
                }
                if expansion.is_some() {
                    painter.rect_stroke(
                        paint_rect,
                        0,
                        Stroke::new(1., crate::appearance::ACCENT),
                        StrokeKind::Inside,
                    );
                }
                let selection_clip = if expansion.is_some() || self.moving_area_pixels(document) {
                    ui.clip_rect()
                } else {
                    rect.expand(7.).intersect(ui.clip_rect())
                };
                let selection_painter = ui.painter().with_clip_rect(selection_clip);
                if live_fit {
                    self.paint_area(
                        &selection_painter,
                        display_document,
                        paint_rect.min,
                        display_scale,
                    );
                } else {
                    self.paint_area(&selection_painter, document, rect.min, scale);
                }
                if let Some(crop) = &self.crop {
                    paint_crop(&painter, crop.rect, document, rect, scale);
                }
                if !(matches!(
                    self.tool,
                    Tool::Crop | Tool::Pen | Tool::Eraser | Tool::Fill
                ) || (self.tool == Tool::Area
                    && self.additional.is_empty()
                    && self.area_bounds(document).is_some()))
                    && self.can_edit_selection(document)
                    && self
                        .selected_indices(document)
                        .iter()
                        .all(|&i| document.annotations[i].visible())
                {
                    let indices = self.selected_indices(document);
                    if indices.len() > 1 {
                        paint_many(
                            &painter,
                            display_document,
                            &indices,
                            paint_rect.min,
                            display_scale,
                            ui.visuals().selection.bg_fill,
                        );
                    } else if let Some(annotation) =
                        self.selected.and_then(|i| document.annotations.get(i))
                    {
                        paint_selection(
                            &selection_painter,
                            annotation,
                            display_document,
                            paint_rect.min,
                            display_scale,
                        );
                    }
                }
                if let Some(Gesture {
                    kind: GestureKind::Marquee { current, .. },
                    origin,
                    ..
                }) = &self.gesture
                {
                    let bounds = mapped_bounds(
                        Rect::from_two_pos(point(*origin), point(*current)),
                        document,
                    );
                    let screen = Rect::from_min_max(
                        rect.min + bounds.min.to_vec2() * scale,
                        rect.min + bounds.max.to_vec2() * scale,
                    );
                    painter.rect(
                        screen,
                        0,
                        ui.visuals().selection.bg_fill.gamma_multiply(0.12),
                        Stroke::new(1.2, ui.visuals().selection.stroke.color),
                        StrokeKind::Inside,
                    );
                }
                if expansion.is_none() {
                    self.paint_snap_guides(&painter, rect, scale);
                }
                if (ui.is_enabled() && !canvas_resizing)
                    && let Some(source) = response.dnd_hover_payload::<crate::stamps::StampSource>()
                    && let Some(position) = ui.input(|i| i.pointer.interact_pos())
                    && let Ok(texture) = crate::stamps::texture(ui.ctx(), &source)
                {
                    let image = texture.size_vec2();
                    let size = image * (self.stamps.size / image.max_elem());
                    let center = object_point(position, rect, scale, document);
                    let bounds =
                        mapped_bounds(Rect::from_center_size(point(center), size), document);
                    let screen = Rect::from_min_max(
                        rect.min + bounds.min.to_vec2() * scale,
                        rect.min + bounds.max.to_vec2() * scale,
                    );
                    ui.painter().image(
                        texture.id(),
                        screen,
                        Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
                        Color32::from_white_alpha(170),
                    );
                    ui.painter().rect_stroke(
                        screen.expand(3.),
                        3.,
                        Stroke::new(1., crate::appearance::ACCENT),
                        StrokeKind::Inside,
                    );
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Copy);
                }

                if response.hovered() {
                    if self.tool == Tool::Eraser
                        && let Some(position) = response.hover_pos()
                    {
                        let source = image_point(position, rect, scale, document);
                        let bounds = mapped_bounds(
                            Rect::from_center_size(
                                point(source),
                                Vec2::splat(self.eraser_diameter),
                            ),
                            document,
                        );
                        let center = rect.min + bounds.center().to_vec2() * scale;
                        let radii = bounds.size() * scale * 0.5;
                        painter.add(egui::Shape::ellipse_stroke(
                            center,
                            radii,
                            Stroke::new(3.0, Color32::BLACK),
                        ));
                        painter.add(egui::Shape::ellipse_stroke(
                            center,
                            radii,
                            Stroke::new(1.5, Color32::WHITE),
                        ));
                    }
                    let cursor = response
                        .hover_pos()
                        .map_or(CursorIcon::Default, |position| {
                            self.canvas_cursor(
                                object_point(position, rect, scale, document),
                                scale,
                                document,
                            )
                        });
                    response.clone().on_hover_cursor(cursor);
                }
                if self.area_drag.is_some()
                    && let Some(position) = ui.input(|i| i.pointer.interact_pos())
                {
                    ui.ctx().set_cursor_icon(
                        self.area_cursor(image_point(position, rect, scale, document), document),
                    );
                }
                if let Some(cursor) = self.active_gesture_cursor(document) {
                    ui.ctx().set_cursor_icon(cursor);
                }
                if !canvas_resizing {
                    response.context_menu(|ui| self.canvas_menu(ui, document, history));
                    self.floating_properties(
                        ui,
                        document,
                        history,
                        rect.min,
                        scale,
                        ui.clip_rect(),
                    );
                }
                self.paint_canvas_handles(ui, document, paint_rect);
            });
    }

    fn active_gesture_cursor(&self, document: &Document) -> Option<CursorIcon> {
        self.gesture.as_ref().map(|g| match g.kind {
            GestureKind::Move(_) | GestureKind::MoveMany(_) => CursorIcon::Grabbing,
            GestureKind::Resize(_, handle) | GestureKind::ResizeMany(_, handle) => {
                handle_cursor(handle, document)
            }
            GestureKind::Draw(_) | GestureKind::Marquee { .. } | GestureKind::Erase { .. } => {
                CursorIcon::Crosshair
            }
        })
    }

    fn canvas_cursor(&self, at: Point, scale: f32, document: &Document) -> CursorIcon {
        if let Some(cursor) = self.active_gesture_cursor(document) {
            return cursor;
        }
        let selected = self.selected_indices(document);
        let handles = if self.can_edit_selection(document)
            && selected.iter().all(|&i| document.annotations[i].visible())
        {
            if selected.len() > 1 {
                selection_handle(document, &selected, at, scale)
            } else {
                self.selected
                    .and_then(|i| document.annotations.get(i))
                    .and_then(|a| hit_document_handle(a, at, scale, document))
            }
        } else {
            None
        };
        let hit = hit_document_annotation(document, at, scale);
        // Select uses both area and object gestures. Its cursor must follow the
        // same selected-object branch as pointer-down, including group handles.
        let selected_object = self.area.is_none()
            && self
                .selected
                .and_then(|i| document.annotations.get(i))
                .is_some_and(|a| {
                    (!a.kind.needs_composite() || a.kind.is_studio() || !self.additional.is_empty())
                        && (handles.is_some() || hit.is_some_and(|i| selected.contains(&i)))
                });
        if self.tool == Tool::Select
            || (self.tool == Tool::Area && selected_object)
            || (self.draws_objects() && self.object_target(at, scale, document))
        {
            if let Some(handle) = handles {
                return handle_cursor(handle, document);
            }
            return if hit.is_some_and(|i| annotation_layers::editable(document, &[i])) {
                CursorIcon::Grab
            } else {
                CursorIcon::Default
            };
        }
        if self.tool == Tool::Area {
            return self.area_cursor(at, document);
        }
        CursorIcon::Crosshair
    }

    fn new_annotation(&self, kind: AnnotationKind, point: Point) -> Annotation {
        Annotation {
            kind,
            start: point,
            end: point,
            color: self.color,
            stroke_width: self.stroke_width,
            fill: self.fill
                || matches!(
                    kind,
                    AnnotationKind::Step | AnnotationKind::Callout | AnnotationKind::Highlighter
                ),
            text: if kind == AnnotationKind::Freehand {
                String::new()
            } else if kind == AnnotationKind::Callout {
                self.callout_text.clone()
            } else {
                self.text.clone()
            },
            font_size: self.font_size,
            effect_size: self.effect_size,
            step_number: self.step_number,
            tail: (kind == AnnotationKind::Callout).then_some(point),
            text_color: self.text_color,
            group_id: None,
            layer: Default::default(),
            group_layer: Default::default(),
            area_source: None,
            area_mask: Vec::new(),
            stamp: None,
            fill_region: None,
            text_style: self.text_style.clone(),
            pixel_mask: None,
            arrow: if kind == AnnotationKind::Arrow {
                crate::arrow_style::ArrowStyle {
                    curve: None,
                    ..self.arrow.clone()
                }
            } else {
                Default::default()
            },
            studio: if kind.is_studio() {
                self.studio.clone()
            } else {
                Default::default()
            },
            points: if kind == AnnotationKind::Freehand {
                vec![point]
            } else {
                Vec::new()
            },
        }
    }

    #[cfg(test)]
    fn begin_gesture(&mut self, point: Point, scale: f32, document: &mut Document) {
        self.begin_tool_gesture(point, scale, document, false, false);
    }

    fn draws_objects(&self) -> bool {
        !matches!(
            self.tool,
            Tool::Select
                | Tool::Area
                | Tool::Crop
                | Tool::CutOut
                | Tool::Wand
                | Tool::Fill
                | Tool::Eraser
        )
    }

    fn object_target(&self, at: Point, scale: f32, document: &Document) -> bool {
        let indices = self.selected_indices(document);
        (self.can_edit_selection(document)
            && if indices.len() > 1 {
                selection_handle(document, &indices, at, scale).is_some()
            } else {
                self.selected
                    .and_then(|i| document.annotations.get(i))
                    .filter(|a| a.visible())
                    .and_then(|a| hit_document_handle(a, at, scale, document))
                    .is_some()
            })
            || hit_document_annotation(document, at, scale).is_some()
    }

    fn begin_tool_gesture(
        &mut self,
        point: Point,
        scale: f32,
        document: &mut Document,
        toggle: bool,
        force_draw: bool,
    ) {
        self.object_press =
            !force_draw && self.draws_objects() && self.object_target(point, scale, document);
        if self.object_press {
            self.begin_selection_gesture(point, scale, document, toggle, false);
            return;
        }

        if self.tool == Tool::Select {
            self.begin_selection_gesture(point, scale, document, false, false);
            return;
        }
        if self.tool == Tool::Pen
            && (document.annotations.len() >= crate::document::MAX_ANNOTATIONS
                || stroke_point_count(document) >= crate::document::MAX_TOTAL_STROKE_POINTS)
        {
            self.image_error = Some(
                "The project has reached its drawing limit. Remove a stroke before drawing more."
                    .into(),
            );
            return;
        }
        self.image_error = None;
        let before = document.edit_state();
        let kind = if self.tool == Tool::Eraser {
            self.clear_selection();
            Some(GestureKind::Erase { last: point })
        } else if let Some(kind) = self.tool.kind().filter(|k| {
            !matches!(
                k,
                AnnotationKind::Text | AnnotationKind::Step | AnnotationKind::Stamp
            )
        }) {
            let index = document.annotations.len();
            document.annotations.push(self.new_annotation(kind, point));
            self.clear_selection();
            Some(GestureKind::Draw(index))
        } else {
            None
        };
        self.gesture = kind.map(|kind| Gesture {
            kind,
            before,
            origin: point,
            preview_geometry: None,
        });
        if self.tool == Tool::Eraser {
            self.update_gesture(point, document);
        }
    }

    fn update_gesture(&mut self, point: Point, document: &mut Document) {
        let total_points = stroke_point_count(document);
        let Some(gesture) = &mut self.gesture else {
            return;
        };
        let editing = match &gesture.kind {
            GestureKind::Move(i) | GestureKind::Resize(i, _) => vec![*i],
            GestureKind::MoveMany(indices) | GestureKind::ResizeMany(indices, _) => indices.clone(),
            _ => Vec::new(),
        };
        if !editing.is_empty() && !annotation_layers::editable(document, &editing) {
            return;
        }
        match &mut gesture.kind {
            GestureKind::MoveMany(indices) => {
                let delta = Vec2::new(point.x - gesture.origin.x, point.y - gesture.origin.y);
                move_selection(document, &gesture.before.annotations, indices, delta);
                return;
            }
            GestureKind::ResizeMany(indices, handle) => {
                if let Err(error) =
                    resize_selection(document, &gesture.before, indices, *handle, point)
                {
                    self.image_error = Some(error);
                }
                return;
            }
            GestureKind::Marquee { current, initial } => {
                *current = point;
                let rect = Rect::from_two_pos(
                    Pos2::new(gesture.origin.x, gesture.origin.y),
                    Pos2::new(point.x, point.y),
                );
                let mut indices = initial.clone();
                if rect.width().max(rect.height()) > 1.0 {
                    indices.extend(
                        document
                            .annotations
                            .iter()
                            .enumerate()
                            .filter_map(|(i, a)| {
                                (a.visible()
                                    && !a.locked()
                                    && rect.contains_rect(render::annotation_bounds(a)))
                                .then_some(i)
                            }),
                    );
                }
                let indices = expand_groups(&indices, document);
                self.set_selection(indices, document);
                return;
            }
            _ => {}
        }
        match gesture.kind {
            GestureKind::MoveMany(_)
            | GestureKind::ResizeMany(_, _)
            | GestureKind::Marquee { .. } => unreachable!(),
            GestureKind::Draw(index) => {
                if let Some(annotation) = document.annotations.get_mut(index) {
                    if annotation.kind == AnnotationKind::Freehand {
                        let previous = annotation.points.last().copied().unwrap_or(point);
                        if ((previous.x - point.x).powi(2) + (previous.y - point.y).powi(2))
                            < 0.0625
                        {
                            return;
                        }
                        if annotation.points.len() >= crate::document::MAX_STROKE_POINTS
                            || total_points >= crate::document::MAX_TOTAL_STROKE_POINTS
                        {
                            self.image_error = Some("Drawing limit reached. This stroke is kept up to the limit; release the pointer to finish it.".into());
                            return;
                        }
                        annotation.points.push(point);
                        annotation.start.x = annotation.start.x.min(point.x);
                        annotation.start.y = annotation.start.y.min(point.y);
                        annotation.end.x = annotation.end.x.max(point.x);
                        annotation.end.y = annotation.end.y.max(point.y);
                        return;
                    }
                    annotation.end = point;
                    if annotation.kind == AnnotationKind::Callout {
                        annotation.tail = Some(default_callout_tail(
                            annotation,
                            Vec2::new(
                                document.image.width() as f32,
                                document.image.height() as f32,
                            ),
                        ));
                    }
                }
            }
            GestureKind::Move(index) => {
                if let (Some(original), Some(annotation)) = (
                    gesture.before.annotations.get(index),
                    document.annotations.get_mut(index),
                ) {
                    *annotation = offset_annotation(
                        original,
                        Vec2::new(point.x - gesture.origin.x, point.y - gesture.origin.y),
                    );
                }
            }
            GestureKind::Resize(index, handle) => {
                if let (Some(original), Some(annotation)) = (
                    gesture.before.annotations.get(index),
                    document.annotations.get_mut(index),
                ) {
                    // Clicking within a handle's hit target is selection, not
                    // an edit. Keep the grab offset during the subsequent drag.
                    let delta = Vec2::new(point.x - gesture.origin.x, point.y - gesture.origin.y);
                    *annotation = if delta == Vec2::ZERO {
                        original.clone()
                    } else {
                        let anchor = handles(original)
                            .into_iter()
                            .find(|(candidate, _)| *candidate == handle)
                            .map_or(Pos2::new(gesture.origin.x, gesture.origin.y), |(_, p)| p);
                        resized(original, handle, from_pos(anchor + delta))
                    };
                    if annotation.kind == AnnotationKind::Step {
                        let bounds = interaction_bounds(annotation);
                        let bounds = step_bounds(from_pos(bounds.center()), bounds.width());
                        annotation.start = from_pos(bounds.min);
                        annotation.end = from_pos(bounds.max);
                    }
                }
            }
            GestureKind::Erase { ref mut last } => {
                match freehand::erase(&document.annotations, *last, point, self.eraser_diameter) {
                    Ok(Some(annotations)) => document.annotations = annotations,
                    Ok(None) => {}
                    Err(message) => self.image_error = Some(message.into()),
                }
                *last = point;
            }
        }
    }

    fn keyboard(&mut self, ui: &mut egui::Ui, document: &mut Document, history: &mut EditHistory) {
        if matches!(self.tool, Tool::Area | Tool::Wand) && self.area_keyboard(ui, document, history)
        {
            return;
        }
        self.clipboard_events(ui, document, history);
        let redo_modifiers = Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        };
        if ui.input_mut(|i| {
            i.consume_key(redo_modifiers, Key::Z) || i.consume_key(Modifiers::COMMAND, Key::Y)
        }) {
            self.finish_gesture(document, history);
            history.redo(document);
            self.area = None;
            self.clear_selection();
        } else if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Z)) {
            self.finish_gesture(document, history);
            history.undo(document);
            self.area = None;
            self.clear_selection();
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            if let Some(gesture) = self.gesture.take() {
                document.annotations = gesture.before.annotations;
                document.geometry = gesture.before.geometry;
            }
            self.deselect_pixels();
            self.cancel_image_tools();
            self.tool = Tool::Select;
        }
        if self.tool == Tool::Crop {
            if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
                self.apply_crop(document, history);
            }
            return;
        }
        if self.gesture.is_some() {
            return;
        }
        if ui.input_mut(|i| i.consume_key(redo_modifiers, Key::G)) {
            self.ungroup_selection(document, history);
        } else if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::G)) {
            self.group_selection(document, history);
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::D)) {
            self.duplicate_selection(document, history);
        }
        if ui.input_mut(|i| {
            i.consume_key(Modifiers::NONE, Key::Delete)
                || i.consume_key(Modifiers::NONE, Key::Backspace)
        }) {
            self.delete(document, history);
        }
        let delta = ui.input_mut(|i| {
            let step = if i.modifiers.shift { 10.0 } else { 1.0 };
            let modifiers = if i.modifiers.shift {
                Modifiers::SHIFT
            } else {
                Modifiers::NONE
            };
            let mut delta = Vec2::ZERO;
            for (key, direction) in [
                (Key::ArrowLeft, Vec2::new(-1.0, 0.0)),
                (Key::ArrowRight, Vec2::new(1.0, 0.0)),
                (Key::ArrowUp, Vec2::new(0.0, -1.0)),
                (Key::ArrowDown, Vec2::new(0.0, 1.0)),
            ] {
                if i.consume_key(modifiers, key) {
                    delta += direction * step;
                }
            }
            delta
        });
        let indices = self.selected_indices(document);
        if delta != Vec2::ZERO && self.can_edit_selection(document) {
            let delta = source_delta(delta, document);
            let before = document.edit_state();
            move_selection(document, &before.annotations, &indices, delta);
            if let Err(error) = crate::canvas::extend_for_annotations(document, &indices) {
                document.annotations = before.annotations;
                document.geometry = before.geometry;
                self.image_error = Some(error);
                return;
            }
            history.commit(before, document);
        }
    }

    fn select(&mut self, index: Option<usize>, document: &Document) {
        self.layer_target = None;
        self.additional.clear();
        if self.selected != index
            || index
                .and_then(|i| document.annotations.get(i))
                .is_some_and(|a| {
                    matches!(a.kind, AnnotationKind::Text | AnnotationKind::Callout)
                        && a.text != self.text
                })
        {
            // Each text object needs its own input undo state; a previous object's
            // text must never reappear when Command/Ctrl+Z is used in this field.
            self.text_editor_generation = self.text_editor_generation.wrapping_add(1);
        }
        self.selected = index;
        if let Some(annotation) = index.and_then(|i| document.annotations.get(i)) {
            if !annotation.kind.needs_composite() || annotation.kind.is_studio() {
                self.area = None;
                self.area_mask.clear();
                self.pixel_mask = None;
            }
            self.studio = annotation.studio.clone();
            if annotation.kind.is_shape() {
                self.shapes_3d = annotation.kind.is_3d();
            }
            self.color = annotation.color;
            self.stroke_width = annotation.stroke_width;
            self.arrow = annotation.arrow.clone();
            self.fill = annotation.fill;
            self.text.clone_from(&annotation.text);
            self.font_size = annotation.font_size;
            self.text_style = annotation.text_style.clone();
            self.sync_stamp_selection(annotation);
            self.sync_fill_selection(annotation);
            self.effect_size = annotation.effect_size;
            self.step_number = annotation.step_number;
            if annotation.kind == AnnotationKind::Step {
                self.step_diameter = interaction_bounds(annotation).width().max(1.0);
            }
            self.text_color = annotation.text_color;
            self.remember_appearance(annotation.kind);
        }
    }

    fn reconcile_selection(&mut self, document: &Document) {
        let indices = self.selected_indices(document);
        self.set_selection(indices, document);
    }

    fn delete(&mut self, document: &mut Document, history: &mut EditHistory) {
        let indices = self.selected_indices(document);
        if !annotation_layers::editable(document, &indices) {
            return;
        }
        let before = document.edit_state();
        for &index in indices.iter().rev() {
            document.annotations.remove(index);
        }
        self.deselect_pixels();
        history.commit(before, document);
        self.focus_canvas = true;
    }

    fn reorder(&mut self, document: &mut Document, history: &mut EditHistory, forward: bool) {
        let indices = self.selected_indices(document);
        if !annotation_layers::editable(document, &indices) {
            return;
        }
        let before = document.edit_state();
        let mut selected = vec![false; document.annotations.len()];
        for i in indices {
            selected[i] = true;
        }
        if forward {
            for i in (0..document.annotations.len().saturating_sub(1)).rev() {
                if selected[i] && !selected[i + 1] {
                    document.annotations.swap(i, i + 1);
                    selected.swap(i, i + 1);
                }
            }
        } else {
            for i in 1..document.annotations.len() {
                if selected[i] && !selected[i - 1] {
                    document.annotations.swap(i, i - 1);
                    selected.swap(i, i - 1);
                }
            }
        }
        self.set_selection(
            selected
                .iter()
                .enumerate()
                .filter_map(|(i, &selected)| selected.then_some(i))
                .collect(),
            document,
        );
        history.commit(before, document);
        self.focus_canvas = true;
    }
}

fn next_step_number(document: &Document) -> Option<u32> {
    document
        .annotations
        .iter()
        .filter(|a| a.kind == AnnotationKind::Step)
        .map(|a| a.step_number)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .filter(|number| *number <= crate::document::MAX_STEP_NUMBER)
}

fn stroke_point_count(document: &Document) -> usize {
    document
        .annotations
        .iter()
        .map(|annotation| annotation.points.len())
        .sum()
}

fn step_bounds(center: Point, diameter: f32) -> Rect {
    Rect::from_center_size(point(center), Vec2::splat(diameter.max(1.0)))
}

fn default_callout_tail(annotation: &Annotation, image_size: Vec2) -> Point {
    let body = interaction_bounds(annotation);
    let center = body.center();
    // Prefer a lower-right pointer; if that edge has no room, use the side
    // with the largest available margin. A full-canvas bubble has no exterior.
    let reach = (body.width().min(body.height()) * 0.45).clamp(16.0, 80.0);
    let choices = [
        (
            image_size.y - body.bottom(),
            Pos2::new(
                (center.x + reach * 0.6).min(image_size.x),
                (body.bottom() + reach).min(image_size.y),
            ),
        ),
        (
            image_size.x - body.right(),
            Pos2::new((body.right() + reach).min(image_size.x), center.y),
        ),
        (
            body.top(),
            Pos2::new(center.x, (body.top() - reach).max(0.0)),
        ),
        (
            body.left(),
            Pos2::new((body.left() - reach).max(0.0), center.y),
        ),
    ];
    let tip = if choices[0].0 >= reach {
        choices[0].1
    } else {
        choices
            .into_iter()
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .unwrap()
            .1
    };
    from_pos(tip)
}

fn original_size(document: &Document) -> [u32; 2] {
    [document.image.width(), document.image.height()]
}

fn image_point(position: Pos2, rect: Rect, scale: f32, document: &Document) -> Point {
    let output = document.output_size();
    document.geometry.inverse_point(
        Point {
            x: ((position.x - rect.min.x) / scale).clamp(0.0, output[0] as f32),
            y: ((position.y - rect.min.y) / scale).clamp(0.0, output[1] as f32),
        },
        original_size(document),
    )
}

fn source_delta(delta: Vec2, document: &Document) -> Vec2 {
    let origin = document
        .geometry
        .inverse_point(Point { x: 0.0, y: 0.0 }, original_size(document));
    let end = document.geometry.inverse_point(
        Point {
            x: delta.x,
            y: delta.y,
        },
        original_size(document),
    );
    point(end) - point(origin)
}

fn output_point(at: Point, document: &Document) -> Pos2 {
    point(document.geometry.map_point(at, original_size(document)))
}

fn mapped_bounds(bounds: Rect, document: &Document) -> Rect {
    Rect::from_two_pos(
        output_point(from_pos(bounds.min), document),
        output_point(from_pos(bounds.max), document),
    )
}

fn hit_document_handle(
    annotation: &Annotation,
    at: Point,
    scale: f32,
    document: &Document,
) -> Option<Handle> {
    let at = output_point(at, document);
    handles(annotation)
        .into_iter()
        .map(|(handle, position)| {
            (
                handle,
                output_point(from_pos(position), document).distance(at) * scale,
            )
        })
        .filter(|(_, distance)| *distance <= 8.)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(handle, _)| handle)
}

fn hit_document_annotation(document: &Document, at: Point, scale: f32) -> Option<usize> {
    document
        .annotations
        .iter()
        .enumerate()
        .rev()
        .find(|(_, annotation)| {
            annotation.visible()
                && !annotation.locked()
                && hits_transformed(annotation, at, scale, document)
        })
        .map(|(index, _)| index)
}

fn hits_transformed(annotation: &Annotation, at: Point, scale: f32, document: &Document) -> bool {
    if annotation.kind == AnnotationKind::Freehand {
        let source = point(at);
        let output = output_point(at, document);
        let hits_segment = |a: Point, b: Point| {
            segment_distance(source, point(a), point(b)) <= annotation.stroke_width * 0.5
                || segment_distance(output, output_point(a, document), output_point(b, document))
                    <= 6.0 / scale
        };
        return match annotation.points.as_slice() {
            [] => false,
            [dot] => hits_segment(*dot, *dot),
            samples => samples
                .windows(2)
                .any(|pair| hits_segment(pair[0], pair[1])),
        };
    }
    if annotation.kind != AnnotationKind::Fill && !annotation.area_mask.is_empty() {
        let r = interaction_bounds(annotation);
        return crate::shapes::contains(
            &annotation
                .area_mask
                .iter()
                .map(|p| r.min + egui::vec2(p.x, p.y) * r.size())
                .collect::<Vec<_>>(),
            point(at),
        );
    }
    let source_at = point(at);
    let at = output_point(at, document);
    let bounds = mapped_bounds(interaction_bounds(annotation), document);
    let size = document.geometry.rotated_size(original_size(document));
    let output = document.output_size();
    let ratios = Vec2::new(
        output[0] as f32 / size[0] as f32,
        output[1] as f32 / size[1] as f32,
    );
    let tolerance = Vec2::splat(6.0 / scale).max(ratios * annotation.stroke_width * 0.5);
    match annotation.kind {
        AnnotationKind::Freehand => unreachable!("freehand tested above"),
        AnnotationKind::Fill => annotation.fill_region.as_ref().is_some_and(|r| {
            let p = (at - bounds.min) / bounds.size();
            r.contains(p.x, p.y)
        }),
        AnnotationKind::Stamp => bounds.contains(at),
        AnnotationKind::Text => bounds.expand(2.0 / scale).contains(at),
        AnnotationKind::Blur
        | AnnotationKind::Pixelate
        | AnnotationKind::Magnify
        | AnnotationKind::Spotlight
        | AnnotationKind::Simplify
        | AnnotationKind::Highlighter
        | AnnotationKind::MoveArea
        | AnnotationKind::DeleteArea => bounds.expand(2.0 / scale).contains(at),
        AnnotationKind::Callout => {
            bounds.expand(2.0 / scale).contains(at)
                || inside_triangle(
                    at,
                    render::callout_tail(annotation).map(|p| output_point(from_pos(p), document)),
                )
        }
        AnnotationKind::Arrow => {
            let start = output_point(annotation.start, document);
            let end = output_point(annotation.end, document);
            let perpendicular = Vec2::new(-(end - start).y, (end - start).x).normalized();
            // The effective stroke width depends on the segment's screen normal.
            let stroke_scale = 1.0
                / Vec2::new(perpendicular.x / ratios.x, perpendicular.y / ratios.y)
                    .length()
                    .max(f32::EPSILON);
            let threshold = (6.0 / scale).max(annotation.stroke_width * stroke_scale * 0.5);
            (annotation.arrow.curve.is_none() && segment_distance(at, start, end) <= threshold)
                || crate::arrow_style::parts(annotation).iter().any(|part| {
                    let points: Vec<_> = part
                        .points
                        .iter()
                        .map(|p| output_point(from_pos(*p), document))
                        .collect();
                    (part.filled
                        && points.len() >= 3
                        && (1..points.len() - 1)
                            .any(|i| inside_triangle(at, [points[0], points[i], points[i + 1]])))
                        || points
                            .windows(2)
                            .any(|p| segment_distance(at, p[0], p[1]) <= threshold)
                })
        }
        AnnotationKind::RoundedRectangle
        | AnnotationKind::Triangle
        | AnnotationKind::Diamond
        | AnnotationKind::Pentagon
        | AnnotationKind::Hexagon
        | AnnotationKind::Star
        | AnnotationKind::Cross
        | AnnotationKind::Parallelogram
        | AnnotationKind::Cylinder
        | AnnotationKind::Prism
        | AnnotationKind::Cone
        | AnnotationKind::Sphere
        | AnnotationKind::HexagonalPrism
        | AnnotationKind::Torus
        | AnnotationKind::Tetrahedron => {
            let points = crate::shapes::outline(annotation.kind, interaction_bounds(annotation));
            (annotation.fill
                && crate::shapes::contains(&points, source_at)
                && (annotation.kind != AnnotationKind::Torus
                    || !crate::shapes::contains(
                        &crate::shapes::torus_hole(interaction_bounds(annotation)),
                        source_at,
                    )))
                || crate::shapes::details(annotation.kind, interaction_bounds(annotation))
                    .iter()
                    .any(|path| {
                        path.windows(2).any(|p| {
                            segment_distance(source_at, p[0], p[1]) <= annotation.stroke_width * 0.5
                                || segment_distance(
                                    at,
                                    output_point(from_pos(p[0]), document),
                                    output_point(from_pos(p[1]), document),
                                ) <= 6. / scale
                        })
                    })
                || points
                    .iter()
                    .zip(points.iter().cycle().skip(1))
                    .take(points.len())
                    .any(|(a, b)| {
                        segment_distance(source_at, *a, *b) <= annotation.stroke_width * 0.5
                            || segment_distance(
                                at,
                                output_point(from_pos(*a), document),
                                output_point(from_pos(*b), document),
                            ) <= 6.0 / scale
                    })
        }
        AnnotationKind::Rectangle => {
            bounds.expand2(tolerance).contains(at)
                && (annotation.fill
                    || !bounds.shrink2(tolerance).is_positive()
                    || !bounds.shrink2(tolerance).contains(at))
        }
        AnnotationKind::Ellipse | AnnotationKind::Step => {
            let radii = bounds.size() * 0.5;
            if radii.x <= 0.0 || radii.y <= 0.0 {
                return false;
            }
            let offset = at - bounds.center();
            let norm = Vec2::new(offset.x / radii.x, offset.y / radii.y).length();
            if (annotation.fill || annotation.kind == AnnotationKind::Step) && norm <= 1.0 {
                return true;
            }
            let outer = radii + tolerance;
            let inner = radii - tolerance;
            let within_outer = Vec2::new(offset.x / outer.x, offset.y / outer.y).length_sq() <= 1.0;
            within_outer
                && (inner.x <= 0.0
                    || inner.y <= 0.0
                    || Vec2::new(offset.x / inner.x, offset.y / inner.y).length_sq() >= 1.0)
        }
    }
}

fn handle_cursor(handle: Handle, document: &Document) -> CursorIcon {
    if matches!(
        handle,
        Handle::CalloutTail
            | Handle::ArrowStart
            | Handle::ArrowEnd
            | Handle::ArrowBend
            | Handle::ArrowCurvePoint(_)
    ) {
        return CursorIcon::Move;
    }
    let reverse = !document.geometry.quarter_turns.is_multiple_of(2);
    if matches!(
        handle,
        Handle::Left | Handle::Right | Handle::Top | Handle::Bottom
    ) {
        let horizontal = matches!(handle, Handle::Left | Handle::Right) ^ reverse;
        return if horizontal {
            CursorIcon::ResizeHorizontal
        } else {
            CursorIcon::ResizeVertical
        };
    }
    let rising = matches!(handle, Handle::TopRight | Handle::BottomLeft);
    if rising ^ reverse {
        CursorIcon::ResizeNeSw
    } else {
        CursorIcon::ResizeNwSe
    }
}

fn crop_between(start: Point, end: Point, document: &Document) -> CropRect {
    let original = original_size(document);
    let x = start
        .x
        .min(end.x)
        .floor()
        .clamp(0.0, original[0].saturating_sub(1) as f32) as u32;
    let y = start
        .y
        .min(end.y)
        .floor()
        .clamp(0.0, original[1].saturating_sub(1) as f32) as u32;
    let right = start
        .x
        .max(end.x)
        .ceil()
        .clamp((x + 1) as f32, original[0] as f32) as u32;
    let bottom = start
        .y
        .max(end.y)
        .ceil()
        .clamp((y + 1) as f32, original[1] as f32) as u32;
    CropRect {
        x,
        y,
        width: right - x,
        height: bottom - y,
    }
}

/// Keep the current pixel scale when tightening or restoring a crop.
fn with_crop(document: &Document, crop: Option<CropRect>) -> ImageGeometry {
    let original = original_size(document);
    let mut geometry = document.geometry;
    let old_size = geometry.rotated_size(original);
    geometry.crop = crop.filter(|crop| *crop != ImageGeometry::default().crop_rect(original));
    if let Some(output) = geometry.resize {
        let new_size = geometry.rotated_size(original);
        geometry.resize = Some([
            (output[0] as f64 * new_size[0] as f64 / old_size[0] as f64)
                .round()
                .max(1.0) as u32,
            (output[1] as f64 * new_size[1] as f64 / old_size[1] as f64)
                .round()
                .max(1.0) as u32,
        ]);
    }
    geometry
}

fn paint_crop(
    painter: &egui::Painter,
    crop: CropRect,
    document: &Document,
    rect: Rect,
    scale: f32,
) {
    let original = Rect::from_min_size(
        Pos2::new(crop.x as f32, crop.y as f32),
        Vec2::new(crop.width as f32, crop.height as f32),
    );
    let mapped = mapped_bounds(original, document);
    let selected = Rect::from_min_max(
        rect.min + mapped.min.to_vec2() * scale,
        rect.min + mapped.max.to_vec2() * scale,
    )
    .intersect(rect);
    let shade = Color32::from_black_alpha(135);
    for excluded in [
        Rect::from_min_max(rect.min, Pos2::new(rect.right(), selected.top())),
        Rect::from_min_max(Pos2::new(rect.left(), selected.bottom()), rect.max),
        Rect::from_min_max(
            Pos2::new(rect.left(), selected.top()),
            selected.left_bottom(),
        ),
        Rect::from_min_max(
            selected.right_top(),
            Pos2::new(rect.right(), selected.bottom()),
        ),
    ] {
        if excluded.is_positive() {
            painter.rect_filled(excluded, 0, shade);
        }
    }
    if selected.is_positive() {
        painter.rect_stroke(
            selected,
            0,
            Stroke::new(2.0, Color32::WHITE),
            StrokeKind::Inside,
        );
        for fraction in [1.0 / 3.0, 2.0 / 3.0] {
            painter.line_segment(
                [
                    Pos2::new(
                        selected.left() + selected.width() * fraction,
                        selected.top(),
                    ),
                    Pos2::new(
                        selected.left() + selected.width() * fraction,
                        selected.bottom(),
                    ),
                ],
                Stroke::new(1.0, Color32::from_white_alpha(115)),
            );
            painter.line_segment(
                [
                    Pos2::new(
                        selected.left(),
                        selected.top() + selected.height() * fraction,
                    ),
                    Pos2::new(
                        selected.right(),
                        selected.top() + selected.height() * fraction,
                    ),
                ],
                Stroke::new(1.0, Color32::from_white_alpha(115)),
            );
        }
    }
}

fn tool_button(ui: &mut egui::Ui, tool: Tool, label: &str, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(48.0, 42.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    if ui.is_rect_visible(rect) {
        let (fill, color) = if selected {
            (crate::appearance::CREAM, crate::appearance::ACCENT)
        } else if response.hovered() {
            (
                ui.visuals().widgets.hovered.weak_bg_fill,
                ui.visuals().strong_text_color(),
            )
        } else {
            (Color32::TRANSPARENT, ui.visuals().text_color())
        };
        let painter = ui.painter();
        painter.rect_filled(rect, 7, fill);
        let center = Pos2::new(rect.center().x, rect.top() + 14.);
        paint_tool_icon(painter, center, tool, color);
        painter.text(
            Pos2::new(rect.center().x, rect.bottom() - 8.),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(10.5),
            color,
        );
        if response.has_focus() {
            painter.rect_stroke(
                rect,
                7,
                Stroke::new(1.0, ui.visuals().selection.stroke.color),
                StrokeKind::Inside,
            );
        }
    }
    response
}

fn paint_tool_icon(painter: &egui::Painter, center: Pos2, tool: Tool, color: Color32) {
    let stroke = Stroke::new(1.5, color);
    match tool {
        Tool::Wand => {
            painter.line_segment(
                [center + egui::vec2(-7., 7.), center + egui::vec2(4., -4.)],
                Stroke::new(2., color),
            );
            for offset in [
                egui::vec2(-6., -6.),
                egui::vec2(7., -7.),
                egui::vec2(7., 3.),
            ] {
                let p = center + offset;
                painter.line_segment([p - egui::vec2(2., 0.), p + egui::vec2(2., 0.)], stroke);
                painter.line_segment([p - egui::vec2(0., 2.), p + egui::vec2(0., 2.)], stroke);
            }
        }
        Tool::Magnify => {
            painter.circle_stroke(center - egui::vec2(2., 2.), 6., stroke);
            painter.line_segment(
                [center + egui::vec2(3., 3.), center + egui::vec2(8., 8.)],
                Stroke::new(2.5, color),
            );
            painter.line_segment(
                [center + egui::vec2(-5., -2.), center + egui::vec2(1., -2.)],
                stroke,
            );
            painter.line_segment(
                [center + egui::vec2(-2., -5.), center + egui::vec2(-2., 1.)],
                stroke,
            );
        }
        Tool::Spotlight => {
            painter.rect_filled(
                Rect::from_center_size(center, egui::vec2(20., 16.)),
                3,
                color.gamma_multiply(0.3),
            );
            painter.circle_filled(center, 5., color);
            painter.circle_stroke(center, 8., stroke);
        }
        Tool::Simplify => {
            for (y, w) in [(-5., 18.), (0., 12.), (5., 15.)] {
                painter.rect_filled(
                    Rect::from_center_size(
                        center + egui::vec2((w - 18.) / 2., y),
                        egui::vec2(w, 3.),
                    ),
                    1,
                    color,
                );
            }
        }
        Tool::CutOut => {
            for y in [-6., 6.] {
                painter.line_segment(
                    [center + egui::vec2(-9., y), center + egui::vec2(9., y)],
                    stroke,
                );
            }
            painter.add(egui::Shape::line(
                vec![
                    center + egui::vec2(-9., 0.),
                    center + egui::vec2(-4., -2.),
                    center + egui::vec2(1., 2.),
                    center + egui::vec2(6., -2.),
                    center + egui::vec2(9., 0.),
                ],
                stroke,
            ));
        }
        Tool::Pen => {
            painter.add(egui::Shape::line(
                vec![
                    center + (Vec2::new(-11.0, 7.0) * 0.75),
                    center + (Vec2::new(-6.0, -3.0) * 0.75),
                    center + (Vec2::new(-1.0, 4.0) * 0.75),
                    center + (Vec2::new(5.0, -8.0) * 0.75),
                    center + (Vec2::new(11.0, -4.0) * 0.75),
                ],
                Stroke::new(2.0, color),
            ));
        }
        Tool::Eraser => {
            let points = [
                (-11.0, 2.0),
                (1.0, -10.0),
                (11.0, 0.0),
                (1.0, 10.0),
                (-3.0, 10.0),
            ]
            .into_iter()
            .map(|(x, y)| center + (Vec2::new(x, y) * 0.75))
            .collect();
            painter.add(egui::Shape::closed_line(points, stroke));
            painter.line_segment(
                [
                    center + (Vec2::new(-5.0, -4.0) * 0.75),
                    center + (Vec2::new(5.0, 6.0) * 0.75),
                ],
                stroke,
            );
        }
        Tool::Area => {
            painter.rect_stroke(
                Rect::from_center_size(center, Vec2::new(18., 15.)),
                0,
                stroke,
                StrokeKind::Inside,
            );
            for offset in [Vec2::new(-9., -7.5), Vec2::new(9., 7.5)] {
                painter.rect_filled(
                    Rect::from_center_size(center + offset, Vec2::splat(4.)),
                    0,
                    color,
                );
            }
        }
        Tool::Select => {
            let points = [
                (-6.0, -10.0),
                (-6.0, 7.0),
                (-1.0, 3.0),
                (3.0, 10.0),
                (6.0, 8.0),
                (2.0, 1.0),
                (9.0, 1.0),
            ]
            .into_iter()
            .map(|(x, y)| center + (Vec2::new(x, y) * 0.75))
            .collect();
            painter.add(egui::Shape::closed_line(points, stroke));
        }
        Tool::Crop => {
            painter.line_segment(
                [
                    center + (Vec2::new(-6.0, -11.0) * 0.75),
                    center + (Vec2::new(-6.0, 6.0) * 0.75),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + (Vec2::new(-6.0, 6.0) * 0.75),
                    center + (Vec2::new(11.0, 6.0) * 0.75),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + (Vec2::new(-11.0, -6.0) * 0.75),
                    center + (Vec2::new(6.0, -6.0) * 0.75),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + (Vec2::new(6.0, -6.0) * 0.75),
                    center + (Vec2::new(6.0, 11.0) * 0.75),
                ],
                stroke,
            );
        }
        Tool::Fill => {
            let p = |x, y| center + Vec2::new(x, y);
            painter.add(egui::Shape::closed_line(
                vec![p(-8., -3.), p(0., -11.), p(10., -1.), p(2., 7.)],
                stroke,
            ));
            painter.line_segment([p(-5., -6.), p(-5., -13.)], stroke);
            painter.line_segment([p(-6., -1.), p(6., -1.)], stroke);
            painter.add(egui::Shape::convex_polygon(
                vec![p(12., 3.), p(9., 9.), p(15., 9.)],
                color,
                Stroke::NONE,
            ));
        }
        Tool::Stamp => {
            painter.circle_stroke(center + Vec2::new(0., -7.), 5., stroke);
            painter.line_segment(
                [center + Vec2::new(-3., -2.), center + Vec2::new(-3., 4.)],
                stroke,
            );
            painter.line_segment(
                [center + Vec2::new(3., -2.), center + Vec2::new(3., 4.)],
                stroke,
            );
            painter.rect_stroke(
                Rect::from_center_size(center + Vec2::new(0., 8.), Vec2::new(21., 7.)),
                2.,
                stroke,
                StrokeKind::Inside,
            );
        }
        Tool::Text => {
            painter.line_segment(
                [
                    center + (Vec2::new(-9.0, -8.0) * 0.75),
                    center + (Vec2::new(9.0, -8.0) * 0.75),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + (Vec2::new(0.0, -8.0) * 0.75),
                    center + (Vec2::new(0.0, 9.0) * 0.75),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + (Vec2::new(-5.0, 9.0) * 0.75),
                    center + (Vec2::new(5.0, 9.0) * 0.75),
                ],
                stroke,
            );
        }
        Tool::Arrow => {
            painter.arrow(
                center + (Vec2::new(-9.0, 8.0) * 0.75),
                Vec2::new(18.0, -16.0) * 0.75,
                stroke,
            );
        }
        Tool::Rectangle | Tool::Shape(_) => {
            painter.rect_stroke(
                Rect::from_center_size(center, Vec2::new(22.0, 17.0) * 0.75),
                1,
                stroke,
                StrokeKind::Middle,
            );
        }
        Tool::Ellipse => {
            painter.add(egui::Shape::ellipse_stroke(
                center,
                Vec2::new(11.0, 9.0) * 0.75,
                stroke,
            ));
        }
        Tool::Step => {
            painter.circle_stroke(center, 7.5, stroke);
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                "1",
                egui::FontId::proportional(12.0),
                color,
            );
        }
        Tool::Callout => {
            let points = [
                (-11.0, -8.0),
                (11.0, -8.0),
                (11.0, 5.0),
                (3.0, 5.0),
                (-3.0, 11.0),
                (-3.0, 5.0),
                (-11.0, 5.0),
            ]
            .into_iter()
            .map(|(x, y)| center + (Vec2::new(x, y) * 0.75))
            .collect();
            painter.add(egui::Shape::closed_line(points, stroke));
            for offset in [-3.0, 1.0] {
                painter.line_segment(
                    [
                        center + (Vec2::new(-6.0, offset) * 0.75),
                        center + (Vec2::new(6.0, offset) * 0.75),
                    ],
                    stroke,
                );
            }
        }
        Tool::Highlighter => {
            let points = [(3.0, -11.0), (11.0, -3.0), (1.0, 7.0), (-7.0, -1.0)]
                .into_iter()
                .map(|(x, y)| center + (Vec2::new(x, y) * 0.75))
                .collect();
            painter.add(egui::Shape::closed_line(points, stroke));
            painter.line_segment(
                [
                    center + (Vec2::new(-7.0, 0.0) * 0.75),
                    center + (Vec2::new(-10.0, 5.0) * 0.75),
                ],
                Stroke::new(3.0, color),
            );
            painter.line_segment(
                [
                    center + (Vec2::new(-11.0, 10.0) * 0.75),
                    center + (Vec2::new(11.0, 10.0) * 0.75),
                ],
                Stroke::new(2.25, color.gamma_multiply(0.5)),
            );
        }
        Tool::Blur => {
            for y in -1_i32..=1 {
                for x in -1_i32..=1 {
                    let strength = if x == 0 && y == 0 { 1.0 } else { 0.45 };
                    let position = center + (Vec2::new(x as f32 * 7.0, y as f32 * 7.0) * 0.75);
                    painter.circle_filled(position, 3.4, color.gamma_multiply(strength * 0.25));
                    painter.circle_filled(position, 2.25, color.gamma_multiply(strength));
                }
            }
        }
        Tool::Pixelate => {
            for y in 0..3 {
                for x in 0..3 {
                    let shade = [0.35, 0.7, 1.0][(x + y * 2) % 3];
                    painter.rect_filled(
                        Rect::from_min_size(
                            center
                                + (Vec2::new(x as f32 * 7.0 - 10.0, y as f32 * 7.0 - 10.0) * 0.75),
                            Vec2::splat(4.5),
                        ),
                        0,
                        color.gamma_multiply(shade),
                    );
                }
            }
        }
    }
}

fn point(value: Point) -> Pos2 {
    Pos2::new(value.x, value.y)
}
fn from_pos(value: Pos2) -> Point {
    Point {
        x: value.x,
        y: value.y,
    }
}

fn interaction_bounds(annotation: &Annotation) -> Rect {
    if matches!(
        annotation.kind,
        AnnotationKind::Text | AnnotationKind::Freehand
    ) {
        render::annotation_bounds(annotation)
    } else {
        Rect::from_two_pos(point(annotation.start), point(annotation.end))
    }
}

fn has_extent(annotation: &Annotation) -> bool {
    let delta = point(annotation.end) - point(annotation.start);
    match annotation.kind {
        AnnotationKind::Arrow => delta.length_sq() >= 4.0,
        AnnotationKind::Rectangle
        | AnnotationKind::Ellipse
        | AnnotationKind::RoundedRectangle
        | AnnotationKind::Triangle
        | AnnotationKind::Diamond
        | AnnotationKind::Pentagon
        | AnnotationKind::Hexagon
        | AnnotationKind::Star
        | AnnotationKind::Cross
        | AnnotationKind::Parallelogram
        | AnnotationKind::Cylinder
        | AnnotationKind::Prism
        | AnnotationKind::Cone
        | AnnotationKind::Sphere
        | AnnotationKind::HexagonalPrism
        | AnnotationKind::Torus
        | AnnotationKind::Tetrahedron
        | AnnotationKind::MoveArea
        | AnnotationKind::DeleteArea
        | AnnotationKind::Blur
        | AnnotationKind::Pixelate
        | AnnotationKind::Magnify
        | AnnotationKind::Spotlight
        | AnnotationKind::Simplify
        | AnnotationKind::Callout
        | AnnotationKind::Highlighter
        | AnnotationKind::Fill
        | AnnotationKind::Stamp
        | AnnotationKind::Step => delta.x.abs() >= 1.0 && delta.y.abs() >= 1.0,
        AnnotationKind::Text => true,
        AnnotationKind::Freehand => !annotation.points.is_empty(),
    }
}

fn handles(annotation: &Annotation) -> Vec<(Handle, Pos2)> {
    let bounds = interaction_bounds(annotation);
    match annotation.kind {
        AnnotationKind::Arrow => {
            let mut h = vec![
                (Handle::ArrowStart, point(annotation.start)),
                (Handle::ArrowEnd, point(annotation.end)),
                (
                    Handle::ArrowBend,
                    crate::arrow_style::curve_point(annotation, 0.5),
                ),
            ];
            if annotation.arrow.curve.is_some() {
                h.extend(
                    ARROW_CURVE_PARAMETERS
                        .into_iter()
                        .enumerate()
                        .map(|(i, t)| {
                            (
                                Handle::ArrowCurvePoint(i),
                                crate::arrow_style::curve_point(annotation, t),
                            )
                        }),
                );
            }
            h
        }
        AnnotationKind::Text => vec![(Handle::TextSize, bounds.right_bottom())],
        _ => {
            let mut handles = Vec::with_capacity(9);
            if annotation.kind == AnnotationKind::Callout
                && let Some(tail) = annotation.tail
            {
                handles.push((Handle::CalloutTail, point(tail)));
            }
            handles.extend([
                (Handle::TopLeft, bounds.left_top()),
                (Handle::TopRight, bounds.right_top()),
                (Handle::BottomLeft, bounds.left_bottom()),
                (Handle::BottomRight, bounds.right_bottom()),
            ]);
            // Numbered step markers retain their circular, uniform corner resize.
            if annotation.kind != AnnotationKind::Step {
                handles.extend([
                    (Handle::Left, bounds.left_center()),
                    (Handle::Right, bounds.right_center()),
                    (Handle::Top, bounds.center_top()),
                    (Handle::Bottom, bounds.center_bottom()),
                ]);
            }
            handles
        }
    }
}

fn segment_distance(at: Pos2, start: Pos2, end: Pos2) -> f32 {
    let direction = end - start;
    let length = direction.length_sq();
    if length <= f32::EPSILON {
        return at.distance(start);
    }
    let t = ((at - start).dot(direction) / length).clamp(0.0, 1.0);
    at.distance(start + direction * t)
}

fn inside_triangle(at: Pos2, vertices: [Pos2; 3]) -> bool {
    fn cross(a: Vec2, b: Vec2) -> f32 {
        a.x * b.y - a.y * b.x
    }
    let [a, b, c] = vertices;
    if cross(b - a, c - a).abs() <= f32::EPSILON {
        return false;
    }
    let sides = [
        cross(b - a, at - a),
        cross(c - b, at - b),
        cross(a - c, at - c),
    ];
    sides.iter().all(|v| *v >= 0.0) || sides.iter().all(|v| *v <= 0.0)
}

fn resized(original: &Annotation, handle: Handle, at: Point) -> Annotation {
    let mut annotation = original.clone();
    let bounds = interaction_bounds(original);
    match handle {
        Handle::ArrowStart => {
            annotation.start = at;
            if let Some(c) = &mut annotation.arrow.curve {
                c[0] = from_pos(point(c[0]) + (point(at) - point(original.start)));
            }
        }
        Handle::ArrowEnd => {
            annotation.end = at;
            if let Some(c) = &mut annotation.arrow.curve {
                c[1] = from_pos(point(c[1]) + (point(at) - point(original.end)));
            }
        }
        Handle::ArrowBend => {
            // Moving both controls by 4/3 delta places B(0.5) exactly under the pointer.
            let delta = (point(at) - crate::arrow_style::curve_point(original, 0.5)) * 4. / 3.;
            annotation.arrow.curve =
                Some(crate::arrow_style::controls(original).map(|p| from_pos(p + delta)));
        }
        Handle::ArrowCurvePoint(index) => {
            let mut controls = crate::arrow_style::controls(original);
            let delta = point(at)
                - crate::arrow_style::curve_point(original, ARROW_CURVE_PARAMETERS[index]);
            // Invert the cubic's weights at t=1/4 and t=3/4. The grabbed
            // point follows the pointer exactly; the opposite adjustment point
            // and both endpoints stay fixed. Stored Bézier controls stay internal.
            controls[index] += delta * (8. / 3.);
            controls[1 - index] -= delta * (8. / 9.);
            annotation.arrow.curve = Some(controls.map(from_pos));
        }
        Handle::CalloutTail => annotation.tail = Some(at),
        Handle::TextSize => {
            let size = point(at) - point(original.start);
            let factor = (size.x / bounds.width().max(1.0)).max(size.y / bounds.height().max(1.0));
            annotation.font_size =
                (original.font_size * factor).clamp(1.0, crate::document::MAX_FONT_SIZE);
        }
        _ => {
            let opposite = match handle {
                Handle::TopLeft => bounds.right_bottom(),
                Handle::TopRight => bounds.left_bottom(),
                Handle::BottomLeft => bounds.right_top(),
                Handle::BottomRight | Handle::Right | Handle::Bottom => bounds.left_top(),
                Handle::Left | Handle::Top => bounds.right_bottom(),
                _ => unreachable!(),
            };
            let mut corner = match handle {
                Handle::Left => Pos2::new(at.x, bounds.top()),
                Handle::Right => Pos2::new(at.x, bounds.bottom()),
                Handle::Top => Pos2::new(bounds.left(), at.y),
                Handle::Bottom => Pos2::new(bounds.right(), at.y),
                _ => point(at),
            };
            if original.kind == AnnotationKind::Step {
                let delta = corner - opposite;
                let side = delta.x.abs().max(delta.y.abs()).max(1.0);
                let direction = Vec2::new(
                    if delta.x < 0.0 { -1.0 } else { 1.0 },
                    if delta.y < 0.0 { -1.0 } else { 1.0 },
                );
                corner = opposite + direction * side;
            }
            let resized = Rect::from_two_pos(opposite, corner);
            annotation.start = from_pos(resized.min);
            annotation.end = from_pos(resized.max);
            if original.kind == AnnotationKind::Freehand {
                let factors =
                    resized.size().max(Vec2::splat(0.1)) / bounds.size().max(Vec2::splat(0.1));
                annotation.points = original
                    .points
                    .iter()
                    .map(|p| from_pos(resized.min + (point(*p) - bounds.min) * factors))
                    .collect();
                annotation.stroke_width = (original.stroke_width * factors.x.min(factors.y))
                    .clamp(0.1, crate::document::MAX_STROKE_WIDTH);
                freehand::sync_bounds(&mut annotation);
            }
        }
    }
    annotation
}

pub(crate) fn paint_checkerboard(painter: &egui::Painter, rect: Rect) {
    paint_checkerboard_at(painter, rect, rect.min);
}

fn paint_checkerboard_at(painter: &egui::Painter, rect: Rect, origin: Pos2) {
    let visible = rect.intersect(painter.clip_rect());
    let tile = 14.0;
    let min = ((visible.min - origin) / tile).floor();
    let max = ((visible.max - origin) / tile).ceil();
    for y in min.y as i32..max.y as i32 {
        for x in min.x as i32..max.x as i32 {
            let color = if (x + y) % 2 == 0 { 218 } else { 240 };
            painter.rect_filled(
                Rect::from_min_size(
                    origin + Vec2::new(x as f32, y as f32) * tile,
                    Vec2::splat(tile),
                ),
                0,
                Color32::from_gray(color),
            );
        }
    }
}

/// Small, high-contrast visual handles keep their larger invisible hit target.
fn paint_object_handle(painter: &egui::Painter, center: Pos2, adjustment: bool) {
    let outline = Stroke::new(1., Color32::from_rgb(32, 35, 39));
    if adjustment {
        painter.add(egui::Shape::convex_polygon(
            vec![
                center + Vec2::new(0., -4.2),
                center + Vec2::new(4.2, 0.),
                center + Vec2::new(0., 4.2),
                center + Vec2::new(-4.2, 0.),
            ],
            Color32::from_rgb(255, 218, 40),
            outline,
        ));
    } else {
        painter.circle(center, 3.3, Color32::WHITE, outline);
    }
}

fn paint_selection(
    painter: &egui::Painter,
    annotation: &Annotation,
    document: &Document,
    origin: Pos2,
    scale: f32,
) {
    let bounds = mapped_bounds(interaction_bounds(annotation), document);
    let screen = Rect::from_min_max(
        origin + bounds.min.to_vec2() * scale,
        origin + bounds.max.to_vec2() * scale,
    );
    if !annotation.kind.is_shape()
        && !matches!(
            annotation.kind,
            AnnotationKind::Arrow | AnnotationKind::Callout | AnnotationKind::Step
        )
    {
        // Shapes already define their edges. Only surround content that needs a
        // selection box, keeping the hairline outside its pixels or stroke.
        for stroke in [
            Stroke::new(1.8, Color32::WHITE),
            Stroke::new(0.7, Color32::from_gray(80)),
        ] {
            painter.rect_stroke(screen.expand(3.), 0, stroke, StrokeKind::Middle);
        }
    }
    for (handle, position) in handles(annotation) {
        let center = origin + output_point(from_pos(position), document).to_vec2() * scale;
        let adjustment = matches!(handle, Handle::ArrowCurvePoint(_) | Handle::CalloutTail)
            || (matches!(handle, Handle::ArrowBend) && annotation.arrow.curve.is_none());
        paint_object_handle(painter, center, adjustment);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn annotation(kind: AnnotationKind) -> Annotation {
        Annotation {
            kind,
            start: Point { x: 20.0, y: 30.0 },
            end: Point { x: 80.0, y: 90.0 },
            color: [255, 0, 0, 255],
            stroke_width: 4.0,
            fill: false,
            text: "Text".into(),
            font_size: 24.0,
            effect_size: 16,
            step_number: 1,
            tail: (kind == AnnotationKind::Callout).then_some(Point { x: 50.0, y: 110.0 }),
            text_color: [255; 4],
            group_id: None,
            layer: Default::default(),
            group_layer: Default::default(),
            area_source: None,
            area_mask: Vec::new(),
            stamp: None,
            fill_region: None,
            text_style: Default::default(),
            pixel_mask: None,
            arrow: Default::default(),
            studio: Default::default(),
            points: if kind == AnnotationKind::Freehand {
                vec![Point { x: 20.0, y: 30.0 }, Point { x: 80.0, y: 90.0 }]
            } else {
                Vec::new()
            },
        }
    }

    fn hits_at(annotation: &Annotation, at: Pos2, zoom: f32) -> bool {
        let document = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(100, 100)),
            annotations: Vec::new(),
            geometry: ImageGeometry::default(),
        };
        hits_transformed(annotation, from_pos(at), zoom, &document)
    }

    #[test]
    fn drawing_tools_select_move_and_bend_existing_arrows_without_new_objects() {
        let mut a = annotation(AnnotationKind::Arrow);
        a.start = Point { x: 30., y: 160. };
        a.end = Point { x: 260., y: 160. };
        let mut d = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(400, 300)),
            annotations: vec![a],
            geometry: Default::default(),
        };
        let mut e = EditorState {
            tool: Tool::Arrow,
            ..Default::default()
        };
        let mut h = EditHistory::new(d.edit_state());
        // First body click selects, rather than creating a second arrow.
        e.begin_gesture(Point { x: 100., y: 160. }, 1., &mut d);
        assert!(matches!(
            e.gesture.as_ref().unwrap().kind,
            GestureKind::Move(0)
        ));
        e.finish_annotation_gesture(&mut d, &mut h);
        e.reconcile_selection(&d);
        assert_eq!(d.annotations.len(), 1);
        assert!(!h.can_undo());
        assert_eq!(
            e.canvas_cursor(Point { x: 100., y: 160. }, 1., &d),
            CursorIcon::Grab
        );
        let middle = crate::arrow_style::curve_point(&d.annotations[0], 0.5);
        e.begin_gesture(from_pos(middle), 1., &mut d);
        assert!(matches!(
            e.gesture.as_ref().unwrap().kind,
            GestureKind::Resize(0, Handle::ArrowBend)
        ));
        e.update_gesture(
            Point {
                x: middle.x,
                y: 80.,
            },
            &mut d,
        );
        e.finish_annotation_gesture(&mut d, &mut h);
        e.reconcile_selection(&d);
        let curved = d.edit_state();
        assert!((crate::arrow_style::curve_point(&d.annotations[0], 0.5).y - 80.).abs() < 0.01);
        assert!(hits_at(&d.annotations[0], Pos2::new(middle.x, 80.), 1.));
        assert!(!hits_at(&d.annotations[0], middle, 1.));
        assert!(h.undo(&mut d));
        assert!(d.annotations[0].arrow.curve.is_none());
        assert!(h.redo(&mut d));
        assert_eq!(d.edit_state(), curved);
        let moved = offset_annotation(&d.annotations[0], Vec2::new(10., 12.));
        assert_eq!(
            moved.arrow.curve.unwrap()[0],
            from_pos(point(d.annotations[0].arrow.curve.unwrap()[0]) + Vec2::new(10., 12.))
        );
        // New arrows use appearance, never another object's absolute curve points.
        assert!(
            e.new_annotation(AnnotationKind::Arrow, Point { x: 5., y: 5. })
                .arrow
                .curve
                .is_none()
        );
        e.begin_tool_gesture(
            from_pos(crate::arrow_style::curve_point(&d.annotations[0], 0.5)),
            1.,
            &mut d,
            false,
            true,
        );
        assert!(matches!(
            e.gesture.as_ref().unwrap().kind,
            GestureKind::Draw(1)
        ));
    }

    #[test]
    fn on_curve_handles_follow_drag_and_keep_the_opposite_point_and_endpoints_fixed() {
        let mut arrow = annotation(AnnotationKind::Arrow);
        arrow.start = Point { x: 40., y: 180. };
        arrow.end = Point { x: 350., y: 160. };
        arrow.arrow.curve = Some([Point { x: 120., y: 20. }, Point { x: 270., y: 260. }]);
        for index in 0..2 {
            let mut d = Document {
                image: std::sync::Arc::new(image::RgbaImage::new(400, 300)),
                annotations: vec![arrow.clone()],
                geometry: Default::default(),
            };
            let before = d.edit_state();
            let mut h = EditHistory::new(before.clone());
            let mut e = EditorState {
                tool: Tool::Arrow,
                ..Default::default()
            };
            e.select(Some(0), &d);
            let grabbed = crate::arrow_style::curve_point(&arrow, ARROW_CURVE_PARAMETERS[index]);
            let opposite =
                crate::arrow_style::curve_point(&arrow, ARROW_CURVE_PARAMETERS[1 - index]);
            let handle = handles(&arrow)
                .into_iter()
                .find(|(h, _)| matches!(h, Handle::ArrowCurvePoint(i) if *i == index))
                .unwrap();
            assert_eq!(
                handle.1, grabbed,
                "The visible handle must lie on the curve"
            );
            assert!(hits_at(&arrow, grabbed, 1.));
            e.begin_gesture(from_pos(grabbed), 1., &mut d);
            assert!(
                matches!(e.gesture.as_ref().unwrap().kind, GestureKind::Resize(0, Handle::ArrowCurvePoint(i)) if i == index)
            );
            for delta in [
                Vec2::new(12., -30.),
                Vec2::new(-24., 45.),
                Vec2::new(30., 25.),
            ] {
                let target = grabbed + delta;
                e.update_gesture(from_pos(target), &mut d);
                let a = &d.annotations[0];
                assert!(
                    crate::arrow_style::curve_point(a, ARROW_CURVE_PARAMETERS[index])
                        .distance(target)
                        < 0.001
                );
                assert!(
                    crate::arrow_style::curve_point(a, ARROW_CURVE_PARAMETERS[1 - index])
                        .distance(opposite)
                        < 0.001
                );
                assert_eq!((a.start, a.end), (arrow.start, arrow.end));
            }
            e.finish_annotation_gesture(&mut d, &mut h);
            let after = d.edit_state();
            assert!(h.undo(&mut d));
            assert_eq!(d.edit_state(), before);
            assert!(!h.can_undo(), "A drag must be one undo step");
            assert!(h.redo(&mut d));
            assert_eq!(d.edit_state(), after);
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("direct-curve.myimage");
            crate::document::save_project(&d, &path).unwrap();
            let reopened = crate::document::load_project(&path).unwrap();
            assert_eq!(reopened.edit_state(), after);
            for (h, p) in handles(&reopened.annotations[0]) {
                if let Handle::ArrowCurvePoint(i) = h {
                    assert!(
                        p.distance(crate::arrow_style::curve_point(
                            &reopened.annotations[0],
                            ARROW_CURVE_PARAMETERS[i]
                        )) < 0.001
                    );
                }
            }
        }
    }

    #[test]
    fn on_curve_handles_hit_after_rotation_resize_and_zoom() {
        let mut arrow = annotation(AnnotationKind::Arrow);
        arrow.start = Point { x: 40., y: 180. };
        arrow.end = Point { x: 350., y: 160. };
        arrow.arrow.curve = Some([Point { x: 120., y: 20. }, Point { x: 270., y: 260. }]);
        let d = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(400, 300)),
            annotations: vec![arrow],
            geometry: ImageGeometry {
                quarter_turns: 1,
                resize: Some([600, 800]),
                ..Default::default()
            },
        };
        for zoom in [0.25, 1., 2.] {
            for (index, t) in ARROW_CURVE_PARAMETERS.into_iter().enumerate() {
                let p = crate::arrow_style::curve_point(&d.annotations[0], t);
                assert!(
                    matches!(hit_document_handle(&d.annotations[0], from_pos(p), zoom, &d), Some(Handle::ArrowCurvePoint(i)) if i == index)
                );
                let moved = resized(
                    &d.annotations[0],
                    Handle::ArrowCurvePoint(index),
                    from_pos(p + Vec2::new(10., 15.)),
                );
                let screen = output_point(from_pos(crate::arrow_style::curve_point(&moved, t)), &d)
                    .to_vec2()
                    * zoom;
                let expected = output_point(from_pos(p + Vec2::new(10., 15.)), &d).to_vec2() * zoom;
                assert!((screen - expected).length() < 0.001);
            }
        }
    }

    #[test]
    fn shape_side_handles_resize_one_axis_and_preserve_editable_history() {
        for kind in [
            AnnotationKind::Rectangle,
            AnnotationKind::RoundedRectangle,
            AnnotationKind::Ellipse,
            AnnotationKind::Triangle,
        ] {
            for (handle, delta) in [
                (Handle::Left, Vec2::new(-10., 12.)),
                (Handle::Right, Vec2::new(30., 12.)),
                (Handle::Top, Vec2::new(12., -10.)),
                (Handle::Bottom, Vec2::new(12., 30.)),
            ] {
                let a = annotation(kind);
                let original = interaction_bounds(&a);
                let mut d = Document {
                    image: std::sync::Arc::new(image::RgbaImage::new(400, 300)),
                    annotations: vec![a],
                    geometry: Default::default(),
                };
                let mut e = EditorState {
                    tool: Tool::Shape(kind),
                    ..Default::default()
                };
                e.select(Some(0), &d);
                let before = d.edit_state();
                let mut history = EditHistory::new(before.clone());
                let positions = handles(&d.annotations[0]);
                assert_eq!(positions.len(), 8);
                let anchor = positions.into_iter().find(|(h, _)| *h == handle).unwrap().1;
                // A slightly off-center press should keep the same grab offset.
                let press = anchor + Vec2::splat(1.);
                e.begin_gesture(from_pos(press), 1., &mut d);
                assert!(
                    matches!(e.gesture.as_ref().unwrap().kind, GestureKind::Resize(0, h) if h == handle)
                );
                e.update_gesture(from_pos(press), &mut d);
                assert_eq!(d.edit_state(), before, "Clicking a handle must not edit");
                e.update_gesture(from_pos(press + delta), &mut d);
                let bounds = interaction_bounds(&d.annotations[0]);
                match handle {
                    Handle::Left | Handle::Right => {
                        assert_eq!(bounds.y_range(), original.y_range());
                        if handle == Handle::Left {
                            assert_eq!(bounds.right(), original.right());
                            assert_eq!(bounds.left(), original.left() + delta.x);
                        } else {
                            assert_eq!(bounds.left(), original.left());
                            assert_eq!(bounds.right(), original.right() + delta.x);
                        }
                    }
                    _ => {
                        assert_eq!(bounds.x_range(), original.x_range());
                        if handle == Handle::Top {
                            assert_eq!(bounds.bottom(), original.bottom());
                            assert_eq!(bounds.top(), original.top() + delta.y);
                        } else {
                            assert_eq!(bounds.top(), original.top());
                            assert_eq!(bounds.bottom(), original.bottom() + delta.y);
                        }
                    }
                }
                e.finish_annotation_gesture(&mut d, &mut history);
                let after = d.edit_state();
                assert!(history.undo(&mut d));
                assert_eq!(d.edit_state(), before);
                assert!(!history.can_undo());
                assert!(history.redo(&mut d));
                assert_eq!(d.edit_state(), after);
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("side-resize.myimage");
                crate::document::save_project(&d, &path).unwrap();
                assert_eq!(
                    crate::document::load_project(&path).unwrap().edit_state(),
                    after
                );
            }
        }
    }

    #[test]
    fn small_shape_handles_choose_nearest_point_under_zoom_and_rotation() {
        let a = annotation(AnnotationKind::RoundedRectangle);
        for quarter_turns in 0..4 {
            let d = Document {
                image: std::sync::Arc::new(image::RgbaImage::new(400, 300)),
                annotations: vec![a.clone()],
                geometry: ImageGeometry {
                    quarter_turns,
                    flip_horizontal: true,
                    resize: Some([600, 400]),
                    ..Default::default()
                },
            };
            for (handle, pos) in handles(&a) {
                // At low zoom the hit targets overlap; the nearest visible point wins.
                for zoom in [0.15, 1., 2.] {
                    assert_eq!(
                        hit_document_handle(&a, from_pos(pos), zoom, &d),
                        Some(handle)
                    );
                }
            }
            assert_eq!(
                handle_cursor(Handle::Right, &d),
                if quarter_turns % 2 == 0 {
                    CursorIcon::ResizeHorizontal
                } else {
                    CursorIcon::ResizeVertical
                }
            );
        }
    }

    #[test]
    fn movement_preserves_size_beyond_source_bounds() {
        let original = annotation(AnnotationKind::Rectangle);
        let moved = offset_annotation(&original, Vec2::new(-100.0, 100.0));
        assert_eq!(moved.start, Point { x: -80.0, y: 130.0 });
        assert_eq!(moved.end, Point { x: -20.0, y: 190.0 });
    }

    #[test]
    fn resize_crossing_opposite_corner_normalizes_rectangle() {
        let resized = resized(
            &annotation(AnnotationKind::Rectangle),
            Handle::TopLeft,
            Point { x: 100.0, y: 110.0 },
        );
        assert_eq!(resized.start, Point { x: 80.0, y: 90.0 });
        assert_eq!(resized.end, Point { x: 100.0, y: 110.0 });
    }

    #[test]
    fn arrow_end_resize_keeps_tail_and_direction() {
        let original = annotation(AnnotationKind::Arrow);
        let resized = resized(&original, Handle::ArrowEnd, Point { x: 5.0, y: 10.0 });
        assert_eq!(resized.start, original.start);
        assert_eq!(resized.end, Point { x: 5.0, y: 10.0 });
    }

    #[test]
    fn hit_testing_uses_topmost_shape_and_visible_outline() {
        let outline = annotation(AnnotationKind::Rectangle);
        let mut filled = outline.clone();
        filled.fill = true;
        assert!(!hits_at(&outline, Pos2::new(50.0, 60.0), 1.0));
        assert!(hits_at(&filled, Pos2::new(50.0, 60.0), 1.0));
        assert_eq!(
            hit_document_annotation(
                &Document {
                    image: std::sync::Arc::new(image::RgbaImage::new(100, 100)),
                    annotations: vec![outline, filled],
                    geometry: ImageGeometry::default(),
                },
                Point { x: 20.0, y: 30.0 },
                1.0
            ),
            Some(1)
        );
    }

    #[test]
    fn arrow_hit_tolerance_remains_usable_when_zoomed_out() {
        let arrow = annotation(AnnotationKind::Arrow);
        assert!(hits_at(&arrow, Pos2::new(50.0, 70.0), 0.25));
        assert!(!hits_at(&arrow, Pos2::new(50.0, 70.0), 2.0));
    }

    #[test]
    fn arrowhead_wing_is_selectable() {
        let mut arrow = annotation(AnnotationKind::Arrow);
        arrow.end.y = arrow.start.y;
        arrow.stroke_width = 8.0;
        assert!(hits_at(&arrow, Pos2::new(58.0, 39.0), 1.0));
    }

    #[test]
    fn a_drag_is_one_undo_and_redo_restores_the_final_geometry() {
        let mut editor = EditorState {
            tool: Tool::Rectangle,
            ..EditorState::default()
        };
        let mut document = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(100, 100)),
            annotations: Vec::new(),
            geometry: ImageGeometry::default(),
        };
        let mut history = EditHistory::new(document.edit_state());
        editor.begin_gesture(Point { x: 10.0, y: 20.0 }, 1.0, &mut document);
        editor.update_gesture(Point { x: 30.0, y: 40.0 }, &mut document);
        editor.update_gesture(Point { x: 70.0, y: 80.0 }, &mut document);
        assert!(!history.can_undo());
        editor.finish_gesture(&mut document, &mut history);
        assert_eq!(document.annotations.len(), 1);
        assert!(history.undo(&mut document));
        assert!(document.annotations.is_empty());
        assert!(!history.can_undo());
        assert!(history.redo(&mut document));
        assert_eq!(document.annotations[0].start, Point { x: 10.0, y: 20.0 });
        assert_eq!(document.annotations[0].end, Point { x: 70.0, y: 80.0 });
    }

    #[test]
    fn empty_drag_does_not_dirty_the_document() {
        let mut editor = EditorState {
            tool: Tool::Rectangle,
            ..EditorState::default()
        };
        let mut document = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(100, 100)),
            annotations: Vec::new(),
            geometry: ImageGeometry::default(),
        };
        let mut history = EditHistory::new(document.edit_state());
        editor.begin_gesture(Point { x: 10.0, y: 20.0 }, 1.0, &mut document);
        editor.finish_gesture(&mut document, &mut history);
        assert!(document.annotations.is_empty());
        assert!(!history.is_dirty(&document));
        assert!(!history.can_undo());
    }

    #[test]
    fn privacy_regions_draw_move_resize_and_adjust_with_undo() {
        for tool in [Tool::Blur, Tool::Pixelate] {
            let original = std::sync::Arc::new(image::RgbaImage::from_pixel(
                200,
                160,
                image::Rgba([100, 120, 140, 255]),
            ));
            let mut document = Document {
                image: original.clone(),
                annotations: Vec::new(),
                geometry: ImageGeometry::default(),
            };
            let mut history = EditHistory::new(document.edit_state());
            let mut editor = EditorState {
                tool,
                ..Default::default()
            };
            editor.begin_gesture(Point { x: 10.0, y: 20.0 }, 1.0, &mut document);
            editor.update_gesture(Point { x: 70.0, y: 80.0 }, &mut document);
            editor.finish_gesture(&mut document, &mut history);
            assert_eq!(document.annotations[0].kind, tool.kind().unwrap());
            assert_eq!(document.annotations[0].effect_size, 16);

            editor.tool = Tool::Select;
            editor.begin_gesture(Point { x: 40.0, y: 50.0 }, 1.0, &mut document);
            editor.update_gesture(Point { x: 50.0, y: 60.0 }, &mut document);
            editor.finish_gesture(&mut document, &mut history);
            assert_eq!(document.annotations[0].start, Point { x: 20.0, y: 30.0 });
            assert_eq!(document.annotations[0].end, Point { x: 80.0, y: 90.0 });

            editor.begin_gesture(Point { x: 80.0, y: 90.0 }, 1.0, &mut document);
            editor.update_gesture(Point { x: 100.0, y: 110.0 }, &mut document);
            editor.finish_gesture(&mut document, &mut history);
            assert_eq!(document.annotations[0].end, Point { x: 100.0, y: 110.0 });
            editor.effect_size = 42;
            editor.apply_inspector(&mut document, &mut history);
            assert_eq!(document.annotations[0].effect_size, 42);

            assert!(history.undo(&mut document));
            editor.reconcile_selection(&document);
            assert_eq!(editor.effect_size, 16);
            assert!(history.redo(&mut document));
            editor.reconcile_selection(&document);
            assert_eq!(editor.effect_size, 42);
            assert!(std::sync::Arc::ptr_eq(&original, &document.image));

            for _ in 0..4 {
                assert!(history.undo(&mut document));
            }
            assert!(document.annotations.is_empty());
            assert!(!history.is_dirty(&document));
        }
    }

    #[test]
    fn privacy_regions_are_selectable_inside_after_crop_rotation_and_resize() {
        let mut document = transformed_document();
        for kind in [AnnotationKind::Blur, AnnotationKind::Pixelate] {
            document.annotations[0] = annotation(kind);
            let region = &document.annotations[0];
            assert!(!region.fill);
            let bounds = mapped_bounds(interaction_bounds(region), &document);
            for zoom in [0.25, 1.0, 2.0] {
                let center = document
                    .geometry
                    .inverse_point(from_pos(bounds.center()), original_size(&document));
                assert_eq!(hit_document_annotation(&document, center, zoom), Some(0));
                let outside = document.geometry.inverse_point(
                    from_pos(bounds.right_center() + Vec2::new(5.0 / zoom, 0.0)),
                    original_size(&document),
                );
                assert_eq!(hit_document_annotation(&document, outside, zoom), None);
                assert!(hit_document_handle(region, region.start, zoom, &document).is_some());
            }
        }
    }

    #[test]
    fn privacy_reordering_and_delete_preserve_editable_properties() {
        let mut document = transformed_document();
        document.annotations = vec![
            annotation(AnnotationKind::Text),
            annotation(AnnotationKind::Blur),
            annotation(AnnotationKind::Pixelate),
        ];
        let before = document.edit_state();
        let mut editor = EditorState::default();
        let mut history = EditHistory::new(before.clone());
        editor.select(Some(2), &document);
        editor.reorder(&mut document, &mut history, false);
        assert_eq!(editor.selected, Some(1));
        assert_eq!(document.annotations[1].kind, AnnotationKind::Pixelate);
        editor.delete(&mut document, &mut history);
        assert_eq!(document.annotations.len(), 2);
        assert!(history.undo(&mut document));
        assert!(history.undo(&mut document));
        assert_eq!(document.edit_state(), before);
    }

    #[test]
    fn empty_privacy_region_does_not_commit() {
        for tool in [Tool::Blur, Tool::Pixelate] {
            let mut document = transformed_document();
            let before = document.edit_state();
            let mut history = EditHistory::new(before.clone());
            let mut editor = EditorState {
                tool,
                ..Default::default()
            };
            editor.begin_gesture(Point { x: 50.0, y: 60.0 }, 1.0, &mut document);
            editor.finish_gesture(&mut document, &mut history);
            assert_eq!(document.edit_state(), before);
            assert!(!history.can_undo());
        }
    }

    fn canvas_frame(
        editor: &mut EditorState,
        document: &mut Document,
        history: &mut EditHistory,
        context: &egui::Context,
        texture: &egui::TextureHandle,
        events: Vec<egui::Event>,
    ) {
        let _ = context.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(240.0))),
                events,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    editor.canvas(ui, document, history, texture, &mut Default::default());
                });
            },
        );
    }

    fn button_event(position: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn selecting_and_deselecting_keeps_canvas_stationary_and_does_not_edit_artwork() {
        for tool in [Tool::Arrow, Tool::Ellipse] {
            let ctx = egui::Context::default();
            crate::appearance::apply(&ctx);
            let texture = ctx.load_texture(
                "stationary image",
                egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
                Default::default(),
            );
            let mut a = annotation(AnnotationKind::Arrow);
            a.start = Point { x: 200., y: 250. };
            a.end = Point { x: 700., y: 250. };
            let mut d = Document {
                image: std::sync::Arc::new(image::RgbaImage::new(900, 500)),
                annotations: vec![a],
                geometry: Default::default(),
            };
            let before = d.edit_state();
            let mut e = EditorState {
                tool,
                ..Default::default()
            };
            let mut h = EditHistory::new(before.clone());
            let mut preview = crate::privacy_preview::PrivacyPreview::default();
            let mut frame = |e: &mut EditorState, d: &mut Document, h: &mut EditHistory, events| {
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200., 500.))),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            e.ui(ui, d, h, &texture, &mut preview);
                        });
                    },
                );
                output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Mesh(m) if m.texture_id == texture.id() => {
                            Some(m.calc_bounds())
                        }
                        _ => None,
                    })
                    .fold(Rect::NOTHING, Rect::union)
            };
            for _ in 0..3 {
                frame(&mut e, &mut d, &mut h, vec![]);
            }
            let image = frame(&mut e, &mut d, &mut h, vec![]);
            assert!(image.is_positive());
            let zoom = e.zoom;
            let body = image.min + Vec2::new(300., 250.) * zoom;
            let handle = image.min + Vec2::new(450., 250.) * zoom + Vec2::new(2., 2.);
            let blank = image.min + Vec2::new(100., 60.) * zoom;
            // Blank margin away from the new canvas edge handles.
            let margin = Pos2::new(image.left() - 10., image.center().y + 40.);
            for (at, selected) in [
                (body, Some(0)),
                (handle, Some(0)),
                (blank, None),
                (body, Some(0)),
                (margin, None),
            ] {
                for events in [
                    vec![egui::Event::PointerMoved(at), button_event(at, true)],
                    vec![],
                    vec![button_event(at, false)],
                    vec![],
                ] {
                    let shown = frame(&mut e, &mut d, &mut h, events);
                    assert_eq!(
                        shown, image,
                        "Selecting/deselecting changed the canvas rectangle"
                    );
                    assert_eq!(e.zoom, zoom);
                }
                assert_eq!(
                    d.edit_state(),
                    before,
                    "A click must not move or create an object"
                );
                assert!(!h.can_undo());
                assert_eq!(e.selected, selected);
            }
            // The canvas margin must retain keyboard focus after release too.
            d.annotations[0].color = [12, 34, 56, 255];
            h.commit(before.clone(), &d);
            frame(
                &mut e,
                &mut d,
                &mut h,
                vec![egui::Event::Key {
                    key: Key::Z,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::COMMAND,
                }],
            );
            assert_eq!(
                d.edit_state(),
                before,
                "Undo must still work after margin deselection"
            );
        }
    }

    #[test]
    fn drag_drawings_remain_unselected_until_explicitly_clicked() {
        for tool in [
            Tool::Arrow,
            Tool::Rectangle,
            Tool::Ellipse,
            Tool::Shape(AnnotationKind::Triangle),
            Tool::Highlighter,
            Tool::Pen,
            Tool::Blur,
            Tool::Pixelate,
        ] {
            let mut d = Document {
                image: std::sync::Arc::new(image::RgbaImage::new(400, 300)),
                annotations: vec![],
                geometry: Default::default(),
            };
            let mut e = EditorState {
                tool,
                ..Default::default()
            };
            let mut h = EditHistory::new(d.edit_state());
            e.begin_gesture(Point { x: 40., y: 60. }, 1., &mut d);
            e.update_gesture(Point { x: 200., y: 180. }, &mut d);
            assert!(
                e.selected_indices(&d).is_empty(),
                "{tool:?} showed handles while drawing"
            );
            e.finish_annotation_gesture(&mut d, &mut h);
            assert_eq!(d.annotations.len(), 1);
            assert!(
                e.selected_indices(&d).is_empty(),
                "{tool:?} auto-selected the drawing"
            );
            assert!(h.can_undo());
            let saved = d.edit_state();
            let hit = if matches!(tool, Tool::Ellipse | Tool::Shape(_)) {
                Point { x: 120., y: 60. }
            } else {
                d.annotations[0].start
            };
            e.begin_gesture(hit, 1., &mut d);
            assert_eq!(e.selected, Some(0), "{tool:?}");
            e.finish_annotation_gesture(&mut d, &mut h);
            assert_eq!(d.edit_state(), saved);
        }
    }

    #[test]
    fn drawing_mode_click_selects_top_object_without_placing_text_stamps_or_steps() {
        for tool in [
            Tool::Arrow,
            Tool::Rectangle,
            Tool::Text,
            Tool::Stamp,
            Tool::Step,
            Tool::Pen,
            Tool::Highlighter,
        ] {
            let ctx = egui::Context::default();
            let texture = ctx.load_texture(
                "object hit",
                egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
                Default::default(),
            );
            let mut a = annotation(AnnotationKind::Rectangle);
            a.start = Point { x: 20., y: 20. };
            a.end = Point { x: 80., y: 80. };
            a.fill = true;
            let mut d = Document {
                image: std::sync::Arc::new(image::RgbaImage::new(200, 200)),
                annotations: vec![a],
                geometry: Default::default(),
            };
            let before = d.edit_state();
            let mut h = EditHistory::new(before.clone());
            let mut e = EditorState {
                tool,
                fit: false,
                ..Default::default()
            };
            canvas_frame(&mut e, &mut d, &mut h, &ctx, &texture, vec![]);
            let p = Pos2::new(70., 70.);
            canvas_frame(
                &mut e,
                &mut d,
                &mut h,
                &ctx,
                &texture,
                vec![egui::Event::PointerMoved(p), button_event(p, true)],
            );
            canvas_frame(
                &mut e,
                &mut d,
                &mut h,
                &ctx,
                &texture,
                vec![button_event(p, false)],
            );
            assert_eq!(e.selected, Some(0), "{tool:?}");
            assert_eq!(e.tool, tool);
            assert_eq!(d.edit_state(), before, "{tool:?} must not insert an object");
            assert!(!h.can_undo());
            assert!(e.text_dialog.is_none());
        }
    }

    #[test]
    fn canvas_pointer_drag_outside_image_extends_instead_of_clamping() {
        let context = egui::Context::default();
        let texture = context.load_texture(
            "outward drag",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut d = Document {
            image: std::sync::Arc::new(image::RgbaImage::from_pixel(
                100,
                100,
                image::Rgba([220, 60, 40, 255]),
            )),
            annotations: vec![],
            geometry: Default::default(),
        };
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState {
            fit: false,
            tool: Tool::Area,
            area: Some(Rect::from_min_max(
                egui::pos2(20., 20.),
                egui::pos2(60., 60.),
            )),
            ..Default::default()
        };
        canvas_frame(&mut e, &mut d, &mut h, &context, &texture, vec![]);
        // The 100-pixel image is centered at (70,70) in a 240-pixel viewport.
        let start = egui::pos2(110., 110.);
        let end = egui::pos2(220., 110.);
        canvas_frame(
            &mut e,
            &mut d,
            &mut h,
            &context,
            &texture,
            vec![egui::Event::PointerMoved(start), button_event(start, true)],
        );
        canvas_frame(
            &mut e,
            &mut d,
            &mut h,
            &context,
            &texture,
            vec![egui::Event::PointerMoved(end), button_event(end, false)],
        );
        assert_eq!(d.output_size(), [170, 100]);
        assert_eq!(d.annotations[0].start, Point { x: 130., y: 20. });
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), before);
        assert!(!h.can_undo());
    }

    #[test]
    fn object_cursors_follow_hover_handles_and_held_moves_outside_canvas() {
        let context = egui::Context::default();
        let texture = context.load_texture(
            "cursor",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut a = crate::markup_tests::object(AnnotationKind::Rectangle, [20., 20.], [60., 60.]);
        a.fill = true;
        let mut d = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(100, 100)),
            annotations: vec![a],
            geometry: Default::default(),
        };
        let mut e = EditorState {
            fit: false,
            tool: Tool::Area,
            selected: Some(0),
            ..Default::default()
        };
        let mut h = EditHistory::new(d.edit_state());
        canvas_frame(&mut e, &mut d, &mut h, &context, &texture, vec![]);
        assert_eq!(
            e.canvas_cursor(Point { x: 40., y: 40. }, 1., &d),
            CursorIcon::Grab
        );
        assert_eq!(
            e.canvas_cursor(Point { x: 20., y: 20. }, 1., &d),
            CursorIcon::ResizeNwSe
        );
        assert_eq!(
            e.canvas_cursor(Point { x: 80., y: 80. }, 1., &d),
            CursorIcon::Crosshair
        );
        let start = egui::pos2(110., 110.);
        for events in [
            vec![egui::Event::PointerMoved(start), button_event(start, true)],
            vec![egui::Event::PointerMoved(egui::pos2(250., 110.))],
        ] {
            let output = context.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(240.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        e.canvas(ui, &mut d, &mut h, &texture, &mut Default::default());
                    });
                },
            );
            assert_eq!(output.platform_output.cursor_icon, CursorIcon::Grabbing);
        }
    }

    #[test]
    fn canvas_pinch_changes_only_view_zoom_and_keeps_annotation_selection() {
        let context = egui::Context::default();
        let texture = context.load_texture(
            "pinch",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut d = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(400, 200)),
            annotations: vec![],
            geometry: Default::default(),
        };
        let before = d.edit_state();
        let area = Rect::from_min_max(egui::pos2(20., 20.), egui::pos2(100., 80.));
        let mut e = EditorState {
            fit: true,
            tool: Tool::Area,
            area: Some(area),
            ..Default::default()
        };
        let mut history = EditHistory::new(before.clone());
        canvas_frame(
            &mut e,
            &mut d,
            &mut history,
            &context,
            &texture,
            vec![egui::Event::PointerMoved(egui::pos2(120., 120.))],
        );
        let fitted = e.zoom;
        canvas_frame(
            &mut e,
            &mut d,
            &mut history,
            &context,
            &texture,
            vec![egui::Event::Zoom(2.)],
        );
        assert_eq!(e.zoom, fitted * 2.);
        assert!(!e.fit);
        canvas_frame(
            &mut e,
            &mut d,
            &mut history,
            &context,
            &texture,
            vec![egui::Event::Zoom(0.5)],
        );
        assert_eq!(e.zoom, fitted);
        assert_eq!(e.area, Some(area));
        assert_eq!(d.edit_state(), before);
        assert!(!history.can_undo());
    }

    #[test]
    fn fit_shows_entire_expanding_canvas_while_pointer_is_held_without_drift() {
        let context = egui::Context::default();
        let texture = context.load_texture(
            "fit drag original",
            egui::ColorImage::new([1, 1], vec![Color32::RED]),
            Default::default(),
        );
        let mut d = Document {
            image: std::sync::Arc::new(image::RgbaImage::from_pixel(
                400,
                200,
                image::Rgba([220, 60, 40, 255]),
            )),
            annotations: vec![],
            geometry: Default::default(),
        };
        let before = d.edit_state();
        let mut history = EditHistory::new(before.clone());
        let mut e = EditorState {
            fit: true,
            tool: Tool::Area,
            area: Some(Rect::from_min_max(
                egui::pos2(180., 50.),
                egui::pos2(380., 150.),
            )),
            ..Default::default()
        };
        canvas_frame(&mut e, &mut d, &mut history, &context, &texture, vec![]);
        let zoom = e.zoom;
        let start = egui::pos2(120. + 80. * zoom, 120.);
        let end = egui::pos2(230., 120.);
        canvas_frame(
            &mut e,
            &mut d,
            &mut history,
            &context,
            &texture,
            vec![egui::Event::PointerMoved(start), button_event(start, true)],
        );
        let mut preview = crate::privacy_preview::PrivacyPreview::default();
        let mut previous = None;
        for events in [vec![egui::Event::PointerMoved(end)], vec![], vec![]] {
            let output = context.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(240.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        e.canvas(ui, &mut d, &mut history, &texture, &mut preview);
                    });
                },
            );
            assert_eq!(d.geometry, before.geometry);
            assert_eq!(e.zoom, zoom);
            assert!(!history.can_undo());
            assert!(d.annotations[0].end.x > 400.);
            let primitives = context.tessellate(output.shapes, output.pixels_per_point);
            let mesh = primitives
                .iter()
                .find_map(|p| match &p.primitive {
                    egui::epaint::Primitive::Mesh(m)
                        if m.texture_id != egui::TextureId::default() =>
                    {
                        Some((p.clip_rect, m))
                    }
                    _ => None,
                })
                .expect("current composition is visible before release");
            let bounds = mesh.1.vertices.iter().fold(Rect::NOTHING, |r, v| {
                r.union(Rect::from_min_max(v.pos, v.pos))
            });
            assert!(
                mesh.0.expand(0.1).contains_rect(bounds),
                "whole growing image must fit: {bounds:?} in {:?}",
                mesh.0
            );
            assert!(bounds.width() > 190. && bounds.width() + canvas_handles::GUTTER * 2. <= 224.);
            assert!(
                bounds.height() < 105.,
                "the view must already zoom out before release"
            );
            let current = (bounds, d.annotations[0].start, d.annotations[0].end);
            if let Some(previous) = previous {
                assert_eq!(current, previous);
            }
            previous = Some(current);
        }
        canvas_frame(
            &mut e,
            &mut d,
            &mut history,
            &context,
            &texture,
            vec![button_event(end, false)],
        );
        assert!(d.output_size()[0] > 400);
        assert!(history.undo(&mut d));
        assert_eq!(d.edit_state(), before);
    }

    #[test]
    fn held_selection_drag_paints_pixels_beyond_original_canvas_without_reframing() {
        let context = egui::Context::default();
        let texture = context.load_texture(
            "original",
            egui::ColorImage::new([1, 1], vec![Color32::RED]),
            Default::default(),
        );
        let mut d = Document {
            image: std::sync::Arc::new(image::RgbaImage::from_pixel(
                100,
                100,
                image::Rgba([220, 60, 40, 255]),
            )),
            annotations: vec![],
            geometry: Default::default(),
        };
        let before = d.edit_state();
        let mut history = EditHistory::new(before.clone());
        let mut e = EditorState {
            fit: false,
            tool: Tool::Area,
            area: Some(Rect::from_min_max(
                egui::pos2(20., 20.),
                egui::pos2(60., 60.),
            )),
            ..Default::default()
        };
        canvas_frame(&mut e, &mut d, &mut history, &context, &texture, vec![]);
        let start = egui::pos2(110., 110.);
        canvas_frame(
            &mut e,
            &mut d,
            &mut history,
            &context,
            &texture,
            vec![egui::Event::PointerMoved(start), button_event(start, true)],
        );
        let end = egui::pos2(220., 110.);
        let mut preview = crate::privacy_preview::PrivacyPreview::default();
        for events in [vec![egui::Event::PointerMoved(end)], vec![]] {
            let output = context.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(240.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        e.canvas(ui, &mut d, &mut history, &texture, &mut preview);
                    });
                },
            );
            assert_eq!(d.output_size(), [100, 100]);
            assert!(!history.can_undo());
            assert_eq!(d.annotations[0].start, Point { x: 130., y: 20. });
            assert_eq!(e.zoom, 1.);
            // The original image ends at x=170. The current textured composition
            // must actually paint across x=220 while the pointer is still held.
            let primitives = context.tessellate(output.shapes, output.pixels_per_point);
            assert!(primitives.iter().any(|p| match &p.primitive {
                egui::epaint::Primitive::Mesh(m) =>
                    m.texture_id != egui::TextureId::default()
                        && m.vertices.iter().any(|v| v.pos.x >= 239.)
                        && p.clip_rect.contains(end),
                _ => false,
            }));
        }
        canvas_frame(
            &mut e,
            &mut d,
            &mut history,
            &context,
            &texture,
            vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert_eq!(d.edit_state(), before);
        assert!(!history.can_undo());
        assert!(e.area_drag.is_none());
    }

    #[test]
    fn canvas_boundary_handles_accept_outer_edges_and_drags_never_move_pixels() {
        let context = egui::Context::default();
        let texture = context.load_texture(
            "boundary input",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut document = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(200, 200)),
            annotations: vec![],
            geometry: Default::default(),
        };
        let before = document.edit_state();
        let mut history = EditHistory::new(before.clone());
        let mut editor = EditorState {
            fit: false,
            ..Default::default()
        };
        editor.select_all_image(&mut document, &mut history);
        editor.set_area_move_mode(false, &document);
        canvas_frame(
            &mut editor,
            &mut document,
            &mut history,
            &context,
            &texture,
            vec![],
        );
        // Canvas is centered at (20,20). Begin just outside its right edge,
        // where the outer half of the handle is painted.
        for (start, end, expected) in [
            (
                egui::pos2(223., 120.),
                egui::pos2(180., 120.),
                Rect::from_min_max(Pos2::ZERO, egui::pos2(160., 200.)),
            ),
            (
                egui::pos2(100., 120.),
                egui::pos2(120., 130.),
                Rect::from_min_max(egui::pos2(20., 0.), egui::pos2(180., 200.)),
            ),
            (
                egui::pos2(200., 220.),
                egui::pos2(180., 200.),
                Rect::from_min_max(egui::pos2(20., 0.), egui::pos2(160., 180.)),
            ),
        ] {
            canvas_frame(
                &mut editor,
                &mut document,
                &mut history,
                &context,
                &texture,
                vec![egui::Event::PointerMoved(start), button_event(start, true)],
            );
            assert!(editor.area_drag.is_some());
            canvas_frame(
                &mut editor,
                &mut document,
                &mut history,
                &context,
                &texture,
                vec![egui::Event::PointerMoved(end), button_event(end, false)],
            );
            assert_eq!(editor.area, Some(expected));
            assert!(editor.area_drag.is_none());
            assert_eq!(document.edit_state(), before);
            assert!(!history.can_undo());
        }
    }

    #[test]
    fn stamp_drop_maps_to_canvas_and_outside_or_cancelled_drops_do_nothing() {
        let context = egui::Context::default();
        let texture = context.load_texture(
            "test",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut editor = EditorState {
            tool: Tool::Stamp,
            fit: false,
            ..Default::default()
        };
        let mut document = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(200, 200)),
            annotations: vec![],
            geometry: ImageGeometry::default(),
        };
        let mut history = EditHistory::new(document.edit_state());
        canvas_frame(
            &mut editor,
            &mut document,
            &mut history,
            &context,
            &texture,
            vec![],
        );
        let source = crate::stamps::StampSource::Builtin { id: "2705".into() };
        let position = Pos2::new(120., 120.);
        // The gallery owns the press; only its release is delivered to the canvas.
        canvas_frame(
            &mut editor,
            &mut document,
            &mut history,
            &context,
            &texture,
            vec![
                egui::Event::PointerMoved(Pos2::ZERO),
                button_event(Pos2::ZERO, true),
            ],
        );
        egui::DragAndDrop::set_payload(&context, source.clone());
        canvas_frame(
            &mut editor,
            &mut document,
            &mut history,
            &context,
            &texture,
            vec![
                egui::Event::PointerMoved(position),
                button_event(position, false),
            ],
        );
        assert_eq!(document.annotations.len(), 1);
        let stamp = &document.annotations[0];
        assert_eq!(stamp.stamp, Some(source.clone()));
        assert!(((stamp.start.x + stamp.end.x) * 0.5 - 100.).abs() < 2.);
        assert!(history.undo(&mut document));
        assert!(!history.can_undo());
        for cancel in [false, true] {
            canvas_frame(
                &mut editor,
                &mut document,
                &mut history,
                &context,
                &texture,
                vec![
                    egui::Event::PointerMoved(Pos2::ZERO),
                    button_event(Pos2::ZERO, true),
                ],
            );
            egui::DragAndDrop::set_payload(&context, source.clone());
            let end = if cancel { position } else { Pos2::ZERO };
            let mut events = vec![egui::Event::PointerMoved(end)];
            if cancel {
                events.push(egui::Event::Key {
                    key: Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                });
            }
            events.push(button_event(end, false));
            canvas_frame(
                &mut editor,
                &mut document,
                &mut history,
                &context,
                &texture,
                events,
            );
            assert!(document.annotations.is_empty());
            assert!(!history.can_undo());
        }
    }

    fn canvas_drag(coalesced: bool) {
        let context = egui::Context::default();
        let texture = context.load_texture(
            "test image",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut editor = EditorState {
            tool: Tool::Rectangle,
            fit: false,
            ..Default::default()
        };
        let mut document = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(200, 200)),
            annotations: Vec::new(),
            geometry: ImageGeometry::default(),
        };
        let mut history = EditHistory::new(document.edit_state());
        canvas_frame(
            &mut editor,
            &mut document,
            &mut history,
            &context,
            &texture,
            Vec::new(),
        );
        let start = Pos2::new(60.0, 60.0);
        let end = Pos2::new(150.0, 130.0);
        if coalesced {
            canvas_frame(
                &mut editor,
                &mut document,
                &mut history,
                &context,
                &texture,
                vec![
                    egui::Event::PointerMoved(start),
                    button_event(start, true),
                    egui::Event::PointerMoved(end),
                    button_event(end, false),
                ],
            );
        } else {
            canvas_frame(
                &mut editor,
                &mut document,
                &mut history,
                &context,
                &texture,
                vec![egui::Event::PointerMoved(start), button_event(start, true)],
            );
            canvas_frame(
                &mut editor,
                &mut document,
                &mut history,
                &context,
                &texture,
                vec![egui::Event::PointerMoved(end)],
            );
            canvas_frame(
                &mut editor,
                &mut document,
                &mut history,
                &context,
                &texture,
                vec![button_event(end, false)],
            );
        }
        assert_eq!(document.annotations.len(), 1);
        let annotation = &document.annotations[0];
        assert_eq!(annotation.kind, AnnotationKind::Rectangle);
        assert_eq!(point(annotation.end) - point(annotation.start), end - start);
        assert!(history.undo(&mut document));
        assert!(document.annotations.is_empty());
        assert!(!history.can_undo());
    }

    #[test]
    fn canvas_draws_from_pointer_events_across_frames() {
        canvas_drag(false);
    }

    #[test]
    fn canvas_draws_when_press_motion_and_release_arrive_in_one_frame() {
        canvas_drag(true);
    }

    #[test]
    fn first_privacy_gesture_never_paints_the_original_texture() {
        let context = egui::Context::default();
        let texture = context.load_texture(
            "private original",
            egui::ColorImage::new([1, 1], vec![Color32::RED]),
            Default::default(),
        );
        for tool in [Tool::Blur, Tool::Pixelate] {
            let mut editor = EditorState {
                tool,
                fit: false,
                ..Default::default()
            };
            let mut document = Document {
                image: std::sync::Arc::new(image::RgbaImage::from_pixel(
                    200,
                    200,
                    image::Rgba([255, 0, 0, 255]),
                )),
                annotations: Vec::new(),
                geometry: ImageGeometry::default(),
            };
            let mut history = EditHistory::new(document.edit_state());
            let mut preview = crate::privacy_preview::PrivacyPreview::default();
            canvas_frame(
                &mut editor,
                &mut document,
                &mut history,
                &context,
                &texture,
                Vec::new(),
            );
            let start = Pos2::new(60.0, 60.0);
            let end = Pos2::new(150.0, 130.0);
            let frame = context.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(240.0))),
                    events: vec![
                        egui::Event::PointerMoved(start),
                        button_event(start, true),
                        egui::Event::PointerMoved(end),
                        button_event(end, false),
                    ],
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        editor.canvas(ui, &mut document, &mut history, &texture, &mut preview);
                    });
                },
            );
            assert_eq!(document.annotations.len(), 1);
            assert_eq!(document.annotations[0].kind, tool.kind().unwrap());
            assert!(!frame.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Mesh(mesh) if mesh.texture_id == texture.id())), "new privacy region must never expose raw image while its preview loads");
        }
    }

    #[test]
    fn privacy_inspectors_render_at_minimum_window_size() {
        let context = egui::Context::default();
        let texture = context.load_texture(
            "privacy layout",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut preview = crate::privacy_preview::PrivacyPreview::default();
        for tool in [Tool::Blur, Tool::Pixelate] {
            let mut document = transformed_document();
            document.annotations[0] = annotation(tool.kind().unwrap());
            let mut editor = EditorState {
                tool,
                ..Default::default()
            };
            editor.select(Some(0), &document);
            let mut history = EditHistory::new(document.edit_state());
            let _ = context.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(980.0, 640.0))),
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        editor.ui(ui, &mut document, &mut history, &texture, &mut preview);
                    });
                },
            );
            assert!(!history.is_dirty(&document));
        }
    }

    fn transformed_document() -> Document {
        Document {
            image: std::sync::Arc::new(image::RgbaImage::new(200, 160)),
            annotations: vec![annotation(AnnotationKind::Rectangle)],
            geometry: ImageGeometry {
                viewport: None,
                flip_horizontal: false,
                flip_vertical: false,
                canvas: None,
                cutouts: [None; 16],
                crop: Some(CropRect {
                    x: 10,
                    y: 20,
                    width: 100,
                    height: 120,
                }),
                quarter_turns: 1,
                resize: Some([240, 50]),
            },
        }
    }

    #[test]
    fn pointer_and_nudges_follow_output_axes_after_rotation_and_nonuniform_resize() {
        let document = transformed_document();
        let source = Point { x: 35.0, y: 65.0 };
        let output = output_point(source, &document);
        let rect = Rect::from_min_size(Pos2::new(23.0, 49.0), Vec2::new(120.0, 25.0));
        let recovered = image_point(rect.min + output.to_vec2() * 0.5, rect, 0.5, &document);
        assert!((recovered.x - source.x).abs() < 0.001);
        assert!((recovered.y - source.y).abs() < 0.001);
        for expected in [Vec2::new(1.0, 0.0), Vec2::new(0.0, -10.0)] {
            let delta = source_delta(expected, &document);
            let moved = Point {
                x: source.x + delta.x,
                y: source.y + delta.y,
            };
            assert!((output_point(moved, &document) - output - expected).length() < 0.001);
        }
    }

    #[test]
    fn selection_handles_have_constant_screen_tolerance_with_nonuniform_resize() {
        let document = transformed_document();
        let mut annotation = document.annotations[0].clone();
        annotation.kind = AnnotationKind::Arrow;
        let annotation = &annotation;
        let at = output_point(annotation.start, &document);
        for zoom in [0.25, 1.0, 2.0] {
            for offset in [Vec2::X, Vec2::Y] {
                let near = document
                    .geometry
                    .inverse_point(from_pos(at + offset * 7.0 / zoom), original_size(&document));
                let far = document
                    .geometry
                    .inverse_point(from_pos(at + offset * 9.0 / zoom), original_size(&document));
                assert!(hit_document_handle(annotation, near, zoom, &document).is_some());
                assert!(hit_document_handle(annotation, far, zoom, &document).is_none());
            }
        }
    }

    #[test]
    fn transformed_outline_hit_testing_tracks_the_visible_shape() {
        let document = transformed_document();
        let bounds = mapped_bounds(interaction_bounds(&document.annotations[0]), &document);
        let near = bounds.left_center() + Vec2::new(-5.0, 0.0);
        let far = bounds.left_center() + Vec2::new(-15.0, 0.0);
        let inverse = |at| {
            document
                .geometry
                .inverse_point(from_pos(at), original_size(&document))
        };
        assert_eq!(
            hit_document_annotation(&document, inverse(near), 1.0),
            Some(0)
        );
        assert_eq!(hit_document_annotation(&document, inverse(far), 1.0), None);
        assert_eq!(
            hit_document_annotation(&document, inverse(bounds.center()), 1.0),
            None
        );
    }

    #[test]
    fn crop_is_reversible_preserves_annotations_and_retains_output_scale() {
        let mut document = transformed_document();
        let original_pixels = document.image.clone();
        let before = document.edit_state();
        let mut history = EditHistory::new(before.clone());
        let mut editor = EditorState {
            tool: Tool::Crop,
            crop: Some(CropDraft {
                rect: CropRect {
                    x: 20,
                    y: 30,
                    width: 50,
                    height: 40,
                },
                drag_start: None,
            }),
            ..Default::default()
        };
        editor.apply_crop(&mut document, &mut history);
        assert_eq!(document.output_size(), [80, 25]);
        assert_eq!(document.annotations, before.annotations);
        assert!(std::sync::Arc::ptr_eq(&document.image, &original_pixels));
        assert!(history.undo(&mut document));
        assert_eq!(document.edit_state(), before);
        assert!(!history.can_undo());
        assert!(history.redo(&mut document));
        assert_eq!(document.output_size(), [80, 25]);
    }

    #[test]
    fn leaving_image_tool_cancels_unapplied_previews_without_dirtying_project() {
        let mut document = transformed_document();
        let before = document.edit_state();
        let mut history = EditHistory::new(before.clone());
        let mut editor = EditorState {
            tool: Tool::Crop,
            crop: Some(CropDraft {
                rect: CropRect {
                    x: 20,
                    y: 30,
                    width: 50,
                    height: 40,
                },
                drag_start: None,
            }),
            resize: Some(ResizeDraft {
                size: [500, 300],
                ratio: 5.0 / 3.0,
                locked: true,
            }),
            ..Default::default()
        };
        editor.finish_gesture(&mut document, &mut history);
        assert!(editor.crop.is_none());
        assert!(!editor.has_modal());
        assert_eq!(document.edit_state(), before);
        assert!(!history.is_dirty(&document));
        assert!(!history.can_undo());
    }

    #[test]
    fn rotation_swaps_resize_axes_and_invalid_geometry_cannot_commit() {
        let mut document = transformed_document();
        let before = document.edit_state();
        let mut history = EditHistory::new(before.clone());
        let mut editor = EditorState::default();
        editor.rotate(&mut document, &mut history, true);
        assert_eq!(document.output_size(), [50, 240]);
        assert_eq!(document.geometry.quarter_turns, 2);
        assert!(history.undo(&mut document));
        assert_eq!(document.edit_state(), before);
        let mut invalid = document.geometry;
        invalid.resize = Some([32_768, 32_768]);
        assert!(!editor.apply_geometry(&mut document, &mut history, invalid));
        assert_eq!(document.edit_state(), before);
        assert!(!history.can_undo());
    }

    #[test]
    fn crop_marquee_rounds_outward_and_clamps_to_source_edges() {
        let document = transformed_document();
        assert_eq!(
            crop_between(
                Point { x: -2.0, y: 159.2 },
                Point { x: 45.1, y: 12.7 },
                &document
            ),
            CropRect {
                x: 0,
                y: 12,
                width: 46,
                height: 148
            }
        );
        assert_eq!(
            crop_between(
                Point { x: 200.0, y: 160.0 },
                Point { x: 200.0, y: 160.0 },
                &document
            ),
            CropRect {
                x: 199,
                y: 159,
                width: 1,
                height: 1
            }
        );
    }

    #[test]
    fn aspect_lock_constrains_both_axes_at_the_dimension_limit() {
        let mut draft = ResizeDraft {
            size: [32_768, 100],
            ratio: 0.25,
            locked: true,
        };
        draft.constrain_axis(0);
        assert_eq!(draft.size, [8192, 32_768]);
        draft.ratio = 4.0;
        draft.size = [100, 32_768];
        draft.constrain_axis(1);
        assert_eq!(draft.size, [32_768, 8192]);
        draft.size[0] = 1;
        draft.constrain_axis(0);
        assert_eq!(draft.size, [4, 1]);
    }

    #[test]
    fn transformed_canvas_centers_the_output_not_the_original_dimensions() {
        let context = egui::Context::default();
        let texture = context.load_texture(
            "centering test",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut editor = EditorState {
            fit: false,
            zoom: 0.5,
            ..Default::default()
        };
        let mut document = transformed_document();
        let mut history = EditHistory::new(document.edit_state());
        let frame = context.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(240.0))),
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    editor.canvas(
                        ui,
                        &mut document,
                        &mut history,
                        &texture,
                        &mut Default::default(),
                    )
                });
            },
        );
        let mesh = frame
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id == texture.id() => Some(mesh),
                _ => None,
            })
            .expect("image UV mesh");
        let bounds = mesh.calc_bounds();
        assert!((bounds.width() - 120.0).abs() < 0.01);
        assert!((bounds.height() - 25.0).abs() < 0.01);
        assert!((bounds.center() - Pos2::new(120.0, 120.0)).length() < 0.01);
    }

    #[test]
    fn steps_number_automatically_and_keep_numbers_through_reordering_and_undo() {
        let mut document = transformed_document();
        document.annotations.clear();
        let original_image = document.image.clone();
        let mut editor = EditorState {
            tool: Tool::Step,
            ..Default::default()
        };
        let mut history = EditHistory::new(document.edit_state());
        editor.stamp_step(Point { x: 35.0, y: 45.0 }, &mut document, &mut history);
        editor.stamp_step(Point { x: 90.0, y: 90.0 }, &mut document, &mut history);
        assert!(editor.tool == Tool::Step);
        assert_eq!(
            document
                .annotations
                .iter()
                .map(|a| a.step_number)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        editor.step_number = 12;
        editor.step_diameter = 64.0;
        editor.text_color = [11, 22, 33, 255];
        editor.apply_inspector(&mut document, &mut history);
        editor.reorder(&mut document, &mut history, false);
        assert_eq!(document.annotations[0].step_number, 12);
        editor.stamp_step(Point { x: 140.0, y: 100.0 }, &mut document, &mut history);
        assert_eq!(document.annotations[2].step_number, 13);
        assert_eq!(
            interaction_bounds(&document.annotations[2]).size(),
            Vec2::splat(64.0)
        );
        assert_eq!(document.annotations[2].text_color, [11, 22, 33, 255]);
        assert!(std::sync::Arc::ptr_eq(&original_image, &document.image));
        assert!(history.undo(&mut document));
        assert_eq!(document.annotations.len(), 2);
        assert!(history.redo(&mut document));
        assert_eq!(document.annotations[2].step_number, 13);
    }

    #[test]
    fn step_limit_does_not_silently_add_duplicate_numbers() {
        let mut document = transformed_document();
        document.annotations = vec![annotation(AnnotationKind::Step)];
        document.annotations[0].step_number = crate::document::MAX_STEP_NUMBER;
        let before = document.edit_state();
        let mut history = EditHistory::new(before.clone());
        let mut editor = EditorState::default();
        editor.stamp_step(Point { x: 50.0, y: 60.0 }, &mut document, &mut history);
        assert_eq!(document.edit_state(), before);
        assert!(editor.image_error.as_ref().unwrap().contains("9999"));
        assert!(!history.can_undo());
    }

    #[test]
    fn step_corner_resize_remains_circular_and_number_edits_keep_off_canvas_position() {
        let step = annotation(AnnotationKind::Step);
        let resized = resized(&step, Handle::BottomRight, Point { x: 120.0, y: 60.0 });
        assert_eq!(interaction_bounds(&resized).size(), Vec2::splat(100.0));
        assert_eq!(resized.start, step.start);
        let mut document = transformed_document();
        let mut off_canvas = step;
        off_canvas.start.x = -20.0;
        off_canvas.end.x = 40.0;
        document.annotations = vec![off_canvas.clone()];
        let mut editor = EditorState::default();
        editor.select(Some(0), &document);
        let mut history = EditHistory::new(document.edit_state());
        editor.step_number = 18;
        editor.apply_inspector(&mut document, &mut history);
        assert_eq!(document.annotations[0].start, off_canvas.start);
        assert_eq!(document.annotations[0].end, off_canvas.end);
        assert_eq!(document.annotations[0].step_number, 18);
    }

    #[test]
    fn step_tool_stamps_successive_canvas_clicks_without_creating_a_drag_gesture() {
        let context = egui::Context::default();
        let texture = context.load_texture(
            "step clicks",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut editor = EditorState {
            tool: Tool::Step,
            fit: false,
            ..Default::default()
        };
        let mut document = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(200, 200)),
            annotations: Vec::new(),
            geometry: ImageGeometry::default(),
        };
        let mut history = EditHistory::new(document.edit_state());
        canvas_frame(
            &mut editor,
            &mut document,
            &mut history,
            &context,
            &texture,
            Vec::new(),
        );
        for at in [Pos2::new(60.0, 60.0), Pos2::new(150.0, 130.0)] {
            canvas_frame(
                &mut editor,
                &mut document,
                &mut history,
                &context,
                &texture,
                vec![
                    egui::Event::PointerMoved(at),
                    button_event(at, true),
                    button_event(at, false),
                ],
            );
            assert!(editor.gesture.is_none());
        }
        assert_eq!(document.annotations.len(), 2);
        assert_eq!(document.annotations[1].step_number, 2);
        assert!(history.undo(&mut document));
        assert_eq!(document.annotations.len(), 1);
    }

    #[test]
    fn callout_draws_a_bubble_and_pointer_as_one_editable_history_step() {
        let mut document = transformed_document();
        document.annotations.clear();
        let mut history = EditHistory::new(document.edit_state());
        let mut editor = EditorState::default();
        editor.activate_tool(Tool::Callout);
        editor.begin_gesture(Point { x: 20.0, y: 20.0 }, 1.0, &mut document);
        editor.update_gesture(Point { x: 100.0, y: 70.0 }, &mut document);
        editor.finish_gesture(&mut document, &mut history);
        let callout = &document.annotations[0];
        assert_eq!(callout.kind, AnnotationKind::Callout);
        assert_eq!(callout.text, "Callout");
        assert!(!interaction_bounds(callout).contains(point(callout.tail.unwrap())));
        assert!(editor.tool == Tool::Select);
        assert!(editor.focus_text);
        assert!(history.undo(&mut document));
        assert!(document.annotations.is_empty());
        assert!(!history.can_undo());
        assert!(history.redo(&mut document));
        editor.reconcile_selection(&document);
        editor.text = "Editable instructions".into();
        editor.text_color = [200, 210, 220, 255];
        editor.font_size = 18.0;
        editor.apply_inspector(&mut document, &mut history);
        assert_eq!(document.annotations[0].text, "Editable instructions");
        assert_eq!(document.annotations[0].font_size, 18.0);
        assert_eq!(document.annotations[0].text_color, [200, 210, 220, 255]);
    }

    #[test]
    fn callout_tail_moves_with_body_and_resizes_independently() {
        let callout = annotation(AnnotationKind::Callout);
        let moved = offset_annotation(&callout, Vec2::new(10.0, 15.0));
        assert_eq!(moved.start, Point { x: 30.0, y: 45.0 });
        assert_eq!(moved.tail, Some(Point { x: 60.0, y: 125.0 }));
        let outside = offset_annotation(&callout, Vec2::new(0.0, 100.0));
        assert_eq!(outside.tail.unwrap().y, 210.0);
        assert_eq!(outside.start.y, 130.0);
        let bubble = resized(&callout, Handle::BottomRight, Point { x: 100.0, y: 95.0 });
        assert_eq!(bubble.tail, callout.tail);
        let tip = resized(&callout, Handle::CalloutTail, Point { x: 150.0, y: 140.0 });
        assert_eq!(tip.start, callout.start);
        assert_eq!(tip.end, callout.end);
        assert_eq!(tip.tail, Some(Point { x: 150.0, y: 140.0 }));
    }

    #[test]
    fn new_markup_hits_and_tail_handles_follow_crop_rotation_and_nonuniform_resize() {
        let mut document = transformed_document();
        for kind in [
            AnnotationKind::Step,
            AnnotationKind::Callout,
            AnnotationKind::Highlighter,
        ] {
            document.annotations = vec![annotation(kind)];
            assert_eq!(
                hit_document_annotation(&document, Point { x: 50.0, y: 60.0 }, 1.0),
                Some(0)
            );
            if kind == AnnotationKind::Callout {
                let callout = &document.annotations[0];
                let triangle = render::callout_tail(callout);
                let centroid = from_pos(Pos2::new(
                    (triangle[0].x + triangle[1].x + triangle[2].x) / 3.0,
                    (triangle[0].y + triangle[1].y + triangle[2].y) / 3.0,
                ));
                assert!(!interaction_bounds(callout).contains(point(centroid)));
                assert_eq!(hit_document_annotation(&document, centroid, 1.0), Some(0));
                assert!(matches!(
                    hit_document_handle(callout, callout.tail.unwrap(), 1.0, &document),
                    Some(Handle::CalloutTail)
                ));
            }
        }
    }

    #[test]
    fn highlighter_opacity_is_editable_without_leaking_into_other_tools() {
        let mut editor = EditorState::default();
        editor.activate_tool(Tool::Highlighter);
        assert_eq!(editor.color, [255, 220, 0, 80]);
        let mut document = transformed_document();
        document.annotations =
            vec![editor.new_annotation(AnnotationKind::Highlighter, Point { x: 10.0, y: 20.0 })];
        document.annotations[0].end = Point { x: 60.0, y: 80.0 };
        let before = document.edit_state();
        let mut history = EditHistory::new(before.clone());
        editor.select(Some(0), &document);
        editor.color = [10, 200, 30, 120];
        editor.apply_inspector(&mut document, &mut history);
        assert_eq!(document.annotations[0].color, [10, 200, 30, 120]);
        editor.activate_tool(Tool::Arrow);
        assert_eq!(editor.color, [230, 66, 55, 255]);
        editor.activate_tool(Tool::Step);
        assert_eq!(editor.color[3], 255);
        editor.activate_tool(Tool::Callout);
        assert_eq!(editor.color[3], 255);
        editor.activate_tool(Tool::Highlighter);
        assert_eq!(editor.color, [10, 200, 30, 120]);
        assert!(history.undo(&mut document));
        assert_eq!(document.edit_state(), before);
    }

    #[test]
    fn callout_draft_text_is_used_when_drawing_and_stays_separate_from_plain_text() {
        let mut document = transformed_document();
        document.annotations.clear();
        let mut history = EditHistory::new(document.edit_state());
        let mut editor = EditorState::default();
        editor.activate_tool(Tool::Text);
        editor.text = "Plain annotation".into();
        editor.apply_inspector(&mut document, &mut history);
        editor.activate_tool(Tool::Callout);
        assert_eq!(editor.text, "Callout");
        editor.text = "Look here".into();
        editor.apply_inspector(&mut document, &mut history);
        editor.begin_gesture(Point { x: 20.0, y: 20.0 }, 1.0, &mut document);
        editor.update_gesture(Point { x: 100.0, y: 70.0 }, &mut document);
        editor.finish_gesture(&mut document, &mut history);
        assert_eq!(document.annotations[0].text, "Look here");
        editor.activate_tool(Tool::Text);
        assert_eq!(editor.text, "Plain annotation");
        editor.activate_tool(Tool::Callout);
        assert_eq!(editor.text, "Look here");
    }

    #[test]
    fn markup_inspectors_and_toolbar_render_at_minimum_window_size() {
        let context = egui::Context::default();
        let texture = context.load_texture(
            "markup layout",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        for tool in [
            Tool::Step,
            Tool::Callout,
            Tool::Highlighter,
            Tool::Pen,
            Tool::Eraser,
        ] {
            let mut document = transformed_document();
            document.annotations[0] = annotation(tool.kind().unwrap_or(AnnotationKind::Freehand));
            let mut editor = EditorState::default();
            editor.activate_tool(tool);
            if tool != Tool::Eraser {
                editor.select(Some(0), &document);
            }
            let mut history = EditHistory::new(document.edit_state());
            let frame = context.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(980.0, 640.0))),
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        editor.ui(
                            ui,
                            &mut document,
                            &mut history,
                            &texture,
                            &mut Default::default(),
                        );
                    });
                },
            );
            assert!(!frame.shapes.is_empty());
            assert!(!history.is_dirty(&document));
        }
    }

    fn stroke(points: &[(f32, f32)]) -> Annotation {
        let mut annotation = annotation(AnnotationKind::Freehand);
        annotation.text.clear();
        annotation.points = points.iter().map(|&(x, y)| Point { x, y }).collect();
        freehand::sync_bounds(&mut annotation);
        annotation
    }

    #[test]
    fn quick_pen_scribble_keeps_each_corner_and_one_undo() {
        let context = egui::Context::default();
        let texture = context.load_texture(
            "pen",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut document = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(200, 200)),
            annotations: vec![],
            geometry: ImageGeometry::default(),
        };
        let mut editor = EditorState {
            fit: false,
            ..Default::default()
        };
        editor.activate_tool(Tool::Pen);
        let mut history = EditHistory::new(document.edit_state());
        canvas_frame(
            &mut editor,
            &mut document,
            &mut history,
            &context,
            &texture,
            vec![],
        );
        let path = [
            Pos2::new(40.0, 40.0),
            Pos2::new(100.0, 40.0),
            Pos2::new(100.0, 100.0),
            Pos2::new(40.0, 100.0),
        ];
        canvas_frame(
            &mut editor,
            &mut document,
            &mut history,
            &context,
            &texture,
            vec![
                egui::Event::PointerMoved(path[0]),
                button_event(path[0], true),
                egui::Event::PointerMoved(path[1]),
                egui::Event::PointerMoved(path[2]),
                egui::Event::PointerMoved(path[3]),
                button_event(path[3], false),
            ],
        );
        assert_eq!(document.annotations.len(), 1);
        let samples = &document.annotations[0].points;
        assert_eq!(samples.len(), 4);
        assert_eq!(samples[0].y, samples[1].y);
        assert_eq!(samples[1].x, samples[2].x);
        assert_eq!(samples[2].y, samples[3].y);
        assert!(editor.tool == Tool::Pen);
        assert!(editor.selected.is_none());
        assert_eq!(document.annotations[0].stroke_width, 8.0);
        let after = document.edit_state();
        assert!(history.undo(&mut document));
        assert!(document.annotations.is_empty());
        assert!(!history.can_undo());
        assert!(history.redo(&mut document));
        assert_eq!(document.edit_state(), after);
    }

    #[test]
    fn pen_dot_and_appearance_stay_editable_and_independent() {
        let mut document = transformed_document();
        document.annotations.clear();
        let mut history = EditHistory::new(document.edit_state());
        let mut editor = EditorState::default();
        editor.activate_tool(Tool::Pen);
        editor.color = [42, 80, 130, 190];
        editor.stroke_width = 13.0;
        editor.apply_inspector(&mut document, &mut history);
        editor.begin_gesture(Point { x: 60.0, y: 60.0 }, 1.0, &mut document);
        editor.finish_gesture(&mut document, &mut history);
        assert_eq!(
            document.annotations[0].points,
            vec![Point { x: 60.0, y: 60.0 }]
        );
        assert_eq!(document.annotations[0].color, [42, 80, 130, 190]);
        editor.activate_tool(Tool::Arrow);
        assert_eq!(editor.stroke_width, 4.0);
        assert_eq!(editor.color, [230, 66, 55, 255]);
        editor.activate_tool(Tool::Highlighter);
        assert_eq!(editor.color[3], 80);
        editor.activate_tool(Tool::Pen);
        assert_eq!(editor.stroke_width, 13.0);
        assert_eq!(editor.color, [42, 80, 130, 190]);
        editor.tool = Tool::Select;
        editor.select(Some(0), &document);
        editor.stroke_width = 21.0;
        editor.color = [90, 40, 15, 255];
        editor.apply_inspector(&mut document, &mut history);
        assert_eq!(document.annotations[0].stroke_width, 21.0);
        assert_eq!(document.annotations[0].color, [90, 40, 15, 255]);
        assert!(history.undo(&mut document));
        assert_eq!(document.annotations[0].stroke_width, 13.0);
    }

    #[test]
    fn eraser_splits_in_layer_order_with_one_undo_and_preserves_unaffected_objects() {
        let mut document = transformed_document();
        let rectangle = annotation(AnnotationKind::Rectangle);
        let mut untouched = stroke(&[(10.0, 130.0), (10.0, 130.0), (180.0, 130.0)]);
        untouched.start = Point { x: -5.0, y: -6.0 };
        untouched.text = "unused imported data".into();
        document.annotations = vec![
            rectangle.clone(),
            stroke(&[(0.0, 50.0), (200.0, 50.0)]),
            untouched.clone(),
            rectangle.clone(),
        ];
        let before = document.edit_state();
        let image = document.image.clone();
        let mut history = EditHistory::new(before.clone());
        let mut editor = EditorState {
            tool: Tool::Eraser,
            eraser_diameter: 10.0,
            ..Default::default()
        };
        editor.begin_gesture(Point { x: 80.0, y: 20.0 }, 1.0, &mut document);
        editor.update_gesture(Point { x: 80.0, y: 80.0 }, &mut document);
        editor.finish_gesture(&mut document, &mut history);
        assert_eq!(document.annotations.len(), 5);
        assert_eq!(document.annotations[0], rectangle);
        assert_eq!(document.annotations[1].points.last().unwrap().x, 73.0);
        assert_eq!(document.annotations[2].points[0].x, 87.0);
        assert_eq!(document.annotations[3], untouched);
        assert_eq!(document.annotations[4], rectangle);
        assert!(std::sync::Arc::ptr_eq(&document.image, &image));
        let after = document.edit_state();
        assert!(history.undo(&mut document));
        assert_eq!(document.edit_state(), before);
        assert!(!history.can_undo());
        assert!(history.redo(&mut document));
        assert_eq!(document.edit_state(), after);
    }

    #[test]
    fn no_op_erase_and_repeated_erase_do_not_add_history() {
        let mut document = transformed_document();
        document.annotations = vec![stroke(&[(10.0, 20.0), (10.0, 20.0), (180.0, 20.0)])];
        let mut history = EditHistory::new(document.edit_state());
        let mut editor = EditorState {
            tool: Tool::Eraser,
            eraser_diameter: 10.0,
            ..Default::default()
        };
        editor.begin_gesture(Point { x: 40.0, y: 100.0 }, 1.0, &mut document);
        editor.update_gesture(Point { x: 100.0, y: 100.0 }, &mut document);
        editor.finish_gesture(&mut document, &mut history);
        assert!(!history.can_undo());
        assert!(!history.is_dirty(&document));
        editor.begin_gesture(Point { x: 60.0, y: 20.0 }, 1.0, &mut document);
        editor.finish_gesture(&mut document, &mut history);
        let after = document.edit_state();
        editor.begin_gesture(Point { x: 60.0, y: 20.0 }, 1.0, &mut document);
        editor.finish_gesture(&mut document, &mut history);
        assert_eq!(document.edit_state(), after);
        assert!(history.undo(&mut document));
        assert!(!history.can_undo());
    }

    #[test]
    fn freehand_selection_movement_and_degenerate_resize_follow_geometry() {
        let mut document = transformed_document();
        let original = stroke(&[(20.0, 50.0), (80.0, 50.0)]);
        document.annotations = vec![original.clone()];
        assert_eq!(
            hit_document_annotation(&document, Point { x: 50.0, y: 50.0 }, 1.0),
            Some(0)
        );
        assert_eq!(
            hit_document_annotation(&document, Point { x: 50.0, y: 80.0 }, 1.0),
            None
        );
        let moved = offset_annotation(&original, Vec2::new(10.0, 20.0));
        assert_eq!(
            moved.points,
            vec![Point { x: 30.0, y: 70.0 }, Point { x: 90.0, y: 70.0 }]
        );
        for original in [
            original,
            stroke(&[(20.0, 30.0), (20.0, 90.0)]),
            stroke(&[(40.0, 40.0)]),
        ] {
            let resized = resized(&original, Handle::BottomRight, Point { x: 100.0, y: 100.0 });
            assert!(
                resized
                    .points
                    .iter()
                    .all(|p| p.x.is_finite() && p.y.is_finite())
            );
            assert!(resized.stroke_width.is_finite() && resized.stroke_width > 0.0);
            assert!(
                resized.points != original.points || resized.stroke_width != original.stroke_width
            );
        }
    }

    #[test]
    fn pen_budget_stops_without_creating_an_invalid_stroke() {
        let mut document = transformed_document();
        document.annotations = vec![stroke(&[(0.0, 0.0)])];
        document.annotations[0].points =
            vec![Point { x: 10.0, y: 10.0 }; crate::document::MAX_STROKE_POINTS];
        let mut editor = EditorState {
            tool: Tool::Pen,
            ..Default::default()
        };
        editor.gesture = Some(Gesture {
            kind: GestureKind::Draw(0),
            before: EditState::default(),
            origin: Point { x: 10.0, y: 10.0 },
            preview_geometry: None,
        });
        editor.update_gesture(Point { x: 20.0, y: 20.0 }, &mut document);
        assert_eq!(
            document.annotations[0].points.len(),
            crate::document::MAX_STROKE_POINTS
        );
        assert!(editor.image_error.is_some());
        document.annotations = vec![document.annotations[0].clone(); 4];
        editor.gesture = None;
        editor.begin_gesture(Point { x: 60.0, y: 60.0 }, 1.0, &mut document);
        assert_eq!(document.annotations.len(), 4);
        assert!(editor.gesture.is_none());
        assert!(editor.image_error.is_some());
    }

    #[test]
    fn escape_cancels_pen_and_eraser_gestures_without_history() {
        let context = egui::Context::default();
        let texture = context.load_texture(
            "cancel stroke",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        for tool in [Tool::Pen, Tool::Eraser] {
            let mut document = Document {
                image: std::sync::Arc::new(image::RgbaImage::new(200, 200)),
                annotations: vec![stroke(&[(0.0, 50.0), (200.0, 50.0)])],
                geometry: ImageGeometry::default(),
            };
            let before = document.edit_state();
            let mut history = EditHistory::new(before.clone());
            let mut editor = EditorState {
                tool,
                fit: false,
                ..Default::default()
            };
            canvas_frame(
                &mut editor,
                &mut document,
                &mut history,
                &context,
                &texture,
                vec![],
            );
            let pos = Pos2::new(80.0, if tool == Tool::Pen { 110.0 } else { 70.0 });
            canvas_frame(
                &mut editor,
                &mut document,
                &mut history,
                &context,
                &texture,
                vec![egui::Event::PointerMoved(pos), button_event(pos, true)],
            );
            assert_ne!(document.edit_state(), before);
            canvas_frame(
                &mut editor,
                &mut document,
                &mut history,
                &context,
                &texture,
                vec![egui::Event::Key {
                    key: Key::Escape,
                    physical_key: Some(Key::Escape),
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
            );
            assert_eq!(document.edit_state(), before);
            assert!(!history.can_undo());
            assert!(editor.gesture.is_none());
            canvas_frame(
                &mut editor,
                &mut document,
                &mut history,
                &context,
                &texture,
                vec![button_event(pos, false)],
            );
        }
    }

    #[test]
    fn eraser_fragment_limit_is_atomic() {
        let mut document = transformed_document();
        document.annotations =
            vec![annotation(AnnotationKind::Rectangle); crate::document::MAX_ANNOTATIONS];
        document.annotations[0] = stroke(&[(0.0, 50.0), (200.0, 50.0)]);
        let before = document.edit_state();
        let mut editor = EditorState {
            tool: Tool::Eraser,
            eraser_diameter: 10.0,
            ..Default::default()
        };
        let mut history = EditHistory::new(before.clone());
        editor.begin_gesture(Point { x: 100.0, y: 50.0 }, 1.0, &mut document);
        editor.finish_annotation_gesture(&mut document, &mut history);
        assert_eq!(document.edit_state(), before);
        assert!(editor.image_error.is_some());
        assert!(!history.can_undo());
    }
}
