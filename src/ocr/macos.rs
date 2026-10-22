use super::Control;
use block2::RcBlock;
use image::{ImageEncoder, RgbaImage, codecs::png::PngEncoder};
use objc2::runtime::NSObjectProtocol;
use objc2::{
    AnyThread, ClassType,
    rc::{Retained, autoreleasepool},
};
use objc2_core_ml::MLCPUComputeDevice;
use objc2_foundation::{NSArray, NSData, NSDictionary, NSError, NSIndexSet};
use objc2_vision::{
    VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestProgressProviding,
    VNRequestTextRecognitionLevel,
};
use std::ptr::NonNull;

/// Recognize only the supplied crop. All image data stays in memory on this Mac.
#[cfg(test)]
pub(super) fn recognize(image: &RgbaImage) -> Result<String, String> {
    recognize_with_cpu_only(image, true)
}

#[cfg(test)]
fn recognize_with_cpu_only(image: &RgbaImage, cpu_only: bool) -> Result<String, String> {
    let control = Control {
        canceled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        deadline: std::time::Instant::now() + super::JOB_TIMEOUT,
    };
    recognize_controlled(image, cpu_only, &control)
}

pub(super) fn recognize_cancellable(
    image: &RgbaImage,
    control: &Control,
) -> Result<String, String> {
    recognize_controlled(image, true, control)
}

fn recognize_controlled(
    image: &RgbaImage,
    cpu_only: bool,
    control: &Control,
) -> Result<String, String> {
    control.check()?;
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|error| {
            format!("Could not prepare the selected region for text recognition: {error}")
        })?;

    // Recognition runs on a worker; that thread has no AppKit autorelease pool.
    autoreleasepool(|_| {
        let request = VNRecognizeTextRequest::new();
        // Vision supplies a valid request on its own callback queue. Checking
        // cancellation here avoids transferring a non-Send Objective-C request
        // between Rust threads, or racing other request-property mutations.
        let progress_control = control.clone();
        let progress = RcBlock::new(
            move |request: NonNull<VNRequest>, _: f64, _: *mut NSError| {
                if progress_control.check().is_err() {
                    // SAFETY: Vision's progress-handler contract supplies a valid
                    // request for the callback. cancel() only asks Vision to abort.
                    unsafe { request.as_ref().cancel() };
                }
            },
        );
        // SAFETY: this block lives until performRequests has returned; Vision
        // copies it when assigning the progressHandler property.
        unsafe { request.setProgressHandler(RcBlock::as_ptr(&progress)) };
        request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
        configure_revision(&request)?;
        request.setUsesLanguageCorrection(true);
        request.setAutomaticallyDetectsLanguage(true);
        // Language availability depends on the OS and the current Vision model.
        // Never pass a language the installed recognizer does not support.
        let supported = unsafe { request.supportedRecognitionLanguagesAndReturnError() }
            .map_err(|error| format!("Could not initialize text recognition: {error}"))?;
        let languages: Vec<_> = ["tr", "en"]
            .into_iter()
            .filter_map(|prefix| {
                supported.iter().find(|language| {
                    let language = language.to_string();
                    language == prefix || language.starts_with(&format!("{prefix}-"))
                })
            })
            .collect();
        if !languages.is_empty() {
            request.setRecognitionLanguages(&NSArray::from_retained_slice(&languages));
        }
        if cpu_only {
            #[cfg(not(test))]
            let set_legacy_cpu_flag = true;
            #[cfg(test)]
            let set_legacy_cpu_flag = std::env::var("MYIMAGE_OCR_TEST_SKIP_LEGACY_CPU").is_err();
            if set_legacy_cpu_flag {
                #[allow(deprecated)]
                unsafe {
                    request.setUsesCPUOnly(true)
                };
            }
            // Keep the legacy CPU flag as well as the macOS 14+ compute-stage
            // assignment: Vision's internal image reader can otherwise compile
            // Neural Engine models even when the outer stage is set to CPU.
            configure_cpu(&request)?;
        }
        control.check()?;

        let data = NSData::with_bytes(&png);
        let handler = VNImageRequestHandler::initWithData_options(
            VNImageRequestHandler::alloc(),
            &data,
            &NSDictionary::new(),
        );
        let requests = NSArray::<VNRequest>::from_slice(&[request.as_ref()]);
        // The request and immutable encoded image outlive this synchronous call.
        let result = handler.performRequests_error(&requests);
        // A canceled/timed-out call has a stable, useful message rather than a
        // platform-specific VNErrorRequestCancelled description.
        control.check()?;
        result.map_err(|error| format!("Text recognition could not finish: {error}"))?;

        let Some(observations) = request.results() else {
            return Ok(String::new());
        };
        let mut lines = Vec::with_capacity(observations.len());
        for observation in observations {
            let candidates = observation.topCandidates(1);
            let Some(candidate) = candidates.firstObject() else {
                continue;
            };
            let text = candidate.string().to_string();
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            // Vision uses a lower-left origin and normalized image coordinates.
            let bounds = unsafe { observation.boundingBox() };
            lines.push((
                bounds.origin.y + bounds.size.height,
                bounds.origin.x,
                text.to_owned(),
            ));
        }
        lines.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.total_cmp(&b.1)));
        Ok(lines
            .into_iter()
            .map(|(_, _, text)| text)
            .collect::<Vec<_>>()
            .join("\n"))
    })
}

