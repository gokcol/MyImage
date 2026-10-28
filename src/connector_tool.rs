use super::*;
#[derive(Default)]
pub(super) struct State {
    source: Option<(usize, EditState)>,
    route: u8,
    message: Option<String>,
}
impl EditorState {
    pub(super) fn start_connector(&mut self, document: &mut Document, history: &mut EditHistory) {
        self.finish_gesture(document, history);
        self.restore_appearance(AnnotationKind::Arrow);
        self.clear_selection();
        self.area = None;
        self.area_mask.clear();
        self.pixel_mask = None;
        self.connector.source = None;
        self.connector.message = None;
        self.tool = Tool::ConnectedArrow;
        self.inspector.show(InspectorPanel::Styles);
    }
    pub(super) fn connector_click(
        &mut self,
        position: Point,
        scale: f32,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let Some(index) = hit_document_annotation(document, position, scale).filter(|&i| {
            document.annotations[i].kind != AnnotationKind::Arrow
                && !document.annotations[i].kind.is_area_edit()
        }) else {
            self.connector.message =
                Some("Click a shape, text, or another annotation to connect.".into());
            return;
        };
        let Some((first, before)) = self.connector.source.take() else {
            self.connector.source = Some((index, document.edit_state()));
            self.connector.message = Some("Now click the destination object.".into());
            return;
        };
        if before != document.edit_state() || first == index {
            self.connector.source = Some((index, document.edit_state()));
            self.connector.message = Some("Choose a different destination object.".into());
            return;
        }
        let start = crate::connectors::ensure_id(document, first);
        let end = crate::connectors::ensure_id(document, index);
        let mut arrow = self.new_annotation(
            AnnotationKind::Arrow,
            from_pos(render::annotation_bounds(&document.annotations[first]).center()),
        );
        arrow.end = from_pos(render::annotation_bounds(&document.annotations[index]).center());
        arrow.arrow.connection = Some(crate::connectors::Connection {
            start: Some(start),
            end: Some(end),
        });
        arrow.arrow.elbow = self.connector.route == 2;
        if self.connector.route == 1 {
            let s = point(arrow.start);
            let e = point(arrow.end);
            let normal = (e - s).normalized().rot90() * 50.;
            arrow.arrow.curve = Some([
                from_pos(s + (e - s) / 3. + normal),
                from_pos(s + (e - s) * 2. / 3. + normal),
            ]);
        }
        let created = document.annotations.len();
        document.annotations.push(arrow);
        crate::connectors::sync(document);
        if let Err(e) = crate::canvas::extend_for_annotations(document, &[created])
            .and_then(|_| crate::document::validate_document(document))
        {
            document.annotations = before.annotations;
            document.geometry = before.geometry;
            self.connector.message = Some(e);
            return;
        }
        history.commit(before, document);
        self.tool = Tool::Area;
        self.select(Some(created), document);
    }
    pub(super) fn connector_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Connected Arrows");
        ui.small("Click a source object, then a destination. The arrow follows both objects when moved or resized.");
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.connector.route, 0, "Straight");
            ui.selectable_value(&mut self.connector.route, 1, "Curved");
            ui.selectable_value(&mut self.connector.route, 2, "Elbow");
        });
        object_properties::palette(ui, &mut self.color);
        object_properties::slider(ui, &mut self.stroke_width, 1.0..=24., "Width", " px");
        if let Some(message) = &self.connector.message {
            ui.label(message);
        }
        if ui.button("Done").clicked() {
            self.connector.source = None;
            self.tool = Tool::Area;
        }
    }
}
