use super::*;
use std::sync::mpsc;

pub(super) struct State {
    tolerance: u8,
    global: bool,
    pub(super) cut_vertical: bool,
    sensitivity: u8,
    pending: Option<(EditState, mpsc::Receiver<Result<Prepared, String>>)>,
}
enum Prepared {
    Wand(Rect, crate::fill::FillRegion),
    Simplify(Vec<Annotation>),
}
impl Default for State {
    fn default() -> Self {
        Self {
            tolerance: 15,
            global: false,
            cut_vertical: false,
            sensitivity: 55,
            pending: None,
        }
    }
}
impl State {
    pub(super) fn busy(&self) -> bool {
        self.pending.is_some()
    }
}
impl EditorState {
    pub(super) fn extra_panel_active(&self, document: &Document) -> bool {
        matches!(self.tool, Tool::Wand | Tool::CutOut)
            || self
                .style_kind(document)
                .is_some_and(AnnotationKind::is_studio)
    }
    pub(super) fn poll_extra_tools(
        &mut self,
        ctx: &egui::Context,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let Some((_, rx)) = &self.extra_tools.pending else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(v) => v,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(60));
                return;
            }
            Err(_) => Err("The image tool stopped. Please try again.".into()),
        };
        let (before, _) = self.extra_tools.pending.take().unwrap();
        if before != document.edit_state() {
            self.image_error =
                Some("The document changed while processing. Please try again.".into());
            return;
        }
        match result {
            Ok(Prepared::Wand(rect, mask)) => {
                self.clear_selection();
                self.area = Some(rect);
                self.area_mask.clear();
                self.pixel_mask = Some(mask);
                self.move_pixels = true;
                self.area_aspect_ratio = None;
                self.focus_canvas = true;
                self.image_error = None;
            }
            Ok(Prepared::Simplify(objects)) => {
                let first = document.annotations.len();
                document.annotations.extend(objects);
                if let Err(error) = crate::document::validate_document(document) {
                    document.annotations = before.annotations;
                    self.image_error = Some(error);
                    return;
                }
                history.commit(before, document);
                self.set_selection((first..document.annotations.len()).collect(), document);
                self.tool = Tool::Area;
                self.image_error = None;
            }
            Err(error) => self.image_error = Some(error),
        }
    }
    pub(super) fn place_wand(&mut self, ctx: &egui::Context, point: Point, document: &Document) {
        if self.extra_tools.busy() {
            return;
        }
        let crop = document.geometry.canvas_rect(original_size(document));
        let rect = Rect::from_min_size(
            egui::pos2(crop.x as f32, crop.y as f32),
            egui::vec2(crop.width as f32, crop.height as f32),
        );
        let mut annotation = self.new_annotation(AnnotationKind::DeleteArea, from_pos(rect.min));
        annotation.end = from_pos(rect.max);
        let document = document.clone();
        let ctx = ctx.clone();
        let tolerance = self.extra_tools.tolerance;
        let global = self.extra_tools.global;
        let (tx, rx) = mpsc::channel();
        self.extra_tools.pending = Some((document.edit_state(), rx));
        std::thread::spawn(move || {
            let result =
                crate::canvas::fill_region(&document, &annotation, point, tolerance, global).map(
                    |mask| {
                        // Tight bounds improve Copy and selection handles without losing holes.
                        let l = mask.runs.iter().map(|r| r[1]).min().unwrap();
                        let r = mask.runs.iter().map(|r| r[2]).max().unwrap();
                        let t = mask.runs.first().unwrap()[0];
                        let b = mask.runs.last().unwrap()[0] + 1;
                        let runs = mask
                            .runs
                            .iter()
                            .map(|v| [v[0] - t, v[1] - l, v[2] - l])
                            .collect();
                        let bounds = Rect::from_min_max(
                            rect.min + egui::vec2(l as f32, t as f32),
                            rect.min + egui::vec2(r as f32, b as f32),
                        );
                        let seed = (egui::pos2(point.x, point.y) - bounds.min) / bounds.size();
                        Prepared::Wand(
                            bounds,
                            crate::fill::FillRegion {
                                seed: Point {
                                    x: seed.x.clamp(0., 0.999999),
                                    y: seed.y.clamp(0., 0.999999),
                                },
                                width: r - l,
                                height: b - t,
                                runs: std::sync::Arc::new(runs),
                                ..mask
                            },
                        )
                    },
                );
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    fn auto_simplify(&mut self, ctx: &egui::Context, document: &Document) {
        if self.extra_tools.busy() {
            return;
        }
        let document = document.clone();
        let ctx = ctx.clone();
        let sensitivity = self.extra_tools.sensitivity;
        let template = self.new_annotation(AnnotationKind::Simplify, Point { x: 0., y: 0. });
        let (tx, rx) = mpsc::channel();
        self.extra_tools.pending = Some((document.edit_state(), rx));
        std::thread::spawn(move || {
            let mut source = document.clone();
            source.geometry = Default::default();
            let result=render::flatten(&source).and_then(|pixels| {
                let crop=document.geometry.crop_rect(original_size(&document));
                let visible=image::imageops::crop_imm(&pixels,crop.x,crop.y,crop.width,crop.height).to_image();
                let regions=crate::studio::simplify_regions(&visible,sensitivity);
                if regions.is_empty(){return Err("No text-like details found. Drag to add simplify blocks manually, or increase sensitivity.".into());}
                let objects=regions.into_iter().map(|[l,t,r,b]|{let mut a=template.clone();a.start=Point{x:(l+crop.x) as f32,y:(t+crop.y) as f32};a.end=Point{x:(r+crop.x) as f32,y:(b+crop.y) as f32};a.studio.ellipse=false;a.color[3]=255;a}).collect();
                Ok(Prepared::Simplify(objects))
            });
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    fn apply_cutout(&mut self, document: &mut Document, history: &mut EditHistory) {
        let Some(rect) = self.area else {
            return;
        };
        let axis = usize::from(!self.extra_tools.cut_vertical)
            ^ usize::from(document.geometry.quarter_turns % 2 == 1);
        let (a, b) = if axis == 0 {
            (rect.left(), rect.right())
        } else {
            (rect.top(), rect.bottom())
        };
        let before = document.edit_state();
        let Some(slot) = document.geometry.cutouts.iter_mut().find(|s| s.is_none()) else {
            self.image_error = Some(
                "This project already has 16 cut outs. Remove one before adding another.".into(),
            );
            return;
        };
        *slot = Some(crate::geometry::Cutout {
            axis: axis as u8,
            start: a.floor().max(0.) as u32,
            end: b.ceil().max(0.) as u32,
        });
        document.geometry.resize = None;
        if let Err(error) = document.geometry.validate(original_size(document)) {
            document.geometry = before.geometry;
            self.image_error = Some(error);
            return;
        }
        history.commit(before, document);
        self.area = None;
        self.pixel_mask = None;
        self.area_mask.clear();
        self.fit = true;
    }
    pub(super) fn extra_tools_panel(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if self.extra_tools.busy() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(if self.tool == Tool::Wand {
                    "Selecting matching pixels…"
                } else {
                    "Finding details…"
                });
            });
        }
        if self.tool == Tool::CutOut {
            ui.label(egui::RichText::new("Cut Out").size(18.).strong());
            ui.label("Remove a strip and join both sides.");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.extra_tools.cut_vertical, false, "Horizontal");
                ui.selectable_value(&mut self.extra_tools.cut_vertical, true, "Vertical");
            });
            ui.weak("Drag on the image to choose the strip. The original stays in your project.");
            if let Some(rect) = self.area {
                let bounds = mapped_bounds(rect, document);
                ui.label(format!(
                    "Remove {:.0} px",
                    if self.extra_tools.cut_vertical {
                        bounds.width()
                    } else {
                        bounds.height()
                    }
                ));
            }
            if ui
                .add_enabled(self.area.is_some(), egui::Button::new("Apply cut out"))
                .clicked()
            {
                self.apply_cutout(document, history);
            }
            ui.separator();
            ui.strong("Saved cut outs");
            let before = document.edit_state();
            for i in 0..document.geometry.cutouts.len() {
                if let Some(c) = document.geometry.cutouts[i] {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{} · {} px",
                            if c.axis == 0 { "Columns" } else { "Rows" },
                            c.end - c.start
                        ));
                        if ui.small_button("Restore").clicked() {
                            document.geometry.cutouts[i] = None;
                        }
                    });
                }
            }
            if document.edit_state() != before {
                history.commit(before, document);
                self.fit = true;
            }
            return;
        }
        if self.tool == Tool::Wand {
            ui.label(egui::RichText::new("Magic Wand").size(18.).strong());
            ui.weak("Click a color on the image to select it.");
            crate::appearance::value_slider(
                ui,
                &mut self.extra_tools.tolerance,
                0..=100,
                "Tolerance",
                "%",
            );
            ui.checkbox(
                &mut self.extra_tools.global,
                "Select matching colors everywhere",
            );
            ui.weak("Click again after changing tolerance.");
            if self.area.is_some() {
                ui.separator();
                self.area_inspector_actions(ui, document, history);
            }
            return;
        }
        let Some(kind) = self.style_kind(document).filter(|k| k.is_studio()) else {
            return;
        };
        let index = self
            .selected
            .filter(|&i| document.annotations[i].kind == kind && self.additional.is_empty());
        if let Some(i) = index {
            self.studio = document.annotations[i].studio.clone();
            self.color = document.annotations[i].color;
            self.stroke_width = document.annotations[i].stroke_width;
        }
        let title = match kind {
            AnnotationKind::Magnify => "Magnify",
            AnnotationKind::Spotlight => "Spotlight",
            _ => "Simplify",
        };
        ui.label(egui::RichText::new(title).size(18.).strong());
        ui.weak("Quick styles");
        let before = document.edit_state();
        let mut changed = false;
        if kind == AnnotationKind::Magnify {
            let sample = self.new_annotation(AnnotationKind::Magnify, Point { x: 0., y: 0. });
            ui.add_enabled_ui(
                index.is_none_or(|i| !document.annotations[i].locked()),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("magnify-quick-styles")
                        .max_height(170.)
                        .show(ui, |ui| {
                            changed |= crate::magnify_ui::gallery(
                                ui,
                                sample,
                                &mut self.studio,
                                &mut self.color,
                            );
                        });
                },
            );
        } else {
            changed |= crate::magnify_ui::shape_picker(ui, &mut self.studio);
        }
        ui.separator();
        ui.strong("Tool properties");
        ui.add_enabled_ui(
            index.is_none_or(|i| !document.annotations[i].locked()),
            |ui| match kind {
                AnnotationKind::Magnify => {
                    changed |= crate::magnify_ui::controls(ui, &mut self.studio);
                    ui.label("Border color");
                    changed |= object_properties::palette(ui, &mut self.color);
                    changed |= object_properties::slider(
                        ui,
                        &mut self.stroke_width,
                        0.1..=32.,
                        "Border width",
                        " px",
                    );
                }
                AnnotationKind::Spotlight => {
                    ui.label("Dim surroundings");
                    changed |= crate::appearance::value_slider(
                        ui,
                        &mut self.studio.dim,
                        0..=100,
                        "Dim surroundings",
                        "%",
                    )
                    .changed();
                    ui.label("Blur surroundings");
                    changed |= crate::appearance::value_slider(
                        ui,
                        &mut self.studio.blur,
                        0..=128,
                        "Blur surroundings",
                        " px",
                    )
                    .changed();
                }
                _ => {
                    ui.horizontal(|ui| {
                        ui.label("Block color");
                        changed |= ui
                            .color_edit_button_srgba_unmultiplied(&mut self.color)
                            .changed();
                    });
                    ui.horizontal(|ui| {
                        for color in [
                            [161, 175, 190, 255],
                            [209, 218, 227, 255],
                            [255; 4],
                            [23, 31, 45, 255],
                            [0, 125, 137, 255],
                        ] {
                            if ui
                                .add(
                                    egui::Button::new("  ").fill(Color32::from_rgba_unmultiplied(
                                        color[0], color[1], color[2], 255,
                                    )),
                                )
                                .clicked()
                            {
                                self.color = color;
                                changed = true;
                            }
                        }
                    });
                    ui.weak("Drag over text or details to replace them with editable blocks.");
                    ui.separator();
                    crate::appearance::value_slider(
                        ui,
                        &mut self.extra_tools.sensitivity,
                        0..=100,
                        "Detection sensitivity",
                        "%",
                    );
                    if ui.button("Auto simplify").clicked() {
                        self.auto_simplify(ui.ctx(), document);
                    }
                    ui.weak("Finds text-like rows. Each result remains a separate editable block.");
                }
            },
        );
        if changed {
            self.remember_appearance(kind);
            if let Some(i) = index.filter(|&i| !document.annotations[i].locked()) {
                document.annotations[i].studio = self.studio.clone();
                document.annotations[i].color = self.color;
                document.annotations[i].stroke_width = self.stroke_width;
                self.record_properties(before, document, history);
            }
        }
    }
    fn area_inspector_actions(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        self.privacy_actions(ui, document, history);
        ui.horizontal(|ui| {
            if ui.button("Move pixels").clicked() {
                self.set_area_move_mode(true, document);
            }
            if ui.button("Adjust boundary").clicked() {
                self.set_area_move_mode(false, document);
            }
            if ui.button("Delete pixels").clicked() {
                self.apply_area_action(AnnotationKind::DeleteArea, document, history);
            }
            if ui.button("Deselect").clicked() {
                self.deselect_pixels();
                self.focus_canvas = true;
            }
        });
        ui.weak("Copy uses this selection. Unselected pixels stay transparent.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{document, markup_tests::object};
    fn doc() -> Document {
        Document {
            image: std::sync::Arc::new(image::RgbaImage::from_fn(80, 60, |x, y| {
                image::Rgba([x as u8 * 3, y as u8 * 4, 80, 255])
            })),
            annotations: vec![],
            geometry: Default::default(),
        }
    }
    fn finish(e: &mut EditorState, ctx: &egui::Context, d: &mut Document, h: &mut EditHistory) {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while e.extra_tools.busy() {
            e.poll_extra_tools(ctx, d, h);
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(e.image_error.is_none(), "{:?}", e.image_error);
    }
    #[test]
    fn wand_keeps_holes_and_separate_regions_through_copy_delete_move_and_project() {
        let ctx = egui::Context::default();
        let mut d = doc();
        d.image = std::sync::Arc::new(image::RgbaImage::from_fn(80, 60, |x, y| {
            if (5..25).contains(&x)
                && (5..25).contains(&y)
                && !((10..15).contains(&x) && (10..15).contains(&y))
                || (50..60).contains(&x) && (5..15).contains(&y)
            {
                image::Rgba([230, 0, 0, 255])
            } else {
                image::Rgba([255; 4])
            }
        }));
        let mut h = EditHistory::new(d.edit_state());
        let mut e = EditorState::default();
        e.extra_tools.global = true;
        e.extra_tools.tolerance = 0;
        e.place_wand(&ctx, Point { x: 6., y: 6. }, &d);
        finish(&mut e, &ctx, &mut d, &mut h);
        assert_eq!(
            e.area.unwrap(),
            Rect::from_min_max(egui::pos2(5., 5.), egui::pos2(60., 25.))
        );
        let mask = e.pixel_mask.clone().unwrap();
        assert!(!mask.contains(7. / 55., 7. / 20.));
        assert!(mask.contains(46. / 55., 2. / 20.));
        let copied = crate::image_clipboard::copy_pixels_masked(
            &d,
            Some((e.area.unwrap(), vec![])),
            Some(mask.clone()),
        )
        .unwrap();
        assert_eq!(copied.dimensions(), (55, 20));
        assert_eq!(copied.get_pixel(7, 7)[3], 0);
        assert_eq!(copied.get_pixel(46, 2)[3], 255);
        e.apply_area_action(AnnotationKind::DeleteArea, &mut d, &mut h);
        assert_eq!(d.annotations[0].pixel_mask, Some(mask));
        let flat = render::flatten(&d).unwrap();
        assert_eq!(flat.get_pixel(6, 6)[3], 0);
        assert_eq!(flat.get_pixel(12, 12)[3], 255);
        assert_eq!(flat.get_pixel(51, 6)[3], 0);
        h.undo(&mut d);
        assert!(d.annotations.is_empty());
        h.redo(&mut d);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wand.myimage");
        document::save_project(&d, &path).unwrap();
        let loaded = document::load_project(&path).unwrap();
        assert_eq!(render::flatten(&loaded).unwrap(), flat);
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[8..12].copy_from_slice(&13_u32.to_le_bytes());
        std::fs::write(&path, bytes).unwrap();
        assert!(
            document::load_project(&path)
                .unwrap_err()
                .contains("version 14")
        );
        h.undo(&mut d);
        e.selected = None;
        e.area = Some(Rect::from_min_max(egui::pos2(5., 5.), egui::pos2(60., 25.)));
        e.tool = Tool::Area;
        e.set_area_move_mode(true, &d);
        e.begin_area_drag(Point { x: 18., y: 18. }, &mut d);
        e.update_area_drag(Point { x: 18., y: 38. }, &mut d);
        e.finish_area_drag(&mut d, &mut h);
        let moved = render::flatten(&d).unwrap();
        assert_eq!(moved.get_pixel(6, 26).0, [230, 0, 0, 255]);
        assert_eq!(moved.get_pixel(12, 32).0, [255; 4]);
    }
    #[test]
    fn connected_wand_does_not_select_disconnected_same_color() {
        let ctx = egui::Context::default();
        let mut d = doc();
        d.image = std::sync::Arc::new(image::RgbaImage::from_fn(12, 8, |x, _| {
            if x == 6 {
                image::Rgba([0, 0, 0, 255])
            } else {
                image::Rgba([255; 4])
            }
        }));
        let mut e = EditorState::default();
        e.extra_tools.tolerance = 0;
        let mut h = EditHistory::new(d.edit_state());
        e.place_wand(&ctx, Point { x: 1., y: 1. }, &d);
        finish(&mut e, &ctx, &mut d, &mut h);
        assert_eq!(e.area.unwrap().width(), 6.);
    }
    #[test]
    fn focus_effects_are_ordered_adjustable_reversible_and_round_trip() {
        let mut d = doc();
        let original = d.image.clone();
        let mut lens = object(AnnotationKind::Magnify, [10., 10.], [50., 50.]);
        lens.studio.ellipse = false;
        lens.stroke_width = 1.;
        lens.color = [0; 4];
        d.annotations.push(lens);
        let a = render::flatten(&d).unwrap();
        assert_eq!(a.get_pixel(20, 20), original.get_pixel(25, 25));
        assert_eq!(a.get_pixel(0, 0), original.get_pixel(0, 0));
        d.annotations[0].studio.zoom = 4.;
        let b = render::flatten(&d).unwrap();
        assert_ne!(a, b);
        let mut spot = object(AnnotationKind::Spotlight, [10., 10.], [50., 50.]);
        spot.studio.ellipse = false;
        spot.studio.dim = 100;
        d.annotations.push(spot);
        let flat = render::flatten(&d).unwrap();
        assert_eq!(flat.get_pixel(0, 0).0, [0, 0, 0, 255]);
        assert_eq!(flat.get_pixel(20, 20), b.get_pixel(20, 20));
        d.annotations[1].studio.blur = 8;
        d.annotations[1].studio.dim = 30;
        let mut block = object(AnnotationKind::Simplify, [2., 2.], [12., 8.]);
        block.studio.ellipse = false;
        block.color = [10, 200, 210, 255];
        d.annotations.push(block);
        let flat = render::flatten(&d).unwrap();
        assert_eq!(flat.get_pixel(4, 4).0, [10, 200, 210, 255]);
        assert_eq!(render::privacy_preview(&d, 80).unwrap(), flat);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("effects.myimage");
        document::save_project(&d, &path).unwrap();
        let loaded = document::load_project(&path).unwrap();
        assert_eq!(loaded.edit_state(), d.edit_state());
        assert_eq!(loaded.image, original);
        assert_eq!(render::flatten(&loaded).unwrap(), flat);
        assert!(render::thumbnail(&loaded, 40, 30).is_ok());
        d.annotations[0].studio.zoom = f32::NAN;
        assert!(document::validate_document(&d).is_err());
    }
    #[test]
    fn linked_magnify_samples_source_and_preserves_editability() {
        let mut d = doc();
        let original = d.image.clone();
        let mut lens = object(AnnotationKind::Magnify, [40., 20.], [76., 56.]);
        lens.studio.linked = true;
        lens.studio.ellipse = false;
        lens.studio.rounded = true;
        lens.stroke_width = 1.;
        crate::studio::set_source_center(&mut lens, egui::pos2(14., 14.));
        d.annotations.push(lens.clone());
        let flat = render::flatten(&d).unwrap();
        assert_eq!(flat.get_pixel(58, 38), original.get_pixel(14, 14));
        assert_eq!(flat.get_pixel(40, 20), original.get_pixel(40, 20));
        assert_eq!(render::privacy_preview(&d, 80).unwrap(), flat);
        let resized = resized(&lens, Handle::BottomRight, Point { x: 78., y: 58. });
        assert!(crate::studio::source_center(&resized).distance(egui::pos2(14., 14.)) < 0.001);
        let moved = resized_source(&lens, egui::pos2(24., 18.));
        assert_eq!(moved.start, lens.start);
        assert_eq!(moved.end, lens.end);
        let mut history = EditHistory::new(d.edit_state());
        let before = d.edit_state();
        d.annotations[0] = moved;
        history.commit(before, &d);
        assert_eq!(
            render::flatten(&d).unwrap().get_pixel(58, 38),
            original.get_pixel(24, 18)
        );
        assert!(history.undo(&mut d));
        assert_eq!(d.annotations[0], lens);
        assert!(history.redo(&mut d));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("linked-lens.myimage");
        document::save_project(&d, &path).unwrap();
        let loaded = document::load_project(&path).unwrap();
        assert_eq!(loaded.edit_state(), d.edit_state());
        assert_eq!(
            render::flatten(&loaded).unwrap(),
            render::flatten(&d).unwrap()
        );
        d.annotations[0].studio.source_offset[0] = f32::NAN;
        assert!(document::validate_document(&d).is_err());
    }
    fn resized_source(lens: &Annotation, at: egui::Pos2) -> Annotation {
        assert!(
            handles(lens)
                .iter()
                .any(|(h, _)| *h == Handle::MagnifySource)
        );
        resized(lens, Handle::MagnifySource, from_pos(at))
    }
    #[test]
    fn linked_magnify_samples_transparency_outside_image_and_lower_layers() {
        let mut d = doc();
        let mut lens = object(AnnotationKind::Magnify, [40., 20.], [76., 56.]);
        lens.studio.linked = true;
        crate::studio::set_source_center(&mut lens, egui::pos2(-20., -20.));
        let mut image = (*d.image).clone();
        crate::studio::apply(&mut image, &lens, [1.; 2]).unwrap();
        assert_eq!(image.get_pixel(58, 38)[3], 0);
        crate::studio::set_source_center(&mut lens, egui::pos2(15., 15.));
        let mut block = object(AnnotationKind::Simplify, [5., 5.], [25., 25.]);
        block.studio.ellipse = false;
        block.color = [5, 120, 180, 255];
        d.annotations = vec![block.clone(), lens];
        assert_eq!(
            render::flatten(&d).unwrap().get_pixel(58, 38).0,
            block.color
        );
    }
    #[test]
    fn former_default_toolbar_gets_magnify_without_replacing_custom_layouts() {
        let mut e = EditorState::default();
        e.set_toolbar(
            [
                "Select", "Arrow", "Text", "Callout", "Shapes", "Stamps", "Fill",
            ]
            .map(str::to_owned)
            .to_vec(),
            true,
        );
        assert_eq!(e.toolbar_pins, default_toolbar());
        assert!(e.take_toolbar_change().is_some());
        e.set_toolbar(vec!["Text".into(), "Magnify".into()], false);
        assert_eq!(e.toolbar_pins, vec!["Text", "Magnify"]);
    }
    #[test]
    fn cutouts_join_pixels_and_annotations_with_all_rotations_and_restore() {
        for turn in 0..4 {
            let mut d = doc();
            d.geometry.quarter_turns = turn;
            d.annotations
                .push(object(AnnotationKind::Rectangle, [25., 35.], [40., 45.]));
            let source = render::flatten(&Document {
                geometry: Default::default(),
                ..d.clone()
            })
            .unwrap();
            let mut e = EditorState {
                tool: Tool::CutOut,
                area: Some(Rect::from_min_max(
                    egui::pos2(10., 20.),
                    egui::pos2(20., 30.),
                )),
                ..Default::default()
            };
            let mut h = EditHistory::new(d.edit_state());
            e.extra_tools.cut_vertical = turn % 2 == 1;
            e.apply_cutout(&mut d, &mut h);
            assert!(e.image_error.is_none());
            let unrotated = image::RgbaImage::from_fn(80, 50, |x, y| {
                *source.get_pixel(x, if y < 20 { y } else { y + 10 })
            });
            let expected = match turn {
                1 => image::imageops::rotate90(&unrotated),
                2 => image::imageops::rotate180(&unrotated),
                3 => image::imageops::rotate270(&unrotated),
                _ => unrotated,
            };
            assert_eq!(render::flatten(&d).unwrap(), expected);
            for p in [Point { x: 7., y: 8. }, Point { x: 25., y: 40. }] {
                let q = d.geometry.inverse_point(
                    d.geometry.map_point(p, original_size(&d)),
                    original_size(&d),
                );
                assert_eq!(p, q);
            }
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("cut.myimage");
            document::save_project(&d, &path).unwrap();
            assert_eq!(
                render::flatten(&document::load_project(&path).unwrap()).unwrap(),
                expected
            );
            assert!(render::thumbnail(&d, 40, 40).is_ok());
            h.undo(&mut d);
            assert!(!d.geometry.has_cutouts());
            h.redo(&mut d);
            assert!(d.geometry.has_cutouts());
        }
    }
    #[test]
    fn overlapping_cutouts_union_and_reject_empty_image() {
        let mut d = doc();
        d.geometry.cutouts[0] = Some(crate::geometry::Cutout {
            axis: 0,
            start: 10,
            end: 30,
        });
        d.geometry.cutouts[1] = Some(crate::geometry::Cutout {
            axis: 0,
            start: 20,
            end: 40,
        });
        assert_eq!(d.output_size(), [50, 60]);
        let flat = render::flatten(&d).unwrap();
        assert_eq!(flat.get_pixel(10, 1), d.image.get_pixel(40, 1));
        d.geometry.cutouts[2] = Some(crate::geometry::Cutout {
            axis: 0,
            start: 0,
            end: 80,
        });
        assert!(document::validate_document(&d).is_err());
    }
    #[test]
    fn automatic_simplify_produces_editable_blocks_without_touching_source() {
        let ctx = egui::Context::default();
        let mut d = doc();
        d.image = std::sync::Arc::new(image::RgbaImage::from_fn(300, 160, |x, y| {
            if (20..130).contains(&x) && (20..29).contains(&y)
                || (20..200).contains(&x) && (50..59).contains(&y)
            {
                image::Rgba([30, 30, 30, 255])
            } else {
                image::Rgba([255; 4])
            }
        }));
        let original = d.image.clone();
        let mut e = EditorState::default();
        let mut h = EditHistory::new(d.edit_state());
        e.auto_simplify(&ctx, &d);
        finish(&mut e, &ctx, &mut d, &mut h);
        assert_eq!(d.annotations.len(), 2);
        assert!(
            d.annotations
                .iter()
                .all(|a| a.kind == AnnotationKind::Simplify)
        );
        assert_eq!(d.image, original);
        assert!(e.area_bounds(&d).is_none());
        assert_eq!(e.selected_indices(&d).len(), 2);
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: Key::Delete,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| e.keyboard(ui, &mut d, &mut h));
            },
        );
        assert!(
            d.annotations.is_empty(),
            "Delete removes selected blocks, not underlying pixels"
        );
        h.undo(&mut d);
        assert_eq!(d.annotations.len(), 2);
        h.undo(&mut d);
        assert!(d.annotations.is_empty());
    }
    #[test]
    fn toolbar_preferences_survive_document_reset_and_ignore_unknown_duplicates() {
        let mut e = EditorState::default();
        e.set_toolbar(
            vec![
                "Wand".into(),
                "Magic Wand".into(),
                "Text".into(),
                "Text".into(),
            ],
            false,
        );
        e.reset();
        assert_eq!(e.toolbar_pins, vec!["Magic Wand", "Text"]);
        let s = crate::storage::Settings {
            editor_toolbar: e.toolbar_pins,
            ..Default::default()
        };
        let loaded: crate::storage::Settings =
            serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(loaded.editor_toolbar, vec!["Magic Wand", "Text"]);
    }
}

