use crate::appearance::Icon as DialogIcon;
use crate::dialogs::{self, Dialog};
use crate::{
    appearance,
    export::{ExportFormat, ExportOptions},
    pdf_export::PdfPageSize,
};
use eframe::egui::{self, RichText};

#[derive(Default)]
pub struct ExportPanel {
    pub open: bool,
    pub options: ExportOptions,
    pub error: Option<String>,
}

impl ExportPanel {
    pub fn begin(&mut self) {
        self.error = None;
        self.open = true;
    }

    /// Returns true when the user is ready to choose a destination in the native picker.
    pub fn show(&mut self, ctx: &egui::Context, size: [u32; 2], has_privacy: bool) -> bool {
        if !self.open {
            return false;
        }
        let mut choose_destination = false;
        let modal = Dialog::new("Export a copy", "A finished image, ready to share.", DialogIcon::Export, 480.).modal(ctx, egui::Id::new("export_options"), |ui| {
            ui.label(
                RichText::new(format!("{} × {} pixels · Includes all annotations", size[0], size[1]))
                    .color(appearance::MUTED),
            );
            ui.add_space(12.0);
            ui.label(RichText::new("FORMAT").small().strong().color(appearance::MUTED));
            ui.horizontal_wrapped(|ui| {
                for format in ExportFormat::ALL {
                    if ui.selectable_value(&mut self.options.format, format, format.label()).changed() {
                        self.error = None;
                    }
                }
            });
            ui.add_space(10.0);
            match self.options.format {
                ExportFormat::Png => {
                    ui.label("Lossless image with transparency. A good choice for screenshots and sharp text.");
                }
                ExportFormat::Jpeg => {
                    ui.label("Smaller files with adjustable quality. Transparent areas use the background color below.");
                    ui.add_space(8.0);
                    ui.label("Quality");
                    crate::appearance::value_slider(ui, &mut self.options.jpeg_quality, 1..=100, "Quality", "%");
                    ui.horizontal(|ui| {
                        ui.label("Background");
                        ui.color_edit_button_srgb(&mut self.options.jpeg_background);
                        if ui.small_button("White").clicked() {
                            self.options.jpeg_background = [255; 3];
                        }
                    });
                }
                ExportFormat::WebP => {
                    ui.label("Lossless WebP with transparency. Keeps image detail without JPEG compression artifacts.");
                }
                ExportFormat::Tiff => {
                    ui.label("Lossless TIFF with transparency, at the original image resolution.");
                }
                ExportFormat::Pdf => {
                    ui.label("A single-page PDF containing the image and annotations. The image keeps its full pixel resolution.");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label("Page size");
                        egui::ComboBox::from_id_salt("export_pdf_page")
                            .selected_text(self.options.pdf_page_size.label())
                            .show_ui(ui, |ui| {
                                for page in PdfPageSize::ALL {
                                    ui.selectable_value(&mut self.options.pdf_page_size, page, page.label());
                                }
                            });
                    });
                    ui.label(RichText::new(match self.options.pdf_page_size {
                        PdfPageSize::Image => "Page follows the image proportions at 96 dpi; very large pages are scaled to PDF limits.",
                        PdfPageSize::A4 | PdfPageSize::Letter => "Fits the whole image with margins. Portrait or landscape is chosen automatically.",
                    }).small().color(appearance::MUTED));
                }
            }
            ui.add_space(14.0);
            dialogs::notice(ui, DialogIcon::Save, if has_privacy { "Share the exported copy" } else { "Keep an editable project" },
                if has_privacy { "Your .myimage project retains original pixels. Share the exported copy to apply blur and pixelation." } else { "Save a .myimage project to keep all annotations editable. Export makes a flattened copy." }, false);
            if let Some(error) = &self.error {
                ui.add_space(8.0);
                dialogs::notice(ui, DialogIcon::Info, "Export needs attention", error, true);
            }
            ui.add_space(14.0);
            dialogs::actions(ui, |ui| {
                if dialogs::secondary(ui, "Cancel", DialogIcon::Close).clicked() {
                    self.open = false;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if dialogs::primary(ui, "Choose destination…", DialogIcon::Export).clicked() {
                        choose_destination = true;
                    }
                });
            });
        });
        if modal.should_close() {
            self.open = false;
        }
        choose_destination && self.open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_export_format_renders_at_the_minimum_window_size() {
        let ctx = egui::Context::default();
        appearance::apply(&ctx);
        let mut panel = ExportPanel::default();
        for format in ExportFormat::ALL {
            panel.begin();
            panel.options.format = format;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(980.0, 640.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    assert!(!panel.show(ctx, [1600, 1000], true));
                    assert!(panel.open);
                },
            );
        }
    }
}
