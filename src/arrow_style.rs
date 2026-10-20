//! Shared arrow geometry for screen rendering, export, bounds and hit testing.
use crate::document::{Annotation, Point};
use eframe::egui::{Pos2, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pattern {
    #[default]
    Solid,
    Dashed,
    Dotted,
    DashDot,
    LongDash,
}

impl Pattern {
    pub const ALL: [Self; 5] = [
        Self::Solid,
        Self::Dashed,
        Self::Dotted,
        Self::DashDot,
        Self::LongDash,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::Dashed => "Dashed",
            Self::Dotted => "Dotted",
            Self::DashDot => "Dash-dot",
            Self::LongDash => "Long dash",
        }
    }
    /// Painted distances along a path. Gaps stay continuous across corners and curves.
    pub fn spans(self, length: f32, width: f32) -> Vec<(f32, f32)> {
        if length <= 0. {
            return Vec::new();
        }
        if self == Self::Solid {
            return vec![(0., length)];
        }
        let unit = width.max(1.);
        let mut result = Vec::new();
        let mut offset = 0.;
        let mut index = 0;
        while offset < length && result.len() < 4096 {
            let dot = self == Self::Dotted || (self == Self::DashDot && index % 2 == 1);
            let dash = if dot {
                0.01
            } else if self == Self::LongDash {
                unit * 8.
            } else {
                unit * 4.
            };
            result.push((offset, (offset + dash).min(length)));
            offset += (dash + unit * 3.).max(length / 4096.);
            index += 1;
        }
        result
    }
}

