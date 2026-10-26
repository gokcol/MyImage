//! Capture home: visual targets, compact options and the resident Capture Now entry point.
use crate::{
    appearance,
    capture::{CaptureDestination, CaptureMode, DisplayInfo},
    selection::CaptureShape,
    storage::Settings,
};
use eframe::egui::{self, Color32, Rect, Stroke, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Capture,
    Panel,
    RefreshDisplays,
    ChooseFolder,
    Library,
    Background,
    Permission,
}
#[derive(Default)]
pub struct Response {
    pub changed: bool,
    pub action: Option<Action>,
}
pub struct Availability {
    pub capture: bool,
    pub background: bool,
    pub permission_needed: bool,
    pub demo: bool,
}
pub fn display_label(index: usize, display: &DisplayInfo) -> String {
    format!(
        "Display {}{}",
        index + 1,
        if display.is_primary { " · Main" } else { "" }
    )
}
pub fn delay_label(seconds: u32) -> String {
    match seconds {
        0 => "No delay".into(),
        1 => "1 second".into(),
        n => format!("{n} seconds"),
    }
}
pub fn menu_summary(settings: &Settings, _shape: CaptureShape, displays: &[DisplayInfo]) -> String {
    let display = displays
        .iter()
        .position(|d| Some(d.id) == settings.display_id)
        .map(|i| format!("Display {}", i + 1))
        .unwrap_or_else(|| "No display".into());
    format!(
        "{} · {display} · {} · {}",
        settings.capture_mode.label(),
        delay_label(settings.delay_seconds),
        settings.capture_destination.label(),
    )
}
fn card(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .corner_radius(8)
        .inner_margin(14)
        .stroke(Stroke::new(1., appearance::palette(ctx).border))
}

