//! Login registration belongs to the OS, not a cached JSON preference. Nothing
//! registers automatically on launch, upgrade, or when opening Settings.
#[cfg(target_os = "macos")]
#[path = "startup/macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "startup/windows.rs"]
mod platform;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Off,
    On,
    ApprovalRequired,
    #[cfg(target_os = "windows")]
    ManagedOn,
    #[cfg(target_os = "windows")]
    ManagedOff,
}

impl Status {
    fn selected(self) -> bool {
        match self {
            Self::On => true,
            #[cfg(target_os = "windows")]
            Self::ManagedOn => true,
            _ => false,
        }
    }
    fn editable(self) -> bool {
        match self {
            #[cfg(target_os = "windows")]
            Self::ManagedOn | Self::ManagedOff => false,
            _ => true,
        }
    }
}

#[derive(Default)]
pub struct Preference {
    status: Option<Result<Status, String>>,
    error: Option<String>,
    was_focused: bool,
}

impl Preference {
    pub fn reset(&mut self) {
        self.status = None;
        self.error = None;
        self.was_focused = false;
    }

    pub fn ui(&mut self, ui: &mut eframe::egui::Ui) {
        let focused = ui.input(|i| i.focused);
        // Refresh on opening and when returning from system settings. Keep the
        // OS query out of ordinary repaints; no timer or manual refresh is needed.
        if self.status.is_none() || (focused && !self.was_focused) {
            self.status = Some(platform::status());
            self.error = None;
        }
        self.was_focused = focused;
        let state = self.status.as_ref().and_then(|s| s.as_ref().ok()).copied();
        let mut enabled = state.is_some_and(Status::selected);
        if ui
            .add_enabled(
                state.is_some_and(Status::editable),
                eframe::egui::Checkbox::new(&mut enabled, "Start MyImage at login"),
            )
            .on_hover_text(
                "Open MyImage automatically when you sign in after restarting your computer. Off by default.",
            )
            .changed()
        {
            self.error = platform::set_enabled(enabled).err();
            self.status = Some(platform::status());
        }
        let state = self.status.as_ref().and_then(|s| s.as_ref().ok()).copied();
        match state {
            Some(Status::ApprovalRequired) => {
                ui.small("Allow MyImage in your system’s login/startup settings.");
            }
            #[cfg(target_os = "windows")]
            Some(Status::ManagedOn | Status::ManagedOff) => {
                ui.small("Your administrator manages this startup setting.");
            }
            _ => {}
        }
        if let Some(error) = self
            .error
            .as_ref()
            .or_else(|| self.status.as_ref().and_then(|s| s.as_ref().err()))
        {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_is_opt_in_and_os_approval_is_not_shown_as_enabled() {
        assert_eq!(Status::default(), Status::Off);
        assert!(!Status::default().selected());
        assert!(!Status::ApprovalRequired.selected());
        assert!(Status::On.selected());
        let preference = Preference::default();
        assert!(preference.status.is_none());
    }

    #[test]
    fn store_manifest_startup_is_disabled_by_default() {
        let xml =
            roxmltree::Document::parse(include_str!("../packaging/windows/msix/AppxManifest.xml"))
                .unwrap();
        let task = xml
            .descendants()
            .find(|n| {
                n.has_tag_name((
                    "http://schemas.microsoft.com/appx/manifest/desktop/windows10",
                    "StartupTask",
                ))
            })
            .unwrap();
        assert_eq!(task.attribute("TaskId"), Some("MyImageStartup"));
        assert_eq!(task.attribute("Enabled"), Some("false"));
    }
}
