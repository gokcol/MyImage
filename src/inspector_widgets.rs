//! Compact, accessible icon tiles shared by the editor side panels.
use super::*;
use crate::appearance::{self, ACCENT, BORDER, CREAM, INK};

#[derive(Clone, Copy)]
pub(super) enum Glyph {
    Rectangle,
    Lasso,
    All,
    Clear,
    Boundary,
    Pixels,
    Copy,
    Cut,
    Delete,
    Blur,
    Pixelate,
    Fill,
    Lock(bool),
    Remove,
    Help,
    App(appearance::Icon),
    Visible(bool),
    Stack,
    Group(bool),
    Order { front: bool, extreme: bool },
}

pub(super) fn tile(
    ui: &mut egui::Ui,
    label: &str,
    glyph: Glyph,
    size: Vec2,
    selected: bool,
    caption: bool,
) -> egui::Response {
    let response = ui.add_sized(size, egui::Button::new("").frame(false));
    response.widget_info(|| {
        if matches!(
            glyph,
            Glyph::Rectangle
                | Glyph::Lasso
                | Glyph::Boundary
                | Glyph::Pixels
                | Glyph::Lock(_)
                | Glyph::Visible(_)
        ) {
            egui::WidgetInfo::selected(
                egui::WidgetType::Button,
                ui.is_enabled(),
                match glyph {
                    Glyph::Lock(value) | Glyph::Visible(value) => value,
                    _ => selected,
                },
                label,
            )
        } else {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
        }
    });
    let r = response.rect;
    let fill = if selected {
        INK
    } else if response.hovered() {
        CREAM
    } else {
        appearance::PAPER
    };
    let color = if selected { appearance::GINGER } else { INK };
    let p = ui.painter();
    p.rect(
        r,
        6.,
        fill,
        Stroke::new(
            if response.has_focus() { 2. } else { 1. },
            if selected || response.has_focus() {
                ACCENT
            } else {
                BORDER
            },
        ),
        StrokeKind::Inside,
    );
    let center = if caption {
        egui::pos2(r.center().x, r.top() + (r.height() - 16.) * 0.5)
    } else {
        r.center()
    };
    paint_glyph(
        p,
        center,
        glyph,
        color,
        if size.y > 50. { 28. } else { 19. },
    );
    if caption {
        p.text(
            r.center_bottom() - Vec2::new(0., 7.),
            egui::Align2::CENTER_BOTTOM,
            label,
            egui::FontId::proportional(11.),
            if selected { Color32::WHITE } else { INK },
        );
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

pub(super) fn paint_glyph(p: &egui::Painter, c: Pos2, glyph: Glyph, color: Color32, size: f32) {
    let r = Rect::from_center_size(c, Vec2::splat(size));
    let at = |x, y| r.min + Vec2::new(x * size, y * size);
    let s = Stroke::new(1.6, color);
    let marquee = |rect: Rect| {
        let points = [
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
            rect.left_top(),
        ];
        p.extend(egui::Shape::dashed_line(&points, s, 3., 2.5));
    };
    match glyph {
        Glyph::Rectangle | Glyph::All | Glyph::Clear => {
            let frame = r.shrink2(Vec2::new(1., 3.));
            marquee(frame);
            if matches!(glyph, Glyph::All) {
                p.rect_filled(frame.shrink(4.), 1., color.gamma_multiply(0.28));
            } else if matches!(glyph, Glyph::Clear) {
                p.line_segment([at(0.25, 0.25), at(0.75, 0.75)], s);
                p.line_segment([at(0.75, 0.25), at(0.25, 0.75)], s);
            }
        }
        Glyph::Lasso => {
            let points: Vec<_> = (0..=40)
                .map(|i| {
                    let a = i as f32 / 40. * std::f32::consts::TAU;
                    c + Vec2::new(
                        a.cos() * size * 0.48,
                        a.sin() * size * (0.3 + 0.07 * (a * 3.).sin()),
                    )
                })
                .collect();
            p.extend(egui::Shape::dashed_line(&points, s, 3., 2.));
            p.line(
                vec![at(0.25, 0.72), at(0.12, 0.94), at(0.3, 1.), at(0.38, 0.84)],
                s,
            );
        }
        Glyph::Boundary | Glyph::Pixels => {
            let base = Rect::from_min_max(at(0., 0.12), at(0.68, 0.8));
            p.rect_filled(base, 2., color.gamma_multiply(0.16));
            let moved = base.translate(Vec2::new(size * 0.24, -size * 0.1));
            if matches!(glyph, Glyph::Pixels) {
                p.rect_filled(moved, 2., color.gamma_multiply(0.48));
            }
            marquee(moved);
            p.arrow(at(0.2, 0.95), Vec2::new(size * 0.72, 0.), s);
        }
        Glyph::Copy => appearance::icon(p, c, appearance::Icon::Copy, color, size * 0.8),
        Glyph::Delete => {
            paint_checkerboard(p, r.shrink(1.));
            appearance::icon(p, c, appearance::Icon::Trash, color, size * 0.8);
        }
        Glyph::Cut => {
            p.circle_stroke(at(0.22, 0.76), size * 0.17, s);
            p.circle_stroke(at(0.78, 0.76), size * 0.17, s);
            p.line_segment([at(0.31, 0.63), at(0.87, 0.04)], s);
            p.line_segment([at(0.69, 0.63), at(0.13, 0.04)], s);
        }
        Glyph::Blur => {
            for ring in (1..=4).rev() {
                p.circle_filled(
                    c,
                    size * ring as f32 / 8.,
                    color.gamma_multiply(0.08 + 0.025 * (5 - ring) as f32),
                );
            }
            p.circle_filled(c, size * 0.1, color);
        }
        Glyph::Pixelate => {
            for y in 0..3 {
                for x in 0..3 {
                    let cell = Rect::from_min_size(
                        at(x as f32 / 3., y as f32 / 3.),
                        Vec2::splat(size / 3. - 1.5),
                    );
                    p.rect_filled(
                        cell,
                        1.,
                        color.gamma_multiply(0.3 + ((x + y) % 3) as f32 * 0.3),
                    );
                }
            }
        }
        Glyph::Fill => paint_tool_icon(p, c, Tool::Fill, color),
        Glyph::Lock(locked) => {
            p.rect_stroke(
                Rect::from_min_max(at(0.15, 0.44), at(0.85, 0.96)),
                2.,
                s,
                StrokeKind::Inside,
            );
            let offset = if locked { 0. } else { 0.23 };
            p.line(
                vec![
                    at(0.3, 0.44),
                    at(0.3, 0.2),
                    at(0.4, 0.07),
                    at(0.6, 0.07),
                    at(0.7, 0.2),
                    at(0.7, 0.44 - offset),
                ],
                s,
            );
            p.circle_filled(at(0.5, 0.7), size * 0.06, color);
        }
        Glyph::Remove => appearance::icon(p, c, appearance::Icon::Remove, color, size),
        Glyph::Help => appearance::icon(p, c, appearance::Icon::Info, color, size),
        Glyph::App(kind) => appearance::icon(p, c, kind, color, size),
        Glyph::Visible(visible) => {
            appearance::icon(p, c, appearance::Icon::Eye, color, size);
            if !visible {
                p.line_segment([r.left_bottom(), r.right_top()], s);
            }
        }
        Glyph::Stack => {
            for dy in [0.2, 0., -0.2] {
                p.line(
                    vec![at(0.05, 0.48 + dy), at(0.5, 0.7 + dy), at(0.95, 0.48 + dy)],
                    s,
                );
            }
            p.line(vec![at(0.05, 0.28), at(0.5, 0.06), at(0.95, 0.28)], s);
        }
        Glyph::Group(ungroup) => {
            for (x, y) in [(0.05, 0.05), (0.42, 0.42)] {
                p.rect_stroke(
                    Rect::from_min_size(at(x, y), Vec2::splat(size * 0.5)),
                    1.,
                    s,
                    StrokeKind::Inside,
                );
            }
            if ungroup {
                p.line_segment([at(0., 1.), at(1., 0.)], s);
            }
        }
        Glyph::Order { front, extreme } => {
            let sign = if front { -1. } else { 1. };
            p.arrow(
                c - Vec2::new(0., sign * size * 0.32),
                Vec2::new(0., sign * size * 0.64),
                s,
            );
            if extreme {
                let y = c.y + sign * size * 0.48;
                p.line_segment([egui::pos2(r.left(), y), egui::pos2(r.right(), y)], s);
            }
        }
    }
}
