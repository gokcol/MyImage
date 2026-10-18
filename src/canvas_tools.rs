use super::*;

pub(super) struct CanvasDraft {
    width: u32,
    height: u32,
    anchor: usize,
}

impl EditorState {
    pub(super) fn open_canvas_size(&mut self, document: &mut Document, history: &mut EditHistory) {
        self.finish_gesture(document, history);
        let [width, height] = document.output_size();
        self.canvas_size = Some(CanvasDraft {
            width,
            height,
            anchor: 0,
        });
    }

    pub(super) fn canvas_size_dialog(
        &mut self,
        ctx: &egui::Context,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let Some(draft) = &mut self.canvas_size else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        let anchors = [
            "Top left",
            "Top center",
            "Top right",
            "Center left",
            "Center",
            "Center right",
            "Bottom left",
            "Bottom center",
            "Bottom right",
        ];
        egui::Window::new("Canvas size").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label("Resize the canvas without resizing your artwork.");
            egui::Grid::new("canvas-size-fields").show(ui, |ui| {
                ui.label("Width"); ui.add(egui::DragValue::new(&mut draft.width).range(1..=crate::document::MAX_IMAGE_SIDE).suffix(" px")); ui.end_row();
                ui.label("Height"); ui.add(egui::DragValue::new(&mut draft.height).range(1..=crate::document::MAX_IMAGE_SIDE).suffix(" px")); ui.end_row();
                ui.label("Anchor existing image");
                egui::ComboBox::from_id_salt("canvas-anchor").selected_text(anchors[draft.anchor]).show_ui(ui, |ui| {
                    for (i, name) in anchors.iter().enumerate() { ui.selectable_value(&mut draft.anchor, i, *name); }
                }); ui.end_row();
            });
            ui.small("Content outside the canvas is hidden, not deleted. Enlarge it to reveal that content again.");
            ui.horizontal(|ui| { apply = ui.button("Resize canvas").clicked(); cancel = ui.button("Cancel").clicked(); });
        });
        if !open || cancel {
            self.canvas_size = None;
            return;
        }
        if apply {
            let size = [draft.width, draft.height];
            let anchor = draft.anchor;
            let before = document.edit_state();
            let result = resize_canvas(document, size, anchor);
            match result {
                Ok(_) => {
                    history.commit(before, document);
                    self.canvas_size = None;
                    self.fit = true;
                    self.image_error = None;
                }
                Err(error) => {
                    self.image_error = Some(error);
                    self.canvas_size = None;
                }
            }
        }
    }
}

