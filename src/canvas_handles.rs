//! Direct canvas-boundary editing. A gesture changes the visible window, never
//! the source pixels or object geometry; preview input stays in a fixed frame.
use super::*;

pub(super) const GUTTER: f32 = 12.;

#[derive(Clone, Copy, Debug)]
struct Edge(i8, i8);
const EDGES: [Edge; 8] = [
    Edge(-1, -1),
    Edge(0, -1),
    Edge(1, -1),
    Edge(1, 0),
    Edge(1, 1),
    Edge(0, 1),
    Edge(-1, 1),
    Edge(-1, 0),
];

impl Edge {
    fn is_corner(self) -> bool {
        self.0 != 0 && self.1 != 0
    }
    fn position(self, rect: Rect) -> Pos2 {
        rect.center()
            + rect.size() * egui::vec2(self.0 as f32, self.1 as f32) * 0.5
            + egui::vec2(self.0 as f32, self.1 as f32) * 5.
    }
    fn cursor(self) -> CursorIcon {
        match (self.0, self.1) {
            (0, _) => CursorIcon::ResizeVertical,
            (_, 0) => CursorIcon::ResizeHorizontal,
            (x, y) if x == y => CursorIcon::ResizeNwSe,
            _ => CursorIcon::ResizeNeSw,
        }
    }
}

pub(super) struct CanvasResize {
    before: EditState,
    edge: Edge,
    press: Pos2,
    frame: Rect,
    scale: f32,
    size: Vec2,
    output: Rect,
    geometry: ImageGeometry,
    source: Option<Document>,
    texture: Option<egui::TextureHandle>,
    error: Option<String>,
}

pub(super) fn fit_scale(viewport: Vec2, size: Vec2) -> f32 {
    ((viewport.x - GUTTER * 2.) / size.x)
        .min((viewport.y - GUTTER * 2.) / size.y)
        .clamp(0.001, 1.)
}

fn hit(rect: Rect, position: Pos2) -> Option<Edge> {
    // Canvas targets are outside the image; object and area-selection handles
    // inside the image keep their normal priority.
    if rect.contains(position) {
        return None;
    }
    EDGES
        .into_iter()
        .map(|edge| (edge, edge.position(rect).distance(position)))
        .filter(|(_, distance)| *distance <= 8.)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(edge, _)| edge)
}

fn resized_output(size: Vec2, edge: Edge, delta: Vec2, lock_aspect: bool) -> Rect {
    let mut rect = Rect::from_min_size(Pos2::ZERO, size);
    if lock_aspect && edge.is_corner() {
        // The more deliberate axis drives the resize. Keep the opposite corner
        // fixed, and derive both dimensions from the gesture's original ratio.
        let relative = delta * egui::vec2(edge.0 as f32, edge.1 as f32) / size;
        let change = if relative.x.abs() >= relative.y.abs() {
            relative.x
        } else {
            relative.y
        };
        let scale = (1. + change).max((1. / size.x).max(1. / size.y));
        let next = (size * scale).round().max(Vec2::splat(1.));
        if edge.0 < 0 {
            rect.min.x = size.x - next.x;
        } else {
            rect.max.x = next.x;
        }
        if edge.1 < 0 {
            rect.min.y = size.y - next.y;
        } else {
            rect.max.y = next.y;
        }
        return rect;
    }
    let delta = delta.round();
    if edge.0 < 0 {
        rect.min.x = delta.x.min(size.x - 1.);
    }
    if edge.0 > 0 {
        rect.max.x = (size.x + delta.x).max(1.);
    }
    if edge.1 < 0 {
        rect.min.y = delta.y.min(size.y - 1.);
    }
    if edge.1 > 0 {
        rect.max.y = (size.y + delta.y).max(1.);
    }
    rect
}

