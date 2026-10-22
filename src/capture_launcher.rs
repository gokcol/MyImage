//! Resident capture launcher: a native child window, independent of the editor.
use super::*;
use crate::capture::{CaptureDestination, CaptureMode};

const SIZE: Vec2 = Vec2::new(560., 292.);
const NOTCH: f32 = 8.;

/// Pearl Compact is intentionally dark, independent of the editor's GUI finish.
fn pearl_palette() -> appearance::Palette {
    appearance::Palette {
        ink: Color32::from_rgb(238, 241, 247),
        muted: Color32::from_rgb(184, 194, 211),
        accent: Color32::from_rgb(82, 92, 219),
        navy: Color32::from_rgb(41, 47, 58),
        paper: Color32::from_rgb(48, 54, 65),
        ginger: Color32::from_rgb(192, 197, 255),
        cream: Color32::from_rgb(55, 59, 92),
        border: Color32::from_rgb(77, 86, 101),
        base: Color32::from_rgb(62, 70, 84),
        accent_top: Color32::from_rgb(122, 130, 239),
    }
}
fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("resident-capture-panel")
}

pub(super) struct Launcher {
    position: Pos2,
    focused: bool,
    request_focus: bool,
}

fn panel_position(screen: Rect, anchor: Rect, size: Vec2) -> Pos2 {
    let x = (anchor.center().x - size.x * 0.5).clamp(
        screen.left() + 8.,
        (screen.right() - size.x - 8.).max(screen.left() + 8.),
    );
    let below = anchor.bottom() + 8.;
    let y = if below + size.y <= screen.bottom() - 8. {
        below
    } else {
        anchor.top() - size.y - 8.
    };
    egui::pos2(
        x,
        y.clamp(
            screen.top() + 8.,
            (screen.bottom() - size.y - 8.).max(screen.top() + 8.),
        ),
    )
}

fn position(displays: &[DisplayInfo], tray: Option<Rect>, zoom: f32) -> Pos2 {
    let display = displays
        .iter()
        .find(|d| {
            let Some(tray) = tray else {
                return false;
            };
            let p = if cfg!(target_os = "macos") {
                tray.center() / d.scale_factor.max(1.)
            } else {
                tray.center()
            };
            Rect::from_min_size(
                egui::pos2(d.x as f32, d.y as f32),
                Vec2::new(d.width as f32, d.height as f32),
            )
            .contains(p)
        })
        .or_else(|| displays.iter().find(|d| d.is_primary))
        .or(displays.first());
    let Some(d) = display else {
        return egui::pos2(20., 40.);
    };
    let divisor = if cfg!(target_os = "macos") {
        zoom
    } else {
        d.scale_factor.max(1.) * zoom
    };
    let screen = Rect::from_min_size(
        egui::pos2(d.x as f32, d.y as f32) / divisor,
        Vec2::new(d.width as f32, d.height as f32) / divisor,
    );
    let tray_divisor = d.scale_factor.max(1.) * zoom;
    let anchor = tray
        .map(|r| Rect::from_min_max(r.min / tray_divisor, r.max / tray_divisor))
        .unwrap_or_else(|| {
            Rect::from_min_size(screen.right_top() - Vec2::new(40., -8.), Vec2::splat(24.))
        });
    panel_position(screen, anchor, SIZE)
}

