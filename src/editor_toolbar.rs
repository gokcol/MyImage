use super::*;
use crate::appearance::Icon as DialogIcon;
use crate::dialogs::{self, Dialog};

impl EditorState {
    pub(super) fn tools(&self) -> Vec<(Tool, &'static str, &'static str)> {
        let mut tools = vec![
            (
                Tool::ConnectedArrow,
                "Connected Arrows",
                "Connect two objects with an arrow that follows them",
            ),
            (
                Tool::Repair,
                "Smart Erase / Repair",
                "Repair a small region using surrounding colors or nearby texture",
            ),
            (
                Tool::ScreenshotText,
                "Edit Screenshot Text",
                "Recognize a text region and replace it with editable text",
            ),
            (
                Tool::Redact,
                "Redact for Sharing",
                "Cover private details and copy a flattened image",
            ),
            (
                Tool::StyleBrush,
                "Style Brush",
                "Copy appearance from one annotation to compatible artwork",
            ),
            (
                Tool::Measure,
                "Measure",
                "Measure output pixels and add a dimension label",
            ),
            (
                Tool::ColorInspector,
                "Color Inspector",
                "Sample image colors, copy HEX/RGB, or apply to artwork",
            ),
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
            (
                Tool::Pen,
                "Pen",
                "Draw editable strokes with a mouse, pen or touchscreen",
            ),
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
        ];
        tools.sort_by_key(|(_, label, _)| {
            self.toolbar_order
                .iter()
                .position(|name| name == label)
                .unwrap_or(usize::MAX)
        });
        tools
    }
    pub fn set_toolbar(&mut self, pins: Vec<String>, upgrade_layout: bool) {
        let valid = self.tools();
        let mut result = Vec::new();
        for pin in pins {
            if valid.iter().any(|(_, label, _)| *label == pin) && !result.contains(&pin) {
                result.push(pin);
            }
        }
        if upgrade_layout {
            // Apply the compact layout once; later explicit re-pinning survives restart.
            result.retain(|label| label != "Pen" && label != "Magnify");
            self.toolbar_changed = true;
        }
        self.toolbar_pins = result;
    }
    pub fn set_toolbar_order(&mut self, order: Vec<String>) {
        let valid = self.tools();
        let mut result = Vec::new();
        // Older settings only record ribbon order. Preserve it, then append More tools.
        let requested = if order.is_empty() {
            &self.toolbar_pins
        } else {
            &order
        };
        for name in requested
            .iter()
            .map(String::as_str)
            .chain(valid.iter().map(|(_, label, _)| *label))
        {
            if valid.iter().any(|(_, label, _)| *label == name) && !result.iter().any(|v| v == name)
            {
                result.push(name.to_owned());
            }
        }
        self.toolbar_changed |= result != order;
        self.toolbar_order = result;
        self.sort_toolbar_pins();
    }
    fn sort_toolbar_pins(&mut self) {
        self.toolbar_pins.sort_by_key(|name| {
            self.toolbar_order
                .iter()
                .position(|n| n == name)
                .unwrap_or(usize::MAX)
        });
    }
    fn move_toolbar_tool(&mut self, label: &str, earlier: bool) {
        let Some(index) = self.toolbar_order.iter().position(|n| n == label) else {
            return;
        };
        let next = if earlier {
            index.checked_sub(1)
        } else {
            (index + 1 < self.toolbar_order.len()).then_some(index + 1)
        };
        if let Some(next) = next {
            self.toolbar_order.swap(index, next);
            self.sort_toolbar_pins();
            self.toolbar_changed = true;
        }
    }
    pub fn take_toolbar_change(&mut self) -> Option<(Vec<String>, Vec<String>)> {
        std::mem::take(&mut self.toolbar_changed)
            .then(|| (self.toolbar_pins.clone(), self.toolbar_order.clone()))
    }
    pub(super) fn choose_tool(
        &mut self,
        tool: Tool,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if tool == Tool::ConnectedArrow {
            self.start_connector(document, history);
            return;
        }
        if matches!(tool, Tool::Repair | Tool::ScreenshotText | Tool::Redact) {
            self.start_retouch(tool, document, history);
            return;
        }
        if matches!(
            tool,
            Tool::StyleBrush | Tool::Measure | Tool::ColorInspector
        ) {
            self.start_precision_tool(tool, document, history);
            return;
        }
        self.precision.measure_dragging = false;
        self.inspector.select(InspectorPanel::Styles);
        self.move_pixels = true;
        self.area_aspect_ratio = None;
        if tool == Tool::Fill {
            self.activate_fill(document, history);
            return;
        }
        self.finish_gesture(document, history);

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
        if matches!(tool, Tool::Crop | Tool::Stamp | Tool::CutOut) {
            self.inspector.show(InspectorPanel::Styles);
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
        // Reserve the right-aligned panel selector and overflow before placing pins.
        let capacity = ((ui.available_width() - inspector_panel::SELECTOR_WIDTH - 42.).max(0.)
            / (44. + ui.spacing().item_spacing.x))
            .floor() as usize;
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
        let (rect, overflow) = ui.allocate_exact_size(egui::vec2(30., 38.), Sense::click());
        overflow.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), "More tools")
        });
        let palette = crate::appearance::palette(ui.ctx());
        if overflow.hovered()
            || overflow.has_focus()
            || egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&overflow))
        {
            ui.painter()
                .rect_filled(rect.shrink2(egui::vec2(1., 5.)), 7, palette.cream);
        }
        let center = rect.center();
        ui.painter().add(egui::Shape::line(
            vec![
                center + egui::vec2(-5., -2.5),
                center + egui::vec2(0., 2.5),
                center + egui::vec2(5., -2.5),
            ],
            Stroke::new(
                2.,
                if hidden_active {
                    palette.accent
                } else {
                    palette.ink
                },
            ),
        ));
        let overflow = overflow
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("More tools · Customize toolbar");
        egui::Popup::menu(&overflow).show(|ui| {
            ui.set_width(160.);
            ui.spacing_mut().item_spacing.y = 1.;
            for &(tool, label, hint) in &tools {
                if visible.iter().any(|(t, _, _)| *t == tool) {
                    continue;
                }
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(196., 27.), Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::SelectableLabel,
                        ui.is_enabled(),
                        self.tool == tool,
                        label,
                    )
                });
                if response.hovered() || self.tool == tool {
                    ui.painter()
                        .rect_filled(rect, 4., crate::appearance::palette(ui.ctx()).cream);
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
        });
        let action = ui
            .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                inspector_panel::selector(ui, self.inspector)
            })
            .inner;
        if let Some((panel, toggle)) = action {
            self.finish_properties(document, history);
            if toggle && self.inspector.is(panel) && self.inspector.preferences.expanded {
                self.inspector.close();
            } else {
                match panel {
                    InspectorPanel::Effects => self.open_image_effects(document, history),
                    InspectorPanel::Presets => {
                        self.cancel_image_tools();
                        self.inspector.show(panel);
                    }
                    _ => self.inspector.show(panel),
                }
            }
        }
        self.copy_feedback(ui);
    }
    pub(super) fn customize_toolbar(&mut self, ctx: &egui::Context) {
        if !self.toolbar_customizer {
            return;
        }
        if self.toolbar_order.is_empty() {
            self.set_toolbar_order(Vec::new());
        }
        let mut open = true;
        Dialog::new(
            "Customize toolbar",
            "Reorder any tool. Check the ones to show on the ribbon.",
            DialogIcon::Settings,
            350.,
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
                                egui::vec2(226., 25.),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.set_min_width(226.);
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
                                            self.sort_toolbar_pins();
                                        } else {
                                            self.toolbar_pins.retain(|p| p != label);
                                        }
                                    }
                                },
                            );
                            if let Some(i) = self.toolbar_order.iter().position(|p| p == label) {
                                for (earlier, enabled, symbol) in [
                                    (true, i > 0, "↑"),
                                    (false, i + 1 < self.toolbar_order.len(), "↓"),
                                ] {
                                    let action_label = format!(
                                        "Move {label} {}",
                                        if earlier { "earlier" } else { "later" }
                                    );
                                    let response =
                                        ui.add_enabled(enabled, egui::Button::new(symbol));
                                    response.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::WidgetType::Button,
                                            ui.is_enabled() && enabled,
                                            &action_label,
                                        )
                                    });
                                    if response.on_hover_text(action_label).clicked() {
                                        self.move_toolbar_tool(label, earlier);
                                    }
                                }
                                ui.weak(format!("{}", i + 1));
                            }
                        });
                    }
                });
            ui.separator();
            if dialogs::secondary(ui, "Restore default toolbar", DialogIcon::Refresh).clicked() {
                self.toolbar_pins = default_toolbar();
                self.toolbar_order.clear();
                self.set_toolbar_order(Vec::new());
            }
            self.toolbar_changed |= before != self.toolbar_pins;
        });
        self.toolbar_customizer = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_toolbar_migration_preserves_order_and_later_customization() {
        let mut editor = EditorState::default();
        let pins = ["Arrow", "Pen", "Shapes", "Magnify", "Fill"]
            .map(str::to_owned)
            .to_vec();
        editor.set_toolbar(pins, true);
        assert_eq!(editor.toolbar_pins, ["Arrow", "Shapes", "Fill"]);
        assert!(editor.take_toolbar_change().is_some());
        let customized = ["Pen", "Arrow", "Magnify"].map(str::to_owned).to_vec();
        editor.set_toolbar(customized.clone(), false);
        assert_eq!(editor.toolbar_pins, customized);
        assert!(editor.take_toolbar_change().is_none());
        assert!(
            !default_toolbar()
                .iter()
                .any(|s| s == "Pen" || s == "Magnify")
        );
    }
    #[test]
    fn full_tool_order_migrates_and_survives_settings_and_document_changes() {
        let mut editor = EditorState::default();
        editor.set_toolbar(vec!["Text".into(), "Arrow".into()], false);
        editor.set_toolbar_order(Vec::new());
        assert_eq!(&editor.toolbar_order[..2], &["Text", "Arrow"]);
        let count = editor.tools().len();
        editor.move_toolbar_tool("Smart Erase / Repair", true);
        let (pins, order) = editor.take_toolbar_change().unwrap();
        assert_eq!(pins, ["Text", "Arrow"]);
        assert_eq!(order.len(), count);
        assert!(
            order.iter().position(|v| v == "Smart Erase / Repair")
                < order.iter().position(|v| v == "Connected Arrows")
        );
        let settings = crate::storage::Settings {
            editor_toolbar: pins.clone(),
            editor_tool_order: order.clone(),
            ..Default::default()
        };
        let restored: crate::storage::Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        let mut other = EditorState::default();
        other.set_toolbar(restored.editor_toolbar, false);
        other.set_toolbar_order(restored.editor_tool_order);
        assert!(other.take_toolbar_change().is_none());
        other.reset();
        assert_eq!(other.toolbar_order, order);
        assert_eq!(other.toolbar_pins, pins);
        other.set_toolbar_order(vec![
            "Pen".into(),
            "Pen".into(),
            "Unknown future tool".into(),
        ]);
        assert_eq!(other.toolbar_order[0], "Pen");
        assert_eq!(other.toolbar_order.len(), count);
        assert!(
            other
                .toolbar_order
                .iter()
                .all(|name| name != "Unknown future tool")
        );
        let legacy: crate::storage::Settings = serde_json::from_str("{}").unwrap();
        assert!(legacy.editor_tool_order.is_empty());
    }
}
