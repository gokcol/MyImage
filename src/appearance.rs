use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

pub const INK: Color32 = Color32::from_rgb(36, 34, 33);
pub const MUTED: Color32 = Color32::from_rgb(112, 102, 91);
pub const ACCENT: Color32 = Color32::from_rgb(159, 77, 25);
pub const NAVY: Color32 = Color32::from_rgb(36, 34, 33);
pub const PAPER: Color32 = Color32::from_rgb(250, 248, 244);

pub const GINGER: Color32 = Color32::from_rgb(249, 177, 93);
pub const CREAM: Color32 = Color32::from_rgb(255, 240, 215);
pub const BORDER: Color32 = Color32::from_rgb(214, 198, 178);

pub fn calico_tabs(p: &egui::Painter, at: Pos2) {
    for (i, color) in [GINGER, CREAM, INK].into_iter().enumerate() {
        p.rect_filled(
            Rect::from_min_size(at + Vec2::new(i as f32 * 11., 0.), Vec2::new(8., 3.)),
            0,
            color,
        );
    }
}

pub fn apply(ctx: &egui::Context) {
    // The custom palette is light. Keep a late OS theme event from selecting
    // egui's unstyled dark/light defaults after startup.
    ctx.set_theme(egui::Theme::Light);
    // Use the platform UI face when present without redistributing OS fonts.
    // Annotation typography is rendered independently for portable projects.
    let font_path = if cfg!(target_os = "macos") {
        std::path::PathBuf::from("/System/Library/Fonts/SFNS.ttf")
    } else {
        std::env::var_os("WINDIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"))
            .join("Fonts")
            .join("segoeui.ttf")
    };
    if let Ok(bytes) = std::fs::read(font_path) {
        let mut fonts = egui::FontDefinitions::default();
        fonts
            .font_data
            .insert("system-ui".into(), egui::FontData::from_owned(bytes).into());
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "system-ui".into());
        ctx.set_fonts(fonts);
    }
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.override_text_color = Some(INK);
    style.visuals.panel_fill = PAPER;
    style.visuals.window_fill = Color32::WHITE;
    style.visuals.window_corner_radius = egui::CornerRadius::same(8);
    style.visuals.window_shadow = egui::Shadow {
        offset: [0, 12],
        blur: 30,
        spread: 1,
        color: Color32::from_black_alpha(65),
    };
    style.visuals.popup_shadow = egui::Shadow {
        offset: [0, 6],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha(45),
    };
    style.visuals.extreme_bg_color = Color32::from_rgb(242, 237, 229);
    style.visuals.faint_bg_color = Color32::from_rgb(246, 240, 230);
    style.visuals.selection.bg_fill = CREAM;
    style.visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.inactive.bg_fill = Color32::WHITE;
    style.visuals.widgets.inactive.weak_bg_fill = Color32::WHITE;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    style.visuals.widgets.hovered.bg_fill = CREAM;
    style.visuals.widgets.hovered.weak_bg_fill = CREAM;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
    ] {
        widget.corner_radius = egui::CornerRadius::same(6);
    }
    style.spacing.item_spacing = Vec2::new(10.0, 9.0);
    style.spacing.button_padding = Vec2::new(13.0, 9.0);
    style.spacing.interact_size.y = 32.0;
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(25.0));
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
    ctx.set_style(style);
}

/// Keep egui's native keyboard, pointer and accessibility behavior, but paint a
/// small white thumb independently of the larger interaction target.
pub fn value_slider<N: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    label: &str,
    suffix: &str,
) -> egui::Response {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.;
        let mut response = slider_track(ui, value, range.clone(), label, false);
        let field = ui
            .scope(|ui| {
                ui.spacing_mut().button_padding = Vec2::new(8., 4.);
                ui.spacing_mut().interact_size = Vec2::new(48., 24.);
                ui.add(
                    egui::DragValue::new(value)
                        .range(range)
                        .speed(1.)
                        .max_decimals(1)
                        .suffix(suffix),
                )
                .on_hover_text(format!("{label}: drag to adjust; click for an exact value"))
            })
            .inner;
        if field.changed() {
            response.mark_changed();
        }
        response.union(field)
    })
    .inner
}

