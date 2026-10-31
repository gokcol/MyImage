//! ImageIO encodes HEVC-in-HEIF using Apple's installed codec. PNG is only an
//! internal lossless pixel bridge, never the bytes published as `.heif`/`.heic`.
use image::{ExtendedColorType, ImageEncoder, ImageReader, RgbaImage};
use objc2_core_foundation::{
    CFData, CFDictionary, CFMutableData, CFNumber, CFRetained, CFString, CFType,
};
use objc2_image_io::{
    CGImageDestination, CGImageSource, kCGImageDestinationLossyCompressionQuality,
    kCGImagePropertyPixelHeight, kCGImagePropertyPixelWidth,
};
use std::io::Cursor;

pub(super) fn available() -> bool {
    let Some(data) = CFMutableData::new(None, 0) else {
        return false;
    };
    let uti = CFString::from_str("public.heic");
    // SAFETY: ImageIO accepts mutable CFData, a valid UTI, one image and no options.
    unsafe { CGImageDestination::with_data(&data, &uti, 1, None).is_some() }
}
pub(super) fn encode(
    image: &RgbaImage,
    quality: u8,
    background: [u8; 3],
) -> Result<Vec<u8>, String> {
    let rgb = image::RgbImage::from_fn(image.width(), image.height(), |x, y| {
        let pixel = image.get_pixel(x, y);
        let alpha = u32::from(pixel[3]);
        image::Rgb(std::array::from_fn(|c| {
            ((u32::from(pixel[c]) * alpha + u32::from(background[c]) * (255 - alpha) + 127) / 255)
                as u8
        }))
    });
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            rgb.as_raw(),
            image.width(),
            image.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|e| e.to_string())?;
    let pixels = CFData::from_bytes(&png);
    // SAFETY: The source owns valid immutable PNG bytes and no unsafe options.
    let source = unsafe { CGImageSource::with_data(&pixels, None) }
        .ok_or("The native HEIF encoder could not read the flattened image.")?;
    let output = CFMutableData::new(None, 0).ok_or("Cannot allocate the HEIF output buffer.")?;
    let uti = CFString::from_str("public.heic");
    // SAFETY: The output buffer and UTI remain alive through finalization.
    let destination = unsafe { CGImageDestination::with_data(&output, &uti, 1, None) }
        .ok_or("HEIF encoding is unavailable on this Mac. Choose PNG, JPEG or WebP.")?;
    let quality = CFNumber::new_f64(f64::from(quality) / 100.);
    // SAFETY: ImageIO's quality key takes a CFNumber in the documented 0..1 range.
    let properties = CFDictionary::from_slices(
        &[unsafe { kCGImageDestinationLossyCompressionQuality }],
        &[&*quality],
    );
    // SAFETY: This is the only image in a source created above; properties contain
    // exactly the documented compression-quality key/value and no project metadata.
    unsafe {
        destination.add_image_from_source(&source, 0, Some(properties.as_opaque()));
    }
    // SAFETY: The destination has exactly one image, is not reused, and its data is alive.
    if !unsafe { destination.finalize() } {
        return Err("Apple's HEIF codec could not encode this image. Choose PNG or JPEG, or try a smaller image.".into());
    }
    let bytes = output.to_vec();
    if !is_heif(&bytes) {
        return Err(
            "The native HEIF codec did not produce a valid HEIF container. No file was saved."
                .into(),
        );
    }
    Ok(bytes)
}
fn is_heif(bytes: &[u8]) -> bool {
    bytes.len() >= 16
        && &bytes[4..8] == b"ftyp"
        && bytes[8..bytes.len().min(64)]
            .windows(4)
            .any(|brand| matches!(brand, b"heic" | b"heix" | b"hevc" | b"hevx"))
}
pub(super) fn decode(bytes: &[u8]) -> Result<RgbaImage, String> {
    let data = CFData::from_bytes(bytes);
    // SAFETY: Source initialization parses immutable input; dimensions are checked
    // before asking ImageIO to materialize image pixels.
    let source =
        unsafe { CGImageSource::with_data(&data, None) }.ok_or("Cannot read this HEIF image.")?;
    // SAFETY: Asking the source type/count does not decode its raster.
    let uti = unsafe { source.r#type() }.ok_or("The HEIF image has no recognized image type.")?;
    if !matches!(
        uti.to_string().as_str(),
        "public.heic" | "public.heif" | "public.heics"
    ) {
        return Err("The file is not a recognized HEIF/HEIC image.".into());
    }
    // SAFETY: Index zero is only read after checking that the source has an image.
    if unsafe { source.count() } == 0 {
        return Err("The HEIF file contains no image.".into());
    }
    let properties = unsafe { source.properties_at_index(0, None) }
        .ok_or("Cannot read HEIF image dimensions.")?;
    // SAFETY: ImageIO documents its property dictionary as CFString keys and
    // CoreFoundation object values; concrete number values are checked below.
    let properties: CFRetained<CFDictionary<CFString, CFType>> =
        unsafe { CFRetained::cast_unchecked(properties) };
    let dimension = |key: &CFString| -> Result<u32, String> {
        let value = properties.get(key).ok_or("Missing HEIF image dimension.")?;
        let n = value
            .downcast_ref::<CFNumber>()
            .and_then(CFNumber::as_i64)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or("Invalid HEIF image dimension.")?;
        Ok(n)
    };
    // SAFETY: These immutable ImageIO property keys are globally valid CFStrings.
    let width = dimension(unsafe { kCGImagePropertyPixelWidth })?;
    let height = dimension(unsafe { kCGImagePropertyPixelHeight })?;
    crate::document::validate_dimensions(width, height)?;
    let output = CFMutableData::new(None, 0).ok_or("Cannot allocate the HEIF decoding buffer.")?;
    let png = CFString::from_str("public.png");
    // SAFETY: A native lossless PNG bridge materializes only the validated first
    // image; none of these intermediate bytes are written to the user's filesystem.
    let destination = unsafe { CGImageDestination::with_data(&output, &png, 1, None) }
        .ok_or("The native HEIF decoder is unavailable.")?;
    unsafe {
        destination.add_image_from_source(&source, 0, None);
    }
    if !unsafe { destination.finalize() } {
        return Err("The native decoder could not read this HEIF image.".into());
    }
    let mut reader =
        ImageReader::with_format(Cursor::new(output.to_vec()), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(crate::document::MAX_IMAGE_SIDE);
    limits.max_image_height = Some(crate::document::MAX_IMAGE_SIDE);
    limits.max_alloc = Some(800_000_000);
    reader.limits(limits);
    reader
        .decode()
        .map(|i| i.to_rgba8())
        .map_err(|e| format!("Cannot decode HEIF pixels: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_heif_is_a_real_container_and_preserves_dimensions_and_matte() {
        if !available() {
            return;
        }
        let pixels = RgbaImage::from_fn(96, 64, |x, _| {
            image::Rgba([
                220,
                20,
                90,
                if x < 32 {
                    0
                } else if x < 64 {
                    128
                } else {
                    255
                },
            ])
        });
        let bytes = encode(&pixels, 95, [16, 96, 48]).unwrap();
        assert!(is_heif(&bytes));
        assert_ne!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.dimensions(), (96, 64));
        for x in [16, 48, 80] {
            let p = pixels.get_pixel(x, 20);
            let a = u32::from(p[3]);
            for c in 0..3 {
                let expected =
                    ((u32::from(p[c]) * a + u32::from([16u8, 96, 48][c]) * (255 - a) + 127) / 255)
                        as u8;
                assert!(decoded.get_pixel(x, 20)[c].abs_diff(expected) < 12);
            }
            assert_eq!(decoded.get_pixel(x, 20)[3], 255);
        }
    }
    #[test]
    fn native_decoder_rejects_png_disguised_as_heif() {
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[255u8, 0, 0, 255], 1, 1, ExtendedColorType::Rgba8)
            .unwrap();
        assert!(decode(&png).is_err());
    }
}
