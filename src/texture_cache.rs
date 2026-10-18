//! Reuse uploaded source textures on warm Recents/Library opens. Identity comes
//! from the loader's version-checked immutable pixels, never just a filename.
use eframe::egui::{self, ColorImage, TextureHandle};
use std::{collections::VecDeque, sync::Arc};
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_FILES: usize = 3;
#[derive(Default)]
pub struct SourceTextures {
    entries: VecDeque<(Arc<ColorImage>, TextureHandle)>,
    bytes: usize,
}
impl SourceTextures {
    pub fn get(&mut self, ctx: &egui::Context, image: Arc<ColorImage>) -> TextureHandle {
        if let Some(index) = self
            .entries
            .iter()
            .position(|(pixels, _)| Arc::ptr_eq(pixels, &image))
        {
            let entry = self.entries.remove(index).unwrap();
            let texture = entry.1.clone();
            self.entries.push_back(entry);
            return texture;
        }
        let texture =
            ctx.load_texture("opened source", image.clone(), egui::TextureOptions::LINEAR);
        let bytes = image.pixels.len() * 4;
        if bytes <= MAX_BYTES {
            while self.entries.len() >= MAX_FILES || self.bytes + bytes > MAX_BYTES {
                let Some((image, _)) = self.entries.pop_front() else {
                    break;
                };
                self.bytes -= image.pixels.len() * 4;
            }
            self.bytes += bytes;
            self.entries.push_back((image, texture.clone()));
        }
        texture
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn pixels() -> Arc<ColorImage> {
        Arc::new(ColorImage::new([16, 16], vec![egui::Color32::WHITE; 256]))
    }
    #[test]
    fn warm_open_reuses_gpu_upload_but_replaced_pixels_do_not() {
        let ctx = egui::Context::default();
        let mut cache = SourceTextures::default();
        let image = pixels();
        let first = cache.get(&ctx, image.clone());
        assert_eq!(first.id(), cache.get(&ctx, image).id());
        assert_ne!(first.id(), cache.get(&ctx, pixels()).id());
    }
    #[test]
    fn least_recent_uploads_are_bounded_and_hidden_window_releases_cache() {
        let ctx = egui::Context::default();
        let mut cache = SourceTextures::default();
        let first = pixels();
        let first_id = cache.get(&ctx, first.clone()).id();
        let _ = cache.get(&ctx, first.clone());
        drop(first);
        for _ in 0..MAX_FILES {
            let _ = cache.get(&ctx, pixels());
        }
        assert!(
            cache
                .entries
                .iter()
                .all(|(_, texture)| texture.id() != first_id)
        );
        assert_eq!(cache.entries.len(), MAX_FILES);
        assert_eq!(cache.bytes, MAX_FILES * 16 * 16 * 4);
        cache.clear();
        assert_eq!(cache.bytes, 0);
        assert!(cache.entries.is_empty());
    }
}
