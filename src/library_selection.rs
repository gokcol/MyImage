//! Selection is independent from the open document and from the other browser surface.
use eframe::egui;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(Clone, Default)]
pub(super) struct FileSelection {
    pub paths: BTreeSet<PathBuf>,
    anchor: Option<PathBuf>,
    pub focus: Option<egui::Id>,
}
impl FileSelection {
    pub fn click(&mut self, path: &Path, order: &[PathBuf], toggle: bool, range: bool) {
        if range
            && let Some(a) = self
                .anchor
                .as_ref()
                .and_then(|p| order.iter().position(|q| q == p))
            && let Some(b) = order.iter().position(|q| q == path)
        {
            if !toggle {
                self.paths.clear();
            }
            self.paths
                .extend(order[a.min(b)..=a.max(b)].iter().cloned());
            return;
        }
        if toggle {
            if !self.paths.remove(path) {
                self.paths.insert(path.to_owned());
            }
        } else {
            self.paths.clear();
            self.paths.insert(path.to_owned());
        }
        self.anchor = Some(path.to_owned());
    }
    pub fn context(&mut self, path: &Path) {
        if !self.paths.contains(path) {
            self.click(path, &[], false, false);
        }
    }
    pub fn retain(&mut self, order: &[PathBuf]) {
        let visible: std::collections::HashSet<_> = order.iter().collect();
        self.paths.retain(|p| visible.contains(p));
        if self.anchor.as_ref().is_some_and(|p| !order.contains(p)) {
            self.anchor = None;
        }
    }
    pub fn clear(&mut self) {
        self.paths.clear();
        self.anchor = None;
    }
    pub fn select_all(&mut self, order: &[PathBuf]) {
        self.paths = order.iter().cloned().collect();
        self.anchor = order.first().cloned();
    }
    pub fn set_group(&mut self, paths: impl IntoIterator<Item = PathBuf>, selected: bool) {
        let mut first = None;
        for path in paths {
            first.get_or_insert_with(|| path.clone());
            if selected {
                self.paths.insert(path);
            } else {
                self.paths.remove(&path);
            }
        }
        if selected {
            self.anchor = first;
        } else if self
            .anchor
            .as_ref()
            .is_some_and(|path| !self.paths.contains(path))
        {
            self.anchor = self.paths.first().cloned();
        }
    }
    pub fn single(&self) -> Option<&PathBuf> {
        (self.paths.len() == 1)
            .then(|| self.paths.first())
            .flatten()
    }
    pub fn list(&self) -> Vec<PathBuf> {
        self.paths.iter().cloned().collect()
    }
    pub fn keyboard(&mut self, ui: &egui::Ui, order: &[PathBuf]) -> Option<Vec<PathBuf>> {
        if !ui.is_enabled() || !self.focus.is_some_and(|id| ui.memory(|m| m.has_focus(id))) {
            return None;
        }
        ui.input_mut(|i| {
            if i.consume_key(egui::Modifiers::COMMAND, egui::Key::A)
                || i.consume_key(egui::Modifiers::CTRL, egui::Key::A)
            {
                self.select_all(order);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                self.clear();
            }
            if (i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace))
                && !self.paths.is_empty()
            {
                return Some(self.list());
            }
            None
        })
    }
}

/// The anchor is in gallery-content coordinates, so it stays fixed while scrolling.
pub(super) struct Marquee {
    start: egui::Pos2,
    before: FileSelection,
    toggle: bool,
    additive: bool,
    layout: (usize, egui::Vec2, egui::Vec2),
}

pub(super) struct GalleryLayout<'a> {
    pub origin: egui::Pos2,
    pub viewport: egui::Rect,
    pub columns: usize,
    pub card: egui::Vec2,
    pub gap: egui::Vec2,
    pub cards: &'a [egui::Rect],
    pub day_checkboxes: &'a [egui::Rect],
}
impl super::LibraryPanel {
    pub(super) fn cancel_marquee(&mut self) {
        if let Some(drag) = self.marquee.take() {
            self.selection = drag.before;
        }
    }

