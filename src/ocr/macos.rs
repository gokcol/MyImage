use image::{ImageEncoder, RgbaImage, codecs::png::PngEncoder};
use objc2::{AnyThread, rc::autoreleasepool};
use objc2_foundation::{NSArray, NSData, NSDictionary};
use objc2_vision::{
    VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel,
};

/// Recognize only the supplied crop. All image data stays in memory on this Mac.
pub(super) fn recognize(image: &RgbaImage) -> Result<String, String> {
    recognize_with_cpu_only(image, true)
}

fn recognize_with_cpu_only(image: &RgbaImage, cpu_only: bool) -> Result<String, String> {
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
        request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
        request.setUsesLanguageCorrection(true);
        request.setAutomaticallyDetectsLanguage(true);
        if cpu_only {
            // A small selected region does not need GPU acceleration. CPU-only
            // recognition avoids lengthy first-use accelerator initialization
            // seen on this Mac while preserving the same accurate OCR model.
            // This API remains available on our macOS 14 minimum deployment.
            #[allow(deprecated)]
            unsafe {
                request.setUsesCPUOnly(true)
            };
        }

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

        let data = NSData::with_bytes(&png);
        let handler = VNImageRequestHandler::initWithData_options(
            VNImageRequestHandler::alloc(),
            &data,
            &NSDictionary::new(),
        );
        let requests = NSArray::<VNRequest>::from_slice(&[request.as_ref()]);
        // The request and immutable encoded image outlive this synchronous call.
        handler
            .performRequests_error(&requests)
            .map_err(|error| format!("Text recognition could not finish: {error}"))?;

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

#[cfg(test)]
mod tests {
    fn text_fixture() -> image::RgbaImage {
        crate::svg_stamp::raster(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="240">
            <rect width="1000" height="240" fill="white"/>
            <g font-family="sans-serif" font-size="40" fill="black">
              <text x="40" y="80">Selected region text</text>
              <text x="40" y="160">Merhaba İstanbul 12345</text>
            </g></svg>"##,
            1000,
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
                "CPU only: {cpu_only}; iteration: {iteration}; elapsed: {:?}; text: {text:?}",
                start.elapsed()
            );
            assert_eq!(text, "Selected region text\nMerhaba İstanbul 12345");
        }
    }
}
