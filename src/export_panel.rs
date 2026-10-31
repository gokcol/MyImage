//! Save As options shared by editable projects and finished image copies.
use crate::appearance::Icon as DialogIcon;
use crate::dialogs::{self, Dialog};
use crate::{
    appearance,
    export::{ExportFormat, ExportOptions},
    pdf_export::PdfPageSize,
};
use eframe::egui;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SaveFormat {
    #[default]
    Project,
    Image(ExportFormat),
}

impl SaveFormat {
    pub const ALL: [Self; 8] = [
        Self::Project,
        Self::Image(ExportFormat::Jpeg),
        Self::Image(ExportFormat::Png),
        Self::Image(ExportFormat::Gif),
        Self::Image(ExportFormat::Heif),
        Self::Image(ExportFormat::Tiff),
        Self::Image(ExportFormat::WebP),
        Self::Image(ExportFormat::Pdf),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Project => "MyImage project",
            Self::Image(ExportFormat::Jpeg) => "JPG",
            Self::Image(format) => format.label(),
        }
    }

    pub fn extension(self) -> &'static str {
        self.extensions()[0]
    }

    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Project => &["myimage"],
            Self::Image(format) => format.extensions(),
        }
    }

    fn menu_label(self) -> String {
        format!("{} (.{})", self.label(), self.extension())
    }
}

#[derive(Default)]
pub struct SaveAsPanel {
    pub open: bool,
    pub format: SaveFormat,
    pub options: ExportOptions,
    pub error: Option<String>,
}

impl SaveAsPanel {
    pub fn begin(&mut self) {
        self.format = SaveFormat::Project;
        self.error = None;
        self.open = true;
    }

    /// Returns true when the user is ready to choose a destination in the native picker.
    pub fn show(&mut self, ctx: &egui::Context, size: [u32; 2], has_privacy: bool) -> bool {
        if !self.open {
            return false;
        }
        self.show_with_encoder(ctx, size, has_privacy, crate::heif::available())
    }

    fn show_with_encoder(
        &mut self,
        ctx: &egui::Context,
        size: [u32; 2],
        has_privacy: bool,
        heif_available: bool,
    ) -> bool {
        if !self.open {
            return false;
        }
        let mut choose_destination = false;
        let modal = Dialog::new(
            "Save as",
            "Keep an editable project or save a finished image.",
            DialogIcon::Save,
            420.,
        )
        .modal(ctx, egui::Id::new("save_as_options"), |ui| {
            ui.weak(format!("{} × {} pixels", size[0], size[1]));
            ui.horizontal(|ui| {
                ui.label("File format");
                let formats = egui::ComboBox::from_id_salt("save_as_format")
                    .selected_text(self.format.menu_label())
                    .width(222.)
                    .height(260.)
                    .show_ui(ui, |ui| {
                        // Keep every format reachable without a scrolling menu in
                        // the minimum-sized workspace.
                        ui.spacing_mut().item_spacing.y = 3.;
                        ui.spacing_mut().interact_size.y = 24.;
                        ui.spacing_mut().button_padding = egui::vec2(8., 4.);
                        for format in SaveFormat::ALL {
                            if ui
                                .selectable_value(&mut self.format, format, format.menu_label())
                                .changed()
                            {
                                self.error = None;
                            }
                        }
                    });
                formats.response.widget_info(|| {
                    let mut info = egui::WidgetInfo::labeled(
                        egui::WidgetType::ComboBox,
                        ui.is_enabled(),
                        "File format",
                    );
                    info.current_text_value = Some(self.format.menu_label());
                    info
                });
            });
            ui.add_space(4.);
            match self.format {
                SaveFormat::Project => {
                    ui.strong("Original image + editable layers");
                    ui.label("Continue editing text, shapes and annotations. The original pixels stay together in one .myimage file.");
                    if has_privacy {
                        dialogs::notice(
                            ui,
                            DialogIcon::Info,
                            "Choose an image format for sharing",
                            "Projects retain the pixels behind blur and pixelation. An image copy applies those edits permanently.",
                            false,
                        );
                    }
                }
                SaveFormat::Image(format) => {
                    self.options.format = format;
                    ui.strong("Finished image · includes all annotations");
                    match format {
                        ExportFormat::Jpeg => {
                            ui.label("Compact JPEG with adjustable quality. Transparent areas use the background color.");
                            self.quality(ui);
                            self.background(ui);
                        }
                        ExportFormat::Png => {
                            ui.label("Lossless PNG keeps screenshots and text sharp.");
                            self.transparency(ui, false);
                        }
                        ExportFormat::Gif => {
                            ui.label("A single-frame GIF with up to 256 colors.");
                            self.transparency(ui, true);
                            ui.small("GIF supports fully transparent pixels only. Partly transparent pixels use the background color.");
                        }
                        ExportFormat::Heif => {
                            ui.label("A compact HEIF image with adjustable quality. Transparent areas use the background color.");
                            self.quality(ui);
                            self.background(ui);
                            if !heif_available {
                                dialogs::notice(
                                    ui,
                                    DialogIcon::Info,
                                    "HEIF encoder unavailable",
                                    "This computer does not provide a usable HEIF encoder. Choose another format to save this image.",
                                    true,
                                );
                            }
                        }
                        ExportFormat::Tiff => {
                            ui.label("Lossless TIFF preserves image detail at full resolution.");
                            self.transparency(ui, false);
                        }
                        ExportFormat::WebP => {
                            ui.label("Lossless WebP preserves image detail without compression artifacts.");
                            self.transparency(ui, false);
                        }
                        ExportFormat::Pdf => {
                            ui.label("A single-page PDF containing the finished image at full pixel resolution.");
                            self.transparency(ui, false);
                            ui.horizontal(|ui| {
                                ui.label("Page size");
                                let pages = egui::ComboBox::from_id_salt("save_as_pdf_page")
                                    .selected_text(self.options.pdf_page_size.label())
                                    .show_ui(ui, |ui| {
                                        for page in PdfPageSize::ALL {
                                            if ui
                                                .selectable_value(
                                                    &mut self.options.pdf_page_size,
                                                    page,
                                                    page.label(),
                                                )
                                                .changed()
                                            {
                                                self.error = None;
                                            }
                                        }
                                    });
                                pages.response.widget_info(|| {
                                    let mut info = egui::WidgetInfo::labeled(
                                        egui::WidgetType::ComboBox,
                                        ui.is_enabled(),
                                        "PDF page size",
                                    );
                                    info.current_text_value =
                                        Some(self.options.pdf_page_size.label().to_owned());
                                    info
                                });
                            });
                            ui.small(match self.options.pdf_page_size {
                                PdfPageSize::Image => "Page follows the image proportions at 96 dpi. Very large pages fit within PDF limits.",
                                PdfPageSize::A4 | PdfPageSize::Letter => "Fits the whole image with margins. Page orientation is chosen automatically.",
                            });
                        }
                    }
                    ui.add_space(4.);
                    ui.weak("Your editable .myimage project stays unchanged.");
                }
            }
            if let Some(error) = &self.error {
                dialogs::notice(ui, DialogIcon::Info, "Save needs attention", error, true);
            }
            let available = self.format != SaveFormat::Image(ExportFormat::Heif) || heif_available;
            dialogs::actions(ui, |ui| {
                if dialogs::secondary(ui, "Cancel", DialogIcon::Close).clicked() {
                    self.open = false;
                }
                if dialogs::button(
                    ui,
                    "Save…",
                    DialogIcon::Save,
                    dialogs::Action::Primary,
                    available,
                )
                .clicked()
                {
                    choose_destination = true;
                }
            });
        });
        if modal.should_close() {
            self.open = false;
        }
        choose_destination && self.open
    }

