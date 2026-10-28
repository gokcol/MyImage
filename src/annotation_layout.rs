//! Layout operates in displayed-image coordinates, retaining editable source geometry.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy)]
pub(super) enum LayoutAction {
    Align { axis: usize, anchor: usize },
    Distribute { axis: usize },
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SnapGuide {
    pub axis: usize,
    pub position: f32,
}

/// A complete group is one layout unit. Individually selected members stay independent.
fn layout_units(document: &Document, indices: &[usize]) -> Vec<Vec<usize>> {
    let selected: BTreeSet<_> = indices.iter().copied().collect();
    let mut groups: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
    for (i, a) in document.annotations.iter().enumerate() {
        if let Some(id) = a.group_id {
            groups.entry(id).or_default().push(i);
        }
    }
    let mut used = BTreeSet::new();
    let mut units = Vec::new();
    for &i in indices {
        if used.contains(&i) {
            continue;
        }
        let unit = document.annotations[i]
            .group_id
            .and_then(|id| groups.get(&id))
            .filter(|members| members.iter().all(|i| selected.contains(i)))
            .cloned()
            .unwrap_or_else(|| vec![i]);
        used.extend(unit.iter().copied());
        units.push(unit);
    }
    units
}

fn anchors(bounds: Rect, axis: usize) -> [f32; 3] {
    [bounds.min[axis], bounds.center()[axis], bounds.max[axis]]
}

impl EditorState {
    pub(super) fn layout_controls(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        ui.checkbox(&mut self.snapping, "Snap to guides")
            .on_hover_text("Snap while moving to image and annotation edges or centers. Hold Alt/Option to bypass.");
        let units = layout_units(document, &self.selected_indices(document));
        if units.len() < 2 || !self.can_edit_selection(document) {
            return;
        }
        egui::CollapsingHeader::new("Align & space").default_open(true).show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
            ui.spacing_mut().button_padding = Vec2::new(5.0, 5.0);
            let column = (ui.available_width() - 12.0) / 3.0;
            for (axis, labels) in [(0, ["Left", "Center", "Right"]), (1, ["Top", "Middle", "Bottom"])] {
                ui.horizontal(|ui| {
                    for (anchor, label) in labels.into_iter().enumerate() {
                        if ui.add_sized([column, 28.0], egui::Button::new(label)).on_hover_text(format!("Align {label} within the selection")).clicked() {
                            self.apply_layout(LayoutAction::Align { axis, anchor }, document, history);
                        }
                    }
                });
            }
            ui.add_enabled_ui(units.len() >= 3, |ui| {
                for (axis, label) in [(0, "Space horizontally"), (1, "Space vertically")] {
                    if ui.add_sized([ui.available_width(), 28.0], egui::Button::new(label))
                        .on_hover_text("Equal gaps; the first and last objects stay in place. Requires three objects or groups.")
                        .clicked() {
                        self.apply_layout(LayoutAction::Distribute { axis }, document, history);
                    }
                }
            });
            ui.weak("Whole groups move as one object.");
        });
    }

    pub(super) fn apply_layout(
        &mut self,
        action: LayoutAction,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let indices = self.selected_indices(document);
        if !annotation_layers::editable(document, &indices) {
            return;
        }
        let units = layout_units(document, &indices);
        let required = if matches!(action, LayoutAction::Distribute { .. }) {
            3
        } else {
            2
        };
        if units.len() < required {
            return;
        }
        let bounds: Vec<_> = units
            .iter()
            .map(|unit| mapped_bounds(selection_bounds(document, unit), document))
            .collect();
        let mut deltas = vec![Vec2::ZERO; units.len()];
        match action {
            LayoutAction::Align { axis, anchor } => {
                let total = bounds.iter().copied().fold(Rect::NOTHING, Rect::union);
                let target = anchors(total, axis)[anchor];
                for (i, bounds) in bounds.iter().enumerate() {
                    deltas[i][axis] = target - anchors(*bounds, axis)[anchor];
                }
            }
            LayoutAction::Distribute { axis } => {
                let mut order: Vec<_> = (0..units.len()).collect();
                order.sort_by(|&a, &b| bounds[a].min[axis].total_cmp(&bounds[b].min[axis]));
                let first = order[0];
                let last = *order.last().unwrap();
                let sizes: f32 = bounds.iter().map(|b| b.size()[axis]).sum();
                let gap = (bounds[last].max[axis] - bounds[first].min[axis] - sizes)
                    / (units.len() - 1) as f32;
                let mut cursor = bounds[first].max[axis] + gap;
                for &i in &order[1..order.len() - 1] {
                    deltas[i][axis] = cursor - bounds[i].min[axis];
                    cursor += bounds[i].size()[axis] + gap;
                }
            }
        }
        let before = document.edit_state();
        let mut candidate = document.clone();
        for (unit, delta) in units.iter().zip(deltas) {
            move_selection(
                &mut candidate,
                &before.annotations,
                unit,
                source_delta(delta, document),
            );
        }
        if let Err(error) = crate::document::validate_document(&candidate) {
            self.image_error = Some(format!("Cannot arrange selection: {error}"));
            return;
        }
        document.annotations = candidate.annotations;
        history.commit(before, document);
        self.set_selection(indices, document);
        self.focus_canvas = true;
        self.image_error = None;
    }

    pub(super) fn update_snapped_gesture(
        &mut self,
        at: Point,
        document: &mut Document,
        bypass: bool,
    ) {
        self.snap_guides.clear();
        let Some(gesture) = &self.gesture else {
            return;
        };
        let indices = match &gesture.kind {
            GestureKind::Move(i) => vec![*i],
            GestureKind::MoveMany(indices) => indices.clone(),
            _ => {
                self.update_gesture(at, document);
                return;
            }
        };
        if !annotation_layers::editable(document, &indices) {
            return;
        }
        // Merely clicking must never reposition an annotation near a guide.
        let delta = point(at) - point(gesture.origin);
        if delta.length_sq() < 0.000001 {
            self.update_gesture(at, document);
            return;
        }
        let original = Document {
            image: document.image.clone(),
            annotations: gesture.before.annotations.clone(),
            geometry: gesture.before.geometry,
        };
        let (delta, guides) = snapped_delta(
            &original,
            &indices,
            delta,
            self.zoom,
            self.snapping && !bypass,
        );
        move_selection(document, &gesture.before.annotations, &indices, delta);
        self.snap_guides = guides;
    }

    pub(super) fn paint_snap_guides(&self, painter: &egui::Painter, rect: Rect, scale: f32) {
        if self.gesture.is_none() {
            return;
        }
        for guide in &self.snap_guides {
            let mut a = rect.min;
            let mut b = rect.max;
            a[guide.axis] = rect.min[guide.axis] + guide.position * scale;
            b[guide.axis] = a[guide.axis];
            painter.line_segment([a, b], Stroke::new(1.2, Color32::from_rgb(226, 62, 160)));
        }
    }
}

