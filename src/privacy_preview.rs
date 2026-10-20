//! One bounded background preview job, with no stale or unprocessed fallback.

use crate::document::{Annotation, Document};
use eframe::egui::{self, TextureHandle};
use image::RgbaImage;
use std::sync::{Arc, mpsc};

pub(crate) const MAX_PREVIEW_SIDE: u32 = 2048;

struct PreviewKey {
    original: Arc<RgbaImage>,
    annotations: Vec<Annotation>,
    max_side: u32,
    expanded_geometry: Option<crate::geometry::ImageGeometry>,
    effects: crate::image_effects::ImageEffects,
}

impl PreviewKey {
    fn matches(&self, document: &Document, max_side: u32) -> bool {
        Arc::ptr_eq(&self.original, &document.image)
            && self.annotations == document.annotations
            && self.effects == document.geometry.effects
            && self.max_side == max_side
            && self.expanded_geometry == document.geometry.canvas.map(|_| document.geometry)
    }
}

struct Reply {
    generation: u64,
    pixels: Result<RgbaImage, String>,
}

pub struct PrivacyPreview {
    tx: mpsc::Sender<Reply>,
    rx: mpsc::Receiver<Reply>,
    generation: u64,
    requested: Option<PreviewKey>,
    running: bool,
    ready: Option<TextureHandle>,
    error: Option<String>,
    refined: bool,
}

impl Default for PrivacyPreview {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            generation: 0,
            requested: None,
            running: false,
            ready: None,
            error: None,
            refined: false,
        }
    }
}

impl PrivacyPreview {
    /// Install the matching composition prepared by the file-opening worker.
    pub fn seed(
        &mut self,
        ctx: &egui::Context,
        document: &Document,
        max_side: u32,
        color: Arc<egui::ColorImage>,
    ) {
        self.clear();
        self.requested = Some(PreviewKey {
            original: Arc::clone(&document.image),
            annotations: document.annotations.clone(),
            max_side: max_side.min(MAX_PREVIEW_SIDE),
            expanded_geometry: document.geometry.canvas.map(|_| document.geometry),
            effects: document.geometry.effects,
        });
        self.refined = true;
        self.ready =
            Some(ctx.load_texture("opened composition", color, egui::TextureOptions::LINEAR));
    }

    /// Release cached pixels, invalidating even a reply still in flight.
    pub fn clear(&mut self) {
        if self.requested.take().is_some() {
            self.generation = self.generation.wrapping_add(1);
        }
        self.ready = None;
        self.refined = false;
        self.error = None;
        while self.rx.try_recv().is_ok() {
            self.running = false;
        }
    }

    /// Display a small *current* composition immediately while the worker builds
    /// the full preview. Never reuse old pixels after a cut/redaction change.
    pub fn continuous_texture(
        &mut self,
        ctx: &egui::Context,
        document: &Document,
    ) -> Result<Option<TextureHandle>, String> {
        self.current_texture(ctx, document, true)
    }

    /// Active drags use only the bounded composition, avoiding a full expanded
    /// canvas allocation/refinement job for every intermediate pointer position.
    pub fn drag_texture(
        &mut self,
        ctx: &egui::Context,
        document: &Document,
    ) -> Result<Option<TextureHandle>, String> {
        self.current_texture(ctx, document, false)
    }

    fn current_texture(
        &mut self,
        ctx: &egui::Context,
        document: &Document,
        refine: bool,
    ) -> Result<Option<TextureHandle>, String> {
        if let Some(texture) = self.texture_impl(ctx, document, refine)? {
            return Ok(Some(texture));
        }
        let side = self
            .requested
            .as_ref()
            .map_or(768, |key| key.max_side.min(768));
        let pixels = crate::render::interactive_preview(document, side)?;
        let color = crate::image_loading::color_image(&pixels);
        let texture = ctx.load_texture(
            "interactive composition",
            color,
            egui::TextureOptions::LINEAR,
        );
        self.ready = Some(texture.clone());
        Ok(Some(texture))
    }

    /// Only a composition matching the current source and complete object stack
    /// may be shown. Geometry is applied to this original-aspect texture by the
    /// normal canvas renderer. Editing while a job runs replaces the desired
    /// state, not the worker: at most one image job can allocate pixels at once.
    pub fn texture(
        &mut self,
        ctx: &egui::Context,
        document: &Document,
    ) -> Result<Option<TextureHandle>, String> {
        self.texture_impl(ctx, document, true)
    }

