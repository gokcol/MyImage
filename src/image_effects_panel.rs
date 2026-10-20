//! Image-wide styling, kept separate from annotation styles and privacy tools.
use super::*;
use crate::{
    appearance,
    image_effects::{ColorFilter, EdgeStyle, Edges, ImageEffects},
};

impl EditorState {
    pub(super) fn open_image_effects(
        &mut self,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        self.finish_gesture(document, history);
        self.effects_open = true;
        self.layers_open = false;
        self.presets_open = false;
    }

    pub(super) fn image_effects_panel(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let mut effects = document.geometry.effects;
        ui.spacing_mut().item_spacing = Vec2::new(6., 6.);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Quick Effects").size(16.).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let help = appearance::icon_button(ui, "Image effects help", DialogIcon::Info);
                egui::Popup::menu(&help).show(|ui| {
                    ui.set_max_width(240.);
                    ui.label("Style the whole image, including its annotations. Effects stay adjustable in your project.");
                    ui.label("Borders and frame shadows add transparent canvas space. Changing a style keeps the same frame, so padding does not accumulate.");
                    ui.small("No effects clears the styling; added canvas space remains available. Undo restores styling and canvas size together.");
                });
            });
        });
        egui::ScrollArea::vertical()
            .id_salt("quick-effects-gallery")
            .max_height(144.)
            .show(ui, |ui| {
                for row in 0..4 {
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - 12.) / 3.;
                        for index in row * 3..row * 3 + 3 {
                            let label = [
                                "No effects",
                                "Soft shadow",
                                "Floating",
                                "Ink border",
                                "Calico border",
                                "Grayscale",
                                "Torn edge",
                                "Wave edge",
                                "Rounded edge",
                                "Perspective",
                                "Reflection",
                                "Fade",
                            ][index];
                            if effect_card(ui, index, label, width, effects).clicked() {
                                match index {
                                    0 => {
                                        effects = ImageEffects {
                                            frame: effects.frame,
                                            ..Default::default()
                                        }
                                    }
                                    1 => {
                                        effects.shadow_opacity = 80;
                                        effects.shadow_blur = 10;
                                        effects.shadow_offset = 5;
                                        self.image_effect_tab = 0;
                                    }
                                    2 => {
                                        effects.shadow_opacity = 110;
                                        effects.shadow_blur = 20;
                                        effects.shadow_offset = 12;
                                        self.image_effect_tab = 0;
                                    }
                                    3 => {
                                        effects.border_width = 3;
                                        effects.border_color = [38, 35, 32, 255];
                                        self.image_effect_tab = 1;
                                    }
                                    4 => {
                                        effects.border_width = 5;
                                        effects.border_color = [237, 142, 48, 255];
                                        self.image_effect_tab = 1;
                                    }
                                    6..=8 => {
                                        effects.edges.style =
                                            [EdgeStyle::Torn, EdgeStyle::Wave, EdgeStyle::Rounded]
                                                [index - 6];
                                        self.image_effect_tab = 3;
                                    }
                                    9 => {
                                        effects.styles.perspective = [20, 0];
                                        self.image_effect_tab = 4;
                                    }
                                    10 => {
                                        effects.styles.reflection_height = 40;
                                        effects.styles.reflection_opacity = 100;
                                        effects.styles.reflection_gap = 6;
                                        self.image_effect_tab = 5;
                                    }
                                    11 => {
                                        effects.styles.fade_depth = 80;
                                        effects.styles.fade_sides = [false, false, true, false];
                                        self.image_effect_tab = 7;
                                    }
                                    _ => {
                                        effects.grayscale = !effects.grayscale;
                                        self.image_effect_tab = 2;
                                    }
                                }
                            }
                        }
                    });
                }
            });
        ui.add_space(2.);
        ui.separator();
        ui.label(
            egui::RichText::new("Effect Styles")
                .small()
                .strong()
                .color(appearance::MUTED),
        );
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(3., 3.);
            ui.spacing_mut().button_padding = Vec2::new(6., 3.);
            ui.spacing_mut().interact_size.y = 22.;
            for (index, name) in [
                (3, "Edges"),
                (1, "Border"),
                (0, "Shadow"),
                (4, "Perspective"),
                (5, "Reflection"),
                (7, "Fade"),
                (6, "Filters"),
                (2, "Adjust"),
            ] {
                ui.selectable_value(
                    &mut self.image_effect_tab,
                    index,
                    egui::RichText::new(name).size(11.),
                );
            }
        });
        ui.separator();
        match self.image_effect_tab {
            4 => {
                ui.horizontal(|ui| {
                    for (label, value) in [
                        ("Flat", [0, 0]),
                        ("Left", [-20, 0]),
                        ("Right", [20, 0]),
                        ("Above", [0, 20]),
                    ] {
                        if ui
                            .selectable_label(effects.styles.perspective == value, label)
                            .clicked()
                        {
                            effects.styles.perspective = value;
                        }
                    }
                });
                ui.small("Horizontal tilt");
                appearance::value_slider(
                    ui,
                    &mut effects.styles.perspective[0],
                    -30..=30,
                    "Horizontal perspective",
                    "%",
                );
                ui.small("Vertical tilt");
                appearance::value_slider(
                    ui,
                    &mut effects.styles.perspective[1],
                    -30..=30,
                    "Vertical perspective",
                    "%",
                );
                ui.small("Tilt the image and its layers together. Handles follow the artwork.");
            }
            5 => {
                let mut enabled = effects.styles.reflection_height > 0;
                if ui.checkbox(&mut enabled, "Reflection").changed() {
                    effects.styles.reflection_height = if enabled { 40 } else { 0 };
                    if enabled && effects.styles.reflection_opacity == 0 {
                        effects.styles.reflection_opacity = 100;
                    }
                }
                ui.add_enabled_ui(enabled, |ui| {
                    ui.small("Height");
                    let mut height = effects.styles.reflection_height.max(1);
                    if appearance::value_slider(ui, &mut height, 1..=100, "Reflection height", "%")
                        .changed()
                    {
                        effects.styles.reflection_height = height;
                    }
                    ui.small("Opacity");
                    let mut opacity = effects.styles.reflection_opacity as f32 / 2.55;
                    if appearance::value_slider(
                        ui,
                        &mut opacity,
                        0.0..=100.0,
                        "Reflection opacity",
                        "%",
                    )
                    .changed()
                    {
                        effects.styles.reflection_opacity = (opacity * 2.55).round() as u8;
                    }
                    ui.small("Gap");
                    appearance::value_slider(
                        ui,
                        &mut effects.styles.reflection_gap,
                        0..=128,
                        "Reflection gap",
                        " px",
                    );
                });
            }
            7 => {
                let mut enabled = effects.styles.fade_depth > 0;
                if ui.checkbox(&mut enabled, "Fade to transparency").changed() {
                    effects.styles.fade_depth = if enabled { 80 } else { 0 };
                    if enabled && !effects.styles.fade_sides.iter().any(|s| *s) {
                        effects.styles.fade_sides[2] = true;
                    }
                }
                ui.add_enabled_ui(enabled, |ui| {
                    ui.small("Depth");
                    let mut depth = effects.styles.fade_depth.max(1);
                    if appearance::value_slider(ui, &mut depth, 1..=512, "Fade depth", " px")
                        .changed()
                    {
                        effects.styles.fade_depth = depth;
                    }
                    ui.horizontal(|ui| {
                        for (i, side) in ["Top", "Right", "Bottom", "Left"].iter().enumerate() {
                            ui.toggle_value(&mut effects.styles.fade_sides[i], *side);
                        }
                    });
                    ui.small("Blend chosen edges smoothly into transparency.");
                });
            }
            6 => {
                for row in [
                    (ColorFilter::None, "Original"),
                    (ColorFilter::Sepia, "Sepia"),
                    (ColorFilter::Warm, "Warm"),
                    (ColorFilter::Cool, "Cool"),
                    (ColorFilter::Invert, "Invert"),
                ]
                .chunks(3)
                {
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - 12.) / 3.;
                        for &(filter, label) in row {
                            let mut preview = ImageEffects::default();
                            preview.styles.filter = filter;
                            if style_card(
                                ui,
                                label,
                                width,
                                effects.styles.filter == filter,
                                preview,
                            )
                            .clicked()
                            {
                                effects.styles.filter = filter;
                            }
                        }
                    });
                }
            }
            3 => {
                let choices = [
                    (EdgeStyle::None, "Straight"),
                    (EdgeStyle::Torn, "Torn"),
                    (EdgeStyle::Wave, "Wave"),
                    (EdgeStyle::Sawtooth, "Sawtooth"),
                    (EdgeStyle::Rounded, "Rounded"),
                ];
                for row in choices.chunks(3) {
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - 12.) / 3.;
                        for &(style, label) in row {
                            if edge_card(ui, style, label, width, effects.edges.style == style)
                                .clicked()
                            {
                                effects.edges.style = style;
                            }
                        }
                    });
                }
                ui.add_enabled_ui(effects.edges.style != EdgeStyle::None, |ui| {
                    ui.small(if effects.edges.style == EdgeStyle::Rounded {
                        "Corner radius"
                    } else {
                        "Edge depth"
                    });
                    appearance::value_slider(
                        ui,
                        &mut effects.edges.depth,
                        1..=128,
                        "Edge depth",
                        " px",
                    );
                    ui.horizontal(|ui| {
                        for (index, name) in ["Top", "Right", "Bottom", "Left"].iter().enumerate() {
                            ui.toggle_value(&mut effects.edges.sides[index], *name);
                        }
                    });
                    ui.small(if effects.edges.style == EdgeStyle::Rounded {
                        "A corner rounds where two enabled sides meet."
                    } else {
                        "Trim edges to transparency. Original pixels stay editable."
                    });
                });
            }
            0 => {
                let mut enabled = effects.shadow_opacity > 0;
                if ui.checkbox(&mut enabled, "Frame shadow").changed() {
                    effects.shadow_opacity = if enabled { 80 } else { 0 };
                }
                ui.add_enabled_ui(enabled, |ui| {
                    ui.small("Opacity");
                    let mut opacity = effects.shadow_opacity as f32 * 100. / 255.;
                    if appearance::value_slider(
                        ui,
                        &mut opacity,
                        0.0..=100.0,
                        "Shadow opacity",
                        "%",
                    )
                    .changed()
                    {
                        effects.shadow_opacity = (opacity * 2.55).round() as u8;
                    }
                    ui.small("Softness");
                    appearance::value_slider(
                        ui,
                        &mut effects.shadow_blur,
                        1..=64,
                        "Shadow softness",
                        " px",
                    );
                    ui.small("Offset");
                    appearance::value_slider(
                        ui,
                        &mut effects.shadow_offset,
                        -64..=64,
                        "Shadow offset",
                        " px",
                    );
                });
            }
            1 => {
                let mut enabled = effects.border_width > 0;
                if ui.checkbox(&mut enabled, "Image border").changed() {
                    effects.border_width = if enabled { 3 } else { 0 };
                }
                ui.add_enabled_ui(enabled, |ui| {
                    ui.small("Width");
                    let mut width = effects.border_width.max(1);
                    if appearance::value_slider(ui, &mut width, 1..=64, "Border width", " px")
                        .changed()
                        && enabled
                    {
                        effects.border_width = width;
                    }
                    ui.horizontal(|ui| {
                        ui.label("Color");
                        ui.color_edit_button_srgba_unmultiplied(&mut effects.border_color);
                        for color in [
                            [38, 35, 32, 255],
                            [237, 142, 48, 255],
                            [255; 4],
                            [0, 143, 154, 255],
                        ] {
                            let response = ui.add_sized(
                                [23., 23.],
                                egui::Button::new("").fill(Color32::from_rgba_unmultiplied(
                                    color[0], color[1], color[2], color[3],
                                )),
                            );
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    ui.is_enabled(),
                                    format!(
                                        "Border color #{:02x}{:02x}{:02x}",
                                        color[0], color[1], color[2]
                                    ),
                                )
                            });
                            if response.clicked() {
                                effects.border_color = color;
                            }
                        }
                    });
                });
            }
            _ => {
                ui.checkbox(&mut effects.grayscale, "Grayscale");
                ui.small("Brightness");
                appearance::value_slider(
                    ui,
                    &mut effects.styles.brightness,
                    -100..=100,
                    "Image brightness",
                    "%",
                );
                ui.small("Contrast");
                appearance::value_slider(
                    ui,
                    &mut effects.styles.contrast,
                    -100..=100,
                    "Image contrast",
                    "%",
                );
                ui.small("Saturation");
                appearance::value_slider(
                    ui,
                    &mut effects.styles.saturation,
                    -100..=100,
                    "Image saturation",
                    "%",
                );
                if ui.button("Reset color adjustments").clicked() {
                    effects.grayscale = false;
                    effects.styles.brightness = 0;
                    effects.styles.contrast = 0;
                    effects.styles.saturation = 0;
                }
            }
        }
        if effects != document.geometry.effects {
            let before = document.edit_state();
            let old_size = document.output_size();
            match crate::image_effects::update(document, effects) {
                Ok(()) => {
                    self.record_properties(before, document, history);
                    if old_size != document.output_size() {
                        self.fit = true;
                    }
                    self.image_error = None;
                }
                Err(error) => self.image_error = Some(error),
            }
        }
    }
}