pub fn ui(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    _shape: &mut CaptureShape,
    displays: &[DisplayInfo],
    available: Availability,
    cat: Option<egui::TextureId>,
) -> Response {
    ui.spacing_mut().button_padding = Vec2::new(9., 5.);
    ui.spacing_mut().interact_size.y = 28.;
    let mut result = Response::default();
    egui::ScrollArea::vertical().id_salt("capture-console-scroll").show(ui, |ui| {
        let width = (ui.available_width() - 24.).clamp(560., 840.);
        let offset = ((ui.available_width() - width) * 0.5).max(0.);
        ui.add_space(10.);
        ui.horizontal(|ui| {
            ui.add_space(offset);
            ui.vertical(|ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing = Vec2::new(10., 10.);
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(38.), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 8, appearance::palette(ui.ctx()).navy);
                    if let Some(cat) = cat {
                        ui.painter().image(cat, rect.shrink(2.), Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)), Color32::WHITE);
                    } else { appearance::icon(ui.painter(), rect.center(), appearance::Icon::Capture, appearance::palette(ui.ctx()).ginger, 23.); }
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Capture your screen").size(20.).strong());
                        ui.label(egui::RichText::new("Your screen, ready to explain.").color(appearance::palette(ui.ctx()).muted));
                    });
                });
                ui.horizontal(|ui| {
                    let tile_width = (width - 10. * (CaptureMode::ALL.len() - 1) as f32) / CaptureMode::ALL.len() as f32;
                    for mode in CaptureMode::ALL {
                        if mode_tile(ui, mode, settings.capture_mode == mode, tile_width).clicked() {
                            result.changed |= settings.capture_mode != mode;
                            settings.capture_mode = mode;
                        }
                    }
                });
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(width - 264.);
                        card(ui.ctx()).show(ui, |ui| {
                            ui.set_width(width - 292.);
                            ui.spacing_mut().item_spacing.y = 10.;
                            ui.horizontal(|ui| {
                                ui.strong("Capture options");
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.small_button("Refresh").on_hover_text("Find connected displays").clicked() { result.action = Some(Action::RefreshDisplays); }
                                });
                            });
                            let label = displays.iter().enumerate().find(|(_,d)| Some(d.id) == settings.display_id)
                                .map(|(i,d)| display_label(i,d)).unwrap_or_else(|| "No display available".into());
                            egui::ComboBox::from_id_salt("capture-display").selected_text(label).width(ui.available_width()).show_ui(ui, |ui| {
                                for (i,d) in displays.iter().enumerate() {
                                    result.changed |= ui.selectable_value(&mut settings.display_id, Some(d.id), display_label(i,d))
                                        .on_hover_text(format!("{} · {} × {} · {:.0}%", d.name, d.width, d.height, d.scale_factor * 100.)).changed();
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Timer");
                                ui.spacing_mut().item_spacing.x = 4.;
                                for seconds in [0, 3, 5, 10] {
                                    result.changed |= ui.selectable_value(&mut settings.delay_seconds, seconds,
                                        if seconds == 0 { "Off".into() } else { format!("{seconds}s") }).changed();
                                }
                            });
                            result.changed |= ui.add_enabled_ui(settings.capture_mode != CaptureMode::Text, |ui| ui.checkbox(&mut settings.include_pointer, "Include pointer")
                                .on_hover_text("Show the mouse pointer in the saved capture. Turn off for a clean image.").changed()).inner;
                            result.changed |= ui.checkbox(&mut settings.capture_on_release, "Capture immediately after selection")
                                .on_hover_text("Off by default: adjust the selection, then click Capture. When enabled, releasing a region or clicking a window captures immediately. Multiple regions still wait for Capture or Enter.").changed();
                            ui.strong("After capture");
                            if settings.capture_mode == CaptureMode::Text {
                                ui.label("Copy text to clipboard · no image saved");
                            } else {
                            ui.horizontal(|ui| {
                                for destination in CaptureDestination::ALL {
                                    if appearance::action_button(
                                        ui, destination.label(),
                                        if destination == CaptureDestination::Editor { appearance::Icon::Edit } else { appearance::Icon::Copy },
                                        true, settings.capture_destination == destination,
                                    ).on_hover_text(destination.hint()).clicked() {
                                        result.changed |= settings.capture_destination != destination;
                                        settings.capture_destination = destination;
                                    }
                                }
                            });
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.strong("Save to library");
                                if ui.small_button("Change…").clicked() { result.action = Some(Action::ChooseFolder); }
                                if ui.small_button("Browse").clicked() { result.action = Some(Action::Library); }
                            });
                            ui.add(egui::Label::new(egui::RichText::new(settings.output_dir.display().to_string()).small().color(appearance::palette(ui.ctx()).muted)).truncate())
                                .on_hover_text(settings.output_dir.display().to_string());
                            ui.small("PNG · Organized by year / month / day");
                            }
                        });
                    });
                    egui::Frame::new().fill(appearance::palette(ui.ctx()).navy).corner_radius(8).inner_margin(16).show(ui, |ui| {
                        ui.vertical(|ui| {
                        ui.set_width(222.);
                        ui.spacing_mut().item_spacing.y = 8.;
                        ui.label(egui::RichText::new("CAPTURE NOW").size(11.).strong().color(appearance::palette(ui.ctx()).ginger));
                        ui.label(egui::RichText::new(settings.capture_mode.label()).size(19.).strong().color(Color32::WHITE));
                        ui.add(egui::Label::new(egui::RichText::new(settings.capture_mode.hint()).size(13.).color(Color32::from_rgb(224, 213, 197))).wrap());
                        if ui.add_enabled(available.capture, egui::Button::new(egui::RichText::new("Capture Now").strong().color(appearance::palette(ui.ctx()).ink)).fill(appearance::palette(ui.ctx()).ginger).min_size(Vec2::new(222., 36.))).clicked() {
                            result.action = Some(Action::Capture);
                        }
                        ui.label(egui::RichText::new("Ctrl + Shift + C").strong().color(Color32::WHITE));
                        ui.label(egui::RichText::new(if cfg!(target_os="macos") { "Also ⌃⇧. / ⌘⇧." } else { "Also Ctrl + Shift + ." }).size(12.).color(Color32::from_rgb(224, 213, 197)));
                        ui.separator();
                        ui.label(egui::RichText::new(if settings.capture_mode == CaptureMode::Text { "Text is copied directly. Your editor and Library stay unchanged." } else { settings.capture_destination.hint() }).size(12.).color(Color32::from_rgb(224, 213, 197)));
                        });
                    });
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(if cfg!(target_os="macos") { "Built-in Capture Now is always in your menu bar." } else { "Built-in Capture Now is always in your system tray." }).size(12.).color(appearance::palette(ui.ctx()).muted));
                    if appearance::action_button(ui, "Capture panel", appearance::Icon::Tray, true, false).clicked() { result.action = Some(Action::Panel); }
                    if ui.add_enabled(available.background, egui::Button::new("Keep ready in background").small().frame(false)).clicked() { result.action = Some(Action::Background); }
                });
                if available.permission_needed {
                    ui.horizontal(|ui| {
                        ui.small("Allow screen recording to capture other apps.");
                        if ui.small_button("Allow screen capture…").clicked() { result.action = Some(Action::Permission); }
                    });
                }
                if available.demo { ui.small("Demo workspace · generated test image"); }
            });
        });
        ui.add_space(16.);
    });
    result
}

