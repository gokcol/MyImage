//! A centered brand card with accessible text and the platform's update route.
use super::*;
use egui::{Align, Align2, CornerRadius, Frame, RichText, Sense};

impl MyImageApp {
    pub(super) fn about_ui(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }
        let palette = appearance::palette(ctx);
        let width = (ctx.content_rect().width() - 48.).clamp(220., 520.);
        let max_height = (ctx.content_rect().height() - 48.).max(180.);
        let mut close = false;
        let mut updates = false;
        let id = egui::Id::new("about-myimage");
        let response = egui::Modal::new(id)
            .area(egui::Modal::default_area(id).anchor(Align2::CENTER_CENTER, Vec2::ZERO))
            .backdrop_color(Color32::from_black_alpha(112))
            .frame(
                Frame::new()
                    .fill(palette.paper)
                    .stroke(Stroke::new(1., palette.border))
                    .corner_radius(18)
                    .shadow(egui::Shadow {
                        offset: [0, 14],
                        blur: 40,
                        spread: 0,
                        color: Color32::from_black_alpha(80),
                    }),
            )
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                egui::ScrollArea::vertical()
                    .id_salt("about-body")
                    .max_height(max_height)
                    .min_scrolled_height(0.)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        close |= about_hero(ui, &self.app_icon, width);
                        Frame::new().inner_margin(24).show(ui, |ui| {
                            ui.set_width(width - 48.);
                            ui.spacing_mut().item_spacing = Vec2::new(10., 8.);
                            ui.vertical_centered(|ui| {
                                let notes = crate::updates::notes();
                                Frame::new()
                                    .fill(palette.cream)
                                    .stroke(Stroke::new(1., palette.border))
                                    .corner_radius(24)
                                    .inner_margin(egui::Margin::symmetric(14, 7))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(format!(
                                                "Version {}  ·  Build {}",
                                                notes.version, notes.build
                                            ))
                                            .size(13.)
                                            .strong()
                                            .color(palette.accent),
                                        );
                                    });
                                ui.add_space(4.);
                                ui.label(
                                    RichText::new("Your ideas, beautifully captured.")
                                        .size(19.)
                                        .strong()
                                        .color(palette.ink),
                                );
                                ui.add(egui::Label::new("Capture a moment. Add your perspective.\nKeep every edit yours.").wrap());
                                ui.add_space(8.);
                                ui.separator();
                                ui.add_space(6.);
                                ui.label(RichText::new("Designed & developed by").size(12.).color(palette.muted));
                                ui.label(RichText::new("Orhan Gökçöl").size(17.).strong().color(palette.ink));
                                ui.hyperlink_to("Get support", "mailto:orhan.gokcol@hotmail.com");
                                ui.add_space(6.);
                                // Opening this local panel does not start a network request.
                                updates = dialogs::primary(ui, "What's new & updates", Icon::Refresh).clicked();
                                ui.label(RichText::new(if cfg!(target_os = "windows") {
                                    "Updates via Microsoft Store"
                                } else {
                                    "Updates via GitHub"
                                }).size(12.).color(palette.muted));
                                ui.add_space(4.);
                                ui.horizontal_wrapped(|ui| {
                                    ui.hyperlink_to("Source on GitHub", "https://github.com/gokcol/MyImage");
                                    ui.label(RichText::new("·").color(palette.muted));
                                    ui.hyperlink_to("Privacy", "https://github.com/gokcol/MyImage/blob/main/PRIVACY.md");
                                    ui.label(RichText::new("·").color(palette.muted));
                                    ui.hyperlink_to("MIT License", "https://github.com/gokcol/MyImage/blob/main/LICENSE");
                                });
                                ui.label(RichText::new("© 2026 Orhan Gökçöl").size(11.).color(palette.muted));
                                ui.add_space(5.);
                                close |= dialogs::secondary(ui, "Done", Icon::Check).clicked();
                            });
                        });
                    });
            });
        if close || updates || response.should_close() {
            self.about_open = false;
        }
        if updates {
            self.updates.pending_notes = false;
            self.updates.open = true;
        }
    }
}

fn about_hero(ui: &mut egui::Ui, icon: &egui::TextureHandle, width: f32) -> bool {
    let p = appearance::palette(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 208.), Sense::hover());
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect_filled(
        rect,
        CornerRadius {
            nw: 18,
            ne: 18,
            sw: 0,
            se: 0,
        },
        p.navy,
    );
    // Quiet capture/annotation motifs frame the existing Calico artwork.
    for (offset, radius) in [
        (Vec2::new(-width * 0.38, -36.), 98.),
        (Vec2::new(width * 0.41, 34.), 124.),
    ] {
        painter.circle_stroke(
            rect.center() + offset,
            radius,
            Stroke::new(1., p.ginger.gamma_multiply(0.18)),
        );
        painter.circle_stroke(
            rect.center() + offset,
            radius - 18.,
            Stroke::new(1., p.ginger.gamma_multiply(0.09)),
        );
    }
    for (offset, motif) in [
        (Vec2::new(-width * 0.34, 7.), Icon::Capture),
        (Vec2::new(width * 0.34, 7.), Icon::Edit),
    ] {
        let center = rect.center() + offset;
        painter.rect_stroke(
            Rect::from_center_size(center, Vec2::splat(42.)),
            12,
            Stroke::new(1., p.ginger.gamma_multiply(0.28)),
            egui::StrokeKind::Inside,
        );
        appearance::icon(&painter, center, motif, p.ginger.gamma_multiply(0.5), 23.);
    }
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(38., 20.)))
            .layout(egui::Layout::top_down(Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.y = 6.;
            ui.add(egui::Image::new(icon).fit_to_exact_size(Vec2::splat(88.)));
            ui.label(
                RichText::new("MyImage")
                    .size(32.)
                    .strong()
                    .color(Color32::WHITE),
            );
            ui.label(
                RichText::new("Capture. Annotate. Make it clear.")
                    .size(13.)
                    .color(p.ginger),
            );
        },
    );
    let button = ui.put(
        Rect::from_min_size(rect.right_top() + Vec2::new(-38., 10.), Vec2::splat(28.)),
        egui::Button::new(RichText::new("×").size(23.).color(Color32::WHITE))
            .fill(p.navy)
            .stroke(Stroke::NONE)
            .corner_radius(14),
    );
    button.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Close About MyImage")
    });
    button.on_hover_text("Close About MyImage").clicked()
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
                        |ctx| app.about_ui(ctx),
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
                |ctx| app.about_ui(ctx),
            );
            assert!(!app.about_open);
        }
    }
}
