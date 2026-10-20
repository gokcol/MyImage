//! Editable callout silhouettes shared by canvas rendering, export and style previews.
use crate::document::{Annotation, Point};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalloutStyle {
    #[default]
    Rounded,
    Rectangle,
    Oval,
    Cloud,
    ThoughtCloud,
    ThoughtOval,
    ArrowLeft,
    ArrowRight,
    DoubleArrow,
    Burst,
    Note,
    PlainRectangle,
}
impl CalloutStyle {
    pub const ALL: [Self; 12] = [
        Self::Rounded,
        Self::Rectangle,
        Self::Oval,
        Self::Cloud,
        Self::ThoughtCloud,
        Self::ThoughtOval,
        Self::ArrowLeft,
        Self::ArrowRight,
        Self::DoubleArrow,
        Self::Burst,
        Self::Note,
        Self::PlainRectangle,
    ];
    pub fn is_default(&self) -> bool {
        *self == Self::Rounded
    }
    pub fn has_tail(self) -> bool {
        matches!(
            self,
            Self::Rounded
                | Self::Rectangle
                | Self::Oval
                | Self::Cloud
                | Self::ThoughtCloud
                | Self::ThoughtOval
        )
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Rounded => "Rounded speech bubble",
            Self::Rectangle => "Rectangular speech bubble",
            Self::Oval => "Oval speech bubble",
            Self::Cloud => "Cloud speech bubble",
            Self::ThoughtCloud => "Cloud thought bubble",
            Self::ThoughtOval => "Oval thought bubble",
            Self::ArrowLeft => "Left arrow callout",
            Self::ArrowRight => "Right arrow callout",
            Self::DoubleArrow => "Double arrow callout",
            Self::Burst => "Burst callout",
            Self::Note => "Note callout",
            Self::PlainRectangle => "Rectangle without tail",
        }
    }
}
fn body(a: &Annotation) -> Rect {
    Rect::from_two_pos(
        egui::pos2(a.start.x, a.start.y),
        egui::pos2(a.end.x, a.end.y),
    )
}
fn ellipse(r: Rect, count: usize) -> Vec<Pos2> {
    (0..count)
        .map(|i| {
            let angle =
                -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / count as f32;
            r.center() + Vec2::new(angle.cos(), angle.sin()) * r.size() * 0.5
        })
        .collect()
}
fn silhouette(style: CalloutStyle, r: Rect) -> Vec<Pos2> {
    let polygon = |points: &[[f32; 2]]| {
        points
            .iter()
            .map(|p| r.min + r.size() * Vec2::new(p[0], p[1]))
            .collect()
    };
    match style {
        CalloutStyle::Oval | CalloutStyle::ThoughtOval => ellipse(r, 64),
        CalloutStyle::Cloud | CalloutStyle::ThoughtCloud | CalloutStyle::Burst => {
            let burst = style == CalloutStyle::Burst;
            let count = if burst { 24 } else { 96 };
            (0..count)
                .map(|i| {
                    let angle = -std::f32::consts::FRAC_PI_2
                        + i as f32 * std::f32::consts::TAU / count as f32;
                    let radius = if burst {
                        if i % 2 == 0 { 1. } else { 0.72 }
                    } else {
                        0.92 + 0.08 * (angle * 8.).cos()
                    };
                    r.center() + Vec2::new(angle.cos(), angle.sin()) * r.size() * (radius * 0.5)
                })
                .collect()
        }
        CalloutStyle::ArrowRight => polygon(&[
            [0., 0.18],
            [0.68, 0.18],
            [0.68, 0.],
            [1., 0.5],
            [0.68, 1.],
            [0.68, 0.82],
            [0., 0.82],
        ]),
        CalloutStyle::ArrowLeft => polygon(&[
            [0., 0.5],
            [0.32, 0.],
            [0.32, 0.18],
            [1., 0.18],
            [1., 0.82],
            [0.32, 0.82],
            [0.32, 1.],
        ]),
        CalloutStyle::DoubleArrow => polygon(&[
            [0., 0.5],
            [0.25, 0.],
            [0.25, 0.2],
            [0.75, 0.2],
            [0.75, 0.],
            [1., 0.5],
            [0.75, 1.],
            [0.75, 0.8],
            [0.25, 0.8],
            [0.25, 1.],
        ]),
        CalloutStyle::Note => polygon(&[[0., 0.], [0.8, 0.], [1., 0.2], [1., 1.], [0., 1.]]),
        _ => vec![
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
        ],
    }
}
/// A tail is inserted into a single boundary, preventing double alpha blending.
fn with_tail(points: Vec<Pos2>, center: Pos2, tip: Pos2) -> Vec<Pos2> {
    let ray = tip - center;
    let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
    let mut intersection = None;
    for i in 0..points.len() {
        let p = points[i];
        let edge = points[(i + 1) % points.len()] - p;
        let d = cross(ray, edge);
        if d.abs() < 0.0001 {
            continue;
        }
        let t = cross(p - center, edge) / d;
        let u = cross(p - center, ray) / d;
        if t > 0. && t < 1. && (0. ..=1.).contains(&u) {
            intersection = Some((i, u));
            break;
        }
    }
    let Some((i, u)) = intersection else {
        return points;
    };
    if points.len() == 4 {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let half = 0.12;
        let lo = (u - half).clamp(0.05, 0.71);
        let hi = (u + half).clamp(lo + 0.05, 0.95);
        let mut out = Vec::new();
        for (j, p) in points.iter().enumerate() {
            out.push(*p);
            if j == i {
                out.extend([a + (b - a) * lo, tip, a + (b - a) * hi]);
            }
        }
        return out;
    }
    let n = points.len();
    let start = (i + n - 2) % n;
    let end = (i + 3) % n;
    let mut out = vec![points[start], tip];
    let mut j = end;
    while j != start {
        out.push(points[j]);
        j = (j + 1) % n;
    }
    out
}
pub fn parts(a: &Annotation) -> Vec<Vec<Pos2>> {
    if a.callout_style == CalloutStyle::Rounded {
        return vec![crate::render::callout_outline(a)];
    }
    let rect = body(a);
    if !rect.is_positive() {
        return Vec::new();
    }
    let outline = silhouette(a.callout_style, rect);
    let tip = a
        .tail
        .map(|p| egui::pos2(p.x, p.y))
        .unwrap_or(rect.center());
    if matches!(
        a.callout_style,
        CalloutStyle::ThoughtCloud | CalloutStyle::ThoughtOval
    ) {
        let mut parts = vec![outline];
        if !rect.contains(tip) {
            let delta = tip - rect.center();
            let edge = rect.center()
                + delta / (2. * (delta.x.abs() / rect.width()).max(delta.y.abs() / rect.height()));
            let gap = tip.distance(edge);
            for (t, factor) in [(0.35, 0.12), (0.7, 0.08), (1., 0.04)] {
                let radius = (rect.width().min(rect.height()) * factor)
                    .min(gap * 0.12)
                    .max(0.5);
                parts.push(ellipse(
                    Rect::from_center_size(edge + (tip - edge) * t, Vec2::splat(radius * 2.)),
                    20,
                ));
            }
        }
        parts
    } else if a.callout_style.has_tail() && !rect.contains(tip) {
        vec![with_tail(outline, rect.center(), tip)]
    } else {
        vec![outline]
    }
}
/// Keep words clear of arrow heads, cloud scallops and the folded note corner.
pub fn text_body(a: &Annotation) -> Rect {
    let r = body(a);
    let (left, top, right, bottom) = match a.callout_style {
        CalloutStyle::ArrowRight => (0., 0.18, 0.28, 0.18),
        CalloutStyle::ArrowLeft => (0.28, 0.18, 0., 0.18),
        CalloutStyle::DoubleArrow => (0.22, 0.2, 0.22, 0.2),
        CalloutStyle::Cloud | CalloutStyle::ThoughtCloud | CalloutStyle::Burst => {
            (0.1, 0.1, 0.1, 0.1)
        }
        CalloutStyle::Oval | CalloutStyle::ThoughtOval => (0.12, 0.12, 0.12, 0.12),
        CalloutStyle::Note => (0., 0.1, 0.1, 0.),
        _ => (0., 0., 0., 0.),
    };
    Rect::from_min_max(
        r.min + r.size() * Vec2::new(left, top),
        r.max - r.size() * Vec2::new(right, bottom),
    )
}
pub fn paint_icon(p: &egui::Painter, rect: Rect, style: CalloutStyle, color: Color32) {
    let mut a = sample_annotation();
    a.start = Point {
        x: rect.left() + 3.,
        y: rect.top() + 2.,
    };
    a.end = Point {
        x: rect.right() - 3.,
        y: rect.bottom() - 8.,
    };
    a.tail = Some(Point {
        x: rect.left() + rect.width() * 0.3,
        y: rect.bottom() - 2.,
    });
    a.callout_style = style;
    for points in parts(&a) {
        p.add(Shape::closed_line(points, Stroke::new(1.5, color)));
    }
    if style == CalloutStyle::Note {
        let r = body(&a);
        let fold = r.min + r.size() * Vec2::new(0.8, 0.2);
        p.line_segment(
            [r.min + r.size() * Vec2::new(0.8, 0.), fold],
            Stroke::new(1., color),
        );
        p.line_segment(
            [fold, r.min + r.size() * Vec2::new(1., 0.2)],
            Stroke::new(1., color),
        );
    }
}
pub fn picker(ui: &mut egui::Ui, current: &mut CalloutStyle) -> bool {
    let mut changed = false;
    egui::Grid::new("callout-shape-grid")
        .spacing(Vec2::splat(4.))
        .show(ui, |ui| {
            for (i, style) in CalloutStyle::ALL.into_iter().enumerate() {
                let (rect, response) = ui.allocate_exact_size(Vec2::new(56., 44.), Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        ui.is_enabled(),
                        style.label(),
                    )
                });
                let selected = *current == style;
                if selected || response.hovered() {
                    ui.painter().rect_filled(
                        rect,
                        4,
                        if selected {
                            crate::appearance::palette(ui.ctx()).accent
                        } else {
                            Color32::from_gray(235)
                        },
                    );
                }
                paint_icon(
                    ui.painter(),
                    rect.shrink(5.),
                    style,
                    if selected {
                        Color32::WHITE
                    } else {
                        crate::appearance::palette(ui.ctx()).ink
                    },
                );
                if response.on_hover_text(style.label()).clicked() {
                    *current = style;
                    changed = true;
                    if egui::Popup::is_any_open(ui.ctx()) {
                        ui.close();
                    }
                }
                if i % 3 == 2 {
                    ui.end_row();
                }
            }
        });
    changed
}