fn resize_canvas(document: &mut Document, size: [u32; 2], anchor: usize) -> Result<bool, String> {
    crate::document::validate_dimensions(size[0], size[1])?;
    let current = document.output_size();
    let left = (size[0] as f32 - current[0] as f32) * (anchor % 3) as f32 * 0.5;
    let top = (size[1] as f32 - current[1] as f32) * (anchor / 3) as f32 * 0.5;
    let inverse = |x, y| {
        point(
            document
                .geometry
                .inverse_point(Point { x, y }, original_size(document)),
        )
    };
    let bounds = Rect::from_two_pos(
        inverse(-left, -top),
        inverse(size[0] as f32 - left, size[1] as f32 - top),
    );
    let before = document.geometry;
    crate::canvas::set_view_rect(document, bounds)?;
    if document.output_size() != size {
        document.geometry.resize = Some(size);
    }
    Ok(before != document.geometry)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shrink_and_reexpand_canvas_preserves_off_canvas_pixels_layers_and_projects() {
        for turns in 0..4 {
            for anchor in [0, 4, 8] {
                let mut d = Document {
                    image: std::sync::Arc::new(image::RgbaImage::from_fn(100, 80, |x, y| {
                        image::Rgba([x as u8, y as u8, 90, 255])
                    })),
                    annotations: vec![crate::markup_tests::object(
                        AnnotationKind::Rectangle,
                        [70., 50.],
                        [90., 70.],
                    )],
                    geometry: ImageGeometry {
                        quarter_turns: turns,
                        flip_horizontal: true,
                        ..Default::default()
                    },
                };
                let size = d.output_size();
                let before = d.edit_state();
                let original = crate::render::flatten(&d).unwrap();
                let mut h = EditHistory::new(before.clone());
                resize_canvas(&mut d, [size[0] / 2, size[1] / 2], anchor).unwrap();
                h.commit(before.clone(), &d);
                assert_eq!(d.output_size(), [size[0] / 2, size[1] / 2]);
                assert_eq!(d.annotations, before.annotations);
                let flat = crate::render::flatten(&d).unwrap();
                let x = (size[0] - flat.width()) * (anchor as u32 % 3) / 2;
                let y = (size[1] - flat.height()) * (anchor as u32 / 3) / 2;
                assert_eq!(
                    flat,
                    image::imageops::crop_imm(&original, x, y, flat.width(), flat.height())
                        .to_image()
                );
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("hidden-content.myimage");
                crate::document::save_project(&d, &path).unwrap();
                let mut loaded = crate::document::load_project(&path).unwrap();
                resize_canvas(&mut loaded, size, anchor).unwrap();
                assert_eq!(crate::render::flatten(&loaded).unwrap(), original);
                assert_eq!(loaded.annotations, before.annotations);
                assert!(h.undo(&mut d));
                assert_eq!(d.edit_state(), before);
            }
        }
    }

    #[test]
    fn shrunk_canvas_keeps_moved_pixels_even_when_their_source_is_hidden() {
        let mut d = Document {
            image: std::sync::Arc::new(image::RgbaImage::from_fn(100, 80, |x, y| {
                image::Rgba([x as u8, y as u8, 80, 255])
            })),
            annotations: vec![],
            geometry: Default::default(),
        };
        let mut a = crate::markup_tests::object(AnnotationKind::MoveArea, [10., 10.], [30., 30.]);
        a.area_source = Some([Point { x: 70., y: 50. }, Point { x: 90., y: 70. }]);
        d.annotations.push(a);
        let full = crate::render::flatten(&d).unwrap();
        resize_canvas(&mut d, [50, 40], 0).unwrap();
        assert_eq!(
            crate::render::flatten(&d).unwrap(),
            image::imageops::crop_imm(&full, 0, 0, 50, 40).to_image()
        );
        crate::canvas::extend_to(
            &mut d,
            Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(100., 80.)),
        )
        .unwrap();
        assert_eq!(d.output_size(), [100, 80]);
        assert_eq!(crate::render::flatten(&d).unwrap(), full);
    }

    #[test]
    fn pasted_images_expand_at_native_size_and_drag_growth_is_one_undo() {
        let mut d = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(80, 60)),
            annotations: vec![],
            geometry: Default::default(),
        };
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState::default();
        let (source, rect) = crate::image_clipboard::pasted_raster(
            image::RgbaImage::from_pixel(120, 90, image::Rgba([255, 0, 0, 255])),
            &d,
        )
        .unwrap();
        assert_eq!(rect.size(), egui::vec2(120., 90.));
        let mut a = e.new_annotation(AnnotationKind::Stamp, from_pos(rect.min));
        a.end = from_pos(rect.max);
        a.stamp = Some(source);
        e.insert_annotations(vec![a], &mut d, &mut h);
        assert_eq!(d.output_size(), [120, 90]);
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), before);
        assert!(h.redo(&mut d));
        let pasted = d.edit_state();
        e.select(Some(0), &d);
        let origin = from_pos(crate::render::annotation_bounds(&d.annotations[0]).center());
        e.begin_selection_gesture(origin, 1., &mut d, false, false);
        e.update_snapped_gesture(
            Point {
                x: origin.x + 100.,
                y: origin.y,
            },
            &mut d,
            true,
        );
        e.finish_annotation_gesture(&mut d, &mut h);
        assert_eq!(d.output_size(), [220, 90]);
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), pasted);
    }
    #[test]
    fn whole_canvas_selection_and_flips_follow_display_axes_and_undo() {
        let mut d = Document {
            image: std::sync::Arc::new(image::RgbaImage::from_fn(20, 16, |x, y| {
                image::Rgba([x as u8, y as u8, 100, 255])
            })),
            annotations: vec![crate::markup_tests::object(
                AnnotationKind::Rectangle,
                [2., 3.],
                [8., 9.],
            )],
            geometry: Default::default(),
        };
        resize_canvas(&mut d, [40, 30], 4).unwrap();
        let mut e = EditorState::default();
        let mut h = EditHistory::new(d.edit_state());
        e.select_all_image(&mut d, &mut h);
        let rect = crate::image_clipboard::copy_bounds(&d, e.area.unwrap()).unwrap();
        assert_eq!([rect.x, rect.y, rect.width, rect.height], [0, 0, 40, 30]);
        let source = d.image.clone();
        let before = d.edit_state();
        let pixels = crate::render::flatten(&d).unwrap();
        e.flip(&mut d, &mut h, true);
        assert_eq!(
            crate::render::flatten(&d).unwrap(),
            image::imageops::flip_horizontal(&pixels)
        );
        e.rotate(&mut d, &mut h, true);
        assert_eq!(
            crate::render::flatten(&d).unwrap(),
            image::imageops::rotate90(&image::imageops::flip_horizontal(&pixels))
        );
        assert!(std::sync::Arc::ptr_eq(&d.image, &source));
        assert_eq!(d.annotations, before.annotations);
        h.undo(&mut d);
        h.undo(&mut d);
        assert_eq!(d.edit_state(), before);
    }

    #[test]
    fn manual_canvas_size_adds_transparency_without_resizing_original() {
        for anchor in [0, 4, 8] {
            let mut d = Document {
                image: std::sync::Arc::new(image::RgbaImage::from_pixel(
                    80,
                    60,
                    image::Rgba([40, 90, 120, 255]),
                )),
                annotations: vec![],
                geometry: Default::default(),
            };
            resize_canvas(&mut d, [160, 120], anchor).unwrap();
            assert_eq!(d.output_size(), [160, 120]);
            let flat = crate::render::flatten(&d).unwrap();
            assert_eq!(flat.pixels().filter(|p| p[3] > 0).count(), 80 * 60);
        }
    }
}
