//! Selection combines object clicks with a pixel-area marquee.
use super::*;

pub(super) struct AreaDrag {
    origin: Point,
    before: EditState,
    rect: Option<Rect>,
    index: Option<usize>,
    moving: bool,
    travelled: bool,
    samples: Vec<Point>,
    mask: Vec<Point>,
    pixel_mask: Option<crate::fill::FillRegion>,
    handle: Option<AreaHandle>,
    moves_pixels: bool,
    area_before: Option<Rect>,
    selection_before: Vec<usize>,
    move_pixels_before: bool,
    aspect_before: Option<f64>,
    preview_geometry: Option<ImageGeometry>,
}

impl AreaDrag {
    fn moves_content(&self, document: &Document) -> bool {
        self.moving
            && self.handle.is_none()
            && match self.index {
                Some(index) => document
                    .annotations
                    .get(index)
                    .is_some_and(|a| a.kind == AnnotationKind::MoveArea),
                None => self.moves_pixels,
            }
    }
}

/// Handles are in displayed-image coordinates, so cursors follow rotations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AreaHandle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

impl AreaHandle {
    fn positions(r: Rect) -> [(Self, Pos2); 8] {
        [
            (Self::TopLeft, r.left_top()),
            (Self::TopRight, r.right_top()),
            (Self::BottomRight, r.right_bottom()),
            (Self::BottomLeft, r.left_bottom()),
            (Self::Top, r.center_top()),
            (Self::Right, r.right_center()),
            (Self::Bottom, r.center_bottom()),
            (Self::Left, r.left_center()),
        ]
    }
    fn hit(r: Rect, at: Pos2, zoom: f32) -> Option<Self> {
        let tolerance = 7. / zoom;
        let corner_tolerance = tolerance.min(r.width().min(r.height()) * 0.25);
        // Corners take priority; the complete edge is draggable, not just its dot.
        Self::positions(r)
            .into_iter()
            .take(4)
            .find(|(_, p)| p.distance(at) <= corner_tolerance)
            .map(|(h, _)| h)
            .or_else(|| {
                if !r.expand(tolerance).contains(at) {
                    return None;
                }
                [
                    (Self::Top, (at.y - r.top()).abs()),
                    (Self::Bottom, (at.y - r.bottom()).abs()),
                    (Self::Left, (at.x - r.left()).abs()),
                    (Self::Right, (at.x - r.right()).abs()),
                ]
                .into_iter()
                .filter(|(h, d)| {
                    *d <= tolerance.min(match h {
                        Self::Top | Self::Bottom => r.height() * 0.25,
                        _ => r.width() * 0.25,
                    })
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(h, _)| h)
            })
    }
    fn cursor(self) -> CursorIcon {
        match self {
            Self::Left | Self::Right => CursorIcon::ResizeHorizontal,
            Self::Top | Self::Bottom => CursorIcon::ResizeVertical,
            Self::TopLeft | Self::BottomRight => CursorIcon::ResizeNwSe,
            Self::TopRight | Self::BottomLeft => CursorIcon::ResizeNeSw,
        }
    }
}

impl EditorState {
    /// A pixel selection is UI state, never an annotation or a project edit.
    pub(super) fn select_all_image(&mut self, document: &mut Document, history: &mut EditHistory) {
        self.finish_gesture(document, history);
        self.deselect_pixels();
        let crop = document.geometry.canvas_rect(original_size(document));
        self.area = Some(Rect::from_min_size(
            egui::pos2(crop.x as f32, crop.y as f32),
            egui::vec2(crop.width as f32, crop.height as f32),
        ));
        self.tool = Tool::Area;
        self.layers_open = false;
        self.presets_open = false;
        self.focus_canvas = true;
    }

    pub(super) fn deselect_pixels(&mut self) {
        self.move_pixels = true;
        self.area_aspect_ratio = None;
        self.area = None;
        self.area_mask.clear();
        self.pixel_mask = None;
        self.clear_selection();
        self.clipboard_status = None;
    }

    pub(super) fn pixel_selection_commands(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if ui
            .button("Select all")
            .on_hover_text("Select the entire visible image (Command/Ctrl + A)")
            .clicked()
        {
            self.select_all_image(document, history);
            ui.close();
        }
        if ui
            .add_enabled(
                self.area.is_some() || self.selected.is_some(),
                egui::Button::new("Deselect"),
            )
            .on_hover_text("Clear the selection (Escape)")
            .clicked()
        {
            self.deselect_pixels();
            self.focus_canvas = true;
            ui.close();
        }
    }

    pub(super) fn area_bounds(&self, document: &Document) -> Option<Rect> {
        self.selected
            .and_then(|i| document.annotations.get(i))
            .filter(|a| a.kind.needs_composite() && !a.kind.is_studio())
            .map(interaction_bounds)
            .or(self.area)
    }

    fn area_handle(&self, at: Point, document: &Document) -> Option<AreaHandle> {
        (self.tool == Tool::Area)
            .then(|| self.area_bounds(document))
            .flatten()
            .and_then(|r| {
                AreaHandle::hit(
                    mapped_bounds(r, document),
                    output_point(at, document),
                    self.zoom,
                )
            })
    }

    pub(super) fn over_area_handle(&self, at: Point, document: &Document) -> bool {
        self.area_handle(at, document).is_some()
    }

    fn inside_area(&self, at: Point, document: &Document) -> bool {
        self.area_bounds(document).is_some_and(|r| {
            let p = (point(at) - r.min) / r.size();
            let annotation = self.selected.and_then(|i| document.annotations.get(i));
            let mask = annotation
                .map(|a| a.area_mask.as_slice())
                .unwrap_or(&self.area_mask);
            let pixel_mask = annotation
                .and_then(|a| a.pixel_mask.as_ref())
                .or(self.pixel_mask.as_ref());
            r.contains(point(at))
                && (mask.is_empty() || crate::studio::polygon_contains(mask, p.x, p.y))
                && pixel_mask.is_none_or(|m| m.contains(p.x, p.y))
        })
    }

    pub(super) fn area_cursor(&self, at: Point, document: &Document) -> CursorIcon {
        if let Some(drag) = &self.area_drag {
            return drag
                .handle
                .map(AreaHandle::cursor)
                .unwrap_or(if drag.moving {
                    CursorIcon::Grabbing
                } else {
                    CursorIcon::Crosshair
                });
        }
        if self
            .selected
            .and_then(|i| document.annotations.get(i))
            .is_some_and(|a| a.locked())
        {
            return CursorIcon::Default;
        }
        self.area_handle(at, document)
            .map(AreaHandle::cursor)
            .unwrap_or(if self.inside_area(at, document) {
                CursorIcon::Grab
            } else {
                CursorIcon::Crosshair
            })
    }

    pub(super) fn set_area_move_mode(&mut self, pixels: bool, document: &Document) {
        let Some(rect) = self.area_bounds(document) else {
            return;
        };
        if let Some(a) = self.selected.and_then(|i| document.annotations.get(i)) {
            self.area_mask = a.area_mask.clone();
            self.pixel_mask = a.pixel_mask.clone();
        }
        self.clear_selection();
        self.area = Some(rect);
        self.move_pixels = pixels;
        self.tool = Tool::Area;
        self.focus_canvas = true;
        self.clipboard_status = None;
    }

