//! A short canvas dissolve only when a different, fully prepared file opens.
//! Retain the already painted shapes/textures: no image readback or extra decode.
use crate::document::Document;
use eframe::egui::{self, Rect, Shape, TextureHandle, TextureId, epaint::ClippedShape};
use std::{collections::HashSet, sync::Arc};

const DURATION: f64 = 0.12;

pub struct CanvasFrame {
    shapes: Vec<ClippedShape>,
    // Keep GPU textures alive when installing the next document clears its preview.
    _textures: Vec<TextureHandle>,
    viewport: Rect,
    pixels_per_point: f32,
}

impl CanvasFrame {
    fn capture(ui: &egui::Ui, start: usize, viewport: Rect) -> Option<Self> {
        let shapes = ui.ctx().graphics(|graphics| {
            graphics.get(ui.layer_id()).map(|list| {
                list.all_entries()
                    .skip(start)
                    .filter_map(|entry| {
                        let clip_rect = entry.clip_rect.intersect(viewport);
                        (clip_rect.is_positive()
                            && entry.shape.visual_bounding_rect().intersects(clip_rect))
                        .then(|| ClippedShape {
                            clip_rect,
                            shape: entry.shape.clone(),
                        })
                    })
                    .collect::<Vec<_>>()
            })
        })?;
        if shapes.is_empty() {
            return None;
        }
        let mut ids = HashSet::new();
        for entry in &shapes {
            if !texture_ids(&entry.shape, &mut ids) {
                return None;
            }
        }
        let manager = ui.ctx().tex_manager();
        {
            let mut textures = manager.write();
            // User textures/callbacks cannot be retained safely by this cache.
            if ids.iter().any(|id| textures.meta(*id).is_none()) {
                return None;
            }
            for id in &ids {
                textures.retain(*id);
            }
        }
        Some(Self {
            shapes,
            _textures: ids
                .into_iter()
                .map(|id| TextureHandle::new(manager.clone(), id))
                .collect(),
            viewport,
            pixels_per_point: ui.ctx().pixels_per_point(),
        })
    }
}

fn texture_ids(shape: &Shape, ids: &mut HashSet<TextureId>) -> bool {
    match shape {
        Shape::Vec(shapes) => return shapes.iter().all(|shape| texture_ids(shape, ids)),
        Shape::Mesh(mesh) => {
            ids.insert(mesh.texture_id);
        }
        Shape::Rect(rect) => {
            if let Some(brush) = &rect.brush {
                ids.insert(brush.fill_texture_id);
            }
        }
        Shape::Callback(_) => return false,
        _ => {}
    }
    true
}

struct Fade {
    previous: CanvasFrame,
    target: Document,
    started: Option<f64>,
}

#[derive(Default)]
pub struct OpenTransition {
    loading: Option<CanvasFrame>,
    fade: Option<Fade>,
}

impl OpenTransition {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn capture_loading(&mut self, ui: &egui::Ui, start: usize, viewport: Rect) {
        self.loading = CanvasFrame::capture(ui, start, viewport);
    }

    pub fn take_loading(&mut self) -> Option<CanvasFrame> {
        self.loading.take()
    }

    pub fn start(&mut self, previous: Option<CanvasFrame>, target: &Document) {
        self.fade = previous.map(|previous| Fade {
            previous,
            target: target.clone(),
            started: None,
        });
    }

