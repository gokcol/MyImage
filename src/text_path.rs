//! Local, editable text paths. Geometry is relative to the text anchor, so moving
//! and duplicating an annotation never leaves a separate path behind.
use eframe::egui::{self, Pos2, Vec2};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shape {
    #[default]
    Horizontal,
    Vertical,
    Diagonal,
    Circle,
    Square,
    Custom,
}
impl Shape {
    pub const ALL: [Self; 6] = [
        Self::Horizontal,
        Self::Vertical,
        Self::Diagonal,
        Self::Circle,
        Self::Square,
        Self::Custom,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Horizontal => "Horizontal",
            Self::Vertical => "Vertical",
            Self::Diagonal => "Diagonal",
            Self::Circle => "Circle",
            Self::Square => "Square",
            Self::Custom => "Custom",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Placement {
    #[default]
    Over,
    On,
    Below,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TextPath {
    pub shape: Shape,
    pub width: u16,
    pub height: u16,
    pub rotation: i16,
    pub offset: i16,
    pub spacing: u16,
    pub placement: Placement,
    pub visible: bool,
    pub color: [u8; 4],
    pub stroke: u16,
    pub fit: bool,
    pub closed: bool,
    /// Normalized thousandths of the path's width and height.
    pub nodes: Vec<[u16; 2]>,
}
impl Default for TextPath {
    fn default() -> Self {
        Self {
            shape: Shape::Horizontal,
            width: 480,
            height: 240,
            rotation: 0,
            offset: 6,
            spacing: 0,
            placement: Placement::Over,
            visible: false,
            color: [90, 110, 140, 180],
            stroke: 2,
            fit: true,
            closed: false,
            nodes: vec![[0, 750], [250, 250], [500, 500], [750, 100], [1000, 750]],
        }
    }
}
impl TextPath {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=8192).contains(&self.width)
            || !(1..=8192).contains(&self.height)
            || self.rotation.unsigned_abs() > 360
            || self.offset.unsigned_abs() > 1024
            || self.spacing > 256
            || !(1..=128).contains(&self.stroke)
            || !(2..=256).contains(&self.nodes.len())
            || self.nodes.iter().flatten().any(|&v| v > 1000)
        {
            return Err("The text path has invalid dimensions or control points.".into());
        }
        if self.geometry().length < 1. {
            return Err("A text path must be at least one pixel long.".into());
        }
        Ok(())
    }
    pub fn scale(&mut self, factor: f32) {
        self.width = (f32::from(self.width) * factor).round().clamp(1., 8192.) as u16;
        self.height = (f32::from(self.height) * factor).round().clamp(1., 8192.) as u16;
        self.offset = (f32::from(self.offset) * factor)
            .round()
            .clamp(-1024., 1024.) as i16;
        self.spacing = (f32::from(self.spacing) * factor).round().clamp(0., 256.) as u16;
        self.stroke = (f32::from(self.stroke) * factor).round().clamp(1., 128.) as u16;
    }
    pub fn geometry(&self) -> Geometry {
        let w = f32::from(self.width);
        let h = f32::from(self.height);
        let mut points = match self.shape {
            Shape::Horizontal => vec![Pos2::ZERO, egui::pos2(w, 0.)],
            Shape::Vertical => vec![Pos2::ZERO, egui::pos2(0., h)],
            Shape::Diagonal => vec![Pos2::ZERO, egui::pos2(w, h)],
            Shape::Circle => (0..=256)
                .map(|i| {
                    let a = i as f32 / 256. * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                    egui::pos2(w * 0.5 + a.cos() * w * 0.5, w * 0.5 + a.sin() * w * 0.5)
                })
                .collect(),
            Shape::Square => vec![
                Pos2::ZERO,
                egui::pos2(w, 0.),
                egui::pos2(w, w),
                egui::pos2(0., w),
                Pos2::ZERO,
            ],
            Shape::Custom => self
                .nodes
                .iter()
                .map(|p| egui::pos2(f32::from(p[0]) * w / 1000., f32::from(p[1]) * h / 1000.))
                .collect(),
        };
        if self.shape == Shape::Custom
            && self.closed
            && let Some(&first) = points.first()
        {
            points.push(first);
        }
        let (s, c) = f32::from(self.rotation).to_radians().sin_cos();
        for p in &mut points {
            *p = egui::pos2(p.x * c - p.y * s, p.x * s + p.y * c);
        }
        Geometry::new(points)
    }
}
pub struct Geometry {
    pub points: Vec<Pos2>,
    distances: Vec<f32>,
    pub length: f32,
}
impl Geometry {
    fn new(mut points: Vec<Pos2>) -> Self {
        points.dedup_by(|a, b| a.distance(*b) < 0.0001);
        if points.len() < 2 {
            points = vec![Pos2::ZERO, Pos2::ZERO];
        }
        let mut distances = vec![0.];
        for pair in points.windows(2) {
            distances.push(distances.last().unwrap() + pair[0].distance(pair[1]));
        }
        let length = *distances.last().unwrap();
        Self {
            points,
            distances,
            length,
        }
    }
    /// Tangent and point, with endpoint extension for intentionally overflowing text.
    pub fn at(&self, distance: f32) -> (Pos2, Vec2) {
        let i = self
            .distances
            .partition_point(|&v| v < distance)
            .clamp(1, self.points.len() - 1);
        let direction = (self.points[i] - self.points[i - 1]).normalized();
        (
            self.points[i - 1] + direction * (distance - self.distances[i - 1]),
            direction,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closed_paths_have_consistent_arc_length_and_tangents() {
        let path = TextPath {
            shape: Shape::Square,
            width: 100,
            ..Default::default()
        };
        let g = path.geometry();
        assert!((g.length - 400.).abs() < 0.01);
        assert_eq!(g.at(150.), (egui::pos2(100., 50.), Vec2::Y));
        let circle = TextPath {
            shape: Shape::Circle,
            width: 100,
            ..Default::default()
        }
        .geometry();
        assert!((circle.length - std::f32::consts::PI * 100.).abs() < 0.02);
    }
    #[test]
    fn invalid_or_degenerate_custom_paths_are_rejected() {
        let mut path = TextPath {
            shape: Shape::Custom,
            nodes: vec![[500, 500]; 2],
            ..Default::default()
        };
        assert!(path.validate().is_err());
        path.nodes[1] = [900, 900];
        assert!(path.validate().is_ok());
        path.rotation = i16::MIN;
        assert!(path.validate().is_err());
    }
}
