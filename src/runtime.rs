//! The event loop stays alive when the editor is hidden. No polling timer is used.
use eframe::{NativeOptions, UserEvent, egui};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::WindowId,
};

pub fn run(background: bool) -> eframe::Result {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let hidden = Arc::new(AtomicBool::new(false));
    let app_hidden = hidden.clone();
    let options = NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("MyImage")
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!(
                    "../assets/branding/myimage-app-icon-256.png"
                ))
                .expect("bundled MyImage app icon must be a valid PNG"),
            )
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([980.0, 640.0]),
        ..Default::default()
    };
    let inner = eframe::create_native(
        "MyImage",
        options,
        Box::new(move |cc| {
            Ok(Box::new(crate::app::MyImageApp::new(
                cc, background, app_hidden,
            )))
        }),
        &event_loop,
    );
    event_loop.run_app(&mut ResidentRuntime {
        inner,
        hidden,
        root: RootWindow::default(),
        modifiers: winit::keyboard::ModifiersState::empty(),
    })?;
    Ok(())
}

struct ResidentRuntime<'a> {
    inner: eframe::EframeWinitApplication<'a>,
    hidden: Arc<AtomicBool>,
    root: RootWindow,
    modifiers: winit::keyboard::ModifiersState,
}

impl ApplicationHandler<UserEvent> for ResidentRuntime<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.resumed(event_loop);
    }
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        self.inner.new_events(event_loop, cause);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.root.observe(id, &event);
        if let WindowEvent::ModifiersChanged(modifiers) = &event {
            self.modifiers = modifiers.state();
        }
        if let WindowEvent::KeyboardInput { event, .. } = &event
            && event.state.is_pressed()
            && !event.repeat
            && (if cfg!(target_os = "macos") {
                self.modifiers.super_key() || self.modifiers.control_key()
            } else {
                self.modifiers.control_key()
            })
            && !self.modifiers.alt_key()
            && !self.modifiers.shift_key()
            && matches!(&event.logical_key,winit::keyboard::Key::Character(s) if s.eq_ignore_ascii_case("v"))
        {
            crate::image_clipboard::request_paste();
        }
        self.inner.window_event(event_loop, id, event);
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        let immediate = matches!(&event, UserEvent::RequestRepaint { when, .. } if *when <= std::time::Instant::now());
        self.inner.user_event(event_loop, event);
        // Some window systems suppress redraw notifications for invisible windows.
        // Process an explicit callback wakeup even then, so tray/hotkey/worker events
        // can show the window. This is event-driven and never runs an idle timer.
        if immediate
            && (self.hidden.load(Ordering::Relaxed) || self.root.occluded)
            && let Some(id) = self.root.id
        {
            self.inner
                .window_event(event_loop, id, WindowEvent::RedrawRequested);
        }
    }
    fn device_event(&mut self, event_loop: &ActiveEventLoop, id: DeviceId, event: DeviceEvent) {
        self.inner.device_event(event_loop, id, event);
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.about_to_wait(event_loop);
        if (self.hidden.load(Ordering::Relaxed) || self.root.occluded)
            && matches!(event_loop.control_flow(), ControlFlow::Poll)
        {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.suspended(event_loop);
    }
    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.exiting(event_loop);
    }
    fn memory_warning(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.memory_warning(event_loop);
    }
}

// Eframe creates the root before immediate viewports. Keep its identity even
// when Capture Now is focused, hidden, or destroyed; child repaints cannot run
// the app's desktop-event queue while the root is invisible.
#[derive(Default)]
struct RootWindow {
    id: Option<WindowId>,
    occluded: bool,
}
impl RootWindow {
    fn observe(&mut self, id: WindowId, event: &WindowEvent) {
        self.id.get_or_insert(id);
        if self.id == Some(id)
            && let WindowEvent::Occluded(value) = event
        {
            self.occluded = *value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launcher_events_never_replace_the_hidden_editor_wakeup_target() {
        let editor = WindowId::from(1);
        let launcher = WindowId::from(2);
        let mut root = RootWindow::default();
        root.observe(editor, &WindowEvent::Focused(true));
        root.observe(editor, &WindowEvent::Occluded(true));
        root.observe(launcher, &WindowEvent::Focused(true));
        root.observe(launcher, &WindowEvent::Occluded(false));
        root.observe(launcher, &WindowEvent::Destroyed);
        assert_eq!(root.id, Some(editor));
        assert!(root.occluded);
        root.observe(editor, &WindowEvent::Occluded(false));
        assert!(!root.occluded);
    }
}
