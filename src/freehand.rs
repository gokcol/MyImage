//! Editable polyline operations. Erasing clips geometry; it never changes pixels.
use crate::document::{Annotation, AnnotationKind, Point};

pub(super) fn sync_bounds(stroke: &mut Annotation) {
    let Some(first) = stroke.points.first().copied() else {
        return;
    };
    stroke.start = first;
    stroke.end = first;
    for point in &stroke.points {
        stroke.start.x = stroke.start.x.min(point.x);
        stroke.start.y = stroke.start.y.min(point.y);
        stroke.end.x = stroke.end.x.max(point.x);
        stroke.end.y = stroke.end.y.max(point.y);
    }
}

/// Remove the swept round eraser from strokes while preserving every other
/// object's layer position. A failed budget check leaves the document untouched.
pub(super) fn erase(
    annotations: &[Annotation],
    from: Point,
    to: Point,
    diameter: f32,
) -> Result<Option<Vec<Annotation>>, &'static str> {
    let mut result = Vec::with_capacity(annotations.len());
    let mut changed = false;
    let mut point_count = 0;
    for annotation in annotations {
        if annotation.kind != AnnotationKind::Freehand
            || annotation.locked()
            || !annotation.visible()
        {
            point_count += annotation.points.len();
            result.push(annotation.clone());
        } else {
            // Inflate by the pen radius so the surviving round caps do not
            // intrude into the erased region.
            let radius = (diameter + annotation.stroke_width) as f64 * 0.5;
            let paths = clip_path(&annotation.points, from, to, radius);
            if paths.len() == 1 && paths.first() == Some(&annotation.points) {
                point_count += annotation.points.len();
                if point_count > crate::document::MAX_TOTAL_STROKE_POINTS {
                    return Err(
                        "This erase would exceed the stroke-point limit. Remove a stroke before continuing.",
                    );
                }
                result.push(annotation.clone());
                if result.len() > crate::document::MAX_ANNOTATIONS {
                    return Err(
                        "This erase would create too many stroke pieces. Remove a stroke before continuing.",
                    );
                }
                continue;
            }
            changed = true;
            let mut style = annotation.clone();
            style.points.clear();
            // Text has no meaning for a pen stroke. Do not multiply an imported
            // unused text payload when one stroke becomes many pieces.
            style.text.clear();
            for points in paths {
                point_count += points.len();
                if points.len() > crate::document::MAX_STROKE_POINTS
                    || point_count > crate::document::MAX_TOTAL_STROKE_POINTS
                {
                    return Err(
                        "This erase would exceed the stroke-point limit. Undo or remove a stroke before continuing.",
                    );
                }
                let mut piece = style.clone();
                piece.points = points;
                sync_bounds(&mut piece);
                result.push(piece);
            }
        }
        if point_count > crate::document::MAX_TOTAL_STROKE_POINTS {
            return Err("This erase would exceed the stroke-point limit.");
        }
        if result.len() > crate::document::MAX_ANNOTATIONS {
            return Err(
                "This erase would create too many stroke pieces. Remove a stroke before continuing.",
            );
        }
    }
    Ok(changed.then_some(result))
}

fn clip_path(points: &[Point], from: Point, to: Point, radius: f64) -> Vec<Vec<Point>> {
    if points.len() == 1 {
        return if capsule_interval(points[0], points[0], from, to, radius).is_some() {
            Vec::new()
        } else {
            vec![points.to_vec()]
        };
    }
    let mut result = Vec::new();
    let mut current = Vec::new();
    let mut cut = false;
    for pair in points.windows(2) {
        let [a, b] = [pair[0], pair[1]];
        let interval = capsule_interval(a, b, from, to, radius).filter(|(enter, exit)| {
            // The clipped boundary is stored as f32. Ignore a partial cut smaller
            // than its rounding error, so repeating an erase does not nibble a
            // saved endpoint or create phantom history changes.
            let magnitude = [a.x, a.y, b.x, b.y]
                .into_iter()
                .map(f32::abs)
                .fold(1.0, f32::max);
            let tolerance = 0.00001 + f32::EPSILON as f64 * magnitude as f64 * 4.0;
            (*enter == 0.0 && *exit == 1.0)
                || (*exit - *enter) * (b.x as f64 - a.x as f64).hypot(b.y as f64 - a.y as f64)
                    > tolerance
        });
        if let Some((enter, exit)) = interval {
            cut = true;
            if enter > 1e-9 {
                push_distinct(&mut current, a);
                push_distinct(&mut current, interpolate(a, b, enter));
            }
            if !current.is_empty() {
                result.push(std::mem::take(&mut current));
            }
            if exit < 1.0 - 1e-9 {
                push_distinct(&mut current, interpolate(a, b, exit));
                push_distinct(&mut current, b);
            }
        } else {
            push_distinct(&mut current, a);
            push_distinct(&mut current, b);
        }
    }
    if !current.is_empty() {
        result.push(current);
    }
    if cut { result } else { vec![points.to_vec()] }
}

fn push_distinct(points: &mut Vec<Point>, point: Point) {
    if points.last() != Some(&point) {
        points.push(point);
    }
}

