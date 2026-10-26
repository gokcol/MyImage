//! Object editing can use the whole workspace. Only the presentation grows during
//! a drag; committing publishes geometry and annotation edits as one undo step.
use super::*;

impl Gesture {
    pub(super) fn changed_objects(&self, document: &Document) -> Vec<usize> {
        let indices = match &self.kind {
            GestureKind::Draw(i) | GestureKind::Move(i) | GestureKind::Resize(i, _) => vec![*i],
            GestureKind::MoveMany(indices) | GestureKind::ResizeMany(indices, _) => indices.clone(),
            GestureKind::Marquee { .. } | GestureKind::Erase { .. } => vec![],
        };
        indices
            .into_iter()
            .filter(|&i| {
                document
                    .annotations
                    .get(i)
                    .is_some_and(|a| has_extent(a) && self.before.annotations.get(i) != Some(a))
            })
            .collect()
    }
}

pub(super) fn object_point(position: Pos2, rect: Rect, scale: f32, document: &Document) -> Point {
    document.geometry.inverse_point(
        from_pos(((position - rect.min) / scale).to_pos2()),
        original_size(document),
    )
}

impl EditorState {
    pub(super) fn commit_object_placement(
        &mut self,
        before: EditState,
        document: &mut Document,
        history: &mut EditHistory,
    ) -> bool {
        let index = document.annotations.len() - 1;
        if let Err(error) = crate::canvas::extend_for_annotations(document, &[index])
            .and_then(|_| crate::document::validate_document(document))
        {
            document.annotations = before.annotations;
            document.geometry = before.geometry;
            self.image_error = Some(error);
            return false;
        }
        history.commit(before, document);
        true
    }

    fn object_preview_indices(&self, document: &Document) -> Vec<usize> {
        if let Some(draft) = &self.text_dialog {
            vec![draft.index]
        } else {
            self.gesture
                .as_ref()
                .map_or_else(Vec::new, |g| g.changed_objects(document))
        }
    }

    pub(super) fn object_canvas_preview(
        &self,
        document: &Document,
        rect: Rect,
        scale: f32,
    ) -> Result<Option<(Document, Rect)>, String> {
        let indices = self.object_preview_indices(document);
        if indices.is_empty() {
            return Ok(None);
        }
        let mut expanded = document.clone();
        if !crate::canvas::extend_for_annotations(&mut expanded, &indices)? {
            return Ok(None);
        }
        let screen = object_preview_rect(&expanded, document, rect, scale);
        Ok(Some((expanded, screen)))
    }

    pub(super) fn object_canvas_texture_frame(
        &mut self,
        document: &Document,
        rect: Rect,
        scale: f32,
        viewport: Rect,
    ) -> Result<Option<(Document, Rect)>, String> {
        let indices = self.object_preview_indices(document);
        let Some(gesture) = self.gesture.as_mut() else {
            return Ok(None);
        };
        let mut source = document.clone();
        if let Some(geometry) = gesture.preview_geometry {
            source.geometry = geometry;
        } else {
            // In-bounds annotation edits must keep the original image texture.
            // Reserving a larger canvas here would unnecessarily replace the
            // whole image with a reduced composition until the drag ends.
            if indices.is_empty() || !crate::canvas::extend_for_annotations(&mut source, &indices)?
            {
                return Ok(None);
            }
            // Reserve the workspace plus an object-sized margin once. A fixed
            // sampling grid avoids shimmering as the visible canvas grows.
            let padding =
                mapped_bounds(selection_bounds(document, &indices), document).size() * scale;
            let viewport = viewport.expand2(padding.max(Vec2::splat(32.)));
            let bounds = Rect::from_two_pos(
                point(object_point(viewport.min, rect, scale, document)),
                point(object_point(viewport.max, rect, scale, document)),
            );
            crate::canvas::extend_to(&mut source, bounds)?;
            gesture.preview_geometry = Some(source.geometry);
        }
        let screen = object_preview_rect(&source, document, rect, scale);
        Ok(Some((source, screen)))
    }
}

