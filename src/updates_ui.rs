use super::*;
use crate::updates::{self, Outcome};
use egui::{Align2, Frame, RichText};

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
        self.updates.reveal_status = self.updates.open;
    }

    /// Keep opt-in checks alive in the tray, without opening a window or interrupting capture.
    pub(super) fn poll_update_service(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        self.updates.poll(ctx);
        if std::mem::take(&mut self.updates.manual_request) {
            self.start_update_check(ctx, frame);
        }
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
        let reveal_status = std::mem::take(&mut self.updates.reveal_status);
        let palette = appearance::palette(ctx);
        let width = (ctx.content_rect().width() - 80.).clamp(220., 540.);
        let body_height = (ctx.content_rect().height() - 150.).max(80.);
        let id = egui::Id::new("myimage-updates");
        let response = egui::Modal::new(id)
            .area(egui::Modal::default_area(id).anchor(Align2::CENTER_CENTER, Vec2::ZERO))
            .backdrop_color(Color32::from_black_alpha(100))
            .frame(Frame::new()
                .fill(Color32::WHITE)
                .stroke(Stroke::new(1., palette.border))
                .corner_radius(18)
                .inner_margin(24)
                .shadow(egui::Shadow {
                    offset: [0, 18], blur: 48, spread: 0,
                    color: Color32::from_black_alpha(55),
                }))
            .show(ctx, |ui| {
                ui.set_width(width);
                // Reserve against the viewport, not the modal's initial centered origin.
                ui.set_max_height(body_height.min(610.) + 64.);
                ui.spacing_mut().item_spacing = Vec2::new(8., 8.);
                egui::ScrollArea::vertical()
                    .id_salt("updates-body")
                    .max_height(body_height.min(610.))
                    .min_scrolled_height(0.)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.add(egui::Image::new(&self.app_icon).fit_to_exact_size(Vec2::splat(56.)));
                            ui.add_space(6.);
                            ui.vertical(|ui| {
                                ui.label(RichText::new("What's New").size(28.).strong().color(palette.ink));
                                ui.label(RichText::new("The latest improvements in MyImage").size(12.).color(palette.muted));
                            });
                        });
                        ui.add_space(8.);
                        Frame::new().fill(palette.cream).corner_radius(8)
                            .inner_margin(egui::Margin::symmetric(10, 5))
                            .show(ui, |ui| {
                                ui.label(RichText::new(format!("Version {} · Build {} · Installed", notes.version, notes.build))
                                    .size(12.).strong().color(palette.accent));
                            });
                        ui.add_space(12.);
                        release_section(ui, "IN THIS RELEASE", &notes.changes);
                        let (platform_title, platform_notes) = if cfg!(target_os = "macos") {
                            ("ON YOUR MAC", &notes.macos)
                        } else {
                            ("ON WINDOWS", &notes.windows)
                        };
                        ui.add_space(10.);
                        release_section(ui, platform_title, platform_notes);
                        ui.add_space(14.);
                        let status_card = Frame::new().fill(palette.base).corner_radius(12).inner_margin(14)
                            .show(ui, |ui| {
                                ui.set_width((width - 28.).max(160.));
                                ui.horizontal(|ui| {
                                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(20.), egui::Sense::hover());
                                    appearance::icon(ui.painter(), rect.center(), Icon::Refresh, palette.accent, 18.);
                                    ui.label(RichText::new("Stay up to date").size(15.).strong().color(palette.ink));
                                });
                                ui.label(RichText::new(if cfg!(target_os = "macos") {
                                    "Mac updates from GitHub"
                                } else { "Windows updates from Microsoft Store" }).size(12.).color(palette.muted));
                                ui.add_space(4.);
                                match &self.updates.result {
                                    Some(Ok(Outcome::Current)) => {
                                        ui.label(RichText::new(if cfg!(target_os = "macos") {
                                            "You're up to date with the latest stable GitHub release."
                                        } else { "Microsoft Store has no update available for this installation." }).color(palette.ink));
                                    }
                                    Some(Ok(Outcome::Available { title, notes, url, .. })) => {
                                        ui.label(RichText::new(title).strong().color(palette.accent));
                                        // Remote notes remain plain text; only the validated update URL is linked.
                                        egui::ScrollArea::vertical().id_salt("remote-release-notes").max_height(140.).show(ui, |ui| { ui.label(notes); });
                                        ui.hyperlink_to(if cfg!(target_os = "macos") { "Download Mac update" } else { "Open Microsoft Store" }, url);
                                    }
                                    #[cfg(target_os = "windows")]
                                    Some(Ok(Outcome::StoreInstallRequired)) => {
                                        ui.label("This copy wasn't installed from Microsoft Store. Store update checks require the Store edition.");
                                    }
                                    Some(Err(error)) => { ui.label(RichText::new(error).color(dialogs::DANGER)); }
                                    None if self.updates.checking() => { ui.horizontal(|ui| { ui.spinner(); ui.label("Checking for updates…"); }); }
                                    None => { ui.label("Check whenever you're ready."); }
                                    #[cfg(all(test, not(target_os = "windows")))]
                                    Some(Ok(Outcome::StoreInstallRequired)) => { ui.label("Open Microsoft Store to install the Store edition."); }
                                }
                                ui.add_space(6.);
                                changed |= ui.checkbox(&mut self.settings.updates.automatic, "Check automatically once a day").changed();
                                ui.label(RichText::new(if cfg!(target_os = "macos") {
                                    "Checks contact GitHub. Your images and projects stay on your device. Downloads open in your browser."
                                } else { "Checks use Microsoft Store services. Your images and projects stay on your device. Windows manages Store updates." }).size(11.).color(palette.muted));
                                if cfg!(target_os = "macos") {
                                    ui.hyperlink_to("Browse GitHub releases", updates::RELEASES_URL);
                                } else {
                                    ui.hyperlink_to("Open Microsoft Store", updates::STORE_URL);
                                }
                            });
                        if reveal_status {
                            status_card.response.scroll_to_me(Some(egui::Align::Max));
                        }
                    });
                ui.add_space(12.);
                ui.separator();
                ui.add_space(4.);
                // Always reachable, even when release notes need to scroll.
                ui.horizontal_wrapped(|ui| {
                    check = dialogs::button(ui, "Check for Updates", Icon::Refresh,
                        dialogs::Action::Primary, !self.updates.checking()).clicked();
                    close = dialogs::secondary(ui, "Close", Icon::Close).clicked();
                });
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

