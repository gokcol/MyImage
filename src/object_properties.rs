//! Compact contextual controls, shared by the inspector and on-canvas toolbar.
use super::*;
use crate::arrow_style::{Endpoint, Pattern};
use crate::text_style::Alignment;

fn property_button_menu<R>(
    ui: &mut egui::Ui,
    button: egui::Button<'_>,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let config = egui::containers::menu::MenuConfig::new()
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    let (response, inner) = if egui::containers::menu::is_in_menu(ui) {
        egui::containers::menu::SubMenuButton::from_button(button)
            .config(config)
            .ui(ui, content)
    } else {
        egui::containers::menu::MenuButton::from_button(button)
            .config(config)
            .ui(ui, content)
    };
    egui::InnerResponse::new(inner.map(|r| r.inner), response)
}
fn property_menu<R>(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    property_button_menu(ui, egui::Button::new(label.into()), content)
}

const COLORS: [(&str, [u8; 3]); 8] = [
    ("Red", [230, 66, 55]),
    ("Blue", [35, 112, 225]),
    ("Teal", [0, 137, 146]),
    ("Amber", [246, 172, 36]),
    ("Violet", [127, 82, 209]),
    ("Pink", [220, 66, 137]),
    ("Ink", [29, 41, 56]),
    ("White", [255; 3]),
];

pub(super) fn palette(ui: &mut egui::Ui, color: &mut [u8; 4]) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.;
        for (name, rgb) in COLORS {
            let selected = color[..3] == rgb;
            let (rect, response) = ui.allocate_exact_size(Vec2::splat(23.), Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::SelectableLabel,
                    ui.is_enabled(),
                    selected,
                    name,
                )
            });
            ui.painter().circle_filled(
                rect.center(),
                8.,
                Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
            );
            ui.painter().circle_stroke(
                rect.center(),
                if selected { 10.5 } else { 8. },
                Stroke::new(
                    1.,
                    if selected {
                        crate::appearance::ACCENT
                    } else {
                        Color32::from_gray(205)
                    },
                ),
            );
            if response.on_hover_text(name).clicked() {
                color[..3].copy_from_slice(&rgb);
                changed = true;
            }
        }
        if !egui::containers::menu::is_in_menu(ui) {
            changed |= ui
                .color_edit_button_srgba_unmultiplied(color)
                .on_hover_text("Custom color")
                .changed();
        }
    });
    changed
}

pub(super) fn slider<N: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    label: &str,
    suffix: &str,
) -> bool {
    ui.push_id(label, |ui| {
        if label.len() > 15 {
            ui.label(label);
            ui.spacing_mut().slider_width = (ui.available_width() - 78.).clamp(64., 170.);
            crate::appearance::value_slider(ui, value, range, label, suffix).changed()
        } else {
            ui.horizontal(|ui| {
                ui.add_sized([66., 24.], egui::Label::new(label));
                ui.spacing_mut().slider_width = (ui.available_width() - 70.).clamp(60., 170.);
                crate::appearance::value_slider(ui, value, range, label, suffix).changed()
            })
            .inner
        }
    })
    .inner
}

pub(super) fn font_size_menu(ui: &mut egui::Ui, value: &mut f32) -> bool {
    let mut changed = false;
    if egui::containers::menu::is_in_menu(ui) {
        ui.horizontal_wrapped(|ui| {
            for size in [12., 16., 24., 32., 48., 64.] {
                if ui
                    .selectable_label(*value == size, format!("{size:.0}"))
                    .clicked()
                {
                    *value = size;
                    changed = true;
                }
            }
        });
        return changed;
    }
    property_menu(ui, format!("{value:.0} px ▾"), |ui| {
        ui.set_min_width(216.);
        ui.horizontal_wrapped(|ui| {
            for size in [12., 16., 20., 24., 32., 48., 64., 96.] {
                if ui
                    .selectable_label(*value == size, format!("{size:.0}"))
                    .clicked()
                {
                    *value = size;
                    changed = true;
                    ui.close();
                }
            }
        });
        changed |= slider(
            ui,
            value,
            1.0..=crate::document::MAX_FONT_SIZE,
            "Font size",
            " px",
        );
    });
    changed
}

fn color_menu(ui: &mut egui::Ui, color: &mut [u8; 4], label: &str) -> bool {
    let mut changed = false;
    ui.push_id(label, |ui| {
        property_menu(
            ui,
            egui::RichText::new("■ ▾")
                .color(Color32::from_rgb(color[0], color[1], color[2]))
                .size(18.),
            |ui| {
                ui.label(label);
                changed |= palette(ui, color);
            },
        )
        .response
        .on_hover_text(label);
    });
    changed
}

fn opacity(ui: &mut egui::Ui, color: &mut [u8; 4]) -> bool {
    let mut value = color[3] as f32 / 255. * 100.;
    if slider(ui, &mut value, 0.0..=100.0, "Opacity", "%") {
        color[3] = (value * 2.55).round() as u8;
        true
    } else {
        false
    }
}

