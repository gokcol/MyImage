//! Single-page PDF copies of rendered images. Editable annotations remain in projects.

use std::io::Write;

use flate2::{Compression, write::ZlibEncoder};
use image::RgbaImage;
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref};

const MAX_IMAGE_SIDE: u32 = 32_768;
const MAX_IMAGE_PIXELS: u64 = 100_000_000;
const POINTS_PER_PIXEL: f32 = 72.0 / 96.0;
const MAX_PAGE_POINTS: f32 = 14_400.0;
const PAGE_MARGIN: f32 = 24.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PdfPageSize {
    #[default]
    Image,
    A4,
    Letter,
}

impl PdfPageSize {
    pub const ALL: [Self; 3] = [Self::Image, Self::A4, Self::Letter];

    pub fn label(self) -> &'static str {
        match self {
            Self::Image => "Image size",
            Self::A4 => "A4",
            Self::Letter => "US Letter",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PageLayout {
    width: f32,
    height: f32,
    image_width: f32,
    image_height: f32,
    image_x: f32,
    image_y: f32,
}

impl PageLayout {
    fn new(width: u32, height: u32, page_size: PdfPageSize) -> Result<Self, String> {
        if width == 0
            || height == 0
            || width > MAX_IMAGE_SIDE
            || height > MAX_IMAGE_SIDE
            || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS
        {
            return Err("The image dimensions exceed the supported PDF export limits.".into());
        }

        let image_width = width as f32;
        let image_height = height as f32;
        if page_size == PdfPageSize::Image {
            // Keep the full image at 96 dpi unless it would exceed the standard
            // 200-inch PDF page limit. Only its placement changes, never its pixels.
            let scale = POINTS_PER_PIXEL.min(MAX_PAGE_POINTS / image_width.max(image_height));
            return Ok(Self {
                width: image_width * scale,
                height: image_height * scale,
                image_width: image_width * scale,
                image_height: image_height * scale,
                image_x: 0.0,
                image_y: 0.0,
            });
        }

        let (mut page_width, mut page_height) = match page_size {
            PdfPageSize::A4 => (210.0 * 72.0 / 25.4, 297.0 * 72.0 / 25.4),
            PdfPageSize::Letter => (612.0, 792.0),
            PdfPageSize::Image => unreachable!(),
        };
        if width > height {
            std::mem::swap(&mut page_width, &mut page_height);
        }
        let scale = ((page_width - 2.0 * PAGE_MARGIN) / image_width)
            .min((page_height - 2.0 * PAGE_MARGIN) / image_height);
        let fitted_width = image_width * scale;
        let fitted_height = image_height * scale;
        Ok(Self {
            width: page_width,
            height: page_height,
            image_width: fitted_width,
            image_height: fitted_height,
            image_x: (page_width - fitted_width) / 2.0,
            image_y: (page_height - fitted_height) / 2.0,
        })
    }
}

/// Writes a lossless, single-page PDF containing the supplied rendered pixels.
///
/// RGB samples retain their original resolution and row order. Transparency is
/// represented by an 8-bit soft mask, not baked against a background. Compression
/// consumes one row at a time rather than allocating another full raw image.
pub fn write_pdf(
    image: &RgbaImage,
    page_size: PdfPageSize,
    writer: &mut impl Write,
) -> Result<(), String> {
    let layout = PageLayout::new(image.width(), image.height(), page_size)?;
    let has_alpha = image.pixels().any(|pixel| pixel[3] != 255);

    let catalog_id = Ref::new(1);
    let page_tree_id = Ref::new(2);
    let page_id = Ref::new(3);
    let image_id = Ref::new(4);
    let mask_id = Ref::new(5);
    let content_id = Ref::new(6);
    let image_name = Name(b"Im1");

    let mut pdf = Pdf::new();
    pdf.catalog(catalog_id).pages(page_tree_id);
    pdf.pages(page_tree_id).kids([page_id]).count(1);
    let mut page = pdf.page(page_id);
    page.media_box(Rect::new(0.0, 0.0, layout.width, layout.height));
    page.parent(page_tree_id);
    page.contents(content_id);
    page.resources().x_objects().pair(image_name, image_id);
    page.finish();

    // Scope each compressed stream so RGB and mask buffers do not coexist.
    // pdf-writer owns the final serialized buffer, including copied streams.
    {
        let rgb = compress_samples(image, false)?;
        let mut object = pdf.image_xobject(image_id, &rgb);
        object.filter(Filter::FlateDecode);
        object.width(image.width() as i32);
        object.height(image.height() as i32);
        object.color_space().device_rgb();
        object.bits_per_component(8);
        if has_alpha {
            object.s_mask(mask_id);
        }
    }
    if has_alpha {
        let alpha = compress_samples(image, true)?;
        let mut mask = pdf.image_xobject(mask_id, &alpha);
        mask.filter(Filter::FlateDecode);
        mask.width(image.width() as i32);
        mask.height(image.height() as i32);
        mask.color_space().device_gray();
        mask.bits_per_component(8);
    }

    let mut content = Content::new();
    content.save_state();
    // Image XObjects already map their first sample row to the top of the
    // image. Positive scales preserve that orientation on PDF's bottom-up page.
    content.transform([
        layout.image_width,
        0.0,
        0.0,
        layout.image_height,
        layout.image_x,
        layout.image_y,
    ]);
    content.x_object(image_name);
    content.restore_state();
    pdf.stream(content_id, &content.finish());

    writer
        .write_all(&pdf.finish())
        .map_err(|error| format!("Could not write the PDF: {error}"))
}

fn compress_samples(image: &RgbaImage, alpha_only: bool) -> Result<Vec<u8>, String> {
    let channels = if alpha_only { 1 } else { 3 };
    let mut row = Vec::with_capacity(image.width() as usize * channels);
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    for pixels in image.rows() {
        row.clear();
        for pixel in pixels {
            if alpha_only {
                row.push(pixel[3]);
            } else {
                row.extend_from_slice(&pixel.0[..3]);
            }
        }
        encoder
            .write_all(&row)
            .map_err(|error| format!("Could not compress the PDF image: {error}"))?;
    }
    encoder
        .finish()
        .map_err(|error| format!("Could not finish compressing the PDF image: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::ZlibDecoder;
    use image::Rgba;
    use std::io::{self, Read};

    fn encode(image: &RgbaImage, page_size: PdfPageSize) -> Vec<u8> {
        let mut bytes = Vec::new();
        write_pdf(image, page_size, &mut bytes).unwrap();
        bytes
    }

    // Inspect objects via the real cross-reference table, not a substring search
    // which could accidentally match compressed image data.
    fn objects(pdf: &[u8]) -> Vec<Option<&[u8]>> {
        assert!(pdf.starts_with(b"%PDF-1.7\n"));
        assert!(pdf.ends_with(b"%%EOF"));
        let start = pdf
            .windows(b"startxref\n".len())
            .rposition(|window| window == b"startxref\n")
            .unwrap()
            + b"startxref\n".len();
        let offset: usize = std::str::from_utf8(&pdf[start..])
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let mut lines = std::str::from_utf8(&pdf[offset..]).unwrap().lines();
        assert_eq!(lines.next(), Some("xref"));
        let subsection: Vec<_> = lines.next().unwrap().split_whitespace().collect();
        assert_eq!(subsection[0], "0");
        let count: usize = subsection[1].parse().unwrap();
        let offsets: Vec<Option<usize>> = lines
            .by_ref()
            .take(count)
            .enumerate()
            .map(|(id, line)| {
                let fields: Vec<_> = line.split_whitespace().collect();
                if fields[2] == "n" {
                    let offset: usize = fields[0].parse().unwrap();
                    assert!(pdf[offset..].starts_with(format!("{id} 0 obj\n").as_bytes()));
                    Some(offset)
                } else {
                    assert_eq!(fields[2], "f");
                    None
                }
            })
            .collect();
        assert_eq!(lines.next(), Some("trailer"));
        assert!(lines.any(|line| line.trim() == "/Root 1 0 R"));
        offsets
            .iter()
            .map(|start| {
                start.map(|start| {
                    let end = offsets
                        .iter()
                        .flatten()
                        .copied()
                        .filter(|&end| end > start)
                        .min()
                        .unwrap_or(offset);
                    &pdf[start..end]
                })
            })
            .collect()
    }

    fn stream(object: &[u8]) -> (&str, &[u8]) {
        let delimiter = b"stream\n";
        let start = object
            .windows(delimiter.len())
            .position(|part| part == delimiter)
            .unwrap();
        let dictionary = std::str::from_utf8(&object[..start]).unwrap();
        let length: usize = dictionary
            .split("/Length ")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let start = start + delimiter.len();
        assert!(object[start + length..].starts_with(b"\nendstream"));
        (dictionary, &object[start..start + length])
    }

    fn decompress(stream: &[u8]) -> Vec<u8> {
        let mut decoded = Vec::new();
        ZlibDecoder::new(stream).read_to_end(&mut decoded).unwrap();
        decoded
    }

    fn close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
    }

    #[test]
    fn rgba_pdf_has_valid_references_and_lossless_top_to_bottom_samples() {
        let image = RgbaImage::from_raw(
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 31, 63, 127, 64,
            ],
        )
        .unwrap();
        let pdf = encode(&image, PdfPageSize::Image);
        let objects = objects(&pdf);
        assert_eq!(objects.len(), 7);
        assert!(
            std::str::from_utf8(objects[1].unwrap())
                .unwrap()
                .contains("/Pages 2 0 R")
        );
        let pages = std::str::from_utf8(objects[2].unwrap()).unwrap();
        assert!(pages.contains("/Kids [3 0 R]"));
        assert!(pages.contains("/Count 1"));
        let page = std::str::from_utf8(objects[3].unwrap()).unwrap();
        for reference in [
            "/Parent 2 0 R",
            "/Contents 6 0 R",
            "/Im1 4 0 R",
            "/MediaBox [0 0 1.5 1.5]",
        ] {
            assert!(page.contains(reference), "Missing {reference}");
        }
        let (rgb_dict, rgb) = stream(objects[4].unwrap());
        for field in [
            "/Width 2",
            "/Height 2",
            "/ColorSpace /DeviceRGB",
            "/BitsPerComponent 8",
            "/Filter /FlateDecode",
            "/SMask 5 0 R",
        ] {
            assert!(rgb_dict.contains(field), "Missing {field}");
        }
        assert_eq!(
            decompress(rgb),
            vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 31, 63, 127]
        );
        let (mask_dict, alpha) = stream(objects[5].unwrap());
        assert!(mask_dict.contains("/ColorSpace /DeviceGray"));
        assert!(mask_dict.contains("/BitsPerComponent 8"));
        assert!(mask_dict.contains("/Filter /FlateDecode"));
        assert!(!mask_dict.contains("/SMask"));
        assert_eq!(decompress(alpha), vec![255, 128, 0, 64]);
        let (_, content) = stream(objects[6].unwrap());
        assert_eq!(
            std::str::from_utf8(content).unwrap(),
            "q\n1.5 0 0 1.5 0 0 cm\n/Im1 Do\nQ"
        );
    }

    #[test]
    fn opaque_pdf_omits_unnecessary_alpha_stream() {
        let image = RgbaImage::from_pixel(96, 192, Rgba([13, 29, 71, 255]));
        let pdf = encode(&image, PdfPageSize::Image);
        let objects = objects(&pdf);
        assert!(objects[5].is_none());
        let (dictionary, rgb) = stream(objects[4].unwrap());
        assert!(!dictionary.contains("/SMask"));
        assert_eq!(decompress(rgb), [13, 29, 71].repeat(96 * 192));
        assert!(
            std::str::from_utf8(objects[3].unwrap())
                .unwrap()
                .contains("/MediaBox [0 0 72 144]")
        );
    }

    #[test]
    fn paper_pages_auto_orient_and_center_the_entire_image_inside_margins() {
        for page_size in [PdfPageSize::A4, PdfPageSize::Letter] {
            for (width, height) in [(1600, 900), (900, 1600), (900, 900)] {
                let layout = PageLayout::new(width, height, page_size).unwrap();
                assert_eq!(layout.width > layout.height, width > height);
                assert!(layout.image_x >= PAGE_MARGIN - 0.001);
                assert!(layout.image_y >= PAGE_MARGIN - 0.001);
                assert!(
                    (layout.image_x - PAGE_MARGIN).abs() < 0.001
                        || (layout.image_y - PAGE_MARGIN).abs() < 0.001
                );
                close(layout.image_x * 2.0 + layout.image_width, layout.width);
                close(layout.image_y * 2.0 + layout.image_height, layout.height);
                close(
                    layout.image_width / layout.image_height,
                    width as f32 / height as f32,
                );
                match page_size {
                    PdfPageSize::A4 => {
                        close(layout.width.min(layout.height), 210.0 * 72.0 / 25.4);
                        close(layout.width.max(layout.height), 297.0 * 72.0 / 25.4);
                    }
                    PdfPageSize::Letter => {
                        close(layout.width.min(layout.height), 612.0);
                        close(layout.width.max(layout.height), 792.0);
                    }
                    PdfPageSize::Image => unreachable!(),
                }
            }
        }
        let image = RgbaImage::from_pixel(16, 9, Rgba([255; 4]));
        let pdf = encode(&image, PdfPageSize::Letter);
        let objects = objects(&pdf);
        assert!(
            std::str::from_utf8(objects[3].unwrap())
                .unwrap()
                .contains("/MediaBox [0 0 792 612]")
        );
        assert_eq!(
            std::str::from_utf8(stream(objects[6].unwrap()).1).unwrap(),
            "q\n744 0 0 418.5 24 96.75 cm\n/Im1 Do\nQ"
        );
    }

    #[test]
    fn large_image_page_scales_proportionally_without_resampling() {
        let image = RgbaImage::from_pixel(32_768, 1, Rgba([1, 2, 3, 255]));
        let layout = PageLayout::new(image.width(), image.height(), PdfPageSize::Image).unwrap();
        close(layout.width, MAX_PAGE_POINTS);
        close(layout.height, MAX_PAGE_POINTS / 32_768.0);
        let pdf = encode(&image, PdfPageSize::Image);
        let objects = objects(&pdf);
        let (dictionary, rgb) = stream(objects[4].unwrap());
        assert!(dictionary.contains("/Width 32768"));
        assert_eq!(decompress(rgb), [1, 2, 3].repeat(32_768));
    }

    #[test]
    fn rejects_invalid_dimensions_before_writing_and_propagates_output_errors() {
        for (width, height) in [(0, 20), (20, 0), (32_769, 1), (1, 32_769), (10_001, 10_000)] {
            assert!(PageLayout::new(width, height, PdfPageSize::Image).is_err());
        }
        assert!(PageLayout::new(10_000, 10_000, PdfPageSize::Image).is_ok());
        let mut output = Vec::new();
        assert!(write_pdf(&RgbaImage::new(0, 20), PdfPageSize::Image, &mut output).is_err());
        assert!(output.is_empty());

        struct FailedWriter;
        impl Write for FailedWriter {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("simulated disk error"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let image = RgbaImage::from_pixel(1, 1, Rgba([255; 4]));
        let error = write_pdf(&image, PdfPageSize::Image, &mut FailedWriter).unwrap_err();
        assert!(error.contains("simulated disk error"));
    }
}
