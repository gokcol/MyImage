//! Flattened copies for sharing. Editable projects keep their original pixels
//! and separate annotations; exporting never changes either or their save state.

use image::{ExtendedColorType, ImageEncoder, RgbImage, RgbaImage};
use std::{
    io::{BufWriter, Seek, Write},
    path::{Path, PathBuf},
};

use crate::{document::Document, pdf_export::PdfPageSize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExportFormat {
    #[default]
    Png,
    Jpeg,
    WebP,
    Tiff,
    Pdf,
}

impl ExportFormat {
    pub const ALL: [Self; 5] = [Self::Png, Self::Jpeg, Self::WebP, Self::Tiff, Self::Pdf];

    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::WebP => "WebP",
            Self::Tiff => "TIFF",
            Self::Pdf => "PDF",
        }
    }

    pub fn extension(self) -> &'static str {
        self.extensions()[0]
    }

    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Png => &["png"],
            Self::Jpeg => &["jpg", "jpeg"],
            Self::WebP => &["webp"],
            Self::Tiff => &["tiff", "tif"],
            Self::Pdf => &["pdf"],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ExportOptions {
    pub format: ExportFormat,
    pub jpeg_quality: u8,
    pub jpeg_background: [u8; 3],
    pub pdf_page_size: PdfPageSize,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            format: ExportFormat::Png,
            jpeg_quality: 90,
            jpeg_background: [255; 3],
            pdf_page_size: PdfPageSize::Image,
        }
    }
}

fn validate_extension(path: &Path, format: ExportFormat) -> Result<(), String> {
    if path.extension().is_some_and(|extension| {
        format
            .extensions()
            .iter()
            .any(|allowed| extension.eq_ignore_ascii_case(allowed))
    }) {
        Ok(())
    } else {
        Err(format!(
            "Please use a .{} filename for {} export. Save project creates an editable .myimage file.",
            format.extension(),
            format.label()
        ))
    }
}

/// Complete an extension-less filename only if doing so cannot bypass the
/// native save dialog's overwrite confirmation for the resulting full path.
pub fn checked_export_path(mut path: PathBuf, format: ExportFormat) -> Result<PathBuf, String> {
    if path.file_name().is_none() {
        return Err("Please choose a filename for the export.".into());
    }
    if path.extension().is_none() {
        path.set_extension(format.extension());
        if path.exists() {
            return Err(format!(
                "{} already exists. Choose its full filename in the save dialog to confirm replacement.",
                path.display()
            ));
        }
    }
    validate_extension(&path, format)?;
    Ok(path)
}

/// Render a snapshot, then publish only a completely encoded and synced file.
/// Call on an I/O worker: large captures and annotations can take time to encode.
pub fn save(document: &Document, path: &Path, options: ExportOptions) -> Result<(), String> {
    validate_extension(path, options.format)?;
    if options.format == ExportFormat::Jpeg && !(1..=100).contains(&options.jpeg_quality) {
        return Err("JPEG quality must be between 1 and 100.".into());
    }
    if options.format == ExportFormat::WebP
        && document.output_size().into_iter().any(|side| side > 16_384)
    {
        return Err(
            "WebP supports images up to 16,384 pixels on each side. Choose PNG or TIFF for this image."
                .into(),
        );
    }
    let image = crate::render::flatten(document)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("Cannot create an export in {}: {error}", parent.display()))?;
    {
        let mut writer = BufWriter::new(temporary.as_file_mut());
        encode(&image, options, &mut writer)
            .map_err(|error| format!("{} export failed: {error}", options.format.label()))?;
        writer
            .flush()
            .map_err(|error| format!("Cannot finish the export: {error}"))?;
    }
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("Cannot flush the export to disk: {error}"))?;
    temporary
        .persist(path)
        .map_err(|error| format!("Cannot export {}: {}", path.display(), error.error))?;
    Ok(())
}

