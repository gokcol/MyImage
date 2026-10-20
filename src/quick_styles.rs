//! Contextual visual starters. Applied styles remain ordinary editable properties.
use super::*;
use crate::appearance;

#[derive(Clone, Copy)]
struct QuickStyle {
    label: &'static str,
    color: [u8; 4],
    width: f32,
    font_size: f32,
    filled: bool,
    text_color: [u8; 4],
    effect_size: u32,
}

fn styles(kind: AnnotationKind) -> [QuickStyle; 6] {
    let colors = if kind == AnnotationKind::Highlighter {
        [
            [255, 218, 35, 85],
            [100, 213, 116, 85],
            [60, 160, 240, 85],
            [241, 113, 173, 85],
            [255, 158, 59, 85],
            [170, 134, 239, 85],
        ]
    } else {
        [
            [225, 62, 61, 255],
            [40, 117, 219, 255],
            [0, 130, 139, 255],
            [240, 170, 28, 255],
            [38, 48, 65, 255],
            [132, 83, 207, 255],
        ]
    };
    let labels = if kind.is_effect() {
        ["8 px", "12 px", "16 px", "24 px", "32 px", "64 px"]
    } else if kind == AnnotationKind::Highlighter {
        ["Yellow", "Green", "Blue", "Pink", "Orange", "Violet"]
    } else {
        ["Red", "Blue", "Teal", "Amber", "Ink", "Violet"]
    };
    std::array::from_fn(|i| QuickStyle {
        label: labels[i],
        color: colors[i],
        width: if kind == AnnotationKind::Freehand {
            [3., 5., 8., 12., 2., 6.][i]
        } else {
            [3., 4., 4., 6., 3., 6.][i]
        },
        font_size: [28., 32., 28., 40., 24., 36.][i],
        filled: i >= 3,
        text_color: if i == 3 { [29, 38, 52, 255] } else { [255; 4] },
        effect_size: [8, 12, 16, 24, 32, 64][i],
    })
}

pub(super) fn kind_name(kind: AnnotationKind) -> &'static str {
    match kind {
        AnnotationKind::Text => "Text",
        AnnotationKind::Stamp => "Stamp",
        AnnotationKind::Fill => "Fill",
        AnnotationKind::MoveArea => "Moved area",
        AnnotationKind::DeleteArea => "Deleted area",
        AnnotationKind::Arrow => "Arrow",
        AnnotationKind::RoundedRectangle
        | AnnotationKind::Triangle
        | AnnotationKind::Diamond
        | AnnotationKind::Pentagon
        | AnnotationKind::Hexagon
        | AnnotationKind::Star
        | AnnotationKind::Cross
        | AnnotationKind::Parallelogram
        | AnnotationKind::Cylinder
        | AnnotationKind::Prism
        | AnnotationKind::Cone
        | AnnotationKind::Sphere
        | AnnotationKind::HexagonalPrism
        | AnnotationKind::Torus
        | AnnotationKind::Tetrahedron => crate::shapes::name(kind),
        AnnotationKind::Rectangle => "Rectangle",
        AnnotationKind::Ellipse => "Ellipse",
        AnnotationKind::Step => "Step",
        AnnotationKind::Callout => "Callout",
        AnnotationKind::Highlighter => "Highlighter",
        AnnotationKind::Freehand => "Pen",
        AnnotationKind::Magnify => "Magnify",
        AnnotationKind::Spotlight => "Spotlight",
        AnnotationKind::Simplify => "Simplify",
        AnnotationKind::Blur => "Blur",
        AnnotationKind::Pixelate => "Pixelate",
    }
}

impl EditorState {
    pub(super) fn style_kind(&self, document: &Document) -> Option<AnnotationKind> {
        if self.selected_indices(document).len() > 1 {
            return None;
        }
        self.selected
            .and_then(|i| document.annotations.get(i))
            .map(|a| a.kind)
            .or(self.tool.kind())
    }