fn configure_revision(request: &VNRecognizeTextRequest) -> Result<(), String> {
    // Revision 3's image reader can spend tens of seconds compiling Neural
    // Engine models on its first use, including CPU-configured requests.
    // Revision 2 keeps accurate recognition while using the established local
    // text model. It is available on our macOS 14 minimum deployment.
    #[allow(deprecated)] // Deliberately pinned; checked against supportedRevisions below.
    let preferred = objc2_vision::VNRecognizeTextRequestRevision2;
    #[cfg(test)]
    let preferred = std::env::var("MYIMAGE_OCR_TEST_REVISION")
        .ok()
        .and_then(|revision| revision.parse::<usize>().ok())
        .unwrap_or(preferred);
    // SAFETY: supportedRevisions is a class method inherited from VNRequest;
    // invoking it on the concrete class obtains text-recognition revisions.
    let supported: Retained<NSIndexSet> =
        unsafe { objc2::msg_send![VNRecognizeTextRequest::class(), supportedRevisions] };
    if !supported.containsIndex(preferred) {
        return Err("This macOS version has no supported CPU text recognition model. Please check for a macOS update.".into());
    }
    // SAFETY: the chosen revision is in this concrete request's supported set.
    unsafe { request.setRevision(preferred) };
    log::info!("OCR using accurate Vision revision {preferred}");
    Ok(())
}