    pub(super) fn begin_area_drag(&mut self, at: Point, document: &mut Document) {
        let rect = self.area_bounds(document);
        let mask = self
            .selected
            .and_then(|i| document.annotations.get(i))
            .map(|a| a.area_mask.clone())
            .unwrap_or_else(|| self.area_mask.clone());
        let handle = self.area_handle(at, document);
        let pixel_mask = self
            .selected
            .and_then(|i| document.annotations.get(i))
            .map(|a| a.pixel_mask.clone())
            .unwrap_or_else(|| self.pixel_mask.clone());
        let moving =
            self.tool != Tool::CutOut && (handle.is_some() || self.inside_area(at, document));
        let index = if moving {
            self.selected.filter(|&i| {
                document.annotations.get(i).is_some_and(|a| {
                    a.kind.needs_composite()
                        && !a.locked()
                        && !(handle.is_some() && a.kind == AnnotationKind::MoveArea)
                })
            })
        } else {
            None
        };
        if moving
            && self
                .selected
                .and_then(|i| document.annotations.get(i))
                .is_some_and(|a| a.locked())
        {
            return;
        }
        let area_before = self.area;
        let selection_before = self.selected_indices(document);
        let move_pixels_before = self.move_pixels;
        let aspect_before = self.area_aspect_ratio;
        if !moving {
            self.move_pixels = true;
            self.area_aspect_ratio = None;
            self.pixel_mask = None;
            self.area = None;
            self.area_mask.clear();
        }
        self.clipboard_status = None;
        self.area_drag = Some(AreaDrag {
            origin: at,
            before: document.edit_state(),
            rect,
            index,
            moving,
            travelled: false,
            samples: vec![at],
            mask,
            pixel_mask,
            handle,
            moves_pixels: self.move_pixels,
            area_before,
            selection_before,
            move_pixels_before,
            aspect_before,
            preview_geometry: None,
        });
    }

    pub(super) fn moving_area_pixels(&self, document: &Document) -> bool {
        self.area_drag
            .as_ref()
            .is_some_and(|drag| drag.moves_content(document))
    }

    /// Render a temporary expanded composition without feeding its new origin,
    /// dimensions or fit zoom back into pointer tracking. The image Arc stays shared.
    pub(super) fn area_drag_preview(
        &self,
        document: &Document,
        rect: Rect,
        scale: f32,
    ) -> Result<Option<(Document, Rect)>, String> {
        let Some(drag) = self
            .area_drag
            .as_ref()
            .filter(|d| d.moves_content(document))
        else {
            return Ok(None);
        };
        let Some(index) = drag.index else {
            return Ok(None);
        };
        let mut expanded = document.clone();
        if !crate::canvas::extend_for_annotations(&mut expanded, &[index])? {
            return Ok(None);
        }
        let bounds = expanded.geometry.view_rect(original_size(document));
        let source = Rect::from_min_size(
            egui::pos2(bounds.x as f32, bounds.y as f32),
            egui::vec2(bounds.width as f32, bounds.height as f32),
        );
        // Mapping the new bounds through the *old* geometry preserves the exact
        // screen scale, even after rotation, flips, cutouts or rounded resizing.
        let mapped = mapped_bounds(source, document);
        let preview_rect = Rect::from_min_max(
            rect.min + mapped.min.to_vec2() * scale,
            rect.min + mapped.max.to_vec2() * scale,
        );
        Ok(Some((expanded, preview_rect)))
    }

    /// Freeze the preview's sampling grid for the whole gesture. Growing the
    /// texture on every pointer move resamples even stationary pixels and shimmers.
    pub(super) fn area_drag_texture_frame(
        &mut self,
        document: &Document,
        rect: Rect,
        scale: f32,
        viewport: Rect,
    ) -> Result<Option<(Document, Rect)>, String> {
        let Some(drag) = self
            .area_drag
            .as_mut()
            .filter(|d| d.moves_content(document))
        else {
            return Ok(None);
        };
        let mut source = document.clone();
        if let Some(geometry) = drag.preview_geometry {
            source.geometry = geometry;
        } else {
            let inverse = |p: Pos2| {
                document.geometry.inverse_point(
                    from_pos((p - rect.min).to_pos2() / scale),
                    original_size(document),
                )
            };
            let padding = drag
                .rect
                .map_or(Vec2::ZERO, |r| mapped_bounds(r, document).size() * scale);
            let viewport = viewport.expand2(padding);
            let bounds =
                Rect::from_two_pos(point(inverse(viewport.min)), point(inverse(viewport.max)));
            crate::canvas::extend_to(&mut source, bounds)?;
            drag.preview_geometry = Some(source.geometry);
        }
        let bounds = source.geometry.view_rect(original_size(document));
        let mapped = mapped_bounds(
            Rect::from_min_size(
                egui::pos2(bounds.x as f32, bounds.y as f32),
                egui::vec2(bounds.width as f32, bounds.height as f32),
            ),
            document,
        );
        let screen = Rect::from_min_max(
            rect.min + mapped.min.to_vec2() * scale,
            rect.min + mapped.max.to_vec2() * scale,
        );
        Ok(Some((source, screen)))
    }

    pub(super) fn area_drag_point(
        &self,
        position: Pos2,
        rect: Rect,
        scale: f32,
        document: &Document,
    ) -> Point {
        if self.moving_area_pixels(document) {
            document.geometry.inverse_point(
                Point {
                    x: (position.x - rect.min.x) / scale,
                    y: (position.y - rect.min.y) / scale,
                },
                original_size(document),
            )
        } else {
            image_point(position, rect, scale, document)
        }
    }

    pub(super) fn update_area_drag(&mut self, at: Point, document: &mut Document) {
        let Some(mut drag) = self.area_drag.take() else {
            return;
        };
        drag.travelled |= output_point(at, document).distance(output_point(drag.origin, document))
            * self.zoom
            > 3.;
        if drag.travelled {
            if drag.moving {
                let initial = drag.rect.unwrap();
                let target = if drag.moves_content(document) {
                    // Keep the lifted region's source size and allow the cursor
                    // outside the canvas. Grow on release, just like raster layers,
                    // so changing canvas bounds never feeds back into the drag.
                    let delta =
                        (output_point(at, document) - output_point(drag.origin, document)).round();
                    let origin = output_point(drag.origin, document);
                    let destination = document
                        .geometry
                        .inverse_point(from_pos(origin + delta), original_size(document));
                    initial.translate(point(destination) - point(drag.origin))
                } else {
                    adjusted_area(
                        initial,
                        drag.origin,
                        at,
                        drag.handle,
                        self.area_aspect_ratio,
                        document,
                    )
                };
                if drag.moves_pixels
                    && drag.index.is_none()
                    && drag.handle.is_none()
                    && target != initial
                {
                    let mut a = self.new_annotation(AnnotationKind::MoveArea, from_pos(target.min));
                    a.area_source = Some([from_pos(initial.min), from_pos(initial.max)]);
                    a.area_mask = drag.mask.clone();
                    a.pixel_mask = drag.pixel_mask.clone();
                    a.end = from_pos(target.max);
                    drag.index = Some(document.annotations.len());
                    document.annotations.push(a);
                }
                if let Some(index) = drag.index {
                    document.annotations[index].start = from_pos(target.min);
                    document.annotations[index].end = from_pos(target.max);
                    self.select(Some(index), document);
                } else {
                    self.clear_selection();
                }
                self.area = Some(target);
                self.area_mask = drag.mask.clone();
                self.pixel_mask = drag.pixel_mask.clone();
            } else {
                self.clear_selection();
                if self.lasso {
                    if drag.samples.len() < crate::document::MAX_STROKE_POINTS
                        && drag
                            .samples
                            .last()
                            .is_none_or(|p| point(*p).distance(point(at)) * self.zoom >= 1.)
                    {
                        drag.samples.push(at);
                    }
                    let bounds = drag.samples.iter().fold(Rect::NOTHING, |mut r, p| {
                        r.extend_with(point(*p));
                        r
                    });
                    if bounds.is_positive() {
                        let crop =
                            crop_between(from_pos(bounds.min), from_pos(bounds.max), document);
                        let rect = Rect::from_min_size(
                            egui::pos2(crop.x as f32, crop.y as f32),
                            egui::vec2(crop.width as f32, crop.height as f32),
                        );
                        self.area = Some(rect);
                        self.area_mask = drag
                            .samples
                            .iter()
                            .map(|p| {
                                from_pos(
                                    ((point(*p) - rect.min) / rect.size())
                                        .clamp(Vec2::ZERO, Vec2::splat(1.))
                                        .to_pos2(),
                                )
                            })
                            .collect();
                    }
                } else {
                    let crop = crop_between(drag.origin, at, document);
                    self.area = Some(Rect::from_min_size(
                        egui::pos2(crop.x as f32, crop.y as f32),
                        egui::vec2(crop.width as f32, crop.height as f32),
                    ));
                    self.area_mask.clear();
                }
            }
        }
        self.area_drag = Some(drag);
    }

