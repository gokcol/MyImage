//! Native still-image capture for macOS and Windows.
//!
//! macOS uses ScreenCaptureKit's SCScreenshotManager (macOS 14 or later).
//! Windows uses xcap 0.9.8's default GDI BitBlt backend. xcap also supplies
//! non-capturing monitor metadata on both systems.
//! Keep all cropping in the returned image's pixel coordinates: desktop bounds
//! have different units on the two operating systems.

use image::RgbaImage;

#[cfg(any(target_os = "macos", target_os = "windows"))]
use xcap::Monitor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum CaptureMode {
    #[default]
    Region,
    Window,
    FullScreen,
    Scrolling,
}
impl CaptureMode {
    pub const ALL: [Self; 4] = [
        Self::Region,
        Self::Window,
        Self::FullScreen,
        Self::Scrolling,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Region => "Region",
            Self::Window => "Window",
            Self::FullScreen => "Full screen",
            Self::Scrolling => "Scrolling",
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Self::Region => "Drag a rectangle around anything on screen.",
            Self::Window => "Point to a window, click to select, then capture.",
            Self::FullScreen => "Capture the entire selected display.",
            Self::Scrolling => {
                "Click a window or select part of it. Scroll down, then finish. Sections are collected automatically."
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub id: u32,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Enumerated front-to-back. Skip vanished, minimized and own-process windows.
pub fn windows() -> Result<Vec<WindowInfo>, String> {
    let windows = xcap::Window::all().map_err(|e| format!("Could not list windows: {e}"))?;
    Ok(windows
        .into_iter()
        .filter_map(|w| {
            if w.pid().ok()? == std::process::id() || w.is_minimized().ok()? {
                return None;
            }
            let width = w.width().ok()?;
            let height = w.height().ok()?;
            if width < 40 || height < 40 {
                return None;
            }
            let name = w
                .title()
                .ok()
                .filter(|s| !s.is_empty())
                .or_else(|| w.app_name().ok())?;
            Some(WindowInfo {
                id: w.id().ok()?,
                name,
                x: w.x().ok()?,
                y: w.y().ok()?,
                width,
                height,
            })
        })
        .collect())
}

pub fn capture_window(id: u32, pointer: bool) -> Result<RgbaImage, String> {
    #[cfg(target_os = "macos")]
    {
        macos::capture(id, true, pointer)
    }
    #[cfg(target_os = "windows")]
    {
        let window = xcap::Window::all()
            .map_err(capture_error)?
            .into_iter()
            .find(|w| w.id().ok() == Some(id))
            .ok_or("This window is no longer available.")?;
        let mut image = window.capture_image().map_err(capture_error)?;
        if pointer {
            windows_cursor::draw(
                &mut image,
                window.x().map_err(capture_error)?,
                window.y().map_err(capture_error)?,
            )?;
        }
        Ok(image)
    }
}

#[cfg(target_os = "windows")]
#[path = "capture_cursor.rs"]
mod windows_cursor;

/// A currently connected display. IDs are runtime handles; refresh the list
/// after a display is connected/disconnected instead of persisting an ID.
#[derive(Debug, Clone)]
pub struct DisplayInfo {
    pub id: u32,
    pub name: String,
    /// Desktop origin: Quartz points on macOS, physical pixels on Windows.
    /// Origin is the main display's upper-left; x grows right and y grows down.
    /// Displays left of or above the main display can have negative origins.
    pub x: i32,
    pub y: i32,
    /// Desktop extent in the same units as x/y: points on macOS, physical
    /// pixels on Windows. Do not use this as the captured image's pixel size.
    pub width: u32,
    pub height: u32,
    /// Backing-pixel scale on macOS; effective display DPI / 96 on Windows.
    /// The returned capture's actual dimensions remain authoritative.
    pub scale_factor: f32,
    pub is_primary: bool,
}

/// Enumerate displays without capturing pixels or requesting permission.
pub fn displays() -> Result<Vec<DisplayInfo>, String> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let monitors = Monitor::all().map_err(|error| {
            format!("Could not list displays. Check that a display is connected, then try again. {error}")
        })?;
        let mut result = Vec::with_capacity(monitors.len());

        for monitor in monitors {
            let id = monitor.id().map_err(|error| error.to_string())?;
            let field_error = |error: xcap::XCapError| {
                format!(
                    "Display {id} changed or could not be read. Refresh the display list and try again. {error}"
                )
            };
            // Avoid xcap::friendly_name here: its macOS implementation invokes
            // NSScreen with an unchecked main-thread marker. Enumeration may
            // legitimately happen on our background capture worker.
            let name = monitor.name().map_err(field_error)?;
            let width = monitor.width().map_err(field_error)?;
            let height = monitor.height().map_err(field_error)?;
            let scale_factor = monitor.scale_factor().map_err(field_error)?;
            if width == 0 || height == 0 || !scale_factor.is_finite() || scale_factor <= 0.0 {
                return Err(format!(
                    "Display {id} has no valid screen geometry. Refresh the display list after the display finishes connecting."
                ));
            }
            result.push(DisplayInfo {
                id,
                name: if name.trim().is_empty() {
                    format!("Display {id}")
                } else {
                    name
                },
                x: monitor.x().map_err(field_error)?,
                y: monitor.y().map_err(field_error)?,
                width,
                height,
                scale_factor,
                is_primary: monitor.is_primary().map_err(field_error)?,
            });
        }

        if result.is_empty() {
            return Err(
                "No active displays were found. Connect or wake a display and try again.".into(),
            );
        }
        result.sort_by_key(|display| (!display.is_primary, display.x, display.y, display.id));
        Ok(result)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err("Screen capture is supported on macOS and Windows.".into())
    }
}

/// Capture the selected display at the native capture backend's pixel size.
///
/// Call this on a worker after hiding the editor/selection UI and letting the
/// window compositor update. Only call following an explicit user capture
/// action: ScreenCaptureKit may present its system consent dialog. No files are
/// written and operating-system errors never produce a substitute blank image.
pub fn capture_display(id: u32, pointer: bool) -> Result<RgbaImage, String> {
    #[cfg(target_os = "macos")]
    {
        macos::capture(id, false, pointer)
    }
    #[cfg(target_os = "windows")]
    {
        // Re-enumerate: the display may have disappeared since the user opened
        // the capture menu. Never silently capture a different monitor.
        let monitors = Monitor::all().map_err(capture_error)?;
        let mut selected = None;
        for monitor in monitors {
            if monitor.id().map_err(capture_error)? == id {
                selected = Some(monitor);
                break;
            }
        }
        let monitor = selected.ok_or_else(|| {
            format!("Display {id} is no longer available. Refresh the display list and select a connected display.")
        })?;

        let mut image = monitor.capture_image().map_err(capture_error)?;
        if pointer {
            windows_cursor::draw(
                &mut image,
                monitor.x().map_err(capture_error)?,
                monitor.y().map_err(capture_error)?,
            )?;
        }
        if image.width() == 0 || image.height() == 0 {
            return Err("The operating system returned an empty capture. Wake or unlock the display and try again.".into());
        }
        Ok(image)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = id;
        Err("Screen capture is supported on macOS and Windows.".into())
    }
}

#[cfg(target_os = "windows")]
fn capture_error(error: xcap::XCapError) -> String {
    format!(
        "Windows could not capture the display. Wake or unlock it, close any secure desktop prompt, refresh the display list, and try again. Details: {error}"
    )
}

/// Query permission without showing a permission prompt.
///
/// This is a UI hint, not a replacement for ScreenCaptureKit's error result.
/// In particular, stale code-signing grants can disagree with the Settings UI.
/// A user-triggered capture must let ScreenCaptureKit enforce authorization.
///
/// Windows does not have an equivalent permission gate for this GDI capture
/// path; true does not guarantee capture of protected or secure content.
pub fn screen_capture_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: This system function takes no pointers and only queries the
        // process's Screen Recording permission. It is available since 10.15.
        unsafe { CGPreflightScreenCaptureAccess() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Ask macOS for Screen Recording permission following an explicit user action.
/// Call from the app's UI thread; opening the app or listing displays must not
/// call this function. A grant may require restarting the app before capture.
pub fn request_screen_capture_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        if screen_capture_permission() {
            return true;
        }
        // SAFETY: This system function takes no pointers and displays the
        // system-managed consent flow for the calling process.
        unsafe { CGRequestScreenCaptureAccess() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

#[cfg(target_os = "macos")]
mod macos {
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };

    use block2::RcBlock;
    use image::RgbaImage;
    use objc2::{AnyThread, rc::autoreleasepool};
    use objc2_core_graphics::{CGDataProvider, CGImage, CGImageAlphaInfo, kCGColorSpaceSRGB};
    use objc2_foundation::{NSArray, NSError};
    use objc2_screen_capture_kit::{
        SCContentFilter, SCScreenshotManager, SCShareableContent, SCStreamConfiguration, SCWindow,
    };

    type CaptureResult = Result<RgbaImage, String>;

    pub(super) fn capture(target_id: u32, window: bool, pointer: bool) -> CaptureResult {
        if !objc2::available!(macos = 14.0) {
            return Err("Native screenshots require macOS 14 or later.".into());
        }
        // A synchronous wait is safe on the app's worker, never on the UI
        // thread: system consent and ScreenCaptureKit callbacks need it alive.
        if objc2::MainThreadMarker::new().is_some() {
            return Err("Screen capture must run on a background worker.".into());
        }

        let (sender, receiver) = mpsc::channel::<CaptureResult>();
        let deadline = Instant::now() + Duration::from_secs(60);
        autoreleasepool(|_| {
            let completion = RcBlock::new(
                move |content: *mut SCShareableContent, error: *mut NSError| {
                    // Permission UI can outlive our wait. A late grant must
                    // not start a screenshot for a request already expired.
                    if Instant::now() >= deadline {
                        return;
                    }
                    autoreleasepool(|_| {
                        // SAFETY: ScreenCaptureKit supplies valid borrowed
                        // objects for this callback's lifetime, or null.
                        let result = unsafe {
                            if let Some(error) = error.as_ref() {
                                Err(describe_error("enumerate shareable displays", error))
                            } else if let Some(content) = content.as_ref() {
                                start_screenshot(
                                    content,
                                    target_id,
                                    window,
                                    pointer,
                                    sender.clone(),
                                    deadline,
                                )
                            } else {
                                Err("ScreenCaptureKit returned no shareable display content."
                                    .into())
                            }
                        };
                        if let Err(error) = result {
                            let _ = sender.send(Err(error));
                        }
                    });
                },
            );
            // SAFETY: This is invoked only on an explicit capture action.
            // ScreenCaptureKit owns a copy of the completion until it finishes.
            // Unlike legacy CG image calls, permission denial is an NSError.
            unsafe {
                SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
                    false, true, &completion,
                );
            }
        });
        receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())).map_err(|error| {
            format!("ScreenCaptureKit did not finish the capture: {error}. Complete any macOS permission dialog, then try Capture again.")
        })?
    }

    /// Create the request while the shareable-content callback owns its objects.
    /// No Objective-C objects are passed through the Rust channel.
    unsafe fn start_screenshot(
        content: &SCShareableContent,
        target_id: u32,
        window: bool,
        pointer: bool,
        sender: mpsc::Sender<CaptureResult>,
        deadline: Instant,
    ) -> Result<(), String> {
        // SAFETY: These framework methods and pointPixelScale/contentRect are
        // available on the macOS 14+ path checked before entering the callback.
        unsafe {
            let filter = if window {
                let window = content
                    .windows()
                    .iter()
                    .find(|w| w.windowID() == target_id)
                    .ok_or("The selected window is no longer available.")?;
                SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &window)
            } else {
                let display = content
                    .displays()
                    .iter()
                    .find(|d| d.displayID() == target_id)
                    .ok_or("The selected display is no longer available.")?;
                SCContentFilter::initWithDisplay_excludingWindows(
                    SCContentFilter::alloc(),
                    &display,
                    &NSArray::<SCWindow>::new(),
                )
            };
            let scale = f64::from(filter.pointPixelScale());
            let rect = filter.contentRect();
            let width = (rect.size.width * scale).round();
            let height = (rect.size.height * scale).round();
            if !width.is_finite()
                || !height.is_finite()
                || width < 1.0
                || height < 1.0
                || width > f64::from(u32::MAX)
                || height > f64::from(u32::MAX)
            {
                return Err("ScreenCaptureKit returned invalid display dimensions.".into());
            }
            let configuration = SCStreamConfiguration::new();
            configuration.setWidth(width as usize);
            configuration.setHeight(height as usize);
            configuration.setShowsCursor(pointer);
            configuration.setIgnoreShadowsSingleWindow(true);
            configuration.setScalesToFit(false);
            configuration.setPixelFormat(u32::from_be_bytes(*b"BGRA"));
            configuration.setColorSpaceName(kCGColorSpaceSRGB);

            // Preserve request objects explicitly through asynchronous capture.
            let retained_filter = filter.clone();
            let retained_configuration = configuration.clone();
            let completion = RcBlock::new(move |image: *mut CGImage, error: *mut NSError| {
                let _keep_alive = (&retained_filter, &retained_configuration);
                if Instant::now() >= deadline {
                    return;
                }
                autoreleasepool(|_| {
                    let result = if let Some(error) = error.as_ref() {
                        Err(describe_error("capture the display", error))
                    } else if let Some(image) = image.as_ref() {
                        copy_image(image)
                    } else {
                        Err("ScreenCaptureKit returned no screenshot.".into())
                    };
                    let _ = sender.send(result);
                });
            });
            if Instant::now() >= deadline {
                return Err("The screenshot request expired. Try Capture again.".into());
            }
            SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
                &filter,
                &configuration,
                Some(&completion),
            );
        }
        Ok(())
    }

    fn describe_error(action: &str, error: &NSError) -> String {
        format!(
            "ScreenCaptureKit could not {action}: {} ({} {}). Check MyImage's Screen & System Audio Recording permission, then quit and reopen this exact app bundle. A rebuilt ad-hoc signed app may need its permission renewed.",
            error.localizedDescription(),
            error.domain(),
            error.code(),
        )
    }

    fn copy_image(image: &CGImage) -> CaptureResult {
        let width = CGImage::width(Some(image));
        let height = CGImage::height(Some(image));
        let stride = CGImage::bytes_per_row(Some(image));
        if width == 0 || height == 0 || width > u32::MAX as usize || height > u32::MAX as usize {
            return Err("ScreenCaptureKit returned an empty or oversized screenshot.".into());
        }
        if CGImage::bits_per_component(Some(image)) != 8
            || CGImage::bits_per_pixel(Some(image)) != 32
        {
            return Err(
                "ScreenCaptureKit did not return the requested 8-bit BGRA screenshot.".into(),
            );
        }
        let alpha = CGImage::alpha_info(Some(image));
        if alpha != CGImageAlphaInfo::PremultipliedFirst
            && alpha != CGImageAlphaInfo::First
            && alpha != CGImageAlphaInfo::NoneSkipFirst
        {
            return Err(format!(
                "ScreenCaptureKit returned an unexpected alpha layout ({alpha:?})."
            ));
        }
        let provider = CGImage::data_provider(Some(image))
            .ok_or("ScreenCaptureKit screenshot has no pixel provider.")?;
        let data = CGDataProvider::data(Some(&provider))
            .ok_or("Could not read ScreenCaptureKit screenshot pixels.")?
            .to_vec();
        copy_bgra(
            &data,
            width as u32,
            height as u32,
            stride,
            alpha == CGImageAlphaInfo::PremultipliedFirst,
            alpha == CGImageAlphaInfo::NoneSkipFirst,
        )
    }

    // SCScreenshotManager documents BGRA for SDR screenshots. Respect its row
    // stride and convert premultiplied alpha to image::RgbaImage's straight alpha.
    fn copy_bgra(
        data: &[u8],
        width: u32,
        height: u32,
        stride: usize,
        premultiplied: bool,
        opaque: bool,
    ) -> CaptureResult {
        let row_bytes = (width as usize)
            .checked_mul(4)
            .ok_or("Screenshot row is too large.")?;
        let required = stride
            .checked_mul(height as usize)
            .ok_or("Screenshot buffer is too large.")?;
        let output_size = row_bytes
            .checked_mul(height as usize)
            .ok_or("Screenshot buffer is too large.")?;
        if width == 0 || height == 0 || stride < row_bytes || data.len() < required {
            return Err("ScreenCaptureKit returned an incomplete pixel buffer.".into());
        }
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(output_size)
            .map_err(|error| format!("Cannot allocate screenshot: {error}"))?;
        for row in data.chunks_exact(stride).take(height as usize) {
            for bgra in row[..row_bytes].chunks_exact(4) {
                let alpha = if opaque { 255 } else { bgra[3] };
                let straight = |component: u8| -> u8 {
                    if premultiplied && alpha > 0 && alpha < 255 {
                        ((u32::from(component) * 255 + u32::from(alpha) / 2) / u32::from(alpha))
                            .min(255) as u8
                    } else {
                        component
                    }
                };
                rgba.extend_from_slice(&[
                    straight(bgra[2]),
                    straight(bgra[1]),
                    straight(bgra[0]),
                    alpha,
                ]);
            }
        }
        RgbaImage::from_raw(width, height, rgba)
            .ok_or("Could not construct the screenshot image.".into())
    }

    #[cfg(test)]
    mod tests {
        use super::copy_bgra;

        #[test]
        fn bgra_conversion_respects_padding_orientation_and_premultiplied_alpha() {
            let bytes = [
                10, 20, 30, 255, 99, 99, 99, 99, 10, 20, 30, 128, 88, 88, 88, 88,
            ];
            let image = copy_bgra(&bytes, 1, 2, 8, true, false).unwrap();
            assert_eq!(image.get_pixel(0, 0).0, [30, 20, 10, 255]);
            assert_eq!(image.get_pixel(0, 1).0, [60, 40, 20, 128]);
            assert!(copy_bgra(&bytes[..8], 1, 2, 8, false, false).is_err());
            assert!(copy_bgra(&bytes, 2, 1, 4, false, false).is_err());
        }
    }
}