    fn apply_quick_style(
        &mut self,
        document: &mut Document,
        history: &mut EditHistory,
        style: QuickStyle,
    ) {
        if self.selected.is_some() && !self.can_edit_selection(document) {
            return;
        }
        let Some(kind) = self.style_kind(document) else {
            return;
        };
        self.finish_annotation_gesture(document, history);
        if kind.is_effect() {
            self.effect_size = style.effect_size;
        } else {
            self.color = style.color;
            match kind {
                AnnotationKind::Text => self.font_size = style.font_size,
                AnnotationKind::Callout => {
                    self.font_size = style.font_size;
                    self.text_color = style.text_color;
                }
                AnnotationKind::Step => self.text_color = style.text_color,
                AnnotationKind::Rectangle
                | AnnotationKind::Ellipse
                | AnnotationKind::RoundedRectangle
                | AnnotationKind::Triangle
                | AnnotationKind::Diamond
                | AnnotationKind::Pentagon
                | AnnotationKind::Hexagon
                | AnnotationKind::Star
                | AnnotationKind::Cross
                | AnnotationKind::Parallelogram
                | AnnotationKind::Cylinder
                | AnnotationKind::Prism
                | AnnotationKind::Cone
                | AnnotationKind::Sphere
                | AnnotationKind::HexagonalPrism
                | AnnotationKind::Torus
                | AnnotationKind::Tetrahedron => {
                    self.stroke_width = style.width;
                    self.fill = style.filled;
                }
                AnnotationKind::Arrow | AnnotationKind::Freehand => self.stroke_width = style.width,
                _ => {}
            }
        }
        self.apply_inspector(document, history);
    }

    fn style_matches(&self, kind: AnnotationKind, style: QuickStyle) -> bool {
        if kind.is_effect() {
            return self.effect_size == style.effect_size;
        }
        self.color == style.color
            && match kind {
                AnnotationKind::Text => self.font_size == style.font_size,
                AnnotationKind::Callout => {
                    self.font_size == style.font_size && self.text_color == style.text_color
                }
                AnnotationKind::Step => self.text_color == style.text_color,
                AnnotationKind::Rectangle
                | AnnotationKind::Ellipse
                | AnnotationKind::RoundedRectangle
                | AnnotationKind::Triangle
                | AnnotationKind::Diamond
                | AnnotationKind::Pentagon
                | AnnotationKind::Hexagon
                | AnnotationKind::Star
                | AnnotationKind::Cross
                | AnnotationKind::Parallelogram
                | AnnotationKind::Cylinder
                | AnnotationKind::Prism
                | AnnotationKind::Cone
                | AnnotationKind::Sphere
                | AnnotationKind::HexagonalPrism
                | AnnotationKind::Torus
                | AnnotationKind::Tetrahedron => {
                    self.stroke_width == style.width && self.fill == style.filled
                }
                AnnotationKind::Arrow | AnnotationKind::Freehand => {
                    self.stroke_width == style.width
                }
                _ => true,
            }
    }

    pub(super) fn quick_styles(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let Some(kind) = self.style_kind(document) else {
            return;
        };
        if kind.is_area_edit() || matches!(kind, AnnotationKind::Stamp | AnnotationKind::Fill) {
            return;
        }
        if !kind.is_shape() {
            ui.label(egui::RichText::new(kind_name(kind)).size(18.).strong());
        }
        ui.label(egui::RichText::new("Quick styles").color(appearance::MUTED));
        ui.add_space(4.);
        let width = (ui.available_width() - 16.) / 3.;
        let enabled = self.selected.is_none() || self.can_edit_selection(document);
        let choices = styles(kind);
        ui.add_enabled_ui(enabled, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8., 6.);
            for row in choices.chunks(3) {
                ui.horizontal(|ui| {
                    for &style in row {
                        let selected = self.style_matches(kind, style);
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::new(width, 58.), Sense::click());
                        let label = format!("{} · {} quick style", kind_name(kind), style.label);
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::SelectableLabel,
                                ui.is_enabled(),
                                selected,
                                &label,
                            )
                        });
                        preview(
                            ui,
                            rect,
                            kind,
                            style,
                            selected,
                            response.hovered() || response.has_focus(),
                        );
                        if response
                            .on_hover_text(format!(
                                "Apply {} style to {}",
                                style.label,
                                kind_name(kind)
                            ))
                            .clicked()
                        {
                            self.apply_quick_style(document, history, style);
                        }
                    }
                });
            }
        });
        ui.add_space(3.);
        ui.separator();
        ui.add_space(3.);
    }
}

