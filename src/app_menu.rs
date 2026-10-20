//! Compact Studio Contrast app controls, separate from the image workspace.
use super::*;
use egui::{Align2, FontId, RichText, Sense};

pub(super) fn config(ctx: &egui::Context) -> egui::containers::menu::MenuConfig {
    let palette = appearance::palette(ctx);
    egui::containers::menu::MenuConfig::new()
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .style(move |style: &mut egui::Style| {
            egui::containers::menu::menu_style(style);
            style.spacing.menu_margin = egui::Margin::ZERO;
            style.visuals.window_fill = palette.paper;
            style.visuals.window_stroke = Stroke::new(1., palette.border);
            style.visuals.menu_corner_radius = egui::CornerRadius::same(8);
            style.visuals.popup_shadow = egui::Shadow {
                offset: [0, 8],
                blur: 26,
                spread: 0,
                color: Color32::from_black_alpha(70),
            };
        })
}

pub(super) fn row(
    ui: &mut egui::Ui,
    label: &str,
    icon: Icon,
    enabled: bool,
    trailing: &str,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
        });
        let active = response.hovered() || response.has_focus();
        if active {
            ui.painter().rect(
                rect,
                5,
                appearance::palette(ui.ctx()).cream,
                Stroke::new(1., appearance::palette(ui.ctx()).border),
                egui::StrokeKind::Inside,
            );
        }
        let badge =
            Rect::from_center_size(rect.left_center() + Vec2::new(17., 0.), Vec2::splat(24.));
        ui.painter().rect_filled(
            badge,
            5,
            if active {
                Color32::WHITE
            } else {
                appearance::palette(ui.ctx()).base
            },
        );
        appearance::icon(
            ui.painter(),
            badge.center(),
            icon,
            if active {
                appearance::palette(ui.ctx()).accent
            } else {
                crate::appearance::palette(ui.ctx()).navy
            },
            14.,
        );
        ui.painter().text(
            rect.left_center() + Vec2::new(37., 0.),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(12.),
            crate::appearance::palette(ui.ctx()).navy,
        );
        ui.painter().text(
            rect.right_center() - Vec2::new(10., 0.),
            Align2::RIGHT_CENTER,
            trailing,
            FontId::proportional(11.),
            crate::appearance::palette(ui.ctx()).muted,
        );
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    })
    .inner
}

