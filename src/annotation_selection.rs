//! Multi-selection and flat groups keep annotation objects and their layer order intact.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

impl EditorState {
    pub(super) fn selected_indices(&self, document: &Document) -> Vec<usize> {
        let mut indices: Vec<_> = self
            .selected
            .into_iter()
            .chain(self.additional.iter().copied())
            .filter(|&i| i < document.annotations.len())
            .collect();
        indices.sort_unstable();
        indices.dedup();
        indices
    }

    pub(super) fn clear_selection(&mut self) {
        self.layer_target = None;
        self.selected = None;
        self.additional.clear();
    }

    pub(super) fn set_selection(&mut self, mut indices: Vec<usize>, document: &Document) {
        indices.retain(|&i| i < document.annotations.len());
        indices.sort_unstable();
        indices.dedup();
        let primary = indices.pop();
        self.select(primary, document);
        self.additional = indices;
    }

    pub(super) fn choose_annotation(
        &mut self,
        index: Option<usize>,
        toggle: bool,
        individual: bool,
        document: &Document,
    ) {
        let unit = index.map_or_else(Vec::new, |i| {
            if individual {
                vec![i]
            } else {
                expand_groups(&[i], document)
            }
        });
        if toggle {
            let mut indices = self.selected_indices(document);
            if unit.iter().all(|i| indices.contains(i)) {
                indices.retain(|i| !unit.contains(i));
            } else {
                indices.extend(unit);
            }
            self.set_selection(indices, document);
        } else {
            self.set_selection(unit, document);
        }
    }