fn encode(
    image: &RgbaImage,
    options: ExportOptions,
    writer: &mut (impl Write + Seek),
) -> Result<(), String> {
    let (width, height) = image.dimensions();
    match options.format {
        ExportFormat::Png => image::codecs::png::PngEncoder::new(writer)
            .write_image(image.as_raw(), width, height, ExtendedColorType::Rgba8)
            .map_err(|error| error.to_string()),
        ExportFormat::Jpeg => {
            let rgb = composite_opaque(image, options.jpeg_background);
            image::codecs::jpeg::JpegEncoder::new_with_quality(writer, options.jpeg_quality)
                .encode(rgb.as_raw(), width, height, ExtendedColorType::Rgb8)
                .map_err(|error| error.to_string())
        }
        ExportFormat::WebP => image::codecs::webp::WebPEncoder::new_lossless(writer)
            .write_image(image.as_raw(), width, height, ExtendedColorType::Rgba8)
            .map_err(|error| error.to_string()),
        ExportFormat::Tiff => write_tiff(image, writer).map_err(|error| error.to_string()),
        ExportFormat::Pdf => crate::pdf_export::write_pdf(image, options.pdf_page_size, writer),
    }
}

fn write_tiff(image: &RgbaImage, writer: &mut (impl Write + Seek)) -> tiff::TiffResult<()> {
    use tiff::{
        encoder::{Compression, DeflateLevel, Predictor, TiffEncoder, colortype::RGBA8},
        tags::{ExtraSamples, Tag},
    };

    let mut encoder = TiffEncoder::new(writer)?
        .with_compression(Compression::Deflate(DeflateLevel::Balanced))
        .with_predictor(Predictor::Horizontal);
    let mut encoded_image = encoder.new_image::<RGBA8>(image.width(), image.height())?;
    // RGBA8 already declares four samples. The extra-sample setter would add a
    // fifth; tag its existing fourth sample explicitly as straight alpha.
    encoded_image.encoder().write_tag(
        Tag::ExtraSamples,
        &[ExtraSamples::UnassociatedAlpha.to_u16()][..],
    )?;
    encoded_image.write_data(image.as_raw())
}