    pub(super) fn finish_area_drag(&mut self, document: &mut Document, history: &mut EditHistory) {
        let Some(drag) = self.area_drag.take() else {
            return;
        };
        if !drag.travelled {
            // A click on a handle must not lose the selection. Ordinary clicks
            // still select annotation objects, as before.
            if drag.handle.is_some() {
                return;
            }
            let index = hit_document_annotation(document, drag.origin, self.zoom);
            if index.is_none() && drag.moving {
                return;
            }
            self.deselect_pixels();
            self.choose_annotation(index, false, false, document);
        } else {
            if let Some(index) = drag.index
                && index >= drag.before.annotations.len()
                && document.annotations[index].area_source
                    == Some([
                        document.annotations[index].start,
                        document.annotations[index].end,
                    ])
            {
                document.annotations.remove(index);
                self.clear_selection();
            }
            if !drag.moving && self.lasso && self.area_mask.len() < 3 {
                self.area = None;
                self.area_mask.clear();
            }
            if drag.index.is_none() {
                return;
            }
            let result = crate::document::validate_document(document).and_then(|()| {
                if drag.moves_content(document) {
                    // A move back to its original position may have removed the
                    // temporary layer above. In that case there is no growth.
                    if let Some(index) = drag.index.filter(|&i| i < document.annotations.len()) {
                        crate::canvas::extend_for_annotations(document, &[index])?;
                    }
                }
                Ok(())
            });
            if let Err(error) = result {
                document.annotations = drag.before.annotations;
                document.geometry = drag.before.geometry;
                self.clear_selection();
                self.area = None;
                self.image_error = Some(error);
            } else {
                history.commit(drag.before, document);
            }
        }
    }

    pub(super) fn selection_region(&self, document: &Document) -> Option<Rect> {
        let indices = self.selected_indices(document);
        if indices.is_empty() {
            return self.area;
        }
        let mut rect = Rect::NOTHING;
        for i in indices {
            rect = rect.union(render::annotation_bounds(&document.annotations[i]));
        }
        rect.is_positive().then_some(rect)
    }

    /// A marquee deletes pixels; an object/layer selection deletes its layers.
    pub(super) fn delete_current_selection(
        &mut self,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if self.area.is_some() {
            self.apply_area_action(AnnotationKind::DeleteArea, document, history);
        } else {
            self.delete(document, history);
        }
    }

    pub(super) fn apply_area_action(
        &mut self,
        kind: AnnotationKind,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        if !matches!(
            kind,
            AnnotationKind::Blur
                | AnnotationKind::Pixelate
                | AnnotationKind::DeleteArea
                | AnnotationKind::Magnify
                | AnnotationKind::Spotlight
                | AnnotationKind::Simplify
        ) {
            return;
        }
        if self.selected.is_some() && !self.can_edit_selection(document) {
            return;
        }
        self.finish_area_drag(document, history);
        let Some(rect) = self.selection_region(document) else {
            return;
        };
        let before = document.edit_state();
        let reuse = self.selected.filter(|&i| {
            self.additional.is_empty()
                && ((document.annotations[i].kind.is_effect() && kind.is_effect())
                    || (kind.is_studio() && document.annotations[i].kind == kind))
        });
        let index = if let Some(index) = reuse {
            document.annotations[index].kind = kind;
            index
        } else {
            let mut a = self.new_annotation(kind, from_pos(rect.min));
            a.end = from_pos(rect.max);
            a.area_mask = if self.selected.is_none() {
                self.area_mask.clone()
            } else {
                self.selected
                    .and_then(|i| document.annotations.get(i))
                    .filter(|a| a.kind.needs_composite())
                    .map(|a| a.area_mask.clone())
                    .unwrap_or_default()
            };
            a.pixel_mask = self
                .selected
                .and_then(|i| document.annotations.get(i))
                .map(|a| a.pixel_mask.clone())
                .unwrap_or_else(|| self.pixel_mask.clone());
            let index = document.annotations.len();
            document.annotations.push(a);
            index
        };
        if let Err(error) = crate::document::validate_document(document) {
            document.annotations = before.annotations;
            document.geometry = before.geometry;
            self.image_error = Some(error);
            return;
        }
        history.commit(before, document);
        if kind == AnnotationKind::DeleteArea {
            self.deselect_pixels();
        } else {
            self.select(Some(index), document);
            self.area = (!kind.is_studio()).then_some(rect);
        }
        self.tool = Tool::Area;
        self.focus_canvas = true;
    }

    pub(super) fn paint_area(
        &self,
        painter: &egui::Painter,
        document: &Document,
        origin: Pos2,
        scale: f32,
    ) {
        if self.area.is_none()
            && !matches!(
                self.tool,
                Tool::Area | Tool::Fill | Tool::Wand | Tool::CutOut
            )
        {
            return;
        }
        let region = if self.tool == Tool::Fill {
            self.area
        } else {
            self.area_bounds(document)
        };
        if let Some(rect) = region {
            let bounds = mapped_bounds(rect, document);
            let screen = Rect::from_min_max(
                origin + bounds.min.to_vec2() * scale,
                origin + bounds.max.to_vec2() * scale,
            );
            for (_, position) in AreaHandle::positions(screen) {
                if self.tool == Tool::Area {
                    let handle = Rect::from_center_size(position, Vec2::splat(6.));
                    painter.rect_filled(handle, 1, Color32::WHITE);
                    painter.rect_stroke(
                        handle,
                        1,
                        Stroke::new(1., crate::appearance::palette(painter.ctx()).accent),
                        StrokeKind::Inside,
                    );
                }
            }
            if self.tool == Tool::CutOut {
                let crop = document.geometry.crop_rect(original_size(document));
                let axis = usize::from(!self.extra_tools.cut_vertical)
                    ^ usize::from(document.geometry.quarter_turns % 2 == 1);
                let strip = if axis == 0 {
                    Rect::from_min_max(
                        egui::pos2(rect.left(), crop.y as f32),
                        egui::pos2(rect.right(), (crop.y + crop.height) as f32),
                    )
                } else {
                    Rect::from_min_max(
                        egui::pos2(crop.x as f32, rect.top()),
                        egui::pos2((crop.x + crop.width) as f32, rect.bottom()),
                    )
                };
                let bounds = mapped_bounds(strip, document);
                let screen = Rect::from_min_max(
                    origin + bounds.min.to_vec2() * scale,
                    origin + bounds.max.to_vec2() * scale,
                );
                painter.rect_filled(screen, 0, Color32::from_rgba_unmultiplied(235, 70, 60, 80));
                painter.rect_stroke(
                    screen,
                    0,
                    Stroke::new(1.5, Color32::from_rgb(235, 70, 60)),
                    StrokeKind::Inside,
                );
                return;
            }
            let pixel_mask = self
                .selected
                .and_then(|i| document.annotations.get(i))
                .and_then(|a| a.pixel_mask.as_ref())
                .or(self.pixel_mask.as_ref());
            if let Some(mask) = pixel_mask {
                editor_tools::paint_pixel_mask(painter, mask, rect, document, origin, scale);
            }
            let mask = if self.tool == Tool::Fill {
                self.area_mask.as_slice()
            } else {
                self.selected
                    .and_then(|i| document.annotations.get(i))
                    .map(|a| a.area_mask.as_slice())
                    .unwrap_or(&self.area_mask)
            };
            if !mask.is_empty() {
                let points: Vec<_> = mask
                    .iter()
                    .map(|p| {
                        origin
                            + output_point(
                                from_pos(rect.min + egui::vec2(p.x, p.y) * rect.size()),
                                document,
                            )
                            .to_vec2()
                                * scale
                    })
                    .collect();
                paint_selection_outline(painter, points);
                return;
            }
            // Keep the complete outline inside the clipped canvas, including
            // Select All at Fit and 100%. Dashes remain the same size at any zoom.
            let screen = screen.shrink(1.5_f32.min(screen.width().min(screen.height()) * 0.25));
            paint_selection_outline(
                painter,
                vec![
                    screen.left_top(),
                    screen.right_top(),
                    screen.right_bottom(),
                    screen.left_bottom(),
                ],
            );
        }
    }

