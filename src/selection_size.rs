//! Selection dimensions are measured in the pixels that will be copied, not
//! original-image coordinates or UI points. Changing them is never a pixel edit.
use super::*;
use area_selection::AreaHandle;

impl EditorState {
    pub(super) fn selection_size_controls(&mut self, ui: &mut egui::Ui, document: &Document) {
        let Some(bounds) = self
            .area_bounds(document)
            .and_then(|r| crate::image_clipboard::copy_bounds(document, r).ok())
        else {
            return;
        };
        let mut size = [bounds.width, bounds.height];
        let maximum = document.output_size();
        let mut changed = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.;
            let width = (ui.available_width() - 40.) / 2.;
            for (axis, label) in [(0, "Width"), (1, "Height")] {
                ui.push_id(("selection_dimension", axis), |ui| {
                    ui.vertical(|ui| {
                        let label = ui.small(label);
                        let response = ui.add_sized(
                            [width, 26.],
                            egui::DragValue::new(&mut size[axis])
                                .range(1..=maximum[axis])
                                .speed(1)
                                .suffix(" px"),
                        );
                        let response = response.labelled_by(label.id).on_hover_text(
                            "Selection size in copied-image pixels. Type a value or drag to adjust.",
                        );
                        if response.changed() {
                            changed = Some(axis);
                        }
                    });
                });
            }
            ui.vertical(|ui| {
                ui.add_space(17.);
                let locked = self.area_aspect_ratio.is_some();
                if inspector_widgets::tile(
                    ui, "Lock aspect ratio", inspector_widgets::Glyph::Lock(locked),
                    Vec2::splat(28.), locked, false,
                ).on_hover_text("Lock proportions while sizing the selection or dragging its handles").clicked() {
                    self.set_selection_ratio_locked(!locked, document);
                }
            });
        });
        if let Some(axis) = changed {
            self.set_selection_dimension(axis, size[axis], document);
        }
    }

    fn set_selection_ratio_locked(&mut self, locked: bool, document: &Document) {
        self.area_aspect_ratio = locked
            .then(|| {
                self.area_bounds(document)
                    .and_then(|r| crate::image_clipboard::copy_bounds(document, r).ok())
                    .map(|b| f64::from(b.width) / f64::from(b.height))
            })
            .flatten();
    }

    fn set_selection_dimension(&mut self, axis: usize, value: u32, document: &Document) {
        let Some(bounds) = self
            .area_bounds(document)
            .and_then(|r| crate::image_clipboard::copy_bounds(document, r).ok())
        else {
            return;
        };
        let maximum = document.output_size();
        let size = dimensions(
            [bounds.width, bounds.height],
            axis,
            value,
            self.area_aspect_ratio,
            maximum,
        );
        // Preserve the top-left unless growing would leave the canvas; in that
        // case shift the boundary, rather than silently shrinking the size.
        let target = Rect::from_min_size(
            egui::pos2(
                bounds.x.min(maximum[0] - size[0]) as f32,
                bounds.y.min(maximum[1] - size[1]) as f32,
            ),
            egui::vec2(size[0] as f32, size[1] as f32),
        );
        let Some(source) = source_rect(target, document) else {
            self.image_error = Some(
                "This selection size cannot be represented at the current image scale.".into(),
            );
            return;
        };
        // Even a selected saved effect only supplies the initial boundary/mask.
        // Numeric selection sizing must never edit that annotation or its pixels.
        let focus = self.focus_canvas;
        self.set_area_move_mode(self.move_pixels, document);
        self.focus_canvas = focus;
        self.area = Some(source);
        self.clipboard_status = None;
    }
}

fn dimensions(
    mut size: [u32; 2],
    axis: usize,
    value: u32,
    ratio: Option<f64>,
    max: [u32; 2],
) -> [u32; 2] {
    if let Some(ratio) = ratio {
        let ratio = if axis == 0 { ratio } else { 1. / ratio };
        let upper = (f64::from(max[1 - axis]) * ratio + 1e-9)
            .floor()
            .clamp(1., f64::from(max[axis])) as u32;
        let lower = ratio.ceil().clamp(1., f64::from(upper)) as u32;
        size[axis] = value.clamp(lower, upper);
        size[1 - axis] = (f64::from(size[axis]) / ratio)
            .round()
            .clamp(1., f64::from(max[1 - axis])) as u32;
    } else {
        size[axis] = value.clamp(1, max[axis]);
    }
    size
}

