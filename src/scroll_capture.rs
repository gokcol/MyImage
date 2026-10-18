//! Guided vertical capture. Failed/ambiguous matches never modify accepted pixels.
use image::RgbaImage;

pub struct Stitcher {
    pub image: RgbaImage,
    last: RgbaImage,
    pub sections: usize,
    window_size: Option<(u32, u32)>,
    window_area: Option<crate::selection::PixelRect>,
    pending: Option<RgbaImage>,
}
impl Stitcher {
    pub fn new(image: RgbaImage) -> Self {
        Self {
            last: image.clone(),
            image,
            sections: 1,
            window_size: None,
            window_area: None,
            pending: None,
        }
    }
    pub fn window(image: RgbaImage) -> Self {
        let size = image.dimensions();
        Self {
            window_size: Some(size),
            ..Self::new(image)
        }
    }
    pub fn region(image: RgbaImage) -> Self {
        let (width, height) = image.dimensions();
        Self {
            window_area: Some(crate::selection::PixelRect {
                x: 0,
                y: 0,
                width,
                height,
            }),
            ..Self::window(image)
        }
    }
    /// Collect reliable overlaps while the user scrolls. A changing/animated frame
    /// may fail transiently; report an alignment problem only once it settles.
    pub fn observe_window(&mut self, frame: RgbaImage) -> Result<bool, String> {
        if Some(frame.dimensions()) != self.window_size {
            return Err("The window changed size. Finish this capture and start another.".into());
        }
        let stable = self
            .pending
            .as_ref()
            .is_some_and(|old| similar_frame(old, &frame));
        self.pending = Some(frame.clone());
        let result = self.join_window(&frame);
        if result.is_err() && !stable {
            Ok(false)
        } else {
            result
        }
    }
    fn join_window(&mut self, frame: &RgbaImage) -> Result<bool, String> {
        if let Some(rect) = self.window_area {
            let next = crop(frame, rect);
            if similar_frame(&self.last, &next) {
                return Ok(false);
            }
            return self.append(next);
        }
        if similar_frame(&self.last, frame) {
            return Ok(false);
        }
        let regions = moving_regions(&self.last, frame);
        if regions.is_empty() {
            return Ok(false);
        }
        for rect in regions {
            let mut candidate = Self::new(crop(&self.last, rect));
            if candidate.append(crop(frame, rect)).is_ok_and(|added| added) {
                self.image = candidate.image;
                self.last = candidate.last;
                self.sections = candidate.sections;
                self.window_area = Some(rect);
                return Ok(true);
            }
        }
        Err("Cannot find a reliable overlap. Scroll back a little, then continue down more slowly. Keep at least half the previous content visible.".into())
    }
    pub fn append(&mut self, next: RgbaImage) -> Result<bool, String> {
        if next.dimensions() != self.last.dimensions() || next.height() < 48 || next.width() < 32 {
            return Err(
                "Keep the window and capture region the same size (at least 32 × 48 pixels)."
                    .into(),
            );
        }
        let h = next.height();
        if self.last == next {
            return Ok(false);
        }
        let mut candidates: Vec<(f64, u32)> = (24..h - 4)
            .map(|overlap| (difference(&self.last, &next, overlap), overlap))
            .collect();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (score, overlap) = candidates[0];
        let ambiguous = candidates
            .iter()
            .skip(1)
            .any(|(cost, other)| overlap.abs_diff(*other) > 3 && *cost < score + 0.6);
        if score > 5. || ambiguous {
            return Err("Could not align this section. Scroll back to leave at least a third overlapping. Select only the moving content, excluding fixed headers and sidebars.".into());
        }
        let added = h - overlap;
        let height = self
            .image
            .height()
            .checked_add(added)
            .ok_or("Capture is too tall.")?;
        if height > 32_768 || u64::from(height) * u64::from(next.width()) > 100_000_000 {
            return Err(
                "This capture reached the image size limit. Finish it and start another capture."
                    .into(),
            );
        }
        let mut joined = RgbaImage::new(next.width(), height);
        image::imageops::replace(&mut joined, &self.image, 0, 0);
        let tail = image::imageops::crop_imm(&next, 0, overlap, next.width(), added).to_image();
        image::imageops::replace(&mut joined, &tail, 0, i64::from(self.image.height()));
        self.image = joined;
        self.last = next;
        self.sections += 1;
        Ok(true)
    }
}

