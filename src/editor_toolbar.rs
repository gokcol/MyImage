use super::*;
use crate::appearance::Icon as DialogIcon;
use crate::dialogs::{self, Dialog};

impl EditorState {
    fn tools(&self) -> Vec<(Tool, &'static str, &'static str)> {
        vec![
            (
                Tool::Area,
                "Select",
                "Select pixels with a rectangle or lasso; click an object to edit it",
            ),
            (Tool::Arrow, "Arrow", "Point to a detail"),
            (Tool::Text, "Text", "Write and format text"),
            (Tool::Callout, "Callout", "Add a text bubble and pointer"),
            (
                shape_tool(self.shape_kind),
                "Shapes",
                "2D shapes and 3D diagrams",
            ),
            (
                Tool::Stamp,
                "Stamps",
                "Stamps, signatures and custom SVG artwork",
            ),
            (
                Tool::Fill,
                "Fill",
                "Fill a background with color or transparency",
            ),
            (Tool::Blur, "Blur", "Soften a selected area"),
            (Tool::Pixelate, "Pixelate", "Hide details with pixel blocks"),
            (
                Tool::Simplify,
                "Simplify",
                "Replace details with editable color blocks",
            ),
            (
                Tool::Wand,
                "Magic Wand",
                "Select connected or matching colors",
            ),
            (
                Tool::Crop,
                "Crop",
                "Trim the visible image, keeping the original",
            ),
            (
                Tool::CutOut,
                "Cut Out",
                "Remove a strip and join the remaining image",
            ),
            (Tool::Pen, "Pen", "Draw editable strokes"),
            (Tool::Highlighter, "Highlighter", "Highlight an area"),
            (Tool::Step, "Step", "Add numbered markers"),
            (Tool::Eraser, "Eraser", "Erase parts of pen strokes"),
            (
                Tool::Magnify,
                "Magnify",
                "Enlarge a detail with an adjustable lens",
            ),
            (
                Tool::Spotlight,
                "Spotlight",
                "Dim or blur everything outside the focus area",
            ),
        ]
    }
    pub fn set_toolbar(&mut self, pins: Vec<String>) {
        let valid = self.tools();
        let mut result = Vec::new();
        for pin in pins {
            if valid.iter().any(|(_, label, _)| *label == pin) && !result.contains(&pin) {
                result.push(pin);
            }
        }
        if result
            == [
                "Select", "Arrow", "Text", "Callout", "Shapes", "Stamps", "Fill",
            ]
        {
            result.push("Magnify".into());
            self.toolbar_changed = true;
        }
        self.toolbar_pins = result;
    }
    pub fn take_toolbar_change(&mut self) -> Option<Vec<String>> {
        std::mem::take(&mut self.toolbar_changed).then(|| self.toolbar_pins.clone())
    }
    fn choose_tool(&mut self, tool: Tool, document: &mut Document, history: &mut EditHistory) {
        self.effects_open = false;
        self.move_pixels = true;
        self.area_aspect_ratio = None;
        if tool == Tool::Fill {
            self.activate_fill(document, history);
            return;
        }
        self.finish_gesture(document, history);
        self.layers_open = false;
        self.presets_open = false;
        if tool == Tool::Simplify {
            self.studio.ellipse = false;
            self.color = [161, 175, 190, 255];
        }
        // Area-based effects can immediately use a current selection.
        let area = self.selection_region(document);
        if let Some(kind) = tool.kind().filter(|k| k.is_studio() || k.is_effect())
            && area.is_some()
        {
            self.apply_area_action(kind, document, history);
            self.tool = tool;
            return;
        }
        self.clear_selection();
        self.area = None;
        self.area_mask.clear();
        self.pixel_mask = None;
        self.activate_tool(tool);
        if tool == Tool::Simplify {
            self.studio.ellipse = false;
            self.color = [161, 175, 190, 255];
        }
        if tool == Tool::Crop {
            self.crop = Some(CropDraft {
                rect: document.geometry.crop_rect(original_size(document)),
                drag_start: None,
            });
        }
        if tool == Tool::CutOut {
            self.lasso = false;
        }
    }
    pub fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let tools = self.tools();
        // Reserve More + Copy; overflowing pinned tools stay accessible in More.
        let capacity = ((ui.available_width() - 210.).max(0.) / 51.).floor() as usize;
        let visible: Vec<_> = self
            .toolbar_pins
            .iter()
            .filter_map(|p| tools.iter().find(|(_, l, _)| l == p))
            .take(capacity)
            .copied()
            .collect();
        for &(tool, label, hint) in &visible {
            if tool_button(
                ui,
                tool,
                label,
                self.tool == tool || (tool == Tool::Area && self.tool == Tool::Select),
            )
            .on_hover_text(hint)
            .clicked()
            {
                self.choose_tool(tool, document, history);
            }
        }
        let hidden_active =
            !visible.iter().any(|(t, _, _)| *t == self.tool) && self.tool != Tool::Select;
        ui.menu_button(
            egui::RichText::new("More ▾")
                .size(12.)
                .color(if hidden_active {
                    crate::appearance::palette(ui.ctx()).accent
                } else {
                    crate::appearance::palette(ui.ctx()).ink
                }),
            |ui| {
                ui.set_width(160.);
                ui.spacing_mut().item_spacing.y = 1.;
                for &(tool, label, hint) in &tools {
                    if visible.iter().any(|(t, _, _)| *t == tool) {
                        continue;
                    }
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(160., 27.), Sense::click());
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::SelectableLabel,
                            ui.is_enabled(),
                            self.tool == tool,
                            label,
                        )
                    });
                    if response.hovered() || self.tool == tool {
                        ui.painter().rect_filled(
                            rect,
                            4.,
                            crate::appearance::palette(ui.ctx()).cream,
                        );
                    }
                    paint_tool_icon(
                        ui.painter(),
                        rect.left_center() + egui::vec2(14., 0.),
                        tool,
                        crate::appearance::palette(ui.ctx()).ink,
                    );
                    ui.painter().text(
                        rect.left_center() + egui::vec2(34., 0.),
                        egui::Align2::LEFT_CENTER,
                        label,
                        egui::FontId::proportional(13.),
                        crate::appearance::palette(ui.ctx()).ink,
                    );
                    if response.on_hover_text(hint).clicked() {
                        self.choose_tool(tool, document, history);
                        ui.close();
                    }
                }
                ui.separator();
                if ui
                    .button(egui::RichText::new("Customize toolbar…").size(13.))
                    .clicked()
                {
                    self.toolbar_customizer = true;
                    ui.close();
                }
            },
        );
        ui.add_space(5.);
        ui.separator();
        ui.add_space(3.);
        if crate::appearance::action_button(ui, "Copy", crate::appearance::Icon::Copy, true, false)
            .on_hover_text(format!("{} (Command/Ctrl+C)", self.copy_scope(document)))
            .clicked()
        {
            self.finish_annotation_gesture(document, history);
            self.copy_image(ui.ctx(), document, false);
            self.focus_canvas = true;
        }
        ui.menu_button("▾", |ui| {
            self.pixel_selection_commands(ui, document, history);
        })
        .response
        .on_hover_text("Select all or clear the selection");
        self.copy_feedback(ui);
    }
    pub(super) fn customize_toolbar(&mut self, ctx: &egui::Context) {
        if !self.toolbar_customizer {
            return;
        }
        let mut open = true;
        Dialog::new(
            "Customize toolbar",
            "Pin everyday tools. Find the rest in More.",
            DialogIcon::Settings,
            280.,
        )
        .fit_content()
        .window(ctx, egui::Id::new("Customize toolbar"), &mut open, |ui| {
            let tools = self.tools();
            let before = self.toolbar_pins.clone();
            ui.spacing_mut().item_spacing = egui::vec2(5., 3.);
            ui.spacing_mut().button_padding = egui::vec2(5., 3.);
            ui.spacing_mut().interact_size.y = 25.;
            egui::ScrollArea::vertical()
                .max_height(360.)
                .show(ui, |ui| {
                    for (tool, label, _) in tools {
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(
                                egui::vec2(154., 25.),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    let (icon_rect, _) = ui
                                        .allocate_exact_size(egui::vec2(24., 25.), Sense::hover());
                                    paint_tool_icon(
                                        ui.painter(),
                                        icon_rect.center(),
                                        tool,
                                        crate::appearance::palette(ui.ctx()).ink,
                                    );
                                    let mut pinned = self.toolbar_pins.iter().any(|p| p == label);
                                    if ui
                                        .checkbox(&mut pinned, egui::RichText::new(label).size(13.))
                                        .changed()
                                    {
                                        if pinned {
                                            self.toolbar_pins.push(label.into());
                                        } else {
                                            self.toolbar_pins.retain(|p| p != label);
                                        }
                                    }
                                },
                            );
                            if let Some(i) = self.toolbar_pins.iter().position(|p| p == label) {
                                if ui
                                    .add_enabled(i > 0, egui::Button::new("↑"))
                                    .on_hover_text("Move earlier")
                                    .clicked()
                                {
                                    self.toolbar_pins.swap(i, i - 1);
                                }
                                if ui
                                    .add_enabled(
                                        i + 1 < self.toolbar_pins.len(),
                                        egui::Button::new("↓"),
                                    )
                                    .on_hover_text("Move later")
                                    .clicked()
                                {
                                    self.toolbar_pins.swap(i, i + 1);
                                }
                                ui.weak(format!("{}", i + 1));
                            }
                        });
                    }
                });
            ui.separator();
            if dialogs::secondary(ui, "Restore default toolbar", DialogIcon::Refresh).clicked() {
                self.toolbar_pins = default_toolbar();
            }
            self.toolbar_changed |= before != self.toolbar_pins;
        });
        self.toolbar_customizer = open;
    }
}