    pub(super) fn selection_controls(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let indices = self.selected_indices(document);
        let grouped = indices
            .iter()
            .any(|&i| document.annotations[i].group_id.is_some());
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    indices.len() >= 2 && self.can_edit_selection(document),
                    egui::Button::new("Group"),
                )
                .on_hover_text("Group selected annotations (Command/Ctrl + G)")
                .clicked()
            {
                self.group_selection(document, history);
            }
            if ui
                .add_enabled(
                    grouped && self.can_edit_selection(document),
                    egui::Button::new("Ungroup"),
                )
                .on_hover_text("Ungroup (Command/Ctrl + Shift + G)")
                .clicked()
            {
                self.ungroup_selection(document, history);
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!indices.is_empty(), egui::Button::new("Duplicate"))
                .on_hover_text("Independent copy (Command/Ctrl + D)")
                .clicked()
            {
                self.duplicate_selection(document, history);
            }
            if ui
                .add_enabled(
                    !document.annotations.is_empty(),
                    egui::Button::new("All annotations"),
                )
                .clicked()
            {
                self.tool = Tool::Select;
                self.set_selection((0..document.annotations.len()).collect(), document);
                self.focus_canvas = true;
            }
        });
        self.layout_controls(ui, document, history);
        ui.add_space(8.0);
    }

    pub(super) fn group_selection(&mut self, document: &mut Document, history: &mut EditHistory) {
        let indices = expand_groups(&self.selected_indices(document), document);
        if indices.len() < 2 || !annotation_layers::editable(document, &indices) {
            return;
        }
        let existing = document.annotations[indices[0]].group_id;
        if existing.is_some()
            && indices
                .iter()
                .all(|&i| document.annotations[i].group_id == existing)
        {
            return;
        }
        let before = document.edit_state();
        let used = document
            .annotations
            .iter()
            .filter_map(|a| a.group_id)
            .collect();
        let id = fresh_group(&used);
        for &i in &indices {
            document.annotations[i].layer.hidden |= document.annotations[i].group_layer.hidden;
            document.annotations[i].group_layer = Default::default();
            document.annotations[i].group_id = Some(id);
        }
        history.commit(before, document);
        self.set_selection(indices, document);
        self.focus_canvas = true;
    }

    pub(super) fn ungroup_selection(&mut self, document: &mut Document, history: &mut EditHistory) {
        let indices = expand_groups(&self.selected_indices(document), document);
        if !annotation_layers::editable(document, &indices) {
            return;
        }
        let before = document.edit_state();
        for &i in &indices {
            document.annotations[i].layer.hidden |= document.annotations[i].group_layer.hidden;
            document.annotations[i].group_layer = Default::default();
            document.annotations[i].group_id = None;
        }
        history.commit(before, document);
        self.set_selection(indices, document);
        self.focus_canvas = true;
    }

    pub(super) fn duplicate_selection(
        &mut self,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let indices = self.selected_indices(document);
        if indices.is_empty() {
            return;
        }
        if document.annotations.len() + indices.len() > crate::document::MAX_ANNOTATIONS {
            self.image_error = Some("Duplicating would exceed the annotation limit.".into());
            return;
        }
        let mut candidate = document.clone();
        let mut used: BTreeSet<_> = document
            .annotations
            .iter()
            .filter_map(|a| a.group_id)
            .collect();
        let mut counts = BTreeMap::new();
        for &i in &indices {
            if let Some(id) = document.annotations[i].group_id {
                *counts.entry(id).or_insert(0) += 1;
            }
        }
        let mut copies = BTreeMap::new();
        for (id, count) in counts {
            if count > 1 {
                let new = fresh_group(&used);
                used.insert(new);
                copies.insert(id, new);
            }
        }
        let delta = clamped_selection_delta(
            document,
            &indices,
            source_delta(Vec2::splat(16.0), document),
        );
        let first = candidate.annotations.len();
        for &i in &indices {
            let mut copy = offset_annotation(&document.annotations[i], delta);
            copy.group_id = copy.group_id.and_then(|id| copies.get(&id).copied());
            if copy.group_id.is_none() {
                copy.layer.hidden |= copy.group_layer.hidden;
                copy.layer.locked |= copy.group_layer.locked;
                copy.group_layer = Default::default();
            }
            candidate.annotations.push(copy);
        }
        if let Err(error) = crate::document::validate_document(&candidate) {
            self.image_error = Some(format!("Cannot duplicate: {error}"));
            return;
        }
        // Leave room for document metadata inside the project's existing 4 MiB limit.
        if serde_json::to_vec(&candidate.annotations)
            .map_or(true, |json| json.len() > 4 * 1024 * 1024 - 1024)
        {
            self.image_error =
                Some("Duplicating would exceed the project annotation data limit.".into());
            return;
        }
        let before = document.edit_state();
        document.annotations = candidate.annotations;
        history.commit(before, document);
        self.set_selection((first..document.annotations.len()).collect(), document);
        self.tool = Tool::Select;
        self.focus_canvas = true;
        self.image_error = None;
    }

    pub(super) fn begin_selection_gesture(
        &mut self,
        at: Point,
        scale: f32,
        document: &mut Document,
        toggle: bool,
        individual: bool,
    ) {
        let indices = self.selected_indices(document);
        let hit = hit_document_annotation(document, at, scale);
        let before = document.edit_state();
        let kind = if !toggle
            && !individual
            && annotation_layers::editable(document, &indices)
            && indices.iter().all(|&i| document.annotations[i].visible())
            && indices.len() > 1
            && let Some(handle) = selection_handle(document, &indices, at, scale)
        {
            Some(GestureKind::ResizeMany(indices, handle))
        } else if toggle && hit.is_some() {
            self.choose_annotation(hit, true, individual, document);
            None
        } else if !toggle
            && annotation_layers::editable(document, &indices)
            && indices.iter().all(|&i| document.annotations[i].visible())
            && indices.len() == 1
            && self.selected.is_some()
            && let Some(handle) =
                hit_document_handle(&document.annotations[indices[0]], at, scale, document)
        {
            Some(GestureKind::Resize(indices[0], handle))
        } else if let Some(index) = hit {
            if individual || !indices.contains(&index) {
                self.choose_annotation(Some(index), false, individual, document);
            } else {
                // A normal click returns from member editing to the complete group.
                let mut selected = indices;
                selected.extend(expand_groups(&[index], document));
                self.set_selection(selected, document);
            }
            let selected = self.selected_indices(document);
            if !annotation_layers::editable(document, &selected) {
                None
            } else if selected.len() == 1 {
                Some(GestureKind::Move(selected[0]))
            } else {
                Some(GestureKind::MoveMany(selected))
            }
        } else {
            let initial = if toggle { indices } else { Vec::new() };
            self.set_selection(initial.clone(), document);
            Some(GestureKind::Marquee {
                current: at,
                initial,
            })
        };
        self.gesture = kind.map(|kind| Gesture {
            kind,
            before,
            origin: at,
        });
    }
}

