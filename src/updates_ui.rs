use super::*;
use crate::updates::{self, Outcome};

impl MyImageApp {
    fn update_window_parent(frame: &eframe::Frame) -> Option<isize> {
        #[cfg(target_os = "windows")]
        {
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = frame.window_handle()
                && let RawWindowHandle::Win32(handle) = handle.as_raw()
            {
                return Some(handle.hwnd.get());
            }
        }
        let _ = frame;
        None
    }

    pub(super) fn start_update_check(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if self.updates.checking() {
            return;
        }
        self.settings.updates.last_attempt = Some(updates::now());
        self.persist_settings();
        self.updates.start(ctx, Self::update_window_parent(frame));
    }

    /// Keep opt-in checks alive in the tray, without opening a window or interrupting capture.
    pub(super) fn poll_update_service(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        self.updates.poll(ctx);
        if !self.demo
            && !self.quitting
            && !self.busy
            && self.pending_capture.is_none()
            && self.selection.is_none()
            && self.scroll_session.is_none()
            && self.settings.updates.due(updates::now())
        {
            self.start_update_check(ctx, frame);
        }
        if self.settings.updates.automatic {
            ctx.request_repaint_after(Duration::from_secs(60));
        }
    }

    /// Called only after capture/hidden-window early returns, before workspace input.
    pub(super) fn prepare_updates(&mut self, ctx: &egui::Context) {
        let idle = self.workspace_unobscured()
            && !self.busy
            && !self.editor.has_modal()
            && self.editor.autosave_ready()
            && !ctx.wants_keyboard_input()
            && !self.screen_text.open
            && self.pending_capture.is_none()
            && self.capture_launcher.is_none()
            && self.recovery.ready
            && self.error.is_none()
            && ctx.input(|i| i.focused && !i.pointer.any_down())
            && !egui::Popup::is_any_open(ctx);
        if idle && !self.demo {
            if self.updates.pending_notes {
                self.finish_edit();
                self.updates.open = true;
                self.updates.pending_notes = false;
            } else if self.settings.updates.seen_release.is_none() {
                // Fresh installations get onboarding; don't stack a second welcome.
                self.settings.updates.seen_release = Some(updates::release_id());
                self.persist_settings();
            }
        }
        if idle && !self.updates.open && self.updates.notification(&self.settings.updates).is_some()
        {
            egui::TopBottomPanel::bottom("update-notice").show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("A MyImage update is available.");
                    if ui.button("View update").clicked() {
                        self.finish_edit();
                        self.updates.open = true;
                    }
                    if ui.button("Dismiss").clicked() {
                        self.settings.updates.dismissed_update = self
                            .updates
                            .notification(&self.settings.updates)
                            .map(str::to_owned);
                        self.persist_settings();
                    }
                });
            });
        }
    }

    pub(super) fn updates_ui(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if !self.updates.open {
            return;
        }
        let mut check = false;
        let mut changed = false;
        let mut close = false;
        let notes = updates::notes();
        let response = Dialog::new("What's new & updates", "Your installed version and the next update.", Icon::Refresh, 550.)
            .modal(ctx, egui::Id::new("myimage-updates"), |ui| {
                ui.strong(format!("Installed: MyImage {} · build {}", notes.version, notes.build));
                #[cfg(target_os = "macos")]
                ui.small("Updates from GitHub · Mac downloads");
                #[cfg(target_os = "windows")]
                ui.small(format!("Microsoft Store · package {}", notes.store_version));
                ui.add_space(8.);
                match &self.updates.result {
                    Some(Ok(Outcome::Current)) => {
                        ui.label(if cfg!(target_os = "macos") { "You're up to date with the latest stable GitHub release." } else { "Microsoft Store has no update available for this installation." });
                    }
                    Some(Ok(Outcome::Available { title, notes, url, .. })) => {
                        ui.strong(title);
                        // Plain text only: no remote HTML, images, or executable links.
                        egui::ScrollArea::vertical().id_salt("remote-release-notes").max_height(160.).show(ui, |ui| { ui.label(notes); });
                        ui.hyperlink_to(if cfg!(target_os = "macos") { "Download Mac update" } else { "Open Microsoft Store" }, url);
                    }
                    #[cfg(target_os = "windows")]
                    Some(Ok(Outcome::StoreInstallRequired)) => {
                        ui.label("This copy wasn't installed from Microsoft Store. Store update checks require the Store edition.");
                    }
                    Some(Err(error)) => { ui.label(error); }
                    None if self.updates.checking() => { ui.horizontal(|ui| { ui.spinner(); ui.label("Checking for updates…"); }); }
                    None => { ui.label("Check for an update when you're ready."); }
                    #[cfg(all(test, not(target_os = "windows")))]
                    Some(Ok(Outcome::StoreInstallRequired)) => { ui.label("Open Microsoft Store to install the Store edition."); }
                }
                ui.horizontal_wrapped(|ui| {
                    check = ui.add_enabled(!self.updates.checking(), egui::Button::new("Check for updates")).clicked();
                    if cfg!(target_os = "macos") {
                        ui.hyperlink_to("GitHub releases", updates::RELEASES_URL);
                    } else {
                        ui.hyperlink_to("Microsoft Store", updates::STORE_URL);
                    }
                });
                changed |= ui.checkbox(&mut self.settings.updates.automatic, "Check automatically once a day").changed();
                ui.small(if cfg!(target_os = "macos") { "Checks contact GitHub for release information. Your images and projects are never sent. Downloads open in your browser." } else { "Checks use Microsoft Store services. Your images and projects are never sent. Windows manages Store updates." });
                ui.separator();
                ui.strong("What's new in your installed version");
                for line in &notes.changes { ui.label(format!("• {line}")); }
                let platform = if cfg!(target_os = "macos") { &notes.macos } else { &notes.windows };
                for line in platform { ui.label(format!("• {line}")); }
                dialogs::actions(ui, |ui| { close = dialogs::secondary(ui, "Close", Icon::Close).clicked(); });
            });
        if changed {
            self.persist_settings();
        }
        if check {
            self.start_update_check(ctx, frame);
        }
        if close || response.should_close() {
            self.updates.open = false;
            self.updates.pending_notes = false;
            self.settings.updates.seen_release = Some(updates::release_id());
            self.persist_settings();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_modal_blocks_workspace_and_capture_and_escape_marks_notes_seen() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.guide_open = false;
        app.updates.open = true;
        assert!(!app.workspace_unobscured());
        app.begin_capture(&ctx);
        assert!(!app.busy && app.pending_capture.is_none());
        let frame = eframe::Frame::_new_kittest();
        for index in 0..4 {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(700., 460.))),
                ..Default::default()
            };
            if index == 3 {
                input.events.push(egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let output = ctx.run(input, |ctx| app.updates_ui(ctx, &frame));
            if index > 0 && index < 3 {
                let rect = ctx
                    .memory(|m| m.area_rect(egui::Id::new("myimage-updates")))
                    .unwrap();
                assert!(
                    ctx.content_rect().expand(1.).contains_rect(rect),
                    "update dialog must fit a small workspace: {rect:?}"
                );
            }
            assert!(
                output.platform_output.commands.is_empty(),
                "opening or dismissing notes must not launch a download"
            );
        }
        assert!(!app.updates.open);
        assert_eq!(
            app.settings.updates.seen_release,
            Some(updates::release_id())
        );
        assert!(!app.settings.updates.automatic);
        assert!(app.workspace_unobscured());
    }
}
