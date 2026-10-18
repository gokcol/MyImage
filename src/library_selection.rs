//! Selection is independent from the open document and from the other browser surface.
use eframe::egui;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(Default)]
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
                self.paths = order.iter().cloned().collect();
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