fn fresh_group(used: &BTreeSet<u64>) -> u64 {
    (1..)
        .find(|id| !used.contains(id))
        .expect("bounded annotation count leaves an unused group ID")
}

pub(super) fn expand_groups(indices: &[usize], document: &Document) -> Vec<usize> {
    let ids: BTreeSet<_> = indices
        .iter()
        .filter_map(|&i| document.annotations.get(i).and_then(|a| a.group_id))
        .collect();
    let mut result = indices.to_vec();
    result.extend(
        document
            .annotations
            .iter()
            .enumerate()
            .filter_map(|(i, a)| a.group_id.filter(|id| ids.contains(id)).map(|_| i)),
    );
    result.sort_unstable();
    result.dedup();
    result
}

pub(super) fn selection_bounds(document: &Document, indices: &[usize]) -> Rect {
    indices
        .iter()
        .filter_map(|&i| document.annotations.get(i))
        .map(render::annotation_bounds)
        .fold(Rect::NOTHING, Rect::union)
}

pub(super) fn clamped_selection_delta(document: &Document, indices: &[usize], delta: Vec2) -> Vec2 {
    if indices
        .iter()
        .any(|&i| document.annotations[i].kind == AnnotationKind::Stamp)
    {
        return delta;
    }
    let bounds = selection_bounds(document, indices);
    let canvas = document.geometry.canvas.map_or(
        Rect::from_min_size(
            Pos2::ZERO,
            Vec2::new(
                document.image.width() as f32,
                document.image.height() as f32,
            ),
        ),
        |c| {
            Rect::from_min_size(
                egui::pos2(c.x as f32, c.y as f32),
                egui::vec2(c.width as f32, c.height as f32),
            )
        },
    );
    Vec2::new(
        delta.x.clamp(
            (canvas.min.x - bounds.min.x).min(0.0),
            (canvas.max.x - bounds.max.x).max(0.0),
        ),
        delta.y.clamp(
            (canvas.min.y - bounds.min.y).min(0.0),
            (canvas.max.y - bounds.max.y).max(0.0),
        ),
    )
}

pub(super) fn offset_annotation(original: &Annotation, delta: Vec2) -> Annotation {
    let mut a = original.clone();
    a.start = from_pos(point(a.start) + delta);
    a.end = from_pos(point(a.end) + delta);
    a.tail = a.tail.map(|p| from_pos(point(p) + delta));
    a.arrow.curve = a.arrow.curve.map(|c| c.map(|p| from_pos(point(p) + delta)));
    for p in &mut a.points {
        *p = from_pos(point(*p) + delta);
    }
    a
}

pub(super) fn move_selection(
    document: &mut Document,
    originals: &[Annotation],
    indices: &[usize],
    delta: Vec2,
) {
    for &i in indices {
        document.annotations[i] = offset_annotation(&originals[i], delta);
    }
}

pub(super) fn resize_selection(
    document: &mut Document,
    before: &EditState,
    indices: &[usize],
    handle: Handle,
    at: Point,
) -> Result<(), String> {
    let original = Document {
        image: document.image.clone(),
        annotations: before.annotations.clone(),
        geometry: before.geometry,
    };
    let bounds = selection_bounds(&original, indices);
    let (anchor, corner) = match handle {
        Handle::TopLeft => (bounds.right_bottom(), bounds.left_top()),
        Handle::TopRight => (bounds.left_bottom(), bounds.right_top()),
        Handle::BottomLeft => (bounds.right_top(), bounds.left_bottom()),
        _ => (bounds.left_top(), bounds.right_bottom()),
    };
    let vector = corner - anchor;
    let factor =
        ((point(at) - anchor).dot(vector) / vector.length_sq().max(0.01)).clamp(0.01, 100.0);
    let transform = |p: Point| from_pos(anchor + (point(p) - anchor) * factor);
    let mut candidate = document.clone();
    for &i in indices {
        let mut a = before.annotations[i].clone();
        a.start = transform(a.start);
        a.end = transform(a.end);
        a.tail = a.tail.map(transform);
        a.arrow.curve = a.arrow.curve.map(|c| c.map(transform));
        for p in &mut a.points {
            *p = transform(*p);
        }
        a.stroke_width = (a.stroke_width * factor).clamp(0.1, crate::document::MAX_STROKE_WIDTH);
        a.font_size = (a.font_size * factor).clamp(1.0, crate::document::MAX_FONT_SIZE);
        if a.kind.is_effect() {
            a.effect_size = (a.effect_size as f32 * factor)
                .round()
                .clamp(2.0, crate::document::MAX_EFFECT_SIZE as f32)
                as u32;
        }
        candidate.annotations[i] = a;
    }
    crate::document::validate_document(&candidate)?;
    document.annotations = candidate.annotations;
    Ok(())
}

