//! Desktop entry points. Construct on the UI thread after its event loop starts.

mod shortcut;
#[cfg(target_os = "macos")]
mod termination;
#[cfg(target_os = "macos")]
pub use termination::TerminationGuard;

use std::sync::{
    Arc, Mutex, Once, Weak,
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
};

use eframe::egui;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

const CAPTURE_ID: &str = "myimage.capture";
const PANEL_ID: &str = "myimage.capture-panel";
const SHOW_ID: &str = "myimage.show";
const CAPTURE_SETTINGS_ID: &str = "myimage.capture-settings";
const LIBRARY_ID: &str = "myimage.library";
const QUIT_ID: &str = "myimage.quit";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopEvent {
    Capture,
    CapturePanel,
    Show,
    CaptureSettings,
    Library,
    Quit,
}

struct EventSink {
    ctx: egui::Context,
    sender: Sender<DesktopEvent>,
    hotkeys: Mutex<Vec<Arc<RegisteredHotkey>>>,
}

struct RegisteredHotkey {
    shortcut: shortcut::CaptureHotkey,
    registered: AtomicBool,
    pressed: AtomicBool,
}

impl RegisteredHotkey {
    fn new(shortcut: shortcut::CaptureHotkey) -> Self {
        Self {
            shortcut,
            registered: AtomicBool::new(false),
            pressed: AtomicBool::new(false),
        }
    }

    fn capture_on_event(&self, state: HotKeyState) -> bool {
        self.registered.load(Ordering::Acquire) && capture_on_release(&self.pressed, state)
    }
}

impl EventSink {
    fn send(&self, event: DesktopEvent) {
        if self.sender.send(event).is_ok() {
            // This wakes eframe even when its editor window is hidden.
            self.ctx.request_repaint();
        }
    }
}

// Both libraries install process-wide callbacks only once. Indirection permits
// a later initialization retry without leaving callbacks pointing at a dead UI.
static CALLBACKS: Once = Once::new();
static EVENT_TARGET: Mutex<Option<Weak<EventSink>>> = Mutex::new(None);

fn with_event_sink(action: impl FnOnce(&EventSink)) {
    let sink = EVENT_TARGET
        .lock()
        .ok()
        .and_then(|target| target.as_ref().and_then(Weak::upgrade));
    // Do not hold the routing lock while waking the UI event loop.
    if let Some(sink) = sink {
        action(&sink);
    }
}

fn menu_action(id: &str) -> Option<DesktopEvent> {
    match id {
        CAPTURE_ID => Some(DesktopEvent::Capture),
        PANEL_ID => Some(DesktopEvent::CapturePanel),
        SHOW_ID => Some(DesktopEvent::Show),
        CAPTURE_SETTINGS_ID => Some(DesktopEvent::CaptureSettings),
        LIBRARY_ID => Some(DesktopEvent::Library),
        QUIT_ID => Some(DesktopEvent::Quit),
        _ => None,
    }
}

fn install_callbacks() {
    CALLBACKS.call_once(|| {
        TrayIconEvent::set_event_handler(Some(|event: TrayIconEvent| {
            if matches!(event, TrayIconEvent::Click {
                ref id, button: MouseButton::Left, button_state: MouseButtonState::Up, ..
            } if id.as_ref() == "myimage.tray")
            {
                with_event_sink(|sink| sink.send(DesktopEvent::CapturePanel));
            }
        }));
        MenuEvent::set_event_handler(Some(|event: MenuEvent| {
            let Some(action) = menu_action(event.id.as_ref()) else {
                return;
            };
            with_event_sink(|sink| sink.send(action));
        }));

        GlobalHotKeyEvent::set_event_handler(Some(|event: GlobalHotKeyEvent| {
            with_event_sink(|sink| {
                let hotkey = sink.hotkeys.lock().ok().and_then(|hotkeys| {
                    hotkeys
                        .iter()
                        .find(|hotkey| hotkey.shortcut.hotkey.id() == event.id)
                        .cloned()
                });
                // Wait for release so key-repeat never opens multiple captures
                // and the selection overlay does not inherit the period press.
                if hotkey.is_some_and(|hotkey| hotkey.capture_on_event(event.state)) {
                    sink.send(DesktopEvent::Capture);
                }
            });
        }));
    });
}

fn capture_on_release(pressed: &AtomicBool, state: HotKeyState) -> bool {
    match state {
        HotKeyState::Pressed => {
            pressed.store(true, Ordering::Relaxed);
            false
        }
        HotKeyState::Released => pressed.swap(false, Ordering::Relaxed),
    }
}