fn release_section(ui: &mut egui::Ui, title: &str, changes: &[String]) {
    let palette = appearance::palette(ui.ctx());
    ui.label(RichText::new(title).size(10.).strong().color(palette.muted));
    ui.add_space(2.);
    for line in changes {
        ui.horizontal_top(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(18., 20.), egui::Sense::hover());
            ui.painter()
                .circle_filled(rect.center(), 3., palette.accent);
            ui.add(egui::Label::new(RichText::new(line).size(13.).color(palette.ink)).wrap());
        });
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
        for index in 0..13 {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    Pos2::ZERO,
                    [
                        Vec2::new(1200., 800.),
                        Vec2::new(700., 460.),
                        Vec2::new(380., 540.),
                    ][(index / 4).min(2)],
                )),
                ..Default::default()
            };
            if index == 12 {
                input.events.push(egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let output = ctx.run(input, |ctx| app.updates_ui(ctx, &frame));
            if index % 4 == 3 {
                let rect = ctx
                    .memory(|m| m.area_rect(egui::Id::new("myimage-updates")))
                    .unwrap();
                assert!(
                    ctx.content_rect().expand(1.).contains_rect(rect),
                    "update dialog must fit a small workspace: {rect:?}"
                );
                assert!(
                    rect.center().distance(ctx.content_rect().center()) < 2.,
                    "dialog must stay centered: {rect:?}, screen: {:?}",
                    ctx.content_rect()
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
