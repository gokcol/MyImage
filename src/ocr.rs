//! Local text recognition of the already-composited selected pixels.
use image::RgbaImage;
use std::{
    sync::{
        Arc, LazyLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

/// The native service is bounded even when an installed OCR model misbehaves.
pub const JOB_TIMEOUT: Duration = Duration::from_secs(30);
const CANCELED: &str = "Text recognition canceled.";
const TIMED_OUT: &str = "Text recognition took too long. Try a smaller region; if this repeats, quit and reopen MyImage.";
static NATIVE_BUSY: LazyLock<Arc<AtomicBool>> = LazyLock::new(|| Arc::new(AtomicBool::new(false)));

#[derive(Clone)]
struct Control {
    canceled: Arc<AtomicBool>,
    deadline: Instant,
}

impl Control {
    fn check(&self) -> Result<(), String> {
        if self.canceled.load(Ordering::Acquire) {
            Err(CANCELED.into())
        } else if Instant::now() >= self.deadline {
            Err(TIMED_OUT.into())
        } else {
            Ok(())
        }
    }
}

struct NativePermit(Arc<AtomicBool>);
impl Drop for NativePermit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[cfg(test)]
pub fn recognize(pixels: RgbaImage) -> Result<String, String> {
    recognize_cancellable(pixels, Arc::new(AtomicBool::new(false)))
}

/// Canceling the review or screen capture also asks the native request to stop. Only
/// one request may occupy the native engine at a time, including a canceled
/// request that the operating system has not yet unwound.
pub fn recognize_cancellable(
    pixels: RgbaImage,
    canceled: Arc<AtomicBool>,
) -> Result<String, String> {
    supervise(
        pixels,
        canceled,
        JOB_TIMEOUT,
        NATIVE_BUSY.clone(),
        recognize_native,
    )
}

fn recognize_native(pixels: RgbaImage, control: Control) -> Result<String, String> {
    let start = Instant::now();
    control.check()?;
    let pixels = prepare_cancellable(pixels, &control)?;
    log::info!(
        "OCR pixels prepared in {:?}; starting native recognition",
        start.elapsed()
    );
    control.check()?;
    #[cfg(target_os = "macos")]
    let text = macos::recognize_cancellable(&pixels, &control)?;
    #[cfg(target_os = "windows")]
    let text = windows::recognize_cancellable(&pixels, &control)?;
    control.check()?;
    Ok(text.trim().to_owned())
}

fn supervise(
    pixels: RgbaImage,
    canceled: Arc<AtomicBool>,
    timeout: Duration,
    busy: Arc<AtomicBool>,
    native: impl FnOnce(RgbaImage, Control) -> Result<String, String> + Send + 'static,
) -> Result<String, String> {
    let start = Instant::now();
    let control = Control {
        canceled,
        deadline: start + timeout,
    };
    control.check()?;
    busy.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| "Previous text recognition is still stopping. Please try again shortly; if it stays unavailable, quit and reopen MyImage.".to_string())?;
    let permit = NativePermit(busy);
    let worker_control = control.clone();
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("myimage-ocr-native".into())
        .spawn(move || {
            let result = native(pixels, worker_control);
            log::info!(
                "Native OCR finished after {:?}; successful: {}",
                start.elapsed(),
                result.is_ok()
            );
            drop(permit);
            let _ = tx.send(result);
        })
        .map_err(|error| format!("Could not start text recognition: {error}"))?;
    loop {
        match rx.recv_timeout(Duration::from_millis(25)) {
            Ok(result) => {
                control.check()?;
                return result;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("Text recognition stopped unexpectedly. Please try again.".into());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Err(error) = control.check() {
                    log::warn!("OCR ended after {:?}: {error}", start.elapsed());
                    return Err(error);
                }
            }
        }
    }
}

#[cfg(test)]
fn prepare(pixels: RgbaImage) -> Result<RgbaImage, String> {
    prepare_cancellable(
        pixels,
        &Control {
            canceled: Arc::new(AtomicBool::new(false)),
            deadline: Instant::now() + JOB_TIMEOUT,
        },
    )
}

fn prepare_cancellable(mut pixels: RgbaImage, control: &Control) -> Result<RgbaImage, String> {
    let (width, height) = pixels.dimensions();
    if width < 2 || height < 2 {
        return Err("Select a larger area containing text.".into());
    }
    crate::document::validate_dimensions(width, height)?;
    // Flatten transparency onto white so excluded lasso/magic-wand pixels never
    // leak through to recognition. Avoid premultiplied dark fringes when scaling.
    for row in pixels.rows_mut() {
        control.check()?;
        for pixel in row {
            let alpha = u32::from(pixel[3]);
            for channel in &mut pixel.0[..3] {
                *channel = ((u32::from(*channel) * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
            }
            pixel[3] = 255;
        }
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

    #[test]
    fn too_small_selection_returns_an_error_without_initializing_native_ocr() {
        assert!(
            recognize(RgbaImage::new(1, 10))
                .unwrap_err()
                .contains("larger area")
        );
    }

    #[test]
    fn timed_out_native_call_cannot_accumulate_more_workers() {
        let busy = Arc::new(AtomicBool::new(false));
        let (release, wait) = mpsc::channel();
        let result = supervise(
            RgbaImage::new(2, 2),
            Arc::new(AtomicBool::new(false)),
            Duration::from_millis(20),
            busy.clone(),
            move |_, _| {
                wait.recv().unwrap();
                Ok("Late result".into())
            },
        );
        assert_eq!(result.unwrap_err(), TIMED_OUT);
        assert!(busy.load(Ordering::Acquire));
        let second = supervise(
            RgbaImage::new(2, 2),
            Arc::new(AtomicBool::new(false)),
            Duration::from_secs(1),
            busy.clone(),
            |_, _| panic!("a second native worker must not start"),
        );
        assert!(second.unwrap_err().contains("still stopping"));
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while busy.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!busy.load(Ordering::Acquire));
        assert_eq!(
            supervise(
                RgbaImage::new(2, 2),
                Arc::new(AtomicBool::new(false)),
                Duration::from_secs(1),
                busy,
                |_, _| Ok("Fresh result".into()),
            )
            .unwrap(),
            "Fresh result"
        );
    }

    #[test]
    fn cancellation_returns_promptly_and_discards_late_results() {
        let canceled = Arc::new(AtomicBool::new(false));
        let cancel = canceled.clone();
        let busy = Arc::new(AtomicBool::new(false));
        let (release, wait) = mpsc::channel();
        let result = supervise(
            RgbaImage::new(2, 2),
            canceled.clone(),
            Duration::from_secs(1),
            busy.clone(),
            move |_, control| {
                cancel.store(true, Ordering::Release);
                wait.recv().unwrap();
                assert_eq!(control.check().unwrap_err(), CANCELED);
                Ok("Canceled text must never be delivered".into())
            },
        );
        assert_eq!(result.unwrap_err(), CANCELED);
        assert!(busy.load(Ordering::Acquire));
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while busy.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!busy.load(Ordering::Acquire));
        assert_eq!(
            supervise(
                RgbaImage::new(2, 2),
                canceled,
                Duration::from_secs(1),
                busy,
                |_, _| panic!("an already canceled request must not start"),
            )
            .unwrap_err(),
            CANCELED
        );
    }
}