fn paint_endpoint(ui: &egui::Ui, rect: Rect, cap: Endpoint, start: bool) {
    let color = ui.visuals().text_color();
    let p = ui.painter();
    let center = rect.center();
    let left = Pos2::new(rect.left() + 4., center.y);
    let right = Pos2::new(rect.right() - 4., center.y);
    p.line_segment([left, right], Stroke::new(2., color));
    let tip = if start { left } else { right };
    let dir = if start { -1. } else { 1. };
    let base = tip - Vec2::new(dir * 9., 0.);
    let top = base + Vec2::new(0., -4.5);
    let bottom = base + Vec2::new(0., 4.5);
    match cap {
        Endpoint::None => {}
        Endpoint::Triangle => {
            p.add(egui::Shape::convex_polygon(
                vec![tip, top, bottom],
                color,
                Stroke::NONE,
            ));
        }
        Endpoint::Open => {
            p.add(egui::Shape::line(
                vec![top, tip, bottom],
                Stroke::new(2., color),
            ));
        }
        Endpoint::Circle => {
            p.circle_filled(tip - Vec2::new(dir * 4., 0.), 4.5, color);
        }
        Endpoint::Diamond => {
            p.add(egui::Shape::convex_polygon(
                vec![
                    tip,
                    tip - Vec2::new(dir * 5., -4.5),
                    tip - Vec2::new(dir * 10., 0.),
                    tip - Vec2::new(dir * 5., 4.5),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Endpoint::Bar => {
            p.line_segment(
                [tip + Vec2::new(0., -5.), tip + Vec2::new(0., 5.)],
                Stroke::new(2.5, color),
            );
        }
    }
}

fn paint_pattern(ui: &egui::Ui, rect: Rect, pattern: Pattern) {
    let color = ui.visuals().text_color();
    let start = Pos2::new(rect.left() + 3., rect.center().y);
    for (lo, hi) in pattern.spans((rect.width() - 6.).max(0.), 1.5) {
        if hi - lo < 0.1 {
            ui.painter()
                .circle_filled(start + Vec2::new(lo, 0.), 1.25, color);
        } else {
            ui.painter().line_segment(
                [start + Vec2::new(lo, 0.), start + Vec2::new(hi, 0.)],
                Stroke::new(2.5, color),
            );
        }
    }
}

fn pattern_choices(ui: &mut egui::Ui, value: &mut Pattern) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.;
        for pattern in Pattern::ALL {
            let response = ui.add(
                egui::Button::new("")
                    .selected(*value == pattern)
                    .min_size(Vec2::new(43., 28.)),
            );
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::SelectableLabel,
                    ui.is_enabled(),
                    *value == pattern,
                    pattern.name(),
                )
            });
            paint_pattern(ui, response.rect.shrink(3.), pattern);
            if response.on_hover_text(pattern.name()).clicked() {
                changed |= *value != pattern;
                *value = pattern;
            }
        }
    });
    changed
}

fn fill_choices(ui: &mut egui::Ui, fill: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for (value, name) in [(false, "Outline"), (true, "Solid fill")] {
            let r = ui.add(
                egui::Button::new("")
                    .selected(*fill == value)
                    .min_size(Vec2::new(43., 28.)),
            );
            r.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::SelectableLabel,
                    ui.is_enabled(),
                    *fill == value,
                    name,
                )
            });
            let rect = Rect::from_center_size(r.rect.center(), Vec2::new(24., 16.));
            let color = ui.visuals().text_color();
            ui.painter().rect(
                rect,
                2,
                if value { color } else { Color32::TRANSPARENT },
                Stroke::new(2., color),
                egui::StrokeKind::Inside,
            );
            if r.on_hover_text(name).clicked() {
                changed |= *fill != value;
                *fill = value;
            }
        }
    });
    changed
}

fn pattern_menu(ui: &mut egui::Ui, value: &mut Pattern) -> bool {
    let mut changed = false;
    let response = property_button_menu(
        ui,
        egui::Button::new(
            egui::RichText::new(value.name())
                .size(1.)
                .color(Color32::TRANSPARENT),
        )
        .min_size(Vec2::new(77., 26.)),
        |ui| {
            changed |= pattern_choices(ui, value);
        },
    )
    .response;
    paint_pattern(
        ui,
        Rect::from_min_max(
            response.rect.min + Vec2::new(2., 0.),
            response.rect.max - Vec2::new(23., 0.),
        ),
        *value,
    );
    ui.painter().add(egui::Shape::line(
        vec![
            response.rect.right_center() + Vec2::new(-15., -2.),
            response.rect.right_center() + Vec2::new(-11., 2.),
            response.rect.right_center() + Vec2::new(-7., -2.),
        ],
        Stroke::new(1.5, ui.visuals().text_color()),
    ));
    response.on_hover_text("Line style");
    changed
}