fn effect_card(
    ui: &mut egui::Ui,
    index: usize,
    label: &str,
    width: f32,
    effects: ImageEffects,
) -> egui::Response {
    if index >= 9 {
        let mut preview = ImageEffects::default();
        let selected = match index {
            9 => {
                preview.styles.perspective = [20, 0];
                effects.styles.perspective != [0; 2]
            }
            10 => {
                preview.styles.reflection_height = 40;
                preview.styles.reflection_opacity = 150;
                effects.styles.reflection_height > 0
            }
            _ => {
                preview.styles.fade_depth = 24;
                preview.styles.fade_sides[2] = true;
                effects.styles.fade_depth > 0
            }
        };
        return style_card(ui, label, width, selected, preview);
    }
    if index >= 6 {
        let style = [EdgeStyle::Torn, EdgeStyle::Wave, EdgeStyle::Rounded][index - 6];
        return edge_card(ui, style, label, width, effects.edges.style == style);
    }
    let selected = match index {
        0 => !effects.active(),
        1 => {
            effects.shadow_opacity == 80 && effects.shadow_blur == 10 && effects.shadow_offset == 5
        }
        2 => {
            effects.shadow_opacity == 110
                && effects.shadow_blur == 20
                && effects.shadow_offset == 12
        }
        3 => effects.border_width == 3 && effects.border_color == [38, 35, 32, 255],
        4 => effects.border_width == 5 && effects.border_color == [237, 142, 48, 255],
        _ => effects.grayscale,
    };
    let response = ui.add_sized([width, 66.], egui::Button::new("").frame(false));
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    let p = ui.painter();
    let r = response.rect;
    p.rect(
        r,
        6.,
        if selected || response.hovered() {
            appearance::CREAM
        } else {
            appearance::PAPER
        },
        Stroke::new(
            if selected { 2. } else { 1. },
            if selected {
                appearance::ACCENT
            } else {
                appearance::BORDER
            },
        ),
        StrokeKind::Inside,
    );
    let preview = Rect::from_center_size(r.center_top() + Vec2::new(0., 25.), Vec2::new(39., 28.));
    if index == 1 || index == 2 {
        p.add(
            egui::Shadow {
                offset: [3, 4],
                blur: if index == 1 { 8 } else { 14 },
                spread: 0,
                color: Color32::from_black_alpha(95),
            }
            .as_shape(preview, 3),
        );
    }
    p.rect_filled(preview, 2., Color32::WHITE);
    if index == 3 || index == 4 {
        p.rect_stroke(
            preview,
            2.,
            Stroke::new(
                2.,
                if index == 3 {
                    appearance::INK
                } else {
                    appearance::GINGER
                },
            ),
            StrokeKind::Outside,
        );
    }
    p.rect_filled(
        Rect::from_min_size(preview.min + Vec2::splat(5.), Vec2::new(11., 18.)),
        1.,
        if index == 5 {
            Color32::from_gray(90)
        } else {
            appearance::GINGER
        },
    );
    for y in [8., 14., 20.] {
        p.line_segment(
            [
                preview.min + Vec2::new(20., y),
                preview.min + Vec2::new(34., y),
            ],
            Stroke::new(
                2.,
                if index == 5 {
                    Color32::from_gray(130)
                } else {
                    Color32::from_rgb(45, 137, 142)
                },
            ),
        );
    }
    p.text(
        r.center_bottom() - Vec2::new(0., 7.),
        egui::Align2::CENTER_BOTTOM,
        label,
        egui::FontId::proportional(10.5),
        appearance::INK,
    );
    response.on_hover_cursor(CursorIcon::PointingHand)
}