impl MyImageApp {
    pub(super) fn close_capture_launcher(&mut self, ctx: &egui::Context) {
        if self.capture_launcher.take().is_some() {
            ctx.send_viewport_cmd_to(viewport_id(), ViewportCommand::Visible(false));
            ctx.request_repaint();
        }
    }
    pub(super) fn toggle_capture_launcher(&mut self, ctx: &egui::Context) {
        if self.capture_launcher.is_some() {
            self.close_capture_launcher(ctx);
            return;
        }
        if self.selection.is_some()
            || self.scroll_session.is_some()
            || self.pending_capture.is_some()
            || self.screen_text.open
        {
            return;
        }
        if let Some(desktop) = &self.desktop {
            desktop.reveal_application();
        }
        self.capture_launcher = Some(Launcher {
            position: position(
                &self.displays,
                self.desktop.as_ref().and_then(|d| d.tray_bounds()),
                ctx.zoom_factor(),
            ),
            focused: false,
            request_focus: true,
        });
        ctx.request_repaint();
    }
    pub(super) fn capture_launcher_ui(&mut self, ctx: &egui::Context) {
        let Some(launcher) = &mut self.capture_launcher else {
            return;
        };
        let mut action = None;
        let mut changed = false;
        let enabled = !self.displays.is_empty()
            && !self.busy
            && self.selection.is_none()
            && self.pending_capture.is_none()
            && self.scroll_session.is_none()
            && !self.recovery.prompt
            && !self.guide_open
            && !self.about_open
            && !self.appearance_open
            && self.pending_action.is_none()
            && self.file_dialog.is_none()
            && !self.export_panel.open
            && !self.editor.has_modal();
        let enabled = enabled && !self.screen_text.open;
        let builder = egui::ViewportBuilder::default()
            .with_title("MyImage Capture Now")
            .with_inner_size(SIZE)
            .with_position(launcher.position)
            .with_resizable(false)
            .with_decorations(false)
            .with_transparent(true)
            .with_taskbar(false)
            .with_has_shadow(true)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_active(true);
        ctx.show_viewport_immediate(viewport_id(), builder, |child, class| {
            if launcher.request_focus {
                child.send_viewport_cmd(ViewportCommand::Focus);
                launcher.request_focus = false;
            }
            let focused = child.input(|i| i.viewport().focused.unwrap_or(i.focused));
            let escape = child.input(|i| i.key_pressed(egui::Key::Escape));
            if child.input(|i| i.viewport().close_requested())
                || (!egui::Popup::is_any_open(child) && (escape || (launcher.focused && !focused)))
            {
                action = Some(Action::Close);
            }
            launcher.focused |= focused;
            let mut content = |ui: &mut egui::Ui| {
                let result = launcher_content(ui, &mut self.settings, &self.displays, enabled);
                changed |= result.0;
                if result.1.is_some() {
                    action = result.1;
                }
            };
            if class == egui::ViewportClass::Embedded {
                egui::Window::new("Capture Now")
                    .resizable(false)
                    .show(child, &mut content);
            } else {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(child, &mut content);
            }
            crate::pointer_style::paint(child, self.settings.pointer_style);
        });
        if changed {
            self.persist_settings();
        }
        if let Some(action) = action {
            self.close_capture_launcher(ctx);
            match action {
                Action::Capture => self.begin_capture(ctx),
                Action::Close => {}
                Action::Editor | Action::Library | Action::Settings => {
                    let event = match action {
                        Action::Editor => DesktopEvent::Show,
                        Action::Library => DesktopEvent::Library,
                        _ => DesktopEvent::CaptureSettings,
                    };
                    let _ = self.desktop_tx.send(event);
                    ctx.request_repaint();
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Action {
    Capture,
    Editor,
    Library,
    Settings,
    Close,
}

fn switch(ui: &mut egui::Ui, rect: Rect, value: &mut bool, label: &str, help: &str) -> bool {
    let mut response = ui.interact(rect, ui.id().with(label), egui::Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *value, label)
    });
    let track = Rect::from_center_size(
        rect.right_center() - Vec2::new(16., 0.),
        Vec2::new(32., 18.),
    );
    let palette = pearl_palette();
    ui.painter().rect_filled(
        track,
        10,
        if *value {
            pearl_palette().accent
        } else {
            Color32::from_rgb(91, 104, 123)
        },
    );
    let center = egui::pos2(
        if *value {
            track.right() - 9.
        } else {
            track.left() + 9.
        },
        track.center().y,
    );
    ui.painter().circle_filled(
        center + Vec2::new(0., 1.),
        7.5,
        Color32::from_black_alpha(22),
    );
    ui.painter().circle_filled(center, 7., Color32::WHITE);
    ui.painter()
        .circle_stroke(center, 7., Stroke::new(0.5, palette.paper));
    ui.painter().text(
        rect.left_center(),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(12.),
        pearl_palette().ink,
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            4,
            Stroke::new(1., pearl_palette().accent),
            egui::StrokeKind::Inside,
        );
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(help)
        .changed()
}

/// Quiet, borderless controls on a shared segmented surface.
fn pearl_button(
    ui: &mut egui::Ui,
    rect: Rect,
    label: &str,
    icon: Option<Icon>,
    selected: bool,
) -> egui::Response {
    let response = ui.interact(rect, ui.id().with(label), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    let p = pearl_palette();
    if selected {
        ui.painter()
            .rect_filled(rect, 7, if icon.is_some() { p.base } else { p.cream });
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 7, p.cream);
    }
    let color = if selected { p.ginger } else { p.muted };
    let font = egui::FontId::proportional(12.);
    let text = ui.painter().layout_no_wrap(label.into(), font, color);
    let width = text.size().x + if icon.is_some() { 22. } else { 0. };
    let left = rect.center().x - width * 0.5;
    if let Some(icon) = icon {
        appearance::icon(
            ui.painter(),
            egui::pos2(left + 7., rect.center().y),
            icon,
            color,
            14.,
        );
    }
    ui.painter().galley(
        egui::pos2(
            left + if icon.is_some() { 22. } else { 0. },
            rect.center().y - text.size().y * 0.5,
        ),
        text,
        color,
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            7,
            Stroke::new(1.5, p.accent),
            egui::StrokeKind::Inside,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn pearl_icon_button(ui: &mut egui::Ui, rect: Rect, label: &str, icon: Icon) -> egui::Response {
    let response = ui.interact(rect, ui.id().with(label), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    let p = pearl_palette();
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, 7, p.cream);
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            7,
            Stroke::new(1.5, p.accent),
            egui::StrokeKind::Inside,
        );
    }
    appearance::icon(ui.painter(), rect.center(), icon, p.muted, 15.);
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label)
}

fn launcher_content(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    displays: &[DisplayInfo],
    enabled: bool,
) -> (bool, Option<Action>) {
    let mut changed = false;
    let mut action = None;
    let p = pearl_palette();
    let (outer, _) = ui.allocate_exact_size(SIZE, egui::Sense::hover());
    let bounds = Rect::from_min_max(outer.min + Vec2::new(0., NOTCH), outer.max);
    let at = |x, y, w, h| Rect::from_min_size(bounds.min + Vec2::new(x, y), Vec2::new(w, h));
    ui.painter().rect(
        bounds.shrink(0.5),
        20,
        p.paper,
        Stroke::new(1., p.border),
        egui::StrokeKind::Inside,
    );
    let tip = outer.center_top();
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            tip + Vec2::new(-NOTCH, NOTCH + 1.),
            tip,
            tip + Vec2::new(NOTCH, NOTCH + 1.),
        ],
        p.paper,
        Stroke::NONE,
    ));
    ui.painter().line(
        vec![
            tip + Vec2::new(-NOTCH, NOTCH),
            tip,
            tip + Vec2::new(NOTCH, NOTCH),
        ],
        Stroke::new(1., p.border),
    );
    // Scope the dropdown and popup colors to this panel; never alter the editor theme.
    let v = ui.visuals_mut();
    *v = egui::Visuals::dark();
    v.override_text_color = Some(p.ink);
    v.weak_text_color = Some(p.muted);
    v.window_fill = p.paper;
    v.panel_fill = p.paper;
    v.extreme_bg_color = p.navy;
    v.selection.bg_fill = p.cream;
    v.selection.stroke = Stroke::new(1., p.ginger);
    for (widget, fill) in [
        (&mut v.widgets.inactive, p.base),
        (&mut v.widgets.hovered, p.cream),
        (&mut v.widgets.active, p.cream),
        (&mut v.widgets.open, p.base),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke = Stroke::new(1., p.border);
        widget.fg_stroke = Stroke::new(1., p.ink);
        widget.corner_radius = egui::CornerRadius::same(8);
    }
    ui.spacing_mut().button_padding = Vec2::new(8., 4.);
    ui.spacing_mut().interact_size.y = 26.;
    ui.spacing_mut().item_spacing = Vec2::new(4., 0.);

    // A quiet aperture mark matches the selected Pearl Compact preview.
    let mark = bounds.min + Vec2::new(28., 22.);
    ui.painter()
        .circle_stroke(mark, 7., Stroke::new(1.2, p.ginger));
    for i in 0..6 {
        let angle = i as f32 * std::f32::consts::TAU / 6.;
        ui.painter().line_segment(
            [
                mark + Vec2::angled(angle) * 7.,
                mark + Vec2::angled(angle + 1.4) * 3.,
            ],
            Stroke::new(1., p.ginger),
        );
    }
    ui.painter().text(
        bounds.min + Vec2::new(48., 22.),
        egui::Align2::LEFT_CENTER,
        "Capture Now",
        egui::FontId::proportional(15.),
        p.ink,
    );
    ui.painter().circle_filled(
        bounds.min + Vec2::new(456., 22.),
        2.5,
        if enabled {
            Color32::from_rgb(54, 156, 113)
        } else {
            p.muted
        },
    );
    ui.painter().text(
        bounds.min + Vec2::new(466., 22.),
        egui::Align2::LEFT_CENTER,
        if enabled { "Ready" } else { "Busy" },
        egui::FontId::proportional(11.),
        p.muted,
    );
    if pearl_icon_button(
        ui,
        at(516., 8., 28., 28.),
        "Close capture panel",
        Icon::Close,
    )
    .clicked()
    {
        action = Some(Action::Close);
    }

    // Capture modes share one recessed track.
    let mode_track = at(14., 44., 532., 56.);
    ui.painter().rect_filled(mode_track, 10, p.navy);
    let mut modes = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("pearl-modes")
            .max_rect(mode_track.shrink(4.))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    modes.spacing_mut().item_spacing.x = 4.;
    for mode in CaptureMode::ALL {
        if crate::capture_panel::compact_mode_tile(
            &mut modes,
            mode,
            settings.capture_mode == mode,
            (524. - 4. * (CaptureMode::ALL.len() - 1) as f32) / CaptureMode::ALL.len() as f32,
            p,
        )
        .clicked()
        {
            changed |= settings.capture_mode != mode;
            settings.capture_mode = mode;
        }
    }

    for (label, y) in [("Display", 123.), ("Send to", 160.)] {
        ui.painter().text(
            bounds.min + Vec2::new(18., y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(12.),
            p.muted,
        );
    }
    let mut display_ui = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("pearl-display")
            .max_rect(at(76., 108., 296., 30.))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    display_ui.spacing_mut().combo_width = 296.;
    display_ui.style_mut().visuals.widgets.inactive.bg_stroke =
        Stroke::new(0.6, p.border.lerp_to_gamma(p.paper, 0.4));
    let name = displays
        .iter()
        .enumerate()
        .find(|(_, d)| Some(d.id) == settings.display_id)
        .map(|(i, d)| crate::capture_panel::display_label(i, d))
        .unwrap_or_else(|| "No display".into());
    let popup_visuals = display_ui.visuals().clone();
    egui::ComboBox::from_id_salt("launcher-display")
        .selected_text(egui::RichText::new(name).size(12.))
        .width(296.)
        .popup_style(egui::style::StyleModifier::new(move |style| {
            style.visuals = popup_visuals.clone()
        }))
        .show_ui(&mut display_ui, |ui| {
            for (i, d) in displays.iter().enumerate() {
                changed |= ui
                    .selectable_value(
                        &mut settings.display_id,
                        Some(d.id),
                        crate::capture_panel::display_label(i, d),
                    )
                    .changed();
            }
        });

    ui.painter().rect(
        at(76., 144., 296., 32.),
        9,
        p.navy,
        Stroke::new(1., p.border),
        egui::StrokeKind::Inside,
    );
    if settings.capture_mode == CaptureMode::Text {
        appearance::icon(
            ui.painter(),
            at(90., 151., 20., 18.).center(),
            Icon::TextSize,
            p.ginger,
            17.,
        );
        ui.painter().text(
            at(116., 147., 250., 26.).left_center(),
            egui::Align2::LEFT_CENTER,
            "Text → clipboard · no image saved",
            egui::FontId::proportional(12.),
            p.ink,
        );
    } else {
        for (i, dest) in CaptureDestination::ALL.into_iter().enumerate() {
            if pearl_button(
                ui,
                at(79. + i as f32 * 146., 147., 144., 26.),
                dest.label(),
                Some(if dest == CaptureDestination::Editor {
                    Icon::Edit
                } else {
                    Icon::Copy
                }),
                settings.capture_destination == dest,
            )
            .on_hover_text(dest.hint())
            .clicked()
            {
                changed |= settings.capture_destination != dest;
                settings.capture_destination = dest;
            }
        }
    }

    changed |= ui
        .add_enabled_ui(settings.capture_mode != CaptureMode::Text, |ui| {
            switch(
                ui,
                at(18., 182., 150., 28.),
                &mut settings.include_pointer,
                "Include pointer",
                "Include the mouse pointer in your capture.",
            )
        })
        .inner;
    changed |= switch(
        ui,
        at(190., 182., 182., 28.),
        &mut settings.capture_on_release,
        "Capture on release",
        "Skip confirmation for single regions and windows. Multi-region capture still waits for Capture or Enter.",
    );

    ui.painter().text(
        bounds.min + Vec2::new(44., 228.),
        egui::Align2::LEFT_CENTER,
        "Delay",
        egui::FontId::proportional(12.),
        p.muted,
    );
    let timer = bounds.min + Vec2::new(26., 228.);
    ui.painter()
        .circle_stroke(timer, 6., Stroke::new(1.1, p.muted));
    ui.painter().line_segment(
        [timer, timer + Vec2::new(2., -3.)],
        Stroke::new(1.1, p.muted),
    );
    ui.painter().line_segment(
        [timer + Vec2::new(-2., -9.), timer + Vec2::new(2., -9.)],
        Stroke::new(1.1, p.muted),
    );
    for (i, secs) in [0, 3, 5, 10].into_iter().enumerate() {
        let label = if secs == 0 {
            "Off".into()
        } else {
            format!("{secs}s")
        };
        if pearl_button(
            ui,
            at(90. + i as f32 * 38., 215., 34., 26.),
            &label,
            None,
            settings.delay_seconds == secs,
        )
        .clicked()
        {
            changed |= settings.delay_seconds != secs;
            settings.delay_seconds = secs;
        }
    }

    ui.painter().line_segment(
        [
            bounds.min + Vec2::new(394., 112.),
            bounds.min + Vec2::new(394., 240.),
        ],
        Stroke::new(0.7, p.border),
    );
    let capture_rect = at(414., 114., 108., 108.);
    let capture = ui
        .add_enabled_ui(enabled, |ui| {
            let response = ui.interact(
                capture_rect,
                ui.id().with("capture-now"),
                egui::Sense::click(),
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    ui.is_enabled(),
                    if settings.capture_mode == CaptureMode::Text {
                        "Select region to copy text"
                    } else {
                        "Capture now"
                    },
                )
            });
            if ui.is_enabled() {
                appearance::satin_with_palette(
                    ui.painter(),
                    capture_rect,
                    54.,
                    true,
                    response.is_pointer_button_down_on(),
                    appearance::Palette {
                        accent: Color32::from_rgb(193, 39, 36),
                        accent_top: Color32::from_rgb(245, 82, 65),
                        ..p
                    },
                );
                ui.painter().rect_stroke(
                    capture_rect,
                    54,
                    Stroke::new(
                        1.,
                        if response.hovered() {
                            Color32::from_rgb(255, 163, 143)
                        } else {
                            Color32::from_rgb(245, 105, 89)
                        },
                    ),
                    egui::StrokeKind::Inside,
                );
            } else {
                ui.painter()
                    .circle_filled(capture_rect.center(), 54., p.base);
            }
            let color = if ui.is_enabled() {
                Color32::WHITE
            } else {
                p.muted
            };
            appearance::icon(
                ui.painter(),
                capture_rect.center() - Vec2::new(0., 15.),
                if settings.capture_mode == CaptureMode::Text {
                    Icon::TextSize
                } else {
                    Icon::Capture
                },
                color,
                26.,
            );
            ui.painter().text(
                capture_rect.center() + Vec2::new(0., 17.),
                egui::Align2::CENTER_CENTER,
                if settings.capture_mode == CaptureMode::Text {
                    "Copy text"
                } else {
                    "Capture"
                },
                egui::FontId::proportional(15.),
                color,
            );
            if response.has_focus() {
                ui.painter().rect_stroke(
                    capture_rect.expand(2.),
                    56,
                    Stroke::new(1.5, p.accent),
                    egui::StrokeKind::Outside,
                );
            }
            response.on_hover_cursor(egui::CursorIcon::PointingHand)
        })
        .inner
        .on_hover_text(if cfg!(target_os = "macos") {
            "Capture now · Control/Command + Shift + ."
        } else {
            "Capture now · Ctrl + Shift + ."
        })
        .on_disabled_hover_text(
            "Open Editor to finish the current task before starting another capture.",
        );
    if capture.clicked() {
        action = Some(Action::Capture);
    }
    ui.painter().text(
        bounds.min + Vec2::new(468., 236.),
        egui::Align2::CENTER_CENTER,
        if cfg!(target_os = "macos") {
            "⌃ ⇧ ."
        } else {
            "Ctrl + Shift + ."
        },
        egui::FontId::proportional(11.),
        p.muted,
    );

    ui.painter().line_segment(
        [
            bounds.min + Vec2::new(1., 248.),
            bounds.min + Vec2::new(559., 248.),
        ],
        Stroke::new(0.7, p.border.lerp_to_gamma(p.paper, 0.35)),
    );
    for (label, icon, next, rect) in [
        (
            "Open Editor",
            Icon::Edit,
            Action::Editor,
            at(14., 253., 112., 26.),
        ),
        (
            "Library",
            Icon::Library,
            Action::Library,
            at(132., 253., 82., 26.),
        ),
    ] {
        if pearl_button(ui, rect, label, Some(icon), false).clicked() {
            action = Some(next);
        }
    }
    if pearl_icon_button(
        ui,
        at(516., 253., 28., 26.),
        "More settings",
        Icon::Settings,
    )
    .clicked()
    {
        action = Some(Action::Settings);
    }
    (changed, action)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launcher_controls_fit_and_update_shared_settings() {
        let ctx = egui::Context::default();
        appearance::apply(&ctx);
        let mut settings = Settings::default();
        let displays = [DisplayInfo {
            id: 1,
            name: "Test".into(),
            x: 0,
            y: 0,
            width: 1470,
            height: 956,
            scale_factor: 2.,
            is_primary: true,
        }];
        settings.display_id = Some(1);
        let mut frame = |events: Vec<egui::Event>, enabled| {
            let mut result = (false, None);
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            result = launcher_content(ui, &mut settings, &displays, enabled);
                        });
                },
            );
            (output, result)
        };
        frame(vec![], true);
        let (output, _) = frame(vec![], true);
        for shape in &output.shapes {
            if let egui::Shape::Text(t) = &shape.shape {
                let rect = Rect::from_min_size(t.pos, t.galley.size());
                assert!(
                    Rect::from_min_size(Pos2::ZERO, SIZE)
                        .expand(1.)
                        .contains_rect(rect),
                    "{} outside panel: {rect:?}",
                    t.galley.job.text
                );
            }
        }
        let center = |output: &egui::FullOutput, label: &str| {
            output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.job.text == label => {
                        Some(Rect::from_min_size(t.pos, t.galley.size()).center())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("Missing {label}"))
        };
        for label in ["Open Editor", "Library"] {
            let _ = center(&output, label);
        }
        // The quiet settings icon keeps its full accessible name and action.
        for (at, expected) in [
            (egui::pos2(530., 270.), Action::Settings),
            (egui::pos2(530., 22.), Action::Close),
        ] {
            let event = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(vec![egui::Event::PointerMoved(at), event(true)], true);
            assert_eq!(frame(vec![event(false)], true).1.1, Some(expected));
        }
        let at = center(&output, "Capture");
        let event = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(at), event(true)], false);
        assert_eq!(frame(vec![event(false)], false).1.1, None);
        for label in [
            "Multi-region",
            "Clipboard",
            "Include pointer",
            "Capture on release",
            "5s",
            "Capture",
        ] {
            let (output, _) = frame(vec![], true);
            let at = center(&output, label);
            let event = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(vec![egui::Event::PointerMoved(at), event(true)], true);
            let (_, result) = frame(vec![event(false)], true);
            if label == "Capture" {
                assert_eq!(result.1, Some(Action::Capture));
            } else {
                assert!(result.0, "{label} did not change");
            }
        }
        assert_eq!(settings.capture_mode, CaptureMode::MultiRegion);
        assert_eq!(settings.capture_destination, CaptureDestination::Clipboard);
        assert!(settings.include_pointer && settings.capture_on_release);
        assert_eq!(settings.delay_seconds, 5);
        assert!(
            !ctx.style().visuals.dark_mode,
            "Launcher styling must not change the editor theme"
        );
        assert_eq!(
            appearance::palette(&ctx).paper,
            appearance::GuiFinish::CobaltSatin.palette().paper
        );
    }

    #[test]
    fn text_capture_is_available_without_image_destination_controls() {
        let ctx = egui::Context::default();
        let mut settings = Settings {
            capture_mode: CaptureMode::Text,
            capture_destination: CaptureDestination::Editor,
            include_pointer: true,
            ..Default::default()
        };
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        launcher_content(ui, &mut settings, &[], true);
                    });
            },
        );
        let labels: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                _ => None,
            })
            .collect();
        assert!(labels.contains(&"Text"));
        assert!(labels.contains(&"Text → clipboard · no image saved"));
        assert!(labels.contains(&"Copy text"));
        assert!(!labels.contains(&"Clipboard"));
        assert_eq!(settings.capture_destination, CaptureDestination::Editor);
        assert!(settings.include_pointer);
    }

    #[test]
    fn anchored_panel_stays_on_display_and_opens_above_bottom_tray() {
        for screen in [
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1470., 956.)),
            Rect::from_min_size(egui::pos2(-1920., -200.), Vec2::new(1920., 1080.)),
        ] {
            for at in [
                screen.left_top(),
                screen.right_top(),
                screen.left_bottom(),
                screen.right_bottom(),
            ] {
                let anchor = Rect::from_center_size(at, Vec2::splat(24.));
                let rect = Rect::from_min_size(panel_position(screen, anchor, SIZE), SIZE);
                assert!(screen.contains_rect(rect));
                if at.y == screen.bottom() {
                    assert!(rect.bottom() < screen.bottom());
                }
            }
        }
    }
}
