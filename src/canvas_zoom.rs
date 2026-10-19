//! Pointer-anchored native pinch/Control-scroll zoom for image surfaces.
use eframe::egui::{self, Id, Rect, Vec2};

#[derive(Clone, Copy)]
struct Frame {
    image: Rect,
    viewport: Rect,
}

pub fn remember(ui: &egui::Ui, salt: &str, image: Rect, viewport: Rect) {
    ui.ctx().data_mut(|d| {
        d.insert_temp(Id::new(salt).with("zoom-frame"), Frame { image, viewport });
    });
}

pub fn prepare(
    ui: &egui::Ui,
    salt: &str,
    zoom: &mut f32,
    fit: &mut bool,
    centered: bool,
    enabled: bool,
) -> Option<Vec2> {
    prepare_with_gutter(ui, salt, zoom, fit, centered, enabled, 0.)
}

pub fn prepare_with_gutter(
    ui: &egui::Ui,
    salt: &str,
    zoom: &mut f32,
    fit: &mut bool,
    centered: bool,
    enabled: bool,
    gutter: f32,
) -> Option<Vec2> {
    let id = ui.make_persistent_id(Id::new(salt));
    let frame = ui
        .ctx()
        .data(|d| d.get_temp::<Frame>(Id::new(salt).with("zoom-frame")))?;
    let (delta, pointer, down) =
        ui.input(|i| (i.zoom_delta(), i.pointer.hover_pos(), i.pointer.any_down()));
    let pointer = pointer?;
    if !enabled
        || !ui.is_enabled()
        || down
        || !delta.is_finite()
        || delta <= 0.
        || delta == 1.
        || !frame.viewport.contains(pointer)
        || ui.ctx().layer_id_at(pointer) != Some(ui.layer_id())
    {
        return None;
    }
    let next = (*zoom * delta).clamp(0.01, 8.);
    let ratio = next / *zoom;
    let size = frame.image.size() * ratio;
    let padding = if centered {
        ((frame.viewport.size() - size) * 0.5).max(Vec2::splat(gutter))
    } else {
        Vec2::ZERO
    };
    let anchor = (pointer - frame.image.min) * ratio;
    let offset = (padding + anchor - (pointer - frame.viewport.min))
        .max(Vec2::ZERO)
        .min((size + Vec2::splat(gutter * 2.) - frame.viewport.size()).max(Vec2::ZERO));
    *zoom = next;
    *fit = false;
    // Clear kinetic scrolling so it cannot compete with the pinch anchor.
    let mut state = egui::scroll_area::State::default();
    state.offset = offset;
    state.store(ui.ctx(), id);
    ui.ctx().request_repaint();
    Some(offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinch_accounts_for_canvas_handle_gutter() {
        let ctx = egui::Context::default();
        let mut zoom = 1.;
        let mut fit = false;
        let viewport = Rect::from_min_size(egui::pos2(8., 8.), Vec2::splat(200.));
        let image = Rect::from_min_size(viewport.min + Vec2::splat(12.), Vec2::splat(300.));
        let pointer = egui::pos2(100., 90.);
        for events in [
            vec![egui::Event::PointerMoved(pointer)],
            vec![egui::Event::Zoom(2.)],
        ] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, Vec2::splat(400.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        if let Some(offset) = prepare_with_gutter(
                            ui,
                            "gutter-test",
                            &mut zoom,
                            &mut fit,
                            true,
                            true,
                            12.,
                        ) {
                            assert_eq!(
                                viewport.min + Vec2::splat(12.) + (pointer - image.min) * 2.
                                    - offset,
                                pointer
                            );
                            assert_eq!(zoom, 2.);
                        }
                        remember(ui, "gutter-test", image, viewport);
                    });
                },
            );
        }
        assert_eq!(zoom, 2.);
    }

    #[test]
    fn native_pinch_preserves_pointer_anchor_and_ignores_inspector_or_drag_input() {
        let ctx = egui::Context::default();
        let mut zoom = 1.;
        let mut fit = true;
        let mut image = Rect::from_min_size(egui::pos2(20., 20.), Vec2::splat(300.));
        let viewport = Rect::from_min_size(egui::pos2(20., 20.), Vec2::splat(200.));
        let pointer = egui::pos2(100., 90.);
        let mut offset = None;
        let mut frame = |events: Vec<egui::Event>, zoom: &mut f32, fit: &mut bool, image: Rect| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, Vec2::splat(400.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        offset = prepare(ui, "test-canvas", zoom, fit, true, true);
                        if let Some(offset) = offset {
                            let state = egui::scroll_area::State::load(
                                ctx,
                                ui.make_persistent_id(Id::new("test-canvas")),
                            )
                            .expect("pinch must reset the actual scroll area's kinetic state");
                            assert_eq!(state.offset, offset);
                            assert_eq!(state.velocity(), Vec2::ZERO);
                        }
                        remember(ui, "test-canvas", image, viewport);
                    });
                },
            );
            offset
        };
        frame(
            vec![egui::Event::PointerMoved(pointer)],
            &mut zoom,
            &mut fit,
            image,
        );
        let anchor = pointer - image.min;
        let offset = frame(vec![egui::Event::Zoom(2.)], &mut zoom, &mut fit, image).unwrap();
        assert_eq!(zoom, 2.);
        assert!(!fit);
        assert_eq!(viewport.min + anchor * 2. - offset, pointer);
        image = Rect::from_min_size(viewport.min - offset, Vec2::splat(600.));
        frame(vec![], &mut zoom, &mut fit, image);
        let offset = frame(vec![egui::Event::Zoom(0.5)], &mut zoom, &mut fit, image).unwrap();
        assert_eq!(zoom, 1.);
        assert_eq!(offset, Vec2::ZERO);
        assert!(
            frame(
                vec![
                    egui::Event::PointerMoved(egui::pos2(350., 100.)),
                    egui::Event::Zoom(2.)
                ],
                &mut zoom,
                &mut fit,
                image
            )
            .is_none()
        );
        assert_eq!(zoom, 1.);
        assert!(
            frame(
                vec![
                    egui::Event::PointerMoved(pointer),
                    egui::Event::PointerButton {
                        pos: pointer,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default()
                    },
                    egui::Event::Zoom(2.)
                ],
                &mut zoom,
                &mut fit,
                image
            )
            .is_none()
        );
        assert_eq!(zoom, 1.);
    }
}