/// Shared outline strokes for the native canvas and raster/PDF export.
pub fn patterned_paths(
    points: &[Pos2],
    closed: bool,
    pattern: Pattern,
    width: f32,
) -> Vec<Vec<Pos2>> {
    if points.len() < 2 {
        return Vec::new();
    }
    let mut points = points.to_vec();
    if closed {
        points.push(points[0]);
    }
    if pattern == Pattern::Solid {
        return vec![points];
    }
    let mut distances = vec![0.];
    for pair in points.windows(2) {
        distances.push(distances.last().unwrap() + pair[0].distance(pair[1]));
    }
    let at = |d: f32| {
        let i = distances
            .partition_point(|v| *v < d)
            .clamp(1, points.len() - 1);
        points[i - 1].lerp(
            points[i],
            ((d - distances[i - 1]) / (distances[i] - distances[i - 1]).max(f32::EPSILON))
                .clamp(0., 1.),
        )
    };
    pattern
        .spans(*distances.last().unwrap(), width)
        .into_iter()
        .map(|(lo, hi)| {
            let mut path = vec![at(lo)];
            let first = distances.partition_point(|d| *d <= lo);
            let last = distances.partition_point(|d| *d < hi);
            path.extend_from_slice(&points[first..last]);
            path.push(at(hi));
            path
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Endpoint {
    #[default]
    None,
    Triangle,
    Open,
    Circle,
    Diamond,
    Bar,
}

impl Endpoint {
    pub const ALL: [Self; 6] = [
        Self::None,
        Self::Triangle,
        Self::Open,
        Self::Circle,
        Self::Diamond,
        Self::Bar,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Triangle => "Arrow",
            Self::Open => "Open arrow",
            Self::Circle => "Circle",
            Self::Diamond => "Diamond",
            Self::Bar => "Bar",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ArrowStyle {
    pub pattern: Pattern,
    pub start: Endpoint,
    pub end: Endpoint,
    pub start_size: f32,
    pub end_size: f32,
    /// Absolute image-space cubic Bézier controls; absent for a straight line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curve: Option<[Point; 2]>,
}

impl Default for ArrowStyle {
    fn default() -> Self {
        Self {
            pattern: Pattern::Solid,
            start: Endpoint::None,
            end: Endpoint::Triangle,
            start_size: 3.,
            end_size: 3.,
            curve: None,
        }
    }
}

impl ArrowStyle {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
    pub fn validate(&self) -> Result<(), String> {
        if [self.start_size, self.end_size]
            .iter()
            .any(|v| !v.is_finite() || !(1.0..=8.0).contains(v))
        {
            return Err("Arrow endpoint sizes must be between 1 and 8.".into());
        }
        if self.curve.is_some_and(|points| {
            points.iter().any(|p| {
                !p.x.is_finite()
                    || !p.y.is_finite()
                    || p.x.abs() > 1_000_000.
                    || p.y.abs() > 1_000_000.
            })
        }) {
            return Err("Arrow curve controls must be finite image coordinates.".into());
        }
        Ok(())
    }
}

pub struct Part {
    pub points: Vec<Pos2>,
    pub filled: bool,
}

fn endpoint(tip: Pos2, direction: Vec2, length: f32, cap: Endpoint) -> Option<Part> {
    let normal = Vec2::new(-direction.y, direction.x);
    let base = tip - direction * length;
    let half = length * 0.45;
    let points = match cap {
        Endpoint::None => return None,
        Endpoint::Triangle => vec![tip, base + normal * half, base - normal * half],
        Endpoint::Open => vec![base + normal * half, tip, base - normal * half],
        Endpoint::Bar => vec![tip + normal * half, tip - normal * half],
        Endpoint::Diamond => vec![
            tip,
            tip - direction * length * 0.5 + normal * half,
            base,
            tip - direction * length * 0.5 - normal * half,
        ],
        Endpoint::Circle => {
            let center = tip - direction * length * 0.5;
            (0..32)
                .map(|i| {
                    let angle = i as f32 * std::f32::consts::TAU / 32.;
                    center + Vec2::angled(angle) * length * 0.5
                })
                .collect()
        }
    };
    Some(Part {
        points,
        filled: matches!(
            cap,
            Endpoint::Triangle | Endpoint::Circle | Endpoint::Diamond
        ),
    })
}

pub fn parts(a: &Annotation) -> Vec<Part> {
    if a.arrow.curve.is_some() {
        return curved_parts(a);
    }
    let start = Pos2::new(a.start.x, a.start.y);
    let end = Pos2::new(a.end.x, a.end.y);
    let vector = end - start;
    let length = vector.length();
    if length <= f32::EPSILON {
        return Vec::new();
    }
    let dir = vector / length;
    let cap_length = |size: f32| (a.stroke_width * 4.).max(12.) * size / 3.;
    // Split short arrows between their endpoints, without reversing the shaft.
    let mut start_length = if a.arrow.start == Endpoint::None {
        0.
    } else {
        cap_length(a.arrow.start_size)
    };
    let mut end_length = if a.arrow.end == Endpoint::None {
        0.
    } else {
        cap_length(a.arrow.end_size)
    };
    let ratio = (length / (start_length + end_length).max(f32::EPSILON)).min(1.);
    start_length *= ratio;
    end_length *= ratio;
    let inset = |cap: Endpoint, len: f32| match cap {
        Endpoint::Triangle => len,
        Endpoint::Circle | Endpoint::Diamond => len * 0.5,
        _ => 0.,
    };
    let shaft_start = start + dir * inset(a.arrow.start, start_length);
    let shaft_end = end - dir * inset(a.arrow.end, end_length);
    let shaft_length = (shaft_end - shaft_start).dot(dir).max(0.);
    let mut parts = Vec::new();
    if a.arrow.pattern == Pattern::Solid {
        parts.push(Part {
            points: vec![shaft_start, shaft_end],
            filled: false,
        });
    } else {
        for (from, to) in a.arrow.pattern.spans(shaft_length, a.stroke_width) {
            parts.push(Part {
                points: vec![shaft_start + dir * from, shaft_start + dir * to],
                filled: false,
            });
        }
    }
    if let Some(part) = endpoint(start, -dir, start_length, a.arrow.start) {
        parts.push(part);
    }
    if let Some(part) = endpoint(end, dir, end_length, a.arrow.end) {
        parts.push(part);
    }
    parts
}

/// Controls and on-curve positions are shared by editing, hit testing and export.
pub fn controls(a: &Annotation) -> [Pos2; 2] {
    let s = Pos2::new(a.start.x, a.start.y);
    let e = Pos2::new(a.end.x, a.end.y);
    a.arrow
        .curve
        .map(|c| c.map(|p| Pos2::new(p.x, p.y)))
        .unwrap_or([s + (e - s) / 3., s + (e - s) * 2. / 3.])
}

pub fn curve_point(a: &Annotation, t: f32) -> Pos2 {
    let s = Pos2::new(a.start.x, a.start.y);
    let e = Pos2::new(a.end.x, a.end.y);
    let [c1, c2] = controls(a);
    let u = 1. - t;
    (s.to_vec2() * u * u * u
        + c1.to_vec2() * 3. * u * u * t
        + c2.to_vec2() * 3. * u * t * t
        + e.to_vec2() * t * t * t)
        .to_pos2()
}

fn curved_parts(a: &Annotation) -> Vec<Part> {
    let start = Pos2::new(a.start.x, a.start.y);
    let end = Pos2::new(a.end.x, a.end.y);
    let [c1, c2] = controls(a);
    let steps = ((start.distance(c1) + c1.distance(c2) + c2.distance(end)) / 3.)
        .ceil()
        .clamp(32., 512.) as usize;
    let points: Vec<_> = (0..=steps)
        .map(|i| curve_point(a, i as f32 / steps as f32))
        .collect();
    let mut distances = vec![0.];
    for p in points.windows(2) {
        distances.push(distances.last().unwrap() + p[0].distance(p[1]));
    }
    let length = *distances.last().unwrap();
    if length <= f32::EPSILON {
        return Vec::new();
    }
    let cap = |endpoint: Endpoint, size: f32| {
        if endpoint == Endpoint::None {
            0.
        } else {
            (a.stroke_width * 4.).max(12.) * size / 3.
        }
    };
    let mut sl = cap(a.arrow.start, a.arrow.start_size);
    let mut el = cap(a.arrow.end, a.arrow.end_size);
    let ratio = (length / (sl + el).max(f32::EPSILON)).min(1.);
    sl *= ratio;
    el *= ratio;
    let inset = |endpoint: Endpoint, size: f32| match endpoint {
        Endpoint::Triangle => size,
        Endpoint::Circle | Endpoint::Diamond => size * 0.5,
        _ => 0.,
    };
    let from = inset(a.arrow.start, sl);
    let to = length - inset(a.arrow.end, el);
    let at_distance = |d: f32| {
        let i = distances
            .partition_point(|v| *v < d)
            .clamp(1, points.len() - 1);
        points[i - 1].lerp(
            points[i],
            ((d - distances[i - 1]) / (distances[i] - distances[i - 1]).max(f32::EPSILON))
                .clamp(0., 1.),
        )
    };
    let slice = |lo: f32, hi: f32| {
        let mut p = vec![at_distance(lo)];
        p.extend(
            points
                .iter()
                .zip(&distances)
                .filter(|(_, d)| **d > lo && **d < hi)
                .map(|(p, _)| *p),
        );
        p.push(at_distance(hi));
        Part {
            points: p,
            filled: false,
        }
    };
    let mut result = Vec::new();
    if a.arrow.pattern == Pattern::Solid {
        result.push(slice(from, to));
    } else {
        for (lo, hi) in a.arrow.pattern.spans(to - from, a.stroke_width) {
            result.push(slice(from + lo, from + hi));
        }
    }
    let tangent = |v: Vec2, fallback: Vec2| {
        if v.length_sq() > 0.0001 {
            v.normalized()
        } else {
            fallback.normalized()
        }
    };
    if let Some(p) = endpoint(
        start,
        -tangent(c1 - start, points[1] - start),
        sl,
        a.arrow.start,
    ) {
        result.push(p);
    }
    if let Some(p) = endpoint(
        end,
        tangent(end - c2, end - points[points.len() - 2]),
        el,
        a.arrow.end,
    ) {
        result.push(p);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document::{self, AnnotationKind, Document},
        markup_tests::object,
        render,
    };
    use std::sync::Arc;

    #[test]
    fn expanded_patterns_keep_gaps_across_corners_and_bound_large_paths() {
        let path = [Pos2::new(0., 0.), Pos2::new(12., 0.), Pos2::new(12., 100.)];
        let dashed = patterned_paths(&path, false, Pattern::Dashed, 4.);
        assert_eq!(dashed[0].len(), 3);
        for (actual, expected) in dashed[0].iter().zip([path[0], path[1], Pos2::new(12., 4.)]) {
            assert!(actual.distance(expected) < 0.001);
        }
        assert!(dashed[1][0].distance(Pos2::new(12., 16.)) < 0.001);
        let spans = Pattern::DashDot.spans(100., 2.);
        assert!((spans[0].1 - spans[0].0 - 8.).abs() < 0.01);
        assert!(spans[1].1 - spans[1].0 < 0.02);
        assert!((spans[2].1 - spans[2].0 - 8.).abs() < 0.01);
        assert_eq!(Pattern::LongDash.spans(100., 2.)[0], (0., 16.));
        for p in Pattern::ALL {
            assert!(p.spans(2_000_000., 0.1).len() <= 4096);
        }
    }

    #[test]
    fn patterned_shapes_and_curves_survive_projects_and_undo() {
        use crate::document::EditHistory;
        let dir = tempfile::tempdir().unwrap();
        for pattern in [Pattern::DashDot, Pattern::LongDash] {
            for kind in crate::shapes::CHOICES
                .iter()
                .chain(crate::shapes::THREE_D.iter())
                .map(|(kind, _)| *kind)
                .chain([AnnotationKind::Arrow])
            {
                let mut a = object(kind, [20., 30.], [270., 170.]);
                a.arrow.pattern = pattern;
                a.fill = false;
                if kind == AnnotationKind::Arrow {
                    a.arrow.curve = Some([Point { x: 40., y: 170. }, Point { x: 240., y: 20. }]);
                }
                let mut d = Document {
                    image: Arc::new(image::RgbaImage::new(300, 200)),
                    annotations: vec![a.clone()],
                    geometry: Default::default(),
                };
                let flat = render::flatten(&d).unwrap();
                assert!(flat.pixels().any(|p| p[3] > 0), "{kind:?}");
                let path = dir.path().join("pattern.myimage");
                document::save_project(&d, &path).unwrap();
                let restored = document::load_project(&path).unwrap();
                assert_eq!(restored.edit_state(), d.edit_state());
                assert_eq!(render::flatten(&restored).unwrap(), flat);
                let mut h = EditHistory::new(d.edit_state());
                let before = d.edit_state();
                d.annotations[0].arrow.pattern = Pattern::Solid;
                assert_ne!(render::flatten(&d).unwrap(), flat);
                h.commit(before, &d);
                assert!(h.undo(&mut d));
                assert_eq!(render::flatten(&d).unwrap(), flat);
                let mut bytes = std::fs::read(&path).unwrap();
                bytes[8..12].copy_from_slice(&20u32.to_le_bytes());
                std::fs::write(&path, bytes).unwrap();
                assert!(
                    document::load_project(&path)
                        .unwrap_err()
                        .contains("version 21")
                );
            }
        }
    }

    #[test]
    fn cubic_curve_round_trips_and_export_follows_bend_and_tangent() {
        let mut a = object(AnnotationKind::Arrow, [30., 160.], [260., 160.]);
        a.arrow.curve = Some([Point { x: 70., y: 20. }, Point { x: 220., y: 20. }]);
        let mid = curve_point(&a, 0.5);
        assert!((mid.y - 55.).abs() < 0.01);
        let pieces = parts(&a);
        assert!(pieces[0].points.len() > 20);
        let tip = pieces.last().unwrap();
        assert_eq!(tip.points[0], Pos2::new(260., 160.));
        let base = tip.points[1].lerp(tip.points[2], 0.5);
        assert!(
            (Pos2::new(260., 160.) - base)
                .normalized()
                .dot((Pos2::new(260., 160.) - Pos2::new(220., 20.)).normalized())
                > 0.999
        );
        let d = Document {
            image: Arc::new(image::RgbaImage::new(320, 220)),
            annotations: vec![a],
            geometry: Default::default(),
        };
        let flat = render::flatten(&d).unwrap();
        assert!(flat.get_pixel(mid.x.round() as u32, mid.y.round() as u32)[3] > 0);
        assert_eq!(
            flat.get_pixel(145, 160)[3],
            0,
            "No straight chord should be rendered"
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("curve.myimage");
        document::save_project(&d, &path).unwrap();
        let restored = document::load_project(&path).unwrap();
        assert_eq!(restored.edit_state(), d.edit_state());
        assert_eq!(render::flatten(&restored).unwrap(), flat);
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[8..12].copy_from_slice(&17u32.to_le_bytes());
        std::fs::write(&path, bytes).unwrap();
        assert!(
            document::load_project(&path)
                .unwrap_err()
                .contains("version 18")
        );
        let mut invalid = d.annotations[0].arrow.clone();
        invalid.curve.as_mut().unwrap()[0].x = f32::NAN;
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn styled_arrows_round_trip_and_render_after_crop_resize_and_rotate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("arrows.myimage");
        let mut d = Document {
            image: Arc::new(image::RgbaImage::new(300, 220)),
            annotations: vec![],
            geometry: Default::default(),
        };
        for (i, cap) in Endpoint::ALL.into_iter().enumerate() {
            let y = 20. + i as f32 * 35.;
            let mut a = object(AnnotationKind::Arrow, [20., y], [270., y]);
            a.arrow = ArrowStyle {
                start: cap,
                end: cap,
                pattern: if i % 2 == 0 {
                    Pattern::Dashed
                } else {
                    Pattern::Dotted
                },
                start_size: 2.,
                end_size: 5.,
                curve: None,
            };
            d.annotations.push(a);
        }
        d.geometry.quarter_turns = 1;
        d.geometry.resize = Some([440, 600]);
        let expected = render::flatten(&d).unwrap();
        assert!(expected.pixels().any(|p| p[3] > 0));
        document::save_project(&d, &path).unwrap();
        let restored = document::load_project(&path).unwrap();
        assert_eq!(restored.edit_state(), d.edit_state());
        assert_eq!(render::flatten(&restored).unwrap(), expected);
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[8..12].copy_from_slice(&15u32.to_le_bytes());
        std::fs::write(&path, bytes).unwrap();
        assert!(
            document::load_project(&path)
                .unwrap_err()
                .contains("version 16")
        );
    }

    #[test]
    fn old_arrows_default_to_original_shape_and_style_validation_rejects_bad_data() {
        let a = object(AnnotationKind::Arrow, [10., 30.], [110., 30.]);
        let mut json = serde_json::to_value(&a).unwrap();
        json.as_object_mut().unwrap().remove("arrow");
        let restored: Annotation = serde_json::from_value(json).unwrap();
        assert!(restored.arrow.is_default());
        let pieces = parts(&restored);
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[1].points[0], Pos2::new(110., 30.));
        let mut bad = restored;
        bad.arrow.end_size = f32::NAN;
        assert!(document::validate_annotation(&bad).is_err());
        bad.arrow.end_size = 3.;
        bad.kind = AnnotationKind::Rectangle;
        bad.arrow.pattern = Pattern::Dotted;
        assert!(document::validate_annotation(&bad).is_ok());
        bad.arrow.start = Endpoint::Circle;
        assert!(document::validate_annotation(&bad).is_err());
    }

    #[test]
    fn every_endpoint_is_inside_selectable_bounds_and_short_arrows_stay_finite() {
        for cap in Endpoint::ALL {
            for length in [0.001, 4., 40., 1000.] {
                let mut a = object(
                    AnnotationKind::Arrow,
                    [50., 50.],
                    [50. + length, 50. + length],
                );
                a.arrow.start = cap;
                a.arrow.end = cap;
                a.arrow.start_size = 8.;
                a.arrow.end_size = 8.;
                let bounds = render::annotation_bounds(&a);
                for p in parts(&a).into_iter().flat_map(|part| part.points) {
                    assert!(p.is_finite());
                    assert!(bounds.contains(p));
                }
            }
        }
    }

    #[test]
    fn plain_line_omits_arrowhead_and_dashes_leave_gaps_in_export() {
        let mut a = object(AnnotationKind::Arrow, [10., 30.], [190., 30.]);
        a.stroke_width = 4.;
        a.arrow.end = Endpoint::None;
        let d = |a| Document {
            image: Arc::new(image::RgbaImage::new(200, 60)),
            annotations: vec![a],
            geometry: Default::default(),
        };
        let solid = render::flatten(&d(a.clone())).unwrap();
        assert_eq!(solid.get_pixel(174, 40)[3], 0);
        a.arrow.pattern = Pattern::Dashed;
        let dashed = render::flatten(&d(a)).unwrap();
        let solid_ink = (10..190).filter(|&x| solid.get_pixel(x, 30)[3] > 0).count();
        let dashed_ink = (10..190)
            .filter(|&x| dashed.get_pixel(x, 30)[3] > 0)
            .count();
        assert!(dashed_ink > 50 && dashed_ink < solid_ink - 20);
    }
}
