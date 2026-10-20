//! Visual selection properties, using the same pixel/layer operations as the canvas.
use super::*;
use crate::appearance::{self};
use inspector_widgets::{Glyph, tile};

fn section(ui: &mut egui::Ui, label: &str) {
    ui.add_space(2.);
    ui.label(
        egui::RichText::new(label)
            .size(11.)
            .strong()
            .color(crate::appearance::palette(ui.ctx()).muted),
    );
}

impl EditorState {
    pub(super) fn privacy_actions(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let enabled = self.selection_region(document).is_some()
            && (self.selected.is_none() || self.can_edit_selection(document));
        ui.add_enabled_ui(enabled, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.;
                let size = Vec2::new((ui.available_width() - 12.) / 3., 48.);
                for (label, glyph, tool) in [
                    ("Blur", Glyph::Blur, Tool::Blur),
                    ("Pixelate", Glyph::Pixelate, Tool::Pixelate),
                    ("Fill", Glyph::Fill, Tool::Fill),
                ] {
                    if tile(ui, label, glyph, size, false, true)
                        .on_hover_text(match tool {
                            Tool::Fill => "Fill the selected area with color or transparency",
                            Tool::Blur => "Blur the selected pixels; adjust the strength afterward",
                            _ => "Pixelate the selected pixels; adjust the block size afterward",
                        })
                        .clicked()
                    {
                        if tool == Tool::Fill {
                            self.activate_fill(document, history);
                        } else {
                            self.apply_area_action(tool.kind().unwrap(), document, history);
                        }
                    }
                }
            });
        });
    }

    pub(super) fn area_inspector(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        ui.spacing_mut().item_spacing.y = 6.;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Selection").size(16.).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {
                if self.selected.is_some() {
                    ui.add_enabled_ui(self.can_edit_selection(document),|ui| {
                        if tile(ui,"Remove edit",Glyph::Remove,Vec2::splat(26.),false,false)
                            .on_hover_text("Remove this edit and restore its original pixels").clicked() {
                            self.delete(document,history); self.area=None;
                        }
                    });
                }
                let help = tile(ui,"Selection help",Glyph::Help,Vec2::splat(26.),false,false);
                egui::Popup::menu(&help).show(|ui| {
                    ui.set_max_width(240.);
                    ui.strong("Select, then make it yours");
                    ui.label("Drag on the image to select pixels. Click an object to edit it.");
                    ui.label("Boundary moves only the marquee. Pixels lifts the selected area as an editable layer.");
                    ui.label("Edges and corners resize the selection. Escape cancels a drag or clears the selection.");
                    ui.small("Original pixels and annotations remain editable in your .myimage project.");
                });
            });
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 5.;
            let size = Vec2::new((ui.available_width() - 15.) / 4., 58.);
            for (label, glyph, lasso) in [
                ("Rectangle", Glyph::Rectangle, false),
                ("Lasso", Glyph::Lasso, true),
            ] {
                if tile(ui, label, glyph, size, self.lasso == lasso, true)
                    .on_hover_text(if lasso {
                        "Draw a freehand outline to select an irregular area"
                    } else {
                        "Drag a rectangular selection on the image"
                    })
                    .clicked()
                {
                    self.lasso = lasso;
                }
            }
            if tile(ui, "Select all", Glyph::All, size, false, true)
                .on_hover_text("Select the entire image · Command/Ctrl+A")
                .clicked()
            {
                self.select_all_image(document, history);
            }
            ui.add_enabled_ui(self.area_bounds(document).is_some(), |ui| {
                if tile(ui, "Deselect", Glyph::Clear, size, false, true)
                    .on_hover_text("Clear the selection · Escape")
                    .clicked()
                {
                    self.deselect_pixels();
                    self.focus_canvas = true;
                }
            });
        });
        let has_area = self.area_bounds(document).is_some();
        let editable = has_area && (self.selected.is_none() || self.can_edit_selection(document));
        if has_area {
            ui.add_space(2.);
            self.selection_size_controls(ui, document);
            section(ui, "Drag inside selection");
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.;
                let size = Vec2::new((ui.available_width()-6.)/2.,56.);
                if tile(ui,"Adjust boundary",Glyph::Boundary,size,!self.move_pixels && self.selected.is_none(),true)
                    .on_hover_text("Move only the selection boundary; image pixels stay in place").clicked() {
                    self.set_area_move_mode(false,document);
                }
                ui.add_enabled_ui(editable,|ui| {
                    if tile(ui,"Move pixels",Glyph::Pixels,size,self.move_pixels,true)
                        .on_hover_text("Drag inside to lift and move pixels. The original area becomes transparent.").clicked() {
                        self.set_area_move_mode(true,document);
                    }
                });
            });
        } else {
            ui.label(
                egui::RichText::new("Drag on your image to select an area.")
                    .size(11.)
                    .color(crate::appearance::palette(ui.ctx()).muted),
            );
        }
        section(ui, "Clipboard & pixels");
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.;
            let size = Vec2::new((ui.available_width() - 12.) / 3., 48.);
            ui.scope(|ui| {
                if tile(ui, "Copy", Glyph::Copy, size, false, true)
                    .on_hover_text(if has_area {
                        "Copy selected pixels and visible edits · Command/Ctrl+C"
                    } else {
                        "Copy the whole image and visible edits · Command/Ctrl+C"
                    })
                    .clicked()
                {
                    self.copy_image(ui.ctx(), document, false);
                }
            });
            ui.add_enabled_ui(editable, |ui| {
                if tile(ui, "Cut", Glyph::Cut, size, false, true)
                    .on_hover_text("Copy and clear to transparency · Command/Ctrl+X")
                    .clicked()
                {
                    self.cut_selection(ui.ctx(), document, history);
                }
                if tile(ui, "Delete pixels", Glyph::Delete, size, false, true)
                    .on_hover_text("Clear to transparency and deselect · Delete")
                    .clicked()
                {
                    self.apply_area_action(AnnotationKind::DeleteArea, document, history);
                }
            });
        });
        section(ui, "Effects");
        self.privacy_actions(ui, document, history);
        if let Some(i) = self
            .selected
            .filter(|&i| document.annotations[i].kind.is_effect())
        {
            ui.add_enabled_ui(editable, |ui| {
                let label = if document.annotations[i].kind == AnnotationKind::Blur {
                    "Blur strength"
                } else {
                    "Block size"
                };
                ui.small(label);
                if appearance::value_slider(
                    ui,
                    &mut self.effect_size,
                    2..=crate::document::MAX_EFFECT_SIZE,
                    label,
                    " px",
                )
                .changed()
                {
                    let before = document.edit_state();
                    document.annotations[i].effect_size = self.effect_size;
                    history.commit(before, document);
                }
            });
        }
        if has_area && !editable {
            ui.small("Unlock this layer in Layers to edit it.");
        }
    }
}
