//! Replace winit's standard About panel with the app's own information dialogs.
//! Kept independently of tray setup so a shortcut/tray failure cannot break About.
use super::{ABOUT_ID, CHECK_UPDATES_ID, DesktopEvent, WHATS_NEW_ID};
use eframe::egui;
use std::sync::{Arc, Mutex, Weak, mpsc::Sender};
use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem as Native, Submenu};

struct Sink {
    ctx: egui::Context,
    sender: Sender<DesktopEvent>,
}
static TARGET: Mutex<Option<Weak<Sink>>> = Mutex::new(None);

pub struct ApplicationMenu {
    menu: Menu,
    information: [MenuItem; 3],
    sink: Arc<Sink>,
}

impl ApplicationMenu {
    pub fn install(ctx: egui::Context, sender: Sender<DesktopEvent>) -> Result<Self, String> {
        let _main_thread = objc2::MainThreadMarker::new()
            .ok_or("The macOS application menu requires the main thread.")?;
        let mut target = TARGET
            .lock()
            .map_err(|_| "Application menu routing is unavailable.")?;
        if target.as_ref().and_then(Weak::upgrade).is_some() {
            return Err("The application menu is already installed.".into());
        }
        let information = [
            MenuItem::with_id(ABOUT_ID, "About MyImage", true, None),
            MenuItem::with_id(WHATS_NEW_ID, "What's New", true, None),
            MenuItem::with_id(CHECK_UPDATES_ID, "Check for Updates…", true, None),
        ];
        let application = Submenu::with_items(
            "MyImage",
            true,
            &[
                &information[0],
                &information[1],
                &information[2],
                &Native::separator(),
                &Native::services(None),
                &Native::separator(),
                &Native::hide(Some("Hide MyImage")),
                &Native::hide_others(None),
                &Native::show_all(None),
                &Native::separator(),
                // Preserve the existing cancellable native termination hook and Cmd+Q.
                &Native::quit(Some("Quit MyImage")),
            ],
        )
        .map_err(|e| e.to_string())?;
        let menu = Menu::with_items(&[&application]).map_err(|e| e.to_string())?;
        super::install_callbacks();
        let sink = Arc::new(Sink { ctx, sender });
        *target = Some(Arc::downgrade(&sink));
        drop(target);
        menu.init_for_nsapp();
        Ok(Self {
            menu,
            information,
            sink,
        })
    }

    pub fn set_information_enabled(&self, enabled: bool) {
        for item in &self.information {
            if item.is_enabled() != enabled {
                item.set_enabled(enabled);
            }
        }
    }
}

pub(super) fn send(event: DesktopEvent) -> bool {
    if !matches!(
        event,
        DesktopEvent::About | DesktopEvent::WhatsNew | DesktopEvent::CheckUpdates
    ) {
        return false;
    }
    let sink = TARGET
        .lock()
        .ok()
        .and_then(|target| target.as_ref().and_then(Weak::upgrade));
    if let Some(sink) = sink
        && sink.sender.send(event).is_ok()
    {
        sink.ctx.request_repaint_of(egui::ViewportId::ROOT);
        return true;
    }
    false
}

impl Drop for ApplicationMenu {
    fn drop(&mut self) {
        if let Ok(mut target) = TARGET.lock()
            && target
                .as_ref()
                .is_some_and(|weak| weak.ptr_eq(&Arc::downgrade(&self.sink)))
        {
            *target = None;
            self.menu.remove_for_nsapp();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn information_events_route_without_a_tray_sink() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let sink = Arc::new(Sink {
            ctx: egui::Context::default(),
            sender,
        });
        *TARGET.lock().unwrap() = Some(Arc::downgrade(&sink));
        for event in [
            DesktopEvent::About,
            DesktopEvent::WhatsNew,
            DesktopEvent::CheckUpdates,
        ] {
            assert!(send(event));
            assert_eq!(receiver.try_recv().unwrap(), event);
        }
        assert!(!send(DesktopEvent::Capture));
        assert!(receiver.try_recv().is_err());
        *TARGET.lock().unwrap() = None;
        assert!(!send(DesktopEvent::About));
    }
}
