//! Local text recognition of the already-composited selected pixels.
use image::RgbaImage;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

pub fn recognize(pixels: RgbaImage) -> Result<String, String> {
    let pixels = prepare(pixels)?;
    #[cfg(target_os = "macos")]
    let text = macos::recognize(&pixels)?;
    #[cfg(target_os = "windows")]
    let text = windows::recognize(&pixels)?;
    Ok(text.trim().to_owned())
}

fn prepare(mut pixels: RgbaImage) -> Result<RgbaImage, String> {
    let (width, height) = pixels.dimensions();
    if width < 2 || height < 2 {
        return Err("Select a larger area containing text.".into());
    }
    crate::document::validate_dimensions(width, height)?;
    // Flatten transparency onto white so excluded lasso/magic-wand pixels never
    // leak through to recognition. Avoid premultiplied dark fringes when scaling.
    for pixel in pixels.pixels_mut() {
        let alpha = u32::from(pixel[3]);
        for channel in &mut pixel.0[..3] {
            *channel = ((u32::from(*channel) * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
        }
        pixel[3] = 255;
    }
    // Both platform engines accept a bounded bitmap. Retain aspect ratio and
    // cap its longest side; no recognition job holds an unbounded native image.
    if width.max(height) > 4000 {
        let scale = 4000. / f64::from(width.max(height));
        pixels = image::imageops::resize(
            &pixels,
            (f64::from(width) * scale).round().max(1.) as u32,
            (f64::from(height) * scale).round().max(1.) as u32,
            image::imageops::FilterType::Lanczos3,
        );
    }
    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transparent_mask_pixels_are_removed_before_recognition() {
        let mut pixels = RgbaImage::from_pixel(3, 2, image::Rgba([0, 0, 0, 0]));
        pixels.put_pixel(1, 0, image::Rgba([0, 0, 0, 128]));
        pixels.put_pixel(2, 0, image::Rgba([12, 34, 56, 255]));
        let prepared = prepare(pixels).unwrap();
        assert_eq!(prepared.get_pixel(0, 0).0, [255; 4]);
        assert_eq!(prepared.get_pixel(1, 0).0, [127, 127, 127, 255]);
        assert_eq!(prepared.get_pixel(2, 0).0, [12, 34, 56, 255]);
    }
    #[test]
    fn recognition_bounds_preserve_aspect_and_reject_empty_regions() {
        let pixels = prepare(RgbaImage::new(8000, 100)).unwrap();
        assert_eq!(pixels.dimensions(), (4000, 50));
        assert!(prepare(RgbaImage::new(1, 10)).is_err());
    }
}
