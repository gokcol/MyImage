//! Native Windows HEIF/HEIC support. Windows supplies the container/HEVC codec;
//! no third-party codec or executable is shipped with MyImage.
use image::RgbaImage;
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
use windows::{
    Foundation::{PropertyType, PropertyValue},
    Graphics::Imaging::{
        BitmapAlphaMode, BitmapDecoder, BitmapEncoder, BitmapPixelFormat, BitmapPropertySet,
        BitmapTransform, BitmapTypedValue, ColorManagementMode, ExifOrientationMode,
    },
    Storage::Streams::{DataReader, DataWriter, InMemoryRandomAccessStream},
    Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
    core::{Error, HSTRING},
};

const JOB_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const CODEC_HELP: &str = "Windows could not use its HEIF/HEVC codec. Install or update Microsoft's HEIF Image Extensions and, where required, HEVC Video Extensions, restart MyImage, or choose PNG/JPEG instead.";

// WinRT operation types come from windows-future through generated APIs, so a
// macro retains their concrete type without adding another direct dependency.
macro_rules! wait {
    ($call:expr,$deadline:expr,$action:expr) => {{
        let operation = $call.map_err(|error| native_error($action, error))?;
        loop {
            if operation
                .Status()
                .map_err(|error| native_error($action, error))?
                .0
                != 0
            {
                break;
            }
            if Instant::now() >= $deadline {
                let _ = operation.Cancel();
                return Err(format!(
                    "Windows HEIF {} timed out. Try a smaller image or choose PNG/JPEG instead.",
                    $action
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        operation
            .GetResults()
            .map_err(|error| native_error($action, error))?
    }};
}

/// The facade caches this probe, so a stalled platform enumerator can create at
/// most one short-lived background probe rather than a worker on every UI frame.
pub(super) fn available() -> bool {
    let (tx, rx) = mpsc::sync_channel(1);
    if std::thread::Builder::new()
        .name("myimage-heif-probe".into())
        .spawn(move || {
            let result = (|| {
                let _apartment = Apartment::new().ok()?;
                let id = BitmapEncoder::HeifEncoderId().ok()?;
                let codecs = BitmapEncoder::GetEncoderInformationEnumerator().ok()?;
                Some(
                    codecs
                        .into_iter()
                        .any(|codec| codec.CodecId().is_ok_and(|actual| actual == id)),
                )
            })()
            .unwrap_or(false);
            let _ = tx.send(result);
        })
        .is_err()
    {
        return false;
    }
    rx.recv_timeout(Duration::from_secs(2)).unwrap_or(false)
}

pub(super) fn encode(
    image: &RgbaImage,
    quality: u8,
    background: [u8; 3],
) -> Result<Vec<u8>, String> {
    crate::document::validate_dimensions(image.width(), image.height())?;
    if !(1..=100).contains(&quality) {
        return Err("HEIF quality must be between 1 and 100.".into());
    }
    // Caller runs this on its file-I/O worker. The apartment guard is declared
    // before every WinRT value, so all interfaces are dropped before uninitializing.
    let _apartment = Apartment::new()?;
    let deadline = Instant::now() + JOB_TIMEOUT;
    let stream =
        InMemoryRandomAccessStream::new().map_err(|e| native_error("prepare the output", e))?;
    let options =
        BitmapPropertySet::new().map_err(|e| native_error("prepare encoding options", e))?;
    let quality_value = PropertyValue::CreateSingle(f32::from(quality) / 100.)
        .map_err(|e| native_error("set image quality", e))?;
    let quality_value = BitmapTypedValue::Create(&quality_value, PropertyType::Single)
        .map_err(|e| native_error("set image quality", e))?;
    options
        .Insert(&HSTRING::from("ImageQuality"), &quality_value)
        .map_err(|e| native_error("set image quality", e))?;
    let id =
        BitmapEncoder::HeifEncoderId().map_err(|e| native_error("find the HEIF encoder", e))?;
    let encoder = wait!(
        BitmapEncoder::CreateWithEncodingOptionsAsync(id, &stream, &options),
        deadline,
        "initialize encoding"
    );
    encoder
        .SetIsThumbnailGenerated(false)
        .map_err(|e| native_error("disable thumbnails", e))?;
    // HEIF transparency support differs across Windows codecs. Use the selected
    // matte explicitly rather than silently dropping alpha or choosing black.
    let pixels = opaque_rgba(image, background);
    encoder
        .SetPixelData(
            BitmapPixelFormat::Rgba8,
            BitmapAlphaMode::Ignore,
            image.width(),
            image.height(),
            72.,
            72.,
            &pixels,
        )
        .map_err(|e| native_error("read image pixels", e))?;
    wait!(encoder.FlushAsync(), deadline, "finish encoding");
    let size = stream
        .Size()
        .map_err(|e| native_error("read the encoded size", e))?;
    if !(12..=MAX_FILE_BYTES).contains(&size) {
        return Err("Windows produced an empty or oversized HEIF image.".into());
    }
    let count = u32::try_from(size).map_err(|_| "The encoded HEIF image is too large.")?;
    let input = stream
        .GetInputStreamAt(0)
        .map_err(|e| native_error("open encoded pixels", e))?;
    let reader =
        DataReader::CreateDataReader(&input).map_err(|e| native_error("read encoded pixels", e))?;
    let loaded = wait!(reader.LoadAsync(count), deadline, "read encoded pixels");
    if loaded != count {
        return Err("Windows returned an incomplete HEIF image.".into());
    }
    let mut bytes = vec![0; count as usize];
    reader
        .ReadBytes(&mut bytes)
        .map_err(|e| native_error("read encoded pixels", e))?;
    if bytes.get(4..8) != Some(b"ftyp") {
        return Err("Windows did not produce a valid HEIF container.".into());
    }
    Ok(bytes)
}

/// Decode with native orientation and sRGB conversion. Validate both stored and
/// oriented dimensions before requesting a pixel buffer from the codec.
pub(super) fn decode(bytes: &[u8]) -> Result<RgbaImage, String> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("HEIF images larger than 512 MB are not supported.".into());
    }
    let _apartment = Apartment::new()?;
    let deadline = Instant::now() + JOB_TIMEOUT;
    let stream =
        InMemoryRandomAccessStream::new().map_err(|e| native_error("prepare image loading", e))?;
    let output = stream
        .GetOutputStreamAt(0)
        .map_err(|e| native_error("prepare image loading", e))?;
    let writer = DataWriter::CreateDataWriter(&output)
        .map_err(|e| native_error("prepare image loading", e))?;
    writer
        .WriteBytes(bytes)
        .map_err(|e| native_error("read HEIF bytes", e))?;
    let stored = wait!(writer.StoreAsync(), deadline, "read HEIF bytes");
    if stored as usize != bytes.len() {
        return Err("Windows could not read the complete HEIF image.".into());
    }
    writer
        .DetachStream()
        .map_err(|e| native_error("finish reading the image", e))?;
    stream
        .Seek(0)
        .map_err(|e| native_error("read HEIF pixels", e))?;
    let decoder = wait!(
        BitmapDecoder::CreateAsync(&stream),
        deadline,
        "open the image"
    );
    crate::document::validate_dimensions(
        decoder
            .PixelWidth()
            .map_err(|e| native_error("read image width", e))?,
        decoder
            .PixelHeight()
            .map_err(|e| native_error("read image height", e))?,
    )?;
    let width = decoder
        .OrientedPixelWidth()
        .map_err(|e| native_error("read oriented width", e))?;
    let height = decoder
        .OrientedPixelHeight()
        .map_err(|e| native_error("read oriented height", e))?;
    crate::document::validate_dimensions(width, height)?;
    let transform = BitmapTransform::new().map_err(|e| native_error("prepare orientation", e))?;
    let provider = wait!(
        decoder.GetPixelDataTransformedAsync(
            BitmapPixelFormat::Rgba8,
            BitmapAlphaMode::Straight,
            &transform,
            ExifOrientationMode::RespectExifOrientation,
            ColorManagementMode::ColorManageToSRgb
        ),
        deadline,
        "decode pixels"
    );
    let pixels = provider
        .DetachPixelData()
        .map_err(|e| native_error("read decoded pixels", e))?;
    if pixels.len() != width as usize * height as usize * 4 {
        return Err("Windows returned an invalid HEIF pixel buffer.".into());
    }
    RgbaImage::from_raw(width, height, pixels.to_vec())
        .ok_or_else(|| "Windows returned an invalid HEIF pixel buffer.".into())
}

fn opaque_rgba(image: &RgbaImage, background: [u8; 3]) -> Vec<u8> {
    let mut rgba = image.as_raw().clone();
    for pixel in rgba.chunks_exact_mut(4) {
        let alpha = u32::from(pixel[3]);
        for channel in 0..3 {
            pixel[channel] = ((u32::from(pixel[channel]) * alpha
                + u32::from(background[channel]) * (255 - alpha)
                + 127)
                / 255) as u8;
        }
        pixel[3] = 255;
    }
    rgba
}
fn native_error(action: &str, error: Error) -> String {
    format!("{CODEC_HELP} Could not {action}. ({error})")
}
struct Apartment;
impl Apartment {
    fn new() -> Result<Self, String> {
        // SAFETY: the caller/probe owns this worker thread. Each successful
        // initialization (including S_FALSE) is balanced exactly once below.
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
            .map_err(|e| native_error("initialize image processing", e))?;
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: this local guard exists only after successful initialization.
        unsafe { RoUninitialize() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_matte_keeps_opaque_colors_and_blends_partial_alpha() {
        let image = RgbaImage::from_raw(
            3,
            1,
            vec![220, 40, 120, 0, 220, 40, 120, 128, 220, 40, 120, 255],
        )
        .unwrap();
        let rgba = opaque_rgba(&image, [20, 80, 160]);
        assert_eq!(&rgba[..4], &[20, 80, 160, 255]);
        assert_eq!(&rgba[4..8], &[120, 60, 140, 255]);
        assert_eq!(&rgba[8..], &[220, 40, 120, 255]);
    }
    #[test]
    #[ignore = "requires Windows HEIF/HEVC encoding extensions on a native Windows runner"]
    fn native_heif_has_real_container_and_decodes_to_requested_dimensions() {
        assert!(
            available(),
            "Install the Windows HEIF/HEVC encoder before running this diagnostic."
        );
        let source = RgbaImage::from_fn(64, 48, |x, y| {
            image::Rgba([(x * 3) as u8, (y * 4) as u8, 160, 255])
        });
        let encoded = encode(&source, 95, [255; 3]).unwrap();
        assert_eq!(&encoded[4..8], b"ftyp");
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded.dimensions(), source.dimensions());
    }
}