    pub(super) fn marquee_selection(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        layout: &GalleryLayout<'_>,
    ) -> Option<egui::Rect> {
        let (down, released, pointer, focused) = ui.input(|i| {
            (
                i.pointer.primary_down(),
                i.pointer.button_released(egui::PointerButton::Primary),
                i.pointer.interact_pos(),
                i.focused,
            )
        });
        if self.marquee.is_some()
            && (!ui.is_enabled()
                || !focused
                || (!down && !released)
                || self
                    .marquee
                    .as_ref()
                    .is_some_and(|d| d.layout != (layout.columns, layout.card, layout.gap))
                || ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)))
        {
            self.cancel_marquee();
            return None;
        }
        if response.drag_started_by(egui::PointerButton::Primary)
            && ui.is_enabled()
            && focused
            && !egui::Popup::is_any_open(ui.ctx())
            && let Some(start) = ui.input(|i| i.pointer.press_origin())
            && layout.viewport.contains(start)
        {
            let local = start - layout.origin.to_vec2();
            // Checkboxes and stars remain separate actions, even if a press slips.
            let on_control = (0..self.filtered.len()).any(|index| {
                egui::Rect::from_min_size(
                    layout.cards[index].right_top() + egui::vec2(-32., 6.),
                    egui::Vec2::splat(24.),
                )
                .contains(local)
                    || egui::Rect::from_min_size(
                        layout.cards[index].min + egui::Vec2::splat(6.),
                        egui::Vec2::splat(24.),
                    )
                    .contains(local)
            });
            let on_control = on_control
                || layout
                    .day_checkboxes
                    .iter()
                    .any(|rect| rect.contains(local));
            if !on_control {
                let modifiers = ui.input(|i| i.modifiers);
                self.marquee = Some(Marquee {
                    start: local,
                    before: self.selection.clone(),
                    toggle: modifiers.command || modifiers.ctrl,
                    additive: modifiers.shift,
                    layout: (layout.columns, layout.card, layout.gap),
                });
                response.request_focus();
                self.selection.focus = Some(response.id);
            }
        }
        if let Some(drag) = &self.marquee {
            let pointer = pointer?;
            let end = layout.viewport.clamp(pointer) - layout.origin.to_vec2();
            let region = egui::Rect::from_two_pos(drag.start, end);
            let hits: BTreeSet<_> = self
                .filtered
                .iter()
                .enumerate()
                .filter(|(index, _)| region.intersects(layout.cards[*index]))
                .map(|(_, &index)| self.entries[index].path.clone())
                .collect();
            self.selection.paths = if drag.toggle {
                drag.before
                    .paths
                    .symmetric_difference(&hits)
                    .cloned()
                    .collect()
            } else if drag.additive {
                drag.before.paths.union(&hits).cloned().collect()
            } else {
                hits
            };
            self.selection.anchor = self
                .filtered
                .iter()
                .map(|&index| &self.entries[index].path)
                .find(|path| self.selection.paths.contains(*path))
                .cloned();
            if down && pointer.x >= layout.viewport.left() && pointer.x <= layout.viewport.right() {
                let edge = 28.;
                let speed = if pointer.y < layout.viewport.top() + edge {
                    ((layout.viewport.top() + edge - pointer.y) / edge).clamp(0., 1.) * 620.
                } else if pointer.y > layout.viewport.bottom() - edge {
                    -((pointer.y - layout.viewport.bottom() + edge) / edge).clamp(0., 1.) * 620.
                } else {
                    0.
                };
                if speed != 0. {
                    let dt = ui.input(|i| i.stable_dt).min(0.05);
                    ui.scroll_with_delta_animation(
                        egui::vec2(0., speed * dt),
                        egui::style::ScrollAnimation::none(),
                    );
                    ui.ctx().request_repaint();
                }
            }
            Some(
                region
                    .translate(layout.origin.to_vec2())
                    .intersect(layout.viewport),
            )
        } else {
            if response.clicked() {
                if !ui.input(|i| i.modifiers.shift || i.modifiers.command || i.modifiers.ctrl) {
                    self.selection.clear();
                }
                self.selection.focus = Some(response.id);
                response.request_focus();
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn toggles_ranges_and_context_keep_the_intended_batch() {
        let order: Vec<PathBuf> = ["a", "b", "c", "d"].map(PathBuf::from).into();
        let mut s = FileSelection::default();
        s.click(&order[0], &order, false, false);
        s.click(&order[2], &order, true, false);
        assert_eq!(s.list(), vec![order[0].clone(), order[2].clone()]);
        s.context(&order[0]);
        assert_eq!(s.paths.len(), 2);
        s.click(&order[2], &order, true, false);
        assert_eq!(s.list(), vec![order[0].clone()]);
        s.click(&order[0], &order, false, false);
        s.click(&order[3], &order, false, true);
        assert_eq!(s.list(), order);
        s.retain(&order[1..3]);
        assert_eq!(s.list(), order[1..3]);
        s.context(&order[3]);
        assert_eq!(s.single(), Some(&order[3]));
    }
}