pub fn zoom_slider(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
) -> egui::Response {
    slider_track(ui, value, range, "Zoom", true)
}

fn slider_track<N: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    label: &str,
    logarithmic: bool,
) -> egui::Response {
    let start = range.start().to_f64();
    let end = range.end().to_f64();
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().interact_size.y = 24.;
            // Opacity affects paint only; the native slider remains interactive and
            // accessible. Our paint below deliberately does not change its hit area.
            ui.set_opacity(0.);
            ui.add(
                egui::Slider::new(value, range)
                    .show_value(false)
                    .logarithmic(logarithmic)
                    .handle_shape(egui::style::HandleShape::Circle),
            )
        })
        .inner;
    response.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), value.to_f64(), label));
    if ui.is_rect_visible(response.rect) {
        let rect = response.rect;
        let inset = rect.height() / 2.5; // egui's native handle position range
        let left = rect.left() + inset;
        let right = rect.right() - inset;
        let fraction = if logarithmic {
            (value.to_f64() / start).ln() / (end / start).ln()
        } else {
            (value.to_f64() - start) / (end - start)
        }
        .clamp(0., 1.) as f32;
        let center = Pos2::new(egui::lerp(left..=right, fraction), rect.center().y);
        let painter = ui.painter();
        let rail = Rect::from_center_size(
            Pos2::new((left + right) * 0.5, center.y),
            Vec2::new(right - left, 3.),
        );
        painter.rect_filled(rail, 2, Color32::from_rgb(216, 223, 230));
        painter.rect_filled(
            Rect::from_min_max(rail.min, Pos2::new(center.x, rail.max.y)),
            2,
            ACCENT,
        );
        if response.has_focus() {
            painter.circle_stroke(center, 10., Stroke::new(1.5, ACCENT));
        }
        painter.circle_filled(
            center + Vec2::new(0., 1.),
            6.5,
            Color32::from_black_alpha(22),
        );
        painter.circle_filled(center, 6., Color32::WHITE);
        painter.circle_stroke(
            center,
            6.,
            Stroke::new(
                1.25,
                if response.hovered() || response.dragged() {
                    ACCENT
                } else {
                    Color32::from_rgb(154, 169, 183)
                },
            ),
        );
    }
    response
}

#[derive(Clone, Copy)]
pub enum Icon {
    Close,
    Check,
    Resize,
    Settings,
    Info,
    Trash,
    Remove,
    Home,
    TextSize,
    Eye,
    Tray,
    Power,
    Favorite,
    Back,
    Forward,
    Refresh,
    List,
    Up,
    Down,
    Capture,
    Library,
    View,
    Edit,
    Create,
    Open,
    OpenProject,
    Copy,
    Save,
    SaveAs,
    Export,
}