fn apply_output_rect(document: &mut Document, output: Rect) -> Result<(), String> {
    let size = [output.width() as u32, output.height() as u32];
    crate::document::validate_dimensions(size[0], size[1])?;
    let inverse = |p: Pos2| {
        point(
            document
                .geometry
                .unwarped()
                .inverse_point(from_pos(p), original_size(document)),
        )
    };
    let source = Rect::from_two_pos(inverse(output.min), inverse(output.max));
    crate::canvas::set_view_rect(document, source)?;
    if document.output_size() != size {
        document.geometry.resize = Some(size);
    }
    document.geometry.validate(original_size(document))
}

impl EditorState {
    fn canvas_handles_available(&self, document: &Document) -> bool {
        self.crop.is_none()
            && !(self.tool == Tool::Area && self.area_bounds(document).is_some())
            && self.gesture.is_none()
            && self.area_drag.is_none()
            && !self.has_modal()
    }

    pub(super) fn finish_canvas_resize(
        &mut self,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if let Some(drag) = self.canvas_resize.take() {
            if let Some(error) = drag.error {
                self.image_error = Some(error);
            } else if drag.output != Rect::from_min_size(Pos2::ZERO, drag.size) {
                document.geometry = drag.geometry;
                history.commit(drag.before, document);
            }
        }
    }

