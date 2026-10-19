use crate::appearance::Icon as DialogIcon;
use crate::dialogs::{self, Dialog};
use crate::{stamps::StampSource, svg_stamp};
use eframe::egui::{self, Vec2};
use std::sync::Arc;

pub struct Designer {
    pub target: Option<usize>,
    pub original: Option<StampSource>,
    pub category: String,
    name: String,
    description: String,
    svg: String,
    preview: Option<StampSource>,
    error: Option<String>,
    copied: bool,
}
pub enum Action {
    Save(StampSource),
    Cancel,
}
impl Designer {
    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
    }
    pub fn new(source: Option<StampSource>, target: Option<usize>, category: String) -> Self {
        let (name, svg, description) = match &source {
            Some(StampSource::Svg { name, svg, prompt }) => {
                (name.clone(), svg.to_string(), prompt.clone())
            }
            _ => ("My stamp".into(), svg_stamp::STARTER.into(), String::new()),
        };
        Self {
            target,
            original: source,
            category,
            name,
            svg,
            description,
            preview: None,
            error: None,
            copied: false,
        }
    }
    fn source(&self) -> StampSource {
        StampSource::Svg {
            name: self.name.trim().into(),
            svg: Arc::new(svg_stamp::clean(&self.svg)),
            prompt: self.description.clone(),
        }
    }
    pub fn show(&mut self, ctx: &egui::Context, categories: &[String]) -> Option<Action> {
        let mut action = None;
        let mut open = true;
        Dialog::new(
            if self.target.is_some() {
                "Edit SVG stamp"
            } else {
                "Design a stamp"
            },
            "Describe · Copy prompt · Paste SVG · Preview · Save",
            DialogIcon::Create,
            730.,
        )
        .window(ctx, egui::Id::new("svg-designer"), &mut open, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Name");
                ui.add(
                    egui::TextEdit::singleline(&mut self.name)
                        .char_limit(60)
                        .desired_width(200.),
                );
                ui.label("Category");
                egui::ComboBox::from_id_salt("designer-category")
                    .selected_text(&self.category)
                    .show_ui(ui, |ui| {
                        for category in categories {
                            ui.selectable_value(&mut self.category, category.clone(), category);
                        }
                    });
            });
            egui::ScrollArea::vertical()
                .max_height((ctx.content_rect().height() - 190.).max(240.))
                .show(ui, |ui| {
                    ui.columns(2, |columns| {
                        let ui = &mut columns[0];
                        ui.strong("Your idea");
                        ui.add(
                            egui::TextEdit::multiline(&mut self.description)
                                .desired_rows(3)
                                .desired_width(f32::INFINITY)
                                .char_limit(2000)
                                .hint_text("A teal approval seal with a checkmark…"),
                        );
                        if dialogs::secondary(ui, "Copy AI prompt", DialogIcon::Copy).clicked() {
                            ctx.copy_text(svg_stamp::prompt(&self.description));
                            self.copied = true;
                        }
                        if self.copied {
                            ui.weak("Prompt copied. Paste it into your preferred AI.");
                        }
                        ui.strong("SVG source");
                        egui::ScrollArea::vertical()
                            .id_salt("svg-code")
                            .max_height(240.)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.svg)
                                        .code_editor()
                                        .desired_rows(12)
                                        .desired_width(f32::INFINITY)
                                        .char_limit(svg_stamp::MAX_BYTES),
                                );
                            });
                        if dialogs::secondary(ui, "Preview SVG", DialogIcon::Eye).clicked() {
                            let source = self.source();
                            match source.validate() {
                                Ok(()) => {
                                    self.preview = Some(source);
                                    self.error = None;
                                }
                                Err(e) => {
                                    self.preview = None;
                                    self.error = Some(e);
                                }
                            }
                        }
                        if let Some(error) = &self.error {
                            dialogs::notice(ui, DialogIcon::Info, "Check SVG", error, true);
                        }
                        let ui = &mut columns[1];
                        ui.strong("Preview");
                        let side = ui.available_width().min(300.);
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::splat(side), egui::Sense::hover());
                        crate::editor::paint_checkerboard(ui.painter(), rect);
                        if let Some(source) = &self.preview
                            && let Ok(texture) = crate::stamps::texture(ctx, source)
                        {
                            let size = texture.size_vec2();
                            let scale = (side - 24.) / size.x.max(size.y);
                            ui.painter().image(
                                texture.id(),
                                egui::Rect::from_center_size(rect.center(), size * scale),
                                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                                egui::Color32::WHITE,
                            );
                        }
                        ui.weak("SVG stays editable in the stamp and project. No API key needed.");
                    });
                });
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                let source = self.source();
                let ready = self.preview.as_ref() == Some(&source);
                if dialogs::button(
                    ui,
                    if self.target.is_some() {
                        "Apply stamp changes"
                    } else {
                        "Save stamp"
                    },
                    DialogIcon::Save,
                    dialogs::Action::Primary,
                    ready,
                )
                .clicked()
                {
                    action = Some(Action::Save(source));
                }
                if dialogs::secondary(ui, "Cancel", DialogIcon::Close).clicked() {
                    action = Some(Action::Cancel);
                }
                if !ready {
                    ui.weak("Preview the current source before saving.");
                }
            });
        });
        if !open {
            action = Some(Action::Cancel);
        }
        action
    }
}
