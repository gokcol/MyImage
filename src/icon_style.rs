//! Native vector icons: consistent optical size, rounded strokes and restrained accents.
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IconStyle {
    #[default]
    StudioColor,
    CobaltDuotone,
    PrecisionOutline,
}
impl IconStyle {
    pub const ALL: [Self; 3] = [
        Self::StudioColor,
        Self::CobaltDuotone,
        Self::PrecisionOutline,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::StudioColor => "Studio Color",
            Self::CobaltDuotone => "Cobalt Duotone",
            Self::PrecisionOutline => "Precision Outline",
        }
    }
    pub fn width(self) -> f32 {
        match self {
            Self::StudioColor => 1.8,
            Self::CobaltDuotone => 1.7,
            Self::PrecisionOutline => 1.5,
        }
    }
}
pub fn current(ctx: &egui::Context) -> IconStyle {
    ctx.data(|d| d.get_temp(egui::Id::new("icon-style")))
        .unwrap_or_default()
}
pub fn set(ctx: &egui::Context, value: IconStyle) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("icon-style"), value));
    ctx.request_repaint();
}
#[derive(Clone, Copy)]
pub enum Family {
    Cobalt,
    Violet,
    Amber,
    Teal,
    Neutral,
}
pub fn color(ctx: &egui::Context, base: Color32, family: Family) -> Color32 {
    color_for(current(ctx), ctx, base, family)
}
fn color_for(style: IconStyle, ctx: &egui::Context, base: Color32, family: Family) -> Color32 {
    // Preserve selected, disabled, destructive and dark-surface foregrounds.
    if base != crate::appearance::palette(ctx).ink {
        return base;
    }
    match style {
        IconStyle::PrecisionOutline => base,
        IconStyle::CobaltDuotone => match family {
            Family::Neutral => base,
            _ => Color32::from_rgb(43, 101, 217),
        },
        IconStyle::StudioColor => match family {
            Family::Cobalt => Color32::from_rgb(43, 101, 217),
            Family::Violet => Color32::from_rgb(118, 81, 182),
            Family::Amber => Color32::from_rgb(162, 101, 20),
            Family::Teal => Color32::from_rgb(24, 124, 121),
            Family::Neutral => base,
        },
    }
}
#[derive(Clone, Copy)]
pub enum Glyph {
    Select,
    Arrow,
    Text,
    Callout,
    Shapes,
    Stamp,
    Fill,
    Magnify,
    Copy,
    Capture,
    Effects,
    Settings,
    Library,
}
impl Glyph {
    fn family(self) -> Family {
        match self {
            Self::Text | Self::Callout | Self::Shapes | Self::Effects => Family::Violet,
            Self::Stamp => Family::Amber,
            Self::Fill | Self::Magnify | Self::Library => Family::Teal,
            Self::Settings => Family::Neutral,
            _ => Family::Cobalt,
        }
    }
}
pub fn paint(p: &egui::Painter, center: Pos2, glyph: Glyph, base: Color32, size: f32) {
    preview(p, center, glyph, base, size, current(p.ctx()));
}
pub fn preview(
    p: &egui::Painter,
    center: Pos2,
    glyph: Glyph,
    base: Color32,
    size: f32,
    style: IconStyle,
) {
    let color = color_for(style, p.ctx(), base, glyph.family());
    let stroke = Stroke::new(style.width() * (size / 20.).clamp(0.75, 1.5), color);
    let tint = if style == IconStyle::PrecisionOutline {
        Color32::TRANSPARENT
    } else {
        color.gamma_multiply(0.14)
    };
    let at = |x: f32, y: f32| center + Vec2::new(x - 12., y - 12.) * (size / 24.);
    let path = |points: &[(f32, f32)], closed: bool| {
        let points: Vec<_> = points.iter().map(|&(x, y)| at(x, y)).collect();
        if closed {
            p.add(egui::Shape::closed_line(points.clone(), stroke));
        } else {
            p.line(points.clone(), stroke);
        }
        // Round caps and joins remain crisp at toolbar sizes.
        for point in points {
            p.circle_filled(point, stroke.width / 2., color);
        }
    };
    let rect = |a: (f32, f32), b: (f32, f32), radius| {
        let r = Rect::from_min_max(at(a.0, a.1), at(b.0, b.1));
        p.rect_filled(r, radius, tint);
        p.rect_stroke(r, radius, stroke, egui::StrokeKind::Middle);
    };
    let circle = |x, y, r| {
        p.circle_filled(at(x, y), r * size / 24., tint);
        p.circle_stroke(at(x, y), r * size / 24., stroke);
    };
    match glyph {
        Glyph::Select => path(&[(4., 3.), (10., 21.), (13., 13.), (21., 10.)], true),
        Glyph::Arrow => {
            path(&[(5., 19.), (19., 5.)], false);
            path(&[(7., 5.), (19., 5.), (19., 17.)], false);
        }
        Glyph::Text => {
            path(&[(4., 6.), (4., 4.), (20., 4.), (20., 6.)], false);
            path(&[(12., 4.), (12., 20.)], false);
            path(&[(8., 20.), (16., 20.)], false);
        }
        Glyph::Callout => {
            rect((3., 3.), (21., 17.), 3.);
            path(&[(6., 17.), (6., 22.), (11., 17.)], false);
        }
        Glyph::Shapes => {
            circle(16., 16., 6.);
            path(&[(3., 13.), (3., 3.), (13., 3.), (13., 7.)], false);
        }
        Glyph::Stamp => {
            circle(12., 6., 3.5);
            path(
                &[
                    (9., 10.),
                    (8., 14.),
                    (5., 15.),
                    (4., 19.),
                    (20., 19.),
                    (19., 15.),
                    (16., 14.),
                    (15., 10.),
                ],
                false,
            );
            path(&[(5., 22.), (19., 22.)], false);
        }
        Glyph::Fill => {
            path(&[(3., 11.), (11., 3.), (20., 12.), (12., 20.)], true);
            path(&[(7., 7.), (7., 2.)], false);
            path(&[(5., 13.), (17., 13.)], false);
            circle(21., 20., 1.5);
        }
        Glyph::Magnify => {
            circle(10., 10., 7.);
            path(&[(15., 15.), (22., 22.)], false);
            path(&[(7., 10.), (13., 10.)], false);
            path(&[(10., 7.), (10., 13.)], false);
        }
        Glyph::Copy => {
            path(
                &[(6., 16.), (3., 16.), (3., 3.), (16., 3.), (16., 6.)],
                false,
            );
            rect((8., 8.), (21., 21.), 2.);
        }
        Glyph::Capture => {
            path(
                &[
                    (3., 8.),
                    (3., 5.),
                    (8., 5.),
                    (10., 2.),
                    (15., 2.),
                    (17., 5.),
                    (21., 5.),
                    (21., 21.),
                    (3., 21.),
                    (3., 8.),
                ],
                false,
            );
            circle(12., 13., 4.5);
        }
        Glyph::Library => {
            for x in [3., 14.] {
                for y in [3., 14.] {
                    rect((x, y), (x + 7., y + 7.), 1.5);
                }
            }
        }
        Glyph::Effects => {
            path(
                &[
                    (13., 3.),
                    (15.5, 9.),
                    (22., 12.),
                    (15.5, 15.),
                    (13., 21.),
                    (10., 15.),
                    (4., 12.),
                    (10., 9.),
                ],
                true,
            );
            path(&[(3., 2.), (3., 6.)], false);
            path(&[(1., 4.), (5., 4.)], false);
        }
        Glyph::Settings => {
            for (y, x) in [(5., 8.), (12., 16.), (19., 8.)] {
                path(&[(3., y), (21., y)], false);
                p.circle_filled(
                    at(x, y),
                    2.2 * size / 24.,
                    crate::appearance::palette(p.ctx()).paper,
                );
                circle(x, y, 2.2);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn style_switching_preserves_semantic_foregrounds_and_other_windows() {
        let ctx = egui::Context::default();
        let other = egui::Context::default();
        crate::appearance::apply(&ctx);
        for finish in crate::appearance::GuiFinish::ALL {
            crate::appearance::set_finish(&ctx, finish);
            let p = crate::appearance::palette(&ctx);
            for style in IconStyle::ALL {
                set(&ctx, style);
                assert_eq!(current(&other), IconStyle::StudioColor);
                for base in [Color32::WHITE, p.cream, p.muted, Color32::RED] {
                    assert_eq!(color(&ctx, base, Family::Violet), base);
                }
                let a = color(&ctx, p.ink, Family::Violet);
                let b = color(&ctx, p.ink, Family::Teal);
                assert_eq!(a != b, style == IconStyle::StudioColor);
            }
        }
    }

    #[test]
    fn vector_icons_fit_existing_hit_targets_at_all_ui_scales() {
        let ctx = egui::Context::default();
        crate::appearance::apply(&ctx);
        for style in IconStyle::ALL {
            set(&ctx, style);
            for size in [14., 18., 21., 26.] {
                let output = ctx.run(Default::default(), |ctx| {
                    let p = ctx.layer_painter(egui::LayerId::background());
                    for glyph in [
                        Glyph::Select,
                        Glyph::Arrow,
                        Glyph::Text,
                        Glyph::Callout,
                        Glyph::Shapes,
                        Glyph::Stamp,
                        Glyph::Fill,
                        Glyph::Magnify,
                        Glyph::Copy,
                        Glyph::Capture,
                        Glyph::Effects,
                        Glyph::Settings,
                        Glyph::Library,
                    ] {
                        paint(
                            &p,
                            Pos2::new(100., 100.),
                            glyph,
                            crate::appearance::palette(ctx).ink,
                            size,
                        );
                    }
                });
                let bounds = Rect::from_center_size(Pos2::new(100., 100.), Vec2::splat(size + 2.));
                for shape in output.shapes {
                    let rect = shape.shape.visual_bounding_rect();
                    assert!(
                        rect == Rect::NOTHING || bounds.contains_rect(rect),
                        "{style:?} {size}: {rect:?}"
                    );
                }
            }
        }
    }
}
