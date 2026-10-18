//! Shared editable shape geometry for canvas, export, previews and hit testing.
use crate::document::AnnotationKind;
use eframe::egui::{Pos2, Rect, Vec2};
use std::f32::consts::{PI, TAU};

pub const CHOICES: [(AnnotationKind, &str); 10] = [
    (AnnotationKind::Rectangle, "Rectangle"),
    (AnnotationKind::RoundedRectangle, "Rounded rectangle"),
    (AnnotationKind::Ellipse, "Ellipse"),
    (AnnotationKind::Triangle, "Triangle"),
    (AnnotationKind::Diamond, "Diamond"),
    (AnnotationKind::Pentagon, "Pentagon"),
    (AnnotationKind::Hexagon, "Hexagon"),
    (AnnotationKind::Star, "Star"),
    (AnnotationKind::Cross, "Cross"),
    (AnnotationKind::Parallelogram, "Parallelogram"),
];

pub const THREE_D: [(AnnotationKind, &str); 7] = [
    (AnnotationKind::Cylinder, "Cylinder"),
    (AnnotationKind::Prism, "Prism"),
    (AnnotationKind::Cone, "Cone"),
    (AnnotationKind::Sphere, "Sphere"),
    (AnnotationKind::HexagonalPrism, "Hexagonal prism"),
    (AnnotationKind::Torus, "Torus"),
    (AnnotationKind::Tetrahedron, "Tetrahedron"),
];

pub fn name(kind: AnnotationKind) -> &'static str {
    CHOICES
        .iter()
        .chain(THREE_D.iter())
        .find(|(k, _)| *k == kind)
        .map(|(_, n)| *n)
        .unwrap_or("Shape")
}

/// Clockwise screen-coordinate outlines, normalized to the drag bounds.
pub fn outline(kind: AnnotationKind, rect: Rect) -> Vec<Pos2> {
    if !rect.is_positive() {
        return Vec::new();
    }
    let normalized: Vec<Vec2> = match kind {
        AnnotationKind::RoundedRectangle => {
            let r = rect.width().min(rect.height()) * 0.2;
            let mut points = Vec::new();
            for (center, angle) in [
                (rect.right_top() + Vec2::new(-r, r), -PI / 2.),
                (rect.right_bottom() + Vec2::new(-r, -r), 0.),
                (rect.left_bottom() + Vec2::new(r, -r), PI / 2.),
                (rect.left_top() + Vec2::new(r, r), PI),
            ] {
                for i in 0..=8 {
                    let a = angle + i as f32 * PI / 16.;
                    points.push(center + Vec2::angled(a) * r);
                }
            }
            return points;
        }
        AnnotationKind::Ellipse | AnnotationKind::Sphere | AnnotationKind::Torus => (0..48)
            .map(|i| Vec2::splat(0.5) + Vec2::angled(i as f32 * TAU / 48.) * 0.5)
            .collect(),
        AnnotationKind::Cylinder => arc([0.5, 0.16], [0.5, 0.16], PI, TAU, 24)
            .into_iter()
            .chain(arc([0.5, 0.84], [0.5, 0.16], 0., PI, 24))
            .collect(),
        AnnotationKind::Cone => std::iter::once(Vec2::new(0.5, 0.))
            .chain(arc([0.5, 0.84], [0.5, 0.16], 0., PI, 24))
            .collect(),
        AnnotationKind::Prism => [
            (0., 0.22),
            (0.32, 0.),
            (1., 0.),
            (1., 0.78),
            (0.68, 1.),
            (0., 1.),
        ]
        .into_iter()
        .map(|(x, y)| Vec2::new(x, y))
        .collect(),
        AnnotationKind::HexagonalPrism => [
            (0., 0.16),
            (0.25, 0.),
            (0.75, 0.),
            (1., 0.16),
            (1., 0.84),
            (0.75, 1.),
            (0.25, 1.),
            (0., 0.84),
        ]
        .into_iter()
        .map(|(x, y)| Vec2::new(x, y))
        .collect(),
        AnnotationKind::Tetrahedron => [(0.45, 0.), (1., 0.78), (0.35, 1.), (0., 0.68)]
            .into_iter()
            .map(|(x, y)| Vec2::new(x, y))
            .collect(),
        AnnotationKind::Triangle => vec![Vec2::new(0.5, 0.), Vec2::new(1., 1.), Vec2::new(0., 1.)],
        AnnotationKind::Diamond => vec![
            Vec2::new(0.5, 0.),
            Vec2::new(1., 0.5),
            Vec2::new(0.5, 1.),
            Vec2::new(0., 0.5),
        ],
        AnnotationKind::Pentagon | AnnotationKind::Hexagon | AnnotationKind::Star => {
            let n = match kind {
                AnnotationKind::Pentagon => 5,
                AnnotationKind::Hexagon => 6,
                _ => 10,
            };
            let mut p: Vec<Vec2> = (0..n)
                .map(|i| {
                    let radius = if kind == AnnotationKind::Star && i % 2 == 1 {
                        0.43
                    } else {
                        1.
                    };
                    Vec2::angled(-PI / 2. + i as f32 * TAU / n as f32) * radius
                })
                .collect();
            let bounds = p.iter().fold(Rect::NOTHING, |r, p| {
                r.union(Rect::from_min_max(p.to_pos2(), p.to_pos2()))
            });
            for v in &mut p {
                *v = (*v - bounds.min.to_vec2()) / bounds.size();
            }
            p
        }
        AnnotationKind::Cross => [
            (0.33, 0.),
            (0.67, 0.),
            (0.67, 0.33),
            (1., 0.33),
            (1., 0.67),
            (0.67, 0.67),
            (0.67, 1.),
            (0.33, 1.),
            (0.33, 0.67),
            (0., 0.67),
            (0., 0.33),
            (0.33, 0.33),
        ]
        .into_iter()
        .map(|(x, y)| Vec2::new(x, y))
        .collect(),
        AnnotationKind::Parallelogram => [(0.25, 0.), (1., 0.), (0.75, 1.), (0., 1.)]
            .into_iter()
            .map(|(x, y)| Vec2::new(x, y))
            .collect(),
        _ => vec![
            Vec2::ZERO,
            Vec2::new(1., 0.),
            Vec2::splat(1.),
            Vec2::new(0., 1.),
        ],
    };
    normalized
        .into_iter()
        .map(|p| rect.min + p * rect.size())
        .collect()
}