fn endpoint_menu(ui: &mut egui::Ui, value: &mut Endpoint, start: bool) -> bool {
    let mut changed = false;
    let response = ui
        .push_id(if start { "line-start" } else { "line-end" }, |ui| {
            property_button_menu(
                ui,
                egui::Button::new(
                    egui::RichText::new(value.name())
                        .size(1.)
                        .color(Color32::TRANSPARENT),
                )
                .min_size(Vec2::new(65., 26.)),
                |ui| {
                    ui.set_min_width(150.);
                    for cap in Endpoint::ALL {
                        let response = ui.selectable_value(value, cap, cap.name());
                        changed |= response.changed();
                        let icon = Rect::from_min_max(
                            Pos2::new(response.rect.right() - 50., response.rect.top()),
                            response.rect.right_bottom(),
                        );
                        paint_endpoint(ui, icon, cap, start);
                    }
                },
            )
            .response
        })
        .inner;
    paint_endpoint(
        ui,
        Rect::from_min_max(
            response.rect.min + Vec2::new(2., 0.),
            response.rect.max - Vec2::new(23., 0.),
        ),
        *value,
        start,
    );
    ui.painter().add(egui::Shape::line(
        vec![
            response.rect.right_center() + Vec2::new(-15., -2.),
            response.rect.right_center() + Vec2::new(-11., 2.),
            response.rect.right_center() + Vec2::new(-7., -2.),
        ],
        Stroke::new(1.5, ui.visuals().text_color()),
    ));
    response.on_hover_text(if start {
        "Start endpoint"
    } else {
        "End endpoint"
    });
    changed
}

fn endpoint_choices(ui: &mut egui::Ui, value: &mut Endpoint, start: bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for cap in Endpoint::ALL {
            let (rect, response) = ui.allocate_exact_size(Vec2::new(35., 26.), Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::SelectableLabel,
                    ui.is_enabled(),
                    *value == cap,
                    cap.name(),
                )
            });
            if *value == cap || response.hovered() {
                ui.painter().rect_filled(
                    rect,
                    4,
                    if *value == cap {
                        ui.visuals().selection.bg_fill
                    } else {
                        ui.visuals().faint_bg_color
                    },
                );
            }
            paint_endpoint(ui, rect.shrink(3.), cap, start);
            if response.on_hover_text(cap.name()).clicked() {
                *value = cap;
                changed = true;
            }
        }
    });
    changed
}
fn arrow_presets(ui: &mut egui::Ui, arrow: &mut crate::arrow_style::ArrowStyle) -> bool {
    let mut changed = false;
    ui.label("Start");
    changed |= endpoint_choices(ui, &mut arrow.start, true);
    ui.label("Line");
    changed |= pattern_choices(ui, &mut arrow.pattern);
    ui.label("End");
    changed |= endpoint_choices(ui, &mut arrow.end, false);
    changed
}

fn arrow_controls(ui: &mut egui::Ui, arrow: &mut crate::arrow_style::ArrowStyle) -> bool {
    let mut changed = false;
    ui.small("Drag the yellow points on the arrow to reshape it.");
    if ui
        .add_enabled(arrow.curve.is_some(), egui::Button::new("Straighten"))
        .clicked()
    {
        arrow.curve = None;
        changed = true;
    }
    if egui::containers::menu::is_in_menu(ui) {
        changed |= arrow_presets(ui, arrow);
    } else {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label("Start");
                changed |= endpoint_menu(ui, &mut arrow.start, true);
            });
            ui.vertical(|ui| {
                ui.label("Line");
                changed |= pattern_menu(ui, &mut arrow.pattern);
            });
            ui.vertical(|ui| {
                ui.label("End");
                changed |= endpoint_menu(ui, &mut arrow.end, false);
            });
        });
    }
    ui.add_enabled_ui(arrow.start != Endpoint::None, |ui| {
        changed |= slider(ui, &mut arrow.start_size, 1.0..=8.0, "Start size", "×");
    });
    ui.add_enabled_ui(arrow.end != Endpoint::None, |ui| {
        changed |= slider(ui, &mut arrow.end_size, 1.0..=8.0, "End size", "×");
    });
    ui.horizontal(|ui| {
        if ui
            .small_button("Line")
            .on_hover_text("Plain line without endpoints")
            .clicked()
        {
            arrow.start = Endpoint::None;
            arrow.end = Endpoint::None;
            changed = true;
        }
        if ui.small_button("Arrow").clicked() {
            arrow.start = Endpoint::None;
            arrow.end = Endpoint::Triangle;
            changed = true;
        }
        if ui.small_button("Double arrow").clicked() {
            arrow.start = Endpoint::Triangle;
            arrow.end = Endpoint::Triangle;
            changed = true;
        }
    });
    changed
}

fn text_controls(
    ui: &mut egui::Ui,
    style: &mut crate::text_style::TextStyle,
    size: &mut f32,
) -> bool {
    let before = style.clone();
    let mut changed = false;
    if egui::containers::menu::is_in_menu(ui) {
        ui.horizontal(|ui| {
            changed |= ui
                .selectable_value(&mut style.monospace, false, "Sans serif")
                .changed();
            changed |= ui
                .selectable_value(&mut style.monospace, true, "Monospace")
                .changed();
        });
        changed |= font_size_menu(ui, size);
    } else {
        ui.horizontal(|ui| {
            property_menu(
                ui,
                if style.monospace {
                    "Monospace ▾"
                } else {
                    "Sans serif ▾"
                },
                |ui| {
                    ui.selectable_value(&mut style.monospace, false, "Sans serif");
                    ui.selectable_value(&mut style.monospace, true, "Monospace");
                },
            );
            changed |= font_size_menu(ui, size);
        });
    }
    changed |= slider(
        ui,
        size,
        1.0..=crate::document::MAX_FONT_SIZE,
        "Font size",
        " px",
    );
    ui.horizontal(|ui| {
        ui.toggle_value(&mut style.italic, egui::RichText::new("I").italics())
            .on_hover_text("Italic");
        ui.toggle_value(&mut style.underline, egui::RichText::new("U").underline())
            .on_hover_text("Underline");
        ui.separator();
        ui.selectable_value(&mut style.alignment, Alignment::Left, "Left");
        ui.selectable_value(&mut style.alignment, Alignment::Center, "Center");
        ui.selectable_value(&mut style.alignment, Alignment::Right, "Right");
    });
    changed || *style != before
}

