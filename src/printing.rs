//! Print an immutable, fully rendered document through the system print dialog.
//! Preparation happens off the UI thread; no source pixels or private temporary
//! files are passed to a printer before annotations/effects have been flattened.
use crate::{document::Document, render};
use eframe::egui;
use std::sync::mpsc;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Submitted,
    NotCompleted,
}
pub type Result = std::result::Result<Outcome, String>;

pub struct Prepared {
    size: [u32; 2],
    title: String,
    // Encoded PNG on Mac, top-down opaque BGRA on Windows.
    bytes: Vec<u8>,
}

pub fn prepare(document: &Document, title: String) -> std::result::Result<Prepared, String> {
    let mut image = render::flatten(document)?;
    matte_white(&mut image);
    let size = [image.width(), image.height()];
    #[cfg(target_os = "macos")]
    let bytes = {
        use image::ImageEncoder;
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(
                image.as_raw(),
                size[0],
                size[1],
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|error| format!("Could not prepare the print image: {error}"))?;
        bytes
    };
    #[cfg(target_os = "windows")]
    let bytes = {
        for pixel in image.pixels_mut() {
            pixel.0.swap(0, 2);
        }
        image.into_raw()
    };
    Ok(Prepared { size, title, bytes })
}

fn matte_white(image: &mut image::RgbaImage) {
    for pixel in image.pixels_mut() {
        let alpha = u32::from(pixel[3]);
        for channel in &mut pixel.0[..3] {
            *channel = ((u32::from(*channel) * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
        }
        pixel[3] = 255;
    }
}

/// Fit in physical units even when the printer's horizontal/vertical DPI differ.
#[cfg(any(target_os = "windows", test))]
fn fit(size: [u32; 2], page: [i32; 2], dpi: [i32; 2]) -> std::result::Result<[i32; 4], String> {
    if size.contains(&0) || page.iter().any(|v| *v <= 0) || dpi.iter().any(|v| *v <= 0) {
        return Err("The printer returned an invalid printable page size.".into());
    }
    let scale = (f64::from(page[0]) / f64::from(dpi[0]) / f64::from(size[0]))
        .min(f64::from(page[1]) / f64::from(dpi[1]) / f64::from(size[1]));
    let width = (f64::from(size[0]) * scale * f64::from(dpi[0]))
        .round()
        .clamp(1., f64::from(page[0])) as i32;
    let height = (f64::from(size[1]) * scale * f64::from(dpi[1]))
        .round()
        .clamp(1., f64::from(page[1])) as i32;
    Ok([(page[0] - width) / 2, (page[1] - height) / 2, width, height])
}

pub struct Job {
    receiver: mpsc::Receiver<Result>,
    #[cfg(target_os = "macos")]
    _native: macos::NativeJob,
}
impl Job {
    pub fn start(
        ctx: &egui::Context,
        frame: &eframe::Frame,
        prepared: Prepared,
    ) -> std::result::Result<Self, String> {
        let handle = frame
            .window_handle()
            .map_err(|error| format!("The print window is unavailable: {error}"))?
            .as_raw();
        let (sender, receiver) = mpsc::channel();
        #[cfg(target_os = "macos")]
        {
            let RawWindowHandle::AppKit(handle) = handle else {
                return Err("No Mac print window is available.".into());
            };
            let native = macos::start(ctx.clone(), handle.ns_view, prepared, sender)?;
            Ok(Self {
                receiver,
                _native: native,
            })
        }
        #[cfg(target_os = "windows")]
        {
            let RawWindowHandle::Win32(handle) = handle else {
                return Err("No Windows print window is available.".into());
            };
            windows::start(ctx.clone(), handle.hwnd.get(), prepared, sender)?;
            Ok(Self { receiver })
        }
    }
    pub fn poll(&self) -> Option<Result> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("The print dialog closed unexpectedly.".into()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn print_payload_contains_finished_annotations_geometry_and_white_transparency() {
        use crate::document::AnnotationKind;
        use crate::geometry::ImageGeometry;
        use std::sync::Arc;
        let mut cover =
            crate::markup_tests::object(AnnotationKind::Rectangle, [0., 0.], [20., 20.]);
        cover.color = [0, 0, 0, 255];
        cover.fill = true;
        let document = Document {
            image: Arc::new(image::RgbaImage::from_pixel(
                80,
                40,
                image::Rgba([200, 50, 20, 0]),
            )),
            annotations: vec![cover],
            geometry: ImageGeometry {
                flip_horizontal: true,
                ..Default::default()
            },
        };
        let original = document.edit_state();
        let prepared = prepare(&document, "Print test".into()).unwrap();
        assert_eq!(prepared.size, [80, 40]);
        #[cfg(target_os = "macos")]
        let pixels = image::load_from_memory(&prepared.bytes)
            .unwrap()
            .into_rgba8();
        #[cfg(target_os = "windows")]
        let pixels = {
            let mut pixels = image::RgbaImage::from_raw(80, 40, prepared.bytes).unwrap();
            for pixel in pixels.pixels_mut() {
                pixel.0.swap(0, 2);
            }
            pixels
        };
        assert_eq!(
            *pixels.get_pixel(69, 10),
            image::Rgba([0, 0, 0, 255]),
            "the opaque cover must survive flattening and flipping"
        );
        assert_eq!(
            *pixels.get_pixel(10, 10),
            image::Rgba([255, 255, 255, 255]),
            "transparent source colors must not leak onto paper"
        );
        assert_eq!(
            document.edit_state(),
            original,
            "printing must not edit the project"
        );
    }

    #[test]
    fn printing_composites_transparency_on_white_without_dark_edges() {
        let mut image =
            image::RgbaImage::from_raw(3, 1, vec![0, 0, 0, 0, 20, 100, 200, 128, 12, 34, 56, 255])
                .unwrap();
        matte_white(&mut image);
        assert_eq!(
            image.as_raw(),
            &[255, 255, 255, 255, 137, 177, 227, 255, 12, 34, 56, 255]
        );
    }
    #[test]
    fn print_fit_centers_without_cropping_and_respects_unequal_printer_dpi() {
        assert_eq!(
            fit([400, 200], [600, 800], [300, 300]).unwrap(),
            [0, 250, 600, 300]
        );
        assert_eq!(
            fit([200, 400], [600, 800], [300, 300]).unwrap(),
            [100, 0, 400, 800]
        );
        assert_eq!(
            fit([400, 200], [1200, 800], [600, 300]).unwrap(),
            [0, 250, 1200, 300]
        );
        for (page, dpi) in [([0, 800], [300, 300]), ([600, 800], [0, 300])] {
            assert!(fit([1, 1], page, dpi).is_err());
        }
    }
}