pub fn contains(points: &[Pos2], at: Pos2) -> bool {
    let mut inside = false;
    for (a, b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        if (a.y > at.y) != (b.y > at.y) && at.x < (b.x - a.x) * (at.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

fn arc(center: [f32; 2], radius: [f32; 2], start: f32, end: f32, steps: usize) -> Vec<Vec2> {
    (0..=steps)
        .map(|i| {
            Vec2::new(center[0], center[1])
                + Vec2::angled(start + (end - start) * i as f32 / steps as f32)
                    * Vec2::new(radius[0], radius[1])
        })
        .collect()
}
/// Interior edges in the same projection as the silhouette.
pub fn details(kind: AnnotationKind, rect: Rect) -> Vec<Vec<Pos2>> {
    let line = |pts: &[(f32, f32)]| {
        pts.iter()
            .map(|&(x, y)| Vec2::new(x, y))
            .collect::<Vec<_>>()
    };
    let paths = match kind {
        AnnotationKind::Cylinder => vec![arc([0.5, 0.16], [0.5, 0.16], 0., PI, 24)],
        AnnotationKind::Cone => vec![arc([0.5, 0.84], [0.5, 0.16], PI, TAU, 24)],
        AnnotationKind::Sphere => vec![
            arc([0.5, 0.5], [0.5, 0.16], 0., TAU, 48),
            arc([0.5, 0.5], [0.18, 0.5], 0., TAU, 48),
        ],
        AnnotationKind::Prism => vec![
            line(&[(0., 0.22), (0.68, 0.22), (1., 0.)]),
            line(&[(0.68, 0.22), (0.68, 1.)]),
        ],
        AnnotationKind::HexagonalPrism => vec![
            line(&[(0., 0.16), (0.25, 0.32), (0.75, 0.32), (1., 0.16)]),
            line(&[(0.25, 0.32), (0.25, 1.)]),
            line(&[(0.75, 0.32), (0.75, 1.)]),
        ],
        AnnotationKind::Tetrahedron => vec![
            line(&[(0.45, 0.), (0.35, 1.)]),
            line(&[(0., 0.68), (1., 0.78)]),
        ],
        AnnotationKind::Torus => vec![torus_hole_normalized()],
        _ => vec![],
    };
    paths
        .into_iter()
        .map(|p| p.into_iter().map(|v| rect.min + v * rect.size()).collect())
        .collect()
}
fn torus_hole_normalized() -> Vec<Vec2> {
    arc([0.5, 0.43], [0.24, 0.19], 0., TAU, 48)
}
pub fn torus_hole(rect: Rect) -> Vec<Pos2> {
    torus_hole_normalized()
        .into_iter()
        .take(48)
        .map(|p| rect.min + p * rect.size())
        .collect()
}
pub fn detail_color(color: [u8; 4], filled: bool) -> [u8; 4] {
    if filled {
        [
            ((color[0] as f32) * 0.55) as u8,
            ((color[1] as f32) * 0.55) as u8,
            ((color[2] as f32) * 0.55) as u8,
            color[3],
        ]
    } else {
        color
    }
}