fn edge_card(
    ui: &mut egui::Ui,
    style: EdgeStyle,
    label: &str,
    width: f32,
    selected: bool,
) -> egui::Response {
    let response = ui.add_sized([width, 62.], egui::Button::new("").frame(false));
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    let id = egui::Id::new(("edge-style-preview", format!("{style:?}")));
    let texture = ui
        .ctx()
        .data_mut(|data| data.get_temp::<egui::TextureHandle>(id));
    let texture = texture.unwrap_or_else(|| {
        let mut pixels = image::RgbaImage::from_fn(80, 48, |x, y| {
            if y > 32 - x.min(24) / 2 {
                image::Rgba([45, 137, 142, 255])
            } else {
                image::Rgba([237, 160, 72, 255])
            }
        });
        crate::image_effects::apply(
            &mut pixels,
            ImageEffects {
                edges: Edges {
                    style,
                    depth: 9,
                    sides: [true; 4],
                },
                frame: Some(crate::geometry::CanvasRect {
                    x: 0,
                    y: 0,
                    width: 80,
                    height: 48,
                }),
                ..Default::default()
            },
            [0.; 2],
            [1.; 2],
        );
        let texture = ui.ctx().load_texture(
            "edge preview",
            egui::ColorImage::from_rgba_unmultiplied([80, 48], pixels.as_raw()),
            egui::TextureOptions::LINEAR,
        );
        ui.ctx()
            .data_mut(|data| data.insert_temp(id, texture.clone()));
        texture
    });
    let p = ui.painter();
    let r = response.rect;
    p.rect(
        r,
        5.,
        if selected || response.hovered() {
            appearance::CREAM
        } else {
            appearance::PAPER
        },
        Stroke::new(
            if selected { 2. } else { 1. },
            if selected {
                appearance::ACCENT
            } else {
                appearance::BORDER
            },
        ),
        StrokeKind::Inside,
    );
    let preview = Rect::from_center_size(r.center_top() + Vec2::new(0., 23.), Vec2::new(48., 29.));
    p.rect_filled(preview, 0., Color32::from_gray(225));
    p.image(
        texture.id(),
        preview,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
        Color32::WHITE,
    );
    p.text(
        r.center_bottom() - Vec2::new(0., 5.),
        egui::Align2::CENTER_BOTTOM,
        label,
        egui::FontId::proportional(10.5),
        appearance::INK,
    );
    response.on_hover_cursor(CursorIcon::PointingHand)
}