pub fn icon(p: &egui::Painter, center: Pos2, kind: Icon, color: Color32, size: f32) {
    let s = Stroke::new(1.8, color);
    let r = Rect::from_center_size(center, Vec2::splat(size));
    match kind {
        Icon::Close => {
            p.line_segment(
                [
                    r.left_top() + Vec2::splat(size * 0.18),
                    r.right_bottom() - Vec2::splat(size * 0.18),
                ],
                s,
            );
            p.line_segment(
                [
                    r.right_top() + Vec2::new(-size * 0.18, size * 0.18),
                    r.left_bottom() + Vec2::new(size * 0.18, -size * 0.18),
                ],
                s,
            );
        }
        Icon::Check => {
            p.line_segment(
                [r.left_center(), center + Vec2::new(-size * 0.1, size * 0.3)],
                s,
            );
            p.line_segment(
                [
                    center + Vec2::new(-size * 0.1, size * 0.3),
                    r.right_top() + Vec2::new(0., size * 0.15),
                ],
                s,
            );
        }
        Icon::Resize => {
            p.rect_stroke(r.shrink(size * 0.12), 1, s, egui::StrokeKind::Inside);
            p.line_segment([r.left_bottom(), r.right_top()], s);
        }
        Icon::Settings => {
            for (i, offset) in [-0.3, 0., 0.3].iter().enumerate() {
                let y = center.y + size * offset;
                p.line_segment([Pos2::new(r.left(), y), Pos2::new(r.right(), y)], s);
                let at = Pos2::new(
                    center.x + if i == 1 { size * 0.22 } else { -size * 0.22 },
                    y,
                );
                p.circle_filled(at, size * 0.12, color);
            }
        }
        Icon::Info => {
            p.circle_stroke(center, size * 0.48, s);
            p.circle_filled(center - Vec2::new(0., size * 0.23), size * 0.06, color);
            p.line_segment(
                [
                    center - Vec2::new(0., size * 0.04),
                    center + Vec2::new(0., size * 0.28),
                ],
                s,
            );
        }
        Icon::Remove => {
            p.circle_stroke(center, size * 0.48, s);
            p.line_segment(
                [
                    center - Vec2::new(size * 0.25, 0.),
                    center + Vec2::new(size * 0.25, 0.),
                ],
                s,
            );
        }
        Icon::Trash => {
            let at = |x: f32, y: f32| r.min + Vec2::new(x * size, y * size);
            p.line_segment([at(0.05, 0.22), at(0.95, 0.22)], s);
            p.line(
                vec![at(0.35, 0.2), at(0.35, 0.02), at(0.65, 0.02), at(0.65, 0.2)],
                s,
            );
            p.line(
                vec![at(0.18, 0.3), at(0.23, 0.95), at(0.77, 0.95), at(0.82, 0.3)],
                s,
            );
            for x in [0.4, 0.6] {
                p.line_segment([at(x, 0.4), at(x, 0.8)], s);
            }
        }

        Icon::Home => {
            let at = |x: f32, y: f32| r.min + Vec2::new(x * size, y * size);
            p.line(vec![at(0., 0.43), at(0.5, 0.), at(1., 0.43)], s);
            p.line(
                vec![
                    at(0.14, 0.38),
                    at(0.14, 0.96),
                    at(0.39, 0.96),
                    at(0.39, 0.64),
                    at(0.61, 0.64),
                    at(0.61, 0.96),
                    at(0.86, 0.96),
                    at(0.86, 0.38),
                ],
                s,
            );
        }
        Icon::TextSize => {
            let at = |x: f32, y: f32| r.min + Vec2::new(x * size, y * size);
            for (left, top, width) in [(0., 0.4, 0.4), (0.5, 0.05, 0.5)] {
                p.line_segment([at(left, top), at(left + width, top)], s);
                p.line_segment([at(left + width / 2., top), at(left + width / 2., 0.95)], s);
            }
        }

        Icon::Eye => {
            p.add(egui::Shape::ellipse_stroke(
                center,
                Vec2::new(size * 0.5, size * 0.3),
                s,
            ));
            p.circle_filled(center, size * 0.12, color);
        }
        Icon::Tray => {
            let at = |x: f32, y: f32| r.min + Vec2::new(x * size, y * size);
            p.line(
                vec![
                    at(0., 0.45),
                    at(0.2, 0.15),
                    at(0.8, 0.15),
                    at(1., 0.45),
                    at(1., 0.9),
                    at(0., 0.9),
                    at(0., 0.45),
                    at(0.3, 0.45),
                    at(0.4, 0.65),
                    at(0.6, 0.65),
                    at(0.7, 0.45),
                    at(1., 0.45),
                ],
                s,
            );
        }
        Icon::Power => {
            let points = (0..28)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2
                        + 0.65
                        + i as f32 / 27. * (std::f32::consts::TAU - 1.3);
                    center + Vec2::angled(a) * (size * 0.42)
                })
                .collect();
            p.line(points, s);
            p.line_segment([r.center_top(), center], s);
        }
        Icon::Create => {
            p.circle_stroke(center, size * 0.48, s);
            p.line_segment(
                [
                    center - Vec2::new(size * 0.25, 0.),
                    center + Vec2::new(size * 0.25, 0.),
                ],
                s,
            );
            p.line_segment(
                [
                    center - Vec2::new(0., size * 0.25),
                    center + Vec2::new(0., size * 0.25),
                ],
                s,
            );
        }
        Icon::Copy => {
            p.rect_stroke(
                r.translate(Vec2::new(-2., -2.)),
                2,
                s,
                egui::StrokeKind::Inside,
            );
            p.rect_filled(r.translate(Vec2::new(2., 2.)), 2, Color32::WHITE);
            p.rect_stroke(
                r.translate(Vec2::new(2., 2.)),
                2,
                s,
                egui::StrokeKind::Inside,
            );
        }
        Icon::Open | Icon::OpenProject => {
            p.line(
                vec![
                    r.left_bottom(),
                    r.left_top() + Vec2::new(0., size * 0.15),
                    r.left_top() + Vec2::new(size * 0.38, size * 0.15),
                    r.left_top() + Vec2::new(size * 0.5, size * 0.32),
                    r.right_top() + Vec2::new(0., size * 0.32),
                ],
                s,
            );
            p.add(egui::Shape::closed_line(
                vec![
                    r.left_bottom(),
                    r.left_top() + Vec2::new(size * 0.16, size * 0.42),
                    r.right_top() + Vec2::new(size * 0.12, size * 0.42),
                    r.right_bottom(),
                ],
                s,
            ));
            if matches!(kind, Icon::OpenProject) {
                p.line_segment([center - Vec2::new(2., 0.), center + Vec2::new(3., 4.)], s);
            }
        }
        Icon::Save | Icon::SaveAs => {
            p.rect_stroke(r, 2, s, egui::StrokeKind::Inside);
            p.rect_stroke(
                Rect::from_min_max(
                    r.min + Vec2::new(size * 0.25, 0.),
                    r.min + Vec2::new(size * 0.75, size * 0.35),
                ),
                0,
                s,
                egui::StrokeKind::Inside,
            );
            p.rect_stroke(
                Rect::from_min_max(
                    r.min + Vec2::new(size * 0.2, size * 0.55),
                    r.max - Vec2::new(size * 0.2, 1.),
                ),
                0,
                s,
                egui::StrokeKind::Inside,
            );
            if matches!(kind, Icon::SaveAs) {
                p.line_segment(
                    [
                        r.right_bottom() - Vec2::new(3., 0.),
                        r.right_bottom() + Vec2::new(3., -6.),
                    ],
                    Stroke::new(2.5, color),
                );
            }
        }
        Icon::Export => {
            p.line(
                vec![
                    r.left_top() + Vec2::new(0., size * 0.4),
                    r.left_bottom(),
                    r.right_bottom(),
                    r.right_top() + Vec2::new(0., size * 0.4),
                ],
                s,
            );
            p.line_segment([center + Vec2::new(0., size * 0.2), r.center_top()], s);
            p.line(
                vec![
                    r.center_top() + Vec2::new(-size * 0.25, size * 0.25),
                    r.center_top(),
                    r.center_top() + Vec2::new(size * 0.25, size * 0.25),
                ],
                s,
            );
        }
        Icon::Library => {
            let cell = (size - 5.0) / 2.0;
            for row in 0..2 {
                for column in 0..2 {
                    let tile = Rect::from_min_size(
                        r.min + Vec2::new(column as f32 * (cell + 5.0), row as f32 * (cell + 5.0)),
                        Vec2::splat(cell),
                    );
                    p.rect_stroke(tile, 2, s, egui::StrokeKind::Inside);
                }
            }
        }
        Icon::Capture => {
            let arm = size * 0.3;
            for (corner, dx, dy) in [
                (r.left_top(), 1.0, 1.0),
                (r.right_top(), -1.0, 1.0),
                (r.left_bottom(), 1.0, -1.0),
                (r.right_bottom(), -1.0, -1.0),
            ] {
                p.line_segment([corner + Vec2::new(dx * arm, 0.0), corner], s);
                p.line_segment([corner, corner + Vec2::new(0.0, dy * arm)], s);
            }
            p.circle_stroke(center, size * 0.18, s);
        }
        Icon::View => {
            p.rect_stroke(r, 3, s, egui::StrokeKind::Inside);
            p.circle_filled(r.min + Vec2::new(size * 0.73, size * 0.28), 2.0, color);
            p.line(
                vec![
                    r.min + Vec2::new(3.0, size - 4.0),
                    r.min + Vec2::new(size * 0.38, size * 0.44),
                    r.min + Vec2::new(size * 0.6, size * 0.7),
                    r.min + Vec2::new(size * 0.78, size * 0.55),
                    r.max - Vec2::splat(3.0),
                ],
                s,
            );
        }
        Icon::Edit => {
            let pt = |x: f32, y: f32| r.min + Vec2::new(x * size, y * size);
            // Image frame with an unmistakable pencil nib at its lower corner.
            p.line(
                vec![
                    pt(0.63, 0.04),
                    pt(0.08, 0.04),
                    pt(0.08, 0.92),
                    pt(0.43, 0.92),
                ],
                s,
            );
            p.circle_filled(pt(0.29, 0.28), size * 0.075, color);
            p.line(vec![pt(0.17, 0.69), pt(0.36, 0.48), pt(0.49, 0.62)], s);
            p.add(egui::Shape::convex_polygon(
                vec![
                    pt(0.43, 0.98),
                    pt(0.50, 0.70),
                    pt(0.87, 0.27),
                    pt(1.03, 0.43),
                    pt(0.65, 0.86),
                ],
                Color32::TRANSPARENT,
                s,
            ));
            p.line_segment([pt(0.50, 0.70), pt(0.65, 0.86)], s);
        }
        Icon::Favorite => {
            let points = (0..10)
                .map(|i| {
                    let angle =
                        std::f32::consts::TAU * i as f32 / 10. - std::f32::consts::FRAC_PI_2;
                    center + Vec2::angled(angle) * size * if i % 2 == 0 { 0.5 } else { 0.23 }
                })
                .collect();
            p.add(egui::Shape::closed_line(points, s));
        }
        Icon::Back | Icon::Forward => {
            let direction = if matches!(kind, Icon::Back) { -1. } else { 1. };
            p.line(
                vec![
                    center + Vec2::new(-direction * size * 0.18, -size * 0.35),
                    center + Vec2::new(direction * size * 0.18, 0.),
                    center + Vec2::new(-direction * size * 0.18, size * 0.35),
                ],
                s,
            );
        }
        Icon::Up | Icon::Down => {
            let direction = if matches!(kind, Icon::Up) { -1. } else { 1. };
            p.line_segment(
                [
                    center + Vec2::new(0., -direction * size * 0.4),
                    center + Vec2::new(0., direction * size * 0.4),
                ],
                s,
            );
            p.line(
                vec![
                    center + Vec2::new(-size * 0.3, 0.),
                    center + Vec2::new(0., direction * size * 0.4),
                    center + Vec2::new(size * 0.3, 0.),
                ],
                s,
            );
        }
        Icon::List => {
            for row in 0..3 {
                let y = r.top() + size * (0.15 + row as f32 * 0.35);
                p.circle_filled(Pos2::new(r.left(), y), 1., color);
                p.line_segment(
                    [
                        Pos2::new(r.left() + size * 0.25, y),
                        Pos2::new(r.right(), y),
                    ],
                    s,
                );
            }
        }
        Icon::Refresh => {
            let points = (0..24)
                .map(|i| center + Vec2::angled(0.2 + i as f32 / 23. * 5.2) * size * 0.4)
                .collect();
            p.line(points, s);
            let tip = center + Vec2::angled(5.4) * size * 0.4;
            p.line(
                vec![
                    tip + Vec2::new(-size * 0.27, 0.),
                    tip,
                    tip + Vec2::new(0., -size * 0.27),
                ],
                s,
            );
        }
    }
}

