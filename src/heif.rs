//! Real HEIF/HEIC encoding through installed operating-system codecs.
//! Native calls see only a flattened image; no editable project or source metadata is supplied.
use image::RgbaImage;
use std::{io::Read, path::Path, sync::OnceLock};
#[cfg(target_os = "macos")]
#[path = "heif/macos.rs"]
mod macos;
#[cfg(target_os = "windows")]
#[path = "heif/windows.rs"]
mod windows;

pub fn available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        #[cfg(target_os = "macos")]
        {
            macos::available()
        }
        #[cfg(target_os = "windows")]
        {
            windows::available()
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            false
        }
    })
}
/// HEIF is deliberately opaque in this app: both supported native encoders receive
/// the selected matte. This avoids promising alpha support unavailable in some codecs.
pub fn encode(image: &RgbaImage, quality: u8, background: [u8; 3]) -> Result<Vec<u8>, String> {
    crate::document::validate_dimensions(image.width(), image.height())?;
    if !(1..=100).contains(&quality) {
        return Err("HEIF quality must be between 1 and 100.".into());
    }
    #[cfg(target_os = "macos")]
    {
        macos::encode(image, quality, background)
    }
    #[cfg(target_os = "windows")]
    {
        windows::encode(image, quality, background)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (image, quality, background);
        Err(
            "HEIF encoding is not available on this operating system. Choose PNG, JPEG or WebP."
                .into(),
        )
    }
}
/// Read HEIF through the native decoder, with a file-size limit before any codec work.
pub fn load(path: &Path) -> Result<RgbaImage, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("Cannot open HEIF image: {e}"))?;
    const LIMIT: u64 = 512 * 1024 * 1024;
    if file.metadata().map_err(|e| e.to_string())?.len() > LIMIT {
        return Err("HEIF images larger than 512 MB are not supported.".into());
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("HEIF image exceeds the supported file-size limit.".into());
    }
    decode(&bytes)
}
pub fn decode(bytes: &[u8]) -> Result<RgbaImage, String> {
    #[cfg(target_os = "macos")]
    {
        macos::decode(bytes)
    }
    #[cfg(target_os = "windows")]
    {
        windows::decode(bytes)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = bytes;
        Err("HEIF decoding is not available on this operating system. Convert the image to PNG first.".into())
    }
}
