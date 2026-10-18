//! Small native first-run guide, also available from the App menu.
use crate::appearance::{self, Icon};
use eframe::egui;

pub const VERSION: u8 = 1;

pub fn show(ctx: &egui::Context, open: &mut bool) -> bool {
    if !*open {
        return false;
    }
    let mut dismissed = false;
    egui::Modal::new(egui::Id::new("quick_start")).show(ctx, |ui| {
        ui.set_width(450.);
        ui.heading("Meet MyImage");
        ui.weak("Capture a moment. Make it clear. Keep it editable.");
        ui.add_space(12.);
        for (icon, title, body) in [
            (Icon::Capture, "1 · Capture", "Use Capture Now or Control + Shift + . to select a region, window or full screen. On Mac, Command + Shift + . works too."),
            (Icon::Edit, "2 · Annotate", "Choose a tool, then draw on the image. Click an object to adjust its style. Drag a selection to move its pixels; drag its handles to resize the boundary."),
            (Icon::Save, "3 · Save & share", "Save a .myimage project to keep every annotation editable. Export makes a finished image or PDF. Autosave protects unfinished work in a separate recovery copy."),
        ] {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(32., 32.), egui::Sense::hover());
                    appearance::icon(ui.painter(), rect.center(), icon, appearance::INK, 23.);
                    ui.vertical(|ui| { ui.strong(title); ui.add(egui::Label::new(body).wrap()); });
                });
            });
            ui.add_space(5.);
        }
        ui.collapsing("Keyboard & mouse shortcuts", |ui| {
            let key = if cfg!(target_os = "macos") { "⌘" } else { "Ctrl" };
            egui::Grid::new("shortcut_reference").spacing(egui::vec2(20., 6.)).show(ui, |ui| {
                for (keys, action) in [
                    (format!("{key} + S"), "Save editable project"),
                    (format!("{key} + Shift + S"), "Save as a new project"),
                    (format!("{key} + C / X / V"), "Copy / cut / paste"),
                    (format!("{key} + A"), "Select the whole image or focused collection"),
                    (format!("{key} + Z / Shift + Z"), "Undo / redo"),
                    ("Escape".into(), "Cancel the current selection or operation"),
                    ("Pinch / Ctrl + scroll".into(), "Zoom around the pointer"),
                    ("Ctrl / ⌘ + click".into(), "Select multiple Library or Recents items"),
                    ("Right-click image".into(), "Image and canvas actions"),
                ] { ui.strong(keys); ui.label(action); ui.end_row(); }
            });
        });
        ui.add_space(10.);
        ui.horizontal(|ui| {
            if ui.add(appearance::primary("Let's go")).clicked() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                dismissed = true;
            }
            ui.weak("Reopen this guide from the calico App menu.");
        });
    });
    if dismissed {
        *open = false;
    }
    dismissed
}