fn crop(image: &RgbaImage, rect: crate::selection::PixelRect) -> RgbaImage {
    image::imageops::crop_imm(image, rect.x, rect.y, rect.width, rect.height).to_image()
}
fn similar_frame(a: &RgbaImage, b: &RgbaImage) -> bool {
    if a.dimensions() != b.dimensions() {
        return false;
    }
    // Allow a blinking caret or pointer while rejecting a moving viewport.
    let mut changed = 0;
    for row in 0..64 {
        for col in 0..64 {
            let x = col * (a.width() - 1) / 63;
            let y = row * (a.height() - 1) / 63;
            if a.get_pixel(x, y).0[..3]
                .iter()
                .zip(&b.get_pixel(x, y).0[..3])
                .any(|(x, y)| x.abs_diff(*y) > 8)
            {
                changed += 1;
            }
        }
    }
    changed < 8
}
fn moving_regions(a: &RgbaImage, b: &RgbaImage) -> Vec<crate::selection::PixelRect> {
    let (w, h) = a.dimensions();
    if b.dimensions() != (w, h) {
        return vec![];
    }
    let mut rows = Vec::new();
    for y in 0..h {
        let mut left = w;
        let mut right = 0;
        let mut count = 0;
        for x in 0..w {
            if a.get_pixel(x, y).0[..3]
                .iter()
                .zip(&b.get_pixel(x, y).0[..3])
                .any(|(a, b)| a.abs_diff(*b) > 12)
            {
                left = left.min(x);
                right = x + 1;
                count += 1;
            }
        }
        // Ignore cursor/caret-sized changes and narrow scrollbars.
        if count > 24.max(w / 100) {
            rows.push((y, left, right));
        }
    }
    if rows.is_empty() {
        return vec![];
    }
    let mut ranges = vec![(0, rows.len())];
    // Window activation can change the title bar. Try excluding isolated edge
    // bands only if the full changed region does not produce a reliable join.
    if let Some(i) = (1..rows.len()).find(|&i| rows[i].0 - rows[i - 1].0 > 16)
        && rows[i - 1].0 - rows[0].0 < h / 4
    {
        ranges.push((i, rows.len()));
    }
    if let Some(i) = (1..rows.len())
        .rev()
        .find(|&i| rows[i].0 - rows[i - 1].0 > 16)
        && rows[rows.len() - 1].0 - rows[i].0 < h / 4
    {
        ranges.push((0, i));
    }
    ranges
        .into_iter()
        .filter_map(|(start, end)| {
            let rows = &rows[start..end];
            let left = rows.iter().map(|r| r.1).min()?;
            let right = rows.iter().map(|r| r.2).max()?;
            let top = rows.first()?.0;
            let bottom = rows.last()?.0 + 1;
            if right - left < 32 || bottom - top < 48 {
                return None;
            }
            let left = left.saturating_sub(8);
            Some(crate::selection::PixelRect {
                x: left,
                y: top,
                width: (right + 8).min(w) - left,
                height: bottom - top,
            })
        })
        .collect()
}