impl EditorState {
    pub(super) fn record_properties(
        &mut self,
        before: EditState,
        document: &Document,
        history: &mut EditHistory,
    ) {
        if self.property_interacting {
            if self.property_before.is_none() {
                self.property_before = Some(before);
            }
        } else {
            history.commit(self.property_before.take().unwrap_or(before), document);
        }
    }

    pub(super) fn finish_properties(&mut self, document: &Document, history: &mut EditHistory) {
        if let Some(before) = self.property_before.take() {
            history.commit(before, document);
        }
    }

    pub(super) fn object_controls(&mut self, ui: &mut egui::Ui, kind: AnnotationKind) -> bool {
        let mut changed = false;
        if kind.is_effect() {
            ui.horizontal(|ui| {
                ui.weak(if kind == AnnotationKind::Blur {
                    "Blur"
                } else {
                    "Pixelate"
                });
            });
            return slider(
                ui,
                &mut self.effect_size,
                2..=crate::document::MAX_EFFECT_SIZE,
                if kind == AnnotationKind::Blur {
                    "Blur strength"
                } else {
                    "Block size"
                },
                " px",
            );
        }
        ui.label(
            if matches!(kind, AnnotationKind::Step | AnnotationKind::Callout) {
                "Fill color"
            } else {
                "Color"
            },
        );
        changed |= palette(ui, &mut self.color);
        changed |= opacity(ui, &mut self.color);
        if kind == AnnotationKind::Arrow || kind.is_shape() || kind == AnnotationKind::Freehand {
            let maximum = self.stroke_width.max(64.);
            changed |= slider(ui, &mut self.stroke_width, 0.1..=maximum, "Width", " px");
            ui.horizontal(|ui| {
                for width in [1., 2., 4., 8., 12., 24.] {
                    if ui
                        .selectable_label(self.stroke_width == width, format!("{width:.0}"))
                        .on_hover_text(format!("{width:.0} px"))
                        .clicked()
                    {
                        self.stroke_width = width;
                        changed = true;
                    }
                }
            });
        }
        if kind == AnnotationKind::Arrow {
            changed |= arrow_controls(ui, &mut self.arrow);
        }
        if kind.is_shape() {
            ui.label("Line");
            changed |= pattern_choices(ui, &mut self.arrow.pattern);
            ui.horizontal(|ui| {
                ui.label("Fill");
                changed |= fill_choices(ui, &mut self.fill);
            });
        }

        if matches!(kind, AnnotationKind::Callout | AnnotationKind::Step) {
            ui.label("Text color");
            changed |= palette(ui, &mut self.text_color);
        }
        if kind == AnnotationKind::Step {
            if self.selected.is_none() {
                ui.small("Numbers advance automatically with each click.");
            }
            ui.add_enabled_ui(self.selected.is_some(), |ui| {
                ui.horizontal(|ui| {
                    ui.label("Number");
                    if ui
                        .add_enabled(self.step_number > 1, egui::Button::new("−"))
                        .clicked()
                    {
                        self.step_number -= 1;
                        changed = true;
                    }
                    if egui::containers::menu::is_in_menu(ui) {
                        ui.label(self.step_number.to_string());
                    } else {
                        egui::ComboBox::from_id_salt("step-number")
                            .width(60.)
                            .selected_text(self.step_number.to_string())
                            .show_ui(ui, |ui| {
                                for n in 1..=30 {
                                    changed |= ui
                                        .selectable_value(&mut self.step_number, n, n.to_string())
                                        .changed();
                                }
                            });
                    }
                    if ui
                        .add_enabled(
                            self.step_number < crate::document::MAX_STEP_NUMBER,
                            egui::Button::new("+"),
                        )
                        .clicked()
                    {
                        self.step_number += 1;
                        changed = true;
                    }
                });
            });
            let maximum = self.step_diameter.max(512.);
            changed |= slider(
                ui,
                &mut self.step_diameter,
                1.0..=maximum,
                "Diameter",
                " px",
            );
        }
        if matches!(kind, AnnotationKind::Text | AnnotationKind::Callout) {
            changed |= text_controls(ui, &mut self.text_style, &mut self.font_size);
        }
        changed
    }

    pub(super) fn select_same_kind(&mut self, document: &Document, kind: AnnotationKind) {
        // Hidden/locked objects stay out of a style-editing selection.
        let indices: Vec<_> = document
            .annotations
            .iter()
            .enumerate()
            .filter_map(|(i, a)| (a.kind == kind && a.visible() && !a.locked()).then_some(i))
            .collect();
        self.tool = Tool::Area;
        self.deselect_pixels();
        self.set_selection(indices, document);
    }