    /// Return (owns input, painted preview). A click without movement uses the
    /// existing image texture, avoiding a quality change just from pressing.
    pub(super) fn canvas_resize_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
        rect: Rect,
        scale: f32,
        viewport: Vec2,
    ) -> (bool, bool) {
        if !ui.is_enabled() {
            return (false, false);
        }
        if self.canvas_resize.is_none()
            && self.canvas_handles_available(document)
            && !egui::Popup::is_any_open(ui.ctx())
        {
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
            if let Some(position) = press
                && ui.clip_rect().contains(position)
                && ui.ctx().layer_id_at(position) == Some(ui.layer_id())
                && let Some(edge) = hit(rect, position)
            {
                let response = ui.interact(
                    Rect::from_center_size(edge.position(rect), Vec2::splat(16.)),
                    ui.id().with("canvas-resize"),
                    Sense::drag(),
                );
                response.request_focus();
                self.finish_properties(document, history);
                self.clear_selection();
                self.deselect_pixels();
                self.image_error = None;
                let [w, h] = document.output_size();
                let size = egui::vec2(w as f32, h as f32);
                self.canvas_resize = Some(CanvasResize {
                    before: document.edit_state(),
                    edge,
                    press: position,
                    frame: rect,
                    scale,
                    size,
                    output: Rect::from_min_size(Pos2::ZERO, size),
                    geometry: document.geometry,
                    source: None,
                    texture: None,
                    error: None,
                });
            }
        }
        let Some(drag) = &mut self.canvas_resize else {
            return (false, false);
        };
        if ui.input_mut(|i| {
            i.consume_key(Modifiers::NONE, Key::Escape)
                || i.consume_key(Modifiers::SHIFT, Key::Escape)
        }) || !ui.input(|i| i.focused)
        {
            self.canvas_resize = None;
            self.focus_canvas = true;
            ui.ctx().request_repaint();
            return (true, false);
        }
        let aspect_locked = drag.edge.is_corner() && ui.input(|i| i.modifiers.shift);
        if let Some(position) = ui.input(|i| i.pointer.interact_pos()) {
            let output = resized_output(
                drag.size,
                drag.edge,
                (position - drag.press) / drag.scale,
                aspect_locked,
            );
            if output != drag.output || drag.error.is_some() {
                let mut next = document.clone();
                next.geometry = drag.before.geometry;
                match apply_output_rect(&mut next, output) {
                    Ok(()) => {
                        drag.geometry = next.geometry;
                        drag.output = output;
                        drag.error = None;
                    }
                    Err(error) => drag.error = Some(error),
                }
            }
        }
        if !ui.input(|i| i.pointer.primary_down()) {
            self.finish_canvas_resize(document, history);
            self.focus_canvas = true;
            ui.ctx().request_repaint();
            return (true, false);
        }
        if drag.output == Rect::from_min_size(Pos2::ZERO, drag.size) && drag.error.is_none() {
            ui.ctx().set_cursor_icon(drag.edge.cursor());
            return (true, false);
        }
        let mut display = document.clone();
        display.geometry = drag.geometry;
        let paint_rect = if self.fit {
            let zoom = fit_scale(viewport, drag.output.size());
            Rect::from_center_size(drag.frame.center(), drag.output.size() * zoom)
        } else {
            Rect::from_min_max(
                drag.frame.min + drag.output.min.to_vec2() * drag.scale,
                drag.frame.min + drag.output.max.to_vec2() * drag.scale,
            )
        };
        // Render a fixed source-coordinate texture once. Changing only the UV
        // window during the drag avoids resampling shimmer and black flashes.
        let view = display.geometry.view_rect(original_size(document));
        let view_rect = Rect::from_min_size(
            egui::pos2(view.x as f32, view.y as f32),
            egui::vec2(view.width as f32, view.height as f32),
        );
        let source_contains = drag.source.as_ref().is_some_and(|source| {
            let b = source.geometry.canvas_rect(original_size(document));
            Rect::from_min_size(
                egui::pos2(b.x as f32, b.y as f32),
                egui::vec2(b.width as f32, b.height as f32),
            )
            .contains_rect(view_rect)
        });
        if !source_contains {
            let reserve = ui.clip_rect().expand(64.);
            let source_rect = Rect::from_two_pos(
                point(object_point(reserve.min, drag.frame, drag.scale, document)),
                point(object_point(reserve.max, drag.frame, drag.scale, document)),
            )
            .union(view_rect);
            let mut source = document.clone();
            // If reserving extra space exceeds limits, use the validated draft.
            if crate::canvas::set_view_rect(&mut source, source_rect).is_err() {
                source = display.clone();
            }
            let texture = render::interactive_preview(&source, 768).map(|pixels| {
                ui.ctx().load_texture(
                    "canvas resize composition",
                    crate::image_loading::color_image(&pixels),
                    egui::TextureOptions::LINEAR,
                )
            });
            match texture {
                Ok(texture) => {
                    drag.source = Some(source);
                    drag.texture = Some(texture);
                }
                Err(error) => {
                    drag.error = Some(error);
                }
            }
        }
        let painter = ui
            .painter()
            .with_clip_rect(paint_rect.intersect(ui.clip_rect()));
        paint_checkerboard_at(&painter, paint_rect, drag.frame.min);
        if let (Some(source), Some(texture)) = (&drag.source, &drag.texture) {
            let mut presentation = display;
            presentation.geometry.canvas =
                Some(source.geometry.canvas_rect(original_size(document)));
            presentation.geometry.viewport = Some(view);
            render::paint_document_image(&painter, texture.id(), &presentation, paint_rect);
        }
        paint_handles(ui.painter(), paint_rect, Some(drag.edge));
        let message = drag.error.as_deref().map_or_else(
            || {
                format!(
                    "{} × {} px{} · Esc to cancel",
                    drag.output.width() as u32,
                    drag.output.height() as u32,
                    if aspect_locked {
                        " · Ratio locked"
                    } else {
                        ""
                    },
                )
            },
            |_| "Canvas limit reached · Esc to cancel".to_owned(),
        );
        let galley = ui.painter().layout(
            message,
            egui::FontId::proportional(12.),
            Color32::WHITE,
            (ui.clip_rect().width() - 32.).max(1.),
        );
        let badge = Rect::from_min_size(
            paint_rect.left_top() + egui::vec2(8., 8.),
            galley.size() + egui::vec2(16., 10.),
        );
        let clip = ui.clip_rect().shrink(4.);
        let badge = Rect::from_min_size(
            badge
                .min
                .clamp(clip.min, (clip.max - badge.size()).max(clip.min)),
            badge.size(),
        );
        ui.painter()
            .rect_filled(badge, 4., crate::appearance::palette(ui.ctx()).navy);
        ui.painter()
            .galley(badge.min + egui::vec2(8., 5.), galley, Color32::WHITE);
        ui.ctx().set_cursor_icon(drag.edge.cursor());
        (true, true)
    }

    pub(super) fn paint_canvas_handles(&self, ui: &mut egui::Ui, document: &Document, rect: Rect) {
        if !self.canvas_handles_available(document) {
            return;
        }
        let hover = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|p| ui.clip_rect().contains(*p))
            .filter(|p| ui.ctx().layer_id_at(*p) == Some(ui.layer_id()))
            .and_then(|p| hit(rect, p));
        paint_handles(ui.painter(), rect, hover);
        if ui.is_enabled()
            && !egui::Popup::is_any_open(ui.ctx())
            && let Some(edge) = hover
        {
            ui.ctx().set_cursor_icon(edge.cursor());
            ui.interact(
                Rect::from_center_size(edge.position(rect), Vec2::splat(16.)),
                ui.id().with("canvas-resize-hint"),
                Sense::hover(),
            )
            .on_hover_text(
                "Drag to resize canvas. Hold Shift on a corner to keep its aspect ratio. Artwork keeps its size; hidden content is preserved.",
            );
        }
    }
}