pub(super) fn group_handles(bounds: Rect) -> [(Handle, Pos2); 4] {
    [
        (Handle::TopLeft, bounds.left_top()),
        (Handle::TopRight, bounds.right_top()),
        (Handle::BottomLeft, bounds.left_bottom()),
        (Handle::BottomRight, bounds.right_bottom()),
    ]
}

pub(super) fn selection_handle(
    document: &Document,
    indices: &[usize],
    at: Point,
    scale: f32,
) -> Option<Handle> {
    let at = output_point(at, document);
    group_handles(selection_bounds(document, indices))
        .into_iter()
        .find(|(_, p)| output_point(from_pos(*p), document).distance(at) * scale <= 8.0)
        .map(|(h, _)| h)
}

pub(super) fn paint_many(
    painter: &egui::Painter,
    document: &Document,
    indices: &[usize],
    origin: Pos2,
    scale: f32,
    accent: Color32,
) {
    let bounds = selection_bounds(document, indices);
    let screen = mapped_bounds(bounds, document);
    let screen = Rect::from_min_max(
        origin + screen.min.to_vec2() * scale,
        origin + screen.max.to_vec2() * scale,
    );
    painter.rect_stroke(
        screen.expand(2.0),
        1,
        Stroke::new(1.5, accent),
        StrokeKind::Outside,
    );
    for (_, p) in group_handles(bounds) {
        let center = origin + output_point(from_pos(p), document).to_vec2() * scale;
        paint_object_handle(painter, center, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markup_tests::object;
    use std::sync::Arc;

    fn document() -> Document {
        Document {
            image: Arc::new(image::RgbaImage::from_pixel(
                400,
                300,
                image::Rgba([255; 4]),
            )),
            geometry: Default::default(),
            annotations: vec![
                object(AnnotationKind::Rectangle, [30.0, 30.0], [80.0, 80.0]),
                object(AnnotationKind::Highlighter, [100.0, 50.0], [160.0, 100.0]),
                object(AnnotationKind::Callout, [180.0, 80.0], [260.0, 130.0]),
            ],
        }
    }

    #[test]
    fn shift_toggles_groups_and_alt_selects_an_editable_member() {
        let mut d = document();
        d.annotations[0].group_id = Some(8);
        d.annotations[2].group_id = Some(8);
        let mut e = EditorState::default();
        e.choose_annotation(Some(0), false, false, &d);
        assert_eq!(e.selected_indices(&d), vec![0, 2]);
        e.choose_annotation(Some(1), true, false, &d);
        assert_eq!(e.selected_indices(&d), vec![0, 1, 2]);
        e.choose_annotation(Some(2), true, false, &d);
        assert_eq!(e.selected_indices(&d), vec![1]);
        e.choose_annotation(Some(2), false, true, &d);
        assert_eq!(e.selected_indices(&d), vec![2]);
        let mut h = EditHistory::new(d.edit_state());
        e.text = "Changed inside group".into();
        e.apply_inspector(&mut d, &mut h);
        assert_eq!(d.annotations[2].text, "Changed inside group");
        assert_eq!(d.annotations[0].group_id, Some(8));
        assert_eq!(d.annotations[2].group_id, Some(8));
        e.begin_selection_gesture(Point { x: 220.0, y: 105.0 }, 1.0, &mut d, false, false);
        assert_eq!(e.selected_indices(&d), vec![0, 2]);
        assert!(matches!(
            e.gesture.as_ref().unwrap().kind,
            GestureKind::MoveMany(_)
        ));
    }

    #[test]
    fn grouping_ungrouping_and_merging_preserve_layer_order_pixels_and_history() {
        let mut d = document();
        d.annotations[1].kind = AnnotationKind::Blur;
        let before = d.edit_state();
        let pixels = render::flatten(&d).unwrap();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState::default();
        e.set_selection(vec![0, 2], &d);
        e.group_selection(&mut d, &mut h);
        let group = d.annotations[0].group_id;
        assert!(group.is_some());
        assert_eq!(d.annotations[2].group_id, group);
        assert_eq!(d.annotations[1].group_id, None);
        assert_eq!(render::flatten(&d).unwrap(), pixels);
        e.set_selection(vec![1, 2], &d);
        e.group_selection(&mut d, &mut h);
        assert!(
            d.annotations
                .iter()
                .all(|a| a.group_id == d.annotations[0].group_id)
        );
        assert_eq!(e.selected_indices(&d), vec![0, 1, 2]);
        e.set_selection(vec![2], &d);
        e.ungroup_selection(&mut d, &mut h);
        assert_eq!(d.edit_state(), before);
        assert!(h.undo(&mut d));
        assert!(d.annotations.iter().all(|a| a.group_id.is_some()));
        assert!(h.undo(&mut d));
        assert_eq!(d.annotations[1].group_id, None);
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), before);
    }

    #[test]
    fn duplicates_preserve_relative_order_and_get_independent_group_ids() {
        let mut d = document();
        d.annotations[0].group_id = Some(u64::MAX);
        d.annotations[2].group_id = Some(u64::MAX);
        let before = d.edit_state();
        let pixels = d.image.clone();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState::default();
        e.set_selection(vec![0, 1, 2], &d);
        e.duplicate_selection(&mut d, &mut h);
        assert_eq!(d.annotations.len(), 6);
        assert_eq!(&d.annotations[..3], before.annotations.as_slice());
        assert_eq!(e.selected_indices(&d), vec![3, 4, 5]);
        assert_ne!(d.annotations[3].group_id, d.annotations[0].group_id);
        assert_eq!(d.annotations[3].group_id, d.annotations[5].group_id);
        assert_eq!(d.annotations[4].group_id, None);
        assert_eq!(
            d.annotations[5].tail.unwrap().x - d.annotations[2].tail.unwrap().x,
            16.0
        );
        assert!(Arc::ptr_eq(&pixels, &d.image));
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), before);
        assert!(!h.can_undo());
        assert!(h.redo(&mut d));
        assert_eq!(d.annotations.len(), 6);
        e.set_selection(vec![0], &d);
        e.duplicate_selection(&mut d, &mut h);
        assert_eq!(d.annotations.last().unwrap().group_id, None);
    }

    #[test]
    fn duplicate_budget_failure_does_not_change_document_or_history() {
        let mut d = document();
        let mut pen = object(AnnotationKind::Freehand, [20.0, 20.0], [20.0, 20.0]);
        pen.points = vec![Point { x: 20.0, y: 20.0 }; crate::document::MAX_STROKE_POINTS];
        d.annotations = vec![pen; 4];
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState::default();
        e.set_selection(vec![0], &d);
        e.duplicate_selection(&mut d, &mut h);
        assert!(e.image_error.is_some());
        assert!(!h.can_undo());
        assert_eq!(d.edit_state(), before);
    }

    #[test]
    fn group_move_clamps_once_and_keeps_relative_geometry_and_pointer() {
        let mut d = document();
        let indices = vec![0, 2];
        let before = d.edit_state();
        let delta = clamped_selection_delta(&d, &indices, Vec2::new(500.0, 500.0));
        move_selection(&mut d, &before.annotations, &indices, delta);
        for i in indices {
            assert_eq!(
                point(d.annotations[i].start) - point(before.annotations[i].start),
                delta
            );
        }
        assert_eq!(d.annotations[1], before.annotations[1]);
        assert_eq!(
            point(d.annotations[2].tail.unwrap()) - point(before.annotations[2].tail.unwrap()),
            delta
        );
        assert!(selection_bounds(&d, &[0, 2]).max.x <= 400.01);
        assert!(selection_bounds(&d, &[0, 2]).max.y <= 300.01);
    }

    #[test]
    fn proportional_group_resize_preserves_text_steps_pen_and_callout_geometry() {
        let mut d = document();
        let mut pen = object(AnnotationKind::Freehand, [40.0, 140.0], [90.0, 180.0]);
        pen.points = vec![pen.start, pen.end];
        d.annotations.push(pen);
        let mut step = object(AnnotationKind::Step, [100.0, 150.0], [130.0, 180.0]);
        step.step_number = 7;
        d.annotations.push(step);
        let mut text = object(AnnotationKind::Text, [150.0, 200.0], [150.0, 200.0]);
        text.text = "Label".into();
        d.annotations.push(text);
        let mut arrow = object(AnnotationKind::Arrow, [40., 50.], [100., 50.]);
        arrow.arrow.curve = Some([Point { x: 60., y: 20. }, Point { x: 80., y: 20. }]);
        d.annotations.push(arrow);
        let indices: Vec<_> = (0..d.annotations.len()).collect();
        let before = d.edit_state();
        let bounds = selection_bounds(&d, &indices);
        let target = from_pos(bounds.min + bounds.size() * 1.5);
        resize_selection(&mut d, &before, &indices, Handle::BottomRight, target).unwrap();
        assert_eq!(
            d.annotations[6].arrow.curve,
            before.annotations[6].arrow.curve.map(|controls| {
                controls.map(|p| from_pos(bounds.min + (point(p) - bounds.min) * 1.5))
            })
        );
        assert_eq!(d.annotations[5].font_size, 42.0);
        assert_eq!(d.annotations[4].step_number, 7);
        assert!((d.annotations[4].end.x - d.annotations[4].start.x - 45.0).abs() < 0.01);
        assert_eq!(
            point(d.annotations[3].points[1]) - point(d.annotations[3].points[0]),
            Vec2::new(75.0, 60.0)
        );
        assert_eq!(
            d.annotations[2].tail,
            Some(from_pos(
                bounds.min + (point(before.annotations[2].tail.unwrap()) - bounds.min) * 1.5
            ))
        );
    }

    #[test]
    fn delete_and_reorder_act_on_all_selected_members_and_undo_together() {
        let mut d = document();
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState::default();
        e.set_selection(vec![0, 2], &d);
        e.reorder(&mut d, &mut h, true);
        assert_eq!(e.selected_indices(&d), vec![1, 2]);
        assert_eq!(d.annotations[0], before.annotations[1]);
        assert_eq!(d.annotations[1], before.annotations[0]);
        e.delete(&mut d, &mut h);
        assert_eq!(d.annotations.len(), 1);
        assert!(e.selected_indices(&d).is_empty());
        assert!(h.undo(&mut d));
        assert_eq!(d.annotations.len(), 3);
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), before);
    }

    #[test]
    fn marquee_selects_groups_and_is_not_an_edit() {
        let mut d = document();
        d.annotations[0].group_id = Some(1);
        d.annotations[2].group_id = Some(1);
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState::default();
        e.begin_selection_gesture(Point { x: 10.0, y: 10.0 }, 1.0, &mut d, false, false);
        e.update_gesture(Point { x: 90.0, y: 90.0 }, &mut d);
        e.finish_gesture(&mut d, &mut h);
        assert_eq!(e.selected_indices(&d), vec![0, 2]);
        assert_eq!(d.edit_state(), before);
        assert!(!h.can_undo());
    }

    #[test]
    fn group_gesture_has_single_undo_and_transformed_handles_hit() {
        let mut d = document();
        d.geometry.quarter_turns = 1;
        d.geometry.resize = Some([150, 800]);
        d.annotations[0].group_id = Some(1);
        d.annotations[1].group_id = Some(1);
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState::default();
        e.begin_selection_gesture(Point { x: 50.0, y: 50.0 }, 1.0, &mut d, false, false);
        e.update_gesture(Point { x: 60.0, y: 65.0 }, &mut d);
        e.update_gesture(Point { x: 70.0, y: 70.0 }, &mut d);
        e.finish_gesture(&mut d, &mut h);
        assert_eq!(e.selected_indices(&d), vec![0, 1]);
        assert_eq!(d.annotations[0].start, Point { x: 50.0, y: 50.0 });
        assert!(h.undo(&mut d));
        assert!(!h.can_undo());
        assert_eq!(d.edit_state(), before);
        let bounds = selection_bounds(&d, &[0, 1]);
        assert!(matches!(
            selection_handle(&d, &[0, 1], from_pos(bounds.right_bottom()), 0.5),
            Some(Handle::BottomRight)
        ));
    }
}