    pub(super) fn selection_footer(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let indices = self.selected_indices(document);
        if let Some(kind) = self.style_kind(document) {
            let count = document
                .annotations
                .iter()
                .filter(|a| a.kind == kind && a.visible() && !a.locked())
                .count();
            if ui
                .add_enabled(
                    count > 0,
                    egui::Button::new(format!(
                        "Select all {} ({count})",
                        quick_styles::kind_name(kind)
                    ))
                    .min_size(Vec2::new(ui.available_width(), 26.)),
                )
                .clicked()
            {
                self.select_same_kind(document, kind);
            }
        }
        if !indices.is_empty() {
            ui.horizontal(|ui| {
                ui.menu_button("Arrange ▾", |ui| {
                    self.selection_controls(ui, document, history);
                });
                ui.menu_button("More ▾", |ui| {
                    self.privacy_actions(ui, document, history);
                    ui.separator();
                    if ui.button("Open layers").clicked() {
                        self.layers_open = true;
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            self.can_edit_selection(document),
                            egui::Button::new("Delete selected"),
                        )
                        .clicked()
                    {
                        self.delete(document, history);
                        ui.close();
                    }
                });
            });
        }
    }

    pub(super) fn floating_properties(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
        origin: Pos2,
        scale: f32,
        viewport: Rect,
    ) {
        if !ui.is_enabled()
            || matches!(self.tool, Tool::Step | Tool::Pen | Tool::Eraser)
            || self.gesture.is_some()
            || self.area_drag.is_some()
            || self.has_modal()
            || self.crop.is_some()
        {
            return;
        }
        let indices = self.selected_indices(document);
        if indices.is_empty()
            || !self.can_edit_selection(document)
            || indices.iter().any(|&i| !document.annotations[i].visible())
        {
            return;
        }
        let Some(index) = self.selected else {
            return;
        };
        let kind = document.annotations[index].kind;
        if kind.is_area_edit() {
            return;
        }
        let mut bounds = Rect::NOTHING;
        for &i in &indices {
            bounds = bounds.union(mapped_bounds(
                render::annotation_bounds(&document.annotations[i]),
                document,
            ));
        }
        let screen = Rect::from_min_max(
            origin + bounds.min.to_vec2() * scale,
            origin + bounds.max.to_vec2() * scale,
        );
        if !screen.intersects(viewport) {
            return;
        }
        let position = floating_position(screen, viewport);
        let mut changed = false;
        egui::Area::new(egui::Id::new("selected-object-properties"))
            .order(egui::Order::Foreground)
            .fixed_pos(position)
            .movable(false)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .fill(Color32::WHITE)
                    .corner_radius(9)
                    .inner_margin(5)
                    .stroke(Stroke::new(1., Color32::from_rgb(204, 218, 228)))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 4.;
                        ui.spacing_mut().button_padding = Vec2::new(6., 5.);
                        ui.spacing_mut().interact_size.y = 26.;
                        ui.horizontal(|ui| {
                            if indices.len() > 1 {
                                ui.weak(format!("{} selected", indices.len()));
                                ui.menu_button("Arrange ▾", |ui| {
                                    self.selection_controls(ui, document, history);
                                });
                                return;
                            }
                            if kind == AnnotationKind::Stamp {
                                property_menu(ui, "Size ▾", |ui| {
                                    ui.label("Scale stamp");
                                    for (label, factor) in [
                                        ("Smaller · 75%", 0.75),
                                        ("Larger · 125%", 1.25),
                                        ("Double · 200%", 2.),
                                    ] {
                                        if ui.button(label).clicked() {
                                            let a = &document.annotations[index];
                                            let size = mapped_bounds(
                                                Rect::from_two_pos(point(a.start), point(a.end)),
                                                document,
                                            )
                                            .size()
                                                * factor;
                                            if let Err(e) = stamp_panel::resize_stamp(
                                                document, history, index, size,
                                            ) {
                                                self.image_error = Some(e);
                                            }
                                            self.select(Some(index), document);
                                            ui.close();
                                        }
                                    }
                                });
                            } else if !kind.is_effect() && kind != AnnotationKind::Spotlight {
                                changed |= color_menu(ui, &mut self.color, "Color");
                            }
                            if kind == AnnotationKind::Arrow
                                || kind.is_shape()
                                || kind == AnnotationKind::Freehand
                            {
                                property_menu(
                                    ui,
                                    format!("{:.0} px ▾", self.stroke_width),
                                    |ui| {
                                        ui.set_min_width(220.);
                                        let maximum = self.stroke_width.max(64.);
                                        changed |= slider(
                                            ui,
                                            &mut self.stroke_width,
                                            0.1..=maximum,
                                            "Width",
                                            " px",
                                        );
                                        ui.horizontal(|ui| {
                                            for width in [1., 2., 4., 8., 12., 24.] {
                                                if ui.button(format!("{width:.0}")).clicked() {
                                                    self.stroke_width = width;
                                                    changed = true;
                                                    ui.close();
                                                }
                                            }
                                        });
                                    },
                                )
                                .response
                                .on_hover_text("Line width");
                            }
                            if kind == AnnotationKind::Arrow {
                                property_menu(ui, "Style ▾", |ui| {
                                    ui.set_min_width(244.);
                                    changed |= arrow_controls(ui, &mut self.arrow);
                                });
                            } else if kind.is_shape() {
                                changed |= pattern_menu(ui, &mut self.arrow.pattern);
                                property_menu(ui, "Shape ▾", |ui| {
                                    ui.set_min_width(250.);
                                    self.shape_picker(ui, document, history);
                                });
                                property_menu(
                                    ui,
                                    if self.fill {
                                        "Solid ▾"
                                    } else {
                                        "Outline ▾"
                                    },
                                    |ui| {
                                        changed |= fill_choices(ui, &mut self.fill);
                                    },
                                );
                            } else if matches!(kind, AnnotationKind::Text | AnnotationKind::Callout)
                            {
                                property_menu(ui, "Aa ▾", |ui| {
                                    ui.set_min_width(244.);
                                    changed |= text_controls(
                                        ui,
                                        &mut self.text_style,
                                        &mut self.font_size,
                                    );
                                    if kind == AnnotationKind::Callout {
                                        changed |= palette(ui, &mut self.text_color);
                                    }
                                    if ui.button("Edit text…").clicked() {
                                        self.finish_properties(document, history);
                                        self.open_text_editor(
                                            index,
                                            document.edit_state(),
                                            document,
                                        );
                                        ui.close();
                                    }
                                });
                            } else if kind.is_effect() {
                                property_menu(
                                    ui,
                                    if kind == AnnotationKind::Blur {
                                        "Blur ▾"
                                    } else {
                                        "Pixelate ▾"
                                    },
                                    |ui| {
                                        for (label, new_kind) in [
                                            ("Blur", AnnotationKind::Blur),
                                            ("Pixelate", AnnotationKind::Pixelate),
                                        ] {
                                            if ui
                                                .selectable_label(kind == new_kind, label)
                                                .clicked()
                                            {
                                                let before = document.edit_state();
                                                document.annotations[index].kind = new_kind;
                                                history.commit(before, document);
                                                self.select(Some(index), document);
                                                ui.close();
                                            }
                                        }
                                    },
                                );
                                property_menu(ui, format!("{} px ▾", self.effect_size), |ui| {
                                    ui.set_min_width(220.);
                                    changed |= slider(
                                        ui,
                                        &mut self.effect_size,
                                        2..=crate::document::MAX_EFFECT_SIZE,
                                        "Intensity",
                                        " px",
                                    );
                                });
                            } else if kind == AnnotationKind::Step {
                                property_menu(
                                    ui,
                                    format!("{} · Size ▾", self.step_number),
                                    |ui| {
                                        ui.set_min_width(244.);
                                        changed |= self.object_controls(ui, kind);
                                    },
                                );
                            } else if kind.is_studio() {
                                property_menu(ui, "Effect ▾", |ui| {
                                    ui.set_min_width(244.);
                                    if kind == AnnotationKind::Magnify {
                                        changed |= slider(
                                            ui,
                                            &mut self.studio.zoom,
                                            1.0..=8.0,
                                            "Zoom",
                                            "×",
                                        );
                                    }
                                    if kind == AnnotationKind::Spotlight {
                                        changed |= slider(
                                            ui,
                                            &mut self.studio.dim,
                                            0..=100,
                                            "Dim surroundings",
                                            "%",
                                        );
                                        changed |= slider(
                                            ui,
                                            &mut self.studio.blur,
                                            0..=128,
                                            "Blur surroundings",
                                            " px",
                                        );
                                    }
                                    changed |=
                                        ui.checkbox(&mut self.studio.ellipse, "Oval").changed();
                                });
                            }
                            if !kind.is_effect() && kind != AnnotationKind::Spotlight {
                                property_menu(
                                    ui,
                                    format!("{:.0}% ▾", self.color[3] as f32 / 255. * 100.),
                                    |ui| {
                                        ui.set_min_width(220.);
                                        changed |= opacity(ui, &mut self.color);
                                    },
                                )
                                .response
                                .on_hover_text("Opacity");
                            }
                            property_menu(ui, "•••", |ui| {
                                self.selection_controls(ui, document, history);
                                if ui.button("Delete").clicked() {
                                    self.delete(document, history);
                                    ui.close();
                                }
                            });
                        });
                    });
            });
        if changed {
            self.apply_inspector(document, history);
            ui.ctx().request_repaint();
        }
    }
}

