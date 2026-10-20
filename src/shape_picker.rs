//! Shape choices share the same editable annotation properties and history.
use super::*;
use crate::{appearance, shapes};

impl EditorState {
    fn picker_kind(&self, document: &Document) -> Option<AnnotationKind> {
        if self.selected_indices(document).len() > 1 {
            return None;
        }
        self.selected
            .and_then(|i| document.annotations.get(i))
            .map(|a| a.kind)
            .or(self.tool.kind())
            .filter(|kind| kind.is_shape())
    }

    fn choose_shape(
        &mut self,
        kind: AnnotationKind,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if !kind.is_shape()
            || self.picker_kind(document).is_none()
            || (self.selected.is_some() && !self.can_edit_selection(document))
        {
            return;
        }
        self.finish_gesture(document, history);
        self.shape_kind = kind;
        self.shapes_3d = kind.is_3d();
        if let Some(index) = self.selected {
            let before = document.edit_state();
            document.annotations[index].kind = kind;
            history.commit(before, document);
            self.select(Some(index), document);
            if !matches!(self.tool, Tool::Select | Tool::Area) {
                self.tool = shape_tool(kind);
            }
        } else {
            self.activate_tool(shape_tool(kind));
        }
    }

    pub(super) fn shape_picker(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let Some(current) = self.picker_kind(document) else {
            return;
        };
        let previous_tab = self.shapes_3d;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.shapes_3d, false, "2D");
            ui.selectable_value(&mut self.shapes_3d, true, "3D");
        });
        if previous_tab != self.shapes_3d && self.selected.is_none() {
            self.choose_shape(
                if self.shapes_3d {
                    shapes::THREE_D[0].0
                } else {
                    shapes::CHOICES[0].0
                },
                document,
                history,
            );
        }
        let current = self.picker_kind(document).unwrap_or(current);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Shapes").size(16.).strong());
            ui.label(
                egui::RichText::new(shapes::name(current))
                    .size(11.)
                    .color(appearance::palette(ui.ctx()).muted),
            );
        });
        let enabled = self.selected.is_none() || self.can_edit_selection(document);
        ui.add_enabled_ui(enabled, |ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(4.);
            let width = (ui.available_width() - 16.) / 5.;
            let choices = if self.shapes_3d {
                shapes::THREE_D.as_slice()
            } else {
                shapes::CHOICES.as_slice()
            };
            for row in choices.chunks(5) {
                ui.horizontal(|ui| {
                    for &(kind, name) in row {
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::new(width, 32.), Sense::click());
                        let selected = kind == current;
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::SelectableLabel,
                                ui.is_enabled(),
                                selected,
                                format!("Shape: {name}"),
                            )
                        });
                        let fill = if selected {
                            Color32::from_rgb(218, 243, 239)
                        } else if response.hovered() {
                            Color32::from_rgb(237, 242, 247)
                        } else {
                            Color32::from_rgb(246, 248, 250)
                        };
                        ui.painter().rect_filled(rect, 5, fill);
                        let color = if selected {
                            appearance::palette(ui.ctx()).accent
                        } else {
                            appearance::palette(ui.ctx()).ink
                        };
                        let bounds = Rect::from_center_size(rect.center(), Vec2::new(24., 20.));
                        ui.painter().add(egui::Shape::closed_line(
                            shapes::outline(kind, bounds),
                            Stroke::new(1.5, color),
                        ));
                        for path in shapes::details(kind, bounds) {
                            ui.painter()
                                .add(egui::Shape::line(path, Stroke::new(1., color)));
                        }
                        if response.has_focus() {
                            ui.painter().rect_stroke(
                                rect,
                                5,
                                Stroke::new(1., appearance::palette(ui.ctx()).accent),
                                StrokeKind::Inside,
                            );
                        }
                        if response.on_hover_text(name).clicked() {
                            self.choose_shape(kind, document, history);
                        }
                    }
                });
            }
        });
        ui.add_space(3.);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn shapes_draw_convert_undo_and_respect_selection_locks() {
        for (kind, _) in shapes::CHOICES.into_iter().chain(shapes::THREE_D) {
            let mut doc = Document {
                image: Arc::new(image::RgbaImage::new(300, 200)),
                annotations: vec![],
                geometry: Default::default(),
            };
            let mut history = EditHistory::new(doc.edit_state());
            let mut editor = EditorState::default();
            editor.activate_tool(shape_tool(AnnotationKind::Rectangle));
            editor.choose_shape(kind, &mut doc, &mut history);
            assert_eq!(editor.shape_kind, kind);
            editor.begin_gesture(Point { x: 30., y: 30. }, 1., &mut doc);
            editor.update_gesture(Point { x: 130., y: 130. }, &mut doc);
            editor.finish_gesture(&mut doc, &mut history);
            assert_eq!(doc.annotations[0].kind, kind);
            editor.tool = Tool::Select;
            editor.select(Some(0), &doc);
            let before = doc.edit_state();
            let replacement = if kind == AnnotationKind::Star {
                AnnotationKind::Diamond
            } else {
                AnnotationKind::Star
            };
            editor.choose_shape(replacement, &mut doc, &mut history);
            let mut expected = before.annotations[0].clone();
            expected.kind = replacement;
            assert_eq!(doc.annotations[0], expected);
            assert!(history.undo(&mut doc));
            assert_eq!(doc.edit_state(), before);
            doc.annotations[0].layer.locked = true;
            let locked = doc.edit_state();
            editor.choose_shape(replacement, &mut doc, &mut history);
            assert_eq!(doc.edit_state(), locked);
        }
    }

    #[test]
    fn polygon_hits_follow_concavity_and_nonuniform_image_transforms() {
        let mut doc = Document {
            image: Arc::new(image::RgbaImage::new(300, 200)),
            annotations: vec![],
            geometry: Default::default(),
        };
        let editor = EditorState::default();
        let mut a = editor.new_annotation(AnnotationKind::Cross, Point { x: 30., y: 30. });
        a.end = Point { x: 130., y: 130. };
        a.fill = true;
        doc.annotations.push(a);
        for turns in 0..4 {
            doc.geometry.quarter_turns = turns;
            doc.geometry.resize = Some([900, 100]);
            assert_eq!(
                hit_document_annotation(&doc, Point { x: 80., y: 80. }, 2.),
                Some(0)
            );
            assert_eq!(
                hit_document_annotation(&doc, Point { x: 35., y: 35. }, 2.),
                None
            );
        }
        doc.annotations[0].fill = false;
        assert_eq!(
            hit_document_annotation(&doc, Point { x: 80., y: 80. }, 2.),
            None
        );
        assert_eq!(
            hit_document_annotation(&doc, Point { x: 80., y: 30. }, 2.),
            Some(0)
        );
    }
}
