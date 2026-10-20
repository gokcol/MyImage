use super::*;
use std::sync::mpsc;

// Rebase a lasso when rounding to source pixels or clipping its selection to a crop.
// Clip polygon edges rather than clamping vertices, which changes concave regions.
fn remap_mask(mask: &[Point], from: Rect, to: Rect) -> Result<Vec<Point>, String> {
    if mask.is_empty() {
        return Ok(Vec::new());
    }
    let mut polygon: Vec<Pos2> = mask
        .iter()
        .map(|p| {
            let absolute = from.min + egui::vec2(p.x, p.y) * from.size();
            let relative = (absolute - to.min) / to.size();
            egui::pos2(relative.x, relative.y)
        })
        .collect();
    for edge in 0..4 {
        let distance = |p: Pos2| match edge {
            0 => p.x,
            1 => 1. - p.x,
            2 => p.y,
            _ => 1. - p.y,
        };
        let input = std::mem::take(&mut polygon);
        let Some(&last) = input.last() else {
            break;
        };
        let mut previous = last;
        for current in input {
            let a = distance(previous);
            let b = distance(current);
            if (a >= 0.) != (b >= 0.) {
                polygon.push(previous + (current - previous) * (a / (a - b)));
            }
            if b >= 0. {
                polygon.push(current);
            }
            previous = current;
        }
    }
    if polygon.len() < 3 {
        return Err("The lasso does not overlap the visible image.".into());
    }
    Ok(polygon
        .into_iter()
        .map(|p| Point {
            x: p.x.clamp(0., 1.),
            y: p.y.clamp(0., 1.),
        })
        .collect())
}