pub(super) fn output_rect(bounds: CropRect) -> Rect {
    Rect::from_min_size(
        egui::pos2(bounds.x as f32, bounds.y as f32),
        egui::vec2(bounds.width as f32, bounds.height as f32),
    )
}

/// Keep the original coordinate storage, but verify its pixel-edge mapping.
/// Tiny inward nudges avoid a floating-point inverse landing just outside an
/// integer edge and adding a spurious row/column to the clipboard crop.
pub(super) fn source_rect(target: Rect, document: &Document) -> Option<Rect> {
    if !target.is_positive() {
        return None;
    }
    let wanted = CropRect {
        x: target.left() as u32,
        y: target.top() as u32,
        width: target.width() as u32,
        height: target.height() as u32,
    };
    let inverse = |p: Pos2| {
        point(
            document
                .geometry
                .inverse_point(from_pos(p), original_size(document)),
        )
    };
    for inset in [0., 0.001, 0.01, 0.1, 0.25] {
        let r = target.shrink(inset);
        let source = Rect::from_two_pos(inverse(r.min), inverse(r.max));
        if source.is_positive()
            && crate::image_clipboard::copy_bounds(document, source).ok() == Some(wanted)
        {
            return Some(source);
        }
    }
    None
}

pub(super) fn locked_drag(
    initial: Rect,
    delta: Vec2,
    handle: AreaHandle,
    ratio: f64,
    canvas: [u32; 2],
) -> Rect {
    use AreaHandle::*;
    let left = matches!(handle, Left | TopLeft | BottomLeft);
    let right = matches!(handle, Right | TopRight | BottomRight);
    let top = matches!(handle, Top | TopLeft | TopRight);
    let bottom = matches!(handle, Bottom | BottomLeft | BottomRight);
    let width = initial.width() + if left { -delta.x } else { delta.x };
    let height = initial.height() + if top { -delta.y } else { delta.y };
    let axis = if !left && !right {
        1
    } else if !top && !bottom || delta.x.abs() / initial.width() >= delta.y.abs() / initial.height()
    {
        0
    } else {
        1
    };
    let max_width = if left {
        initial.right()
    } else if right {
        canvas[0] as f32 - initial.left()
    } else {
        2. * initial
            .center()
            .x
            .min(canvas[0] as f32 - initial.center().x)
    };
    let max_height = if top {
        initial.bottom()
    } else if bottom {
        canvas[1] as f32 - initial.top()
    } else {
        2. * initial
            .center()
            .y
            .min(canvas[1] as f32 - initial.center().y)
    };
    let size = dimensions(
        [initial.width() as u32, initial.height() as u32],
        axis,
        if axis == 0 { width } else { height }.round().max(1.) as u32,
        Some(ratio),
        [
            max_width.floor().max(1.) as u32,
            max_height.floor().max(1.) as u32,
        ],
    );
    let [w, h] = size.map(|v| v as f32);
    let x = if left {
        initial.right() - w
    } else if right {
        initial.left()
    } else {
        initial.center().x - w * 0.5
    };
    let y = if top {
        initial.bottom() - h
    } else if bottom {
        initial.top()
    } else {
        initial.center().y - h * 0.5
    };
    Rect::from_min_size(
        egui::pos2(
            x.round().clamp(0., canvas[0] as f32 - w),
            y.round().clamp(0., canvas[1] as f32 - h),
        ),
        egui::vec2(w, h),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> Document {
        Document {
            image: std::sync::Arc::new(image::RgbaImage::from_fn(100, 100, |x, y| {
                image::Rgba([x as u8, y as u8, 80, 255])
            })),
            annotations: vec![],
            geometry: Default::default(),
        }
    }

    fn bounds(editor: &EditorState, document: &Document) -> CropRect {
        crate::image_clipboard::copy_bounds(document, editor.area.unwrap()).unwrap()
    }

    #[test]
    fn numeric_dimensions_copy_exact_output_pixels_after_crop_rotation_resize_and_cutouts() {
        for rotation in 0..4 {
            for cutouts in [false, true] {
                let mut d = document();
                d.geometry.crop = Some(CropRect {
                    x: 7,
                    y: 9,
                    width: 86,
                    height: 81,
                });
                d.geometry.quarter_turns = rotation;
                d.geometry.resize = Some([157, 123]);
                if cutouts {
                    d.geometry.cutouts[0] = Some(crate::geometry::Cutout {
                        axis: 0,
                        start: 30,
                        end: 41,
                    });
                    d.geometry.cutouts[1] = Some(crate::geometry::Cutout {
                        axis: 1,
                        start: 45,
                        end: 51,
                    });
                }
                let before = d.edit_state();
                let flat = render::flatten(&d).unwrap();
                let mut h = EditHistory::new(before.clone());
                let mut e = EditorState {
                    tool: Tool::Area,
                    area: source_rect(
                        Rect::from_min_size(egui::pos2(10., 12.), egui::vec2(40., 30.)),
                        &d,
                    ),
                    ..Default::default()
                };
                e.set_selection_dimension(0, 37, &d);
                e.set_selection_dimension(1, 23, &d);
                assert_eq!(
                    bounds(&e, &d),
                    CropRect {
                        x: 10,
                        y: 12,
                        width: 37,
                        height: 23
                    }
                );
                let copied =
                    crate::image_clipboard::copy_pixels(&d, Some((e.area.unwrap(), vec![])))
                        .unwrap();
                assert_eq!(
                    copied,
                    image::imageops::crop_imm(&flat, 10, 12, 37, 23).to_image()
                );
                // Optional boundary-only movement keeps the precise entered size.
                e.set_area_move_mode(false, &d);
                let at = d
                    .geometry
                    .inverse_point(Point { x: 28., y: 23. }, original_size(&d));
                let to = d
                    .geometry
                    .inverse_point(Point { x: 33., y: 30. }, original_size(&d));
                e.begin_area_drag(at, &mut d);
                e.update_area_drag(to, &mut d);
                e.finish_area_drag(&mut d, &mut h);
                let actual = bounds(&e, &d);
                assert_eq!([actual.width, actual.height], [37, 23]);
                assert_eq!([actual.x, actual.y], [15, 19]);
                assert_eq!(d.edit_state(), before);
                assert!(!h.can_undo());
            }
        }
    }

    #[test]
    fn ratio_lock_uses_a_stable_ratio_in_both_fields_and_respects_canvas_limits() {
        let mut d = document();
        let mut e = EditorState {
            area: Some(Rect::from_min_size(
                egui::pos2(20., 30.),
                egui::vec2(40., 20.),
            )),
            ..Default::default()
        };
        let before = d.edit_state();
        e.set_selection_ratio_locked(true, &d);
        e.set_selection_dimension(0, 61, &d);
        assert_eq!([bounds(&e, &d).width, bounds(&e, &d).height], [61, 31]);
        e.set_selection_dimension(1, 20, &d);
        assert_eq!([bounds(&e, &d).width, bounds(&e, &d).height], [40, 20]);
        e.set_selection_dimension(1, 100, &d);
        assert_eq!([bounds(&e, &d).width, bounds(&e, &d).height], [100, 50]);
        assert_eq!(bounds(&e, &d).x, 0);
        e.set_selection_dimension(0, 0, &d);
        assert_eq!([bounds(&e, &d).width, bounds(&e, &d).height], [2, 1]);
        e.set_selection_ratio_locked(false, &d);
        e.set_selection_dimension(1, 17, &d);
        assert_eq!([bounds(&e, &d).width, bounds(&e, &d).height], [2, 17]);
        assert_eq!(d.edit_state(), before);
        let mut h = EditHistory::new(before);
        e.set_selection_ratio_locked(true, &d);
        e.select_all_image(&mut d, &mut h);
        assert!(e.area_aspect_ratio.is_none());
    }

    #[test]
    fn locked_edges_and_corners_preserve_opposite_anchors_and_do_not_edit_pixels() {
        use AreaHandle::*;
        let initial = Rect::from_min_size(egui::pos2(30., 30.), egui::vec2(40., 20.));
        for handle in [
            TopLeft,
            Top,
            TopRight,
            Right,
            BottomRight,
            Bottom,
            BottomLeft,
            Left,
        ] {
            for delta in [
                egui::vec2(12., 6.),
                egui::vec2(-500., -500.),
                egui::vec2(500., 500.),
            ] {
                let actual = locked_drag(initial, delta, handle, 2., [100, 100]);
                assert!(actual.is_positive());
                assert!(Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.)).contains_rect(actual));
                assert!((actual.width() * 0.5 - actual.height()).abs() <= 0.5);
                if matches!(handle, Left | TopLeft | BottomLeft) {
                    assert_eq!(actual.right(), initial.right());
                }
                if matches!(handle, Right | TopRight | BottomRight) {
                    assert_eq!(actual.left(), initial.left());
                }
                if matches!(handle, Top | TopLeft | TopRight) {
                    assert_eq!(actual.bottom(), initial.bottom());
                }
                if matches!(handle, Bottom | BottomLeft | BottomRight) {
                    assert_eq!(actual.top(), initial.top());
                }
            }
        }
        let mut d = document();
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState {
            tool: Tool::Area,
            area: Some(initial),
            ..Default::default()
        };
        e.set_selection_ratio_locked(true, &d);
        e.begin_area_drag(from_pos(initial.right_bottom()), &mut d);
        e.update_area_drag(Point { x: 82., y: 56. }, &mut d);
        e.finish_area_drag(&mut d, &mut h);
        assert_eq!([bounds(&e, &d).width, bounds(&e, &d).height], [52, 26]);
        assert_eq!(d.edit_state(), before);
        assert!(!h.can_undo());
    }

    #[test]
    fn numeric_size_detaches_saved_effect_and_preserves_masks_and_field_focus() {
        let mut d = document();
        let mut e = EditorState {
            area: Some(Rect::from_min_size(egui::pos2(20., 20.), Vec2::splat(40.))),
            area_mask: vec![
                Point { x: 0., y: 0. },
                Point { x: 1., y: 0. },
                Point { x: 0., y: 1. },
            ],
            pixel_mask: Some(crate::fill::FillRegion {
                seed: Point { x: 0.1, y: 0.1 },
                tolerance: 0,
                global: true,
                width: 40,
                height: 40,
                runs: std::sync::Arc::new((0..40).map(|y| [y, 0, 20]).collect()),
            }),
            ..Default::default()
        };
        let mut h = EditHistory::new(d.edit_state());
        e.apply_area_action(AnnotationKind::Blur, &mut d, &mut h);
        let before = d.edit_state();
        let mask = d.annotations[0].area_mask.clone();
        let pixels = d.annotations[0].pixel_mask.clone();
        e.focus_canvas = false;
        e.set_selection_dimension(0, 30, &d);
        assert!(e.selected.is_none());
        assert!(e.move_pixels);
        assert!(!e.focus_canvas);
        assert_eq!(e.area_mask, mask);
        assert_eq!(e.pixel_mask, pixels);
        assert_eq!(d.edit_state(), before);
        assert!(h.undo(&mut d));
        assert!(!h.can_undo());
    }

    #[test]
    fn growing_near_an_edge_preserves_entered_size_and_one_pixel_is_valid() {
        let d = document();
        let mut e = EditorState {
            area: Some(Rect::from_min_size(
                egui::pos2(90., 92.),
                egui::vec2(10., 8.),
            )),
            ..Default::default()
        };
        e.set_selection_dimension(0, 70, &d);
        e.set_selection_dimension(1, 80, &d);
        assert_eq!(
            bounds(&e, &d),
            CropRect {
                x: 30,
                y: 20,
                width: 70,
                height: 80
            }
        );
        e.set_selection_dimension(0, 1, &d);
        e.set_selection_dimension(1, 1, &d);
        assert_eq!(
            bounds(&e, &d),
            CropRect {
                x: 30,
                y: 20,
                width: 1,
                height: 1
            }
        );
        assert_eq!(
            crate::image_clipboard::copy_pixels(&d, Some((e.area.unwrap(), vec![])))
                .unwrap()
                .dimensions(),
            (1, 1)
        );
    }
}