impl MyImageApp {
    pub(super) fn app_menu_contents(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.set_width(278.);
        ui.set_max_height((ctx.content_rect().height() - 55.).max(200.));
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let (header, _) = ui.allocate_exact_size(Vec2::new(278., 58.), Sense::hover());
        ui.painter().rect_filled(
            header,
            egui::CornerRadius {
                nw: 8,
                ne: 8,
                sw: 0,
                se: 0,
            },
            crate::appearance::palette(ui.ctx()).navy,
        );
        appearance::calico_tabs(ui.painter(), header.min + Vec2::new(16., 0.));
        ui.painter().image(
            self.app_icon.id(),
            Rect::from_min_size(header.min + Vec2::new(12., 6.), Vec2::splat(46.)),
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
            Color32::WHITE,
        );
        ui.painter().text(
            header.min + Vec2::new(69., 10.),
            Align2::LEFT_TOP,
            "MyImage",
            FontId::proportional(21.),
            Color32::WHITE,
        );
        let build = include_str!("../packaging/macos/Info.plist")
            .split_once("<key>CFBundleVersion</key>")
            .and_then(|(_, value)| value.split_once("<string>"))
            .and_then(|(_, value)| value.split_once("</string>"))
            .map_or("development", |(value, _)| value);
        ui.painter().text(
            header.min + Vec2::new(70., 36.),
            Align2::LEFT_TOP,
            format!("{}  ·  build {build}", env!("CARGO_PKG_VERSION")),
            FontId::proportional(11.),
            Color32::from_gray(195),
        );
        egui::Frame::new().inner_margin(9).show(ui, |ui| {
            ui.set_width(260.);
            ui.spacing_mut().item_spacing = Vec2::new(5., 3.);
            ui.spacing_mut().interact_size.y = 22.;
            ui.spacing_mut().button_padding = Vec2::new(4., 3.);
            egui::ScrollArea::vertical()
                .max_height((ctx.content_rect().height() - 105.).max(130.))
                .show(ui, |ui| {
                let (rect, play) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 36.), Sense::click());
                play.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Play with Calico"));
                ui.painter().rect(rect, 6, if play.hovered() { appearance::palette(ui.ctx()).ginger } else { appearance::palette(ui.ctx()).cream }, Stroke::new(1., appearance::palette(ui.ctx()).border), egui::StrokeKind::Inside);
                appearance::icon(ui.painter(), rect.left_center() + Vec2::new(18., 0.), Icon::Favorite, appearance::palette(ui.ctx()).accent, 18.);
                ui.painter().text(rect.left_center() + Vec2::new(37., 0.), Align2::LEFT_CENTER, "Play with Calico", FontId::proportional(13.), appearance::palette(ui.ctx()).ink);
                ui.painter().text(rect.right_center() - Vec2::new(12., 0.), Align2::RIGHT_CENTER, "5s", FontId::proportional(11.), appearance::palette(ui.ctx()).accent);
                if play.on_hover_text("A five-second surprise · Escape to stop").clicked() {
                    self.calico.start(ctx.input(|i| i.time));
                    ui.close();
                }
                ui.add_space(5.);
                let can_open = !self.busy && !self.guide_open && !self.about_open && !self.appearance_open && !self.recovery.prompt
                    && self.pending_action.is_none() && self.file_dialog.is_none()
                    && !self.editor.has_modal() && !self.export_panel.open;
                if row(ui, if self.view == View::Viewer { "Return to Editor" } else { "View only" }, if self.view == View::Viewer { Icon::Edit } else { Icon::Eye }, can_open && self.document.is_some(), "›").clicked() {
                    self.finish_edit();
                    self.view = if self.view == View::Viewer { View::Editor } else { View::Viewer };
                    ui.close();
                }
                self.recovery_menu(ui);
                if row(ui, "About MyImage", Icon::Info, can_open, "›").clicked() {
                    self.about_open = true;
                    ui.close();
                }
                if row(ui, "Quick start & shortcuts", Icon::Info, can_open, "›").clicked() {
                    self.guide_open = true;
                    ui.close();
                }
                if row(ui, "Releases & updates", Icon::Refresh, true, "↗").clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab("https://github.com/gokcol/MyImage/releases"));
                    ui.close();
                }
                if row(ui, "Storage folder", Icon::Open, can_open, "›").clicked() {
                    self.file_dialog = Some(file_browser::FileDialog::Storage);
                    ui.close();
                }
                ui.add_space(5.);
                egui::Frame::new().fill(appearance::palette(ui.ctx()).base).corner_radius(6).inner_margin(8).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(14.), Sense::hover());
                        appearance::icon(ui.painter(), r.center(), Icon::TextSize, crate::appearance::palette(ui.ctx()).muted, 13.);
                        ui.label(RichText::new("Interface size").size(11.).color(crate::appearance::palette(ui.ctx()).muted));
                    });
                    ui.add_space(4.);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.;
                        let width = (ui.available_width() - 6.) / 4.;
                        for size in storage::UiTextSize::ALL {
                            let selected = self.settings.ui_text_size == size;
                            let response = ui.add_sized([width, 27.], egui::Button::new(RichText::new(size.label()).size(10.5).color(if selected { Color32::WHITE } else { crate::appearance::palette(ui.ctx()).navy })).fill(if selected { crate::appearance::palette(ui.ctx()).navy } else { Color32::TRANSPARENT }).stroke(Stroke::NONE).corner_radius(4));
                            if response.clicked() {
                                self.settings.ui_text_size = size;
                                ctx.set_zoom_factor(size.factor());
                                self.persist_settings();
                            }
                        }
                    });
                }).response.on_hover_text("Scales the interface. Image pixels and annotation text stay unchanged.");
                ui.add_space(6.);
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - 5.) / 2.;
                    ui.allocate_ui_with_layout(Vec2::new(width, 33.), egui::Layout::top_down(egui::Align::Min), |ui| {
                        if row(ui, "Background", Icon::Tray, self.desktop.is_some(), "").on_hover_text("Keep in menu bar / tray").clicked() {
                            self.hide(ctx);
                            ui.close();
                        }
                    });
                    ui.allocate_ui_with_layout(Vec2::new(width, 33.), egui::Layout::top_down(egui::Align::Min), |ui| {
                        if row(ui, "Quit MyImage", Icon::Power, true, "").clicked() {
                            self.request_quit(ctx);
                            ui.close();
                        }
                    });
                });
                if row(ui, "Settings · Appearance", Icon::Settings, can_open, "›").clicked() {
                    self.appearance_open = true;
                    ui.close();
                }
            });
        });
    }
}