pub(super) struct FillState {
    color: [u8; 4],
    tolerance: u8,
    global: bool,
    pending: Option<Pending>,
    error: Option<String>,
}
struct Pending {
    before: EditState,
    index: Option<usize>,
    receiver: mpsc::Receiver<Result<Annotation, String>>,
}
impl Default for FillState {
    fn default() -> Self {
        Self {
            color: [46, 137, 221, 255],
            tolerance: 15,
            global: false,
            pending: None,
            error: None,
        }
    }
}
impl FillState {
    pub(super) fn busy(&self) -> bool {
        self.pending.is_some()
    }
}
impl EditorState {
    pub(super) fn activate_fill(&mut self, document: &mut Document, history: &mut EditHistory) {
        self.finish_gesture(document, history);
        // Preserve a rectangle/lasso when switching from Select to Fill.
        if self.area.is_none()
            && self.selected.is_some()
            && self.style_kind(document) != Some(AnnotationKind::Fill)
        {
            self.area = self.selection_region(document);
            self.area_mask = self
                .selected
                .and_then(|i| document.annotations.get(i))
                .filter(|a| a.kind.needs_composite())
                .map(|a| a.area_mask.clone())
                .unwrap_or_default();
            self.pixel_mask = self
                .selected
                .and_then(|i| document.annotations.get(i))
                .and_then(|a| a.pixel_mask.clone());
        }
        if self.style_kind(document) != Some(AnnotationKind::Fill) {
            self.clear_selection();
        }
        self.tool = Tool::Fill;
        self.fill_tool.error = None;
        self.layers_open = false;
        self.effects_open = false;
        self.presets_open = false;
    }
    pub(super) fn sync_fill_selection(&mut self, annotation: &Annotation) {
        if let Some(region) = &annotation.fill_region {
            self.fill_tool.color = annotation.color;
            self.fill_tool.tolerance = region.tolerance;
            self.fill_tool.global = region.global;
        }
    }
    pub(super) fn poll_fill(
        &mut self,
        ctx: &egui::Context,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let Some(pending) = &self.fill_tool.pending else {
            return;
        };
        let result = match pending.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(75));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("The fill worker stopped. Please try again.".into())
            }
        };
        let pending = self.fill_tool.pending.take().unwrap();
        if document.edit_state() != pending.before {
            self.fill_tool.error =
                Some("The image changed while preparing the fill. Please try again.".into());
            return;
        }
        match result {
            Ok(annotation) => {
                let index = pending.index.unwrap_or(document.annotations.len());
                if pending.index.is_some() {
                    document.annotations[index] = annotation;
                } else {
                    document.annotations.push(annotation);
                }
                if let Err(error) = crate::document::validate_document(document) {
                    document.annotations = pending.before.annotations;
                    self.fill_tool.error = Some(error);
                    return;
                }
                history.commit(pending.before, document);
                self.select(Some(index), document);
                self.fill_tool.error = None;
                self.focus_canvas = true;
            }
            Err(error) => self.fill_tool.error = Some(error),
        }
    }
    fn start_fill_job(
        &mut self,
        ctx: &egui::Context,
        document: &Document,
        mut annotation: Annotation,
        seed: Point,
        index: Option<usize>,
    ) {
        if self.fill_tool.busy() {
            return;
        }
        let old_bounds = Rect::from_min_max(point(annotation.start), point(annotation.end));
        let bounds = Rect::from_min_max(old_bounds.min.floor(), old_bounds.max.ceil());
        annotation.area_mask = match remap_mask(&annotation.area_mask, old_bounds, bounds) {
            Ok(mask) => mask,
            Err(error) => {
                self.fill_tool.error = Some(error);
                return;
            }
        };
        annotation.start = from_pos(bounds.min);
        annotation.end = from_pos(bounds.max);
        let mut source = document.clone();
        if let Some(index) = index {
            source.annotations.truncate(index);
        }
        let tolerance = self.fill_tool.tolerance;
        let global = self.fill_tool.global;
        let (sender, receiver) = mpsc::channel();
        self.fill_tool.pending = Some(Pending {
            before: document.edit_state(),
            index,
            receiver,
        });
        self.fill_tool.error = None;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                annotation.fill_region = Some(crate::canvas::fill_region(
                    &source,
                    &annotation,
                    seed,
                    tolerance,
                    global,
                )?);
                Ok(annotation)
            }))
            .unwrap_or_else(|_| Err("The fill could not be prepared for this image.".into()));
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }
    pub(super) fn place_fill(&mut self, ctx: &egui::Context, at: Point, document: &Document) {
        let crop = document.geometry.canvas_rect(original_size(document));
        let canvas = Rect::from_min_size(
            egui::pos2(crop.x as f32, crop.y as f32),
            egui::vec2(crop.width as f32, crop.height as f32),
        );
        let region = self.area.unwrap_or(canvas).intersect(canvas);
        if !region.contains(point(at)) {
            self.fill_tool.error = Some(
                "Click inside the selected area. Use Clear selection to fill elsewhere.".into(),
            );
            return;
        }
        let mut annotation =
            self.new_annotation(AnnotationKind::Fill, from_pos(region.min.floor()));
        annotation.end = from_pos(region.max.ceil());
        annotation.color = self.fill_tool.color;
        annotation.text.clear();
        annotation.layer.name = if annotation.color[3] == 0 {
            "Transparent fill"
        } else {
            "Color fill"
        }
        .into();
        annotation.area_mask = match remap_mask(
            &self.area_mask,
            self.area.unwrap_or(canvas),
            Rect::from_min_max(point(annotation.start), point(annotation.end)),
        ) {
            Ok(mask) => mask,
            Err(error) => {
                self.fill_tool.error = Some(error);
                return;
            }
        };
        if self.area.is_some() {
            annotation.pixel_mask = self.pixel_mask.clone();
        }
        self.start_fill_job(ctx, document, annotation, at, None);
    }
    pub(super) fn fill_panel(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let selected = self.selected.filter(|&i| {
            document
                .annotations
                .get(i)
                .is_some_and(|a| a.kind == AnnotationKind::Fill)
                && self.additional.is_empty()
        });
        let enabled = selected.is_none_or(|i| !document.annotations[i].locked());
        ui.heading("Fill");
        ui.weak("Quick colors");
        ui.add_enabled_ui(enabled,|ui| {
            let old_color=self.fill_tool.color;
            ui.horizontal_wrapped(|ui| {
                for (name,color) in [("Red",[225,62,61,255]),("Blue",[46,137,221,255]),("Teal",[0,137,137,255]),("Yellow",[255,215,45,255]),("White",[255;4]),("Black",[27,35,48,255]),("Transparent",[0;4])] {
                    let (rect,response)=ui.allocate_exact_size(Vec2::splat(28.),Sense::click());
                    if color[3]==0 { paint_checkerboard(ui.painter(),rect.shrink(2.)); } else { ui.painter().rect_filled(rect.shrink(2.),5,Color32::from_rgba_unmultiplied(color[0],color[1],color[2],color[3])); }
                    let selected=self.fill_tool.color==color;
                    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel,ui.is_enabled(),selected,name));
                    ui.painter().rect_stroke(rect,5,Stroke::new(if selected {2.} else {1.},if selected {Color32::from_rgb(0,137,146)} else {Color32::from_gray(205)}),StrokeKind::Inside);
                    if response.on_hover_text(name).clicked() {self.fill_tool.color=color;}
                }
            });
            ui.add_space(7.);ui.separator();
            ui.horizontal(|ui| {ui.label("Color");ui.color_edit_button_srgba_unmultiplied(&mut self.fill_tool.color);});
            let mut transparent=self.fill_tool.color[3]==0;
            if ui.checkbox(&mut transparent,"Transparent fill").changed() {self.fill_tool.color[3]=if transparent {0} else {255};}
            ui.label("Opacity");
            let mut opacity=self.fill_tool.color[3] as f32/255.*100.;
            if crate::appearance::value_slider(ui,&mut opacity,0.0..=100.0,"Fill opacity","%").changed() {self.fill_tool.color[3]=(opacity*2.55).round() as u8;}
            if old_color!=self.fill_tool.color && let Some(index)=selected {
                let before=document.edit_state();document.annotations[index].color=self.fill_tool.color;self.color=self.fill_tool.color;self.record_properties(before,document,history);
            }
            ui.add_space(5.);ui.label("Tolerance");
            crate::appearance::value_slider(ui,&mut self.fill_tool.tolerance,0..=100,"Fill tolerance","%");
            ui.weak("Lower values protect edges. Higher values include more similar colors.");
            ui.checkbox(&mut self.fill_tool.global,"Global fill").on_hover_text("Fill matching colors in disconnected regions too, within the selected area or canvas.");
            if self.fill_tool.global {ui.weak("Matches colors across separate regions.");} else {ui.weak("Stops at boundaries around the clicked region.");}
            if let Some(index)=selected {
                let annotation=&document.annotations[index];let region=annotation.fill_region.as_ref().unwrap();
                if ui.button("Update fill region").on_hover_text("Recalculate from the image and visible layers below this fill, using the current tolerance and Global fill setting.").clicked() {
                    let seed=Point {x:annotation.start.x+(annotation.end.x-annotation.start.x)*region.seed.x,y:annotation.start.y+(annotation.end.y-annotation.start.y)*region.seed.y};
                    self.start_fill_job(ui.ctx(),document,annotation.clone(),seed,Some(index));
                }
                ui.weak("Color updates immediately. Update the region after changing tolerance, Global fill or underlying artwork.");
                if ui.button("Remove fill").clicked() {self.delete(document,history);}
                if ui.button("Open layers").clicked() {self.layers_open=true;}
            }
            ui.separator();
            if self.area.is_some() {
                ui.strong(if self.area_mask.is_empty() {"Limited to rectangle"} else {"Limited to lasso"});
                if ui.button("Clear selection").clicked() {self.area=None;self.area_mask.clear();self.clear_selection();self.tool=Tool::Fill;self.fill_tool.error=None;}
            } else {ui.strong("Canvas");}
            ui.weak("Click a background to fill it. Select an area first to limit the fill.");
        });
        if !enabled {
            ui.weak("Unlock this fill in Layers to edit it.");
        }
        if self.fill_tool.busy() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Finding the fill boundary…");
            });
        }
        if let Some(error) = &self.fill_tool.error {
            ui.colored_label(Color32::DARK_RED, error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn doc() -> Document {
        Document {
            image: std::sync::Arc::new(image::RgbaImage::from_pixel(60, 50, image::Rgba([255; 4]))),
            annotations: vec![],
            geometry: Default::default(),
        }
    }
    fn finish(e: &mut EditorState, ctx: &egui::Context, d: &mut Document, h: &mut EditHistory) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while e.fill_tool.busy() {
            e.poll_fill(ctx, d, h);
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(e.fill_tool.error.is_none(), "{:?}", e.fill_tool.error);
    }
    #[test]
    fn switching_to_fill_preserves_area_and_async_fill_is_one_edit() {
        let ctx = egui::Context::default();
        let mut e = EditorState::default();
        let mut d = doc();
        let mut h = EditHistory::new(d.edit_state());
        let area = Rect::from_min_max(egui::pos2(10., 10.), egui::pos2(40., 40.));
        e.area = Some(area);
        e.area_mask = vec![
            Point { x: 0., y: 0. },
            Point { x: 1., y: 0. },
            Point { x: 0., y: 1. },
        ];
        e.activate_fill(&mut d, &mut h);
        assert_eq!(e.area, Some(area));
        assert_eq!(e.area_mask.len(), 3);
        e.place_fill(&ctx, Point { x: 15., y: 15. }, &d);
        assert!(e.has_modal());
        finish(&mut e, &ctx, &mut d, &mut h);
        assert!(!e.has_modal());
        assert_eq!(d.annotations.len(), 1);
        assert_eq!(d.annotations[0].area_mask, e.area_mask);
        let flat = crate::render::flatten(&d).unwrap();
        assert_eq!(flat.get_pixel(15, 15), &image::Rgba(e.fill_tool.color));
        assert_eq!(flat.get_pixel(39, 39), d.image.get_pixel(39, 39));
        h.undo(&mut d);
        assert!(d.annotations.is_empty());
        assert!(!h.can_undo());
        h.redo(&mut d);
        assert_eq!(d.annotations.len(), 1);
        assert!(
            annotation_clipboard::selection_snapshot(&d, &[0])
                .unwrap_err()
                .contains("Fills")
        );
    }
    #[test]
    fn a_lasso_crossing_the_crop_is_clipped_without_changing_its_shape() {
        let ctx = egui::Context::default();
        let mut e = EditorState::default();
        let mut d = doc();
        d.geometry.crop = Some(crate::geometry::CropRect {
            x: 10,
            y: 10,
            width: 40,
            height: 30,
        });
        e.area = Some(Rect::from_min_max(
            egui::pos2(0.2, 0.2),
            egui::pos2(40.2, 40.2),
        ));
        e.area_mask = vec![
            Point { x: 0., y: 0. },
            Point { x: 1., y: 0. },
            Point { x: 0., y: 1. },
        ];
        let mut h = EditHistory::new(d.edit_state());
        e.activate_fill(&mut d, &mut h);
        e.place_fill(&ctx, Point { x: 15., y: 15. }, &d);
        finish(&mut e, &ctx, &mut d, &mut h);
        let flat = crate::render::flatten(&d).unwrap();
        assert_eq!(flat.get_pixel(5, 5), &image::Rgba(e.fill_tool.color));
        assert_eq!(flat.get_pixel(14, 14), &image::Rgba([255; 4]));
        assert_eq!(flat.get_pixel(25, 5), &image::Rgba([255; 4]));
        assert_eq!(d.annotations[0].start, Point { x: 10., y: 10. });
        assert_eq!(d.annotations[0].end, Point { x: 41., y: 40. });
        assert_eq!(d.image.get_pixel(15, 15), &image::Rgba([255; 4]));
    }
    #[test]
    fn boundary_sampling_includes_editable_lines_below_the_fill() {
        let ctx = egui::Context::default();
        let mut e = EditorState::default();
        let mut d = doc();
        let mut line = e.new_annotation(AnnotationKind::Rectangle, Point { x: 10., y: 10. });
        line.end = Point { x: 40., y: 40. };
        line.color = [0, 0, 0, 255];
        line.fill = false;
        line.stroke_width = 2.;
        d.annotations.push(line);
        let mut h = EditHistory::new(d.edit_state());
        e.activate_fill(&mut d, &mut h);
        e.place_fill(&ctx, Point { x: 20., y: 20. }, &d);
        finish(&mut e, &ctx, &mut d, &mut h);
        let flat = crate::render::flatten(&d).unwrap();
        assert_eq!(flat.get_pixel(20, 20), &image::Rgba(e.fill_tool.color));
        assert_eq!(flat.get_pixel(45, 20), &image::Rgba([255; 4]));
        let a = d.annotations[1].clone();
        e.fill_tool.global = true;
        e.start_fill_job(&ctx, &d, a, Point { x: 20., y: 20. }, Some(1));
        finish(&mut e, &ctx, &mut d, &mut h);
        assert_eq!(d.annotations.len(), 2);
        assert_eq!(
            crate::render::flatten(&d).unwrap().get_pixel(45, 20),
            &image::Rgba(e.fill_tool.color)
        );
        h.undo(&mut d);
        assert!(!d.annotations[1].fill_region.as_ref().unwrap().global);
        d.annotations[0].start = Point { x: 15., y: 15. };
        e.fill_tool.global = false;
        e.start_fill_job(
            &ctx,
            &d,
            d.annotations[1].clone(),
            Point { x: 20., y: 20. },
            Some(1),
        );
        finish(&mut e, &ctx, &mut d, &mut h);
        let refreshed = crate::render::flatten(&d).unwrap();
        assert_eq!(refreshed.get_pixel(12, 20), &image::Rgba([255; 4]));
        assert_eq!(refreshed.get_pixel(20, 20), &image::Rgba(e.fill_tool.color));
    }
}