/// Owns the native tray/menu-bar icon and shortcut registration.
///
/// A successful construction guarantees that a tray icon was created, even if
/// the shortcut is unavailable. On `Err`, keep the editor visible and do not
/// enable close-to-tray: otherwise the user could lose their way back into it.
pub struct DesktopIntegration {
    _tray: TrayIcon,
    capture_item: MenuItem,
    summary_item: MenuItem,
    menu_summary: String,
    menu_enabled: bool,
    manager: Option<GlobalHotKeyManager>,
    hotkeys: Vec<Arc<RegisteredHotkey>>,
    hotkey_error: Option<String>,
    sink: Arc<EventSink>,
}

impl DesktopIntegration {
    pub fn new(ctx: egui::Context, sender: Sender<DesktopEvent>) -> Result<Self, String> {
        if EVENT_TARGET
            .lock()
            .map_err(|_| "Desktop event routing is unavailable".to_owned())?
            .as_ref()
            .and_then(Weak::upgrade)
            .is_some()
        {
            return Err("Desktop integration is already running".to_owned());
        }

        let menu = Menu::new();
        // A label, rather than a second local accelerator, avoids dispatching
        // both a menu event and a global-hotkey event for the same keypress.
        let capture_label = if cfg!(target_os = "macos") {
            "Capture now    ⌃⇧. / ⌘⇧."
        } else {
            "Capture now    Ctrl+Shift+."
        };
        let capture = MenuItem::with_id(CAPTURE_ID, capture_label, true, None);
        let panel = MenuItem::with_id(PANEL_ID, "Capture panel…", true, None);
        let summary = MenuItem::new("Loading capture setup…", false, None);
        let settings = MenuItem::with_id(CAPTURE_SETTINGS_ID, "Capture settings…", true, None);
        let library = MenuItem::with_id(LIBRARY_ID, "Image library", true, None);
        let show = MenuItem::with_id(SHOW_ID, "Open Editor", true, None);
        let separator = PredefinedMenuItem::separator();
        let quit = MenuItem::with_id(QUIT_ID, "Quit MyImage", true, None);
        menu.append_items(&[
            &panel,
            &capture,
            &summary,
            &settings,
            &separator,
            &library,
            &show,
            &PredefinedMenuItem::separator(),
            &quit,
        ])
        .map_err(|error| format!("Could not create the MyImage tray menu: {error}"))?;

        install_callbacks();
        let tray = TrayIconBuilder::new()
            .with_id("myimage.tray")
            .with_tooltip(if cfg!(target_os = "macos") {
                "MyImage — Capture: Ctrl+Shift+. / Cmd+Shift+."
            } else {
                "MyImage — Capture: Ctrl+Shift+."
            })
            .with_icon(camera_icon()?)
            .with_icon_as_template(cfg!(target_os = "macos"))
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|error| format!("Could not create the MyImage tray icon: {error}"))?;

        let sink = Arc::new(EventSink {
            ctx,
            sender,
            hotkeys: Mutex::new(Vec::new()),
        });
        *EVENT_TARGET
            .lock()
            .map_err(|_| "Desktop event routing is unavailable".to_owned())? =
            Some(Arc::downgrade(&sink));