fn difference(previous: &RgbaImage, next: &RgbaImage, overlap: u32) -> f64 {
    let mut total = 0u64;
    let mut count = 0u64;
    // Deterministic spread across the overlap; avoid edges and scrollbars.
    for row in 0..32 {
        let y = row * (overlap - 1) / 31;
        for col in 0..48 {
            let x = (col + 1) * (next.width() - 1) / 49;
            let a = previous.get_pixel(x, previous.height() - overlap + y);
            let b = next.get_pixel(x, y);
            for c in 0..3 {
                total += u64::from(a[c].abs_diff(b[c]));
                count += 1;
            }
        }
    }
    total as f64 / count as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    fn page() -> RgbaImage {
        RgbaImage::from_fn(240, 1200, |x, y| {
            let n = x
                .wrapping_mul(1973)
                .wrapping_add(y.wrapping_mul(9277))
                .wrapping_mul(26699);
            image::Rgba([
                (n % 251) as u8,
                ((n >> 8) % 251) as u8,
                ((n >> 16) % 251) as u8,
                255,
            ])
        })
    }
    #[test]
    fn automatic_partial_window_preserves_exact_area_and_ignores_idle_frames() {
        let page = page();
        let frame = |y| image::imageops::crop_imm(&page, 31, y, 130, 300).to_image();
        let mut stitch = Stitcher::region(frame(40));
        assert!(!stitch.observe_window(frame(40)).unwrap());
        assert!(!stitch.observe_window(frame(40)).unwrap());
        for y in [150, 280, 410] {
            assert!(stitch.observe_window(frame(y)).unwrap());
            assert!(!stitch.observe_window(frame(y)).unwrap());
        }
        assert_eq!(stitch.sections, 4);
        assert_eq!(
            stitch.image,
            image::imageops::crop_imm(&page, 31, 40, 130, 670).to_image()
        );
        let mut caret = frame(410);
        caret.put_pixel(35, 20, image::Rgba([255; 4]));
        assert!(!stitch.observe_window(caret.clone()).unwrap());
        assert!(!stitch.observe_window(caret).unwrap());
    }
    #[test]
    fn automatic_window_capture_collects_continuous_scroll_and_removes_fixed_chrome() {
        let page = page();
        let frame = |offset| {
            let mut window = RgbaImage::from_pixel(280, 480, image::Rgba([30, 40, 50, 255]));
            image::imageops::replace(
                &mut window,
                &image::imageops::crop_imm(&page, 0, offset, 240, 400).to_image(),
                20,
                50,
            );
            window
        };
        let mut stitch = Stitcher::window(frame(0));
        assert!(stitch.observe_window(frame(150)).unwrap());
        assert_eq!(stitch.sections, 2);
        assert!(stitch.observe_window(frame(310)).unwrap());
        assert_eq!(stitch.sections, 3);
        assert_eq!(stitch.image.height(), 710);
        // Eight-pixel horizontal margins are retained; fixed top/bottom rows are not duplicated.
        assert_eq!(
            image::imageops::crop_imm(&stitch.image, 8, 0, 240, 710).to_image(),
            image::imageops::crop_imm(&page, 0, 0, 240, 710).to_image()
        );
        let before = stitch.image.clone();
        let unrelated = RgbaImage::from_pixel(280, 480, image::Rgba([255, 255, 255, 255]));
        assert!(!stitch.observe_window(unrelated.clone()).unwrap());
        assert!(stitch.observe_window(unrelated).is_err());
        assert_eq!(stitch.image, before);
        assert!(stitch.observe_window(frame(450)).unwrap());
        assert!(stitch.observe_window(RgbaImage::new(200, 300)).is_err());
    }
    #[test]
    fn window_focus_change_does_not_become_a_scrolling_header() {
        let page = page();
        let frame = |offset, active| {
            let mut window = RgbaImage::from_pixel(280, 500, image::Rgba([30, 40, 50, 255]));
            image::imageops::replace(
                &mut window,
                &image::imageops::crop_imm(&page, 0, offset, 240, 400).to_image(),
                20,
                70,
            );
            if active {
                for y in 0..28 {
                    for x in 0..280 {
                        window.put_pixel(x, y, image::Rgba([80, 80, 80, 255]));
                    }
                }
            }
            window
        };
        let mut stitch = Stitcher::window(frame(0, false));
        assert!(!stitch.observe_window(frame(0, true)).unwrap());
        assert!(!stitch.observe_window(frame(0, true)).unwrap());
        assert!(stitch.observe_window(frame(180, true)).unwrap());
        assert_eq!(stitch.image.height(), 580);
        assert_eq!(
            image::imageops::crop_imm(&stitch.image, 8, 0, 240, 580).to_image(),
            image::imageops::crop_imm(&page, 0, 0, 240, 580).to_image()
        );
    }
    #[test]
    fn stitches_variable_overlaps_without_duplicate_rows() {
        let page = page();
        let frame = |y| image::imageops::crop_imm(&page, 0, y, 240, 400).to_image();
        let mut stitch = Stitcher::new(frame(0));
        assert!(!stitch.append(frame(0)).unwrap());
        assert!(stitch.append(frame(180)).unwrap());
        assert!(stitch.append(frame(420)).unwrap());
        assert_eq!(stitch.sections, 3);
        assert_eq!(
            stitch.image,
            image::imageops::crop_imm(&page, 0, 0, 240, 820).to_image()
        );
    }
    #[test]
    fn mismatches_leave_accepted_image_unchanged() {
        let page = page();
        let mut stitch = Stitcher::new(image::imageops::crop_imm(&page, 0, 0, 240, 400).to_image());
        let before = stitch.image.clone();
        assert!(stitch.append(RgbaImage::new(240, 400)).is_err());
        assert!(stitch.append(RgbaImage::new(200, 400)).is_err());
        assert_eq!(stitch.image, before);
        assert_eq!(stitch.sections, 1);
    }
}