pub fn nav_button(ui: &mut egui::Ui, label: &str, kind: Icon, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(52.0, 44.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    let color = if selected { Color32::WHITE } else { MUTED };
    if selected || response.hovered() {
        ui.painter()
            .rect_filled(rect, 10, if selected { NAVY } else { CREAM });
    }
    icon(
        ui.painter(),
        rect.center_top() + Vec2::new(0.0, 14.0),
        kind,
        color,
        18.0,
    );
    ui.painter().text(
        rect.center_top() + Vec2::new(0.0, 36.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(10.5),
        color,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Compact starting actions with native button focus/keyboard behavior.
pub fn start_chip(
    ui: &mut egui::Ui,
    title: &str,
    hint: &str,
    kind: Icon,
    background: Color32,
    width: f32,
    cat: Option<egui::TextureId>,
) -> egui::Response {
    let response = ui.add(
        egui::Button::new("")
            .frame(false)
            .min_size(Vec2::new(width, 92.)),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), title)
    });
    let active = response.hovered() || response.has_focus();
    let pressed = response.is_pointer_button_down_on();
    let dark = matches!(kind, Icon::Open);
    let ginger = Color32::from_rgb(236, 143, 57);
    let ink = Color32::from_rgb(36, 34, 33);
    let foreground = if dark { Color32::WHITE } else { ink };
    let rect = response.rect.translate(Vec2::new(
        0.,
        if pressed {
            1.
        } else if active {
            -2.
        } else {
            0.
        },
    ));
    let p = ui.painter();
    p.rect_filled(
        rect.translate(Vec2::new(0., 3.)),
        10,
        Color32::from_black_alpha(16),
    );
    p.rect(
        rect,
        10,
        background,
        Stroke::new(
            if active { 2. } else { 1. },
            if active {
                ginger
            } else if dark {
                ink
            } else {
                Color32::from_rgb(214, 198, 178)
            },
        ),
        egui::StrokeKind::Inside,
    );
    // Three small calico-color tabs give the set a shared, recognizable signature.
    for (i, color) in [ginger, Color32::from_rgb(255, 244, 225), ink]
        .into_iter()
        .enumerate()
    {
        p.rect_filled(
            Rect::from_min_size(
                rect.min + Vec2::new(17. + i as f32 * 11., 0.),
                Vec2::new(8., 3.),
            ),
            0,
            color,
        );
    }
    let badge = Rect::from_center_size(rect.left_center() + Vec2::new(38., 0.), Vec2::splat(44.));
    p.rect_filled(badge, 9, if dark { ginger } else { ink });
    icon(
        p,
        badge.center(),
        kind,
        if dark {
            ink
        } else {
            Color32::from_rgb(255, 244, 225)
        },
        25.,
    );
    let text = rect.left_center() + Vec2::new(74., -12.);
    p.text(
        text,
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(19.),
        foreground,
    );
    p.text(
        text + Vec2::new(0., 26.),
        egui::Align2::LEFT_CENTER,
        hint,
        egui::FontId::proportional(12.),
        if dark {
            Color32::from_rgb(218, 211, 201)
        } else {
            Color32::from_rgb(91, 76, 62)
        },
    );
    if let Some(texture) = cat {
        p.image(
            texture,
            Rect::from_center_size(rect.right_center() - Vec2::new(37., 0.), Vec2::splat(62.)),
            Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
            Color32::WHITE,
        );
    } else {
        icon(
            p,
            rect.right_top() + Vec2::new(-24., 27.),
            Icon::Forward,
            foreground.gamma_multiply(if active { 1. } else { 0.5 }),
            14.,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn primary(label: &str) -> egui::Button<'_> {
    egui::Button::new(egui::RichText::new(label).color(Color32::WHITE).strong()).fill(ACCENT)
}

pub fn action_button(
    ui: &mut egui::Ui,
    label: &str,
    kind: Icon,
    enabled: bool,
    primary: bool,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        let text = ui
            .painter()
            .layout_no_wrap(label.into(), egui::FontId::proportional(12.), INK);
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(text.size().x + 40., 30.), egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
        });
        let color = if primary { Color32::WHITE } else { INK };
        let visuals = ui.style().interact(&response);
        ui.painter().rect(
            rect,
            6,
            if primary {
                ACCENT
            } else if response.hovered() {
                visuals.bg_fill
            } else {
                Color32::TRANSPARENT
            },
            if primary || response.hovered() || response.has_focus() {
                visuals.bg_stroke
            } else {
                Stroke::NONE
            },
            egui::StrokeKind::Inside,
        );
        icon(
            ui.painter(),
            rect.left_center() + Vec2::new(15., 0.),
            kind,
            color,
            14.,
        );
        ui.painter().text(
            rect.left_center() + Vec2::new(29., 0.),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(12.),
            color,
        );
        response
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_slider_keeps_native_pointer_keyboard_and_disabled_behavior() {
        let ctx = egui::Context::default();
        apply(&ctx);
        let mut value = 50.;
        let frame = |value: &mut f32, events, enabled| {
            let mut response = None;
            let output = ctx.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.add_enabled_ui(enabled, |ui| {
                            response = Some(slider_track(ui, value, 1.0..=100.0, "Opacity", false));
                        });
                    });
                },
            );
            (response.unwrap(), output)
        };
        let (response, output) = frame(&mut value, vec![], true);
        assert!(response.rect.height() >= 24.);
        assert!(output.shapes.iter().any(|s| matches!(&s.shape,
            egui::Shape::Circle(c) if c.radius == 6. && c.fill == Color32::WHITE)));
        let target = response.rect.center() + Vec2::new(20., 0.);
        let pointer = |pressed| egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut value,
            vec![egui::Event::PointerMoved(target), pointer(true)],
            true,
        );
        let (response, _) = frame(&mut value, vec![pointer(false)], true);
        assert!(value > 50.);
        let before = value;
        ctx.memory_mut(|memory| memory.request_focus(response.id));
        frame(
            &mut value,
            vec![egui::Event::Key {
                key: egui::Key::ArrowRight,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            true,
        );
        assert!(value > before);
        value = 25.;
        frame(&mut value, vec![], false);
        frame(&mut value, vec![pointer(true)], false);
        frame(&mut value, vec![pointer(false)], false);
        assert_eq!(value, 25.);
    }

    #[test]
    fn native_theme_events_keep_the_custom_palette_and_spacing() {
        let ctx = egui::Context::default();
        apply(&ctx);
        for theme in [egui::Theme::Dark, egui::Theme::Light] {
            let _ = ctx.run(
                egui::RawInput {
                    system_theme: Some(theme),
                    ..Default::default()
                },
                |_| {},
            );
            assert_eq!(ctx.style().visuals.panel_fill, PAPER);
            assert_eq!(ctx.style().visuals.selection.stroke.color, ACCENT);
            assert_eq!(ctx.style().spacing.button_padding, Vec2::new(13.0, 9.0));
        }
    }
}
