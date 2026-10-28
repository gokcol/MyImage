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
        if item(ui, "Copy", Icon::Copy, single)
            .on_hover_text("Copy this saved image, including project annotations, to the clipboard")
            .clicked()
        {
            action = Some(LibraryAction::Copy(path.to_owned()));
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
        if !recent && self.marquee.is_some() {
            return false;
        }
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
            let paths: Vec<PathBuf> = [
                "/test/Alpha.myimage",
                "/test/Beta.myimage",
                "/test/Gamma.myimage",
            ]
            .map(PathBuf::from)
            .into();
            p.entries = paths
                .iter()
                .map(|path| LibraryEntry {
                    path: path.clone(),
                    name: path.file_name().unwrap().to_string_lossy().into_owned(),
                    date: chrono::Local::now().date_naive(),
                    kind: EntryKind::Project,
                    modified: None,
                    created: None,
                    bytes: 1,
                })
                .collect();
            for _ in 0..3 {
                frame(&ctx, &mut p, recent, &paths, vec![], egui::Modifiers::NONE);
            }
            for (label, modifiers) in [
                ("Alpha.myimage", egui::Modifiers::CTRL),
                ("Beta.myimage", egui::Modifiers::COMMAND),
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
            let pos = find(&out, "Alpha.myimage");
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
            find(&out, "Copy");
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
    fn selection_fixture(
        view: crate::library::GalleryView,
        count: usize,
    ) -> (egui::Context, LibraryPanel, Vec<PathBuf>) {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut panel = LibraryPanel::default();
        panel.root = Some("/test".into());
        panel.needs_scan = false;
        panel.preferences.view = view;
        panel.preferences.sort = crate::library::SortBy::Name;
        panel.preferences.descending = false;
        let paths: Vec<_> = (0..count)
            .map(|i| PathBuf::from(format!("/test/Project{i:02}.myimage")))
            .collect();
        panel.entries = paths
            .iter()
            .map(|path| LibraryEntry {
                path: path.clone(),
                name: path.file_name().unwrap().to_string_lossy().into_owned(),
                date: NaiveDate::from_ymd_opt(2026, 10, 28).unwrap(),
                kind: EntryKind::Project,
                modified: None,
                created: None,
                bytes: 1,
            })
            .collect();
        for _ in 0..3 {
            frame(
                &ctx,
                &mut panel,
                false,
                &paths,
                vec![],
                egui::Modifiers::NONE,
            );
        }
        (ctx, panel, paths)
    }
    fn bounds(out: &egui::FullOutput, label: &str) -> Rect {
        let b = out
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find_map(|(_, n)| (n.label() == Some(label)).then(|| n.bounds()).flatten())
            .unwrap_or_else(|| panic!("Missing {label}"));
        Rect::from_min_max(
            egui::pos2(b.x0 as f32, b.y0 as f32),
            egui::pos2(b.x1 as f32, b.y1 as f32),
        )
    }
    fn primary(pos: Pos2, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        }
    }
    fn click_control(ctx: &egui::Context, p: &mut LibraryPanel, paths: &[PathBuf], label: &str) {
        let (out, _) = frame(ctx, p, false, paths, vec![], egui::Modifiers::NONE);
        let at = bounds(&out, label).center();
        for pressed in [true, false] {
            let (_, action) = frame(
                ctx,
                p,
                false,
                paths,
                vec![
                    egui::Event::PointerMoved(at),
                    primary(at, pressed, egui::Modifiers::NONE),
                ],
                egui::Modifiers::NONE,
            );
            assert!(
                action.is_none(),
                "Selection control must not open or alter projects"
            );
        }
    }
    #[test]
    fn gallery_drag_selects_grid_and_list_with_modifiers_and_keeps_recents_separate() {
        for view in [
            crate::library::GalleryView::Grid,
            crate::library::GalleryView::List,
        ] {
            for modifiers in [
                egui::Modifiers::NONE,
                egui::Modifiers::SHIFT,
                egui::Modifiers::COMMAND,
                egui::Modifiers::CTRL,
            ] {
                let (ctx, mut panel, paths) = selection_fixture(view, 9);
                panel
                    .selection
                    .paths
                    .extend([paths[0].clone(), paths[3].clone()]);
                panel.recent_selection.paths.insert(paths[7].clone());
                let (out, _) = frame(&ctx, &mut panel, false, &paths, vec![], modifiers);
                let a = bounds(&out, "Project00.myimage");
                let b = bounds(&out, "Project01.myimage");
                let start = a.min + egui::vec2(12., 45.);
                let end = b.min + egui::vec2(60., 58.);
                for events in [
                    vec![
                        egui::Event::PointerMoved(start),
                        primary(start, true, modifiers),
                    ],
                    vec![egui::Event::PointerMoved(end)],
                    vec![primary(end, false, modifiers)],
                ] {
                    let (_, action) = frame(&ctx, &mut panel, false, &paths, events, modifiers);
                    assert!(action.is_none());
                }
                let expected = if modifiers.command || modifiers.ctrl {
                    vec![paths[1].clone(), paths[3].clone()]
                } else if modifiers.shift {
                    vec![paths[0].clone(), paths[1].clone(), paths[3].clone()]
                } else {
                    paths[..2].to_vec()
                };
                assert_eq!(panel.selection.list(), expected, "{view:?} {modifiers:?}");
                assert_eq!(panel.recent_selection.list(), vec![paths[7].clone()]);
                assert!(panel.marquee.is_none());
                let (_, action) = frame(
                    &ctx,
                    &mut panel,
                    false,
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
                    matches!(action,Some(LibraryAction::Trash(selected)) if selected == expected)
                );
            }
        }
    }
    #[test]
    fn gallery_drag_scrolls_to_virtual_rows_and_escape_restores_selection() {
        let (ctx, mut p, paths) = selection_fixture(crate::library::GalleryView::List, 45);
        p.selection.paths.insert(paths[1].clone());
        let (out, _) = frame(&ctx, &mut p, false, &paths, vec![], egui::Modifiers::NONE);
        let start = bounds(&out, "Project00.myimage").min + egui::vec2(12., 45.);
        let viewport = bounds(&out, "Drag to select projects");
        let end = egui::pos2(start.x + 60., viewport.bottom() - 2.);
        frame(
            &ctx,
            &mut p,
            false,
            &paths,
            vec![
                egui::Event::PointerMoved(start),
                primary(start, true, egui::Modifiers::NONE),
            ],
            egui::Modifiers::NONE,
        );
        for _ in 0..120 {
            frame(
                &ctx,
                &mut p,
                false,
                &paths,
                vec![egui::Event::PointerMoved(end)],
                egui::Modifiers::NONE,
            );
        }
        assert!(
            p.selection.paths.contains(&paths[12]),
            "Dragging at the edge must reach offscreen rows: {:?}",
            p.selection.list()
        );
        frame(
            &ctx,
            &mut p,
            false,
            &paths,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            egui::Modifiers::NONE,
        );
        assert!(p.marquee.is_none());
        assert_eq!(p.selection.list(), vec![paths[1].clone()]);
        frame(
            &ctx,
            &mut p,
            false,
            &paths,
            vec![primary(end, false, egui::Modifiers::NONE)],
            egui::Modifiers::NONE,
        );
        assert_eq!(p.selection.list(), vec![paths[1].clone()]);
    }
    #[test]
    fn dated_library_bulk_and_checkbox_selection_respects_day_month_year_and_search() {
        let (ctx, mut p, paths) = selection_fixture(crate::library::GalleryView::Grid, 6);
        for (entry, date) in p.entries.iter_mut().zip([
            (2026, 10, 28),
            (2026, 10, 28),
            (2026, 10, 27),
            (2026, 9, 15),
            (2025, 10, 28),
            (2027, 1, 1),
        ]) {
            entry.date = NaiveDate::from_ymd_opt(date.0, date.1, date.2).unwrap();
        }
        for (filter, expected) in [
            (
                DateFilter::Day(NaiveDate::from_ymd_opt(2026, 10, 28).unwrap()),
                vec![paths[0].clone(), paths[1].clone()],
            ),
            (DateFilter::Month(2026, 10), paths[..3].to_vec()),
            (DateFilter::Year(2026), paths[..4].to_vec()),
        ] {
            p.filter = filter;
            p.filter_dirty = true;
            for _ in 0..3 {
                frame(&ctx, &mut p, false, &paths, vec![], egui::Modifiers::NONE);
            }
            click_control(&ctx, &mut p, &paths, "Select all");
            assert_eq!(p.selection.list(), expected);
            click_control(&ctx, &mut p, &paths, "Deselect");
            assert!(p.selection.paths.is_empty());
        }
        p.filter = DateFilter::Day(NaiveDate::from_ymd_opt(2026, 10, 28).unwrap());
        p.filter_dirty = true;
        for _ in 0..3 {
            frame(&ctx, &mut p, false, &paths, vec![], egui::Modifiers::NONE);
        }
        click_control(&ctx, &mut p, &paths, "Select Project00.myimage");
        click_control(&ctx, &mut p, &paths, "Select Project01.myimage");
        assert_eq!(p.selection.list(), paths[..2]);
        click_control(&ctx, &mut p, &paths, "Select Project00.myimage");
        assert_eq!(p.selection.list(), vec![paths[1].clone()]);
        let (out, _) = frame(&ctx, &mut p, false, &paths, vec![], egui::Modifiers::NONE);
        find(&out, "2026 / October / 28 · Wednesday");
        p.filter = DateFilter::Year(2026);
        p.search = "Project02".into();
        p.filter_dirty = true;
        for _ in 0..3 {
            frame(&ctx, &mut p, false, &paths, vec![], egui::Modifiers::NONE);
        }
        click_control(&ctx, &mut p, &paths, "Select all");
        assert_eq!(p.selection.list(), vec![paths[2].clone()]);
    }
    #[test]
    fn gallery_drag_crosses_date_headers_and_filter_changes_cancel_it() {
        let (ctx, mut p, paths) = selection_fixture(crate::library::GalleryView::List, 4);
        p.entries[0].date = NaiveDate::from_ymd_opt(2026, 10, 27).unwrap();
        p.entries[2].date = NaiveDate::from_ymd_opt(2026, 10, 29).unwrap();
        p.entries[3].date = NaiveDate::from_ymd_opt(2026, 10, 30).unwrap();
        p.filter_dirty = true;
        for _ in 0..3 {
            frame(&ctx, &mut p, false, &paths, vec![], egui::Modifiers::NONE);
        }
        let (out, _) = frame(&ctx, &mut p, false, &paths, vec![], egui::Modifiers::NONE);
        let start = bounds(&out, "Project01.myimage").min + egui::vec2(100., 60.);
        let end = bounds(&out, "Project00.myimage").min + egui::vec2(40., 40.);
        for events in [
            vec![
                egui::Event::PointerMoved(start),
                primary(start, true, egui::Modifiers::NONE),
            ],
            vec![egui::Event::PointerMoved(end)],
            vec![primary(end, false, egui::Modifiers::NONE)],
        ] {
            let (_, a) = frame(&ctx, &mut p, false, &paths, events, egui::Modifiers::NONE);
            assert!(a.is_none());
        }
        assert_eq!(p.selection.list(), paths[..2]);
        // A second drag begins in a date separator, which is genuine background.
        let (out, _) = frame(&ctx, &mut p, false, &paths, vec![], egui::Modifiers::NONE);
        let start = find(&out, "2026 / October / 27 · Tuesday");
        frame(
            &ctx,
            &mut p,
            false,
            &paths,
            vec![
                egui::Event::PointerMoved(start),
                primary(start, true, egui::Modifiers::NONE),
            ],
            egui::Modifiers::NONE,
        );
        frame(
            &ctx,
            &mut p,
            false,
            &paths,
            vec![egui::Event::PointerMoved(end)],
            egui::Modifiers::NONE,
        );
        assert!(p.marquee.is_some());
        p.filter = DateFilter::Day(p.entries[1].date);
        p.filter_dirty = true;
        frame(&ctx, &mut p, false, &paths, vec![], egui::Modifiers::NONE);
        assert!(p.marquee.is_none());
        assert_eq!(p.selection.list(), vec![paths[1].clone()]);
    }
    fn day_state(out: &egui::FullOutput, label: &str) -> egui::accesskit::Toggled {
        out.platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find_map(|(_, node)| {
                (node.label() == Some(label))
                    .then(|| node.toggled())
                    .flatten()
            })
            .unwrap()
    }
    #[test]
    fn day_checkbox_preserves_other_days_and_tracks_partial_and_filtered_selection() {
        for view in [
            crate::library::GalleryView::Grid,
            crate::library::GalleryView::List,
        ] {
            let (ctx, mut p, paths) = selection_fixture(view, 4);
            p.entries[0].date = NaiveDate::from_ymd_opt(2026, 10, 27).unwrap();
            p.filter_dirty = true;
            p.selection.paths.insert(paths[0].clone());
            for _ in 0..3 {
                frame(&ctx, &mut p, false, &paths, vec![], egui::Modifiers::NONE);
            }
            let day = "Select projects for 2026-10-28";
            click_control(&ctx, &mut p, &paths, day);
            assert_eq!(p.selection.list(), paths);
            let (out, _) = frame(&ctx, &mut p, false, &paths, vec![], egui::Modifiers::NONE);
            assert_eq!(day_state(&out, day), egui::accesskit::Toggled::True);
            click_control(&ctx, &mut p, &paths, day);
            assert_eq!(p.selection.list(), vec![paths[0].clone()]);
            click_control(&ctx, &mut p, &paths, "Select Project01.myimage");
            let (out, _) = frame(&ctx, &mut p, false, &paths, vec![], egui::Modifiers::NONE);
            assert_eq!(day_state(&out, day), egui::accesskit::Toggled::Mixed);
            click_control(&ctx, &mut p, &paths, day);
            assert_eq!(p.selection.list(), paths);
            click_control(&ctx, &mut p, &paths, "Deselect");
            let (out, _) = frame(&ctx, &mut p, false, &paths, vec![], egui::Modifiers::NONE);
            assert_eq!(day_state(&out, day), egui::accesskit::Toggled::False);
            p.search = "Project01".into();
            p.filter_dirty = true;
            for _ in 0..3 {
                frame(&ctx, &mut p, false, &paths, vec![], egui::Modifiers::NONE);
            }
            click_control(&ctx, &mut p, &paths, day);
            assert_eq!(p.selection.list(), vec![paths[1].clone()]);
        }
    }
    #[test]
    fn day_checkbox_includes_offscreen_projects_and_does_not_start_a_marquee() {
        let (ctx, mut p, paths) = selection_fixture(crate::library::GalleryView::List, 45);
        let day = "Select projects for 2026-10-28";
        click_control(&ctx, &mut p, &paths, day);
        assert_eq!(p.selection.list(), paths);
        let (out, _) = frame(&ctx, &mut p, false, &paths, vec![], egui::Modifiers::NONE);
        let start = bounds(&out, day).center();
        let end = start + egui::vec2(100., 140.);
        for events in [
            vec![
                egui::Event::PointerMoved(start),
                primary(start, true, egui::Modifiers::NONE),
            ],
            vec![egui::Event::PointerMoved(end)],
            vec![primary(end, false, egui::Modifiers::NONE)],
        ] {
            frame(&ctx, &mut p, false, &paths, events, egui::Modifiers::NONE);
            assert!(p.marquee.is_none());
        }
        assert_eq!(p.selection.list(), paths);
    }
}