        let mut integration = Self {
            _tray: tray,
            capture_item: capture,
            summary_item: summary,
            menu_summary: String::new(),
            menu_enabled: true,
            manager: None,
            hotkeys: Vec::new(),
            hotkey_error: None,
            sink,
        };
        // Conflicts and manager errors must not remove the working tray menu.
        let _ = integration.retry_hotkey();
        Ok(integration)
    }

    pub fn update_capture_menu(&mut self, summary: String, enabled: bool) {
        if self.menu_summary != summary {
            self.summary_item.set_text(&summary);
            self.menu_summary = summary;
        }
        if self.menu_enabled != enabled {
            self.capture_item.set_enabled(enabled);
            self.menu_enabled = enabled;
        }
    }

    /// Tray library returns desktop physical pixels on both supported platforms.
    pub fn tray_bounds(&self) -> Option<egui::Rect> {
        self._tray.rect().map(|r| {
            egui::Rect::from_min_size(
                egui::pos2(r.position.x as f32, r.position.y as f32),
                egui::vec2(r.size.width as f32, r.size.height as f32),
            )
        })
    }

    pub fn hotkey_error(&self) -> Option<&str> {
        self.hotkey_error.as_deref()
    }

    pub fn capture_feedback(&self, message: &str) {
        let _ = self._tray.set_tooltip(Some(message));
        #[cfg(target_os = "macos")]
        if objc2::MainThreadMarker::new().is_some() {
            // Merely ordering out the capture window leaves MyImage as the
            // active Mac application. Hide it to return keyboard input to the
            // previous app, so Command+V reaches the intended destination.
            // SAFETY: AppKit is initialized and this is its main thread.
            unsafe {
                let app: *mut objc2::runtime::AnyObject =
                    objc2::msg_send![objc2::class!(NSApplication), sharedApplication];
                let _: () =
                    objc2::msg_send![app, hide: std::ptr::null::<objc2::runtime::AnyObject>()];
            }
        }
    }

    pub fn reveal_application(&self) {
        #[cfg(target_os = "macos")]
        if objc2::MainThreadMarker::new().is_some() {
            // SAFETY: AppKit is initialized and accessed on its main thread.
            // Window activation remains controlled by the caller's Focus command.
            unsafe {
                let app: *mut objc2::runtime::AnyObject =
                    objc2::msg_send![objc2::class!(NSApplication), sharedApplication];
                let _: () = objc2::msg_send![app, unhideWithoutActivation];
            }
        }
    }

    /// Retry unavailable shortcuts without disturbing working registrations.
    /// Must be called on the same UI thread as `new`.
    pub fn retry_hotkey(&mut self) -> Result<(), String> {
        let result = self.register_hotkeys();
        match result {
            Ok(()) => {
                self.hotkey_error = None;
                Ok(())
            }
            Err(error) => {
                let message = format!(
                    "{error}. If another application owns an unavailable shortcut, free it and retry. \
                     Capture in the MyImage menu or editor remains available."
                );
                self.hotkey_error = Some(message.clone());
                Err(message)
            }
        }
    }

    fn register_hotkeys(&mut self) -> Result<(), String> {
        if self.hotkeys.is_empty() {
            let hotkeys: Vec<_> = shortcut::capture_hotkeys()?
                .into_iter()
                .map(|shortcut| Arc::new(RegisteredHotkey::new(shortcut)))
                .collect();
            *self
                .sink
                .hotkeys
                .lock()
                .map_err(|_| "Shortcut event routing is unavailable".to_owned())? = hotkeys.clone();
            self.hotkeys = hotkeys;
        }
        if self.manager.is_none() {
            self.manager = Some(GlobalHotKeyManager::new().map_err(|error| error.to_string())?);
        }
        let manager = self
            .manager
            .as_ref()
            .ok_or("The operating system did not provide a shortcut manager")?;
        register_missing_hotkeys(&self.hotkeys, |hotkey| {
            manager.register(hotkey).map_err(|error| error.to_string())
        })
    }
}

fn register_missing_hotkeys(
    hotkeys: &[Arc<RegisteredHotkey>],
    mut register: impl FnMut(HotKey) -> Result<(), String>,
) -> Result<(), String> {
    let mut errors = Vec::new();
    for hotkey in hotkeys {
        if hotkey.registered.load(Ordering::Acquire) {
            continue;
        }
        match register(hotkey.shortcut.hotkey) {
            Ok(()) => hotkey.registered.store(true, Ordering::Release),
            Err(error) => errors.push(format!(
                "{} could not be registered: {error}",
                hotkey.shortcut.label
            )),
        }
    }
    if errors.is_empty() {
        return Ok(());
    }
    let working: Vec<_> = hotkeys
        .iter()
        .filter(|hotkey| hotkey.registered.load(Ordering::Acquire))
        .map(|hotkey| hotkey.shortcut.label)
        .collect();
    let mut message = errors.join("; ");
    if !working.is_empty() {
        message.push_str(&format!(". Still available: {}", working.join(", ")));
    }
    Err(message)
}

impl Drop for DesktopIntegration {
    fn drop(&mut self) {
        for hotkey in &self.hotkeys {
            if hotkey.registered.swap(false, Ordering::AcqRel)
                && let Some(manager) = self.manager.as_ref()
            {
                let _ = manager.unregister(hotkey.shortcut.hotkey);
            }
        }
        if let Ok(mut target) = EVENT_TARGET.lock()
            && target
                .as_ref()
                .is_some_and(|weak| weak.ptr_eq(&Arc::downgrade(&self.sink)))
        {
            *target = None;
        }
    }
}

