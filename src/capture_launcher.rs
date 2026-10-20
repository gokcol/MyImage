//! Resident capture launcher: a native child window, independent of the editor.
use super::*;
use crate::capture::{CaptureDestination, CaptureMode};

const SIZE: Vec2 = Vec2::new(560., 300.);
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
            && self.pending_action.is_none()
            && self.file_dialog.is_none()
            && !self.export_panel.open
            && !self.editor.has_modal();
        let builder = egui::ViewportBuilder::default()
            .with_title("MyImage Capture Now")
            .with_inner_size(SIZE)
            .with_position(launcher.position)
            .with_resizable(false)
            .with_decorations(false)
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
                let result = launcher_content(
                    ui,
                    &mut self.settings,
                    &self.displays,
                    self.app_icon.id(),
                    enabled,
                );
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
                    .frame(egui::Frame::NONE.fill(appearance::PAPER))
                    .show(child, &mut content);
            }
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

fn switch(ui: &mut egui::Ui, value: &mut bool, label: &str, help: &str) -> bool {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(185., 24.), egui::Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *value, label)
    });
    let track = Rect::from_center_size(
        rect.right_center() - Vec2::new(19., 0.),
        Vec2::new(36., 20.),
    );
    ui.painter().rect_filled(
        track,
        10,
        if *value {
            appearance::ACCENT
        } else {
            Color32::from_rgb(177, 178, 180)
        },
    );
    let center = egui::pos2(
        if *value {
            track.right() - 10.
        } else {
            track.left() + 10.
        },
        track.center().y,
    );
    ui.painter().circle_filled(center, 8., Color32::WHITE);
    ui.painter().text(
        rect.left_center(),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.),
        appearance::INK,
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            4,
            Stroke::new(1., appearance::ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    response.on_hover_text(help).changed()
}