fn interpolate(a: Point, b: Point, t: f64) -> Point {
    Point {
        x: (a.x as f64 + (b.x as f64 - a.x as f64) * t) as f32,
        y: (a.y as f64 + (b.y as f64 - a.y as f64) * t) as f32,
    }
}

/// Parameter interval of segment p..q inside a capsule a..b. The capsule is
/// convex: the union of its two end disks and axial rectangle is one interval.
fn capsule_interval(p: Point, q: Point, a: Point, b: Point, radius: f64) -> Option<(f64, f64)> {
    let mut intervals = Vec::with_capacity(3);
    let d = [q.x as f64 - p.x as f64, q.y as f64 - p.y as f64];
    let length2 = d[0] * d[0] + d[1] * d[1];
    for center in [a, b] {
        let v = [p.x as f64 - center.x as f64, p.y as f64 - center.y as f64];
        let c = v[0] * v[0] + v[1] * v[1] - radius * radius;
        if length2 <= f64::EPSILON {
            if c <= 0.0 {
                intervals.push((0.0_f64, 1.0_f64));
            }
        } else {
            let dot = v[0] * d[0] + v[1] * d[1];
            let discriminant = dot * dot - length2 * c;
            if discriminant > 0.0 {
                let root = discriminant.sqrt();
                let lo = ((-dot - root) / length2).max(0.0);
                let hi = ((-dot + root) / length2).min(1.0);
                if lo < hi {
                    intervals.push((lo, hi));
                }
            }
        }
    }
    let axis = [b.x as f64 - a.x as f64, b.y as f64 - a.y as f64];
    let length = axis[0].hypot(axis[1]);
    if length > f64::EPSILON {
        let unit = [axis[0] / length, axis[1] / length];
        let v = [p.x as f64 - a.x as f64, p.y as f64 - a.y as f64];
        let mut interval = (0.0_f64, 1.0_f64);
        let inside = [
            (
                v[0] * unit[0] + v[1] * unit[1],
                d[0] * unit[0] + d[1] * unit[1],
                0.0,
                length,
            ),
            (
                -v[0] * unit[1] + v[1] * unit[0],
                -d[0] * unit[1] + d[1] * unit[0],
                -radius,
                radius,
            ),
        ]
        .into_iter()
        .all(|(origin, delta, low, high)| {
            if delta.abs() <= f64::EPSILON {
                origin >= low && origin <= high
            } else {
                let t1 = (low - origin) / delta;
                let t2 = (high - origin) / delta;
                interval.0 = interval.0.max(t1.min(t2));
                interval.1 = interval.1.min(t1.max(t2));
                interval.0 < interval.1
            }
        });
        if inside {
            intervals.push(interval);
        }
    }
    intervals
        .into_iter()
        .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, y: f32) -> Point {
        Point { x, y }
    }

    #[test]
    fn erase_sweep_cuts_between_samples_and_clears_round_caps() {
        let split = clip_path(
            &[p(0.0, 50.0), p(100.0, 50.0)],
            p(50.0, 0.0),
            p(50.0, 100.0),
            7.0,
        );
        assert_eq!(
            split,
            vec![
                vec![p(0.0, 50.0), p(43.0, 50.0)],
                vec![p(57.0, 50.0), p(100.0, 50.0)]
            ]
        );
    }

    #[test]
    fn eraser_handles_dots_duplicates_endpoints_and_no_intersection() {
        assert!(clip_path(&[p(10.0, 10.0)], p(10.0, 0.0), p(10.0, 20.0), 2.0).is_empty());
        assert_eq!(
            clip_path(&[p(2.0, 2.0)], p(10.0, 0.0), p(10.0, 20.0), 2.0),
            vec![vec![p(2.0, 2.0)]]
        );
        let points = vec![p(0.0, 0.0), p(0.0, 0.0), p(100.0, 0.0)];
        assert_eq!(
            clip_path(&points, p(0.0, 0.0), p(0.0, 0.0), 5.0),
            vec![vec![p(5.0, 0.0), p(100.0, 0.0)]]
        );
        assert!(clip_path(&[p(0.0, 0.0), p(0.0, 0.0)], p(0.0, 0.0), p(0.0, 0.0), 5.0).is_empty());
        assert_eq!(
            clip_path(&points, p(10.0, 10.0), p(10.0, 20.0), 2.0),
            vec![points]
        );
    }

    #[test]
    fn one_sweep_can_split_a_loop_into_several_editable_paths() {
        let paths = clip_path(
            &[p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(0.0, 100.0)],
            p(50.0, -10.0),
            p(50.0, 110.0),
            10.0,
        );
        assert_eq!(paths.len(), 3);
        assert_eq!(
            paths[1],
            vec![p(60.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(60.0, 100.0)]
        );
    }

    #[test]
    fn repeating_an_erase_preserves_clipped_boundaries_exactly() {
        let from = p(43.321, -20.234);
        let to = p(60.71, 100.87);
        let pieces = clip_path(&[p(0.17, 40.21), p(100.23, 48.56)], from, to, 8.345);
        assert_eq!(pieces.len(), 2);
        for piece in pieces {
            assert_eq!(clip_path(&piece, from, to, 8.345), vec![piece]);
        }
    }
}