fn preview(
    ui: &egui::Ui,
    rect: Rect,
    kind: AnnotationKind,
    style: QuickStyle,
    selected: bool,
    hovered: bool,
) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect_filled(
        rect,
        7,
        if selected {
            Color32::from_rgb(229, 245, 244)
        } else {
            appearance::PAPER
        },
    );
    painter.rect_stroke(
        rect,
        7,
        Stroke::new(
            if selected { 2. } else { 1. },
            if selected || hovered {
                appearance::ACCENT
            } else {
                Color32::from_rgb(221, 228, 234)
            },
        ),
        StrokeKind::Inside,
    );
    let preview_scale = 0.75;
    let origin = rect.min + Vec2::new((rect.width() - 80. * preview_scale) * 0.5, 2.);
    let mut annotation = Annotation {
        kind,
        start: Point { x: 12., y: 12. },
        end: Point { x: 68., y: 40. },
        color: style.color,
        stroke_width: style.width.min(7.),
        fill: style.filled
            || matches!(
                kind,
                AnnotationKind::Step | AnnotationKind::Callout | AnnotationKind::Highlighter
            ),
        text: "Ab".into(),
        font_size: style.font_size.min(32.),
        effect_size: style.effect_size,
        step_number: 1,
        tail: None,
        text_color: style.text_color,
        group_id: None,
        layer: Default::default(),
        group_layer: Default::default(),
        area_source: None,
        area_mask: Vec::new(),
        stamp: None,
        fill_region: None,
        text_style: Default::default(),
        pixel_mask: None,
        arrow: Default::default(),
        studio: Default::default(),
        points: vec![],
    };
    match kind {
        AnnotationKind::Arrow => {
            annotation.start = Point { x: 15., y: 39. };
            annotation.end = Point { x: 65., y: 12. };
        }
        AnnotationKind::Text => {
            annotation.start = Point { x: 15., y: 6. };
        }
        AnnotationKind::Step => {
            annotation.start = Point { x: 23., y: 7. };
            annotation.end = Point { x: 57., y: 41. };
        }
        AnnotationKind::Callout => {
            annotation.start.y = 6.;
            annotation.end.y = 35.;
            annotation.text = "Note".into();
            annotation.tail = Some(Point { x: 47., y: 47. });
        }
        AnnotationKind::Freehand => {
            annotation.points = vec![
                Point { x: 12., y: 32. },
                Point { x: 24., y: 14. },
                Point { x: 38., y: 35. },
                Point { x: 52., y: 13. },
                Point { x: 68., y: 25. },
            ];
        }
        AnnotationKind::Highlighter => {
            painter.text(
                origin + Vec2::new(40., 26.) * preview_scale,
                egui::Align2::CENTER_CENTER,
                "Text",
                egui::FontId::proportional(17. * preview_scale),
                appearance::INK,
            );
        }
        _ => {}
    }
    if kind.is_effect() {
        // A neutral strength swatch: no source-image pixels enter the style gallery.
        for y in 0..3 {
            for x in 0..5 {
                let pos =
                    origin + Vec2::new(16. + x as f32 * 12., 12. + y as f32 * 10.) * preview_scale;
                let shade = [140, 180, 210][(x + y * 2) % 3];
                let color =
                    Color32::from_rgb(shade, shade.saturating_add(10), shade.saturating_add(16));
                if kind == AnnotationKind::Blur {
                    painter.circle_filled(pos, 7. * preview_scale, color.gamma_multiply(0.6));
                } else {
                    painter.rect_filled(
                        Rect::from_center_size(pos, Vec2::splat(10. * preview_scale)),
                        0,
                        color,
                    );
                }
            }
        }
    } else {
        render::paint_annotations(
            &painter,
            &[annotation],
            Pos2::new(origin.x, origin.y),
            preview_scale,
        );
    }
    painter.text(
        rect.center_bottom() - Vec2::new(0., 11.),
        egui::Align2::CENTER_CENTER,
        style.label,
        egui::FontId::proportional(11.),
        appearance::INK,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{document, presets::tests::artwork};

    fn blank() -> Document {
        Document {
            image: std::sync::Arc::new(image::RgbaImage::from_pixel(
                800,
                600,
                image::Rgba([245, 248, 252, 255]),
            )),
            annotations: vec![],
            geometry: Default::default(),
        }
    }

    #[test]
    fn every_kind_styles_undo_and_reopen_without_losing_content_or_geometry() {
        let dir = tempfile::tempdir().unwrap();
        for a in artwork() {
            let mut d = blank();
            d.annotations.push(a);
            d.annotations[0].group_id = None;
            d.annotations[0].group_layer = Default::default();
            d.annotations[0].layer.locked = false;
            let original = d.edit_state();
            let mut h = EditHistory::new(original.clone());
            let mut e = EditorState::default();
            e.select(Some(0), &d);
            let kind = d.annotations[0].kind;
            e.apply_quick_style(&mut d, &mut h, styles(kind)[5]);
            assert!(h.is_dirty(&d));
            let updated = d.edit_state();
            let before = &original.annotations[0];
            let after = &updated.annotations[0];
            assert_eq!(
                (before.start, before.end, before.tail),
                (after.start, after.end, after.tail)
            );
            assert_eq!(before.points, after.points);
            assert_eq!(before.text, after.text);
            assert_eq!(before.step_number, after.step_number);
            assert_eq!(before.layer, after.layer);
            assert!(h.undo(&mut d));
            assert_eq!(d.edit_state(), original);
            assert!(!h.can_undo());
            assert!(h.redo(&mut d));
            assert_eq!(d.edit_state(), updated);
            let path = dir.path().join("styled.myimage");
            document::save_project(&d, &path).unwrap();
            assert_eq!(document::load_project(&path).unwrap().edit_state(), updated);
        }
    }

    #[test]
    fn styles_protect_locked_members_and_multiple_selection_and_set_new_artwork_defaults() {
        let mut d = blank();
        d.annotations = artwork();
        for a in &mut d.annotations[..3] {
            a.group_layer.locked = true;
        }
        let original = d.edit_state();
        let mut h = EditHistory::new(original.clone());
        let mut e = EditorState::default();
        e.select(Some(0), &d);
        let color = e.color;
        e.apply_quick_style(&mut d, &mut h, styles(AnnotationKind::Text)[1]);
        assert_eq!(d.edit_state(), original);
        assert_eq!(e.color, color);
        assert!(!h.can_undo());
        e.set_selection(vec![3, 5], &d);
        e.apply_quick_style(&mut d, &mut h, styles(AnnotationKind::Ellipse)[1]);
        assert_eq!(d.edit_state(), original);
        assert!(!h.can_undo());
        e.clear_selection();
        e.activate_tool(Tool::Pen);
        let style = styles(AnnotationKind::Freehand)[3];
        e.apply_quick_style(&mut d, &mut h, style);
        let a = e.new_annotation(AnnotationKind::Freehand, Point { x: 20., y: 20. });
        assert_eq!(a.color, style.color);
        assert_eq!(a.stroke_width, style.width);
        assert!(!h.can_undo());
        e.activate_tool(Tool::Arrow);
        e.activate_tool(Tool::Pen);
        assert_eq!(e.color, style.color);
    }
    fn frame(
        e: &mut EditorState,
        d: &mut Document,
        h: &mut EditHistory,
        ctx: &egui::Context,
        texture: &egui::TextureHandle,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.enable_accesskit();
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1060., 690.))),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::TopBottomPanel::top("test-toolbar").show(ctx, |ui| {
                    ui.horizontal(|ui| e.toolbar(ui, d, h));
                });
                egui::TopBottomPanel::bottom("test-image-controls").show(ctx, |ui| {
                    ui.horizontal(|ui| e.image_controls(ui, d, h));
                });
                egui::CentralPanel::default()
                    .show(ctx, |ui| e.ui(ui, d, h, texture, &mut Default::default()));
            },
        )
    }

    fn click(
        label: &str,
        e: &mut EditorState,
        d: &mut Document,
        h: &mut EditHistory,
        ctx: &egui::Context,
        texture: &egui::TextureHandle,
    ) -> egui::FullOutput {
        let output = frame(e, d, h, ctx, texture, vec![]);
        let at = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.job.text == label => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                _ => None,
            })
            .or_else(|| {
                output
                    .platform_output
                    .accesskit_update
                    .as_ref()
                    .and_then(|update| {
                        update.nodes.iter().find_map(|(_, n)| {
                            (n.label() == Some(label)).then(|| n.bounds()).flatten()
                        })
                    })
                    .map(|r| Pos2::new(((r.x0 + r.x1) * 0.5) as f32, ((r.y0 + r.y1) * 0.5) as f32))
            })
            .unwrap_or_else(|| panic!("Missing button {label}"));
        frame(
            e,
            d,
            h,
            ctx,
            texture,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        frame(
            e,
            d,
            h,
            ctx,
            texture,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ],
        )
    }

    #[test]
    fn canvas_context_menu_operates_without_drawing_and_selection_controls_are_contextual() {
        let ctx = egui::Context::default();
        appearance::apply(&ctx);
        let texture = ctx.load_texture(
            "context menu UI",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut d = blank();
        let before = d.edit_state();
        let mut h = EditHistory::new(before.clone());
        let mut e = EditorState::default();
        e.activate_tool(Tool::Arrow);
        let output = frame(&mut e, &mut d, &mut h, &ctx, &texture, vec![]);
        let at = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id == texture.id() => Some(
                    mesh.vertices
                        .iter()
                        .fold(Rect::NOTHING, |r, v| {
                            r.union(Rect::from_min_max(v.pos, v.pos))
                        })
                        .center(),
                ),
                _ => None,
            })
            .expect("image canvas");
        for pressed in [true, false] {
            frame(
                &mut e,
                &mut d,
                &mut h,
                &ctx,
                &texture,
                vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::PointerButton {
                        pos: at,
                        button: PointerButton::Secondary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(d.edit_state(), before);
        click("Rotate left", &mut e, &mut d, &mut h, &ctx, &texture);
        assert_eq!(d.output_size(), [600, 800]);
        click(
            "Image tools · crop, resize, rotate",
            &mut e,
            &mut d,
            &mut h,
            &ctx,
            &texture,
        );
        click("Undo", &mut e, &mut d, &mut h, &ctx, &texture);
        assert_eq!(d.edit_state(), before);
        click(
            "Image tools · crop, resize, rotate",
            &mut e,
            &mut d,
            &mut h,
            &ctx,
            &texture,
        );
        click("Crop", &mut e, &mut d, &mut h, &ctx, &texture);
        assert!(e.tool == Tool::Crop && e.crop.is_some());
        click("Cancel", &mut e, &mut d, &mut h, &ctx, &texture);
        assert_eq!(d.edit_state(), before);
        let mut a = e.new_annotation(AnnotationKind::Arrow, Point { x: 20., y: 20. });
        a.end = Point { x: 150., y: 100. };
        d.annotations.push(a);
        e.select(Some(0), &d);
        let output = frame(&mut e, &mut d, &mut h, &ctx, &texture, vec![]);
        for label in ["Arrange ▾", "More ▾", "Select all Arrow (1)"] {
            assert!(
                output.shapes.iter().any(
                    |s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text == label)
                ),
                "Missing selected control {label}"
            );
        }
        // Secondary controls are revealed on demand, preserving room for properties.
        click("Arrange ▾", &mut e, &mut d, &mut h, &ctx, &texture);
        let output = frame(&mut e, &mut d, &mut h, &ctx, &texture, vec![]);
        for label in ["Group", "Duplicate"] {
            assert!(
                output.shapes.iter().any(
                    |s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text == label)
                ),
                "Missing arrangement control {label}"
            );
        }
    }

    #[test]
    fn tool_toolbar_opens_contextual_gallery_and_real_style_clicks_update_the_selected_object() {
        let ctx = egui::Context::default();
        appearance::apply(&ctx);
        let texture = ctx.load_texture(
            "styles UI",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            Default::default(),
        );
        let mut d = blank();
        let mut h = EditHistory::new(d.edit_state());
        let mut e = EditorState {
            layers_open: true,
            ..Default::default()
        };
        click("Arrow", &mut e, &mut d, &mut h, &ctx, &texture);
        assert!(e.tool == Tool::Arrow);
        assert!(!e.layers_open && !e.presets_open);
        let output = frame(&mut e, &mut d, &mut h, &ctx, &texture, vec![]);
        let text_bounds = |label: &str| {
            output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.job.text == label => {
                        let bounds = Rect::from_min_size(t.pos, t.galley.size());
                        assert!(s.clip_rect.contains_rect(bounds), "Clipped {label}");
                        Some(bounds)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("Missing {label}"))
        };
        let gallery = text_bounds("Quick styles");
        assert!(text_bounds("Arrow").bottom() < gallery.top());
        for hidden in [
            "Crop",
            "Resize…",
            "Rotate left",
            "Undo",
            "Group",
            "Ungroup",
            "Duplicate",
            "Select all",
            "Snap to guides",
            "Delete selected",
        ] {
            assert!(
                !output.shapes.iter().any(
                    |s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text == hidden)
                ),
                "Unexpected {hidden}"
            );
        }
        assert!(text_bounds("Arrow properties").top() > text_bounds("Violet").bottom());
        click("Blue", &mut e, &mut d, &mut h, &ctx, &texture);
        assert_eq!(e.color, styles(AnnotationKind::Arrow)[1].color);
        assert!(!h.can_undo());
        let mut a = e.new_annotation(AnnotationKind::Arrow, Point { x: 20., y: 20. });
        a.end = Point { x: 100., y: 80. };
        d.annotations.push(a);
        h = EditHistory::new(d.edit_state());
        e.activate_tool(Tool::Select);
        e.select(Some(0), &d);
        click("Violet", &mut e, &mut d, &mut h, &ctx, &texture);
        assert_eq!(
            d.annotations[0].color,
            styles(AnnotationKind::Arrow)[5].color
        );
        assert!(h.undo(&mut d));
        assert_eq!(
            d.annotations[0].color,
            styles(AnnotationKind::Arrow)[1].color
        );
        click("Callout", &mut e, &mut d, &mut h, &ctx, &texture);
        click("Amber", &mut e, &mut d, &mut h, &ctx, &texture);
        assert_eq!(e.color, styles(AnnotationKind::Callout)[3].color);
        assert_eq!(e.text_color, [29, 38, 52, 255]);
    }
}