impl MyImageApp {
    pub(super) fn about_ui(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }
        let mut close = false;
        let response = Dialog::new(
            "About MyImage",
            "Capture. Annotate. Make it clear.",
            Icon::Info,
            360.,
        )
        .modal(ctx, egui::Id::new("about-myimage"), |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Image::new(&self.app_icon).fit_to_exact_size(Vec2::splat(72.)));
                ui.vertical(|ui| {
                    ui.label(RichText::new("MyImage").size(24.).strong());
                    let build = include_str!("../packaging/macos/Info.plist")
                        .split_once("<key>CFBundleVersion</key>")
                        .and_then(|(_, s)| s.split_once("<string>"))
                        .and_then(|(_, s)| s.split_once("</string>"))
                        .map_or("development", |(s, _)| s);
                    ui.label(format!(
                        "Version {} · build {build}",
                        env!("CARGO_PKG_VERSION")
                    ));
                    ui.label(
                        RichText::new("Created by Orhan Gökçöl")
                            .strong()
                            .color(appearance::palette(ui.ctx()).accent),
                    );
                });
            });
            ui.add_space(8.);
            ui.label("A playful capture and annotation studio for macOS and Windows.");
            ui.small("© 2026 Orhan Gökçöl · MIT License");
            ui.hyperlink_to(
                "GitHub · source & releases",
                "https://github.com/gokcol/MyImage",
            );
            ui.hyperlink_to(
                "Privacy policy",
                "https://github.com/gokcol/MyImage/blob/main/PRIVACY.md",
            );
            ui.add_space(8.);
            close = dialogs::primary(ui, "Done", Icon::Check).clicked();
        });
        if close || response.should_close() {
            self.about_open = false;
        }
    }
}

impl MyImageApp {
    pub(super) fn appearance_ui(&mut self, ctx: &egui::Context) {
        if !self.appearance_open {
            return;
        }
        let mut finish = self.settings.gui_finish;
        let mut icon_style = self.settings.icon_style;
        let response = Dialog::new(
            "Appearance",
            "Choose your finish and icons. Your workspace stays in place.",
            Icon::Settings,
            350.,
        )
        .modal(ctx, egui::Id::new("appearance-settings"), |ui| {
            for choice in appearance::GuiFinish::ALL {
                let colors = choice.palette();
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(65., 30.), egui::Sense::hover());
                    for (i, color) in [colors.navy, colors.accent, colors.cream]
                        .into_iter()
                        .enumerate()
                    {
                        ui.painter().rect_filled(
                            Rect::from_min_size(
                                r.min + Vec2::new(i as f32 * 21., 5.),
                                Vec2::splat(19.),
                            ),
                            4,
                            color,
                        );
                    }
                    ui.selectable_value(&mut finish, choice, choice.label());
                });
            }
            ui.add_space(5.);
            ui.separator();
            ui.strong("Icon style");
            for choice in crate::icon_style::IconStyle::ALL {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(65., 30.), egui::Sense::hover());
                    for (i, glyph) in [
                        crate::icon_style::Glyph::Arrow,
                        crate::icon_style::Glyph::Callout,
                        crate::icon_style::Glyph::Magnify,
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        crate::icon_style::preview(
                            ui.painter(),
                            r.left_center() + Vec2::new(10. + i as f32 * 22., 0.),
                            glyph,
                            appearance::palette(ui.ctx()).ink,
                            17.,
                            choice,
                        );
                    }
                    ui.selectable_value(&mut icon_style, choice, choice.label());
                });
            }
            ui.add_space(5.);
            ui.small("Applies to the interface only. Image and annotation colors are unchanged.");
            dialogs::actions(ui, |ui| dialogs::primary(ui, "Done", Icon::Check).clicked())
        });
        if finish != self.settings.gui_finish {
            self.settings.gui_finish = finish;
            appearance::set_finish(ctx, finish);
            self.persist_settings();
        }
        if icon_style != self.settings.icon_style {
            self.settings.icon_style = icon_style;
            crate::icon_style::set(ctx, icon_style);
            self.persist_settings();
        }
        if response.inner || response.should_close() {
            self.appearance_open = false;
        }
    }
}