fn configure_cpu(request: &VNRecognizeTextRequest) -> Result<usize, String> {
    // SAFETY: the request is fully configured and still belongs to this worker;
    // supported devices and assignments are inspected before it is performed.
    let supported = unsafe { request.supportedComputeStageDevicesAndReturnError() }
        .map_err(|error| format!("Could not configure text recognition: {error}"))?;
    let mut assigned = 0;
    for stage in supported.allKeys() {
        let devices = supported.objectForKey(&stage).ok_or_else(|| {
            "Vision returned no devices for a text recognition stage.".to_string()
        })?;
        let cpu = devices
            .iter()
            .find(|device| device.isKindOfClass(MLCPUComputeDevice::class()))
            .ok_or_else(|| {
                "This Mac's text recognizer has no supported CPU device. Please check for a macOS update.".to_string()
            })?;
        // SAFETY: this device is explicitly listed as supported for this stage.
        unsafe { request.setComputeDevice_forComputeStage(Some(&cpu), &stage) };
        assigned += 1;
    }
    if assigned == 0 {
        return Err("Vision returned no text recognition compute stages.".into());
    }
    log::info!("OCR configured {assigned} native compute stages on the CPU");
    Ok(assigned)
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "inspects the installed Vision text model's actual compute devices"]
    fn every_supported_stage_is_explicitly_assigned_to_cpu() {
        use objc2::{ClassType, runtime::NSObjectProtocol};
        objc2::rc::autoreleasepool(|_| {
            let request = objc2_vision::VNRecognizeTextRequest::new();
            request.setRecognitionLevel(objc2_vision::VNRequestTextRecognitionLevel::Accurate);
            super::configure_revision(&request).unwrap();
            #[allow(deprecated)]
            unsafe {
                request.setUsesCPUOnly(true)
            };
            let supported =
                unsafe { request.supportedComputeStageDevicesAndReturnError() }.unwrap();
            for stage in supported.allKeys() {
                let device = unsafe { request.computeDeviceForComputeStage(&stage) };
                eprintln!("Deprecated CPU flag; stage {stage}: {device:?}");
            }
            assert_eq!(super::configure_cpu(&request).unwrap(), supported.count());
            for stage in supported.allKeys() {
                let device = unsafe { request.computeDeviceForComputeStage(&stage) }.unwrap();
                eprintln!("Explicit CPU assignment; stage {stage}: {device:?}");
                assert!(device.isKindOfClass(objc2_core_ml::MLCPUComputeDevice::class()));
            }
        });
    }

    fn text_fixture() -> image::RgbaImage {
        let width = std::env::var("MYIMAGE_OCR_TEST_FIXTURE_WIDTH")
            .ok()
            .and_then(|width| width.parse::<u32>().ok())
            .filter(|width| (500..=4000).contains(width))
            .unwrap_or(1000);
        crate::svg_stamp::raster(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="240">
            <rect width="1000" height="240" fill="white"/>
            <g font-family="sans-serif" font-size="40" fill="black">
              <text x="40" y="80">Selected region text</text>
              <text x="40" y="160">Merhaba İstanbul 12345</text>
            </g></svg>"##,
            width,
        )
        .unwrap()
    }

    #[test]
    #[ignore = "exercises the installed macOS Vision OCR model"]
    fn recognizes_real_multiline_image_on_worker_thread() {
        let image = text_fixture();
        let text = std::thread::spawn(move || super::recognize(&image))
            .join()
            .unwrap()
            .unwrap();
        let supports_turkish = objc2::rc::autoreleasepool(|_| {
            let request = objc2_vision::VNRecognizeTextRequest::new();
            request.setRecognitionLevel(objc2_vision::VNRequestTextRecognitionLevel::Accurate);
            super::configure_revision(&request).unwrap();
            unsafe { request.supportedRecognitionLanguagesAndReturnError() }
                .unwrap()
                .iter()
                .any(|language| language.to_string().starts_with("tr"))
        });
        eprintln!("Recognized fixture: {text:?}; Turkish model supported: {supports_turkish}");
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some("Selected region text"), "{text}");
        let second = lines.next().expect("second line of recognized text");
        assert!(second.starts_with("Merhaba "), "{text}");
        assert!(second.ends_with(" 12345"), "{text}");
        if supports_turkish {
            assert!(second.contains("İstanbul"), "{text}");
        }
        assert!(lines.next().is_none(), "{text}");
    }

    #[test]
    #[ignore = "exercises the installed macOS Vision OCR model"]
    fn blank_region_returns_no_text() {
        let image = image::RgbaImage::from_pixel(320, 160, image::Rgba([255; 4]));
        assert!(super::recognize(&image).unwrap().is_empty());
    }

    #[test]
    #[ignore = "diagnoses installed Vision model cold and warm recognition latency"]
    fn compare_recognition_latency() {
        let image = text_fixture();
        let cpu_only = std::env::var("MYIMAGE_OCR_TEST_CPU_ONLY").is_ok();
        for iteration in 1..=2 {
            let start = std::time::Instant::now();
            let text = super::recognize_with_cpu_only(&image, cpu_only).unwrap();
            eprintln!(
                "CPU only: {cpu_only}; dimensions: {:?}; iteration: {iteration}; elapsed: {:?}; text: {text:?}",
                image.dimensions(),
                start.elapsed()
            );
            assert_eq!(text, "Selected region text\nMerhaba İstanbul 12345");
        }
    }
}