fn launcher_content(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    displays: &[DisplayInfo],
    cat: egui::TextureId,
    enabled: bool,
) -> (bool, Option<Action>) {
    let mut changed = false;
    let mut action = None;
    ui.spacing_mut().item_spacing = Vec2::new(8., 4.);
    ui.spacing_mut().button_padding = Vec2::new(8., 3.);
    ui.spacing_mut().interact_size.y = 24.;
    let (header, _) = ui.allocate_exact_size(Vec2::new(SIZE.x, 36.), egui::Sense::hover());
    ui.painter().rect_filled(header, 0, appearance::NAVY);
    let cat_rect =
        Rect::from_center_size(header.left_center() + Vec2::new(27., 0.), Vec2::splat(26.));
    ui.painter().image(
        cat,
        cat_rect,
        Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
        Color32::WHITE,
    );
    ui.painter().text(
        header.left_center() + Vec2::new(50., 0.),
        egui::Align2::LEFT_CENTER,
        "Capture Now",
        egui::FontId::proportional(16.),
        appearance::CREAM,
    );
    ui.painter().text(
        header.right_center() - Vec2::new(50., 0.),
        egui::Align2::RIGHT_CENTER,
        "READY WHEN YOU ARE",
        egui::FontId::proportional(10.),
        appearance::GINGER,
    );
    let close_rect =
        Rect::from_center_size(header.right_center() - Vec2::new(26., 0.), Vec2::splat(26.));
    let close = ui.interact(
        close_rect,
        ui.id().with("close-launcher"),
        egui::Sense::click(),
    );
    close.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Close capture panel")
    });
    appearance::icon(
        ui.painter(),
        close_rect.center(),
        Icon::Close,
        appearance::CREAM,
        15.,
    );
    if close.clicked() {
        action = Some(Action::Close);
    }
    egui::Frame::new().inner_margin(10).show(ui,|ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(394.);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x=6.;
                    for mode in CaptureMode::ALL {
                        if crate::capture_panel::compact_mode_tile(ui,mode,settings.capture_mode==mode,74.).clicked() {
                            changed |= settings.capture_mode != mode;
                            settings.capture_mode=mode;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Display");
                    let name=displays.iter().enumerate().find(|(_,d)|Some(d.id)==settings.display_id)
                        .map(|(i,d)|crate::capture_panel::display_label(i,d)).unwrap_or_else(||"No display".into());
                    egui::ComboBox::from_id_salt("launcher-display").selected_text(name).width(170.).show_ui(ui,|ui| {
                        for (i,d) in displays.iter().enumerate() {
                            changed |= ui.selectable_value(&mut settings.display_id,Some(d.id),crate::capture_panel::display_label(i,d)).changed();
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Send to");
                    for dest in CaptureDestination::ALL {
                        if appearance::action_button(ui,dest.label(),if dest==CaptureDestination::Editor {Icon::Edit} else {Icon::Copy},true,settings.capture_destination==dest).on_hover_text(dest.hint()).clicked() {
                            changed |= settings.capture_destination != dest;
                            settings.capture_destination=dest;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    changed |= switch(ui,&mut settings.include_pointer,"Include pointer","Include the mouse pointer in your capture.");
                    ui.add_space(10.);
                    changed |= switch(ui,&mut settings.capture_on_release,"Capture on release","Skip confirmation for single regions and windows. Multi-region capture still waits for Capture or Enter.");
                });
                ui.horizontal(|ui| {
                    ui.label("Delay");
                    for secs in [0,3,5,10] {
                        changed |= ui.selectable_value(&mut settings.delay_seconds,secs,if secs==0 {"Off".into()} else {format!("{secs}s")}).changed();
                    }
                });
            });
            ui.add_space(6.);
            ui.vertical_centered(|ui| {
                ui.set_width(116.);
                ui.add_space(24.);
                let r = ui.add_enabled_ui(enabled, |ui| {
                    let (rect,response) = ui.allocate_exact_size(Vec2::splat(100.),egui::Sense::click());
                    response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,ui.is_enabled(),"Capture now"));
                    let c=rect.center();
                    ui.painter().circle_filled(c+Vec2::new(0.,3.),49.,Color32::from_black_alpha(30));
                    ui.painter().circle_filled(c,48.,if !ui.is_enabled() {appearance::BORDER} else if response.hovered() {appearance::GINGER} else {appearance::ACCENT});
                    ui.painter().circle_stroke(c,47.,Stroke::new(3.,appearance::GINGER));
                    appearance::icon(ui.painter(),c-Vec2::new(0.,17.),Icon::Capture,Color32::WHITE,24.);
                    ui.painter().text(c+Vec2::new(0.,17.),egui::Align2::CENTER_CENTER,"Capture",egui::FontId::proportional(16.),Color32::WHITE);
                    if response.has_focus() {ui.painter().circle_stroke(c,50.,Stroke::new(2.,appearance::INK));}
                    response
                }).inner;
                if r.clicked() {action=Some(Action::Capture);}
                ui.add_space(6.);
                ui.label(egui::RichText::new(if cfg!(target_os="macos") {"⌃⇧.  /  ⌘⇧."} else {"Ctrl + Shift + ."}).small().color(appearance::MUTED));
                if !enabled {ui.label(egui::RichText::new("Open Editor to finish the current task").small().color(appearance::MUTED));}
            });
        });
    });
    ui.separator();
    ui.horizontal(|ui| {
        ui.add_space(12.);
        for (label, icon, next) in [
            ("Open Editor", Icon::Edit, Action::Editor),
            ("Library", Icon::Library, Action::Library),
        ] {
            if appearance::action_button(ui, label, icon, true, false).clicked() {
                action = Some(next);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(12.);
            if appearance::action_button(ui, "More settings", Icon::Settings, true, false).clicked()
            {
                action = Some(Action::Settings);
            }
        });
    });
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
                            result = launcher_content(
                                ui,
                                &mut settings,
                                &displays,
                                egui::TextureId::Managed(0),
                                enabled,
                            );
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
        for label in ["Open Editor", "Library", "More settings"] {
            let _ = center(&output, label);
        }
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
