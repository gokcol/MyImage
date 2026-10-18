//! Original-resolution, four-connected flood fill. Saved scanline masks keep
//! preview downsampling from bridging thin solid boundaries.
use crate::document::{Annotation, Point};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const MAX_RUNS: usize = 65_536;
pub const MAX_TOTAL_RUNS: usize = 131_072;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FillRegion {
    /// Seed relative to the annotation rectangle; stays meaningful after moving/resizing.
    pub seed: Point,
    pub tolerance: u8,
    pub global: bool,
    pub width: u32,
    pub height: u32,
    /// Sorted [row, inclusive left, exclusive right] runs in mask coordinates.
    pub runs: Arc<Vec<[u32; 3]>>,
}
impl FillRegion {
    pub fn validate(&self) -> Result<(), String> {
        if self.width == 0
            || self.height == 0
            || self.width.max(self.height) > 32_768
            || u64::from(self.width) * u64::from(self.height) > 100_000_000
            || self.tolerance > 100
            || !self.seed.x.is_finite()
            || !self.seed.y.is_finite()
            || !(0.0..1.0).contains(&self.seed.x)
            || !(0.0..1.0).contains(&self.seed.y)
            || self.runs.is_empty()
            || self.runs.len() > MAX_RUNS
        {
            return Err("Invalid fill region or mask size.".into());
        }
        let mut previous: Option<[u32; 3]> = None;
        for &[y, left, right] in self.runs.iter() {
            if y >= self.height
                || right > self.width
                || left >= right
                || previous.is_some_and(|[py, _, pr]| y < py || (y == py && left <= pr))
            {
                return Err("Invalid fill mask scanlines.".into());
            }
            previous = Some([y, left, right]);
        }
        Ok(())
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        if !(0.0..1.0).contains(&x) || !(0.0..1.0).contains(&y) {
            return false;
        }
        let px = (x * self.width as f32).floor() as u32;
        let py = (y * self.height as f32).floor() as u32;
        let i = self
            .runs
            .partition_point(|r| r[0] < py || (r[0] == py && r[2] <= px));
        self.runs
            .get(i)
            .is_some_and(|r| r[0] == py && r[1] <= px && px < r[2])
    }
}
struct Bits(Vec<u64>);
impl Bits {
    fn new(size: usize) -> Self {
        Self(vec![0; size.div_ceil(64)])
    }
    fn get(&self, i: usize) -> bool {
        self.0[i / 64] & (1 << (i % 64)) != 0
    }
    fn set(&mut self, i: usize, on: bool) {
        if on {
            self.0[i / 64] |= 1 << (i % 64);
        } else {
            self.0[i / 64] &= !(1 << (i % 64));
        }
    }
}
fn matches(a: Rgba<u8>, b: Rgba<u8>, tolerance: u8) -> bool {
    let premul = |p: Rgba<u8>| {
        [
            ((p[0] as u32 * p[3] as u32 + 127) / 255) as u8,
            ((p[1] as u32 * p[3] as u32 + 127) / 255) as u8,
            ((p[2] as u32 * p[3] as u32 + 127) / 255) as u8,
            p[3],
        ]
    };
    premul(a)
        .into_iter()
        .zip(premul(b))
        .all(|(a, b)| u32::from(a.abs_diff(b)) * 100 <= u32::from(tolerance) * 255)
}
fn take_run(bits: &mut Bits, width: u32, x: u32, y: u32) -> [u32; 3] {
    let row = y as usize * width as usize;
    let mut left = x;
    let mut right = x + 1;
    while left > 0 && bits.get(row + left as usize - 1) {
        left -= 1;
    }
    while right < width && bits.get(row + right as usize) {
        right += 1;
    }
    for px in left..right {
        bits.set(row + px as usize, false);
    }
    [y, left, right]
}
pub fn build(
    image: &RgbaImage,
    annotation: &Annotation,
    seed: Point,
    tolerance: u8,
    global: bool,
) -> Result<FillRegion, String> {
    let bounds = [
        annotation.start.x.floor(),
        annotation.start.y.floor(),
        annotation.end.x.ceil(),
        annotation.end.y.ceil(),
    ];
    if bounds
        .iter()
        .any(|v| !v.is_finite() || v.abs() > 1_000_000.)
    {
        return Err("Invalid fill bounds.".into());
    }
    let rect = bounds.map(|v| v as i64);
    let mask_width = rect[2] - rect[0];
    let mask_height = rect[3] - rect[1];
    if mask_width <= 0
        || mask_height <= 0
        || mask_width.max(mask_height) > 32_768
        || mask_width * mask_height > 100_000_000
    {
        return Err("The fill region is too large. Select a smaller area.".into());
    }
    let left = annotation.start.x.floor().max(0.) as u32;
    let top = annotation.start.y.floor().max(0.) as u32;
    let right = annotation.end.x.ceil().clamp(0., image.width() as f32) as u32;
    let bottom = annotation.end.y.ceil().clamp(0., image.height() as f32) as u32;
    if right <= left
        || bottom <= top
        || tolerance > 100
        || !seed.x.is_finite()
        || !seed.y.is_finite()
        || seed.x < left as f32
        || seed.x >= right as f32
        || seed.y < top as f32
        || seed.y >= bottom as f32
    {
        return Err("Click inside the canvas or selected area to fill it.".into());
    }
    let width = right - left;
    let height = bottom - top;
    let sx = seed.x.floor() as u32 - left;
    let sy = seed.y.floor() as u32 - top;
    let color = *image.get_pixel(left + sx, top + sy);
    let mut bits = Bits::new(width as usize * height as usize);
    for y in top..bottom {
        for (l, r) in crate::area_pixels::masked_spans(annotation, rect, y, left, right) {
            for x in l..r {
                if matches(*image.get_pixel(x, y), color, tolerance) {
                    bits.set(
                        (y - top) as usize * width as usize + (x - left) as usize,
                        true,
                    );
                }
            }
        }
    }
    if !bits.get(sy as usize * width as usize + sx as usize) {
        return Err("Click inside the selected area to fill it.".into());
    }
    let mut runs = Vec::new();
    if global {
        for y in 0..height {
            let mut x = 0;
            while x < width {
                if bits.get(y as usize * width as usize + x as usize) {
                    let run = take_run(&mut bits, width, x, y);
                    x = run[2];
                    runs.push(run);
                    if runs.len() > MAX_RUNS {
                        return Err(
                            "This fill is too detailed. Reduce tolerance or select a smaller area."
                                .into(),
                        );
                    }
                } else {
                    x += 1;
                }
            }
        }
    } else {
        runs.push(take_run(&mut bits, width, sx, sy));
        let mut pending = vec![runs[0]];
        while let Some([y, left, right]) = pending.pop() {
            for ny in [y.checked_sub(1), (y + 1 < height).then_some(y + 1)]
                .into_iter()
                .flatten()
            {
                let mut x = left;
                while x < right {
                    if bits.get(ny as usize * width as usize + x as usize) {
                        let run = take_run(&mut bits, width, x, ny);
                        x = run[2];
                        runs.push(run);
                        pending.push(run);
                        if runs.len() > MAX_RUNS {
                            return Err("This fill is too detailed. Reduce tolerance or select a smaller area.".into());
                        }
                    } else {
                        x += 1;
                    }
                }
            }
        }
        runs.sort_unstable();
    }
    // Adjacent runs discovered from separate branches can merge on the same row.
    let mut merged: Vec<[u32; 3]> = Vec::new();
    for run in runs {
        if let Some(last) = merged.last_mut()
            && last[0] == run[0]
            && last[2] == run[1]
        {
            last[2] = run[2];
        } else {
            merged.push(run);
        }
    }
    // Keep the full mask coordinate frame when a moved fill is partly off-canvas.
    // Clipping the frame itself would stretch the saved mask when rendered again.
    let dx = (i64::from(left) - rect[0]) as u32;
    let dy = (i64::from(top) - rect[1]) as u32;
    for run in &mut merged {
        run[0] += dy;
        run[1] += dx;
        run[2] += dx;
    }
    let region = FillRegion {
        seed: Point {
            x: ((sx + dx) as f32 + 0.5) / mask_width as f32,
            y: ((sy + dy) as f32 + 0.5) / mask_height as f32,
        },
        tolerance,
        global,
        width: mask_width as u32,
        height: mask_height as u32,
        runs: Arc::new(merged),
    };
    region.validate()?;
    Ok(region)
}
/// Replace matching pixels, including alpha. Alpha zero clears them instead of
/// blending a transparent brush over an unchanged opaque background.
pub fn apply(
    image: &mut RgbaImage,
    annotation: &Annotation,
    scale: [f32; 2],
) -> Result<(), String> {
    let region = annotation
        .fill_region
        .as_ref()
        .ok_or("Fill mask is missing.")?;
    let left = annotation.start.x * scale[0];
    let top = annotation.start.y * scale[1];
    let width = (annotation.end.x - annotation.start.x) * scale[0];
    let height = (annotation.end.y - annotation.start.y) * scale[1];
    if width <= 0. || height <= 0. {
        return Ok(());
    }
    let color = Rgba(if annotation.color[3] == 0 {
        [0; 4]
    } else {
        annotation.color
    });
    // Destination pixel centres sample the original-resolution mask. Never flood
    // the downsampled preview, where a one-pixel boundary could have disappeared.
    for &[row, l, r] in region.runs.iter() {
        let y0 = (top + row as f32 / region.height as f32 * height - 0.5)
            .ceil()
            .clamp(0., image.height() as f32) as u32;
        let y1 = (top + (row + 1) as f32 / region.height as f32 * height - 0.5)
            .ceil()
            .clamp(0., image.height() as f32) as u32;
        let x0 = (left + l as f32 / region.width as f32 * width - 0.5)
            .ceil()
            .clamp(0., image.width() as f32) as u32;
        let x1 = (left + r as f32 / region.width as f32 * width - 0.5)
            .ceil()
            .clamp(0., image.width() as f32) as u32;
        for y in y0..y1 {
            for x in x0..x1 {
                image.put_pixel(x, y, color);
            }
        }
    }
    Ok(())
}