#[derive(Clone)]
struct MaskPreview {
    mask: crate::fill::FillRegion,
    texture: egui::TextureHandle,
}
pub(super) fn paint_pixel_mask(
    painter: &egui::Painter,
    mask: &crate::fill::FillRegion,
    rect: Rect,
    document: &Document,
    origin: Pos2,
    scale: f32,
) {
    let id = egui::Id::new("wand-selection-preview");
    let cached = painter.ctx().data(|d| d.get_temp::<MaskPreview>(id));
    let texture = if let Some(cached) = cached.filter(|c| {
        std::sync::Arc::ptr_eq(&c.mask.runs, &mask.runs)
            && c.mask.width == mask.width
            && c.mask.height == mask.height
    }) {
        cached.texture
    } else {
        let density = (768. / mask.width.max(mask.height) as f32).min(1.);
        let width = (mask.width as f32 * density).ceil().max(1.) as usize;
        let height = (mask.height as f32 * density).ceil().max(1.) as usize;
        let mut pixels =
            egui::ColorImage::new([width, height], vec![Color32::TRANSPARENT; width * height]);
        for &[y, l, r] in mask.runs.iter() {
            let top = (y as f32 * density).floor() as usize;
            let bottom = (((y + 1) as f32 * density).ceil() as usize).min(height);
            let left = (l as f32 * density).floor() as usize;
            let right = ((r as f32 * density).ceil() as usize).min(width);
            for py in top..bottom {
                for px in left..right {
                    pixels.pixels[py * width + px] =
                        Color32::from_rgba_unmultiplied(0, 175, 190, 65);
                }
            }
        }
        let texture =
            painter
                .ctx()
                .load_texture("Color selection", pixels, egui::TextureOptions::NEAREST);
        painter.ctx().data_mut(|d| {
            d.insert_temp(
                id,
                MaskPreview {
                    mask: mask.clone(),
                    texture: texture.clone(),
                },
            )
        });
        texture
    };
    let mut mesh = egui::Mesh::with_texture(texture.id());
    for (l, r) in document.geometry.kept_intervals(original_size(document), 0) {
        for (t, b) in document.geometry.kept_intervals(original_size(document), 1) {
            let piece = rect.intersect(Rect::from_min_max(
                egui::pos2(l as f32, t as f32),
                egui::pos2(r as f32, b as f32),
            ));
            if !piece.is_positive() {
                continue;
            }
            let uv = Rect::from_min_max(
                ((piece.min - rect.min) / rect.size()).to_pos2(),
                ((piece.max - rect.min) / rect.size()).to_pos2(),
            );
            mesh.add_rect_with_uv(piece, uv, Color32::WHITE);
        }
    }
    for v in &mut mesh.vertices {
        v.pos = origin + output_point(from_pos(v.pos), document).to_vec2() * scale;
    }
    painter.add(egui::Shape::mesh(mesh));
}
