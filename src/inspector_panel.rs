//! A compact inspector with a draggable, keyboard-accessible left divider.
use eframe::egui::{self, Color32, CursorIcon, Id, Key, Pos2, Rect, Sense, Stroke, Vec2};

#[derive(Clone, Copy, Default)]
struct WidthPreference(Option<f32>);

fn fitted_width(ui: &egui::Ui) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let labels: f32 = ["Styles", "Layers", "Presets", "Effects"]
        .into_iter()
        .map(|label| {
            ui.painter()
                .layout_no_wrap(label.into(), font.clone(), Color32::WHITE)
                .size()
                .x
        })
        .sum();
    // Match the tab padding/spacing plus the panel margins. The tile grids
    // and property controls need at least 260 points of content width.
    (labels + 4. * 12. + 3. * 4. + 20.).ceil().max(280.)
}

pub(super) fn show<R>(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let id = Id::new("annotation-inspector-width");
    let divider_id = id.with("divider");
    let workspace = ui.available_rect_before_wrap();
    // Leave a useful canvas even in a small window or at a large UI scale.
    let max = (workspace.width() - 240.).clamp(280., 560.);
    let min = 280.;
    let fit = fitted_width(ui).clamp(min, max);
    let mut preference = ui
        .ctx()
        .data(|d| d.get_temp::<WidthPreference>(id))
        .unwrap_or_default();

    // Consume this frame's interaction before laying out either side, so
    // the canvas and inspector move together without a frame of overlap.
    if let Some(response) = ui
        .ctx()
        .read_response(divider_id)
        .filter(|r| r.enabled() && ui.is_enabled())
    {
        // A native drag can be delivered entirely between two repaints.
        // egui then sees a release without an ongoing `dragged` response.
        let completed_drag = if egui::Popup::is_any_open(ui.ctx()) {
            None
        } else {
            ui.input(|i| {
                let mut origin = None;
                let mut end = None;
                for event in &i.events {
                    if let egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        ..
                    } = event
                    {
                        if *pressed {
                            origin = response.rect.contains(*pos).then_some(*pos);
                        } else if let Some(start) = origin.take()
                            && pos.distance(start) > 3.
                        {
                            end = Some(*pos);
                        }
                    }
                }
                end
            })
        };
        if let Some(pointer) = completed_drag {
            preference.0 = Some((workspace.right() - pointer.x).clamp(min, max));
            response.request_focus();
        } else if response.double_clicked() {
            preference.0 = None;
        } else if (response.dragged() || response.drag_stopped())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            preference.0 = Some((workspace.right() - pointer.x).clamp(min, max));
        }
        if response.has_focus() {
            for (key, direction) in [(Key::ArrowLeft, 1.), (Key::ArrowRight, -1.)] {
                let step = ui.input_mut(|i| {
                    if i.consume_key(egui::Modifiers::SHIFT, key) {
                        40.
                    } else if i.consume_key(egui::Modifiers::NONE, key) {
                        10.
                    } else {
                        0.
                    }
                });
                if step != 0. {
                    preference.0 =
                        Some((preference.0.unwrap_or(fit) + direction * step).clamp(min, max));
                }
            }
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Home)) {
                preference.0 = None;
            }
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, preference));
    let width = preference.0.unwrap_or(fit).clamp(min, max);
    let panel = egui::SidePanel::right("annotation-inspector")
        .resizable(false) // The divider below also supports reset and keyboard input.
        .exact_width(width)
        .show_separator_line(false)
        .frame(
            egui::Frame::side_top_panel(ui.style())
                .fill(Color32::WHITE)
                .inner_margin(10),
        )
        .show_inside(ui, contents);

    let edge = panel.response.rect.left();
    let grab = Rect::from_min_max(
        Pos2::new(edge - 5., workspace.top()),
        Pos2::new(edge + 5., workspace.bottom()),
    );
    let response = ui
        .interact(grab, divider_id, Sense::click_and_drag())
        .on_hover_cursor(CursorIcon::ResizeHorizontal)
        .on_hover_text("Drag to resize · Double-click to fit\nArrow keys resize · Home resets");
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            "Resize properties panel; double-click to fit",
        )
    });
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    if response.has_focus() {
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                divider_id,
                egui::EventFilter {
                    horizontal_arrows: true,
                    ..Default::default()
                },
            )
        });
    }
    let palette = crate::appearance::palette(ui.ctx());
    let active = response.hovered() || response.dragged() || response.has_focus();
    ui.painter().line_segment(
        [
            Pos2::new(edge, workspace.top()),
            Pos2::new(edge, workspace.bottom()),
        ],
        Stroke::new(
            if active { 2. } else { 1. },
            if active {
                palette.accent
            } else {
                palette.border
            },
        ),
    );
    // A subtle grip makes resizing discoverable without adding a toolbar control.
    ui.painter().rect_filled(
        Rect::from_center_size(Pos2::new(edge, workspace.center().y), Vec2::new(3., 28.)),
        2.,
        if active {
            palette.accent
        } else {
            palette.border
        },
    );
    panel
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document::{Document, EditHistory},
        editor::{EditorState, Tool},
    };

    #[test]
    fn divider_resizes_and_resets_without_drawing_on_the_canvas() {
        let ctx = egui::Context::default();
        crate::appearance::apply(&ctx);
        let texture = ctx.load_texture(
            "inspector-test",
            egui::ColorImage::filled([1, 1], Color32::WHITE),
            Default::default(),
        );
        let mut editor = EditorState {
            tool: Tool::Pen,
            ..Default::default()
        };
        let mut document = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(900, 600)),
            annotations: vec![],
            geometry: Default::default(),
        };
        let before = document.edit_state();
        let mut history = EditHistory::new(before.clone());
        let mut preview = Default::default();
        let mut time = 0.;
        let mut frame = |events, size: Vec2| {
            time += 0.05;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        editor.ui(ui, &mut document, &mut history, &texture, &mut preview);
                    });
                },
            );
            egui::containers::panel::PanelState::load(&ctx, Id::new("annotation-inspector"))
                .unwrap()
                .rect
        };
        let size = Vec2::new(1100., 720.);
        for _ in 0..3 {
            frame(vec![], size);
        }
        let fitted = frame(vec![], size);
        let at = fitted.left_center();
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(at), button(at, true)], size);
        let to = at - Vec2::new(140., 0.);
        frame(vec![egui::Event::PointerMoved(to)], size);
        frame(vec![], size);
        frame(vec![button(to, false)], size);
        let wider = frame(vec![], size);
        assert!(
            (wider.width() - fitted.width() - 140.).abs() < 2.,
            "{fitted:?} -> {wider:?}"
        );
        // A smaller window constrains the width; enlarging it restores the user's choice.
        let small = frame(vec![], Vec2::new(700., 720.));
        assert!(small.left() >= 240.);
        assert!((frame(vec![], size).width() - wider.width()).abs() < 1.);
        let at = wider.left_center();
        for _ in 0..2 {
            frame(vec![egui::Event::PointerMoved(at), button(at, true)], size);
            frame(vec![button(at, false)], size);
        }
        let reset = frame(vec![], size);
        assert!(
            (reset.width() - fitted.width()).abs() < 1.,
            "{reset:?} vs {fitted:?}"
        );
        // Rapid input may arrive between repaints, including the final release.
        let at = reset.left_center();
        let to = at - Vec2::new(80., 0.);
        frame(
            vec![
                egui::Event::PointerMoved(at),
                button(at, true),
                egui::Event::PointerMoved(to),
                button(to, false),
            ],
            size,
        );
        let rapid = frame(vec![], size);
        assert!((rapid.width() - reset.width() - 80.).abs() < 2.);
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let keyboard = frame(vec![key(Key::ArrowLeft, egui::Modifiers::SHIFT)], size);
        assert!((keyboard.width() - rapid.width() - 40.).abs() < 2.);
        let keyboard = frame(vec![key(Key::ArrowLeft, egui::Modifiers::NONE)], size);
        assert!((keyboard.width() - rapid.width() - 50.).abs() < 2.);
        let home = frame(vec![key(Key::Home, egui::Modifiers::NONE)], size);
        assert!((home.width() - fitted.width()).abs() < 1.);
        assert_eq!(document.edit_state(), before);
        assert!(!history.can_undo());
        assert!(editor.gesture.is_none());
    }
}