fn style_card(
    ui: &mut egui::Ui,
    label: &str,
    width: f32,
    selected: bool,
    mut effects: ImageEffects,
) -> egui::Response {
    let response = ui.add_sized([width, 62.], egui::Button::new("").frame(false));
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    let id = egui::Id::new(("effect-style-preview", label));
    let texture = ui
        .ctx()
        .data_mut(|data| data.get_temp::<egui::TextureHandle>(id))
        .unwrap_or_else(|| {
            let mut pixels = image::RgbaImage::from_fn(80, 68, |x, y| {
                if y >= 48 {
                    image::Rgba([0; 4])
                } else if y > 32 - x.min(24) / 2 {
                    image::Rgba([45, 137, 142, 255])
                } else {
                    image::Rgba([237, 160, 72, 255])
                }
            });
            effects.frame = Some(crate::geometry::CanvasRect {
                x: 0,
                y: 0,
                width: 80,
                height: 48,
            });
            crate::image_effects::apply(&mut pixels, effects, [0.; 2], [1.; 2]);
            let texture = ui.ctx().load_texture(
                "effect preview",
                egui::ColorImage::from_rgba_unmultiplied([80, 68], pixels.as_raw()),
                egui::TextureOptions::LINEAR,
            );
            ui.ctx()
                .data_mut(|data| data.insert_temp(id, texture.clone()));
            texture
        });
    let r = response.rect;
    let p = ui.painter();
    p.rect(
        r,
        5.,
        if selected || response.hovered() {
            appearance::CREAM
        } else {
            appearance::PAPER
        },
        Stroke::new(
            if selected { 2. } else { 1. },
            if selected {
                appearance::ACCENT
            } else {
                appearance::BORDER
            },
        ),
        StrokeKind::Inside,
    );
    let preview = Rect::from_center_size(r.center_top() + Vec2::new(0., 24.), Vec2::new(47., 40.));
    p.image(
        texture.id(),
        preview,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
        Color32::WHITE,
    );
    p.text(
        r.center_bottom() - Vec2::new(0., 5.),
        egui::Align2::CENTER_BOTTOM,
        label,
        egui::FontId::proportional(10.5),
        appearance::INK,
    );
    response.on_hover_cursor(CursorIcon::PointingHand)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quick_cards_combine_clear_and_keep_disabled_border_off() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        appearance::apply(&ctx);
        let mut e = EditorState::default();
        let mut d = Document {
            image: std::sync::Arc::new(image::RgbaImage::from_pixel(
                100,
                80,
                image::Rgba([210, 80, 40, 255]),
            )),
            annotations: vec![],
            geometry: Default::default(),
        };
        let original = d.edit_state();
        let mut h = EditHistory::new(original.clone());
        let mut frame = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(280., 650.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| e.image_effects_panel(ui, &mut d, &mut h));
                },
            )
        };
        for label in ["Soft shadow", "Ink border", "No effects"] {
            let out = frame(vec![]);
            let at = out
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find_map(|(_, n)| (n.label() == Some(label)).then(|| n.bounds()).flatten())
                .unwrap();
            let pos = egui::pos2((at.x0 + at.x1) as f32 * 0.5, (at.y0 + at.y1) as f32 * 0.5);
            for pressed in [true, false] {
                frame(vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ]);
            }
        }
        for _ in 0..3 {
            frame(vec![]);
        }
        assert!(!d.geometry.effects.active());
        assert_eq!(d.geometry.effects.border_width, 0);
        assert!(h.undo(&mut d));
        assert!(d.geometry.effects.border_width > 0 && d.geometry.effects.shadow_opacity > 0);
        assert!(h.undo(&mut d));
        assert_eq!(d.geometry.effects.border_width, 0);
        assert!(d.geometry.effects.shadow_opacity > 0);
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), original);
        assert!(!h.undo(&mut d));
    }
}