    fn quality(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Quality");
            ui.weak("Default: 90%");
            if ui.small_button("Reset to 90%").clicked() {
                self.options.jpeg_quality = 90;
                self.error = None;
            }
        });
        if appearance::value_slider(
            ui,
            &mut self.options.jpeg_quality,
            1..=100,
            "Image quality",
            "%",
        )
        .changed()
        {
            self.error = None;
        }
    }

    fn transparency(&mut self, ui: &mut egui::Ui, partial_alpha_matte: bool) {
        if ui
            .checkbox(
                &mut self.options.preserve_transparency,
                "Preserve transparency",
            )
            .changed()
        {
            self.error = None;
        }
        if !self.options.preserve_transparency || partial_alpha_matte {
            self.background(ui);
        }
    }

    fn background(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Background");
            if ui
                .color_edit_button_srgb(&mut self.options.jpeg_background)
                .changed()
            {
                self.error = None;
            }
            if ui.small_button("White").clicked() {
                self.options.jpeg_background = [255; 3];
                self.error = None;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Workspace {
        ctx: egui::Context,
        panel: SaveAsPanel,
        heif_available: bool,
        time: f64,
        choose_destination: bool,
    }

    impl Workspace {
        fn new(heif_available: bool) -> Self {
            let ctx = egui::Context::default();
            appearance::apply(&ctx);
            ctx.enable_accesskit();
            let mut panel = SaveAsPanel::default();
            panel.begin();
            Self {
                ctx,
                panel,
                heif_available,
                time: 0.,
                choose_destination: false,
            }
        }

        fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
            self.time += 0.03;
            self.ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(980., 640.),
                    )),
                    time: Some(self.time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |_ui| {});
                    self.choose_destination =
                        self.panel
                            .show_with_encoder(ctx, [1600, 1000], true, self.heif_available);
                },
            )
        }

        fn settle(&mut self) -> egui::FullOutput {
            self.frame(vec![]);
            self.frame(vec![]);
            self.frame(vec![])
        }

        fn click(&mut self, label: &str) {
            let output = self.settle();
            let at = output
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .iter()
                .find_map(|(_, node)| {
                    (node.label() == Some(label))
                        .then(|| node.bounds())
                        .flatten()
                })
                .map(|b| egui::pos2(((b.x0 + b.x1) * 0.5) as f32, ((b.y0 + b.y1) * 0.5) as f32))
                .unwrap_or_else(|| panic!("Missing Save As control {label}"));
            for pressed in [true, false] {
                self.frame(vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
            }
        }

        fn labels(&mut self) -> Vec<String> {
            self.settle()
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .iter()
                .filter_map(|(_, node)| node.label().map(str::to_owned))
                .collect()
        }

        fn texts(&mut self) -> Vec<String> {
            self.settle()
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .iter()
                .filter_map(|(_, node)| node.label().or_else(|| node.value()).map(str::to_owned))
                .collect()
        }
    }

    #[test]
    fn all_save_formats_are_available_in_order_and_fit_the_minimum_window() {
        assert_eq!(
            SaveFormat::ALL.map(SaveFormat::extension),
            [
                "myimage", "jpg", "png", "gif", "heif", "tiff", "webp", "pdf"
            ]
        );
        let mut w = Workspace::new(true);
        assert_eq!(w.panel.format, SaveFormat::Project);
        assert_eq!(w.panel.options.jpeg_quality, 90);
        assert!(w.panel.options.preserve_transparency);
        for format in SaveFormat::ALL {
            w.panel.begin();
            w.click("File format");
            w.click(&format.menu_label());
            assert_eq!(w.panel.format, format);
            let output = w.settle();
            let nodes = output.platform_output.accesskit_update.unwrap().nodes;
            let save = nodes
                .iter()
                .find_map(|(_, node)| (node.label() == Some("Save…")).then_some(node))
                .unwrap();
            assert!(!save.is_disabled());
            let bounds = save.bounds().unwrap();
            assert!(bounds.x0 >= 0. && bounds.x1 <= 980. && bounds.y0 >= 0. && bounds.y1 <= 640.);
            assert!(w.panel.open);
            assert!(!w.choose_destination);
        }
        w.click("Save…");
        assert!(w.choose_destination);
    }

    #[test]
    fn format_specific_controls_and_cancel_do_not_leak_incompatible_settings() {
        let mut w = Workspace::new(true);
        for format in SaveFormat::ALL {
            w.panel.begin();
            w.panel.format = format;
            let labels = w.labels();
            let quality = matches!(
                format,
                SaveFormat::Image(ExportFormat::Jpeg | ExportFormat::Heif)
            );
            let transparency = matches!(
                format,
                SaveFormat::Image(
                    ExportFormat::Png
                        | ExportFormat::WebP
                        | ExportFormat::Tiff
                        | ExportFormat::Gif
                        | ExportFormat::Pdf
                )
            );
            assert_eq!(labels.iter().any(|s| s == "Reset to 90%"), quality);
            assert_eq!(
                labels.iter().any(|s| s == "Preserve transparency"),
                transparency
            );
            assert_eq!(
                labels.iter().any(|s| s == "PDF page size"),
                format == SaveFormat::Image(ExportFormat::Pdf)
            );
            assert!(
                !labels
                    .iter()
                    .any(|s| s.to_ascii_lowercase().contains("export"))
            );
            w.click("Cancel");
            assert!(!w.panel.open);
            assert!(!w.choose_destination);
        }
    }

    #[test]
    fn transparency_and_quality_controls_update_options_and_begin_returns_to_project() {
        let mut w = Workspace::new(true);
        w.panel.format = SaveFormat::Image(ExportFormat::Png);
        w.click("Preserve transparency");
        assert!(!w.panel.options.preserve_transparency);
        assert!(w.labels().iter().any(|s| s == "White"));
        w.panel.options.jpeg_background = [10, 20, 30];
        w.click("White");
        assert_eq!(w.panel.options.jpeg_background, [255; 3]);
        w.panel.format = SaveFormat::Image(ExportFormat::Jpeg);
        w.panel.options.jpeg_quality = 25;
        w.click("Reset to 90%");
        assert_eq!(w.panel.options.jpeg_quality, 90);
        w.panel.begin();
        assert_eq!(w.panel.format, SaveFormat::Project);
    }

    #[test]
    fn unavailable_heif_encoder_disables_save_without_changing_format() {
        let mut w = Workspace::new(false);
        w.panel.format = SaveFormat::Image(ExportFormat::Heif);
        let output = w.settle();
        let nodes = output.platform_output.accesskit_update.unwrap().nodes;
        let save = nodes
            .iter()
            .find_map(|(_, node)| (node.label() == Some("Save…")).then_some(node))
            .unwrap();
        assert!(save.is_disabled());
        let bounds = save.bounds().unwrap();
        assert!(bounds.x0 >= 0. && bounds.x1 <= 980. && bounds.y0 >= 0. && bounds.y1 <= 640.);
        assert!(w.texts().iter().any(|s| s == "HEIF encoder unavailable"));
        w.click("Save…");
        assert!(!w.choose_destination);
        assert!(w.panel.open);
        assert_eq!(w.panel.format, SaveFormat::Image(ExportFormat::Heif));
        w.click("File format");
        w.click(&SaveFormat::Image(ExportFormat::Png).menu_label());
        w.click("Save…");
        assert!(w.choose_destination);
    }
}