fn composite_opaque(image: &RgbaImage, background: [u8; 3]) -> RgbImage {
    RgbImage::from_fn(image.width(), image.height(), |x, y| {
        let pixel = image.get_pixel(x, y);
        let alpha = u32::from(pixel[3]);
        image::Rgb(std::array::from_fn(|channel| {
            ((u32::from(pixel[channel]) * alpha
                + u32::from(background[channel]) * (255 - alpha)
                + 127)
                / 255) as u8
        }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Annotation, AnnotationKind, Point, load_project, save_project};
    use image::{ImageFormat, ImageReader, Rgba};
    use std::{fs, sync::Arc};

    fn sample_document() -> Document {
        let image = RgbaImage::from_fn(96, 64, |x, y| {
            Rgba([(x * 2) as u8, (y * 3) as u8, 140, (x * 255 / 95) as u8])
        });
        Document {
            geometry: Default::default(),
            image: Arc::new(image),
            annotations: vec![
                Annotation {
                    kind: AnnotationKind::Rectangle,
                    start: Point { x: 15.0, y: 12.0 },
                    end: Point { x: 80.0, y: 48.0 },
                    color: [245, 30, 60, 200],
                    stroke_width: 3.0,
                    fill: true,
                    text: String::new(),
                    font_size: 16.0,
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
                },
                Annotation {
                    kind: AnnotationKind::Text,
                    start: Point { x: 20.0, y: 20.0 },
                    end: Point { x: 20.0, y: 20.0 },
                    color: [255, 255, 255, 255],
                    stroke_width: 2.0,
                    fill: false,
                    text: "Edit me".into(),
                    font_size: 14.0,
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
                },
            ],
        }
    }

    fn options(format: ExportFormat) -> ExportOptions {
        ExportOptions {
            format,
            ..Default::default()
        }
    }

    #[test]
    fn lossless_formats_include_annotations_and_preserve_all_rgba_pixels() {
        let directory = tempfile::tempdir().unwrap();
        let document = sample_document();
        let flattened = crate::render::flatten(&document).unwrap();
        assert_ne!(flattened, *document.image);
        assert_eq!(flattened.get_pixel(0, 0)[3], 0);
        for (format, expected) in [
            (ExportFormat::Png, ImageFormat::Png),
            (ExportFormat::WebP, ImageFormat::WebP),
            (ExportFormat::Tiff, ImageFormat::Tiff),
        ] {
            let path = directory
                .path()
                .join(format!("copy.{}", format.extension()));
            save(&document, &path, options(format)).unwrap();
            let bytes = fs::read(&path).unwrap();
            assert_eq!(image::guess_format(&bytes).unwrap(), expected);
            let decoded = ImageReader::open(&path)
                .unwrap()
                .decode()
                .unwrap()
                .into_rgba8();
            assert_eq!(decoded, flattened, "{} lost RGBA data", format.label());
        }
    }

    #[test]
    fn jpeg_flattens_transparency_against_the_selected_background() {
        let directory = tempfile::tempdir().unwrap();
        let document = Document {
            geometry: Default::default(),
            image: Arc::new(RgbaImage::from_fn(96, 32, |x, _| {
                Rgba([
                    200,
                    40,
                    120,
                    if x < 32 {
                        0
                    } else if x < 64 {
                        128
                    } else {
                        255
                    },
                ])
            })),
            annotations: Vec::new(),
        };
        for (index, background) in [[255; 3], [16, 96, 48]].into_iter().enumerate() {
            let path = directory.path().join(format!("matte-{index}.jpeg"));
            save(
                &document,
                &path,
                ExportOptions {
                    format: ExportFormat::Jpeg,
                    jpeg_quality: 95,
                    jpeg_background: background,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(
                image::guess_format(&fs::read(&path).unwrap()).unwrap(),
                ImageFormat::Jpeg
            );
            let decoded = image::open(path).unwrap().into_rgb8();
            assert_eq!(decoded.dimensions(), (96, 32));
            let expected = composite_opaque(&document.image, background);
            for x in [16, 48, 80] {
                for channel in 0..3 {
                    assert!(
                        decoded.get_pixel(x, 16)[channel]
                            .abs_diff(expected.get_pixel(x, 16)[channel])
                            <= 3,
                        "JPEG matte mismatch at {x}, channel {channel}"
                    );
                }
            }
        }
    }

    #[test]
    fn tiff_declares_straight_alpha_and_lossless_deflate_compression() {
        use tiff::tags::{CompressionMethod, ExtraSamples, Tag};

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("alpha.tiff");
        save(&sample_document(), &path, options(ExportFormat::Tiff)).unwrap();
        let mut decoded = tiff::decoder::Decoder::new(fs::File::open(path).unwrap()).unwrap();
        assert_eq!(
            decoded.get_tag_u16_vec(Tag::ExtraSamples).unwrap(),
            vec![ExtraSamples::UnassociatedAlpha.to_u16()]
        );
        assert_eq!(decoded.get_tag_u32(Tag::SamplesPerPixel).unwrap(), 4);
        assert_eq!(
            decoded.get_tag_u32(Tag::Compression).unwrap(),
            u32::from(CompressionMethod::Deflate.to_u16())
        );
    }

    #[test]
    fn every_export_leaves_the_project_and_its_annotations_editable() {
        let directory = tempfile::tempdir().unwrap();
        let project_path = directory.path().join("original.myimage");
        let document = sample_document();
        save_project(&document, &project_path).unwrap();
        let original_bytes = fs::read(&project_path).unwrap();
        for format in ExportFormat::ALL {
            let path = directory
                .path()
                .join(format!("copy.{}", format.extension()));
            save(&document, &path, options(format)).unwrap();
            assert_eq!(fs::read(&project_path).unwrap(), original_bytes);
            let mut reopened = load_project(&project_path).unwrap();
            assert_eq!(*reopened.image, *document.image);
            assert_eq!(reopened.annotations, document.annotations);
            reopened.annotations[1].text = format!("Editable after {}", format.label());
            let changed_path = directory.path().join("edited.myimage");
            save_project(&reopened, &changed_path).unwrap();
            assert_eq!(
                load_project(&changed_path).unwrap().annotations,
                reopened.annotations
            );
        }
    }

    #[test]
    fn invalid_settings_and_render_failures_keep_existing_exports_intact() {
        let directory = tempfile::tempdir().unwrap();
        let mut document = sample_document();
        let jpg = directory.path().join("existing.jpg");
        fs::write(&jpg, b"existing JPEG").unwrap();
        for quality in [0, 101, 255] {
            assert!(
                save(
                    &document,
                    &jpg,
                    ExportOptions {
                        jpeg_quality: quality,
                        ..options(ExportFormat::Jpeg)
                    }
                )
                .is_err()
            );
            assert_eq!(fs::read(&jpg).unwrap(), b"existing JPEG");
        }
        let png = directory.path().join("existing.png");
        fs::write(&png, b"existing PNG").unwrap();
        document.annotations[0].start.x = f32::NAN;
        assert!(save(&document, &png, options(ExportFormat::Png)).is_err());
        assert_eq!(fs::read(&png).unwrap(), b"existing PNG");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn webp_dimension_limit_is_reported_without_replacing_destination() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("wide.webp");
        fs::write(&path, b"previous WebP").unwrap();
        let document = Document {
            geometry: Default::default(),
            image: Arc::new(RgbaImage::new(16_385, 1)),
            annotations: Vec::new(),
        };
        assert!(
            save(&document, &path, options(ExportFormat::WebP))
                .unwrap_err()
                .contains("16,384")
        );
        assert_eq!(fs::read(path).unwrap(), b"previous WebP");
    }

    #[test]
    fn successful_export_replaces_atomically_and_publish_failures_leave_no_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let document = sample_document();
        let path = directory.path().join("existing.png");
        fs::write(&path, b"old contents").unwrap();
        save(&document, &path, options(ExportFormat::Png)).unwrap();
        assert_eq!(
            image::open(&path).unwrap().into_rgba8(),
            crate::render::flatten(&document).unwrap()
        );

        let blocked = directory.path().join("folder.png");
        fs::create_dir(&blocked).unwrap();
        fs::write(blocked.join("keep"), b"untouched").unwrap();
        assert!(save(&document, &blocked, options(ExportFormat::Png)).is_err());
        assert_eq!(fs::read(blocked.join("keep")).unwrap(), b"untouched");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
        let missing = directory.path().join("missing/copy.png");
        assert!(save(&document, &missing, options(ExportFormat::Png)).is_err());
        assert!(!missing.exists());
    }

    #[test]
    fn extension_aliases_and_case_are_accepted_but_project_paths_are_protected() {
        let directory = tempfile::tempdir().unwrap();
        let document = sample_document();
        for format in ExportFormat::ALL {
            for extension in format.extensions() {
                let path = directory
                    .path()
                    .join(format!("capture.{}", extension.to_uppercase()));
                assert_eq!(checked_export_path(path.clone(), format).unwrap(), path);
                save(&document, &path, options(format)).unwrap();
            }
        }
        let project = directory.path().join("keep.myimage");
        fs::write(&project, b"editable project").unwrap();
        for format in ExportFormat::ALL {
            assert!(checked_export_path(project.clone(), format).is_err());
            assert!(save(&document, &project, options(format)).is_err());
            assert_eq!(fs::read(&project).unwrap(), b"editable project");
        }
        assert!(
            checked_export_path(directory.path().join("wrong.png"), ExportFormat::Jpeg).is_err()
        );
        assert!(
            save(
                &document,
                &directory.path().join("wrong.png"),
                options(ExportFormat::Jpeg)
            )
            .is_err()
        );
    }

    #[test]
    fn inferred_extensions_cannot_bypass_overwrite_confirmation() {
        let directory = tempfile::tempdir().unwrap();
        for format in ExportFormat::ALL {
            let bare = directory.path().join(format!("capture-{}", format.label()));
            let full = bare.with_extension(format.extension());
            assert_eq!(checked_export_path(bare.clone(), format).unwrap(), full);
            fs::write(&full, b"keep").unwrap();
            assert!(
                checked_export_path(bare, format)
                    .unwrap_err()
                    .contains("confirm replacement")
            );
            assert_eq!(fs::read(&full).unwrap(), b"keep");
        }
        assert!(checked_export_path(PathBuf::new(), ExportFormat::Png).is_err());
    }

    #[test]
    fn all_exports_use_output_dimensions_and_preserve_original_project() {
        use crate::geometry::{CropRect, ImageGeometry};
        let directory = tempfile::tempdir().unwrap();
        let mut document = sample_document();
        document.geometry = ImageGeometry {
            effects: Default::default(),
            viewport: None,
            flip_horizontal: false,
            flip_vertical: false,
            canvas: None,
            cutouts: [None; 16],
            crop: Some(CropRect {
                x: 5,
                y: 7,
                width: 80,
                height: 50,
            }),
            quarter_turns: 1,
            resize: Some([60, 100]),
        };
        let project = directory.path().join("editable.myimage");
        save_project(&document, &project).unwrap();
        let before = fs::read(&project).unwrap();
        let state = document.edit_state();
        let flattened = crate::render::flatten(&document).unwrap();
        for format in ExportFormat::ALL {
            let path = directory
                .path()
                .join(format!("transformed.{}", format.extension()));
            save(&document, &path, options(format)).unwrap();
            if format == ExportFormat::Pdf {
                let bytes = fs::read(&path).unwrap();
                let source = String::from_utf8_lossy(&bytes);
                assert!(source.contains("/MediaBox [0 0 45 75]"));
            } else {
                let image = image::open(&path).unwrap().into_rgba8();
                assert_eq!(image.dimensions(), (60, 100));
                if format != ExportFormat::Jpeg {
                    assert_eq!(image, flattened);
                }
            }
            assert_eq!(document.edit_state(), state);
            assert_eq!(fs::read(&project).unwrap(), before);
        }
        let reopened = load_project(&project).unwrap();
        assert_eq!(reopened.image, document.image);
        assert_eq!(reopened.edit_state(), state);
        assert_eq!(crate::render::flatten(&reopened).unwrap(), flattened);
    }

    #[test]
    fn webp_limits_follow_cropped_and_resized_output_not_original() {
        use crate::geometry::{CropRect, ImageGeometry};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cropped.webp");
        let mut document = Document {
            image: Arc::new(RgbaImage::new(16_385, 1)),
            annotations: Vec::new(),
            geometry: ImageGeometry {
                flip_horizontal: false,
                flip_vertical: false,
                canvas: None,
                crop: Some(CropRect {
                    x: 16_300,
                    y: 0,
                    width: 85,
                    height: 1,
                }),
                ..Default::default()
            },
        };
        save(&document, &path, options(ExportFormat::WebP)).unwrap();
        assert_eq!(
            image::open(&path).unwrap().into_rgba8().dimensions(),
            (85, 1)
        );
        let before = fs::read(&path).unwrap();
        document.geometry.resize = Some([16_385, 1]);
        assert!(
            save(&document, &path, options(ExportFormat::WebP))
                .unwrap_err()
                .contains("16,384")
        );
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    /// Generate review artifacts only when explicitly invoked with a destination.
    #[test]
    #[ignore = "manual image/PDF visual review fixture"]
    fn export_review_fixture() {
        let directory = std::env::var_os("MYIMAGE_EXPORT_REVIEW_DIR")
            .map(PathBuf::from)
            .expect("set MYIMAGE_EXPORT_REVIEW_DIR to the review artifact directory");
        fs::create_dir_all(&directory).unwrap();
        let document = review_document();
        save_project(&document, &directory.join("export-review.myimage")).unwrap();
        for format in ExportFormat::ALL {
            let path = directory.join(format!("export-review.{}", format.extension()));
            save(&document, &path, options(format)).unwrap();
        }
        for page_size in [PdfPageSize::A4, PdfPageSize::Letter] {
            let path = directory.join(format!("export-review-{page_size:?}.pdf"));
            save(
                &document,
                &path,
                ExportOptions {
                    pdf_page_size: page_size,
                    ..options(ExportFormat::Pdf)
                },
            )
            .unwrap();
        }
    }
    fn review_document() -> Document {
        let mut document = sample_document();
        document.image = Arc::new(RgbaImage::from_fn(800, 500, |x, y| {
            let alpha = if !(24..776).contains(&x) || !(24..476).contains(&y) {
                0
            } else {
                255
            };
            Rgba([(30 + x / 8) as u8, (70 + y / 4) as u8, 170, alpha])
        }));
        document.annotations[0].start = Point { x: 110.0, y: 115.0 };
        document.annotations[0].end = Point { x: 690.0, y: 360.0 };
        document.annotations[0].fill = false;
        document.annotations[0].stroke_width = 8.0;
        document.annotations[1].start = Point { x: 150.0, y: 160.0 };
        document.annotations[1].font_size = 42.0;
        document.annotations[1].text = "MyImage export\nTürkçe: ğüşiöç".into();
        document.annotations.push(Annotation {
            kind: AnnotationKind::Arrow,
            start: Point { x: 580.0, y: 410.0 },
            end: Point { x: 460.0, y: 315.0 },
            color: [255, 205, 80, 255],
            stroke_width: 9.0,
            fill: false,
            text: String::new(),
            font_size: 20.0,
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
        });
        document
    }

    #[test]
    #[ignore = "manual transformed image/PDF visual review fixture"]
    fn geometry_review_fixture() {
        use crate::geometry::{CropRect, ImageGeometry};
        let directory = std::env::var_os("MYIMAGE_GEOMETRY_REVIEW_DIR")
            .map(PathBuf::from)
            .expect("set MYIMAGE_GEOMETRY_REVIEW_DIR to the review artifact directory");
        fs::create_dir_all(&directory).unwrap();
        let mut document = review_document();
        document.geometry = ImageGeometry {
            effects: Default::default(),
            viewport: None,
            flip_horizontal: false,
            flip_vertical: false,
            canvas: None,
            cutouts: [None; 16],
            crop: Some(CropRect {
                x: 90,
                y: 90,
                width: 620,
                height: 350,
            }),
            quarter_turns: 1,
            resize: Some([420, 620]),
        };
        save_project(&document, &directory.join("geometry-review.myimage")).unwrap();
        for format in ExportFormat::ALL {
            save(
                &document,
                &directory.join(format!("geometry-review.{}", format.extension())),
                options(format),
            )
            .unwrap();
        }
        crate::render::thumbnail(&document, 384, 256)
            .unwrap()
            .save(directory.join("geometry-thumbnail.png"))
            .unwrap();
        document.geometry = Default::default();
        save(
            &document,
            &directory.join("geometry-original-with-annotations.png"),
            options(ExportFormat::Png),
        )
        .unwrap();
        document
            .image
            .save(directory.join("geometry-original.png"))
            .unwrap();
    }

    fn privacy_review_document() -> Document {
        let mut document = Document {
            image: Arc::new(RgbaImage::from_pixel(1000, 660, Rgba([242, 246, 251, 255]))),
            geometry: Default::default(),
            annotations: Vec::new(),
        };
        let text = |x, y, size, content: &str| Annotation {
            kind: AnnotationKind::Text,
            start: Point { x, y },
            end: Point { x, y },
            color: [28, 43, 68, 255],
            stroke_width: 2.0,
            fill: false,
            text: content.into(),
            font_size: size,
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
        };
        document.annotations = vec![
            text(54.0, 40.0, 36.0, "MyImage privacy tools"),
            text(
                54.0,
                100.0,
                20.0,
                "Synthetic test information. Original remains editable in the project.",
            ),
            text(54.0, 190.0, 28.0, "Blur"),
            text(320.0, 190.0, 32.0, "alex@example.test"),
            text(54.0, 320.0, 28.0, "Pixelate"),
            text(320.0, 320.0, 32.0, "TEST 1234 5678 9012"),
            text(
                54.0,
                448.0,
                22.0,
                "An annotation below a filter is hidden too:",
            ),
            text(
                54.0,
                600.0,
                19.0,
                "Move, resize, adjust strength or remove either privacy region.",
            ),
        ];
        // Readable test strings are baked into the original to exercise source
        // pixels as well as the separately editable text beneath a filter.
        document.image = Arc::new(crate::render::flatten(&document).unwrap());
        let effect = |kind, start, end, size| Annotation {
            kind,
            start,
            end,
            color: [0, 0, 0, 255],
            stroke_width: 2.0,
            fill: false,
            text: String::new(),
            font_size: 24.0,
            effect_size: size,
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
        };
        document.annotations = vec![
            effect(
                AnnotationKind::Blur,
                Point { x: 304.0, y: 174.0 },
                Point { x: 900.0, y: 248.0 },
                20,
            ),
            effect(
                AnnotationKind::Pixelate,
                Point { x: 304.0, y: 304.0 },
                Point { x: 900.0, y: 378.0 },
                20,
            ),
            text(60.0, 498.0, 30.0, "PRIVATE ANNOTATION"),
            effect(
                AnnotationKind::Pixelate,
                Point { x: 48.0, y: 486.0 },
                Point { x: 490.0, y: 550.0 },
                24,
            ),
            text(565.0, 498.0, 28.0, "Visible annotation above"),
        ];
        document
    }

    #[test]
    fn privacy_exports_include_filters_and_leave_original_project_adjustable() {
        let directory = tempfile::tempdir().unwrap();
        let mut document = sample_document();
        document.annotations.push(Annotation {
            kind: AnnotationKind::Pixelate,
            start: Point { x: 8.0, y: 8.0 },
            end: Point { x: 88.0, y: 56.0 },
            color: [0; 4],
            stroke_width: 2.0,
            fill: false,
            text: String::new(),
            font_size: 16.0,
            effect_size: 20,
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
        });
        let path = directory.path().join("privacy.myimage");
        save_project(&document, &path).unwrap();
        let before = fs::read(&path).unwrap();
        let expected = crate::render::flatten(&document).unwrap();
        for format in ExportFormat::ALL {
            let export = directory
                .path()
                .join(format!("privacy.{}", format.extension()));
            save(&document, &export, options(format)).unwrap();
            if format == ExportFormat::Pdf {
                let bytes = fs::read(&export).unwrap();
                assert!(bytes.starts_with(b"%PDF-1.7"));
                // The image stream is compressed directly from filtered RGB,
                // never the original project image or annotation strings.
                let stream = bytes
                    .windows(7)
                    .position(|window| window == b"stream\n")
                    .unwrap()
                    + 7;
                let mut decoder = flate2::read::ZlibDecoder::new(&bytes[stream..]);
                let mut rgb = Vec::new();
                std::io::Read::read_to_end(&mut decoder, &mut rgb).unwrap();
                assert_eq!(
                    rgb,
                    expected
                        .pixels()
                        .flat_map(|p| p.0[..3].to_vec())
                        .collect::<Vec<_>>()
                );
            } else {
                let pixels = image::open(&export).unwrap().into_rgba8();
                if format == ExportFormat::Jpeg {
                    let opaque = composite_opaque(&expected, [255; 3]);
                    let error: u64 = pixels
                        .pixels()
                        .zip(opaque.pixels())
                        .map(|(actual, expected)| {
                            (0..3)
                                .map(|channel| {
                                    u64::from(actual[channel].abs_diff(expected[channel]))
                                })
                                .sum::<u64>()
                        })
                        .sum();
                    assert!(error / (96 * 64 * 3) < 8);
                } else {
                    assert_eq!(pixels, expected);
                }
            }
            assert_eq!(fs::read(&path).unwrap(), before);
        }
        let mut reopened = load_project(&path).unwrap();
        assert_eq!(reopened.image, document.image);
        assert_eq!(reopened.annotations, document.annotations);
        reopened.annotations.last_mut().unwrap().effect_size = 8;
        assert_ne!(crate::render::flatten(&reopened).unwrap(), expected);
        reopened.annotations.pop();
        assert_eq!(
            crate::render::flatten(&reopened).unwrap(),
            crate::render::flatten(&sample_document()).unwrap()
        );
    }

    #[test]
    #[ignore = "manual privacy image/PDF visual review fixture"]
    fn privacy_review_fixture() {
        let directory = std::env::var_os("MYIMAGE_PRIVACY_REVIEW_DIR")
            .map(PathBuf::from)
            .expect("set MYIMAGE_PRIVACY_REVIEW_DIR to the review artifact directory");
        fs::create_dir_all(&directory).unwrap();
        let document = privacy_review_document();
        save_project(&document, &directory.join("privacy-review.myimage")).unwrap();
        document
            .image
            .save(directory.join("privacy-original.png"))
            .unwrap();
        for format in ExportFormat::ALL {
            save(
                &document,
                &directory.join(format!("privacy-review.{}", format.extension())),
                options(format),
            )
            .unwrap();
        }
        crate::render::privacy_preview(&document, 800)
            .unwrap()
            .save(directory.join("privacy-preview.png"))
            .unwrap();
        crate::render::thumbnail(&document, 384, 256)
            .unwrap()
            .save(directory.join("privacy-thumbnail.png"))
            .unwrap();
    }
}
