//! Calico welcome and compact native shortcut guide.
use crate::appearance::{self, Icon};
use crate::dialogs::{self, Dialog};
use eframe::egui::{self, Color32, RichText};

pub const VERSION: u8 = 2;

pub fn show(ctx: &egui::Context, open: &mut bool, calico: &egui::TextureHandle) -> bool {
    if !*open {
        return false;
    }
    let mut dismissed = false;
    Dialog::new("Welcome to MyImage", "Your compact capture and annotation studio.", Icon::Capture, 510.)
        .modal(ctx, egui::Id::new("quick_start"), |ui| {
            ui.spacing_mut().item_spacing.y = 7.;
            ui.horizontal(|ui| {
                let (badge, _) = ui.allocate_exact_size(egui::vec2(64., 68.), egui::Sense::hover());
                ui.painter().circle_filled(badge.center(), 30., appearance::palette(ui.ctx()).cream);
                ui.painter().image(calico.id(), badge.shrink(3.), egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)), Color32::WHITE);
                appearance::calico_tabs(ui.painter(), badge.left_bottom() + egui::vec2(24., -2.));
                ui.add_space(9.);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Small paws. Big ideas.").size(23.).strong().color(appearance::palette(ui.ctx()).ink));
                    ui.label(RichText::new("Capture it. Make it yours.").color(appearance::palette(ui.ctx()).muted));
                });
            });
            ui.add_space(6.);
            egui::ScrollArea::vertical()
                .id_salt("quick_start_body")
                .max_height((ctx.content_rect().height() - 330.).clamp(110., 400.))
                .show(ui, |ui| {
                    step_card(ui, "01", Icon::Capture, "Catch a moment", "Capture a region, a window, or your whole screen.", "Ctrl + Shift + .  ·  On Mac, ⌘ + Shift + . also works", appearance::palette(ui.ctx()).cream, appearance::palette(ui.ctx()).accent);
                    step_card(ui, "02", Icon::Edit, "Make your mark", "Add arrows, text and stamps. Click an object to style it.", "Drag a selection to move pixels; use its handles to adjust the boundary.", Color32::from_rgb(229, 242, 239), Color32::from_rgb(30, 104, 94));
                    step_card(ui, "03", Icon::Save, "Keep it editable. Share it anywhere.", "Save .myimage for the layers. Export an image or PDF to share.", "Right-click a selection for actions, including Delete.", Color32::from_rgb(239, 234, 227), appearance::palette(ui.ctx()).ink);
                    ui.add_space(3.);
                    egui::Frame::new().fill(appearance::palette(ui.ctx()).cream).corner_radius(7).inner_margin(9).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new("A little peace of mind").strong().color(appearance::palette(ui.ctx()).accent));
                        ui.add(egui::Label::new(RichText::new("Autosave keeps a separate editable recovery copy. If a session ends unexpectedly, choose what to recover next time.").small().color(appearance::palette(ui.ctx()).ink)).wrap());
                    });
                    ui.add_space(4.);
                    shortcuts(ui);
                });
            ui.add_space(7.);
            ui.horizontal_wrapped(|ui| {
                if dialogs::primary(ui, "Let's create", Icon::Edit).clicked() {
                    dismissed = true;
                }
                ui.small(RichText::new("Your calico App menu keeps this guide handy.").color(appearance::palette(ui.ctx()).muted));
            });
            dismissed |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
        });
    if dismissed {
        *open = false;
    }
    dismissed
}

#[allow(clippy::too_many_arguments)]
fn step_card(
    ui: &mut egui::Ui,
    number: &str,
    icon: Icon,
    title: &str,
    body: &str,
    tip: &str,
    tint: Color32,
    accent: Color32,
) {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(egui::Stroke::new(
            1.,
            appearance::palette(ui.ctx()).border.gamma_multiply(0.6),
        ))
        .corner_radius(9)
        .inner_margin(12)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(42., 48.), egui::Sense::hover());
                ui.painter().rect_filled(
                    egui::Rect::from_center_size(
                        rect.center_top() + egui::vec2(0., 17.),
                        egui::vec2(34., 34.),
                    ),
                    8,
                    tint,
                );
                appearance::icon(
                    ui.painter(),
                    rect.center_top() + egui::vec2(0., 17.),
                    icon,
                    accent,
                    23.,
                );
                ui.painter().text(
                    rect.center_bottom(),
                    egui::Align2::CENTER_BOTTOM,
                    number,
                    egui::FontId::proportional(10.),
                    accent,
                );
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(title)
                            .strong()
                            .color(appearance::palette(ui.ctx()).ink),
                    );
                    ui.add(
                        egui::Label::new(
                            RichText::new(body).color(appearance::palette(ui.ctx()).ink),
                        )
                        .wrap(),
                    );
                    ui.add(
                        egui::Label::new(
                            RichText::new(tip)
                                .small()
                                .color(appearance::palette(ui.ctx()).muted),
                        )
                        .wrap(),
                    );
                });
            });
        });
}

fn shortcuts(ui: &mut egui::Ui) {
    ui.collapsing("Shortcuts & little tricks", |ui| {
        let key = if cfg!(target_os = "macos") {
            "⌘"
        } else {
            "Ctrl"
        };
        let column_width = ((ui.available_width() - 16.) / 2.).max(80.);
        egui::Grid::new("shortcut_reference")
            .min_col_width(column_width)
            .max_col_width(column_width)
            .spacing(egui::vec2(16., 4.))
            .show(ui, |ui| {
                for (keys, action) in [
                    (format!("{key} + S"), "Save editable project"),
                    (format!("{key} + Shift + S"), "Save as a new project"),
                    (format!("{key} + C / X / V"), "Copy / cut / paste"),
                    (
                        format!("{key} + A"),
                        "Select all in the focused image or collection",
                    ),
                    (format!("{key} + Z / Shift + Z"), "Undo / redo"),
                    (
                        "Delete / Backspace".into(),
                        "Delete the selected area or layers",
                    ),
                    ("Escape".into(), "Cancel a selection or operation"),
                    ("Pinch / Ctrl + scroll".into(), "Zoom around the pointer"),
                    (
                        "Ctrl / ⌘ + click".into(),
                        "Select several Library or Recents items",
                    ),
                    (
                        "Right-click image".into(),
                        "Selection, image and canvas actions",
                    ),
                ] {
                    ui.label(RichText::new(keys).small().strong());
                    ui.add(egui::Label::new(RichText::new(action).small()).wrap());
                    ui.end_row();
                }
            });
    });
}
