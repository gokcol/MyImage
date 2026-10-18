use super::*;

impl LibraryPanel {
    pub fn unhide_recent(&mut self, path: &Path) {
        if self.preferences.hidden_recents.remove(path) {
            self.preferences_dirty = true;
        }
    }
    pub(super) fn file_menu(
        &mut self,
        ui: &mut egui::Ui,
        path: &Path,
        recent: bool,
    ) -> Option<LibraryAction> {
        let selected = if recent {
            &self.recent_selection
        } else {
            &self.selection
        };
        let paths = selected.list();
        let single = paths.len() == 1;
        let mut action = None;
        let item = |ui: &mut egui::Ui, label: &str, icon, enabled| {
            appearance::action_button(ui, label, icon, enabled, false)
        };
        use appearance::Icon;
        ui.set_min_width(180.);
        if paths.len() > 1 {
            ui.strong(format!("{} selected", paths.len()));
            ui.separator();
        }
        if item(ui, "Open in Editor", Icon::Edit, single).clicked() {
            action = Some(LibraryAction::Open(path.to_owned()));
            ui.close();
        }
        if recent && item(ui, "Reveal in Library", Icon::Library, single).clicked() {
            action = Some(LibraryAction::RevealLibrary(path.to_owned()));
            ui.close();
        }
        if item(
            ui,
            if cfg!(target_os = "macos") {
                "Reveal in Finder"
            } else {
                "Reveal in Explorer"
            },
            Icon::Open,
            single,
        )
        .clicked()
        {
            action = Some(LibraryAction::Reveal(path.to_owned()));
            ui.close();
        }
        ui.separator();
        let all_favorites = paths.iter().all(|p| self.preferences.favorites.contains(p));
        if item(
            ui,
            if all_favorites {
                "Remove from Favorites"
            } else {
                "Add to Favorites"
            },
            Icon::Favorite,
            true,
        )
        .clicked()
        {
            for p in &paths {
                if all_favorites {
                    self.preferences.favorites.remove(p);
                } else {
                    self.preferences.favorites.insert(p.clone());
                }
            }
            self.preferences_dirty = true;
            self.filter_dirty = true;
            ui.close();
        }
        if item(ui, "Rename…", Icon::Edit, single).clicked() {
            action = Some(LibraryAction::Rename(path.to_owned()));
            ui.close();
        }
        if item(ui, "Duplicate", Icon::Copy, true).clicked() {
            action = Some(LibraryAction::Duplicate(paths.clone()));
            ui.close();
        }
        if item(ui, "File info…", Icon::Info, single).clicked() {
            action = Some(LibraryAction::Info(path.to_owned()));
            ui.close();
        }
        ui.separator();
        if recent
            && item(ui, "Remove from Recents", Icon::Remove, true)
                .on_hover_text("Keep the files in your library and on disk")
                .clicked()
        {
            self.preferences
                .hidden_recents
                .extend(paths.iter().cloned());
            self.preferences_dirty = true;
            self.recent_selection.clear();
            ui.close();
        }
        if item(ui, "Move to Trash…", Icon::Trash, true).clicked() {
            action = Some(LibraryAction::Trash(paths));
            ui.close();
        }
        action
    }
    pub(super) fn item_interaction(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        path: &Path,
        recent: bool,
        order: &[PathBuf],
    ) -> bool {
        let m = ui.input(|i| i.modifiers);
        let toggle = m.ctrl || m.command;
        let selection = if recent {
            &mut self.recent_selection
        } else {
            &mut self.selection
        };
        // macOS may translate Control-click into a secondary click.
        if response.clicked() || (response.secondary_clicked() && m.ctrl) {
            selection.click(path, order, toggle, m.shift);
            selection.focus = Some(response.id);
            response.request_focus();
            return !toggle && !m.shift;
        }
        if response.secondary_clicked() {
            selection.context(path);
            selection.focus = Some(response.id);
            response.request_focus();
        }
        false
    }
    pub fn reveal_in_library(&mut self, path: PathBuf) {
        self.collection = Collection::All;
        self.filter = DateFilter::All;
        self.search = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        self.selection.click(&path, &[], false, false);
        self.filter_dirty = true;
        self.reset_scroll = true;
    }
    pub fn forget_paths(&mut self, paths: &[PathBuf]) {
        for p in paths {
            self.preferences.favorites.remove(p);
            self.preferences.hidden_recents.remove(p);
        }
        self.selection.paths.retain(|p| !paths.contains(p));
        self.recent_selection.paths.retain(|p| !paths.contains(p));
        self.preferences_dirty = true;
    }
    pub fn renamed_path(&mut self, old: &Path, new: &Path) {
        if self.preferences.favorites.remove(old) {
            self.preferences.favorites.insert(new.to_owned());
        }
        if self.preferences.hidden_recents.remove(old) {
            self.preferences.hidden_recents.insert(new.to_owned());
        }
        for s in [&mut self.selection, &mut self.recent_selection] {
            if s.paths.remove(old) {
                s.paths.insert(new.to_owned());
            }
        }
        self.preferences_dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(
        ctx: &egui::Context,
        p: &mut LibraryPanel,
        recent: bool,
        paths: &[PathBuf],
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) -> (egui::FullOutput, Option<LibraryAction>) {
        let mut action = None;
        let out = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 700.))),
                events,
                modifiers,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    if recent {
                        action =
                            p.recents_ui(ui, Path::new("/test"), paths, Some(&paths[0]), false);
                    } else {
                        action = p.ui(ui, Path::new("/test"));
                    }
                });
            },
        );
        (out, action)
    }
    fn find(out: &egui::FullOutput, label: &str) -> Pos2 {
        out.shapes
            .iter()
            .find_map(|s| {
                if let egui::Shape::Text(t) = &s.shape {
                    (t.galley.job.text == label)
                        .then(|| Rect::from_min_size(t.pos, t.galley.size()).center())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("Missing {label}"))
    }
    #[test]
    fn library_and_recents_pointer_selection_delete_and_context_are_scoped() {
        for recent in [false, true] {
            let ctx = egui::Context::default();
            let mut p = LibraryPanel::default();
            p.root = Some("/test".into());
            p.needs_scan = false;
            let paths: Vec<PathBuf> = ["/test/Alpha.png", "/test/Beta.png", "/test/Gamma.png"]
                .map(PathBuf::from)
                .into();
            p.entries = paths
                .iter()
                .map(|path| LibraryEntry {
                    path: path.clone(),
                    name: path.file_name().unwrap().to_string_lossy().into_owned(),
                    date: chrono::Local::now().date_naive(),
                    kind: EntryKind::Image,
                    modified: None,
                    created: None,
                    bytes: 1,
                })
                .collect();
            for _ in 0..3 {
                frame(&ctx, &mut p, recent, &paths, vec![], egui::Modifiers::NONE);
            }
            for (label, modifiers) in [
                ("Alpha.png", egui::Modifiers::CTRL),
                ("Beta.png", egui::Modifiers::COMMAND),
            ] {
                let (out, _) = frame(&ctx, &mut p, recent, &paths, vec![], modifiers);
                let pos = find(&out, label);
                for pressed in [true, false] {
                    let (_, action) = frame(
                        &ctx,
                        &mut p,
                        recent,
                        &paths,
                        vec![
                            egui::Event::PointerMoved(pos),
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers,
                            },
                        ],
                        modifiers,
                    );
                    assert!(action.is_none(), "Modifier click must not open an image");
                }
            }
            let s = if recent {
                &p.recent_selection
            } else {
                &p.selection
            };
            assert_eq!(s.paths.len(), 2);
            // Focus must persist across idle frames; Delete must target exactly the visible selection.
            frame(&ctx, &mut p, recent, &paths, vec![], egui::Modifiers::NONE);
            let (_, action) = frame(
                &ctx,
                &mut p,
                recent,
                &paths,
                vec![egui::Event::Key {
                    key: egui::Key::Delete,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                egui::Modifiers::NONE,
            );
            assert!(
                matches!(action,Some(LibraryAction::Trash(ref selected)) if selected.len()==2),
                "Delete did not target two files in recent={recent}"
            );
            let (out, _) = frame(&ctx, &mut p, recent, &paths, vec![], egui::Modifiers::NONE);
            let pos = find(&out, "Alpha.png");
            for pressed in [true, false] {
                frame(
                    &ctx,
                    &mut p,
                    recent,
                    &paths,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Secondary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    egui::Modifiers::NONE,
                );
            }
            let (out, _) = frame(&ctx, &mut p, recent, &paths, vec![], egui::Modifiers::NONE);
            find(&out, "2 selected");
            find(&out, "Move to Trash…");
            find(&out, "Duplicate");
            if recent {
                find(&out, "Remove from Recents");
            }
        }
    }
    #[test]
    fn recents_removal_is_persistent_and_reopening_unhides_the_file() {
        let mut p = LibraryPanel::default();
        let path = PathBuf::from("/test/keep.myimage");
        p.preferences.hidden_recents.insert(path.clone());
        let json = serde_json::to_vec(&p.preferences).unwrap();
        p.preferences = serde_json::from_slice(&json).unwrap();
        assert!(p.preferences.hidden_recents.contains(&path));
        p.unhide_recent(&path);
        assert!(!p.preferences.hidden_recents.contains(&path));
        assert!(p.take_preferences_change().is_some());
    }
}