fn floating_position(selection: Rect, viewport: Rect) -> Pos2 {
    let width = 330.;
    let height = 40.;
    let x = selection
        .right()
        .min(viewport.right() - width - 8.)
        .max(viewport.left() + 8.);
    let above = selection.top() - height - 14.;
    let y = if above >= viewport.top() + 8. {
        above
    } else {
        selection.bottom() + 14.
    };
    Pos2::new(
        x,
        y.clamp(
            viewport.top() + 8.,
            (viewport.bottom() - height - 8.).max(viewport.top() + 8.),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> Document {
        Document {
            image: std::sync::Arc::new(image::RgbaImage::from_pixel(
                700,
                500,
                image::Rgba([255; 4]),
            )),
            annotations: vec![],
            geometry: Default::default(),
        }
    }

    #[test]
    fn slider_gesture_is_one_undo_and_keeps_object_position() {
        let mut e = EditorState::default();
        let mut d = document();
        let mut a = e.new_annotation(AnnotationKind::Arrow, Point { x: 50., y: 90. });
        a.end = Point { x: 400., y: 200. };
        d.annotations.push(a);
        let original = d.edit_state();
        let mut h = EditHistory::new(original.clone());
        e.select(Some(0), &d);
        e.property_interacting = true;
        for width in [6., 10., 14., 20.] {
            e.stroke_width = width;
            e.apply_inspector(&mut d, &mut h);
        }
        e.property_interacting = false;
        e.finish_properties(&d, &mut h);
        assert_eq!(d.annotations[0].stroke_width, 20.);
        assert_eq!(d.annotations[0].start, original.annotations[0].start);
        assert_eq!(d.annotations[0].end, original.annotations[0].end);
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), original);
        assert!(!h.can_undo());
        assert!(h.redo(&mut d));
        assert_eq!(d.annotations[0].stroke_width, 20.);
    }

    #[test]
    fn selected_object_properties_sync_and_protect_locked_and_multiple_objects() {
        let mut e = EditorState::default();
        let mut d = document();
        for kind in [
            AnnotationKind::Arrow,
            AnnotationKind::Text,
            AnnotationKind::Callout,
            AnnotationKind::Highlighter,
            AnnotationKind::Freehand,
            AnnotationKind::Step,
            AnnotationKind::Blur,
            AnnotationKind::Magnify,
        ] {
            d.annotations
                .push(e.new_annotation(kind, Point { x: 40., y: 50. }));
        }
        let mut h = EditHistory::new(d.edit_state());
        e.select(Some(1), &d);
        e.font_size = 48.;
        e.text_style.italic = true;
        e.text_style.alignment = Alignment::Center;
        e.apply_inspector(&mut d, &mut h);
        e.select(Some(0), &d);
        e.arrow.start = Endpoint::Circle;
        e.arrow.pattern = Pattern::Dashed;
        e.apply_inspector(&mut d, &mut h);
        e.select(Some(1), &d);
        assert_eq!(e.font_size, 48.);
        assert!(e.text_style.italic);
        assert_eq!(e.text_style.alignment, Alignment::Center);
        e.select(Some(0), &d);
        assert_eq!(e.arrow.start, Endpoint::Circle);
        assert_eq!(e.arrow.pattern, Pattern::Dashed);
        d.annotations[0].layer.locked = true;
        let before = d.edit_state();
        e.stroke_width = 24.;
        e.apply_inspector(&mut d, &mut h);
        assert_eq!(d.edit_state(), before);
        d.annotations[0].layer.locked = false;
        e.set_selection(vec![0, 1], &d);
        let before = d.edit_state();
        e.color = [0; 4];
        e.apply_inspector(&mut d, &mut h);
        assert_eq!(d.edit_state(), before);
    }

    #[test]
    fn select_type_ignores_hidden_and_locked_objects() {
        let mut e = EditorState::default();
        let mut d = document();
        for kind in [
            AnnotationKind::Arrow,
            AnnotationKind::Text,
            AnnotationKind::Arrow,
            AnnotationKind::Arrow,
            AnnotationKind::Arrow,
        ] {
            d.annotations
                .push(e.new_annotation(kind, Point { x: 20., y: 20. }));
        }
        d.annotations[2].layer.hidden = true;
        d.annotations[3].group_layer.locked = true;
        e.select_same_kind(&d, AnnotationKind::Arrow);
        assert_eq!(e.selected_indices(&d), vec![0, 4]);
    }

    #[test]
    fn contextual_bar_stays_in_view_near_edges_and_under_zoom() {
        let viewport = Rect::from_min_max(Pos2::new(30., 80.), Pos2::new(1000., 720.));
        for bounds in [
            viewport,
            viewport.translate(Vec2::new(-300., -200.)),
            Rect::from_min_size(Pos2::new(950., 650.), Vec2::splat(40.)),
        ] {
            let p = floating_position(bounds, viewport);
            assert!(viewport.contains_rect(Rect::from_min_size(p, Vec2::new(330., 40.))));
        }
    }

    #[test]
    fn compact_arrow_panel_contains_clickable_controls_without_horizontal_overflow() {
        let ctx = egui::Context::default();
        crate::appearance::apply(&ctx);
        let mut e = EditorState::default();
        let mut rect = Rect::NOTHING;
        let mut output = None;
        for _ in 0..2 {
            output = Some(ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(280., 700.))),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(6., 5.);
                        e.object_controls(ui, AnnotationKind::Arrow);
                        rect = ui.min_rect();
                    });
                },
            ));
        }
        assert!(rect.right() <= 280., "Panel overflow: {rect:?}");
        let labels: Vec<_> = output
            .unwrap()
            .shapes
            .into_iter()
            .filter_map(|s| {
                if let egui::Shape::Text(t) = s.shape {
                    Some(t.galley.job.text.clone())
                } else {
                    None
                }
            })
            .collect();
        for label in [
            "Opacity",
            "Width",
            "Start",
            "Line",
            "End",
            "Start size",
            "End size",
            "Double arrow",
        ] {
            assert!(
                labels.iter().any(|s| s == label),
                "Missing {label}: {labels:?}"
            );
        }
    }

    #[test]
    fn floating_style_menu_updates_object_without_clicking_through_canvas() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::appearance::apply(&ctx);
        let mut e = EditorState::default();
        let mut d = document();
        let mut a = e.new_annotation(AnnotationKind::Arrow, Point { x: 100., y: 160. });
        a.end = Point { x: 360., y: 100. };
        d.annotations.push(a);
        e.select(Some(0), &d);
        let mut h = EditHistory::new(d.edit_state());
        let texture = ctx.load_texture(
            "test-image",
            egui::ColorImage::filled([700, 500], Color32::WHITE),
            Default::default(),
        );
        let mut preview = crate::privacy_preview::PrivacyPreview::default();
        let frame = |e: &mut EditorState,
                     d: &mut Document,
                     h: &mut EditHistory,
                     preview: &mut crate::privacy_preview::PrivacyPreview,
                     events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100., 800.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| e.ui(ui, d, h, &texture, preview));
                },
            )
        };
        let find =
            |out: egui::FullOutput, label: &str| {
                out.platform_output
                    .accesskit_update
                    .as_ref()
                    .and_then(|update| {
                        update.nodes.iter().rev().find_map(|(_, n)| {
                            (n.label() == Some(label)).then(|| n.bounds()).flatten()
                        })
                    })
                    .map(|r| Pos2::new(((r.x0 + r.x1) * 0.5) as f32, ((r.y0 + r.y1) * 0.5) as f32))
                    .or_else(|| {
                        out.shapes.iter().rev().find_map(|s| {
                            if let egui::Shape::Text(t) = &s.shape {
                                (t.galley.job.text == label)
                                    .then(|| Rect::from_min_size(t.pos, t.galley.size()).center())
                            } else {
                                None
                            }
                        })
                    })
                    .unwrap_or_else(|| panic!("Missing {label}"))
            };
        for _ in 0..3 {
            frame(&mut e, &mut d, &mut h, &mut preview, vec![]);
        }
        let pos = find(
            frame(&mut e, &mut d, &mut h, &mut preview, vec![]),
            "Style ▾",
        );
        for pressed in [true, false] {
            frame(
                &mut e,
                &mut d,
                &mut h,
                &mut preview,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(e.selected, Some(0));
        assert!(e.area.is_none());
        assert_eq!(d.annotations.len(), 1);
        let pos = find(
            frame(&mut e, &mut d, &mut h, &mut preview, vec![]),
            "Double arrow",
        );
        for pressed in [true, false] {
            frame(
                &mut e,
                &mut d,
                &mut h,
                &mut preview,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(d.annotations[0].arrow.start, Endpoint::Triangle);
        assert_eq!(e.selected, Some(0));
        assert!(e.area.is_none());
        assert_eq!(d.annotations.len(), 1);
        for label in ["Dash-dot"] {
            for _ in 0..2 {
                frame(&mut e, &mut d, &mut h, &mut preview, vec![]);
            }
            let pos = find(frame(&mut e, &mut d, &mut h, &mut preview, vec![]), label);
            for pressed in [true, false] {
                frame(
                    &mut e,
                    &mut d,
                    &mut h,
                    &mut preview,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                );
            }
        }
        assert_eq!(d.annotations[0].arrow.pattern, Pattern::DashDot);
        assert_eq!(e.selected, Some(0));
        assert!(h.undo(&mut d));
        assert_eq!(d.annotations[0].arrow.pattern, Pattern::Solid);
        assert!(h.undo(&mut d));
        assert!(d.annotations[0].arrow.is_default());
        assert!(!h.can_undo());
    }

    #[test]
    #[ignore = "Writes a disposable visual review project"]
    fn write_object_properties_review_fixture() {
        let e = EditorState::default();
        let mut d = document();
        for (kind, start, end) in [
            (AnnotationKind::Arrow, [70., 140.], [410., 75.]),
            (AnnotationKind::Rectangle, [440., 70.], [630., 190.]),
            (AnnotationKind::Text, [70., 220.], [350., 270.]),
            (AnnotationKind::Callout, [410., 250.], [650., 360.]),
            (AnnotationKind::Step, [60., 350.], [120., 410.]),
            (AnnotationKind::Highlighter, [160., 375.], [350., 410.]),
        ] {
            let mut a = e.new_annotation(
                kind,
                Point {
                    x: start[0],
                    y: start[1],
                },
            );
            a.end = Point {
                x: end[0],
                y: end[1],
            };
            if kind == AnnotationKind::Text {
                a.text = "Click to style".into();
            }
            if kind == AnnotationKind::Callout {
                a.text = "Editable callout".into();
                a.tail = Some(Point { x: 380., y: 430. });
            }
            if kind == AnnotationKind::Highlighter {
                a.color = [255, 215, 0, 100];
            }
            d.annotations.push(a);
        }
        let dir = std::path::Path::new("/tmp/myimage-properties-review");
        std::fs::create_dir_all(dir).unwrap();
        crate::document::save_project(&d, &dir.join("Objects.myimage")).unwrap();
        crate::render::flatten(&d)
            .unwrap()
            .save(dir.join("Objects.png"))
            .unwrap();
    }
}