    fn texture_impl(
        &mut self,
        ctx: &egui::Context,
        document: &Document,
        refine: bool,
    ) -> Result<Option<TextureHandle>, String> {
        let limit = if refine { MAX_PREVIEW_SIDE } else { 768 };
        let max_side = (ctx.input(|input| input.max_texture_side).max(1) as u32).min(limit);
        if !self
            .requested
            .as_ref()
            .is_some_and(|key| key.matches(document, max_side))
        {
            self.generation = self.generation.wrapping_add(1);
            self.requested = Some(PreviewKey {
                original: Arc::clone(&document.image),
                annotations: document.annotations.clone(),
                max_side,
                expanded_geometry: document.geometry.canvas.map(|_| document.geometry),
                effects: document.geometry.effects,
            });
            self.ready = None;
            self.refined = false;
            self.error = None;
        }
        while let Ok(reply) = self.rx.try_recv() {
            self.running = false;
            if reply.generation != self.generation {
                continue;
            }
            match reply.pixels {
                Ok(pixels) => {
                    self.refined = true;
                    let color = egui::ColorImage::from_rgba_unmultiplied(
                        [pixels.width() as usize, pixels.height() as usize],
                        pixels.as_raw(),
                    );
                    self.ready = Some(ctx.load_texture(
                        "privacy composition",
                        color,
                        egui::TextureOptions::LINEAR,
                    ));
                    self.error = None;
                }
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        // A completed matching reply needs no further refinement.
        if self.ready.is_some() && self.refined {
            return Ok(self.ready.clone());
        }
        if !self.running && refine {
            let document = document.clone();
            let generation = self.generation;
            let tx = self.tx.clone();
            let ctx = ctx.clone();
            self.running = true;
            std::thread::spawn(move || {
                let pixels = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    crate::render::privacy_preview(&document, max_side)
                }))
                .unwrap_or_else(|_| Err("The privacy preview could not be rendered.".into()));
                let _ = tx.send(Reply { generation, pixels });
                ctx.request_repaint();
            });
        }
        Ok(self.ready.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{AnnotationKind, Point};

    fn document() -> Document {
        Document {
            image: Arc::new(RgbaImage::new(64, 48)),
            annotations: vec![Annotation {
                kind: AnnotationKind::Blur,
                start: Point { x: 5.0, y: 6.0 },
                end: Point { x: 40.0, y: 35.0 },
                color: [0, 0, 0, 255],
                stroke_width: 4.0,
                fill: false,
                text: String::new(),
                font_size: 24.0,
                effect_size: 16,
                step_number: 1,
                tail: None,
                text_color: [255; 4],
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
                points: Vec::new(),
            }],
            geometry: Default::default(),
        }
    }

    #[test]
    fn drag_preview_reuses_matching_pixels_without_starting_refinement_workers() {
        let ctx = egui::Context::default();
        let mut doc = document();
        let mut cache = PrivacyPreview::default();
        doc.geometry.canvas = Some(crate::geometry::CanvasRect {
            x: -50,
            y: -30,
            width: 200,
            height: 150,
        });
        let first = cache.drag_texture(&ctx, &doc).unwrap().unwrap();
        assert!(!cache.running);
        assert_eq!(
            cache.drag_texture(&ctx, &doc).unwrap().unwrap().id(),
            first.id()
        );
        let previous_generation = cache.generation;
        doc.geometry.canvas.as_mut().unwrap().width += 20;
        let moved = cache.drag_texture(&ctx, &doc).unwrap().unwrap();
        assert_ne!(moved.id(), first.id());
        cache
            .tx
            .send(Reply {
                generation: previous_generation,
                pixels: Ok(RgbaImage::new(1, 1)),
            })
            .unwrap();
        assert_eq!(
            cache.drag_texture(&ctx, &doc).unwrap().unwrap().id(),
            moved.id()
        );
        assert!(!cache.running);
        assert!(moved.size().iter().all(|&v| v <= 768));
        cache.continuous_texture(&ctx, &doc).unwrap();
        assert!(cache.running, "Release resumes normal refinement");
    }

    #[test]
    fn continuous_preview_has_no_blank_transition_and_rejects_stale_worker_pixels() {
        let ctx = egui::Context::default();
        let mut doc = document();
        doc.image = Arc::new(RgbaImage::from_pixel(
            64,
            48,
            image::Rgba([220, 80, 20, 255]),
        ));
        doc.annotations[0].kind = AnnotationKind::DeleteArea;
        let mut cache = PrivacyPreview {
            running: true,
            ..Default::default()
        };
        let first = cache.continuous_texture(&ctx, &doc).unwrap().unwrap();
        let old_generation = cache.generation;
        assert_eq!(
            cache.continuous_texture(&ctx, &doc).unwrap().unwrap().id(),
            first.id()
        );
        doc.annotations[0].start.x += 5.;
        let moved = cache.continuous_texture(&ctx, &doc).unwrap().unwrap();
        assert_ne!(
            moved.id(),
            first.id(),
            "Never return a stale cut/redaction frame"
        );
        cache
            .tx
            .send(Reply {
                generation: old_generation,
                pixels: Ok(RgbaImage::new(64, 48)),
            })
            .unwrap();
        assert_eq!(
            cache.continuous_texture(&ctx, &doc).unwrap().unwrap().id(),
            moved.id()
        );
        assert!(
            cache.running,
            "Stale job completion starts the newest refinement"
        );
        let pixels = crate::render::interactive_preview(&doc, 768).unwrap();
        assert_eq!(pixels.get_pixel(12, 10)[3], 0);
        assert_eq!(pixels.get_pixel(2, 2), doc.image.get_pixel(2, 2));
        assert_eq!(pixels, crate::render::privacy_preview(&doc, 768).unwrap());
    }

    #[test]
    fn interactive_expanded_preview_matches_full_composition_and_stays_bounded() {
        let mut doc = document();
        doc.image = Arc::new(RgbaImage::from_fn(64, 48, |x, y| {
            image::Rgba([x as u8, y as u8, 80, 255])
        }));
        doc.geometry.crop = Some(crate::geometry::CropRect {
            x: 4,
            y: 3,
            width: 50,
            height: 40,
        });
        doc.geometry.canvas = Some(crate::geometry::CanvasRect {
            x: -8,
            y: -10,
            width: 90,
            height: 70,
        });
        assert_eq!(
            crate::render::interactive_preview(&doc, 768).unwrap(),
            crate::render::privacy_preview(&doc, 768).unwrap()
        );
        doc.geometry.canvas = Some(crate::geometry::CanvasRect {
            x: -1000,
            y: -1000,
            width: 8000,
            height: 6000,
        });
        assert_eq!(
            crate::render::interactive_preview(&doc, 768)
                .unwrap()
                .dimensions(),
            (768, 576)
        );
        doc.annotations[0].effect_size = 0;
        assert!(crate::render::interactive_preview(&doc, 768).is_err());
    }

    #[test]
    fn prepared_opening_is_immediate_and_editing_invalidates_its_composition() {
        let ctx = egui::Context::default();
        let mut doc = document();
        let mut cache = PrivacyPreview::default();
        let side = ctx.input(|i| i.max_texture_side).max(1) as u32;
        let pixels = crate::render::privacy_preview(&doc, side.min(MAX_PREVIEW_SIDE)).unwrap();
        cache.seed(
            &ctx,
            &doc,
            side,
            Arc::new(crate::image_loading::color_image(&pixels)),
        );
        let ready = cache.texture(&ctx, &doc).unwrap().unwrap();
        assert!(!cache.running);
        doc.geometry.quarter_turns = 1;
        assert_eq!(cache.texture(&ctx, &doc).unwrap().unwrap().id(), ready.id());
        cache.running = true;
        doc.annotations[0].effect_size += 1;
        assert!(cache.texture(&ctx, &doc).unwrap().is_none());
    }

    #[test]
    fn moving_or_adjusting_a_region_never_returns_the_previous_texture() {
        let ctx = egui::Context::default();
        let mut doc = document();
        // Simulate a running worker so the test can deliver replies explicitly.
        let mut cache = PrivacyPreview {
            running: true,
            ..Default::default()
        };
        assert!(cache.texture(&ctx, &doc).unwrap().is_none());
        cache
            .tx
            .send(Reply {
                generation: cache.generation,
                pixels: Ok(RgbaImage::new(64, 48)),
            })
            .unwrap();
        assert!(cache.texture(&ctx, &doc).unwrap().is_some());
        cache.running = true;
        let old_generation = cache.generation;
        doc.annotations[0].start.x += 1.0;
        assert!(cache.texture(&ctx, &doc).unwrap().is_none());
        // An older in-flight reply cannot satisfy the changed region.
        cache
            .tx
            .send(Reply {
                generation: old_generation,
                pixels: Ok(RgbaImage::new(64, 48)),
            })
            .unwrap();
        // Keep a current reply queued after the stale one, simulating completion
        // without letting the real worker run during this deterministic test.
        cache
            .tx
            .send(Reply {
                generation: cache.generation,
                pixels: Err("test failure".into()),
            })
            .unwrap();
        assert!(cache.texture(&ctx, &doc).is_err());
        cache.running = true;
        doc.annotations[0].effect_size += 1;
        assert!(cache.texture(&ctx, &doc).unwrap().is_none());
    }

    #[test]
    fn geometry_reuses_composition_but_original_replacement_invalidates_it() {
        let ctx = egui::Context::default();
        let mut doc = document();
        let mut cache = PrivacyPreview {
            running: true,
            ..Default::default()
        };
        assert!(cache.texture(&ctx, &doc).unwrap().is_none());
        cache
            .tx
            .send(Reply {
                generation: cache.generation,
                pixels: Ok(RgbaImage::new(64, 48)),
            })
            .unwrap();
        let first = cache.texture(&ctx, &doc).unwrap().unwrap().id();
        doc.geometry.quarter_turns = 1;
        assert_eq!(cache.texture(&ctx, &doc).unwrap().unwrap().id(), first);
        cache.running = true;
        doc.image = Arc::new((*doc.image).clone());
        assert!(cache.texture(&ctx, &doc).unwrap().is_none());
        let stale = cache.generation;
        cache.clear();
        cache
            .tx
            .send(Reply {
                generation: stale,
                pixels: Ok(RgbaImage::new(64, 48)),
            })
            .unwrap();
        cache.clear();
        assert!(cache.ready.is_none());
        assert!(!cache.running);
    }
}
