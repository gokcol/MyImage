use super::*;
use crate::text_path::{Placement, Shape, TextPath};
pub(super) fn controls(ui: &mut egui::Ui, style: &mut crate::text_style::TextStyle) -> bool {
    let before = style.clone();
    let mut enabled = style.path.is_some();
    if ui.checkbox(&mut enabled, "Text on a path").changed() {
        style.path = enabled.then(TextPath::default);
    }
    let Some(path) = &mut style.path else {
        return *style != before;
    };
    egui::ComboBox::from_id_salt("text-path-shape")
        .selected_text(path.shape.name())
        .show_ui(ui, |ui| {
            for shape in Shape::ALL {
                ui.selectable_value(&mut path.shape, shape, shape.name());
            }
        });
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut path.placement, Placement::Over, "Over");
        ui.selectable_value(&mut path.placement, Placement::On, "On");
        ui.selectable_value(&mut path.placement, Placement::Below, "Below");
    });
    if path.shape != Shape::Vertical {
        object_properties::slider(
            ui,
            &mut path.width,
            20..=2048,
            if path.shape == Shape::Circle {
                "Diameter"
            } else {
                "Width"
            },
            " px",
        );
    }
    if matches!(
        path.shape,
        Shape::Vertical | Shape::Diagonal | Shape::Custom
    ) {
        object_properties::slider(ui, &mut path.height, 20..=2048, "Height", " px");
    }
    object_properties::slider(ui, &mut path.rotation, -180..=180, "Rotate", "°");
    object_properties::slider(ui, &mut path.offset, -100..=200, "Distance", " px");
    object_properties::slider(ui, &mut path.spacing, 0..=64, "Spacing", " px");
    ui.checkbox(&mut path.fit, "Fit long text to path").on_hover_text("Reduce letters together when they are longer than the path. Turn off to extend beyond its end.");
    if path.shape == Shape::Custom {
        custom_pad(ui, path);
        ui.checkbox(&mut path.closed, "Close path");
    }
    ui.checkbox(&mut path.visible, "Show path");
    if path.visible {
        ui.horizontal(|ui| {
            ui.label("Path color");
            ui.color_edit_button_srgba_unmultiplied(&mut path.color);
        });
        object_properties::slider(ui, &mut path.color[3], 0..=255, "Opacity", " /255");
        object_properties::slider(ui, &mut path.stroke, 1..=32, "Line width", " px");
    }
    *style != before
}
fn custom_pad(ui: &mut egui::Ui, path: &mut TextPath) {
    ui.small("Drag points. Double-click a line to add a point; right-click a point to remove it.");
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().min(300.), 112.),
        Sense::click_and_drag(),
    );
    let rect = rect.shrink(8.);
    let accent = crate::appearance::palette(ui.ctx()).accent;
    ui.painter()
        .rect_filled(rect.expand(8.), 5., ui.visuals().extreme_bg_color);
    let map =
        |p: [u16; 2]| rect.min + egui::vec2(f32::from(p[0]), f32::from(p[1])) / 1000. * rect.size();
    let unmap = |p: Pos2| {
        let p = (rect.clamp(p) - rect.min) / rect.size() * 1000.;
        [p.x.round() as u16, p.y.round() as u16]
    };
    let mut points: Vec<_> = path.nodes.iter().copied().map(map).collect();
    if path.closed {
        points.push(points[0]);
    }
    ui.painter()
        .add(egui::Shape::line(points.clone(), Stroke::new(2., accent)));
    let draw_id = ui.id().with("draw-text-path");
    let mut drawing = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(draw_id).unwrap_or(false));
    let mut remove = None;
    if !drawing {
        for (i, node) in path.nodes.iter_mut().enumerate() {
            let p = map(*node);
            let r = ui.interact(
                Rect::from_center_size(p, Vec2::splat(16.)),
                response.id.with(i),
                Sense::click_and_drag(),
            );
            ui.painter().circle_filled(p, 4., accent);
            if r.dragged()
                && let Some(p) = r.interact_pointer_pos()
            {
                *node = unmap(p);
            }
            if r.secondary_clicked() {
                remove = Some(i);
            }
        }
        if let Some(i) = remove
            && path.nodes.len() > 2
        {
            path.nodes.remove(i);
        }
        if response.double_clicked()
            && path.nodes.len() < 256
            && let Some(p) = response.interact_pointer_pos()
        {
            let segment = points
                .windows(2)
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    segment_distance(p, a[0], a[1]).total_cmp(&segment_distance(p, b[0], b[1]))
                })
                .map_or(0, |(i, _)| i);
            path.nodes
                .insert((segment + 1).min(path.nodes.len()), unmap(p));
        }
    } else if let Some(p) = response.interact_pointer_pos() {
        if response.drag_started() {
            let p = unmap(p);
            path.nodes = vec![p, [p[0].saturating_add(1).min(1000), p[1]]];
        }
        if response.dragged()
            && path.nodes.len() < 256
            && path
                .nodes
                .last()
                .is_none_or(|&last| map(last).distance(p) > 3.)
        {
            path.nodes.push(unmap(p));
        }
        if response.drag_stopped() {
            drawing = false;
        }
    }
    if ui
        .button(if drawing {
            "Cancel drawing"
        } else {
            "Draw a new path"
        })
        .clicked()
    {
        drawing = !drawing;
    }
    if drawing {
        ui.small("Drag inside the box to draw your path.");
    }
    ui.ctx().data_mut(|d| d.insert_temp(draw_id, drawing));
}
impl EditorState {
    pub(super) fn path_text_dialog(
        &mut self,
        ctx: &egui::Context,
        mut draft: text_editor::TextDraft,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let Some(annotation) = document.annotations.get_mut(draft.index) else {
            return;
        };
        let mut open = true;
        let mut apply = ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter));
        let mut cancel = !egui::Popup::is_any_open(ctx)
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        egui::Window::new("Edit path text")
            .id(egui::Id::new("path-text-editor"))
            .default_width(286.)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                let response = ui.add(
                    egui::TextEdit::multiline(&mut annotation.text)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY)
                        .char_limit(16_384),
                );
                if draft.focus {
                    response.request_focus();
                    draft.focus = false;
                }
                egui::ScrollArea::vertical()
                    .max_height(440.)
                    .show(ui, |ui| {
                        object_properties::palette(ui, &mut annotation.color);
                        object_properties::text_controls(
                            ui,
                            &mut annotation.text_style,
                            &mut annotation.font_size,
                        );
                        controls(ui, &mut annotation.text_style);
                    });
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        self.text_dialog = Some(draft);
        if apply || cancel || !open {
            self.finish_text_editor(document, history, !cancel);
        }
    }
}