pub(crate) fn sample_annotation() -> Annotation {
    Annotation {
        kind: crate::document::AnnotationKind::Callout,
        start: Point { x: 0., y: 0. },
        end: Point { x: 100., y: 60. },
        color: [230, 66, 55, 255],
        stroke_width: 2.,
        fill: true,
        text: String::new(),
        font_size: 28.,
        effect_size: 12,
        step_number: 1,
        tail: Some(Point { x: 30., y: 85. }),
        text_color: [255; 4],
        points: vec![],
        group_id: None,
        layer: Default::default(),
        group_layer: Default::default(),
        area_source: None,
        area_mask: vec![],
        stamp: None,
        fill_region: None,
        text_style: Default::default(),
        callout_style: Default::default(),
        pixel_mask: None,
        studio: Default::default(),
        arrow: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Document, EditHistory, load_project, save_project};
    #[test]
    fn callout_shapes_round_trip_with_editable_text_and_undo() {
        let dir = tempfile::tempdir().unwrap();
        let mut d = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(400, 300)),
            annotations: vec![sample_annotation()],
            geometry: Default::default(),
        };
        d.annotations[0].text = "Editable words".into();
        let mut history = EditHistory::new(d.edit_state());
        for style in CalloutStyle::ALL {
            let before = d.edit_state();
            d.annotations[0].callout_style = style;
            history.commit(before, &d);
            let path = dir.path().join(format!("{style:?}.myimage"));
            save_project(&d, &path).unwrap();
            let loaded = load_project(&path).unwrap();
            assert_eq!(loaded.annotations, d.annotations);
            let rendered = crate::render::flatten(&loaded).unwrap();
            assert!(rendered.pixels().any(|p| p[3] > 0));
        }
        assert!(history.undo(&mut d));
        assert_ne!(d.annotations[0].callout_style, CalloutStyle::PlainRectangle);
        assert_eq!(d.annotations[0].text, "Editable words");
        assert!(history.redo(&mut d));
        assert_eq!(d.annotations[0].callout_style, CalloutStyle::PlainRectangle);
    }
    #[test]
    fn legacy_callouts_default_and_wrong_object_styles_are_rejected() {
        let a = sample_annotation();
        let mut value = serde_json::to_value(&a).unwrap();
        value.as_object_mut().unwrap().remove("callout_style");
        assert_eq!(
            serde_json::from_value::<Annotation>(value)
                .unwrap()
                .callout_style,
            CalloutStyle::Rounded
        );
        let mut a = a;
        a.kind = crate::document::AnnotationKind::Text;
        a.callout_style = CalloutStyle::Cloud;
        assert!(crate::document::validate_annotation(&a).is_err());
    }
}