fn paint_handles(painter: &egui::Painter, rect: Rect, hover: Option<Edge>) {
    painter.rect_stroke(
        rect.expand(0.5),
        0.,
        Stroke::new(1., Color32::from_gray(141)),
        StrokeKind::Outside,
    );
    for edge in EDGES {
        let active = hover.is_some_and(|h| h.0 == edge.0 && h.1 == edge.1);
        painter.rect(
            Rect::from_center_size(edge.position(rect), Vec2::splat(6.)),
            0.,
            if active {
                crate::appearance::palette(painter.ctx()).ginger
            } else {
                Color32::from_gray(245)
            },
            Stroke::new(1., crate::appearance::palette(painter.ctx()).navy),
            StrokeKind::Inside,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn fixture() -> Document {
        Document {
            image: Arc::new(image::RgbaImage::from_pixel(
                200,
                140,
                image::Rgba([25, 75, 150, 255]),
            )),
            annotations: vec![],
            geometry: Default::default(),
        }
    }
    fn button(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }
    fn frame(
        ctx: &egui::Context,
        e: &mut EditorState,
        d: &mut Document,
        h: &mut EditHistory,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        frame_with_modifiers(ctx, e, d, h, events, Modifiers::NONE)
    }
    fn frame_with_modifiers(
        ctx: &egui::Context,
        e: &mut EditorState,
        d: &mut Document,
        h: &mut EditHistory,
        mut events: Vec<egui::Event>,
        modifiers: Modifiers,
    ) -> egui::FullOutput {
        for event in &mut events {
            if let egui::Event::PointerButton { modifiers: m, .. } = event {
                *m = modifiers;
            }
        }
        let texture = ctx.load_texture(
            "canvas handle fixture",
            egui::ColorImage::new([1, 1], vec![Color32::BLUE]),
            Default::default(),
        );
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(500., 400.))),
                events,
                modifiers,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    e.canvas(ui, d, h, &texture, &mut Default::default());
                });
            },
        )
    }

    #[test]
    fn shift_corners_keep_displayed_ratio_and_opposite_corner_fixed() {
        for turns in 0..4 {
            let mut d = fixture();
            d.geometry.quarter_turns = turns;
            let [w, h] = d.output_size();
            d.geometry.resize = Some([w * 2, h * 3]);
            let [w, h] = d.output_size();
            let size = egui::vec2(w as f32, h as f32);
            for edge in EDGES {
                for amount in [-0.4, 0., 0.5, 2.] {
                    let delta = egui::vec2(edge.0 as f32 * size.x * amount, 0.);
                    let output = resized_output(size, edge, delta, true);
                    if edge.is_corner() {
                        let expected = size * (1. + amount);
                        assert!((output.size() - expected).abs().max_elem() <= 0.5);
                        let opposite = Edge(-edge.0, -edge.1);
                        assert_eq!(
                            opposite.position(output),
                            opposite.position(Rect::from_min_size(Pos2::ZERO, size))
                        );
                        let original = d.edit_state();
                        apply_output_rect(&mut d, output).unwrap();
                        assert_eq!(
                            d.output_size(),
                            [expected.x.round() as u32, expected.y.round() as u32]
                        );
                        assert!(d.annotations.is_empty());
                        d.geometry = original.geometry;
                    } else {
                        assert_eq!(output, resized_output(size, edge, delta, false));
                    }
                }
                let smallest = resized_output(
                    size,
                    edge,
                    -size * egui::vec2(edge.0 as f32, edge.1 as f32) * 10.,
                    true,
                );
                assert!(smallest.width() >= 1. && smallest.height() >= 1.);
            }
        }
    }

    #[test]
    fn shift_can_toggle_during_canvas_drag_without_drift_and_escape_cancels() {
        for cancel in [false, true] {
            let mut d = fixture();
            let before = d.edit_state();
            let source = d.image.clone();
            let mut h = EditHistory::new(before.clone());
            let mut e = EditorState {
                fit: false,
                zoom: 1.,
                tool: Tool::Arrow,
                ..Default::default()
            };
            let ctx = egui::Context::default();
            frame(&ctx, &mut e, &mut d, &mut h, vec![]);
            let start = egui::pos2(355., 275.);
            let end = start + egui::vec2(100., 10.);
            frame(
                &ctx,
                &mut e,
                &mut d,
                &mut h,
                vec![egui::Event::PointerMoved(start), button(start, true)],
            );
            frame(
                &ctx,
                &mut e,
                &mut d,
                &mut h,
                vec![egui::Event::PointerMoved(end)],
            );
            for modifiers in [
                Modifiers::SHIFT,
                Modifiers::NONE,
                Modifiers::SHIFT,
                Modifiers::SHIFT,
            ] {
                frame_with_modifiers(&ctx, &mut e, &mut d, &mut h, vec![], modifiers);
                let size = e.canvas_resize.as_ref().unwrap().output.size();
                assert_eq!(
                    size,
                    if modifiers.shift {
                        egui::vec2(300., 210.)
                    } else {
                        egui::vec2(300., 150.)
                    }
                );
                assert_eq!(d.edit_state(), before);
                assert!(!h.can_undo());
            }
            if cancel {
                frame_with_modifiers(
                    &ctx,
                    &mut e,
                    &mut d,
                    &mut h,
                    vec![egui::Event::Key {
                        key: Key::Escape,
                        physical_key: Some(Key::Escape),
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::SHIFT,
                    }],
                    Modifiers::SHIFT,
                );
            }
            frame_with_modifiers(
                &ctx,
                &mut e,
                &mut d,
                &mut h,
                vec![button(end, false)],
                Modifiers::SHIFT,
            );
            assert!(e.canvas_resize.is_none());
            assert!(Arc::ptr_eq(&source, &d.image));
            assert!(d.annotations.is_empty());
            if cancel {
                assert_eq!(d.edit_state(), before);
                assert!(!h.can_undo());
            } else {
                assert_eq!(d.output_size(), [300, 210]);
                assert!(h.undo(&mut d));
                assert_eq!(d.edit_state(), before);
                assert!(!h.can_undo());
                assert!(h.redo(&mut d));
                assert_eq!(d.output_size(), [300, 210]);
            }
        }
    }

    #[test]
    fn every_canvas_handle_has_fixed_preview_and_one_undo_in_drawing_modes() {
        for tool in [
            Tool::Area,
            Tool::Arrow,
            Tool::Pen,
            Tool::Fill,
            Tool::Wand,
            Tool::Text,
            Tool::Stamp,
        ] {
            for edge in EDGES {
                let mut d = fixture();
                let before = d.edit_state();
                let image = d.image.clone();
                let mut h = EditHistory::new(before.clone());
                let mut e = EditorState {
                    fit: false,
                    zoom: 1.,
                    tool,
                    ..Default::default()
                };
                let ctx = egui::Context::default();
                frame(&ctx, &mut e, &mut d, &mut h, vec![]);
                let rect = Rect::from_min_size(egui::pos2(150., 130.), egui::vec2(200., 140.));
                let start = edge.position(rect) + egui::vec2(0.5, 0.5);
                frame(
                    &ctx,
                    &mut e,
                    &mut d,
                    &mut h,
                    vec![egui::Event::PointerMoved(start), button(start, true)],
                );
                assert!(e.canvas_resize.is_some(), "{tool:?}: {edge:?}");
                assert_eq!(
                    d.edit_state(),
                    before,
                    "A handle press must not edit anything"
                );
                assert!(!e.autosave_ready());
                let end = start + egui::vec2(edge.0 as f32 * 32., edge.1 as f32 * 24.);
                frame(
                    &ctx,
                    &mut e,
                    &mut d,
                    &mut h,
                    vec![egui::Event::PointerMoved(end)],
                );
                let draft = e.canvas_resize.as_ref().unwrap();
                let geometry = draft.geometry;
                let texture = draft.texture.as_ref().unwrap().id();
                for _ in 0..3 {
                    frame(
                        &ctx,
                        &mut e,
                        &mut d,
                        &mut h,
                        vec![egui::Event::PointerMoved(end)],
                    );
                    let draft = e.canvas_resize.as_ref().unwrap();
                    assert_eq!(
                        draft.geometry, geometry,
                        "Stationary pointer must not drift"
                    );
                    assert_eq!(
                        draft.texture.as_ref().unwrap().id(),
                        texture,
                        "Stable sampling frame must reuse its texture"
                    );
                    assert_eq!(d.edit_state(), before);
                    assert!(!h.can_undo());
                }
                frame(&ctx, &mut e, &mut d, &mut h, vec![button(end, false)]);
                assert!(e.canvas_resize.is_none());
                assert!(e.autosave_ready());
                assert_eq!(
                    d.output_size(),
                    [
                        200 + if edge.0 == 0 { 0 } else { 32 },
                        140 + if edge.1 == 0 { 0 } else { 24 }
                    ]
                );
                assert!(Arc::ptr_eq(&image, &d.image));
                assert!(
                    d.annotations.is_empty(),
                    "Handle input must not create objects"
                );
                let rendered = render::flatten(&d).unwrap();
                assert_eq!(rendered.pixels().filter(|p| p[3] > 0).count(), 200 * 140);
                assert!(h.undo(&mut d));
                assert_eq!(d.edit_state(), before);
                assert!(!h.can_undo());
                assert!(h.redo(&mut d));
                assert_eq!(d.geometry, geometry);
            }
        }
    }

    #[test]
    fn canvas_handle_click_escape_and_invalid_growth_preserve_document() {
        for mode in [0, 1, 2] {
            let mut d = fixture();
            let before = d.edit_state();
            let mut h = EditHistory::new(before.clone());
            let mut e = EditorState {
                fit: false,
                zoom: 1.,
                ..Default::default()
            };
            let ctx = egui::Context::default();
            frame(&ctx, &mut e, &mut d, &mut h, vec![]);
            let start = egui::pos2(355., 200.);
            frame(
                &ctx,
                &mut e,
                &mut d,
                &mut h,
                vec![egui::Event::PointerMoved(start), button(start, true)],
            );
            let end = start
                + egui::vec2(
                    if mode == 2 {
                        1_000_000.
                    } else if mode == 1 {
                        40.
                    } else {
                        0.
                    },
                    0.,
                );
            frame(
                &ctx,
                &mut e,
                &mut d,
                &mut h,
                vec![egui::Event::PointerMoved(end)],
            );
            if mode == 1 {
                frame(
                    &ctx,
                    &mut e,
                    &mut d,
                    &mut h,
                    vec![egui::Event::Key {
                        key: Key::Escape,
                        physical_key: Some(Key::Escape),
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    }],
                );
            }
            frame(&ctx, &mut e, &mut d, &mut h, vec![button(end, false)]);
            assert_eq!(d.edit_state(), before);
            assert!(!h.can_undo());
            assert!(e.canvas_resize.is_none());
            assert_eq!(e.image_error.is_some(), mode == 2);
        }
    }

    #[test]
    fn quick_canvas_drag_consumes_drawing_input_and_keyboard_undo_keeps_focus() {
        let mut d = fixture();
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState {
            fit: false,
            zoom: 1.,
            tool: Tool::Arrow,
            ..Default::default()
        };
        let ctx = egui::Context::default();
        frame(&ctx, &mut e, &mut d, &mut h, vec![]);
        let start = egui::pos2(355., 200.);
        let end = start + egui::vec2(40., 0.);
        frame(
            &ctx,
            &mut e,
            &mut d,
            &mut h,
            vec![
                egui::Event::PointerMoved(start),
                button(start, true),
                egui::Event::PointerMoved(end),
                button(end, false),
            ],
        );
        assert_eq!(d.output_size(), [240, 140]);
        assert!(d.annotations.is_empty());
        frame(
            &ctx,
            &mut e,
            &mut d,
            &mut h,
            vec![egui::Event::Key {
                key: Key::Z,
                physical_key: Some(Key::Z),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::COMMAND,
            }],
        );
        assert_eq!(d.edit_state(), before);
        assert!(!h.can_undo());
    }

    #[test]
    fn handle_geometry_preserves_hidden_layers_through_transforms_and_project_roundtrip() {
        for turns in 0..4 {
            for flipped in [false, true] {
                let mut d = fixture();
                d.annotations.push(crate::markup_tests::object(
                    AnnotationKind::Rectangle,
                    [130., 90.],
                    [180., 125.],
                ));
                d.geometry.quarter_turns = turns;
                d.geometry.flip_horizontal = flipped;
                d.geometry.flip_vertical = !flipped;
                let [w, h] = d.output_size();
                d.geometry.resize = Some([w * 2, h * 3]);
                let original = render::flatten(&d).unwrap();
                let before = d.edit_state();
                let source = d.image.clone();
                let size = d.output_size();
                let inset = egui::vec2(40., 30.);
                let region = Rect::from_min_max(
                    inset.to_pos2(),
                    egui::pos2(size[0] as f32 - 40., size[1] as f32 - 30.),
                );
                apply_output_rect(&mut d, region).unwrap();
                let actual = render::flatten(&d).unwrap();
                let expected =
                    image::imageops::crop_imm(&original, 40, 30, size[0] - 80, size[1] - 60)
                        .to_image();
                assert_eq!(actual.dimensions(), expected.dimensions());
                // Cropping before the existing resize filter can change edge
                // antialiasing by a few levels; geometry and retained data stay exact.
                let max_error = actual
                    .pixels()
                    .zip(expected.pixels())
                    .flat_map(|(a, b)| a.0.into_iter().zip(b.0).map(|(a, b)| a.abs_diff(b)))
                    .max()
                    .unwrap();
                assert!(
                    max_error <= 3,
                    "turns={turns} flip={flipped}, max channel error={max_error}"
                );
                assert_eq!(d.annotations, before.annotations);
                assert!(Arc::ptr_eq(&source, &d.image));
                let dir = tempfile::tempdir().unwrap();
                let file = dir.path().join("hidden-canvas.myimage");
                crate::document::save_project(&d, &file).unwrap();
                let mut loaded = crate::document::load_project(&file).unwrap();
                apply_output_rect(
                    &mut loaded,
                    Rect::from_min_max(
                        egui::pos2(-40., -30.),
                        egui::pos2(size[0] as f32 - 40., size[1] as f32 - 30.),
                    ),
                )
                .unwrap();
                assert!(
                    render::flatten(&loaded).unwrap() == original,
                    "Expanded project must restore the original composition, turns={turns} flip={flipped}"
                );
                assert_eq!(loaded.annotations, before.annotations);
            }
        }
    }
}