fn camera_icon() -> Result<Icon, String> {
    const SIDE: u32 = 36;
    let mut rgba = vec![0; (SIDE * SIDE * 4) as usize];
    for y in 0..SIDE {
        for x in 0..SIDE {
            let body = (5..31).contains(&x) && (11..29).contains(&y);
            let top = (12..24).contains(&x) && (7..11).contains(&y);
            let lens_distance = (x as f32 - 17.5).powi(2) + (y as f32 - 19.5).powi(2);
            let lens_hole = (16.0..42.0).contains(&lens_distance);
            if (body || top) && !lens_hole {
                let offset = ((y * SIDE + x) * 4) as usize;
                // macOS tints this template's alpha; a blue camera remains
                // distinguishable on both light and dark Windows taskbars.
                let color = if cfg!(target_os = "macos") {
                    [0, 0, 0, 255]
                } else {
                    [69, 154, 244, 255]
                };
                rgba[offset..offset + 4].copy_from_slice(&color);
            }
        }
    }
    Icon::from_rgba(rgba, SIDE, SIDE)
        .map_err(|error| format!("Could not create the MyImage tray image: {error}"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn menu_routes_quick_capture_and_workspace_links_separately() {
        use super::*;
        assert_eq!(menu_action(CAPTURE_ID), Some(DesktopEvent::Capture));
        assert_eq!(menu_action(PANEL_ID), Some(DesktopEvent::CapturePanel));
        assert_eq!(
            menu_action(CAPTURE_SETTINGS_ID),
            Some(DesktopEvent::CaptureSettings)
        );
        assert_eq!(menu_action(LIBRARY_ID), Some(DesktopEvent::Library));
        assert_eq!(menu_action(SHOW_ID), Some(DesktopEvent::Show));
        assert_eq!(menu_action(QUIT_ID), Some(DesktopEvent::Quit));
        assert_eq!(menu_action("unknown"), None);
    }

    use super::*;
    use global_hotkey::hotkey::{Code, Modifiers};

    fn two_shortcuts() -> Vec<Arc<RegisteredHotkey>> {
        [
            ("Ctrl+Shift+.", Modifiers::CONTROL),
            ("Cmd+Shift+.", Modifiers::SUPER),
        ]
        .into_iter()
        .map(|(label, modifier)| {
            Arc::new(RegisteredHotkey::new(shortcut::CaptureHotkey {
                label,
                hotkey: HotKey::new(Some(modifier | Modifiers::SHIFT), Code::Period),
            }))
        })
        .collect()
    }

    #[test]
    fn held_shortcut_captures_once_on_release() {
        let pressed = AtomicBool::new(false);
        assert!(!capture_on_release(&pressed, HotKeyState::Released));
        for _ in 0..8 {
            assert!(!capture_on_release(&pressed, HotKeyState::Pressed));
        }
        assert!(capture_on_release(&pressed, HotKeyState::Released));
        assert!(!capture_on_release(&pressed, HotKeyState::Released));
        assert!(!capture_on_release(&pressed, HotKeyState::Pressed));
        assert!(capture_on_release(&pressed, HotKeyState::Released));
    }

    #[test]
    fn both_shortcuts_keep_independent_press_state() {
        let hotkeys = two_shortcuts();
        register_missing_hotkeys(&hotkeys, |_| Ok(())).unwrap();
        assert_ne!(
            hotkeys[0].shortcut.hotkey.id(),
            hotkeys[1].shortcut.hotkey.id()
        );
        for hotkey in &hotkeys {
            assert!(!hotkey.capture_on_event(HotKeyState::Released));
            for _ in 0..8 {
                assert!(!hotkey.capture_on_event(HotKeyState::Pressed));
            }
        }
        assert!(hotkeys[0].capture_on_event(HotKeyState::Released));
        assert!(!hotkeys[0].capture_on_event(HotKeyState::Released));
        assert!(hotkeys[1].capture_on_event(HotKeyState::Released));
        assert!(!hotkeys[1].capture_on_event(HotKeyState::Released));
    }

    #[test]
    fn partial_registration_keeps_working_shortcut_and_retries_only_failure() {
        for failed_index in 0..2 {
            let hotkeys = two_shortcuts();
            let failed = &hotkeys[failed_index];
            let working = &hotkeys[1 - failed_index];
            let error = register_missing_hotkeys(&hotkeys, |hotkey| {
                if hotkey.id() == failed.shortcut.hotkey.id() {
                    Err("already registered by another application".to_owned())
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
            assert!(error.contains(&format!(
                "{} could not be registered",
                failed.shortcut.label
            )));
            assert!(error.contains(&format!("Still available: {}", working.shortcut.label)));
            assert!(!failed.capture_on_event(HotKeyState::Pressed));
            assert!(!failed.capture_on_event(HotKeyState::Released));
            assert!(!working.capture_on_event(HotKeyState::Pressed));

            let mut retried = Vec::new();
            register_missing_hotkeys(&hotkeys, |hotkey| {
                retried.push(hotkey.id());
                Ok(())
            })
            .unwrap();
            assert_eq!(retried, vec![failed.shortcut.hotkey.id()]);
            // Retrying the failed chord must not reset a held working chord.
            assert!(working.capture_on_event(HotKeyState::Released));
            assert!(!working.capture_on_event(HotKeyState::Released));
            assert!(!failed.capture_on_event(HotKeyState::Released));
            assert!(!failed.capture_on_event(HotKeyState::Pressed));
            assert!(failed.capture_on_event(HotKeyState::Released));
            register_missing_hotkeys(&hotkeys, |_| {
                panic!("registered shortcuts must not be registered again")
            })
            .unwrap();
        }
    }
}
