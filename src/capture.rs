//! Native still-image capture for macOS and Windows.
//!
//! macOS uses ScreenCaptureKit's SCScreenshotManager (macOS 14 or later).
//! Windows uses xcap 0.9.8's default GDI BitBlt backend. xcap also supplies
//! non-capturing monitor metadata on both systems.
//! Keep all cropping in the returned image's pixel coordinates: desktop bounds
//! have different units on the two operating systems.

use image::RgbaImage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureDestination {
    #[default]
    Editor,
    Clipboard,
}
impl CaptureDestination {
    pub const ALL: [Self; 2] = [Self::Editor, Self::Clipboard];
    pub fn label(self) -> &'static str {
        match self {
            Self::Editor => "Editor",
            Self::Clipboard => "Clipboard",
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Self::Editor => "Save to Library and open in Editor.",
            Self::Clipboard => {
                "Copy, save to Library, and return to your other app. Your open project stays untouched."
            }
        }
    }
    pub fn capture_label(self) -> &'static str {
        match self {
            Self::Editor => "Capture",
            Self::Clipboard => "Capture & copy",
        }
    }
    pub fn finish_label(self) -> &'static str {
        match self {
            Self::Editor => "Finish & edit",
            Self::Clipboard => "Finish & copy",
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
use xcap::Monitor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum CaptureMode {
    #[default]
    Region,
    MultiRegion,
    Window,
    FullScreen,
    Scrolling,
    Text,
}
impl CaptureMode {
    pub const ALL: [Self; 6] = [
        Self::Region,
        Self::MultiRegion,
        Self::Window,
        Self::FullScreen,
        Self::Scrolling,
        Self::Text,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Region => "Region",
            Self::MultiRegion => "Multi-region",
            Self::Window => "Window",
            Self::FullScreen => "Full screen",
            Self::Scrolling => "Scrolling",
            Self::Text => "Text",
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Self::Region => "Click a window or drag an area. Adjust its edges before capturing.",
            Self::MultiRegion => {
                "Select several areas. Adjust each one, then capture together with transparent gaps."
            }
            Self::Window => "Point to a window, click to select, then capture.",
            Self::FullScreen => "Click the screen to capture, or choose a fixed display.",
            Self::Scrolling => {
                "Click a window or select part of it. Scroll down, then finish. Sections are collected automatically."
            }
            Self::Text => {
                "Select a screen region and copy its text. No image or project is created."
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

/// Resolve a selection against freshly enumerated geometry. macOS can replace
/// a mirrored display's runtime ID with its source display; only that explicit
/// relationship is eligible for fallback. Never choose an unrelated display.
pub fn resolve_display(id: u32, displays: &[DisplayInfo]) -> Result<DisplayInfo, String> {
    let ids: Vec<_> = displays.iter().map(|display| display.id).collect();
    let resolved = resolve_display_id(id, &ids, display_mirror_source);
    displays
        .iter()
        .find(|display| Some(display.id) == resolved)
        .cloned()
        .ok_or_else(|| {
            format!(
                "Display {id} is no longer available. Refresh the display list and select a connected display."
            )
        })
}

fn display_mirror_source(id: u32) -> u32 {
    #[cfg(target_os = "macos")]
    {
        objc2_core_graphics::CGDisplayMirrorsDisplay(id)
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Windows monitor handles must match exactly.
        let _ = id;
        0
    }
}

fn resolve_display_id(
    requested: u32,
    available: &[u32],
    mirror_source: impl Fn(u32) -> u32,
) -> Option<u32> {
    if requested == 0 {
        return None;
    }
    if available.contains(&requested) {
        return Some(requested);
    }
    let canonical = |id| match mirror_source(id) {
        0 => id,
        source => source,
    };
    let source = canonical(requested);
    available
        .iter()
        .copied()
        .find(|&id| id != 0 && canonical(id) == source)
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

/// Select from freshly enumerated desktop geometry. Mirrored displays that
/// occupy the same desktop rectangle share one overlay.
pub fn capture_targets(
    displays: &[DisplayInfo],
    fixed: Option<u32>,
) -> Result<Vec<DisplayInfo>, String> {
    if let Some(id) = fixed {
        return resolve_display(id, displays).map(|d| vec![d]);
    }
    let mut targets: Vec<DisplayInfo> = Vec::new();
    for display in displays {
        if !targets.iter().any(|d| {
            d.id == display.id
                || (d.x, d.y, d.width, d.height)
                    == (display.x, display.y, display.width, display.height)
        }) {
            targets.push(display.clone());
        }
    }
    if targets.is_empty() {
        Err("No active displays were found. Connect or wake a display and try again.".into())
    } else {
        Ok(targets)
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
                let displays = content.displays();
                let ids: Vec<_> = displays.iter().map(|display| display.displayID()).collect();
                // CoreGraphics and ScreenCaptureKit may expose different
                // members of one mirrored desktop. Resolve only that verified
                // mirror family, never an arbitrary remaining display.
                let display_id =
                    super::resolve_display_id(target_id, &ids, super::display_mirror_source)
                        .ok_or("The selected display is no longer available.")?;
                let display = displays
                    .iter()
                    .find(|d| d.displayID() == display_id)
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
            "ScreenCaptureKit could not {action}: {} ({} {}). Check MyImage's Screen & System Audio Recording permission, then quit and reopen this exact app bundle. If access is already enabled, macOS may be retaining a grant for an older signing identity.",
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

#[cfg(test)]
mod display_tests {
    use super::{DisplayInfo, resolve_display, resolve_display_id};

    fn mirror_source(id: u32) -> u32 {
        match id {
            11 | 12 => 22,
            33 => 44,
            _ => 0,
        }
    }

    #[test]
    fn any_display_includes_extended_desktop_and_deduplicates_mirrors() {
        let display = DisplayInfo {
            id: 1,
            name: "Main".into(),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            scale_factor: 1.,
            is_primary: true,
        };
        let mut left = display.clone();
        left.id = 2;
        left.x = -2560;
        left.y = -200;
        left.width = 2560;
        left.scale_factor = 1.5;
        left.is_primary = false;
        let mut mirror = display.clone();
        mirror.id = 3;
        let displays = vec![display, left, mirror];
        let targets = super::capture_targets(&displays, None).unwrap();
        assert_eq!(targets.iter().map(|d| d.id).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(
            (
                targets[1].x,
                targets[1].y,
                targets[1].width,
                targets[1].scale_factor
            ),
            (-2560, -200, 2560, 1.5)
        );
        assert_eq!(super::capture_targets(&displays, Some(2)).unwrap()[0].id, 2);
        // Zero is always invalid. Other synthetic IDs must not be sent to the
        // host's mirror service: headless Macs can alias unknown IDs together.
        // Missing/unrelated IDs are covered with a fake mirror service below.
        assert!(super::capture_targets(&displays, Some(0)).is_err());
        assert!(super::capture_targets(&[], None).is_err());
    }

    #[test]
    fn exact_display_wins_over_other_mirror_members() {
        assert_eq!(
            resolve_display_id(11, &[22, 12, 11], mirror_source),
            Some(11)
        );
    }

    #[test]
    fn mirrored_selection_resolves_only_within_its_family() {
        assert_eq!(resolve_display_id(11, &[33, 22], mirror_source), Some(22));
        assert_eq!(resolve_display_id(22, &[33, 11], mirror_source), Some(11));
        assert_eq!(resolve_display_id(11, &[12], mirror_source), Some(12));
    }

    #[test]
    fn unrelated_or_missing_displays_do_not_fall_back() {
        assert_eq!(resolve_display_id(11, &[33], mirror_source), None);
        assert_eq!(resolve_display_id(55, &[22, 44], mirror_source), None);
        assert_eq!(resolve_display_id(11, &[], mirror_source), None);
        assert_eq!(resolve_display_id(11, &[22], |_| 0), None);
    }

    #[test]
    fn zero_is_never_a_display_or_mirror_match() {
        assert_eq!(resolve_display_id(0, &[0, 22], mirror_source), None);
        assert_eq!(resolve_display_id(11, &[0], |_| 22), None);
    }

    #[test]
    fn refreshed_display_geometry_is_returned_in_full() {
        let display = DisplayInfo {
            id: 22,
            name: "Fresh display".into(),
            x: -1920,
            y: 100,
            width: 1920,
            height: 1080,
            scale_factor: 2.0,
            is_primary: false,
        };
        let resolved = resolve_display(22, &[display]).unwrap();
        assert_eq!(resolved.name, "Fresh display");
        assert_eq!((resolved.x, resolved.y), (-1920, 100));
        assert_eq!((resolved.width, resolved.height), (1920, 1080));
        assert_eq!(resolved.scale_factor, 2.0);
        assert!(!resolved.is_primary);
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires a connected mirrored display and native display-service access"]
    fn native_mirror_members_resolve_to_the_current_desktop_without_capturing_pixels() {
        use objc2_core_graphics::{CGDisplayMirrorsDisplay, CGError, CGGetOnlineDisplayList};

        let displays = super::displays().unwrap();
        let mut ids = [0; 32];
        let mut count = 0;
        // SAFETY: The output buffers remain valid for the synchronous query.
        let error =
            unsafe { CGGetOnlineDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) };
        assert_eq!(error, CGError::Success);
        let mut mirrors = 0;
        for &id in &ids[..count as usize] {
            let source = CGDisplayMirrorsDisplay(id);
            if source == 0 {
                continue;
            }
            let resolved = resolve_display(id, &displays).unwrap();
            assert!(resolved.id == id || resolved.id == source);
            assert!(resolved.width > 0 && resolved.height > 0);
            println!(
                "Mirror {id} -> desktop {} at {}, {} ({} x {})",
                resolved.id, resolved.x, resolved.y, resolved.width, resolved.height
            );
            mirrors += 1;
        }
        assert!(mirrors > 0, "this diagnostic requires an active mirror set");
    }
}