    pub fn paint(&mut self, ui: &egui::Ui, viewport: Rect, document: &Document) {
        let Some(fade) = &mut self.fade else {
            return;
        };
        let interrupted = ui.input(|input| {
            input.pointer.any_down()
                || input.raw.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::PointerButton { pressed: true, .. }
                            | egui::Event::Key { pressed: true, .. }
                            | egui::Event::MouseWheel { .. }
                            | egui::Event::Zoom(_)
                            | egui::Event::Touch { .. }
                            | egui::Event::Text(_)
                            | egui::Event::Paste(_)
                            | egui::Event::Cut
                    )
                })
        });
        // Edits, zooming, layout/DPI changes and dialogs must never replay stale
        // pixels over the current document (especially after a privacy edit).
        if interrupted
            || !ui.is_enabled()
            || viewport != fade.previous.viewport
            || ui.ctx().pixels_per_point() != fade.previous.pixels_per_point
            || !Arc::ptr_eq(&document.image, &fade.target.image)
            || document.annotations != fade.target.annotations
            || document.geometry != fade.target.geometry
        {
            self.fade = None;
            return;
        }
        let now = ui.input(|input| input.time);
        let elapsed = now - *fade.started.get_or_insert(now);
        if elapsed >= DURATION {
            self.fade = None;
            return;
        }
        let t = (elapsed / DURATION).clamp(0., 1.) as f32;
        let opacity = 1. - t * t * (3. - 2. * t);
        for entry in &fade.previous.shapes {
            let mut painter = ui.painter().with_clip_rect(entry.clip_rect);
            painter.set_opacity(opacity);
            painter.add(entry.shape.clone());
        }
        ui.ctx().request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Color32, Pos2, Vec2};

    fn input(time: f64) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(300.))),
            time: Some(time),
            ..Default::default()
        }
    }

    fn document() -> Document {
        Document {
            image: Arc::new(image::RgbaImage::new(100, 80)),
            annotations: vec![],
            geometry: Default::default(),
        }
    }

    fn begin(ctx: &egui::Context, doc: &Document) -> (OpenTransition, TextureId, Rect) {
        let mut transition = OpenTransition::default();
        let texture = ctx.load_texture(
            "previous protected canvas",
            egui::ColorImage::filled([10, 10], Color32::RED),
            egui::TextureOptions::LINEAR,
        );
        let id = texture.id();
        let viewport = Rect::from_min_size(egui::pos2(20., 20.), Vec2::splat(180.));
        let _ = ctx.run(input(0.), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let start = ui.painter().add(Shape::Noop).0;
                // A larger paint/clip rect exercises clipping to just the canvas.
                ui.painter().image(
                    id,
                    ui.max_rect(),
                    Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
                    Color32::WHITE,
                );
                transition.capture_loading(ui, start, viewport);
            });
        });
        let previous = transition.take_loading().expect("capture old canvas");
        drop(texture); // Mimic resetting the preview while installing the target.
        assert!(ctx.tex_manager().read().meta(id).is_some());
        transition.start(Some(previous), doc);
        (transition, id, viewport)
    }

    #[test]
    fn dissolve_retains_only_the_canvas_textures_until_it_finishes() {
        let ctx = egui::Context::default();
        let doc = document();
        let (mut transition, id, viewport) = begin(&ctx, &doc);
        for (time, expected_alpha) in [(1., Some(255)), (1.06, Some(128)), (1.13, None)] {
            let output = ctx.run(input(time), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| transition.paint(ui, viewport, &doc));
            });
            assert!(
                output.textures_delta.set.is_empty(),
                "dissolve must not upload new pixels"
            );
            let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
            let old = primitives.iter().find(|primitive| {
                matches!(&primitive.primitive,
                egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == id)
            });
            if let Some(alpha) = expected_alpha {
                let old = old.unwrap();
                assert_eq!(old.clip_rect, viewport);
                let egui::epaint::Primitive::Mesh(mesh) = &old.primitive else {
                    unreachable!()
                };
                assert!(
                    mesh.vertices
                        .iter()
                        .all(|v| v.color.a().abs_diff(alpha) <= 1)
                );
                assert!(ctx.tex_manager().read().meta(id).is_some());
            } else {
                assert!(old.is_none());
                assert!(ctx.tex_manager().read().meta(id).is_none());
                assert!(output.textures_delta.free.contains(&id));
            }
        }
    }

    #[test]
    fn edits_input_layout_and_dpi_changes_cancel_without_replaying_old_pixels() {
        for change in [
            "annotation",
            "geometry",
            "source",
            "pointer",
            "key",
            "zoom",
            "layout",
            "dpi",
            "disabled",
        ] {
            let ctx = egui::Context::default();
            let mut doc = document();
            let (mut transition, id, mut viewport) = begin(&ctx, &doc);
            let mut raw = input(1.);
            match change {
                "annotation" => doc.annotations.push(crate::markup_tests::object(
                    crate::document::AnnotationKind::Pixelate,
                    [0., 0.],
                    [90., 70.],
                )),
                "geometry" => doc.geometry.flip_horizontal = true,
                "source" => doc.image = Arc::new((*doc.image).clone()),
                "pointer" => raw.events.push(egui::Event::PointerButton {
                    pos: viewport.center(),
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }),
                "key" => raw.events.push(egui::Event::Key {
                    key: egui::Key::Z,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND,
                }),
                "zoom" => raw.events.push(egui::Event::Zoom(1.1)),
                "layout" => viewport.max.x -= 10.,
                "dpi" => ctx.set_pixels_per_point(2.),
                "disabled" => {}
                _ => unreachable!(),
            }
            let output = ctx.run(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    // A widget may consume its shortcut before the transition runs.
                    ui.input_mut(|i| i.events.clear());
                    ui.add_enabled_ui(change != "disabled", |ui| {
                        transition.paint(ui, viewport, &doc)
                    });
                });
            });
            assert!(transition.fade.is_none(), "cancel on {change}");
            assert!(
                ctx.tex_manager().read().meta(id).is_none(),
                "release texture on {change}"
            );
            assert!(!ctx.tessellate(output.shapes, output.pixels_per_point).iter().any(|p|
                matches!(&p.primitive, egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == id)),
                "stale pixels after {change}");
        }
    }
}
