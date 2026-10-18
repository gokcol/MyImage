//! A focused editing session: live canvas preview, one undo step, explicit cancellation.
use super::*;
use crate::text_style::{Alignment, TextStyle};

pub(super) struct TextDraft {
    index: usize,
    before: EditState,
    focus: bool,
}
impl EditorState {
    pub(super) fn open_text_editor(
        &mut self,
        index: usize,
        before: EditState,
        document: &Document,
    ) {
        if document.annotations.get(index).is_none_or(|a| {
            a.locked() || !matches!(a.kind, AnnotationKind::Text | AnnotationKind::Callout)
        }) {
            return;
        }
        self.select(Some(index), document);
        self.text_editor_generation = self.text_editor_generation.wrapping_add(1);
        self.text_dialog = Some(TextDraft {
            index,
            before,
            focus: true,
        });
        self.focus_text = false;
    }
    pub(super) fn finish_text_editor(
        &mut self,
        document: &mut Document,
        history: &mut EditHistory,
        apply: bool,
    ) {
        let Some(draft) = self.text_dialog.take() else {
            return;
        };
        if apply {
            // Empty text is not an invisible object; cancel a new blank annotation.
            if document
                .annotations
                .get(draft.index)
                .is_some_and(|a| a.kind == AnnotationKind::Text && a.text.trim().is_empty())
            {
                document.annotations.remove(draft.index);
            }
            history.commit(draft.before, document);
        } else {
            document.annotations = draft.before.annotations;
            document.geometry = draft.before.geometry;
        }
        self.tool = Tool::Area;
        self.select(
            (draft.index < document.annotations.len()).then_some(draft.index),
            document,
        );
        self.focus_canvas = true;
    }
    pub(super) fn text_editor_window(
        &mut self,
        ctx: &egui::Context,
        document: &mut Document,
        history: &mut EditHistory,
    ) {
        let Some(mut draft) = self.text_dialog.take() else {
            return;
        };
        let Some(annotation) = document.annotations.get_mut(draft.index) else {
            return;
        };
        let mut open = true;
        let mut apply = ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter));
        let mut cancel = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        egui::Window::new("Edit text")
            .id(egui::Id::new("text-properties-window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.)
            .default_pos(egui::pos2(360., 200.))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("Text properties");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.weak("Live preview");
                    });
                });
                ui.add_space(6.);
                ui.horizontal(|ui| {
                    font_controls(ui, &mut annotation.text_style);
                    ui.label("Size");
                    object_properties::font_size_menu(ui, &mut annotation.font_size);
                    ui.label("Color");
                    let color = if annotation.kind == AnnotationKind::Callout {
                        &mut annotation.text_color
                    } else {
                        &mut annotation.color
                    };
                    ui.color_edit_button_srgba_unmultiplied(color);
                });
                ui.horizontal(|ui| {
                    ui.toggle_value(
                        &mut annotation.text_style.italic,
                        egui::RichText::new("Italic").italics(),
                    );
                    ui.toggle_value(
                        &mut annotation.text_style.underline,
                        egui::RichText::new("Underline").underline(),
                    );
                    ui.separator();
                    ui.selectable_value(
                        &mut annotation.text_style.alignment,
                        Alignment::Left,
                        "Left",
                    );
                    ui.selectable_value(
                        &mut annotation.text_style.alignment,
                        Alignment::Center,
                        "Center",
                    );
                    ui.selectable_value(
                        &mut annotation.text_style.alignment,
                        Alignment::Right,
                        "Right",
                    );
                });
                ui.add_space(8.);
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .stroke(Stroke::new(1., Color32::from_rgb(166, 187, 204)))
                    .corner_radius(8)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.set_min_size(egui::vec2(436., 150.));
                        egui::ScrollArea::vertical()
                            .id_salt("text-draft-scroll")
                            .max_height(200.)
                            .show(ui, |ui| {
                                let response = ui.add(
                                    egui::TextEdit::multiline(&mut annotation.text)
                                        .id(egui::Id::new((
                                            "focused-annotation-text",
                                            self.text_editor_generation,
                                        )))
                                        .font(if annotation.text_style.monospace {
                                            egui::FontId::monospace(20.)
                                        } else {
                                            egui::FontId::proportional(20.)
                                        })
                                        .text_color(Color32::from_rgb(29, 41, 56))
                                        .frame(false)
                                        .desired_width(436.)
                                        .desired_rows(6)
                                        .char_limit(16_384)
                                        .hint_text("Write your text here…"),
                                );
                                if draft.focus {
                                    response.request_focus();
                                    draft.focus = false;
                                    if let Some(mut state) =
                                        egui::text_edit::TextEditState::load(ctx, response.id)
                                    {
                                        state.cursor.set_char_range(Some(
                                            egui::text::CCursorRange::two(
                                                egui::text::CCursor::new(0),
                                                egui::text::CCursor::new(
                                                    annotation.text.chars().count(),
                                                ),
                                            ),
                                        ));
                                        state.store(ctx, response.id);
                                    }
                                }
                            });
                    });
                ui.add_space(6.);
                ui.horizontal(|ui| {
                    ui.weak("Enter: new line · Ctrl/⌘ + Enter: apply");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        apply |= ui
                            .add(egui::Button::new("Apply").fill(Color32::from_rgb(201, 237, 235)))
                            .clicked();
                        cancel |= ui.button("Cancel").clicked();
                    });
                });
            });
        self.text_dialog = Some(draft);
        if apply || cancel || !open {
            self.finish_text_editor(document, history, apply);
        }
    }
}
fn font_controls(ui: &mut egui::Ui, style: &mut TextStyle) {
    egui::ComboBox::from_id_salt("text-font-family")
        .width(140.)
        .selected_text(if style.monospace {
            "Monospace"
        } else {
            "Sans serif"
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut style.monospace, false, "Sans serif");
            ui.selectable_value(&mut style.monospace, true, "Monospace");
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document() -> Document {
        Document {
            image: std::sync::Arc::new(image::RgbaImage::new(200, 200)),
            annotations: vec![],
            geometry: Default::default(),
        }
    }
    #[test]
    fn text_session_cancel_and_apply_are_atomic_and_undoable() {
        let mut e = EditorState::default();
        let mut d = document();
        let mut h = EditHistory::new(d.edit_state());
        let before = d.edit_state();
        d.annotations
            .push(e.new_annotation(AnnotationKind::Text, Point { x: 20., y: 20. }));
        e.open_text_editor(0, before, &d);
        d.annotations[0].text = "Draft".into();
        e.finish_text_editor(&mut d, &mut h, false);
        assert!(d.annotations.is_empty());
        assert!(!h.can_undo());
        let before = d.edit_state();
        d.annotations
            .push(e.new_annotation(AnnotationKind::Text, Point { x: 20., y: 20. }));
        e.open_text_editor(0, before, &d);
        d.annotations[0].text = "Applied".into();
        d.annotations[0].text_style.italic = true;
        e.finish_text_editor(&mut d, &mut h, true);
        assert!(h.can_undo());
        h.undo(&mut d);
        assert!(d.annotations.is_empty());
        h.redo(&mut d);
        assert_eq!(d.annotations[0].text, "Applied");
        assert!(d.annotations[0].text_style.italic);
        let before = d.edit_state();
        e.open_text_editor(0, before, &d);
        d.annotations[0].text = "Unwanted".into();
        e.finish_text_editor(&mut d, &mut h, false);
        assert_eq!(d.annotations[0].text, "Applied");
    }
    #[test]
    fn locked_text_does_not_open_for_editing() {
        let mut e = EditorState::default();
        let mut d = document();
        let mut a = e.new_annotation(AnnotationKind::Text, Point { x: 0., y: 0. });
        a.layer.locked = true;
        d.annotations.push(a);
        e.open_text_editor(0, d.edit_state(), &d);
        assert!(e.text_dialog.is_none());
    }
    #[test]
    fn long_text_window_stays_bounded_and_apply_shortcut_preserves_content() {
        let context = egui::Context::default();
        let mut editor = EditorState::default();
        let mut document = document();
        let mut annotation = editor.new_annotation(AnnotationKind::Text, Point { x: 10., y: 10. });
        annotation.text = "A long editable line\n".repeat(100);
        document.annotations.push(annotation);
        let content = document.annotations[0].text.clone();
        let mut history = EditHistory::new(document.edit_state());
        editor.open_text_editor(0, document.edit_state(), &document);
        let generation = editor.text_editor_generation;
        for frame in 0..3 {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200., 800.))),
                ..Default::default()
            };
            if frame == 2 {
                input.modifiers = Modifiers::COMMAND;
                input.events.push(egui::Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::COMMAND,
                });
            }
            let _ = context.run(input, |ctx| {
                editor.text_editor_window(ctx, &mut document, &mut history)
            });
            if frame == 1 {
                let rect = context
                    .memory(|m| m.area_rect(egui::Id::new("text-properties-window")))
                    .unwrap();
                assert!(rect.height() < 500., "window height {}", rect.height());
                assert!(rect.width() < 620., "window width {}", rect.width());
            }
        }
        assert!(editor.text_dialog.is_none());
        assert_eq!(document.annotations[0].text, content);
        editor.open_text_editor(0, document.edit_state(), &document);
        assert_ne!(editor.text_editor_generation, generation);
    }
}
