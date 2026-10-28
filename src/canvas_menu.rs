//! Compact icon actions and non-destructive image sizing for canvas menus.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum MenuIcon {
    App(DialogIcon),
    Glyph(inspector_widgets::Glyph),
    Tool(Tool),
    Rotate(bool),
    Flip(bool),
    Paste,
}
fn paint(ui: &egui::Ui, center: Pos2, icon: MenuIcon, color: Color32) {
    let p = ui.painter();
    let stroke = Stroke::new(1.5, color);
    match icon {
        MenuIcon::App(icon) => crate::appearance::icon(p, center, icon, color, 16.),
        MenuIcon::Glyph(glyph) => inspector_widgets::paint_glyph(p, center, glyph, color, 16.),
        MenuIcon::Tool(tool) => paint_tool_icon(p, center, tool, color),
        MenuIcon::Rotate(clockwise) => {
            let direction = if clockwise { 1. } else { -1. };
            let points: Vec<_> = (0..20)
                .map(|n| {
                    let angle = -0.3 + n as f32 / 19. * 4.6;
                    center + Vec2::new(angle.cos() * 7. * direction, angle.sin() * 7.)
                })
                .collect();
            let tip = points[19];
            let tangent = (tip - points[18]).normalized();
            p.add(egui::Shape::line(points, stroke));
            p.line_segment([tip - tangent * 4. + tangent.rot90() * 3., tip], stroke);
            p.line_segment([tip - tangent * 4. - tangent.rot90() * 3., tip], stroke);
        }
        MenuIcon::Flip(horizontal) => {
            let at = |x, y| {
                center
                    + if horizontal {
                        Vec2::new(x, y)
                    } else {
                        Vec2::new(y, x)
                    }
            };
            p.line_segment(
                [at(0., -8.), at(0., 8.)],
                Stroke::new(1., color.gamma_multiply(0.5)),
            );
            for side in [-1., 1.] {
                p.add(egui::Shape::convex_polygon(
                    vec![at(side * 3., -6.), at(side * 3., 6.), at(side * 8., 6.)],
                    color.gamma_multiply(0.18),
                    stroke,
                ));
            }
        }
        MenuIcon::Paste => {
            p.rect_stroke(
                Rect::from_center_size(center + Vec2::new(0., 1.), Vec2::new(12., 14.)),
                1.,
                stroke,
                StrokeKind::Inside,
            );
            p.rect_filled(
                Rect::from_center_size(center + Vec2::new(0., -6.), Vec2::new(7., 3.)),
                1.,
                color,
            );
            for y in [-1., 3.] {
                p.line_segment(
                    [center + Vec2::new(-3., y), center + Vec2::new(3., y)],
                    stroke,
                );
            }
        }
    }
}
pub(super) fn item(
    ui: &mut egui::Ui,
    label: &str,
    icon: MenuIcon,
    enabled: bool,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        let color = ui.visuals().text_color();
        let text =
            ui.painter()
                .layout_no_wrap(label.to_owned(), egui::FontId::proportional(13.), color);
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(text.size().x + 34., 24.), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
        });
        if response.hovered() || response.has_focus() {
            ui.painter()
                .rect_filled(rect, 3., ui.visuals().widgets.hovered.bg_fill);
        }
        paint(ui, rect.left_center() + Vec2::new(13., 0.), icon, color);
        ui.painter().galley(
            rect.left_center() + Vec2::new(28., -text.size().y / 2.),
            text,
            color,
        );
        response
    })
    .inner
}
fn scaled_size(document: &Document, percent: u32, relative: bool) -> [u32; 2] {
    let base = if relative {
        document.output_size()
    } else {
        document.geometry.rotated_size(original_size(document))
    };
    base.map(|value| {
        ((u64::from(value) * u64::from(percent) + 50) / 100).clamp(1, u64::from(u32::MAX)) as u32
    })
}
fn resize_geometry(document: &Document, size: [u32; 2]) -> ImageGeometry {
    let mut geometry = document.geometry;
    geometry.resize = (size != geometry.rotated_size(original_size(document))).then_some(size);
    geometry
}
impl EditorState {
    pub(super) fn open_resize(&mut self, document: &mut Document, history: &mut EditHistory) {
        self.finish_gesture(document, history);
        let size = document.output_size();
        self.resize = Some(ResizeDraft {
            size,
            ratio: size[0] as f64 / size[1] as f64,
            locked: true,
        });
    }
    fn quick_resize(
        &mut self,
        document: &mut Document,
        history: &mut EditHistory,
        percent: u32,
        relative: bool,
    ) -> bool {
        self.finish_gesture(document, history);
        self.apply_geometry(
            document,
            history,
            resize_geometry(document, scaled_size(document, percent, relative)),
        )
    }
    fn quick_resize_menu(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        ui.horizontal(|ui| {
            if item(ui, "Resize image…", MenuIcon::App(DialogIcon::Resize), true)
                .on_hover_text("Detailed resize · Command/Ctrl + Shift + R")
                .clicked()
            {
                self.open_resize(document, history);
                ui.close();
            }
            for (percent, symbol, label) in [
                (90, "−", "Decrease image size by 10%"),
                (110, "+", "Increase image size by 10%"),
            ] {
                let valid = resize_geometry(document, scaled_size(document, percent, true))
                    .validate(original_size(document))
                    .is_ok();
                let response =
                    ui.add_enabled(valid, egui::Button::new(symbol).min_size(Vec2::splat(24.)));
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        ui.is_enabled() && valid,
                        label,
                    )
                });
                if response.on_hover_text(label).clicked() {
                    self.quick_resize(document, history, percent, true);
                }
            }
        });
        let menu = ui.menu_button("     Resize presets", |ui| {
            for percent in [25, 50, 125, 150, 100] {
                let label = if percent == 100 {
                    "Set to original".to_owned()
                } else {
                    format!("{percent}%")
                };
                let geometry = resize_geometry(document, scaled_size(document, percent, false));
                if item(
                    ui,
                    &label,
                    MenuIcon::App(if percent == 100 {
                        DialogIcon::Refresh
                    } else {
                        DialogIcon::Resize
                    }),
                    geometry.validate(original_size(document)).is_ok(),
                )
                .on_hover_text(
                    "Percentage of the unscaled image; keeps the current crop and rotation",
                )
                .clicked()
                {
                    self.quick_resize(document, history, percent, false);
                    ui.close();
                }
            }
            ui.separator();
            if item(
                ui,
                "Detailed resize…",
                MenuIcon::App(DialogIcon::Settings),
                true,
            )
            .on_hover_text("Command/Ctrl + Shift + R")
            .clicked()
            {
                self.open_resize(document, history);
                ui.close();
            }
        });
        menu.response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), "Resize presets")
        });
        paint(
            ui,
            menu.response.rect.left_center() + Vec2::new(13., 0.),
            MenuIcon::App(DialogIcon::Resize),
            ui.visuals().text_color(),
        );
    }
    pub fn image_menu(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        ui.set_min_width(240.);
        if item(ui, "Canvas size…", MenuIcon::App(DialogIcon::Resize), true)
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
        if item(ui, "Crop", MenuIcon::Tool(Tool::Crop), true)
            .on_hover_text("Trim the canvas without losing original pixels")
            .clicked()
        {
            ui.close();
            self.finish_gesture(document, history);
            self.inspector.show(InspectorPanel::Styles);
            self.tool = Tool::Crop;
            self.clear_selection();
            self.crop = Some(CropDraft {
                rect: document.geometry.crop_rect(original_size(document)),
                drag_start: None,
            });
        }
        self.quick_resize_menu(ui, document, history);
        if item(ui, "Rotate left", MenuIcon::Rotate(false), true).clicked() {
            ui.close();
            self.rotate(document, history, false);
        }
        if item(ui, "Rotate right", MenuIcon::Rotate(true), true).clicked() {
            ui.close();
            self.rotate(document, history, true);
        }
        if item(ui, "Rotate 180°", MenuIcon::App(DialogIcon::Refresh), true).clicked() {
            self.finish_gesture(document, history);
            let mut geometry = document.geometry;
            geometry.quarter_turns = (geometry.quarter_turns + 2) % 4;
            self.apply_geometry(document, history, geometry);
            ui.close();
        }
        ui.separator();
        if item(ui, "Flip horizontal", MenuIcon::Flip(true), true).clicked() {
            self.flip(document, history, true);
            ui.close();
        }
        if item(ui, "Flip vertical", MenuIcon::Flip(false), true).clicked() {
            self.flip(document, history, false);
            ui.close();
        }
        ui.separator();
        if item(
            ui,
            "Reset image edits",
            MenuIcon::App(DialogIcon::Refresh),
            document.geometry != ImageGeometry::default(),
        )
        .on_hover_text(
            "Restore the original image dimensions, crop, and orientation; annotations are kept",
        )
        .clicked()
        {
            self.finish_gesture(document, history);
            self.apply_geometry(document, history, ImageGeometry::default());
            ui.close();
        }
    }

    pub(super) fn canvas_menu(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if item(ui, "Style Brush", MenuIcon::Tool(Tool::StyleBrush), true)
            .on_hover_text(
                "Copy an annotation’s appearance, then click compatible artwork to apply it",
            )
            .clicked()
        {
            self.start_style_brush(
                document,
                history,
                self.precision.context_source.or(self.selected),
            );
            ui.close();
        }
        ui.separator();
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
        if item(
            ui,
            "Select all annotations",
            MenuIcon::Glyph(inspector_widgets::Glyph::All),
            !document.annotations.is_empty(),
        )
        .clicked()
        {
            self.finish_gesture(document, history);
            self.tool = Tool::Select;
            self.inspector.select(InspectorPanel::Styles);
            self.set_selection((0..document.annotations.len()).collect(), document);
            self.focus_canvas = true;
            ui.close();
        }
        ui.separator();
        if item(ui, "Fit image", MenuIcon::App(DialogIcon::Fit), true).clicked() {
            self.fit = true;
            ui.close();
        }
        if item(
            ui,
            "Actual size (100%)",
            MenuIcon::App(DialogIcon::View),
            true,
        )
        .clicked()
        {
            self.fit = false;
            self.zoom = 1.;
            ui.close();
        }
        ui.horizontal(|ui| {
            let (icon, _) = ui.allocate_exact_size(Vec2::splat(20.), Sense::hover());
            crate::appearance::icon(
                ui.painter(),
                icon.center(),
                DialogIcon::Search,
                ui.visuals().text_color(),
                14.,
            );
            ui.label("Zoom");
            if crate::appearance::zoom_slider(ui, &mut self.zoom, 0.05..=8.).changed() {
                self.fit = false;
            }
            ui.small(format!("{:.0}%", self.zoom * 100.));
        });
    }

    pub(super) fn edit_actions(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if item(ui, "Undo", MenuIcon::Rotate(false), history.can_undo())
            .on_hover_text("Undo the last change (Command/Ctrl + Z)")
            .clicked()
        {
            self.finish_gesture(document, history);
            history.undo(document);
            self.clear_selection();
            self.area = None;
            ui.close();
        }
        if item(ui, "Redo", MenuIcon::Rotate(true), history.can_redo())
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
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resize_steps_presets_and_original_preserve_pixels_crop_rotation_and_annotations() {
        for turns in 0..4 {
            let mut d = Document {
                image: std::sync::Arc::new(image::RgbaImage::new(400, 300)),
                annotations: vec![crate::markup_tests::object(
                    AnnotationKind::Arrow,
                    [20., 30.],
                    [180., 140.],
                )],
                geometry: ImageGeometry {
                    crop: Some(CropRect {
                        x: 10,
                        y: 20,
                        width: 200,
                        height: 100,
                    }),
                    quarter_turns: turns,
                    flip_horizontal: true,
                    ..Default::default()
                },
            };
            let original = d.image.clone();
            let before = d.edit_state();
            let mut h = EditHistory::new(before.clone());
            let mut e = EditorState::default();
            let unscaled = d.output_size();
            assert!(e.quick_resize(&mut d, &mut h, 110, true));
            assert_eq!(d.output_size(), unscaled.map(|v| v * 110 / 100));
            assert!(e.quick_resize(&mut d, &mut h, 110, true));
            assert_eq!(d.output_size(), unscaled.map(|v| v * 121 / 100));
            for percent in [25, 50, 125, 150, 100] {
                let prior = d.edit_state();
                assert!(e.quick_resize(&mut d, &mut h, percent, false));
                assert_eq!(d.output_size(), unscaled.map(|v| (v * percent + 50) / 100));
                assert_eq!(d.annotations, before.annotations);
                assert_eq!(d.geometry.crop, before.geometry.crop);
                assert_eq!(d.geometry.quarter_turns, turns);
                assert!(std::sync::Arc::ptr_eq(&d.image, &original));
                let after = d.edit_state();
                assert!(h.undo(&mut d));
                assert_eq!(d.edit_state(), prior);
                assert!(h.redo(&mut d));
                assert_eq!(d.edit_state(), after);
            }
            assert_eq!(d.edit_state(), before);
        }
    }
    #[test]
    fn quick_resize_rejects_oversize_without_editing_and_bounds_small_images() {
        let mut d = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(1, 1)),
            annotations: vec![],
            geometry: ImageGeometry {
                resize: Some([32768, 1]),
                ..Default::default()
            },
        };
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState::default();
        assert!(!e.quick_resize(&mut d, &mut h, 110, true));
        assert_eq!(d.edit_state(), before);
        assert!(!h.can_undo());
        assert!(e.quick_resize(&mut d, &mut h, 25, false));
        assert_eq!(d.output_size(), [1, 1]);
    }
}
