//! Shared native Studio Contrast surfaces. File pickers remain platform dialogs.
use crate::appearance::{self, Icon};
use eframe::egui::{
    self, Color32, CornerRadius, Frame, Id, Response, RichText, Shadow, Stroke, Vec2,
};

pub const CHARCOAL: Color32 = Color32::from_rgb(41, 43, 47);
pub const SURFACE: Color32 = Color32::from_rgb(250, 250, 250);
pub const LINE: Color32 = Color32::from_rgb(216, 219, 223);
pub const MUTED: Color32 = Color32::from_rgb(103, 109, 117);
pub const DANGER: Color32 = Color32::from_rgb(157, 51, 44);

pub struct Dialog<'a> {
    title: &'a str,
    subtitle: &'a str,
    icon: Icon,
    width: f32,
}
impl<'a> Dialog<'a> {
    pub fn new(title: &'a str, subtitle: &'a str, icon: Icon, width: f32) -> Self {
        Self {
            title,
            subtitle,
            icon,
            width,
        }
    }
    pub fn modal<R>(
        self,
        ctx: &egui::Context,
        id: Id,
        content: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::ModalResponse<R> {
        egui::Modal::new(id)
            .frame(surface())
            .backdrop_color(Color32::from_black_alpha(85))
            .show(ctx, |ui| self.contents(ui, false, content).0)
    }
    pub fn window<R>(
        self,
        ctx: &egui::Context,
        id: Id,
        open: &mut bool,
        content: impl FnOnce(&mut egui::Ui) -> R,
    ) {
        let mut close = false;
        egui::Window::new(self.title)
            .id(id)
            .open(open)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .frame(surface())
            .default_width(self.width)
            .show(ctx, |ui| {
                close = self.contents(ui, true, content).1;
            });
        if close {
            *open = false;
        }
    }
    fn contents<R>(
        &self,
        ui: &mut egui::Ui,
        closable: bool,
        content: impl FnOnce(&mut egui::Ui) -> R,
    ) -> (R, bool) {
        let width = self
            .width
            .min((ui.ctx().content_rect().width() - 40.).max(220.));
        ui.set_width(width);
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let mut close = false;
        let header = Frame::new()
            .fill(CHARCOAL)
            .corner_radius(CornerRadius {
                nw: 8,
                ne: 8,
                sw: 0,
                se: 0,
            })
            .inner_margin(14)
            .show(ui, |ui| {
                ui.set_width(width - 28.);
                ui.spacing_mut().item_spacing = egui::vec2(10., 3.);
                ui.spacing_mut().button_padding = egui::vec2(4., 2.);
                ui.spacing_mut().interact_size = Vec2::splat(22.);
                ui.horizontal_top(|ui| {
                    let (badge, _) = ui.allocate_exact_size(Vec2::splat(34.), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(badge, 7, Color32::from_rgb(65, 61, 57));
                    appearance::icon(
                        ui.painter(),
                        badge.center(),
                        self.icon,
                        appearance::GINGER,
                        20.,
                    );
                    let text_width =
                        (width - 28. - 44. - if closable { 32. } else { 0. }).max(100.);
                    ui.allocate_ui_with_layout(
                        egui::vec2(text_width, 0.),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_min_width(text_width);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(self.title)
                                        .size(18.)
                                        .strong()
                                        .color(Color32::WHITE),
                                )
                                .wrap(),
                            );
                            if !self.subtitle.is_empty() {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(self.subtitle)
                                            .size(12.)
                                            .color(Color32::from_rgb(203, 200, 197)),
                                    )
                                    .wrap(),
                                );
                            }
                        },
                    );
                    if closable {
                        let button = ui.add(
                            egui::Button::new(
                                RichText::new("×").size(19.).color(Color32::from_gray(220)),
                            )
                            .frame(false),
                        );
                        close = button.clone().on_hover_text("Close").clicked();
                        button.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                true,
                                "Close dialog",
                            )
                        });
                    }
                });
            });
        let max_height =
            (ui.ctx().content_rect().height() - header.response.rect.height() - 70.).max(100.);
        let inner = Frame::new()
            .inner_margin(14)
            .show(ui, |ui| {
                ui.set_width(width - 28.);
                // Modal centering initially places the area halfway down the
                // screen. Reserve against the screen, not that temporary space.
                ui.set_max_height(max_height);
                apply_style(ui);
                egui::ScrollArea::vertical()
                    .id_salt("studio-dialog-body")
                    .max_height(max_height)
                    .min_scrolled_height(0.)
                    .auto_shrink([false, true])
                    .show(ui, content)
                    .inner
            })
            .inner;
        (inner, close)
    }
}

pub fn surface() -> Frame {
    Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1., LINE))
        .corner_radius(8)
        .shadow(Shadow {
            offset: [0, 12],
            blur: 30,
            spread: 1,
            color: Color32::from_black_alpha(65),
        })
}
fn apply_style(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing = egui::vec2(8., 8.);
    ui.spacing_mut().button_padding = egui::vec2(10., 6.);
    ui.spacing_mut().interact_size.y = 30.;
    let visuals = &mut ui.style_mut().visuals;
    visuals.override_text_color = Some(CHARCOAL);
    visuals.weak_text_color = Some(MUTED);
    visuals.extreme_bg_color = Color32::WHITE;
    visuals.selection.bg_fill = Color32::from_rgb(242, 220, 194);
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
    ] {
        widget.corner_radius = CornerRadius::same(5);
        widget.bg_stroke = Stroke::new(1., LINE);
    }
    visuals.widgets.inactive.bg_fill = Color32::WHITE;
    visuals.widgets.inactive.weak_bg_fill = Color32::WHITE;
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(242, 237, 230);
    visuals.widgets.hovered.weak_bg_fill = visuals.widgets.hovered.bg_fill;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1., appearance::ACCENT);
    visuals.widgets.active.bg_stroke = Stroke::new(1.5, appearance::ACCENT);
}

