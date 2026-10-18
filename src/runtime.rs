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
        window: None,
        occluded: false,
        modifiers: winit::keyboard::ModifiersState::empty(),
    })?;
    Ok(())
}

struct ResidentRuntime<'a> {
    inner: eframe::EframeWinitApplication<'a>,
    hidden: Arc<AtomicBool>,
    window: Option<WindowId>,
    occluded: bool,
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
        self.window = Some(id);
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
        if let WindowEvent::Occluded(value) = &event {
            self.occluded = *value;
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
            && (self.hidden.load(Ordering::Relaxed) || self.occluded)
            && let Some(id) = self.window
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
        if (self.hidden.load(Ordering::Relaxed) || self.occluded)
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
