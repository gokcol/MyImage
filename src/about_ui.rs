//! MyPDF-style About card, centered with a fixed footer and platform update actions.
use super::*;
use egui::{Align, Align2, FontId, Frame, RichText, Sense};

impl MyImageApp {
    pub(super) fn about_ui(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if !self.about_open {
            return;
        }
        let palette = appearance::palette(ctx);
        let width = (ctx.content_rect().width() - 96.).clamp(220., 420.);
        let body_height = (ctx.content_rect().height() - 144.).clamp(80., 440.);
        let mut close = false;
        let mut show_notes = false;
        let mut check = false;
        let id = egui::Id::new("about-myimage");
        let response = egui::Modal::new(id)
            .area(egui::Modal::default_area(id).anchor(Align2::CENTER_CENTER, Vec2::ZERO))
            .backdrop_color(Color32::from_black_alpha(100))
            .frame(
                Frame::new()
                    .fill(Color32::WHITE)
                    .stroke(Stroke::new(1., palette.border))
                    .corner_radius(18)
                    .inner_margin(24)
                    .shadow(egui::Shadow {
                        offset: [0, 18],
                        blur: 48,
                        spread: 0,
                        color: Color32::from_black_alpha(55),
                    }),
            )
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing = Vec2::new(8., 8.);
                egui::ScrollArea::vertical()
                    .id_salt("about-body")
                    .max_height(body_height)
                    .min_scrolled_height(0.)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(12.);
                            ui.add(egui::Image::new(&self.app_icon).fit_to_exact_size(Vec2::splat(80.)));
                            ui.add_space(12.);
                            ui.label(RichText::new("MyImage").size(32.).strong().color(palette.ink));
                            ui.label(RichText::new("Screen capture & image editor").size(15.).color(palette.muted));
                            ui.add_space(10.);
                            let notes = crate::updates::notes();
                            Frame::new()
                                .fill(palette.cream)
                                .corner_radius(12)
                                .inner_margin(egui::Margin::symmetric(12, 5))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(format!("Version {} · Build {}", notes.version, notes.build)).size(12.).color(palette.accent));
                                });
                            ui.add_space(16.);
                            ui.label(RichText::new("Developed by Orhan Gökçöl").size(14.).strong().color(palette.ink));
                            ui.add_space(18.);
                        });
                        if ui.available_width() >= 360. {
                            ui.columns(2, |columns| {
                                show_notes = about_action(&mut columns[0], "What's New", Icon::Info).clicked();
                                check = about_action(&mut columns[1], "Check for Updates…", Icon::Refresh).clicked();
                            });
                        } else {
                            show_notes = about_action(ui, "What's New", Icon::Info).clicked();
                            check = about_action(ui, "Check for Updates…", Icon::Refresh).clicked();
                        }
                        ui.add_space(18.);
                        ui.separator();
                        ui.add_space(12.);
                        ui.label(RichText::new("MyImage makes everyday screen capture and visual communication easier. Capture, annotate and keep your work editable — your images and projects stay on your device.").size(12.).color(palette.muted));
                        ui.add_space(4.);
                        ui.label(RichText::new(if cfg!(target_os = "windows") {
                            "Updates are checked through Microsoft Store."
                        } else {
                            "Updates are checked through GitHub."
                        }).size(12.).color(palette.muted));
                        ui.add_space(8.);
                    });
                ui.add_space(10.);
                ui.horizontal(|ui| {
                    ui.hyperlink_to("GitHub", "https://github.com/gokcol/MyImage");
                    ui.hyperlink_to("Support", "mailto:orhan.gokcol@hotmail.com");
                    ui.hyperlink_to("Privacy", "https://github.com/gokcol/MyImage/blob/main/PRIVACY.md");
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        close = dialogs::primary(ui, "Close", Icon::Close).clicked();
                    });
                });
            });
        if close || show_notes || check || response.should_close() {
            self.about_open = false;
        }
        if show_notes || check {
            self.updates.pending_notes = false;
            self.updates.open = true;
        }
        if check {
            self.start_update_check(ctx, frame);
        }
    }
}

fn about_action(ui: &mut egui::Ui, label: &str, icon: Icon) -> egui::Response {
    let p = appearance::palette(ui.ctx());
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 36.), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    if response.hovered() || response.has_focus() {
        ui.painter().rect(
            rect,
            8,
            p.cream,
            Stroke::new(1., p.border),
            egui::StrokeKind::Inside,
        );
    }
    let font = FontId::proportional(13.);
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, p.ink);
    let left = rect.center().x - (galley.size().x + 28.) * 0.5;
    appearance::icon(
        ui.painter(),
        egui::pos2(left + 9., rect.center().y),
        icon,
        p.ink,
        18.,
    );
    ui.painter().galley(
        egui::pos2(left + 28., rect.center().y - galley.size().y * 0.5),
        galley,
        p.ink,
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_stays_centered_and_bounded_across_window_sizes_and_finishes() {
        for finish in appearance::GuiFinish::ALL {
            let ctx = egui::Context::default();
            appearance::set_finish(&ctx, finish);
            let mut app =
                MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
            app.guide_open = false;
            app.about_open = true;
            assert!(!app.workspace_unobscured());
            app.begin_capture(&ctx);
            assert!(app.pending_capture.is_none());
            for size in [
                Vec2::new(1200., 800.),
                Vec2::new(700., 460.),
                Vec2::new(380., 540.),
            ] {
                for pass in 0..4 {
                    let output = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                            ..Default::default()
                        },
                        |ctx| app.about_ui(ctx, &eframe::Frame::_new_kittest()),
                    );
                    assert!(output.platform_output.commands.is_empty());
                    if pass == 3 {
                        let rect = ctx
                            .memory(|m| m.area_rect(egui::Id::new("about-myimage")))
                            .unwrap();
                        assert!(
                            ctx.content_rect().expand(1.).contains_rect(rect),
                            "{size:?}: {rect:?}"
                        );
                        assert!(
                            rect.center().distance(ctx.content_rect().center()) < 2.,
                            "dialog must remain centered after resize: {rect:?}"
                        );
                    }
                }
            }
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |ctx| app.about_ui(ctx, &eframe::Frame::_new_kittest()),
            );
            assert!(!app.about_open);
        }
    }
}