fn snapped_delta(
    document: &Document,
    indices: &[usize],
    raw: Vec2,
    zoom: f32,
    enabled: bool,
) -> (Vec2, Vec<SnapGuide>) {
    if !enabled {
        return (raw, Vec::new());
    }
    let source_bounds = selection_bounds(document, indices);
    let moved = mapped_bounds(source_bounds.translate(raw), document);
    let [w, h] = document.output_size();
    let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::new(w as f32, h as f32));
    let selected: BTreeSet<_> = indices.iter().copied().collect();
    let others: Vec<_> = (0..document.annotations.len())
        .filter(|i| !selected.contains(i) && document.annotations[*i].visible())
        .collect();
    let mut targets = vec![canvas];
    for unit in layout_units(document, &others) {
        let bounds = mapped_bounds(selection_bounds(document, &unit), document);
        if bounds.intersects(canvas) {
            targets.push(bounds);
        }
    }
    let threshold = 6.0 / zoom.max(0.001);
    let mut correction = Vec2::ZERO;
    let mut guides = Vec::new();
    for axis in 0..2 {
        let mut nearest: Option<(f32, f32)> = None;
        for target in &targets {
            for position in anchors(*target, axis) {
                if position < 0.0 || position > canvas.max[axis] {
                    continue;
                }
                for edge in anchors(moved, axis) {
                    let delta = position - edge;
                    if delta.abs() <= threshold
                        && nearest.is_none_or(|(best, _)| delta.abs() < best.abs())
                    {
                        nearest = Some((delta, position));
                    }
                }
            }
        }
        if let Some((delta, position)) = nearest {
            correction[axis] = delta;
            guides.push(SnapGuide { axis, position });
        }
    }
    let desired = raw + source_delta(correction, document);
    let delta = desired;
    let actual = mapped_bounds(source_bounds.translate(delta), document);
    // Only show guides that the mapped movement actually reaches.
    guides.retain(|g| {
        anchors(actual, g.axis)
            .iter()
            .any(|v| (*v - g.position).abs() < 0.01)
    });
    (delta, guides)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{document, export, markup_tests::object};
    use std::sync::Arc;

    fn fixture() -> Document {
        Document {
            image: Arc::new(image::RgbaImage::from_pixel(
                800,
                600,
                image::Rgba([246, 248, 251, 255]),
            )),
            geometry: Default::default(),
            annotations: vec![
                object(AnnotationKind::Rectangle, [60.0, 90.0], [160.0, 160.0]),
                object(AnnotationKind::Rectangle, [260.0, 180.0], [400.0, 280.0]),
                object(AnnotationKind::Rectangle, [610.0, 350.0], [730.0, 440.0]),
            ],
        }
    }

    fn bounds(d: &Document, i: usize) -> Rect {
        mapped_bounds(render::annotation_bounds(&d.annotations[i]), d)
    }

    fn close(a: f32, b: f32) {
        assert!((a - b).abs() < 0.002, "{a} != {b}");
    }

    #[test]
    fn all_six_alignments_follow_display_axes_after_crop_rotate_and_stretch() {
        for turns in 0..4 {
            for axis in 0..2 {
                for anchor in 0..3 {
                    let mut d = fixture();
                    d.geometry.crop = Some(CropRect {
                        x: 20,
                        y: 30,
                        width: 740,
                        height: 500,
                    });
                    d.geometry.quarter_turns = turns;
                    d.geometry.resize = Some([500, 320]);
                    let original = d.edit_state();
                    let total = mapped_bounds(selection_bounds(&d, &[0, 1, 2]), &d);
                    let target = anchors(total, axis)[anchor];
                    let mut e = EditorState::default();
                    e.set_selection(vec![0, 1, 2], &d);
                    let mut history = EditHistory::new(original.clone());
                    e.apply_layout(LayoutAction::Align { axis, anchor }, &mut d, &mut history);
                    for i in 0..3 {
                        close(anchors(bounds(&d, i), axis)[anchor], target);
                        assert_eq!(d.annotations[i].color, original.annotations[i].color);
                        close(
                            d.annotations[i].end.x - d.annotations[i].start.x,
                            original.annotations[i].end.x - original.annotations[i].start.x,
                        );
                    }
                    assert!(history.undo(&mut d));
                    assert_eq!(d.edit_state(), original);
                    assert!(!history.can_undo());
                    assert!(history.redo(&mut d));
                }
            }
        }
    }

    #[test]
    fn distribution_uses_equal_gaps_for_different_sizes_and_fixed_endpoints() {
        for axis in 0..2 {
            let mut d = fixture();
            let before = d.edit_state();
            let mut e = EditorState::default();
            e.set_selection(vec![0, 1, 2], &d);
            let mut h = EditHistory::new(before.clone());
            e.apply_layout(LayoutAction::Distribute { axis }, &mut d, &mut h);
            close(
                bounds(&d, 1).min[axis] - bounds(&d, 0).max[axis],
                bounds(&d, 2).min[axis] - bounds(&d, 1).max[axis],
            );
            assert_eq!(d.annotations[0], before.annotations[0]);
            assert_eq!(d.annotations[2], before.annotations[2]);
            assert!(h.undo(&mut d));
            assert_eq!(d.edit_state(), before);
        }
    }

    #[test]
    fn complete_groups_are_units_and_partial_members_remain_editable() {
        let mut d = fixture();
        d.annotations[0].group_id = Some(9);
        let mut text = object(AnnotationKind::Text, [80.0, 115.0], [80.0, 115.0]);
        text.text = "One".into();
        text.font_size = 20.0;
        text.group_id = Some(9);
        d.annotations.push(text);
        assert_eq!(
            layout_units(&d, &[0, 1, 2, 3]),
            vec![vec![0, 3], vec![1], vec![2]]
        );
        assert_eq!(layout_units(&d, &[1, 3]), vec![vec![1], vec![3]]);
        let before = d.edit_state();
        let mut e = EditorState::default();
        let mut h = EditHistory::new(before.clone());
        e.set_selection(vec![0, 1, 2, 3], &d);
        e.apply_layout(LayoutAction::Align { axis: 1, anchor: 2 }, &mut d, &mut h);
        close(
            d.annotations[0].start.y - before.annotations[0].start.y,
            d.annotations[3].start.y - before.annotations[3].start.y,
        );
        assert_eq!(d.annotations[3].group_id, Some(9));
        assert_eq!(d.annotations[3].text, "One");
        e.set_selection(vec![0, 3], &d);
        let state = d.edit_state();
        e.apply_layout(LayoutAction::Align { axis: 0, anchor: 0 }, &mut d, &mut h);
        assert_eq!(d.edit_state(), state);
    }

    #[test]
    fn snapped_movement_uses_nearest_edge_and_zoom_scaled_tolerance() {
        let d = fixture();
        let raw = Vec2::new(94.0, 40.0);
        let (delta, guides) = snapped_delta(&d, &[0], raw, 1.0, true);
        let moved = mapped_bounds(selection_bounds(&d, &[0]).translate(delta), &d);
        close(moved.max.x, bounds(&d, 1).min.x);
        assert!(guides.iter().any(|g| g.axis == 0));
        let (disabled, guides) = snapped_delta(&d, &[0], raw, 1.0, false);
        assert_eq!(disabled, raw);
        assert!(guides.is_empty());
        let (zoomed, _) = snapped_delta(&d, &[0], raw, 4.0, true);
        assert_eq!(zoomed.x, raw.x);
    }

    #[test]
    fn canvas_centers_snap_without_self_targets_and_hidden_objects_are_ignored() {
        let mut d = fixture();
        d.annotations.truncate(1);
        let b = bounds(&d, 0);
        let raw = Vec2::new(400.0 - b.center().x + 3.0, 300.0 - b.center().y - 4.0);
        let (delta, guides) = snapped_delta(&d, &[0], raw, 1.0, true);
        let moved = b.translate(delta);
        close(moved.center().x, 400.0);
        close(moved.center().y, 300.0);
        assert_eq!(guides.len(), 2);
        let (delta, guides) = snapped_delta(&d, &[0], Vec2::new(3.0, 4.0), 1.0, true);
        assert_eq!(delta, Vec2::new(3.0, 4.0));
        assert!(guides.is_empty());
        d.annotations.push(object(
            AnnotationKind::Rectangle,
            [65.0, 800.0],
            [165.0, 870.0],
        ));
        let (_, guides) = snapped_delta(&d, &[0], Vec2::new(3.0, 4.0), 1.0, true);
        assert!(guides.is_empty());
    }

    #[test]
    fn snapping_handles_rotation_nonuniform_resize_and_collective_movement() {
        let mut d = fixture();
        d.annotations.truncate(2);
        d.geometry.quarter_turns = 1;
        d.geometry.resize = Some([450, 1000]);
        let b = mapped_bounds(selection_bounds(&d, &[0, 1]), &d);
        let raw = source_delta(
            Vec2::new(225.0 - b.center().x + 2.0, 500.0 - b.center().y + 3.0),
            &d,
        );
        let (delta, guides) = snapped_delta(&d, &[0, 1], raw, 1.0, true);
        let moved = mapped_bounds(selection_bounds(&d, &[0, 1]).translate(delta), &d);
        close(moved.center().x, 225.0);
        close(moved.center().y, 500.0);
        assert_eq!(guides.len(), 2);
        let before = d.edit_state();
        move_selection(&mut d, &before.annotations, &[0, 1], delta);
        close(
            d.annotations[0].start.x - before.annotations[0].start.x,
            d.annotations[1].start.x - before.annotations[1].start.x,
        );
    }

    #[test]
    fn snapped_gesture_is_one_undo_and_click_is_a_noop() {
        let mut d = fixture();
        let before = d.edit_state();
        let mut e = EditorState::default();
        let mut h = EditHistory::new(before.clone());
        let origin = Point { x: 100.0, y: 120.0 };
        e.begin_selection_gesture(origin, 1.0, &mut d, false, false);
        e.update_snapped_gesture(origin, &mut d, false);
        assert_eq!(d.edit_state(), before);
        e.update_snapped_gesture(Point { x: 197.0, y: 160.0 }, &mut d, false);
        assert!(!e.snap_guides.is_empty());
        e.update_snapped_gesture(Point { x: 197.0, y: 160.0 }, &mut d, true);
        assert!(e.snap_guides.is_empty());
        e.finish_annotation_gesture(&mut d, &mut h);
        assert!(h.undo(&mut d));
        assert!(!h.can_undo());
        assert_eq!(d.edit_state(), before);
    }

    #[test]
    fn arranged_groups_save_reopen_and_export_without_flattening_project() {
        let dir = tempfile::tempdir().unwrap();
        let mut d = fixture();
        let original_image = d.image.clone();
        let mut e = EditorState::default();
        let mut h = EditHistory::new(d.edit_state());
        e.set_selection(vec![0, 1, 2], &d);
        e.apply_layout(LayoutAction::Align { axis: 1, anchor: 1 }, &mut d, &mut h);
        e.apply_layout(LayoutAction::Distribute { axis: 0 }, &mut d, &mut h);
        e.group_selection(&mut d, &mut h);
        let path = dir.path().join("layout.myimage");
        document::save_project(&d, &path).unwrap();
        let state = d.edit_state();
        for format in export::ExportFormat::ALL {
            let output = dir.path().join(format!("layout.{}", format.extension()));
            export::save(
                &d,
                &output,
                export::ExportOptions {
                    format,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        assert_eq!(d.edit_state(), state);
        let mut reopened = document::load_project(&path).unwrap();
        assert_eq!(reopened.edit_state(), state);
        assert_eq!(reopened.image.as_ref(), original_image.as_ref());
        reopened.annotations[1].color = [220, 30, 70, 255];
        document::save_project(&reopened, &path).unwrap();
        assert_eq!(
            document::load_project(&path).unwrap().annotations[1].color,
            [220, 30, 70, 255]
        );
    }

    #[test]
    fn outside_canvas_snaps_only_report_reached_guides() {
        let mut d = fixture();
        d.annotations[0].start.x = -10.0;
        let (delta, guides) = snapped_delta(&d, &[0], Vec2::new(-100.0, -100.0), 0.5, true);
        let moved = mapped_bounds(selection_bounds(&d, &[0]).translate(delta), &d);
        assert!(delta.x < -80.0, "canvas edges must not constrain movement");
        for guide in guides {
            assert!(
                anchors(moved, guide.axis)
                    .iter()
                    .any(|v| (v - guide.position).abs() < 0.01)
            );
        }
    }

    #[test]
    fn layout_translates_freehand_samples_and_callout_tail_without_changing_style() {
        let mut d = fixture();
        let mut pen = object(AnnotationKind::Freehand, [100.0, 130.0], [145.0, 155.0]);
        pen.points = vec![pen.start, Point { x: 120.0, y: 145.0 }, pen.end];
        d.annotations[0] = pen;
        d.annotations[1] = object(AnnotationKind::Callout, [260.0, 180.0], [400.0, 280.0]);
        let before = d.edit_state();
        let mut e = EditorState::default();
        e.set_selection(vec![0, 1, 2], &d);
        e.apply_layout(
            LayoutAction::Align { axis: 1, anchor: 2 },
            &mut d,
            &mut EditHistory::new(before.clone()),
        );
        for (i, a) in d.annotations.iter().enumerate() {
            let delta = point(a.start) - point(before.annotations[i].start);
            for (p, old) in a.points.iter().zip(&before.annotations[i].points) {
                close(p.y - old.y, delta.y);
            }
            if let Some(tail) = a.tail {
                close(tail.y - before.annotations[i].tail.unwrap().y, delta.y);
            }
            assert_eq!(a.stroke_width, before.annotations[i].stroke_width);
        }
    }

    fn ui_frame(
        e: &mut EditorState,
        d: &mut Document,
        h: &mut EditHistory,
        ctx: &egui::Context,
        texture: &egui::TextureHandle,
        input: egui::RawInput,
    ) -> egui::FullOutput {
        ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                e.ui(ui, d, h, texture, &mut Default::default());
            });
        })
    }

    fn click_label(
        label: &str,
        e: &mut EditorState,
        d: &mut Document,
        h: &mut EditHistory,
        ctx: &egui::Context,
        texture: &egui::TextureHandle,
    ) {
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 1000.0))),
            ..Default::default()
        };
        let frame = ui_frame(e, d, h, ctx, texture, input());
        let at = frame
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == label => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("Missing visible control: {label}"));
        for pressed in [true, false] {
            let mut input = input();
            input.events = vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ];
            ui_frame(e, d, h, ctx, texture, input);
        }
    }

    #[test]
    fn real_inspector_buttons_select_align_distribute_and_toggle_snapping() {
        let ctx = egui::Context::default();
        crate::appearance::apply(&ctx);
        let texture = ctx.load_texture(
            "layout UI",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut d = fixture();
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState::default();
        e.set_inspector_preferences(InspectorPreferences {
            expanded: true,
            pinned: true,
            ..Default::default()
        });
        e.select(Some(0), &d);
        click_label("Arrange ▾", &mut e, &mut d, &mut h, &ctx, &texture);
        click_label("All annotations", &mut e, &mut d, &mut h, &ctx, &texture);
        assert_eq!(e.selected_indices(&d), vec![0, 1, 2]);
        assert_eq!(d.edit_state(), before);
        click_label("Arrange ▾", &mut e, &mut d, &mut h, &ctx, &texture);
        click_label("Middle", &mut e, &mut d, &mut h, &ctx, &texture);
        for i in 0..3 {
            close(bounds(&d, i).center().y, 265.0);
        }
        click_label("Arrange ▾", &mut e, &mut d, &mut h, &ctx, &texture);
        click_label("Space horizontally", &mut e, &mut d, &mut h, &ctx, &texture);
        close(
            bounds(&d, 1).min.x - bounds(&d, 0).max.x,
            bounds(&d, 2).min.x - bounds(&d, 1).max.x,
        );
        let arranged = d.edit_state();
        click_label("Arrange ▾", &mut e, &mut d, &mut h, &ctx, &texture);
        click_label("Snap to guides", &mut e, &mut d, &mut h, &ctx, &texture);
        assert!(!e.snapping);
        assert_eq!(d.edit_state(), arranged);
        assert!(h.undo(&mut d));
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), before);
    }

    #[test]
    fn canvas_pointer_events_snap_and_paint_guides_until_release() {
        let ctx = egui::Context::default();
        let texture = ctx.load_texture(
            "snap UI",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut d = fixture();
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState {
            fit: false,
            ..Default::default()
        };
        let mut frame = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(816.0, 616.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        e.canvas(ui, &mut d, &mut h, &texture, &mut Default::default());
                    });
                },
            )
        };
        frame(vec![]);
        let start = Pos2::new(118.0, 133.0);
        frame(vec![
            egui::Event::PointerMoved(start),
            egui::Event::PointerButton {
                pos: start,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ]);
        let end = start + Vec2::new(94.0, 40.0);
        let output = frame(vec![egui::Event::PointerMoved(end)]);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::LineSegment { stroke, .. } if stroke.color == Color32::from_rgb(226,62,160))));
        let output = frame(vec![egui::Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
        assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::LineSegment { stroke, .. } if stroke.color == Color32::from_rgb(226,62,160))));
        close(bounds(&d, 0).max.x, bounds(&d, 1).min.x);
        assert!(e.snap_guides.is_empty());
        assert!(h.undo(&mut d));
        assert!(!h.can_undo());
        assert_eq!(d.edit_state(), before);
    }

    #[test]
    #[ignore = "manual native layout review fixture"]
    fn layout_review_fixture() {
        let dir = std::path::PathBuf::from(
            std::env::var("MYIMAGE_LAYOUT_REVIEW_DIR").expect("fixture output folder"),
        );
        std::fs::create_dir_all(&dir).unwrap();
        let mut d = fixture();
        for (i, a) in d.annotations.iter_mut().enumerate() {
            a.color = [[0, 125, 137, 255], [230, 66, 55, 255], [62, 100, 220, 255]][i];
        }
        document::save_project(&d, &dir.join("layout-review.myimage")).unwrap();
        render::flatten(&d)
            .unwrap()
            .save(dir.join("layout-before.png"))
            .unwrap();
    }
}