#[derive(Clone, Copy)]
pub enum Action {
    Primary,
    Secondary,
    Danger,
}
pub fn button(
    ui: &mut egui::Ui,
    label: &str,
    icon: Icon,
    action: Action,
    enabled: bool,
) -> Response {
    let (fill, ink, line) = match action {
        Action::Primary => (
            appearance::ACCENT,
            Color32::WHITE,
            Color32::from_rgb(126, 61, 23),
        ),
        Action::Secondary => (Color32::WHITE, CHARCOAL, LINE),
        Action::Danger => (
            Color32::from_rgb(253, 240, 237),
            DANGER,
            Color32::from_rgb(228, 192, 185),
        ),
    };
    let shadow_index = ui.painter().add(egui::Shape::Noop);
    let label_galley = egui::WidgetText::from(RichText::new(label).strong()).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Button,
    );
    let label_width = label_galley.size().x;
    let response = ui.add_enabled(
        enabled,
        egui::Button::new("")
            .fill(fill)
            .stroke(Stroke::new(1., line))
            .corner_radius(6)
            .min_size(egui::vec2(label_width + 64., 32.)),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled && ui.is_enabled(), label)
    });
    if ui.is_rect_visible(response.rect) {
        let active = enabled && ui.is_enabled();
        if active {
            ui.painter().set(
                shadow_index,
                Shadow {
                    offset: [0, 2],
                    blur: 4,
                    spread: 0,
                    color: Color32::from_black_alpha(22),
                }
                .as_shape(response.rect, 6),
            );
            if response.hovered() || response.has_focus() {
                ui.painter().rect_stroke(
                    response.rect.expand(1.),
                    7,
                    Stroke::new(1.5, appearance::ACCENT),
                    egui::StrokeKind::Outside,
                );
            }
            ui.painter().line_segment(
                [
                    response.rect.left_top() + egui::vec2(7., 1.5),
                    response.rect.right_top() + egui::vec2(-7., 1.5),
                ],
                Stroke::new(1., Color32::from_white_alpha(45)),
            );
        }
        ui.painter().galley_with_override_text_color(
            response.rect.left_center() + Vec2::new(32., -label_galley.size().y * 0.5),
            label_galley,
            if active { ink } else { ink.gamma_multiply(0.4) },
        );
        appearance::icon(
            ui.painter(),
            response.rect.left_center() + egui::vec2(16., 0.),
            icon,
            if active { ink } else { ink.gamma_multiply(0.4) },
            15.,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
pub fn primary(ui: &mut egui::Ui, label: &str, icon: Icon) -> Response {
    button(ui, label, icon, Action::Primary, true)
}
pub fn secondary(ui: &mut egui::Ui, label: &str, icon: Icon) -> Response {
    button(ui, label, icon, Action::Secondary, true)
}
pub fn danger(ui: &mut egui::Ui, label: &str, icon: Icon) -> Response {
    button(ui, label, icon, Action::Danger, true)
}
pub fn actions<R>(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.add_space(4.);
    ui.separator();
    ui.add_space(3.);
    ui.horizontal_wrapped(content).inner
}
pub fn notice(ui: &mut egui::Ui, icon: Icon, title: &str, text: &str, error: bool) {
    Frame::new()
        .fill(if error {
            Color32::from_rgb(253, 241, 238)
        } else {
            Color32::from_rgb(239, 241, 243)
        })
        .stroke(Stroke::new(
            1.,
            if error {
                Color32::from_rgb(231, 201, 195)
            } else {
                LINE
            },
        ))
        .corner_radius(6)
        .inner_margin(10)
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(20.), egui::Sense::hover());
                appearance::icon(
                    ui.painter(),
                    rect.center(),
                    icon,
                    if error { DANGER } else { appearance::ACCENT },
                    18.,
                );
                ui.vertical(|ui| {
                    if !title.is_empty() {
                        ui.label(RichText::new(title).strong().color(CHARCOAL));
                    }
                    ui.add(
                        egui::Label::new(RichText::new(text).color(if error {
                            DANGER
                        } else {
                            MUTED
                        }))
                        .wrap(),
                    );
                });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tall_prompts_fit_reduced_workspaces_and_escape_preserves_the_choice() {
        for size in [
            egui::vec2(980., 640.),
            egui::vec2(700., 460.),
            egui::vec2(560., 360.),
        ] {
            let ctx = egui::Context::default();
            appearance::apply(&ctx);
            for frame in 0..4 {
                let mut input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                };
                if frame == 3 {
                    input.events.push(egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
                let _ = ctx.run(input, |ctx| {
                    let shown = Dialog::new("Recover unfinished projects", "Choose the editable work to continue.", Icon::Refresh, 490.).modal(ctx, Id::new("bounded-studio"), |ui| {
                        for _ in 0..20 { ui.label("A recovered project with a long filename and detailed information that wraps onto another line."); }
                        let chosen = actions(ui, |ui| primary(ui, "Recover project", Icon::Refresh).clicked());
                        assert!(!chosen, "Escape must not activate the primary action");
                    });
                    if frame > 0 { assert!(ctx.content_rect().expand(1.).contains_rect(shown.response.rect), "{size:?}: {:?}", shown.response.rect); }
                    assert_eq!(shown.should_close(), frame == 3);
                });
            }
        }
    }
}