pub fn destination_picker(ui: &mut egui::Ui, destination: &mut CaptureDestination) -> bool {
    let before = *destination;
    egui::ComboBox::from_id_salt("capture-destination")
        .selected_text(format!("To: {}", destination.label()))
        .width(118.)
        .show_ui(ui, |ui| {
            for choice in CaptureDestination::ALL {
                if ui
                    .selectable_value(destination, choice, choice.label())
                    .on_hover_text(choice.hint())
                    .clicked()
                {
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text(destination.hint());
    *destination != before
}

pub(super) fn mode_icon(painter: &egui::Painter, mode: CaptureMode, rect: Rect, color: Color32) {
    let color = crate::icon_style::color(
        painter.ctx(),
        color,
        match mode {
            CaptureMode::MultiRegion => crate::icon_style::Family::Violet,
            CaptureMode::Scrolling => crate::icon_style::Family::Teal,
            _ => crate::icon_style::Family::Cobalt,
        },
    );
    let stroke = Stroke::new(crate::icon_style::current(painter.ctx()).width(), color);
    match mode {
        CaptureMode::Text => appearance::icon(
            painter,
            rect.center(),
            appearance::Icon::TextSize,
            color,
            rect.height(),
        ),
        CaptureMode::Region => appearance::icon(
            painter,
            rect.center(),
            appearance::Icon::Fit,
            color,
            rect.height(),
        ),
        CaptureMode::MultiRegion => {
            for offset in [Vec2::ZERO, Vec2::splat(10.)] {
                painter.rect_stroke(
                    Rect::from_min_size(rect.min + offset, rect.size() * 0.65),
                    2,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
        CaptureMode::Window => {
            painter.rect_stroke(rect, 3, stroke, egui::StrokeKind::Inside);
            painter.line_segment(
                [
                    rect.min + Vec2::new(0., 7.),
                    rect.right_top() + Vec2::new(0., 7.),
                ],
                stroke,
            );
            for x in [5., 10.] {
                painter.circle_filled(rect.min + Vec2::new(x, 3.5), 1., color);
            }
        }
        CaptureMode::FullScreen => {
            painter.rect_stroke(
                rect.shrink2(Vec2::new(0., 3.)),
                2,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    rect.center_bottom() + Vec2::new(-8., 1.),
                    rect.center_bottom() + Vec2::new(8., 1.),
                ],
                stroke,
            );
        }
        CaptureMode::Scrolling => {
            painter.rect_stroke(rect, 2, stroke, egui::StrokeKind::Inside);
            let c = rect.center();
            painter.line_segment([c - Vec2::new(0., 8.), c + Vec2::new(0., 8.)], stroke);
            painter.line_segment([c + Vec2::new(-4., 4.), c + Vec2::new(0., 8.)], stroke);
            painter.line_segment([c + Vec2::new(4., 4.), c + Vec2::new(0., 8.)], stroke);
        }
    }
}
pub(super) fn mode_tile(
    ui: &mut egui::Ui,
    mode: CaptureMode,
    selected: bool,
    width: f32,
) -> egui::Response {
    sized_mode_tile(
        ui,
        mode,
        selected,
        Vec2::new(width, 64.),
        appearance::palette(ui.ctx()),
    )
}

pub(super) fn compact_mode_tile(
    ui: &mut egui::Ui,
    mode: CaptureMode,
    selected: bool,
    width: f32,
    palette: appearance::Palette,
) -> egui::Response {
    sized_mode_tile(ui, mode, selected, Vec2::new(width, 48.), palette)
}

fn sized_mode_tile(
    ui: &mut egui::Ui,
    mode: CaptureMode,
    selected: bool,
    size: Vec2,
    palette: appearance::Palette,
) -> egui::Response {
    let compact = size.y < 64.;
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            mode.label(),
        )
    });
    if compact {
        let p = palette;
        if selected {
            ui.painter().rect_filled(rect, 8, p.base);
        } else if response.hovered() {
            ui.painter().rect_filled(rect, 8, p.cream);
        }
        let color = if selected { p.ginger } else { p.muted };
        mode_icon(
            ui.painter(),
            mode,
            Rect::from_center_size(rect.center_top() + Vec2::new(0., 15.), Vec2::new(21., 17.)),
            color,
        );
        ui.painter().text(
            rect.center_bottom() - Vec2::new(0., 10.),
            egui::Align2::CENTER_CENTER,
            mode.label(),
            egui::FontId::proportional(12.),
            color,
        );
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                8,
                Stroke::new(1.5, p.accent),
                egui::StrokeKind::Inside,
            );
        }
        return response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(mode.hint());
    }
    let color = if selected {
        appearance::palette(ui.ctx()).cream
    } else {
        appearance::palette(ui.ctx()).ink
    };
    ui.painter().rect_filled(
        rect,
        10,
        if selected {
            appearance::palette(ui.ctx()).navy
        } else {
            Color32::WHITE
        },
    );
    ui.painter().rect_stroke(
        rect,
        10,
        Stroke::new(
            if selected || response.has_focus() {
                2.
            } else {
                1.
            },
            if selected || response.hovered() {
                appearance::palette(ui.ctx()).ginger
            } else {
                appearance::palette(ui.ctx()).border
            },
        ),
        egui::StrokeKind::Inside,
    );
    if selected {
        appearance::satin(
            ui.painter(),
            rect,
            10.,
            true,
            response.is_pointer_button_down_on(),
        );
    }
    mode_icon(
        ui.painter(),
        mode,
        Rect::from_center_size(
            rect.center_top() + Vec2::new(0., if compact { 17. } else { 21. }),
            Vec2::new(25., 20.),
        ),
        color,
    );
    ui.painter().text(
        rect.center_bottom() - Vec2::new(0., 13.),
        egui::Align2::CENTER_CENTER,
        mode.label(),
        egui::FontId::proportional(if compact { 12. } else { 13. }),
        color,
    );
    if !selected && response.hovered() {
        ui.painter().rect_stroke(
            rect,
            10,
            Stroke::new(1.5, appearance::palette(ui.ctx()).accent),
            egui::StrokeKind::Inside,
        );
    }
    response.on_hover_text(mode.hint())
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Pos2;
    #[test]
    fn capture_modes_and_primary_action_are_visible_and_interactive() {
        let ctx = egui::Context::default();
        appearance::apply(&ctx);
        let mut settings = Settings::default();
        let mut shape = CaptureShape::Rectangle;
        let mut frame = |events| {
            let mut response = Response::default();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(940., 480.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        response = super::ui(
                            ui,
                            &mut settings,
                            &mut shape,
                            &[],
                            Availability {
                                capture: true,
                                background: true,
                                permission_needed: false,
                                demo: false,
                            },
                            None,
                        );
                    });
                },
            );
            (output, response)
        };
        frame(vec![]);
        let (output, _) = frame(vec![]);
        for label in [
            "Region",
            "Window",
            "Multi-region",
            "Full screen",
            "Scrolling",
            "Include pointer",
            "Editor",
            "Clipboard",
            "Capture Now",
        ] {
            let t = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.job.text == label => Some((s.clip_rect, t)),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("Missing {label}"));
            assert!(
                t.0.contains_rect(Rect::from_min_size(t.1.pos, t.1.galley.size())),
                "{label} clipped: {:?} within {:?}",
                Rect::from_min_size(t.1.pos, t.1.galley.size()),
                t.0
            );
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
                .unwrap()
        };
        for label in [
            "Window",
            "Multi-region",
            "Full screen",
            "Scrolling",
            "Region",
            "Include pointer",
            "Capture immediately after selection",
            "Clipboard",
            "Editor",
            "Capture Now",
        ] {
            let (output, _) = frame(vec![]);
            let at = center(&output, label);
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                pressed,
                button: egui::PointerButton::Primary,
                modifiers: egui::Modifiers::NONE,
            };
            frame(vec![egui::Event::PointerMoved(at), button(true)]);
            let (_, response) = frame(vec![button(false)]);
            if label == "Capture Now" {
                assert_eq!(response.action, Some(Action::Capture));
            } else {
                assert!(
                    response.changed,
                    "{label} did not change the capture options"
                );
            }
        }
        assert_eq!(settings.capture_mode, CaptureMode::Region);
        assert!(settings.include_pointer);
    }
}
