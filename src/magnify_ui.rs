//! Compact visual controls for editable magnification lenses.
use crate::{document::Annotation, studio::Options};
use eframe::egui::{self, Color32, Sense, Stroke, StrokeKind, Vec2};
const COLORS: [[u8; 4]; 5] = [
    [229, 57, 53, 255],
    [115, 177, 68, 255],
    [28, 156, 226, 255],
    [245, 198, 45, 255],
    [40, 43, 47, 255],
];
fn preset(index: usize) -> (bool, bool, [u8; 4]) {
    let (mode, color) = if index < 16 {
        (index / 4, index % 4)
    } else {
        (index - 16, 4)
    };
    (mode < 2, mode % 2 == 1, COLORS[color])
}
fn preview(mut a: Annotation, index: usize) -> image::RgbaImage {
    let (ellipse, linked, color) = preset(index);
    a.studio = Options {
        ellipse,
        linked,
        ..Default::default()
    };
    a.color = color;
    a.stroke_width = 3.;
    a.start = crate::document::Point { x: 38., y: 26. };
    a.end = crate::document::Point { x: 94., y: 78. };
    if !linked {
        a.start.x = 24.;
        a.start.y = 12.;
    }
    a.pixel_mask = None;
    a.area_mask.clear();
    let mut image = image::RgbaImage::from_fn(112, 88, |x, y| {
        let mountain = (x as i32 - 44).abs() / 2 + 16;
        let second = (x as i32 - 92).abs() / 2 + 30;
        let c = if (y as i32) > second {
            [46, 88, 102, 255]
        } else if (y as i32) > mountain + 13 {
            [78, 119, 145, 255]
        } else if (y as i32) > mountain {
            [235, 242, 240, 255]
        } else {
            [163, 206, 224, 255]
        };
        image::Rgba(c)
    });
    crate::studio::apply(&mut image, &a, [1.; 2]).expect("valid preset preview");
    image
}
pub fn gallery(
    ui: &mut egui::Ui,
    template: Annotation,
    options: &mut Options,
    color: &mut [u8; 4],
) -> bool {
    let id = egui::Id::new("magnify-style-thumbnails-v1");
    let textures = ui
        .ctx()
        .data_mut(|data| data.get_temp::<Vec<egui::TextureHandle>>(id))
        .unwrap_or_else(|| {
            let textures: Vec<_> = (0..20)
                .map(|i| {
                    let image = preview(template.clone(), i);
                    ui.ctx().load_texture(
                        format!("magnify-style-{i}"),
                        egui::ColorImage::from_rgba_unmultiplied([112, 88], image.as_raw()),
                        egui::TextureOptions::LINEAR,
                    )
                })
                .collect();
            ui.ctx()
                .data_mut(|data| data.insert_temp(id, textures.clone()));
            textures
        });
    let mut changed = false;
    let width = ((ui.available_width() - 18.) / 4.).clamp(40., 62.);
    egui::Grid::new("magnify-style-grid")
        .spacing(Vec2::splat(6.))
        .show(ui, |ui| {
            for (i, texture) in textures.iter().enumerate() {
                let (ellipse, linked, tint) = preset(i);
                let selected = options.ellipse == ellipse
                    && !options.rounded
                    && options.linked == linked
                    && *color == tint;
                let label = format!(
                    "{} {} lens, {}",
                    if linked { "Linked" } else { "In-place" },
                    if ellipse { "oval" } else { "rectangle" },
                    ["red", "green", "blue", "yellow", "charcoal"][if i < 16 { i % 4 } else { 4 }]
                );
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(width, width * 0.79), Sense::click());
                ui.painter().image(
                    texture.id(),
                    rect.shrink(3.),
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                    Color32::WHITE,
                );
                if selected || response.hovered() {
                    ui.painter().rect_stroke(
                        rect,
                        4.,
                        Stroke::new(
                            if selected { 2. } else { 1. },
                            crate::appearance::palette(ui.ctx()).accent,
                        ),
                        StrokeKind::Inside,
                    );
                }
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::SelectableLabel,
                        ui.is_enabled(),
                        selected,
                        &label,
                    )
                });
                if response.on_hover_text(label).clicked() {
                    options.ellipse = ellipse;
                    options.rounded = false;
                    options.linked = linked;
                    *color = tint;
                    changed = true;
                }
                if i % 4 == 3 {
                    ui.end_row();
                }
            }
        });
    changed
}
pub fn shape_picker(ui: &mut egui::Ui, options: &mut Options) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for (ellipse, rounded, label) in [
            (true, false, "Oval"),
            (false, false, "Rectangle"),
            (false, true, "Rounded rectangle"),
        ] {
            let selected = options.ellipse == ellipse && options.rounded == rounded;
            let (r, response) = ui.allocate_exact_size(egui::vec2(42., 30.), Sense::click());
            if selected || response.hovered() {
                ui.painter().rect_filled(
                    r,
                    4.,
                    if selected {
                        Color32::from_rgb(255, 238, 215)
                    } else {
                        Color32::from_gray(235)
                    },
                );
            }
            let shape = r.shrink2(egui::vec2(8., 7.));
            let stroke = Stroke::new(1.7, crate::appearance::palette(ui.ctx()).ink);
            if ellipse {
                ui.painter().add(egui::epaint::EllipseShape::stroke(
                    shape.center(),
                    shape.size() / 2.,
                    stroke,
                ));
            } else {
                ui.painter().rect_stroke(
                    shape,
                    if rounded { 4. } else { 0. },
                    stroke,
                    StrokeKind::Inside,
                );
            }
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::SelectableLabel,
                    ui.is_enabled(),
                    selected,
                    label,
                )
            });
            if response.on_hover_text(label).clicked() {
                options.ellipse = ellipse;
                options.rounded = rounded;
                changed = true;
            }
        }
    });
    changed
}
pub fn controls(ui: &mut egui::Ui, options: &mut Options) -> bool {
    let mut changed = shape_picker(ui, options);
    ui.horizontal(|ui| {
        changed |= ui
            .selectable_value(&mut options.linked, false, "In place")
            .changed();
        changed |= ui
            .selectable_value(&mut options.linked, true, "Linked lens")
            .changed();
    });
    let mut percent = options.zoom * 100.;
    changed |=
        crate::appearance::value_slider(ui, &mut percent, 100.0..=800.0, "Magnification", "%")
            .changed();
    options.zoom = percent / 100.;
    if options.linked {
        ui.weak("Drag the yellow point to choose the source detail.");
    }
    changed
}