fn object_preview_rect(expanded: &Document, original: &Document, rect: Rect, scale: f32) -> Rect {
    let bounds = expanded.geometry.view_rect(original_size(original));
    let source = Rect::from_min_size(
        egui::pos2(bounds.x as f32, bounds.y as f32),
        egui::vec2(bounds.width as f32, bounds.height as f32),
    );
    let mapped = mapped_bounds(source, original);
    Rect::from_min_max(
        rect.min + mapped.min.to_vec2() * scale,
        rect.min + mapped.max.to_vec2() * scale,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn document() -> Document {
        Document {
            image: Arc::new(image::RgbaImage::from_pixel(
                200,
                140,
                image::Rgba([20, 80, 160, 255]),
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
        let texture = ctx.load_texture(
            "object canvas fixture",
            egui::ColorImage::new([1, 1], vec![Color32::BLUE]),
            Default::default(),
        );
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(500., 400.))),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    e.canvas(ui, d, h, &texture, &mut Default::default());
                });
            },
        )
    }

    // Keep both caches alive across frames: rebuilding the fixture texture or
    // preview every frame would hide a source/composition/source transition.
    fn persistent_frame(
        ctx: &egui::Context,
        editor: &mut EditorState,
        document: &mut Document,
        history: &mut EditHistory,
        texture: &egui::TextureHandle,
        preview: &mut crate::privacy_preview::PrivacyPreview,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1200., 900.))),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    editor.canvas(ui, document, history, texture, preview);
                });
            },
        )
    }

    fn source_quad(
        ctx: &egui::Context,
        output: &egui::FullOutput,
        source: egui::TextureId,
    ) -> Vec<(Pos2, Pos2)> {
        let meshes: Vec<_> = ctx
            .tessellate(output.shapes.clone(), output.pixels_per_point)
            .into_iter()
            .filter_map(|primitive| match primitive.primitive {
                egui::epaint::Primitive::Mesh(mesh)
                    if mesh.texture_id != egui::TextureId::default() =>
                {
                    Some(mesh)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            meshes.len(),
            1,
            "one unchanged source image, no reduced composition"
        );
        assert_eq!(
            meshes[0].texture_id, source,
            "the screenshot texture must not flicker"
        );
        meshes[0]
            .vertices
            .iter()
            .map(|vertex| (vertex.pos, vertex.uv))
            .collect()
    }

    #[test]
    fn in_bounds_arrow_edits_keep_the_same_source_texture_and_sampling() {
        let ctx = egui::Context::default();
        let mut document = document();
        document.image = Arc::new(image::RgbaImage::from_fn(1600, 1000, |x, y| {
            // Fine detail makes the old 768-pixel fallback observably different.
            let value = if (x / 3 + y / 3) % 2 == 0 { 235 } else { 35 };
            image::Rgba([value, value, value, 255])
        }));
        let original = document.image.clone();
        let original_geometry = document.geometry;
        let texture = ctx.load_texture(
            "persistent captured window",
            crate::image_loading::color_image(&document.image),
            egui::TextureOptions::LINEAR,
        );
        let mut preview = crate::privacy_preview::PrivacyPreview::default();
        let mut history = EditHistory::new(document.edit_state());
        let mut editor = EditorState {
            tool: Tool::Arrow,
            fit: false,
            zoom: 0.5,
            snapping: false,
            ..Default::default()
        };
        let output = persistent_frame(
            &ctx,
            &mut editor,
            &mut document,
            &mut history,
            &texture,
            &mut preview,
            vec![],
        );
        let expected = source_quad(&ctx, &output, texture.id());
        let rect = expected.iter().fold(Rect::NOTHING, |rect, (pos, _)| {
            rect.union(Rect::from_min_max(*pos, *pos))
        });
        let check = |output: &egui::FullOutput| {
            assert_eq!(source_quad(&ctx, output, texture.id()), expected);
        };
        let start = rect.min + egui::vec2(100., 90.);
        let end = rect.min + egui::vec2(360., 230.);
        for events in [
            vec![egui::Event::PointerMoved(start), button(start, true)],
            vec![egui::Event::PointerMoved(end)],
            vec![button(end, false)],
        ] {
            check(&persistent_frame(
                &ctx,
                &mut editor,
                &mut document,
                &mut history,
                &texture,
                &mut preview,
                events,
            ));
        }
        assert_eq!(document.annotations.len(), 1);
        assert_eq!(document.geometry, original_geometry);
        for handle in [None, Some(Handle::ArrowEnd), Some(Handle::ArrowBend)] {
            editor.select(Some(0), &document);
            let original_arrow = document.annotations[0].clone();
            let point = match handle {
                None => crate::arrow_style::curve_point(&original_arrow, 0.25),
                Some(Handle::ArrowEnd) => point(original_arrow.end),
                _ => crate::arrow_style::curve_point(&original_arrow, 0.5),
            };
            let press = rect.min + point.to_vec2() * 0.5;
            let destination = press
                + if handle == Some(Handle::ArrowBend) {
                    egui::vec2(0., 60.)
                } else {
                    egui::vec2(35., 20.)
                };
            check(&persistent_frame(
                &ctx,
                &mut editor,
                &mut document,
                &mut history,
                &texture,
                &mut preview,
                vec![egui::Event::PointerMoved(press), button(press, true)],
            ));
            assert!(editor.gesture.is_some());
            check(&persistent_frame(
                &ctx,
                &mut editor,
                &mut document,
                &mut history,
                &texture,
                &mut preview,
                vec![egui::Event::PointerMoved(destination)],
            ));
            assert_ne!(document.annotations[0], original_arrow);
            assert!(editor.gesture.as_ref().unwrap().preview_geometry.is_none());
            check(&persistent_frame(
                &ctx,
                &mut editor,
                &mut document,
                &mut history,
                &texture,
                &mut preview,
                vec![button(destination, false)],
            ));
            assert_eq!(document.geometry, original_geometry);
        }
        assert!(document.annotations[0].arrow.curve.is_some());
        editor.select(Some(0), &document);
        editor.color = [20, 100, 230, 180];
        editor.stroke_width = 12.;
        editor.arrow.pattern = crate::arrow_style::Pattern::Dashed;
        editor.apply_inspector(&mut document, &mut history);
        check(&persistent_frame(
            &ctx,
            &mut editor,
            &mut document,
            &mut history,
            &texture,
            &mut preview,
            vec![],
        ));
        assert_eq!(document.annotations[0].color, [20, 100, 230, 180]);
        assert_eq!(document.annotations[0].stroke_width, 12.);
        assert!(Arc::ptr_eq(&document.image, &original));
        assert_eq!(document.geometry, original_geometry);
    }

    #[test]
    fn retained_sampling_frame_starts_only_at_the_edge_and_lasts_until_release() {
        let mut document = document();
        let mut editor = EditorState::default();
        let mut arrow = editor.new_annotation(AnnotationKind::Arrow, Point { x: 40., y: 40. });
        arrow.end = Point { x: 120., y: 80. };
        document.annotations.push(arrow);
        let before = document.edit_state();
        let mut history = EditHistory::new(before.clone());
        editor.gesture = Some(Gesture {
            kind: GestureKind::Move(0),
            before: before.clone(),
            origin: Point { x: 40., y: 40. },
            preview_geometry: None,
        });
        let rect = Rect::from_min_size(egui::pos2(150., 130.), egui::vec2(200., 140.));
        let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(500., 400.));
        document.annotations[0].end.x = 150.;
        assert!(
            editor
                .object_canvas_texture_frame(&document, rect, 1., viewport)
                .unwrap()
                .is_none()
        );
        assert!(editor.gesture.as_ref().unwrap().preview_geometry.is_none());
        document.annotations[0].end.x = 240.;
        let (outside, _) = editor
            .object_canvas_texture_frame(&document, rect, 1., viewport)
            .unwrap()
            .unwrap();
        assert!(outside.geometry.canvas.is_some());
        // Return exactly to the initial arrow: changed_objects is empty, but
        // abandoning the established sampling frame mid-drag would shimmer.
        document.annotations = before.annotations.clone();
        let (inside, _) = editor
            .object_canvas_texture_frame(&document, rect, 1., viewport)
            .unwrap()
            .unwrap();
        assert_eq!(inside.geometry, outside.geometry);
        assert_eq!(document.geometry, before.geometry);
        editor.finish_annotation_gesture(&mut document, &mut history);
        assert!(
            editor
                .object_canvas_texture_frame(&document, rect, 1., viewport)
                .unwrap()
                .is_none()
        );
        assert_eq!(document.edit_state(), before);
        assert!(!history.can_undo());
    }

    #[test]
    fn draw_from_workspace_and_across_each_edge_grows_transparently_in_one_undo() {
        for tool in [
            Tool::Arrow,
            Tool::Rectangle,
            Tool::Ellipse,
            Tool::Shape(AnnotationKind::RoundedRectangle),
            Tool::Shape(AnnotationKind::Torus),
            Tool::Callout,
            Tool::Highlighter,
            Tool::Pen,
        ] {
            for (start, end) in [
                (egui::pos2(100., 90.), egui::pos2(230., 200.)), // begins above and left of the image
                (egui::pos2(240., 200.), egui::pos2(400., 320.)), // ends below and right
                (egui::pos2(400., 100.), egui::pos2(460., 70.)), // entirely outside
            ] {
                let mut d = document();
                let source = d.image.clone();
                let before = d.edit_state();
                let mut h = EditHistory::new(before.clone());
                let ctx = egui::Context::default();
                let mut e = EditorState {
                    tool,
                    fit: false,
                    zoom: 1.,
                    snapping: false,
                    ..Default::default()
                };
                frame(&ctx, &mut e, &mut d, &mut h, vec![]);
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
                assert_eq!(
                    d.annotations.len(),
                    1,
                    "{tool:?}: margin press must start drawing"
                );
                assert_eq!(
                    d.geometry, before.geometry,
                    "no coordinate feedback during a held drag"
                );
                assert!(!h.can_undo());
                assert!(e.selected.is_none());
                let canvas_rect =
                    Rect::from_min_size(egui::pos2(150., 130.), egui::vec2(200., 140.));
                let (preview, screen) = e
                    .object_canvas_preview(&d, canvas_rect, 1.)
                    .unwrap()
                    .expect("live extension");
                assert!(screen.contains_rect(canvas_rect));
                assert!(preview.output_size()[0] > 200 || preview.output_size()[1] > 140);
                frame(&ctx, &mut e, &mut d, &mut h, vec![button(end, false)]);
                assert!(e.image_error.is_none(), "{:?}", e.image_error);
                assert_eq!(d.annotations.len(), 1);
                let bounds = render::annotation_bounds(&d.annotations[0]);
                let b = d.geometry.view_rect(original_size(&d));
                let expanded = Rect::from_min_size(
                    egui::pos2(b.x as f32, b.y as f32),
                    egui::vec2(b.width as f32, b.height as f32),
                );
                assert!(
                    expanded.contains_rect(bounds),
                    "{tool:?}: {expanded:?} should contain {bounds:?}"
                );
                let flat = render::flatten(&d).unwrap();
                assert!(
                    flat.pixels().any(|p| p[3] == 0),
                    "new empty space must be transparent"
                );
                for y in 0..140 {
                    for x in 0..200 {
                        if !bounds.expand(1.).contains(egui::pos2(x as f32, y as f32)) {
                            assert_eq!(
                                flat.get_pixel((x - b.x) as u32, (y - b.y) as u32),
                                source.get_pixel(x as u32, y as u32)
                            );
                        }
                    }
                }
                assert!(Arc::ptr_eq(&d.image, &source));
                let result = d.edit_state();
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("outside.myimage");
                crate::document::save_project(&d, &path).unwrap();
                let reopened = crate::document::load_project(&path).unwrap();
                assert_eq!(reopened.edit_state(), result);
                assert_eq!(render::flatten(&reopened).unwrap(), flat);
                assert!(h.undo(&mut d));
                assert_eq!(d.edit_state(), before);
                assert!(!h.can_undo());
                assert!(h.redo(&mut d));
                assert_eq!(d.edit_state(), result);
            }
        }
    }

    #[test]
    fn clicks_escape_and_invalid_growth_do_not_leave_extra_canvas_or_history() {
        let ctx = egui::Context::default();
        let mut d = document();
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState {
            tool: Tool::Rectangle,
            fit: false,
            ..Default::default()
        };
        frame(&ctx, &mut e, &mut d, &mut h, vec![]);
        let start = egui::pos2(90., 90.);
        frame(
            &ctx,
            &mut e,
            &mut d,
            &mut h,
            vec![
                egui::Event::PointerMoved(start),
                button(start, true),
                button(start, false),
            ],
        );
        assert_eq!(d.edit_state(), before);
        assert!(!h.can_undo());
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
            vec![egui::Event::PointerMoved(egui::pos2(410., 320.))],
        );
        frame(
            &ctx,
            &mut e,
            &mut d,
            &mut h,
            vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert_eq!(d.edit_state(), before);
        assert!(!h.can_undo());
        e.tool = Tool::Rectangle;
        e.begin_tool_gesture(Point { x: 50., y: 50. }, 1., &mut d, false, false);
        e.update_gesture(
            Point {
                x: 1_000_000.,
                y: 1_000_000.,
            },
            &mut d,
        );
        e.finish_annotation_gesture(&mut d, &mut h);
        assert!(e.image_error.is_some());
        assert_eq!(d.edit_state(), before);
        assert!(!h.can_undo());
    }

    #[test]
    fn moved_resized_and_grouped_objects_extend_transformed_canvas_without_clamping() {
        for turns in 0..4 {
            for flip in [false, true] {
                let mut d = document();
                d.geometry.quarter_turns = turns;
                d.geometry.flip_horizontal = flip;
                d.geometry.resize = Some([300, 240]);
                crate::canvas::set_view_rect(
                    &mut d,
                    Rect::from_min_max(egui::pos2(15., 10.), egui::pos2(180., 120.)),
                )
                .unwrap();
                let mut e = EditorState {
                    tool: Tool::Select,
                    snapping: false,
                    ..Default::default()
                };
                let mut a = e.new_annotation(AnnotationKind::Rectangle, Point { x: 20., y: 20. });
                a.end = Point { x: 80., y: 70. };
                a.fill = true;
                let mut arrow = e.new_annotation(AnnotationKind::Arrow, Point { x: 100., y: 20. });
                arrow.end = Point { x: 170., y: 90. };
                arrow.arrow.curve = Some([Point { x: 80., y: 40. }, Point { x: 140., y: 110. }]);
                d.annotations = vec![a, arrow];
                e.set_selection(vec![0, 1], &d);
                let before = d.edit_state();
                let mut h = EditHistory::new(before.clone());
                e.gesture = Some(Gesture {
                    kind: GestureKind::MoveMany(vec![0, 1]),
                    before: before.clone(),
                    origin: Point { x: 40., y: 40. },
                    preview_geometry: None,
                });
                e.update_snapped_gesture(Point { x: -60., y: -40. }, &mut d, true);
                assert_eq!(d.annotations[0].start, Point { x: -80., y: -60. });
                assert_eq!(d.geometry, before.geometry);
                e.finish_annotation_gesture(&mut d, &mut h);
                assert!(e.image_error.is_none());
                assert!(d.geometry.view_rect(original_size(&d)).x < 0);
                let moved = d.edit_state();
                assert!(h.undo(&mut d));
                assert_eq!(d.edit_state(), before);
                assert!(h.redo(&mut d));
                assert_eq!(d.edit_state(), moved);
                e.select(Some(0), &d);
                let resize_before = d.edit_state();
                e.gesture = Some(Gesture {
                    kind: GestureKind::Resize(0, Handle::Right),
                    before: resize_before.clone(),
                    origin: Point { x: -20., y: -35. },
                    preview_geometry: None,
                });
                e.update_gesture(Point { x: 260., y: -35. }, &mut d);
                e.finish_annotation_gesture(&mut d, &mut h);
                let b = d.geometry.view_rect(original_size(&d));
                assert!(b.x + b.width as i32 >= 262);
                assert!(h.undo(&mut d));
                assert_eq!(d.edit_state(), resize_before);
            }
        }
    }

    #[test]
    fn text_stamps_and_steps_can_be_placed_in_the_margin_and_cancelled() {
        for tool in [Tool::Text, Tool::Stamp, Tool::Step] {
            let ctx = egui::Context::default();
            let mut d = document();
            let before = d.edit_state();
            let mut h = EditHistory::new(before.clone());
            let mut e = EditorState {
                tool,
                fit: false,
                ..Default::default()
            };
            frame(&ctx, &mut e, &mut d, &mut h, vec![]);
            let at = egui::pos2(100., 80.);
            frame(
                &ctx,
                &mut e,
                &mut d,
                &mut h,
                vec![egui::Event::PointerMoved(at), button(at, true)],
            );
            frame(&ctx, &mut e, &mut d, &mut h, vec![button(at, false)]);
            assert_eq!(d.annotations.len(), 1, "{tool:?}");
            if tool == Tool::Text {
                assert!(
                    e.object_canvas_preview(
                        &d,
                        Rect::from_min_size(egui::pos2(150., 130.), egui::vec2(200., 140.)),
                        1.
                    )
                    .unwrap()
                    .is_some()
                );
                e.finish_text_editor(&mut d, &mut h, false);
                assert_eq!(d.edit_state(), before);
                assert!(!h.can_undo());
                let a = e.new_annotation(AnnotationKind::Text, Point { x: -50., y: -50. });
                d.annotations.push(a);
                e.open_text_editor(0, before.clone(), &d);
                e.finish_text_editor(&mut d, &mut h, true);
            }
            assert!(d.geometry.view_rect(original_size(&d)).x < 0, "{tool:?}");
            assert!(d.geometry.view_rect(original_size(&d)).y < 0, "{tool:?}");
            assert!(h.undo(&mut d));
            assert_eq!(d.edit_state(), before);
            assert!(!h.can_undo());
        }
    }

    #[test]
    fn growing_canvas_preserves_curved_arrows_in_preview_and_export() {
        let mut d = document();
        let e = EditorState::default();
        let mut a = e.new_annotation(AnnotationKind::Arrow, Point { x: 30., y: 90. });
        a.end = Point { x: 170., y: 90. };
        a.arrow.curve = Some([Point { x: 70., y: 0. }, Point { x: 130., y: 0. }]);
        d.annotations.push(a);
        let base = render::flatten(&d).unwrap();
        crate::canvas::extend_to(
            &mut d,
            Rect::from_min_max(egui::pos2(-50., -40.), egui::pos2(280., 200.)),
        )
        .unwrap();
        let flat = render::flatten(&d).unwrap();
        let preview = render::interactive_preview(&d, 768).unwrap();
        assert_eq!(flat, preview);
        for y in 0..140 {
            for x in 0..200 {
                assert_eq!(base.get_pixel(x, y), flat.get_pixel(x + 50, y + 40));
            }
        }
        assert_eq!(flat.get_pixel(0, 0)[3], 0);
    }
    #[test]
    fn live_fit_stays_stable_and_margin_input_respects_rotated_resized_images() {
        for turns in 0..4 {
            for flip in [false, true] {
                let ctx = egui::Context::default();
                let mut d = document();
                d.geometry.quarter_turns = turns;
                d.geometry.flip_vertical = flip;
                d.geometry.resize = Some([200, 140]);
                let before = d.edit_state();
                let mut h = EditHistory::new(before.clone());
                let mut e = EditorState {
                    tool: Tool::Arrow,
                    fit: true,
                    ..Default::default()
                };
                frame(&ctx, &mut e, &mut d, &mut h, vec![]);
                let rect = Rect::from_min_size(egui::pos2(150., 130.), egui::vec2(200., 140.));
                let start = egui::pos2(100., 80.);
                let end = egui::pos2(600., 460.);
                let expected_start = object_point(start, rect, 1., &d);
                let expected_end = object_point(end, rect, 1., &d);
                frame(
                    &ctx,
                    &mut e,
                    &mut d,
                    &mut h,
                    vec![egui::Event::PointerMoved(start), button(start, true)],
                );
                let mut last = None;
                for events in [vec![egui::Event::PointerMoved(end)], vec![], vec![]] {
                    let output = frame(&ctx, &mut e, &mut d, &mut h, events);
                    assert_eq!(d.annotations[0].start, expected_start);
                    assert_eq!(d.annotations[0].end, expected_end);
                    assert_eq!(d.geometry, before.geometry);
                    assert_eq!(e.zoom, 1.);
                    let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
                    let (clip, mesh) = primitives
                        .iter()
                        .find_map(|p| match &p.primitive {
                            egui::epaint::Primitive::Mesh(m)
                                if m.texture_id != egui::TextureId::default() =>
                            {
                                Some((p.clip_rect, m))
                            }
                            _ => None,
                        })
                        .expect("current preview stays visible");
                    let bounds = mesh.vertices.iter().fold(Rect::NOTHING, |r, v| {
                        r.union(Rect::from_min_max(v.pos, v.pos))
                    });
                    assert!(
                        clip.expand(0.1).contains_rect(bounds),
                        "entire growing canvas must fit while held"
                    );
                    let geometry = e.gesture.as_ref().unwrap().preview_geometry.unwrap();
                    if let Some((old_bounds, old_geometry)) = last {
                        assert_eq!(bounds, old_bounds);
                        assert_eq!(geometry, old_geometry);
                    }
                    last = Some((bounds, geometry));
                }
                frame(&ctx, &mut e, &mut d, &mut h, vec![button(end, false)]);
                assert!(d.output_size()[0] > 200);
                assert!(h.undo(&mut d));
                assert_eq!(d.edit_state(), before);
            }
        }
    }
}