    pub(super) fn area_keyboard(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) -> bool {
        if self.gesture.is_some() || !self.additional.is_empty() {
            return false;
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            if let Some(drag) = self.area_drag.take() {
                document.annotations = drag.before.annotations;
                document.geometry = drag.before.geometry;
                self.set_selection(drag.selection_before, document);
                self.area = drag.area_before;
                self.area_mask = drag.mask;
                self.pixel_mask = drag.pixel_mask;
                self.move_pixels = drag.move_pixels_before;
                self.area_aspect_ratio = drag.aspect_before;
                return true;
            }
            self.deselect_pixels();
            return true;
        }
        if ui.input(|i| i.key_pressed(Key::Z) && i.modifiers.command) {
            self.area = None;
        }
        if self.area_bounds(document).is_none() {
            return false;
        }
        if ui.input_mut(|i| {
            i.consume_key(Modifiers::NONE, Key::Delete)
                || i.consume_key(Modifiers::NONE, Key::Backspace)
        }) {
            self.delete_current_selection(document, history);
            return true;
        }
        false
    }
}

/// Adjust on the displayed pixel grid, then map back to source coordinates.
/// This keeps edges, cursors and movement consistent after crop/rotation/resize
/// and when cut-out strips have compacted the visible canvas.
fn adjusted_area(
    initial: Rect,
    origin: Point,
    at: Point,
    handle: Option<AreaHandle>,
    ratio: Option<f64>,
    document: &Document,
) -> Rect {
    let [w, h] = document.output_size();
    let extent = Rect::from_min_size(Pos2::ZERO, egui::vec2(w as f32, h as f32));
    let mut target = crate::image_clipboard::copy_bounds(document, initial)
        .map(selection_size::output_rect)
        .unwrap_or_else(|_| mapped_bounds(initial, document).intersect(extent));
    if !target.is_positive() {
        return initial;
    }
    let delta = output_point(at, document) - output_point(origin, document);
    if let (Some(handle), Some(ratio)) = (handle, ratio) {
        target = selection_size::locked_drag(target, delta, handle, ratio, [w, h]);
        return selection_size::source_rect(target, document).unwrap_or(initial);
    }
    if let Some(handle) = handle {
        let min = Vec2::splat(1.);
        if matches!(
            handle,
            AreaHandle::Left | AreaHandle::TopLeft | AreaHandle::BottomLeft
        ) {
            target.min.x = (target.left() + delta.x).clamp(0., (target.right() - min.x).max(0.));
        }
        if matches!(
            handle,
            AreaHandle::Right | AreaHandle::TopRight | AreaHandle::BottomRight
        ) {
            target.max.x =
                (target.right() + delta.x).clamp((target.left() + min.x).min(w as f32), w as f32);
        }
        if matches!(
            handle,
            AreaHandle::Top | AreaHandle::TopLeft | AreaHandle::TopRight
        ) {
            target.min.y = (target.top() + delta.y).clamp(0., (target.bottom() - min.y).max(0.));
        }
        if matches!(
            handle,
            AreaHandle::Bottom | AreaHandle::BottomLeft | AreaHandle::BottomRight
        ) {
            target.max.y =
                (target.bottom() + delta.y).clamp((target.top() + min.y).min(h as f32), h as f32);
        }
    } else {
        target = target.translate(egui::vec2(
            delta.x.clamp(-target.left(), w as f32 - target.right()),
            delta.y.clamp(-target.top(), h as f32 - target.bottom()),
        ));
    }
    target = Rect::from_min_max(target.min.round(), target.max.round());
    selection_size::source_rect(target, document).unwrap_or(initial)
}

fn paint_selection_outline(painter: &egui::Painter, mut points: Vec<Pos2>) {
    if let Some(&first) = points.first() {
        points.push(first);
        painter.add(egui::Shape::line(
            points.clone(),
            Stroke::new(2., Color32::WHITE),
        ));
        painter.extend(egui::Shape::dashed_line(
            &points,
            Stroke::new(1.5, Color32::from_rgb(24, 32, 44)),
            5.,
            5.,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn doc() -> Document {
        Document {
            image: std::sync::Arc::new(image::RgbaImage::from_fn(100, 100, |x, y| {
                image::Rgba([x as u8, y as u8, 80, 255])
            })),
            annotations: vec![],
            geometry: Default::default(),
        }
    }
    fn rectangle() -> Rect {
        Rect::from_min_max(egui::pos2(20., 20.), egui::pos2(60., 60.))
    }

    #[test]
    fn deleting_pixels_clears_selection_and_keeps_an_editable_removal_layer() {
        let mut d = doc();
        let source = d.image.clone();
        let mut e = EditorState {
            tool: Tool::Area,
            area: Some(rectangle()),
            area_aspect_ratio: Some(1.),
            ..Default::default()
        };
        let mut h = EditHistory::new(d.edit_state());
        e.delete_current_selection(&mut d, &mut h);
        assert!(e.area.is_none() && e.selected.is_none());
        assert!(e.area_mask.is_empty() && e.pixel_mask.is_none());
        assert!(e.area_aspect_ratio.is_none());
        assert_eq!(d.annotations.len(), 1);
        assert_eq!(d.annotations[0].kind, AnnotationKind::DeleteArea);
        assert!(std::sync::Arc::ptr_eq(&source, &d.image));
        let deleted = d.edit_state();
        e.delete_current_selection(&mut d, &mut h);
        assert_eq!(
            d.edit_state(),
            deleted,
            "A second Delete has no stale selection"
        );
        let flat = render::flatten(&d).unwrap();
        assert_eq!(flat.get_pixel(30, 30)[3], 0);
        assert_eq!(flat.get_pixel(10, 10), source.get_pixel(10, 10));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deleted.myimage");
        crate::document::save_project(&d, &path).unwrap();
        let reopened = crate::document::load_project(&path).unwrap();
        assert_eq!(reopened.edit_state(), deleted);
        assert_eq!(render::flatten(&reopened).unwrap(), flat);
        assert!(h.undo(&mut d));
        assert!(d.annotations.is_empty());
        assert!(!h.can_undo());
        assert!(h.redo(&mut d));
        // Layers above the removal remain visible, and hiding the removal restores pixels.
        let mut above = e.new_annotation(AnnotationKind::Rectangle, Point { x: 25., y: 25. });
        above.end = Point { x: 45., y: 45. };
        above.fill = true;
        above.color = [255, 0, 0, 255];
        d.annotations.push(above);
        assert_eq!(
            render::flatten(&d).unwrap().get_pixel(35, 35).0,
            [255, 0, 0, 255]
        );
        d.annotations[0].layer.hidden = true;
        assert_eq!(
            render::flatten(&d).unwrap().get_pixel(50, 50),
            source.get_pixel(50, 50)
        );
    }

    #[test]
    fn deleting_selected_layers_respects_groups_and_locks_without_erasing_underneath() {
        let mut d = doc();
        let mut e = EditorState::default();
        for kind in [AnnotationKind::Blur, AnnotationKind::Rectangle] {
            let mut a = e.new_annotation(kind, Point { x: 20., y: 20. });
            a.end = Point { x: 60., y: 60. };
            a.group_id = Some(1);
            d.annotations.push(a);
        }
        let mut h = EditHistory::new(d.edit_state());
        e.choose_annotation(Some(0), false, false, &d);
        d.annotations[0].group_layer.locked = true;
        d.annotations[1].group_layer.locked = true;
        let before = d.edit_state();
        e.delete_current_selection(&mut d, &mut h);
        assert_eq!(d.edit_state(), before);
        assert_eq!(e.selected_indices(&d), vec![0, 1]);
        for a in &mut d.annotations {
            a.group_layer.locked = false;
        }
        e.delete_current_selection(&mut d, &mut h);
        assert!(d.annotations.is_empty());
        assert!(e.area.is_none() && e.selected_indices(&d).is_empty());
        assert_eq!(render::flatten(&d).unwrap(), *d.image);
        assert!(h.undo(&mut d));
        assert_eq!(d.annotations.len(), 2);
    }

    #[test]
    fn edge_crossing_keeps_stationary_preview_pixels_on_a_fixed_sampling_grid() {
        let mut d = Document {
            image: std::sync::Arc::new(image::RgbaImage::from_fn(1600, 1000, |x, y| {
                image::Rgba([
                    (x % 251) as u8,
                    (y % 251) as u8,
                    if x % 2 == 0 { 0 } else { 255 },
                    255,
                ])
            })),
            annotations: vec![],
            geometry: Default::default(),
        };
        let mut e = EditorState {
            tool: Tool::Area,
            area: Some(Rect::from_min_max(
                egui::pos2(1300., 300.),
                egui::pos2(1500., 500.),
            )),
            ..Default::default()
        };
        e.begin_area_drag(Point { x: 1400., y: 400. }, &mut d);
        let rect = Rect::from_min_size(egui::pos2(100., 150.), egui::vec2(800., 500.));
        let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(1000., 800.));
        let mut baseline: Option<(ImageGeometry, Rect, image::RgbaImage)> = None;
        for dx in [1., 90., 99., 100., 101., 120., 200., 90., 0.] {
            e.update_area_drag(
                Point {
                    x: 1400. + dx,
                    y: 400.,
                },
                &mut d,
            );
            let (frame, screen) = e
                .area_drag_texture_frame(&d, rect, 0.5, viewport)
                .unwrap()
                .unwrap();
            let pixels = render::interactive_preview(&frame, 768).unwrap();
            if let Some((geometry, first_screen, first)) = &baseline {
                assert_eq!(frame.geometry, *geometry);
                assert_eq!(screen, *first_screen);
                assert_eq!(pixels.dimensions(), first.dimensions());
                for (x, y, p) in pixels
                    .enumerate_pixels()
                    .filter(|(x, _, _)| *x < pixels.width() / 2)
                {
                    assert_eq!(
                        p,
                        first.get_pixel(x, y),
                        "Stationary pixels must not shimmer at {dx}"
                    );
                }
            } else {
                assert!(pixels.pixels().any(|p| p[3] > 0));
                baseline = Some((frame.geometry, screen, pixels));
            }
        }
    }

    #[test]
    fn pixel_drags_extend_every_edge_and_corner_with_transparency_and_one_undo() {
        for (dx, dy) in [
            (90., 0.),
            (-90., 0.),
            (0., 90.),
            (0., -90.),
            (-90., -90.),
            (90., 90.),
        ] {
            for masked in [false, true] {
                let mut d = doc();
                let original = d.clone();
                let before = d.edit_state();
                let mut h = EditHistory::new(before.clone());
                let mut e = EditorState {
                    tool: Tool::Area,
                    area: Some(rectangle()),
                    ..Default::default()
                };
                if masked {
                    e.area_mask = vec![
                        Point { x: 0., y: 0. },
                        Point { x: 1., y: 0. },
                        Point { x: 0., y: 1. },
                    ];
                }
                e.begin_area_drag(Point { x: 30., y: 30. }, &mut d);
                e.update_area_drag(
                    Point {
                        x: 30. + dx,
                        y: 30. + dy,
                    },
                    &mut d,
                );
                assert!(
                    d.geometry.canvas.is_none(),
                    "Keep the pointer mapping stable until release"
                );
                let during = d.edit_state();
                let (preview, screen) = e
                    .area_drag_preview(
                        &d,
                        Rect::from_min_size(egui::pos2(200., 200.), Vec2::splat(100.)),
                        1.,
                    )
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    d.edit_state(),
                    during,
                    "Preview must not mutate the document"
                );
                assert!(!h.can_undo(), "No history entry until release");
                assert!(std::sync::Arc::ptr_eq(&preview.image, &d.image));
                let preview_pixels = render::flatten(&preview).unwrap();
                assert!(screen.contains(egui::pos2(230. + dx, 230. + dy)));
                assert_eq!(
                    render::interactive_preview(&preview, 768).unwrap(),
                    render::privacy_preview(&preview, 768).unwrap()
                );
                e.finish_area_drag(&mut d, &mut h);
                assert!(e.image_error.is_none());
                assert_eq!(d.edit_state(), preview.edit_state());
                assert_eq!(render::flatten(&d).unwrap(), preview_pixels);
                assert!(e.area_drag_preview(&d, screen, 1.).unwrap().is_none());
                let bounds = d.geometry.canvas.unwrap();
                assert_eq!(bounds.x, (20. + dx).min(0.) as i32);
                assert_eq!(bounds.y, (20. + dy).min(0.) as i32);
                assert_eq!(
                    bounds.width,
                    ((60. + dx).max(100.) - bounds.x as f32) as u32
                );
                assert_eq!(
                    bounds.height,
                    ((60. + dy).max(100.) - bounds.y as f32) as u32
                );
                let pixels = render::flatten(&d).unwrap();
                let at =
                    |x: f32, y: f32| ((x - bounds.x as f32) as u32, (y - bounds.y as f32) as u32);
                let (x, y) = at(30., 30.);
                assert_eq!(pixels.get_pixel(x, y)[3], 0);
                let (x, y) = at(30. + dx, 30. + dy);
                assert_eq!(pixels.get_pixel(x, y), original.image.get_pixel(30, 30));
                let copied = crate::image_clipboard::copy_pixels(
                    &d,
                    Some((e.area.unwrap(), e.area_mask.clone())),
                )
                .unwrap();
                assert_eq!(copied.dimensions(), (40, 40));
                assert_eq!(copied.get_pixel(10, 10), original.image.get_pixel(30, 30));
                if masked {
                    assert_eq!(copied.get_pixel(39, 39)[3], 0);
                }
                assert_eq!(
                    crate::render::interactive_preview(&d, 768).unwrap(),
                    crate::render::privacy_preview(&d, 768).unwrap()
                );
                let moved = d.edit_state();
                assert!(h.undo(&mut d));
                assert_eq!(d.edit_state(), before);
                assert!(!h.can_undo());
                assert!(h.redo(&mut d));
                assert_eq!(d.edit_state(), moved);
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("expanded.myimage");
                crate::document::save_project(&d, &path).unwrap();
                let loaded = crate::document::load_project(&path).unwrap();
                assert_eq!(loaded.image, original.image);
                assert_eq!(render::flatten(&loaded).unwrap(), pixels);
            }
        }
    }

    #[test]
    fn live_expansion_preserves_screen_mapping_with_cutouts_and_returns_to_original_bounds() {
        for turns in 0..4 {
            for flips in [(false, false), (true, false), (false, true), (true, true)] {
                let mut d = doc();
                d.geometry.quarter_turns = turns;
                d.geometry.flip_horizontal = flips.0;
                d.geometry.flip_vertical = flips.1;
                d.geometry.resize = Some([137, 199]);
                d.geometry.cutouts[0] = Some(crate::geometry::Cutout {
                    axis: 0,
                    start: 35,
                    end: 45,
                });
                d.geometry.cutouts[1] = Some(crate::geometry::Cutout {
                    axis: 1,
                    start: 40,
                    end: 50,
                });
                let original = d.edit_state();
                let mut e = EditorState {
                    tool: Tool::Area,
                    area: Some(Rect::from_min_max(
                        egui::pos2(60., 60.),
                        egui::pos2(80., 80.),
                    )),
                    ..Default::default()
                };
                let start = Point { x: 70., y: 70. };
                let rect =
                    Rect::from_min_size(egui::pos2(250., 200.), egui::vec2(137., 199.) * 0.7);
                e.begin_area_drag(start, &mut d);
                let destination = d.geometry.inverse_point(
                    from_pos(output_point(start, &d) + egui::vec2(-230., 220.)),
                    [100, 100],
                );
                e.update_area_drag(destination, &mut d);
                let (preview, screen) = e.area_drag_preview(&d, rect, 0.7).unwrap().unwrap();
                for p in [
                    start,
                    destination,
                    Point { x: 15., y: 25. },
                    Point { x: 85., y: 95. },
                ] {
                    let a = rect.min + output_point(p, &d).to_vec2() * 0.7;
                    let size = preview.output_size();
                    let b = screen.min
                        + output_point(p, &preview).to_vec2() * screen.size()
                            / egui::vec2(size[0] as f32, size[1] as f32);
                    assert!((a - b).length() < 0.002);
                }
                // A preview is not cumulative: returning inside removes the margin.
                e.update_area_drag(start, &mut d);
                assert!(e.area_drag_preview(&d, rect, 0.7).unwrap().is_none());
                let mut h = EditHistory::new(original.clone());
                e.finish_area_drag(&mut d, &mut h);
                assert_eq!(d.edit_state(), original);
                assert!(!h.can_undo());
            }
        }
    }

    #[test]
    fn outward_moves_follow_rotation_flips_resize_and_existing_canvas() {
        for turns in 0..4 {
            for flip in [false, true] {
                for delta in [
                    egui::vec2(150., 0.),
                    egui::vec2(-150., 0.),
                    egui::vec2(0., 150.),
                    egui::vec2(0., -150.),
                ] {
                    let mut d = doc();
                    d.geometry.crop = Some(CropRect {
                        x: 10,
                        y: 10,
                        width: 80,
                        height: 80,
                    });
                    d.geometry.quarter_turns = turns;
                    d.geometry.flip_horizontal = flip;
                    d.geometry.resize = Some([160, 240]);
                    let mut e = EditorState {
                        tool: Tool::Area,
                        area: Some(rectangle()),
                        ..Default::default()
                    };
                    let mut h = EditHistory::new(d.edit_state());
                    for _ in 0..2 {
                        let before = d.edit_state();
                        let source_rect = e.area.unwrap();
                        let start = from_pos(source_rect.center());
                        let at = output_point(start, &d) + delta;
                        let at = d.geometry.inverse_point(from_pos(at), original_size(&d));
                        e.begin_area_drag(start, &mut d);
                        e.update_area_drag(at, &mut d);
                        let moved = e.area.unwrap();
                        assert!((moved.size() - source_rect.size()).length() < 0.001);
                        assert!(
                            (output_point(from_pos(moved.center()), &d)
                                - output_point(start, &d)
                                - delta)
                                .length()
                                < 0.001
                        );
                        let origin = egui::pos2(300., 280.);
                        for zoom in [0.35, 1., 2.] {
                            let size = d.output_size();
                            let rect = Rect::from_min_size(
                                origin,
                                egui::vec2(size[0] as f32, size[1] as f32) * zoom,
                            );
                            let (preview, screen) =
                                e.area_drag_preview(&d, rect, zoom).unwrap().unwrap();
                            let size = preview.output_size();
                            let factor = screen.size() / egui::vec2(size[0] as f32, size[1] as f32);
                            for p in [start, from_pos(moved.center()), Point { x: 72., y: 64. }] {
                                let old_screen = origin + output_point(p, &d).to_vec2() * zoom;
                                let new_screen =
                                    screen.min + output_point(p, &preview).to_vec2() * factor;
                                assert!(
                                    (old_screen - new_screen).length() < 0.002,
                                    "Expanded render must not shift existing pixels or the pointer"
                                );
                            }
                        }
                        e.finish_area_drag(&mut d, &mut h);
                        assert!(e.image_error.is_none());
                        let b = d.geometry.canvas_rect(original_size(&d));
                        assert!(
                            Rect::from_min_size(
                                egui::pos2(b.x as f32, b.y as f32),
                                egui::vec2(b.width as f32, b.height as f32)
                            )
                            .expand(0.001)
                            .contains_rect(moved)
                        );
                        assert_eq!(
                            d.annotations.len(),
                            1,
                            "Repeat dragging repositions the existing move"
                        );
                        let after = d.edit_state();
                        assert!(h.undo(&mut d));
                        assert_eq!(d.edit_state(), before);
                        assert!(h.redo(&mut d));
                        assert_eq!(d.edit_state(), after);
                    }
                }
            }
        }
    }

    #[test]
    fn oversized_expansion_and_dragging_back_restore_original_without_history() {
        let mut d = doc();
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState {
            tool: Tool::Area,
            area: Some(rectangle()),
            ..Default::default()
        };
        e.begin_area_drag(Point { x: 40., y: 40. }, &mut d);
        e.update_area_drag(Point { x: 300., y: 300. }, &mut d);
        e.update_area_drag(Point { x: 40., y: 40. }, &mut d);
        e.finish_area_drag(&mut d, &mut h);
        assert_eq!(d.edit_state(), before);
        assert!(!h.can_undo());
        e.begin_area_drag(Point { x: 40., y: 40. }, &mut d);
        e.update_area_drag(
            Point {
                x: 100_000.,
                y: 100_000.,
            },
            &mut d,
        );
        e.finish_area_drag(&mut d, &mut h);
        assert!(e.image_error.is_some());
        assert_eq!(d.edit_state(), before);
        assert!(!h.can_undo());
    }

    #[test]
    fn default_drag_moves_masked_pixels_repeatedly_and_handles_only_resize_boundary() {
        for lasso in [false, true] {
            let mut d = doc();
            let original = d.clone();
            let mut e = EditorState {
                tool: Tool::Area,
                area: Some(rectangle()),
                ..Default::default()
            };
            if lasso {
                e.area_mask = vec![
                    Point { x: 0., y: 0. },
                    Point { x: 1., y: 0. },
                    Point { x: 0., y: 1. },
                ];
            }
            let mut h = EditHistory::new(d.edit_state());
            e.begin_area_drag(Point { x: 30., y: 30. }, &mut d);
            e.update_area_drag(Point { x: 40., y: 40. }, &mut d);
            e.finish_area_drag(&mut d, &mut h);
            assert_eq!(d.annotations.len(), 1);
            assert_eq!(d.annotations[0].kind, AnnotationKind::MoveArea);
            let flat = render::flatten(&d).unwrap();
            assert_eq!(flat.get_pixel(22, 22)[3], 0);
            assert_eq!(flat.get_pixel(32, 32), original.image.get_pixel(22, 22));
            if lasso {
                assert_eq!(flat.get_pixel(59, 59), original.image.get_pixel(59, 59));
            }
            e.begin_area_drag(Point { x: 40., y: 40. }, &mut d);
            e.update_area_drag(Point { x: 45., y: 45. }, &mut d);
            e.finish_area_drag(&mut d, &mut h);
            assert_eq!(d.annotations.len(), 1, "Reposition the same move layer");
            assert_eq!(
                render::flatten(&d).unwrap().get_pixel(37, 37),
                original.image.get_pixel(22, 22)
            );
            let moved = d.edit_state();
            let corner = from_pos(e.area.unwrap().right_bottom());
            e.begin_area_drag(corner, &mut d);
            e.update_area_drag(Point { x: 90., y: 90. }, &mut d);
            e.finish_area_drag(&mut d, &mut h);
            assert_eq!(
                d.edit_state(),
                moved,
                "Boundary resize must not stretch lifted pixels"
            );
            assert_eq!(e.area.unwrap().right_bottom(), egui::pos2(90., 90.));
            assert!(h.undo(&mut d));
            assert!(h.undo(&mut d));
            assert_eq!(d.edit_state(), original.edit_state());
            assert!(!h.can_undo());
            assert!(h.redo(&mut d));
            assert!(h.redo(&mut d));
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("moved.myimage");
            crate::document::save_project(&d, &path).unwrap();
            let reopened = crate::document::load_project(&path).unwrap();
            assert_eq!(reopened.edit_state(), moved);
            assert_eq!(reopened.image, original.image);
        }
    }

    #[test]
    fn moving_boundary_changes_copy_scope_without_moving_pixels_or_undo_history() {
        let mut d = doc();
        d.annotations.push(crate::markup_tests::object(
            AnnotationKind::Arrow,
            [5., 5.],
            [80., 80.],
        ));
        let before = d.edit_state();
        let pixels = render::flatten(&d).unwrap();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState {
            tool: Tool::Area,
            area: Some(rectangle()),
            move_pixels: false,
            ..Default::default()
        };
        e.begin_area_drag(Point { x: 40., y: 40. }, &mut d);
        e.update_area_drag(Point { x: 50., y: 45. }, &mut d);
        e.finish_area_drag(&mut d, &mut h);
        assert_eq!(e.area, Some(rectangle().translate(egui::vec2(10., 5.))));
        assert_eq!(d.edit_state(), before);
        assert_eq!(render::flatten(&d).unwrap(), pixels);
        assert!(!h.can_undo());
        assert_eq!(
            crate::image_clipboard::copy_pixels(&d, Some((e.area.unwrap(), vec![]))).unwrap(),
            image::imageops::crop_imm(&pixels, 30, 25, 40, 40).to_image()
        );
        // Movement clamps without shrinking the boundary at the image edge.
        e.begin_area_drag(Point { x: 50., y: 45. }, &mut d);
        e.update_area_drag(Point { x: 500., y: 500. }, &mut d);
        e.finish_area_drag(&mut d, &mut h);
        assert_eq!(
            e.area.unwrap(),
            Rect::from_min_max(egui::pos2(60., 60.), egui::pos2(100., 100.))
        );
        assert_eq!(d.edit_state(), before);
    }

    #[test]
    fn all_eight_handles_follow_display_orientation_and_keep_opposite_edges_fixed() {
        for rotation in 0..4 {
            for zoom in [0.25, 1., 3.] {
                let mut d = doc();
                d.geometry.crop = Some(CropRect {
                    x: 10,
                    y: 10,
                    width: 80,
                    height: 80,
                });
                d.geometry.quarter_turns = rotation;
                d.geometry.resize = Some([160, 240]);
                let before = d.edit_state();
                let mut h = EditHistory::new(before.clone());
                let initial = mapped_bounds(rectangle(), &d);
                for (handle, start) in AreaHandle::positions(initial) {
                    let mut e = EditorState {
                        tool: Tool::Area,
                        area: Some(rectangle()),
                        zoom,
                        ..Default::default()
                    };
                    let source = |p: Pos2| d.geometry.inverse_point(from_pos(p), original_size(&d));
                    let a = source(start);
                    let b = source(start + egui::vec2(16., 18.));
                    assert_eq!(e.area_handle(a, &d), Some(handle));
                    assert_eq!(e.area_cursor(a, &d), handle.cursor());
                    e.begin_area_drag(a, &mut d);
                    e.update_area_drag(b, &mut d);
                    e.finish_area_drag(&mut d, &mut h);
                    let actual = mapped_bounds(e.area.unwrap(), &d);
                    let left = matches!(
                        handle,
                        AreaHandle::Left | AreaHandle::TopLeft | AreaHandle::BottomLeft
                    );
                    let right = matches!(
                        handle,
                        AreaHandle::Right | AreaHandle::TopRight | AreaHandle::BottomRight
                    );
                    let top = matches!(
                        handle,
                        AreaHandle::Top | AreaHandle::TopLeft | AreaHandle::TopRight
                    );
                    let bottom = matches!(
                        handle,
                        AreaHandle::Bottom | AreaHandle::BottomLeft | AreaHandle::BottomRight
                    );
                    assert_eq!(
                        actual.min,
                        initial.min
                            + egui::vec2(if left { 16. } else { 0. }, if top { 18. } else { 0. })
                    );
                    assert_eq!(
                        actual.max,
                        initial.max
                            + egui::vec2(
                                if right { 16. } else { 0. },
                                if bottom { 18. } else { 0. }
                            )
                    );
                    assert_eq!(d.edit_state(), before);
                    assert!(!h.can_undo());
                }
            }
        }
    }

    #[test]
    fn edges_hit_between_handles_and_cannot_collapse_or_cross_the_opposite_edge() {
        let mut d = doc();
        let mut h = EditHistory::new(d.edit_state());
        let mut e = EditorState {
            tool: Tool::Area,
            area: Some(rectangle()),
            ..Default::default()
        };
        // Grab three pixels off the top edge, away from any handle. No initial jump.
        let at = Point { x: 32., y: 23. };
        assert_eq!(e.area_handle(at, &d), Some(AreaHandle::Top));
        e.begin_area_drag(at, &mut d);
        e.update_area_drag(Point { x: 50., y: 33. }, &mut d);
        assert_eq!(e.area.unwrap().min, egui::pos2(20., 30.));
        assert_eq!(e.area.unwrap().max, egui::pos2(60., 60.));
        e.update_area_drag(Point { x: 50., y: 500. }, &mut d);
        e.finish_area_drag(&mut d, &mut h);
        assert_eq!(e.area.unwrap().height(), 1.);
        assert_eq!(e.area.unwrap().bottom(), 60.);
        assert!(!h.can_undo());
        let small = Rect::from_min_size(egui::pos2(5., 5.), Vec2::splat(10.));
        assert!(
            AreaHandle::hit(small, small.center(), 1.).is_none(),
            "small selections still have a movable interior"
        );
    }

    #[test]
    fn masks_and_existing_effects_can_be_adjusted_as_selection_only() {
        let mut d = doc();
        let mask = vec![
            Point { x: 0., y: 0. },
            Point { x: 1., y: 0. },
            Point { x: 0., y: 1. },
        ];
        let pixels = crate::fill::FillRegion {
            seed: Point { x: 0.1, y: 0.1 },
            tolerance: 0,
            global: true,
            width: 40,
            height: 40,
            runs: std::sync::Arc::new((0..40).map(|y| [y, 0, 20]).collect()),
        };
        let mut e = EditorState {
            tool: Tool::Area,
            area: Some(rectangle()),
            area_mask: mask.clone(),
            pixel_mask: Some(pixels.clone()),
            ..Default::default()
        };
        let mut h = EditHistory::new(d.edit_state());
        e.apply_area_action(AnnotationKind::Blur, &mut d, &mut h);
        let before = d.edit_state();
        e.set_area_move_mode(false, &d);
        assert!(e.selected.is_none());
        e.begin_area_drag(Point { x: 30., y: 30. }, &mut d);
        e.update_area_drag(Point { x: 40., y: 40. }, &mut d);
        e.finish_area_drag(&mut d, &mut h);
        e.begin_area_drag(from_pos(e.area.unwrap().right_bottom()), &mut d);
        e.update_area_drag(Point { x: 80., y: 90. }, &mut d);
        e.finish_area_drag(&mut d, &mut h);
        assert_eq!(e.area_mask, mask);
        assert_eq!(e.pixel_mask, Some(pixels));
        assert_eq!(d.edit_state(), before);
        assert!(h.undo(&mut d)); // Only the original blur was committed.
        assert!(d.annotations.is_empty());
        assert!(!h.can_undo());
    }

    #[test]
    fn escape_restores_the_previous_boundary_or_pixel_edit_during_a_drag() {
        for pixels in [false, true] {
            let mut d = doc();
            let before = d.edit_state();
            let mut h = EditHistory::new(before.clone());
            let mut e = EditorState {
                tool: Tool::Area,
                area: Some(rectangle()),
                move_pixels: pixels,
                area_aspect_ratio: Some(1.),
                ..Default::default()
            };
            e.begin_area_drag(Point { x: 40., y: 40. }, &mut d);
            e.update_area_drag(Point { x: 250., y: 250. }, &mut d);
            let ctx = egui::Context::default();
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: Key::Escape,
                        physical_key: Some(Key::Escape),
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        assert!(e.area_keyboard(ui, &mut d, &mut h));
                    });
                },
            );
            assert!(e.area_drag.is_none());
            assert_eq!(e.area, Some(rectangle()));
            assert_eq!(e.area_aspect_ratio, Some(1.));
            assert!(e.selected.is_none());
            assert_eq!(d.edit_state(), before);
            assert!(!h.can_undo());
            e.deselect_pixels();
            assert!(e.move_pixels);
            assert!(e.area_aspect_ratio.is_none());
        }
    }

    #[test]
    fn moving_boundary_across_cutouts_preserves_visible_size() {
        let mut d = doc();
        d.geometry.cutouts[0] = Some(crate::geometry::Cutout {
            axis: 0,
            start: 35,
            end: 50,
        });
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let area = Rect::from_min_max(egui::pos2(5., 20.), egui::pos2(30., 60.));
        let mut e = EditorState {
            tool: Tool::Area,
            area: Some(area),
            move_pixels: false,
            ..Default::default()
        };
        e.begin_area_drag(Point { x: 17., y: 40. }, &mut d);
        e.update_area_drag(Point { x: 60., y: 45. }, &mut d);
        e.finish_area_drag(&mut d, &mut h);
        assert_eq!(mapped_bounds(e.area.unwrap(), &d).size(), area.size());
        assert_eq!(d.edit_state(), before);
        assert!(!h.can_undo());
    }
    #[test]
    fn select_all_copies_visible_canvas_under_every_rotation_without_editing_project() {
        for rotation in 0..4 {
            let mut d = doc();
            d.geometry.crop = Some(CropRect {
                x: 10,
                y: 20,
                width: 80,
                height: 70,
            });
            d.geometry.cutouts[0] = Some(crate::geometry::Cutout {
                axis: 0,
                start: 30,
                end: 40,
            });
            d.geometry.quarter_turns = rotation;
            d.geometry.resize = Some([113, 87]);
            d.annotations.push(crate::markup_tests::object(
                AnnotationKind::Rectangle,
                [20., 30.],
                [60., 60.],
            ));
            let before = d.edit_state();
            let mut h = EditHistory::new(before.clone());
            let mut e = EditorState::default();
            e.set_selection(vec![0], &d);
            e.area_mask = vec![
                Point { x: 0., y: 0. },
                Point { x: 1., y: 0. },
                Point { x: 0., y: 1. },
            ];
            e.select_all_image(&mut d, &mut h);
            assert!(e.selected_indices(&d).is_empty());
            assert!(e.area_mask.is_empty());
            assert!(e.pixel_mask.is_none());
            assert_eq!(e.copy_scope(&d), "Selected: 113 × 87 px");
            let pixels = crate::image_clipboard::copy_pixels(
                &d,
                Some((e.area.unwrap(), e.area_mask.clone())),
            )
            .unwrap();
            assert_eq!(pixels, render::flatten(&d).unwrap());
            assert_eq!(d.edit_state(), before);
            assert!(!h.can_undo());
            e.deselect_pixels();
            assert!(e.area.is_none());
            assert_eq!(e.copy_scope(&d), "Copy: whole image");
        }
    }

    #[test]
    fn select_all_boundary_stays_inside_canvas_and_out_of_copied_pixels() {
        let mut d = doc();
        let before = render::flatten(&d).unwrap();
        let mut h = EditHistory::new(d.edit_state());
        let mut e = EditorState::default();
        e.select_all_image(&mut d, &mut h);
        let ctx = egui::Context::default();
        for scale in [0.2, 1., 4.] {
            let canvas = Rect::from_min_size(egui::pos2(20., 20.), Vec2::splat(100. * scale));
            let output = ctx.run(Default::default(), |ctx| {
                let painter = ctx
                    .layer_painter(egui::LayerId::background())
                    .with_clip_rect(canvas);
                e.paint_area(&painter, &d, canvas.min, scale);
            });
            let lines: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::LineSegment { points, .. } => Some(points),
                    _ => None,
                })
                .collect();
            assert!(lines.len() > 4, "dashed outline is visible at every zoom");
            assert!(
                lines
                    .iter()
                    .flat_map(|line| line.iter())
                    .all(|&p| canvas.contains(p))
            );
        }
        assert_eq!(
            crate::image_clipboard::copy_pixels(&d, Some((e.area.unwrap(), vec![]))).unwrap(),
            before
        );
        assert!(!h.can_undo());
    }
    #[test]
    fn resizing_area_and_saved_filter_keeps_mask_and_single_undo_step() {
        let mut d = doc();
        let mut e = EditorState {
            tool: Tool::Area,
            area: Some(Rect::from_min_max(
                egui::pos2(10., 10.),
                egui::pos2(50., 50.),
            )),
            area_mask: vec![
                Point { x: 0., y: 0. },
                Point { x: 1., y: 0. },
                Point { x: 0., y: 1. },
            ],
            ..Default::default()
        };
        let mut h = EditHistory::new(d.edit_state());
        e.begin_area_drag(Point { x: 50., y: 50. }, &mut d);
        e.update_area_drag(Point { x: 70., y: 70. }, &mut d);
        e.finish_area_drag(&mut d, &mut h);
        assert_eq!(e.area.unwrap().width(), 60.);
        assert!(!h.can_undo());
        e.apply_area_action(AnnotationKind::Blur, &mut d, &mut h);
        let before = d.edit_state();
        e.begin_area_drag(Point { x: 70., y: 70. }, &mut d);
        e.update_area_drag(Point { x: 80., y: 80. }, &mut d);
        e.finish_area_drag(&mut d, &mut h);
        assert_eq!(d.annotations[0].end, Point { x: 80., y: 80. });
        assert_eq!(d.annotations[0].area_mask.len(), 3);
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), before);
    }

    #[test]
    fn clicks_select_objects_drags_select_pixels_and_moves_are_undoable() {
        let mut d = doc();
        let mut e = EditorState {
            tool: Tool::Area,
            ..Default::default()
        };
        let mut a = e.new_annotation(AnnotationKind::Rectangle, Point { x: 20., y: 20. });
        a.end = Point { x: 60., y: 60. };
        a.fill = true;
        d.annotations.push(a);
        let original = d.edit_state();
        let mut h = EditHistory::new(original.clone());
        e.begin_area_drag(Point { x: 30., y: 30. }, &mut d);
        e.finish_area_drag(&mut d, &mut h);
        assert_eq!(e.selected, Some(0));
        assert!(e.area.is_none());
        assert!(!h.can_undo());
        e.begin_area_drag(Point { x: 5., y: 5. }, &mut d);
        e.update_area_drag(Point { x: 15., y: 15. }, &mut d);
        e.finish_area_drag(&mut d, &mut h);
        assert!(e.selected.is_none());
        assert_eq!(d.edit_state(), original);
        assert!(
            e.move_pixels,
            "New selections move their contents by default"
        );
        e.begin_area_drag(Point { x: 10., y: 10. }, &mut d);
        e.update_area_drag(Point { x: 20., y: 20. }, &mut d);
        e.finish_area_drag(&mut d, &mut h);
        assert_eq!(d.annotations[1].kind, AnnotationKind::MoveArea);
        let flat = render::flatten(&d).unwrap();
        assert_eq!(flat.get_pixel(5, 5)[3], 0);
        assert_eq!(flat.get_pixel(16, 16), d.image.get_pixel(6, 6));
        assert!(h.undo(&mut d));
        assert_eq!(d.edit_state(), original);
    }
    #[test]
    fn lasso_filters_are_masked_and_object_filters_cover_the_object() {
        let mut d = doc();
        let mut e = EditorState {
            tool: Tool::Area,
            lasso: true,
            ..Default::default()
        };
        let mut h = EditHistory::new(d.edit_state());
        e.begin_area_drag(Point { x: 10., y: 10. }, &mut d);
        for p in [
            Point { x: 80., y: 10. },
            Point { x: 10., y: 80. },
            Point { x: 10., y: 10. },
        ] {
            e.update_area_drag(p, &mut d);
        }
        e.finish_area_drag(&mut d, &mut h);
        assert!(e.area_mask.len() >= 3);
        e.apply_area_action(AnnotationKind::DeleteArea, &mut d, &mut h);
        let flat = render::flatten(&d).unwrap();
        assert_eq!(flat.get_pixel(20, 20)[3], 0);
        assert_eq!(flat.get_pixel(70, 70), d.image.get_pixel(70, 70));
        assert!(h.undo(&mut d));
        e.area = None;
        e.clear_selection();
        let mut a = e.new_annotation(AnnotationKind::Ellipse, Point { x: 20., y: 20. });
        a.end = Point { x: 50., y: 50. };
        a.fill = true;
        d.annotations.push(a);
        e.select(Some(0), &d);
        e.apply_area_action(AnnotationKind::Blur, &mut d, &mut h);
        assert_eq!(d.annotations.len(), 2);
        assert!(d.annotations[1].area_mask.is_empty());
        e.apply_area_action(AnnotationKind::Pixelate, &mut d, &mut h);
        assert_eq!(d.annotations.len(), 2);
        assert_eq!(d.annotations[1].kind, AnnotationKind::Pixelate);
    }
}
